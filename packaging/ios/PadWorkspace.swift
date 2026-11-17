import UIKit

/// Three native presentation roles around the existing, single Rust workspace.
@MainActor
final class FeiPadWorkspaceController: UISplitViewController, UISplitViewControllerDelegate {
    private let editor: EditorHost
    private let sidebar = FeiPadSidebarController()
    private let browser = FeiRouteController()
    private let sidebarNavigation: UINavigationController
    private let browserNavigation: UINavigationController
    private let editorNavigation: UINavigationController
    private var state: FeiNavigationState?
    private var focused = false
    private var sidebarSuppressed = false
    private var previousTheme: FeiMobileTheme?
    private var previousAppearance = ""
    private var previousFocusIcon: Bool?
    private var browserButtonsScope: String?
    private var browserMore: UIBarButtonItem?
    private lazy var focusItem: UIBarButtonItem = {
        let item = UIBarButtonItem(image: nil,primaryAction: UIAction { [weak self] _ in self?.toggleFocus() })
        item.accessibilityIdentifier = "fei.ipad.focus"; return item
    }()
    private lazy var composeItem: UIBarButtonItem = {
        let item = UIBarButtonItem(systemItem: .compose,primaryAction: UIAction { [weak self] _ in self?.editor.navigationCoordinator?.composeNote() })
        item.accessibilityIdentifier = "fei.new"; return item
    }()
    private lazy var sidebarItem: UIBarButtonItem = {
        let item = UIBarButtonItem(image: UIImage(systemName: "sidebar.left"),primaryAction: UIAction { [weak self] _ in
            guard let self else { return }
            if self.view.bounds.height > self.view.bounds.width { self.show(.primary) }
            else { self.sidebarSuppressed.toggle(); self.lastFit = nil; self.updateFit(animated: true) }
        })
        item.accessibilityIdentifier = "fei.sidebar.toggle"; return item
    }()
    private var lastFit: DisplayMode?
    private var metricsScale: CGFloat = 0
    private var lastBrowserScope: String?
    private let empty = UILabel()

