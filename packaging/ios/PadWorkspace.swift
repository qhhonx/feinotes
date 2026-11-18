import UIKit

/// Home and document columns around the existing, single Rust workspace.
@MainActor
final class FeiPadWorkspaceController: UISplitViewController, UISplitViewControllerDelegate {
    private let editor: EditorHost
    private let browser = FeiRouteController()
    private let browserNavigation: UINavigationController
    private let editorNavigation: UINavigationController
    private var state: FeiNavigationState?
    private var focused = false
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
    private lazy var settingsItem: UIBarButtonItem = {
        let item = UIBarButtonItem(image: UIImage(systemName: "gearshape"),primaryAction: UIAction { [weak self] _ in self?.editor.navigationCoordinator?.send("settings") })
        item.accessibilityIdentifier = "fei.settings"; return item
    }()
    private var lastFit: DisplayMode?
    private var metricsScale: CGFloat = 0
    private var lastBrowserScope: String?
    private let empty = UILabel()

    init(host: EditorHost) {
        editor = host
        browserNavigation = UINavigationController(rootViewController: browser)
        editorNavigation = UINavigationController(rootViewController: host)
        super.init(style: .doubleColumn)
        host.padWorkspace = self
        browser.isWorkspaceBrowser = true
        delegate = self
        primaryEdge = .leading
        preferredSplitBehavior = .tile
        showsSecondaryOnlyButton = false
        displayModeButtonVisibility = .never
        browserNavigation.navigationBar.prefersLargeTitles = true
        presentsWithGesture = true
        setViewController(browserNavigation,for: .primary)
        setViewController(editorNavigation,for: .secondary)
        NotificationCenter.default.addObserver(self,selector: #selector(contentSizeChanged),name: UIContentSizeCategory.didChangeNotification,object: nil)
    }
    required init?(coder: NSCoder) { fatalError("init(coder:) is unsupported") }

    override func viewDidLoad() {
        super.viewDidLoad()
        view.accessibilityIdentifier = "fei.ipad.workspace"
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
        NSLayoutConstraint.activate([empty.centerXAnchor.constraint(equalTo: editor.view.safeAreaLayoutGuide.centerXAnchor),empty.centerYAnchor.constraint(equalTo: editor.view.safeAreaLayoutGuide.centerYAnchor),empty.leadingAnchor.constraint(greaterThanOrEqualTo: editor.view.safeAreaLayoutGuide.leadingAnchor,constant: 24),empty.trailingAnchor.constraint(lessThanOrEqualTo: editor.view.safeAreaLayoutGuide.trailingAnchor,constant: -24)])
        updateFit()
    }

    override func viewDidAppear(_ animated: Bool) {
        super.viewDidAppear(animated)
        if showsEmptyDetail && view.bounds.height > view.bounds.width { show(.primary) }
    }

    var showsEmptyDetail: Bool { state.map { ["notes","folder"].contains($0.kind) } ?? true }
    var usesTopEditorTools: Bool { !isCollapsed }
    var allowsCloudResume: Bool {
        browser.notesList?.allowsCloudResume != false && state?.browser?.shared != true && state?.browser?.favorites != true && state?.browser?.folder == nil
            && [browserNavigation,editorNavigation].allSatisfy { $0.transitionCoordinator == nil && $0.topViewController?.presentedViewController == nil }
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
        for navigation in [browserNavigation,editorNavigation] {
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
        settingsItem.accessibilityLabel = next.labels["settings"]; settingsItem.isEnabled = !next.busy
        var leading = [settingsItem]
        let browserScope = next.browser?.listing.scope
        if let folder = next.browser?.folder {
            let parent = next.items.first(where: { $0.id == folder })?.parent
            let back = UIBarButtonItem(image: UIImage(systemName: "chevron.left"),primaryAction: UIAction { [weak coordinator] _ in coordinator?.send("browse-scope",value: parent.map { "folder:"+$0 } ?? (next.browser?.shared == true ? "shared" : "all"),scope: next.browser?.listing.scope) })
            back.accessibilityIdentifier = "fei.browser.parent"; back.accessibilityLabel = next.labels["back"]; leading = [back]
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
        let detailChanged = next.routes.last?.key != previous?.routes.last?.key
        if !isCollapsed,view.bounds.height > view.bounds.width,!showsEmptyDetail,detailChanged { hide(.primary); show(.secondary) }
        if isCollapsed {
            if !showsEmptyDetail && detailChanged { show(.secondary) }
            else if showsEmptyDetail && next.browser?.listing.scope != lastBrowserScope { show(.primary) }
        } else if ["notes","folder"].contains(next.kind) && (previous?.kind == "settings" || view.bounds.height > view.bounds.width && previous?.kind != next.kind) { show(.primary) }
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
        focused.toggle(); lastFit = nil
        updateFit(animated: true)
        editor.navigationCoordinator?.refreshPadButtons()
    }
    func beginEditorInput() {
        if !isCollapsed,displayMode == .oneOverSecondary { hide(.primary) }
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
        let listMinimum = 300*scale, detailMinimum = 400*min(scale,1.2)
        if metricsScale != scale {
        metricsScale = scale
        minimumPrimaryColumnWidth = listMinimum
        maximumPrimaryColumnWidth = max(440,listMinimum)
        preferredPrimaryColumnWidth = max(400,listMinimum)
        if #available(iOS 26.0,*) { minimumSecondaryColumnWidth = detailMinimum }
        }
        let width = view.bounds.inset(by: view.safeAreaInsets).width
        let overlay = view.bounds.height > view.bounds.width || width < listMinimum+detailMinimum+1
        let mode: DisplayMode = focused ? .secondaryOnly : overlay ? .oneOverSecondary : .oneBesideSecondary
        if lastFit != mode {
            lastFit = mode
            let change = { self.preferredSplitBehavior = overlay ? .overlay : .tile; self.preferredDisplayMode = mode; self.view.layoutIfNeeded() }
            if animated && !UIAccessibility.isReduceMotionEnabled { UIView.animate(withDuration: 0.3,delay: 0,options: [.beginFromCurrentState,.allowUserInteraction,.curveEaseInOut],animations: change) }
            else { change() }
            if overlay && !focused && showsEmptyDetail { show(.primary) }
        }
    }
    func splitViewController(_ svc: UISplitViewController,topColumnForCollapsingToProposedTopColumn proposedTopColumn: Column) -> Column {
        state.map { ["note","history","history-preview","settings","trash"].contains($0.kind) } == true ? .secondary : .primary
    }
    func splitViewControllerDidCollapse(_ svc: UISplitViewController) { refreshColumns() }
    func splitViewControllerDidExpand(_ svc: UISplitViewController) { refreshColumns() }
    private func refreshColumns() {
        browser.notesList?.configureSearch(in: browser,expanded: !isCollapsed)
        editor.refreshEditorLayout()
        editor.navigationCoordinator?.refreshPadButtons()
    }
}