    init(host: EditorHost) {
        editor = host
        sidebarNavigation = UINavigationController(rootViewController: sidebar)
        browserNavigation = UINavigationController(rootViewController: browser)
        editorNavigation = UINavigationController(rootViewController: host)
        super.init(style: .tripleColumn)
        host.padWorkspace = self
        browser.isWorkspaceBrowser = true
        delegate = self
        primaryEdge = .leading
        preferredSplitBehavior = .tile
        showsSecondaryOnlyButton = false
        displayModeButtonVisibility = .never
        sidebarNavigation.navigationBar.prefersLargeTitles = true
        browserNavigation.navigationBar.prefersLargeTitles = true
        presentsWithGesture = true
        setViewController(sidebarNavigation,for: .primary)
        setViewController(browserNavigation,for: .supplementary)
        setViewController(editorNavigation,for: .secondary)
        NotificationCenter.default.addObserver(self,selector: #selector(contentSizeChanged),name: UIContentSizeCategory.didChangeNotification,object: nil)
    }
    required init?(coder: NSCoder) { fatalError("init(coder:) is unsupported") }

    override func viewDidLoad() {
        super.viewDidLoad()
        view.accessibilityIdentifier = "fei.ipad.workspace"
        sidebar.view.accessibilityIdentifier = "fei.ipad.sidebar"
        browser.view.accessibilityIdentifier = "fei.ipad.browser"
        editor.view.accessibilityIdentifier = "fei.ipad.detail"
        empty.font = .preferredFont(forTextStyle: .title2)
        empty.adjustsFontForContentSizeCategory = true
        empty.textColor = .secondaryLabel
        empty.textAlignment = .center
        empty.numberOfLines = 0
        empty.accessibilityIdentifier = "fei.ipad.empty-detail"
        empty.translatesAutoresizingMaskIntoConstraints = false
        empty.isHidden = true
        editor.view.addSubview(empty)
        NSLayoutConstraint.activate([empty.centerXAnchor.constraint(equalTo: editor.view.centerXAnchor),empty.centerYAnchor.constraint(equalTo: editor.view.centerYAnchor),empty.leadingAnchor.constraint(greaterThanOrEqualTo: editor.view.safeAreaLayoutGuide.leadingAnchor,constant: 24),empty.trailingAnchor.constraint(lessThanOrEqualTo: editor.view.safeAreaLayoutGuide.trailingAnchor,constant: -24)])
        updateFit()
    }

    override func viewDidAppear(_ animated: Bool) {
        super.viewDidAppear(animated)
        if showsEmptyDetail && view.bounds.height > view.bounds.width { show(.supplementary) }
    }

    var showsEmptyDetail: Bool { state.map { ["notes","folder"].contains($0.kind) } ?? true }
    var usesTopEditorTools: Bool { !isCollapsed }
    var allowsCloudResume: Bool {
        browser.notesList?.allowsCloudResume != false && state?.browser?.shared != true && state?.browser?.favorites != true && state?.browser?.folder == nil
            && [sidebarNavigation,browserNavigation,editorNavigation].allSatisfy { $0.transitionCoordinator == nil && $0.topViewController?.presentedViewController == nil }
    }
    func beginBrowserSelection() { browser.notesList?.beginSelection() }
    func beginBrowserReorder() { browser.notesList?.beginReordering() }
    var browserTitle: String { state?.browser?.title ?? "" }
    var browserIsExpanded: Bool { !isCollapsed }

    func detailRoutes(_ next: FeiNavigationState) -> [FeiNavigationState.Route] {
        let routes = next.routes.filter { $0.key != "notes" && $0.key != "shared" && !$0.key.hasPrefix("folder:") }
        return routes.isEmpty ? [.init(key: "detail-empty",title: "")] : routes
    }

    func update(_ next: FeiNavigationState,coordinator: FeiNavigationCoordinator) {
        let previous = state
        state = next
        loadViewIfNeeded()
        let theme = next.theme ?? .system
        let appearance: UIUserInterfaceStyle = next.appearance == "dark" ? .dark : next.appearance == "light" ? .light : .unspecified
        overrideUserInterfaceStyle = appearance
        view.tintColor = theme.accentColor
        if previousTheme != next.theme || previousAppearance != next.appearance {
        for navigation in [sidebarNavigation,browserNavigation,editorNavigation] {
            navigation.view.tintColor = theme.accentColor
            navigation.navigationBar.tintColor = theme.accentColor
            let bar = UINavigationBarAppearance(); bar.configureWithDefaultBackground()
            if let color = theme.chromeColor { bar.backgroundColor = color }
            navigation.navigationBar.standardAppearance = bar
            navigation.navigationBar.scrollEdgeAppearance = bar
            navigation.navigationBar.compactAppearance = bar
        }
        previousTheme = next.theme; previousAppearance = next.appearance
        }
        sidebar.update(next,coordinator: coordinator)
        browser.title = next.browser?.title ?? next.labels["sharing"]
        browser.navigationItem.title = browser.title
        browser.navigationItem.largeTitleDisplayMode = .always
        browser.routeKey = "ipad-browser"
        browser.updateListing(next.browser?.listing,labels: next.labels,busy: next.busy,coordinator: coordinator)
        if let list = browser.notesList {
            list.browsesWorkspace = true
            list.selectNote(["note","history","history-preview"].contains(next.kind) ? next.current : nil)
            list.applyTheme(theme)
            list.configureSearch(in: browser,expanded: !isCollapsed)
        }
        sidebarItem.accessibilityLabel = next.labels["folders"]
        var leading = [sidebarItem]
        let browserScope = next.browser?.listing.scope
        if let folder = next.browser?.folder {
            let parent = next.items.first(where: { $0.id == folder })?.parent
            let back = UIBarButtonItem(image: UIImage(systemName: "chevron.left"),primaryAction: UIAction { [weak coordinator] _ in coordinator?.send("browse-scope",value: parent.map { "folder:"+$0 } ?? "all",scope: next.browser?.listing.scope) })
            back.accessibilityIdentifier = "fei.browser.parent"; back.accessibilityLabel = next.labels["back"]; leading.append(back)
        }
        if browser.notesList?.isSelecting != true && (browserButtonsScope != browserScope || browser.navigationItem.leftBarButtonItems?.map({ $0.accessibilityIdentifier }) != leading.map({ $0.accessibilityIdentifier })) { browser.navigationItem.leftBarButtonItems = leading }
        browserButtonsScope = browserScope
        let close = (browser.navigationItem.rightBarButtonItems ?? []).filter { $0.accessibilityIdentifier == "fei.notes.search-close" }
        if browserMore == nil { browserMore = coordinator.browserActionsItem() }
        let trailing = close+[browserMore!]
        if browser.notesList?.isSelecting != true && browser.notesList?.tableView.isEditing != true && browser.navigationItem.rightBarButtonItems?.map(ObjectIdentifier.init) != trailing.map(ObjectIdentifier.init) { browser.navigationItem.rightBarButtonItems = trailing }
        browser.notesList?.configureSearch(in: browser,expanded: !isCollapsed)
        empty.text = next.labels["select-note"]
        empty.isHidden = !showsEmptyDetail
        if !empty.isHidden { editor.view.bringSubviewToFront(empty) }
        updateFit()
        updateReadingGuard()
        let detailChanged = next.routes.last?.key != previous?.routes.last?.key
        if !isCollapsed,view.bounds.height > view.bounds.width,!showsEmptyDetail,detailChanged { hide(.primary); hide(.supplementary); show(.secondary) }
        if isCollapsed {
            if !showsEmptyDetail && detailChanged { show(.secondary) }
            else if showsEmptyDetail && next.browser?.listing.scope != lastBrowserScope { show(.supplementary) }
        } else if ["notes","folder"].contains(next.kind) && (previous?.kind == "settings" || view.bounds.height > view.bounds.width && previous?.kind != next.kind) { show(.supplementary) }
        lastBrowserScope = next.browser?.listing.scope
        editor.refreshEditorLayout()
    }

    func configureDetailButtons(_ controller: UIViewController,state: FeiNavigationState,coordinator: FeiNavigationCoordinator) {
        guard ["note","notes","folder"].contains(state.kind) else { return }
        if ["notes","folder"].contains(state.kind) { controller.navigationItem.rightBarButtonItems = [] }
        if previousFocusIcon != focused {
            focusItem.image = UIImage(systemName: focused ? "arrow.down.right.and.arrow.up.left" : "arrow.up.left.and.arrow.down.right")
            previousFocusIcon = focused
        }
        focusItem.accessibilityLabel = state.labels[focused ? "show-browser" : "focus-note"]
        controller.navigationItem.leftItemsSupplementBackButton=controller.navigationController?.viewControllers.count ?? 0 > 1
        if controller.navigationItem.leftBarButtonItems?.first !== focusItem { controller.navigationItem.leftBarButtonItems = [focusItem] }
        if state.can_create {
            composeItem.tintColor=(state.theme ?? .system).accentColor
            composeItem.accessibilityLabel = state.labels["new"]; composeItem.isEnabled = coordinator.allowsCompose
            var items = (controller.navigationItem.rightBarButtonItems ?? []).filter { $0.accessibilityIdentifier != "fei.new" }
            let position = items.first?.accessibilityIdentifier == "fei.edit.done" ? 1 : 0
            items.insert(composeItem,at: position)
            if controller.navigationItem.rightBarButtonItems?.map(ObjectIdentifier.init) != items.map(ObjectIdentifier.init) { controller.navigationItem.rightBarButtonItems = items }
        }
    }

    private func toggleFocus() {
        editor.settleEditorComposition()
        focused.toggle(); sidebarSuppressed = focused; lastFit = nil
        updateFit(animated: true)
        updateReadingGuard()
        editor.navigationCoordinator?.refreshPadButtons()
    }
    private func updateReadingGuard(mode: DisplayMode? = nil) {
        let guarded=state?.kind == "note" && !focused && !sidebarSuppressed && (mode ?? displayMode) == .twoBesideSecondary
        editor.setBrowserTapGuard(guarded ? { [weak self] in self?.beginEditorInput() } : nil)
    }
    func splitViewController(_ svc: UISplitViewController,willChangeTo displayMode: DisplayMode) { updateReadingGuard(mode: displayMode) }
    func beginEditorInput() {
        guard !isCollapsed,!focused,!sidebarSuppressed else { return }
        sidebarSuppressed = true; lastFit = nil; editor.setBrowserTapGuard(nil)
        updateFit(animated: true)
    }

    override func viewDidLayoutSubviews() {
        super.viewDidLayoutSubviews()
        updateFit()
        browser.notesList?.configureSearch(in: browser,expanded: !isCollapsed)
    }
    @objc private func contentSizeChanged() { lastFit = nil; updateFit() }
    private func updateFit(animated: Bool = false) {
        guard isViewLoaded,view.bounds.width > 0 else { return }
        // Product tuning, not measurements of Apple's column widths.
        let scale = min(1.45,max(1,UIFont.preferredFont(forTextStyle: .body).pointSize/17))
        let sidebarMinimum = 200*scale, listMinimum = 250*scale, detailMinimum = 400*min(scale,1.2)
        if metricsScale != scale {
        metricsScale = scale
        minimumPrimaryColumnWidth = sidebarMinimum
        maximumPrimaryColumnWidth = max(300,sidebarMinimum)
        preferredPrimaryColumnWidth = max(224,sidebarMinimum)
        minimumSupplementaryColumnWidth = listMinimum
        maximumSupplementaryColumnWidth = max(360,listMinimum)
        preferredSupplementaryColumnWidth = max(280,listMinimum)
        if #available(iOS 26.0,*) { minimumSecondaryColumnWidth = detailMinimum }
        }
        let width = view.bounds.inset(by: view.safeAreaInsets).width
        let portrait = view.bounds.height > view.bounds.width
        let mode: DisplayMode = focused ? .secondaryOnly : portrait ? .oneOverSecondary : !sidebarSuppressed && width >= sidebarMinimum+listMinimum+detailMinimum+2 ? .twoBesideSecondary : width >= listMinimum+detailMinimum+1 ? .oneBesideSecondary : .secondaryOnly
        if lastFit != mode {
            lastFit = mode
            let change = { self.preferredSplitBehavior = portrait ? .overlay : .tile; self.preferredDisplayMode = mode; self.view.layoutIfNeeded() }
            if animated && !UIAccessibility.isReduceMotionEnabled { UIView.animate(withDuration: 0.3,delay: 0,options: [.beginFromCurrentState,.allowUserInteraction,.curveEaseInOut],animations: change) }
            else { change() }
            if portrait && showsEmptyDetail { show(.supplementary) }
        }
    }
    func splitViewController(_ svc: UISplitViewController,topColumnForCollapsingToProposedTopColumn proposedTopColumn: Column) -> Column {
        state.map { ["note","history","history-preview","settings","trash"].contains($0.kind) } == true ? .secondary : .supplementary
    }
    func splitViewControllerDidCollapse(_ svc: UISplitViewController) { refreshColumns() }
    func splitViewControllerDidExpand(_ svc: UISplitViewController) { refreshColumns() }
    private func refreshColumns() {
        updateReadingGuard()
        browser.notesList?.configureSearch(in: browser,expanded: !isCollapsed)
        editor.refreshEditorLayout()
        editor.navigationCoordinator?.refreshPadButtons()
    }
}

@MainActor
private final class FeiPadSidebarController: UIViewController,UICollectionViewDelegate {
    private var collection: UICollectionView!
    private var source: UICollectionViewDiffableDataSource<Int,String>!
    private var state: FeiNavigationState?
    private weak var coordinator: FeiNavigationCoordinator?
    private var items: [String:FeiNavigationState.Item] = [:]
    private var lastItems: [FeiNavigationState.Item] = []
    private var lastSession = ""
    private lazy var settingsItem = UIBarButtonItem(image: UIImage(systemName: "gearshape"),primaryAction: UIAction { [weak self] _ in self?.coordinator?.send("settings") })
    private lazy var folderItem = UIBarButtonItem(image: UIImage(systemName: "folder.badge.plus"),primaryAction: UIAction { [weak self] _ in self?.coordinator?.createSidebarFolder() })

    override func viewDidLoad() {
        super.viewDidLoad()
        var configuration = UICollectionLayoutListConfiguration(appearance: .sidebar)
        configuration.headerMode = .none
        collection = UICollectionView(frame: .zero,collectionViewLayout: UICollectionViewCompositionalLayout.list(using: configuration))
        collection.autoresizingMask = [.flexibleWidth,.flexibleHeight]; collection.frame = view.bounds
        collection.accessibilityIdentifier = "fei.sidebar.list"
        collection.delegate = self
        let registration = UICollectionView.CellRegistration<UICollectionViewListCell,String> { [weak self] cell,_,key in
            guard let self,let state = self.state else { return }
            let theme = state.theme ?? .system
            let item = self.items[key]
            var content = cell.defaultContentConfiguration()
            let label: String
            let symbol: String
            if let item { label = item.title; symbol = "folder" }
            else {
                label = state.labels[key] ?? key
                symbol = ["all-notes":"note.text","favorites":"star","sharing":"person.2","trash":"trash"][key] ?? "folder"
            }
            content.text = label
            content.textProperties.font = .preferredFont(forTextStyle: .body)
            content.textProperties.numberOfLines = 2
            content.textProperties.color = theme.textColor
            content.image = key == "sharing" ? feiSharedDocumentsIcon() : UIImage(systemName: symbol); content.imageProperties.tintColor = theme.folderColor
            cell.contentConfiguration = content
            cell.accessibilityIdentifier = item.map { "fei.sidebar.folder."+$0.id } ?? "fei.sidebar."+key
            let children = self.items.values.contains { $0.parent == key }
            cell.accessories = children ? [.outlineDisclosure()] : []
            cell.configurationUpdateHandler = { [weak self] cell,state in
                let theme=self?.state?.theme ?? .system
                var background = UIBackgroundConfiguration.listSidebarCell()
                if state.isSelected { background.backgroundColor = theme.selectedColor }
                cell.backgroundConfiguration = background
                if var content=cell.contentConfiguration as? UIListContentConfiguration {
                    content.textProperties.color=state.isSelected ? theme.selectedTextColor : theme.textColor
                    content.imageProperties.tintColor=state.isSelected ? theme.selectedTextColor : theme.folderColor
                    cell.contentConfiguration=content
                }
            }
        }
        source = UICollectionViewDiffableDataSource<Int,String>(collectionView: collection) { collection,index,key in collection.dequeueConfiguredReusableCell(using: registration,for: index,item: key) }
        view.addSubview(collection)
        setContentScrollView(collection,for: .all)
    }

    func update(_ next: FeiNavigationState,coordinator: FeiNavigationCoordinator) {
        loadViewIfNeeded()
        let previous = state; state = next; self.coordinator = coordinator
        let theme = next.theme ?? .system
        collection.tintColor = theme.accentColor; collection.backgroundColor = theme.listColor
        title = next.labels["folders"]
        navigationItem.largeTitleDisplayMode = .always
        settingsItem.accessibilityIdentifier = "fei.settings"; settingsItem.accessibilityLabel = next.labels["settings"]; settingsItem.isEnabled = !next.busy
        folderItem.accessibilityIdentifier = "fei.sidebar.new-folder"; folderItem.accessibilityLabel = next.labels["new-folder"]; folderItem.isEnabled = !next.busy && next.can_create
        navigationItem.leftBarButtonItem = nil; navigationItem.rightBarButtonItem = nil
        if toolbarItems?.first !== settingsItem { toolbarItems = [settingsItem,.flexibleSpace(),folderItem] }
        navigationController?.setToolbarHidden(false,animated: false)
        collection.isUserInteractionEnabled = !next.busy
        let folders = next.items.filter { $0.folder && !$0.trashed }
        let session = next.browser?.listing.scope.components(separatedBy: "|").first ?? next.editor_scope
        if folders != lastItems || session != lastSession || previous?.labels != next.labels || previous?.theme != next.theme {
            let old = source.snapshot().sectionIdentifiers.contains(1) ? source.snapshot(for: 1) : NSDiffableDataSourceSectionSnapshot<String>()
            items = Dictionary(uniqueKeysWithValues: folders.map { ($0.id,$0) })
            var snapshot = NSDiffableDataSourceSnapshot<Int,String>(); snapshot.appendSections([0,1])
            snapshot.appendItems(["all-notes","favorites","sharing","trash"],toSection: 0)
            source.apply(snapshot,animatingDifferences: false)
            var hierarchy = NSDiffableDataSourceSectionSnapshot<String>()
            var visited = Set<String>()
            func append(_ parent: String?) {
                for item in folders.filter({ $0.parent == parent || parent == nil && $0.parent != nil && items[$0.parent!] == nil }).sorted(by: { $0.title.localizedStandardCompare($1.title) == .orderedAscending }) {
                    guard visited.insert(item.id).inserted else { continue }
                    hierarchy.append([item.id],to: parent)
                    append(item.id)
                }
            }
            append(nil)
            if session == lastSession { hierarchy.expand(old.items.filter { old.isExpanded($0) && visited.contains($0) }) }
            var parent = next.browser?.folder
            var ancestors = Set<String>()
            while let key = parent,visited.contains(key),ancestors.insert(key).inserted { hierarchy.expand([key]); parent = items[key]?.parent }
            source.apply(hierarchy,to: 1,animatingDifferences: false)
            var refreshed = source.snapshot()
            refreshed.reconfigureItems(refreshed.itemIdentifiers)
            source.apply(refreshed,animatingDifferences: false)
            lastItems = folders; lastSession = session
        }
        let selected = next.browser?.folder ?? (next.browser?.shared == true ? "sharing" : next.browser?.favorites == true ? "favorites" : "all-notes")
        for path in collection.indexPathsForSelectedItems ?? [] { collection.deselectItem(at: path,animated: false) }
        if let path = source.indexPath(for: selected) { collection.selectItem(at: path,animated: false,scrollPosition: []) }
    }

    func collectionView(_ collectionView: UICollectionView,didSelectItemAt indexPath: IndexPath) {
        guard let key = source.itemIdentifier(for: indexPath),let state else { return }
        view.endEditing(true)
        if key == "sharing" { coordinator?.send("shared-list"); return }
        if key == "trash" { coordinator?.send("trash"); return }
        let value = items[key] == nil ? key == "favorites" ? "favorites" : "all" : "folder:"+key
        coordinator?.send("browse-scope",value: value,scope: state.browser?.listing.scope)
        splitViewController?.show(.supplementary)
    }
    func collectionView(_ collectionView: UICollectionView,contextMenuConfigurationForItemAt indexPath: IndexPath,point: CGPoint) -> UIContextMenuConfiguration? {
        guard let key = source.itemIdentifier(for: indexPath),items[key] != nil else { return nil }
        return UIContextMenuConfiguration(identifier: key as NSString,previewProvider: nil) { [weak self] _ in self?.coordinator?.itemMenu(key,browsingWorkspace: true) }
    }
}
