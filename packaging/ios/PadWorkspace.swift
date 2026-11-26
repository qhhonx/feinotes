import UIKit

/// Home and document columns around the existing, single Rust workspace.
@MainActor
final class FeiPadWorkspaceController: UISplitViewController, UISplitViewControllerDelegate, UIGestureRecognizerDelegate {
    private let editor: EditorHost
    private let browserRoot: FeiRouteController
    private var browser: FeiRouteController { browserNavigation.topViewController as? FeiRouteController ?? browserRoot }
    private let browserNavigation: UINavigationController
    private let editorNavigation: UINavigationController
    private var state: FeiNavigationState?
    private var previousTheme: FeiMobileTheme?
    private var previousAppearance = ""
    private var focusSymbol: String?
    private var sidebarMode: DisplayMode?
    private var usesOverlay = false
    private var browserButtonsScope: String?
    private lazy var focusItem: UIBarButtonItem = {
        let item = UIBarButtonItem(image: nil,primaryAction: UIAction { [weak self] _ in self?.toggleSidebar() })
        item.accessibilityIdentifier = "fei.ipad.focus"; return item
    }()
    private func sidebarDismissButton() -> UIBarButtonItem {
        let item = UIBarButtonItem(image: UIImage(systemName: "sidebar.left"),primaryAction: UIAction { [weak self] _ in self?.hide(.primary) })
        item.accessibilityIdentifier = "fei.ipad.dismiss-sidebar"
        item.accessibilityLabel = state?.labels["hide-browser"]
        return item
    }
    private lazy var composeItem: UIBarButtonItem = {
        let item = UIBarButtonItem(systemItem: .compose,primaryAction: nil)
        item.accessibilityIdentifier = "fei.new"; return item
    }()
    private lazy var settingsItem: UIBarButtonItem = {
        let item = UIBarButtonItem(image: UIImage(systemName: "gearshape"),primaryAction: UIAction { [weak self] _ in guard let self else { return }
            if self.state?.kind == "settings" {
                self.editor.navigationCoordinator?.send(self.state?.settings?.page == "Shared" ? "back" : "close-settings")
            } else {
                self.editor.navigationCoordinator?.send("settings")
            } })
        item.accessibilityIdentifier = "fei.settings"; return item
    }()
    private var lastFit: DisplayMode?
    private lazy var sidebarBackGesture: UIPanGestureRecognizer = {
        let gesture = UIPanGestureRecognizer(target: self,action: #selector(swipedToParent(_:)))
        gesture.delegate = self
        gesture.maximumNumberOfTouches = 1
        return gesture
    }()
    private var sidebarSwipeScope: String?
    private var metricsScale: CGFloat = 0
    private var lastBrowserScope: String?
    #if FEI_MOBILE_QA
    private var lastFolderTransition: String?
    #endif
    private let empty = UILabel()

    init(host: EditorHost) {
        editor = host
        let root = FeiRouteController()
        root.isWorkspaceBrowser = true
        root.routeKey = "ipad-browser"
        root.configureNavigationTitleMode()
        browserRoot = root
        browserNavigation = UINavigationController(rootViewController: root)
        editorNavigation = UINavigationController(rootViewController: host)
        super.init(style: .doubleColumn)
        host.padWorkspace = self
        delegate = self
        primaryEdge = .leading
        preferredSplitBehavior = .tile
        showsSecondaryOnlyButton = false
        displayModeButtonVisibility = .never
        browserNavigation.navigationBar.prefersLargeTitles = true
        host.navigationItem.largeTitleDisplayMode = .never
        editorNavigation.navigationBar.prefersLargeTitles = true
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
        view.addGestureRecognizer(sidebarBackGesture)
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
    var visibleBrowserController: FeiRouteController? { primaryVisible ? browser : nil }
    private var primaryVisible: Bool {
        if isCollapsed { return browser.viewIfLoaded?.window != nil }
        let mode = sidebarMode ?? displayMode
        return mode == .oneBesideSecondary || mode == .oneOverSecondary
    }

    func detailRoutes(_ next: FeiNavigationState) -> [FeiNavigationState.Route] {
        let routes = next.routes.filter { $0.key != "notes" && $0.key != "shared" && !$0.key.hasPrefix("folder:") }
        let empty = FeiNavigationState.Route(key: "detail-empty",title: "")
        if routes.isEmpty || ["settings:SharedSync","settings:SharedFolderAccess","settings:Cloud"].contains(routes[0].key) {
            return [empty] + routes
        }
        return routes
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
        browserNavigation.view.backgroundColor = theme.workspaceListColor
        for navigation in [browserNavigation,editorNavigation] {
            navigation.view.tintColor = theme.accentColor
            navigation.navigationBar.tintColor = theme.toolbarColor
            let bar = UINavigationBarAppearance(); bar.configureWithDefaultBackground()
            if let color = theme.chromeColor { bar.backgroundColor = color }
            navigation.navigationBar.standardAppearance = bar
            navigation.navigationBar.scrollEdgeAppearance = bar
            navigation.navigationBar.compactAppearance = bar
        }
        previousTheme = next.theme; previousAppearance = next.appearance
        }
        let path = next.browser?.path ?? []
        updateSidebarGestures()
        let existing = Dictionary(uniqueKeysWithValues: browserNavigation.viewControllers.compactMap { ($0 as? FeiRouteController).map { ($0.routeKey,$0) } })
        let controllers = [browserRoot] + path.map { route in
            let page = existing[route.key] ?? FeiRouteController()
            page.routeKey = route.key
            page.isWorkspaceBrowser = true
            page.title = route.title
            page.configureNavigationTitleMode()
            return page
        }
        let title = next.browser?.title ?? next.labels["sharing"]
        controllers.last?.title = title
        controllers.last?.navigationItem.title = title
        if let destination = controllers.last, browserNavigation.viewControllers.map(ObjectIdentifier.init) != controllers.map(ObjectIdentifier.init) {
            destination.updateListing(next.browser?.listing,labels: next.labels,busy: next.busy,coordinator: coordinator)
            let from = browserNavigation.viewControllers
            let animated = previous != nil && primaryVisible && !UIAccessibility.isReduceMotionEnabled
                && browserNavigation.transitionCoordinator == nil
            let prefix = zip(from,controllers).allSatisfy { $0.0 === $0.1 }
            if animated && prefix && controllers.count == from.count + 1 {
                browserNavigation.pushViewController(destination,animated: true)
                #if FEI_MOBILE_QA
                lastFolderTransition = browserNavigation.transitionCoordinator?.isAnimated == true ? "animated push" : "push without animation"
                #endif
            } else if animated && prefix && controllers.count < from.count {
                browserNavigation.popToViewController(destination,animated: true)
                #if FEI_MOBILE_QA
                lastFolderTransition = browserNavigation.transitionCoordinator?.isAnimated == true ? "animated pop" : "pop without animation"
                #endif
            } else {
                browserNavigation.setViewControllers(controllers,animated: false)
            }
            browserNavigation.interactivePopGestureRecognizer?.isEnabled = false
        }
        browser.updateListing(next.browser?.listing,labels: next.labels,busy: next.busy,coordinator: coordinator)
        if let list = browser.notesList {
            list.browsesWorkspace = true
            list.selectNote(["note","history","history-preview"].contains(next.kind) ? next.current : nil)
            list.configureSearch(in: browser,expanded: !isCollapsed)
        }
        settingsItem.accessibilityLabel = next.labels[next.kind == "settings" ? "close-settings" : "settings"]; settingsItem.isEnabled = !next.busy
        var leading = [settingsItem]
        let browserScope = next.browser?.listing.scope
        if next.browser?.folder != nil {
            let back = UIBarButtonItem(image: UIImage(systemName: "chevron.backward"),primaryAction: UIAction { [weak self] _ in self?.returnToBrowserParent() })
            back.accessibilityIdentifier = "fei.browser.parent"; back.accessibilityLabel = next.labels["back"]; leading = [back]
        }
        browser.navigationItem.hidesBackButton = true
        if browser.notesList?.isSelecting != true && (browserButtonsScope != browserScope || browser.navigationItem.leftBarButtonItems?.map({ $0.accessibilityIdentifier }) != leading.map({ $0.accessibilityIdentifier })) { browser.navigationItem.leftBarButtonItems = leading }
        browserButtonsScope = browserScope
        let items = browser.navigationItem.rightBarButtonItems ?? []
        let close = items.filter { $0.accessibilityIdentifier == "fei.notes.search-close" }
        let more = items.first { $0.accessibilityIdentifier == "fei.library-actions" } ?? coordinator.browserActionsItem()
        more.isEnabled = !next.busy
        #if FEI_MOBILE_QA
        if ProcessInfo.processInfo.environment["FEI_QA_FOLDER_TRANSITIONS"] == "1" {
            more.accessibilityValue = lastFolderTransition
        }
        #endif
        let dismiss = items.first { $0.accessibilityIdentifier == "fei.ipad.dismiss-sidebar" }
        var actions = [more]
        // The browser may stay open while switching between unrelated document sessions.
        if let share = coordinator.folderShareItem(next,browser: true) {
            if next.browser?.folder == previous?.browser?.folder,
               next.browser?.personal == previous?.browser?.personal,
               next.folder_access == previous?.folder_access,
               next.appearance == previous?.appearance,next.theme == previous?.theme,
               let existing = items.first(where: { $0.accessibilityIdentifier == "fei.folder.share" }) {
                existing.isEnabled = !next.busy
                actions.append(existing)
            } else { actions.append(share) }
        }
        if !isCollapsed && close.isEmpty && coordinator.showsComposeShortcut {
            composeItem.tintColor = (next.theme ?? .system).toolbarColor
            coordinator.configureListCompose(composeItem)
            actions.append(composeItem)
        }
        let trailing = close+actions+(usesOverlay && !isCollapsed ? [dismiss ?? sidebarDismissButton()] : [])
        if browser.notesList?.isSelecting != true && browser.notesList?.tableView.isEditing != true && browser.navigationItem.rightBarButtonItems?.map(ObjectIdentifier.init) != trailing.map(ObjectIdentifier.init) { browser.navigationItem.rightBarButtonItems = trailing }
        browser.notesList?.configureSearch(in: browser,expanded: !isCollapsed)
        empty.text = next.labels["select-note"]
        empty.isHidden = !showsEmptyDetail
        if !empty.isHidden { editor.view.bringSubviewToFront(empty) }
        updateFit()
        let detailChanged = next.routes.last?.key != previous?.routes.last?.key
        if !isCollapsed,usesOverlay,detailChanged,
           next.kind == "settings" && previous?.kind != "settings" || ["note","shared-opening"].contains(next.kind) && next.editor_scope != previous?.editor_scope {
            hide(.primary); show(.secondary)
        }
        if isCollapsed {
            if !showsEmptyDetail && detailChanged { show(.secondary) }
            else if showsEmptyDetail && (next.browser?.listing.scope != lastBrowserScope || previous?.kind == "shared-opening") { show(.primary) }
        } else if ["notes","folder"].contains(next.kind) && (previous?.kind == "settings" || view.bounds.height > view.bounds.width && previous?.kind != next.kind) { show(.primary) }
        lastBrowserScope = next.browser?.listing.scope
        editor.refreshEditorLayout()
    }

    func configureDetailButtons(_ controller: UIViewController,state: FeiNavigationState,coordinator: FeiNavigationCoordinator) {
        let ordinaryDetail = ["note","notes","folder"].contains(state.kind)
        let settingsRoot = state.kind == "settings" && state.settings?.page == "root"
        guard ordinaryDetail || settingsRoot else {
            if controller.navigationItem.leftBarButtonItem === focusItem { controller.navigationItem.leftBarButtonItems = nil }
            return
        }
        if ["notes","folder"].contains(state.kind) { controller.navigationItem.rightBarButtonItems = [] }
        updateSidebarButton(state)
        controller.navigationItem.leftItemsSupplementBackButton = !settingsRoot && (controller.navigationController?.viewControllers.count ?? 0) > 1
        if controller.navigationItem.leftBarButtonItem !== focusItem { controller.navigationItem.leftBarButtonItems = [focusItem] }

    }

    private func updateSidebarButton(_ state: FeiNavigationState) {
        let symbol = usesOverlay || isCollapsed ? "sidebar.left" : primaryVisible ? "arrow.up.left.and.arrow.down.right" : "arrow.down.right.and.arrow.up.left"
        if focusSymbol != symbol { focusSymbol = symbol; focusItem.image = UIImage(systemName: symbol) }
        focusItem.accessibilityLabel = state.labels[primaryVisible ? "hide-browser" : "show-browser"]
        focusItem.accessibilityValue = state.labels[primaryVisible ? "sidebar-open" : "sidebar-closed"]
    }

    private func returnToBrowserParent() {
        guard let browser = state?.browser,browser.folder != nil,state?.busy == false,browser.listing.selection == nil else { return }
        let target = browser.path.dropLast().last?.key ?? (!browser.personal ? "shared-origin" : browser.shared ? "shared" : "all")
        editor.navigationCoordinator?.send("browse-scope",value: target,scope: browser.listing.scope)
    }

    private var backDirection: CGFloat { browserNavigation.view.effectiveUserInterfaceLayoutDirection == .rightToLeft ? -1 : 1 }

    private func updateSidebarGestures() {
        let showingSidebar: Bool
        if #available(iOS 26.0,*) { showingSidebar = isShowing(.primary) }
        else { showingSidebar = primaryVisible }
        let nestedSidebar = showingSidebar && state?.browser?.folder != nil
        // The split controller keeps its edge-open gesture while the sidebar is hidden.
        // A visible folder owns swipes originating inside its actual view.
        if presentsWithGesture == nestedSidebar { presentsWithGesture = !nestedSidebar }
        // Check the live hierarchy on touch-down. Disabling during a split transition
        // could cancel a swipe whose browser and scope are still valid.
    }

    func gestureRecognizer(_ gestureRecognizer: UIGestureRecognizer,shouldReceive touch: UITouch) -> Bool {
        guard gestureRecognizer === sidebarBackGesture,state?.busy == false,
              state?.browser?.folder != nil,state?.browser?.listing.selection == nil,
              browser.notesList?.allowsFolderBackGesture == true,
              let column = browserNavigation.viewIfLoaded,column.window != nil,
              let touched = touch.view,touched === column || touched.isDescendant(of: column) else { return false }
        return column.bounds.contains(touch.location(in: column))
    }

    func gestureRecognizerShouldBegin(_ gestureRecognizer: UIGestureRecognizer) -> Bool {
        let velocity = sidebarBackGesture.velocity(in: view)
        return velocity.x*backDirection > abs(velocity.y)*1.35
    }

    func gestureRecognizer(_ gestureRecognizer: UIGestureRecognizer,shouldRecognizeSimultaneouslyWith other: UIGestureRecognizer) -> Bool {
        // A short swipe still exposes row actions. Those actions do not commit on a full swipe.
        gestureRecognizer === sidebarBackGesture && other is UIPanGestureRecognizer &&
            other.view?.isDescendant(of: browserNavigation.view) == true
    }

    @objc private func swipedToParent(_ gesture: UIPanGestureRecognizer) {
        switch gesture.state {
        case .began: sidebarSwipeScope = state?.browser?.listing.scope
        case .ended:
            defer { sidebarSwipeScope = nil }
            guard let scope = sidebarSwipeScope,scope == state?.browser?.listing.scope else { return }
            let travel = gesture.translation(in: view)
            let minimum = min(96,max(56,browserNavigation.view.bounds.width*0.23))
            if travel.x*backDirection >= minimum && abs(travel.y) < abs(travel.x)*0.55 { returnToBrowserParent() }
        case .cancelled,.failed: sidebarSwipeScope = nil
        default: break
        }
    }

    private func toggleSidebar() {
        editor.settleEditorComposition()
        if primaryVisible && !isCollapsed { hide(.primary) } else { show(.primary) }
    }
    func beginEditorInput() {
        if !isCollapsed,displayMode == .oneOverSecondary { hide(.primary) }
    }

    override func viewDidLayoutSubviews() {
        super.viewDidLayoutSubviews()
        updateFit()
        updateSidebarGestures()
        browser.notesList?.configureSearch(in: browser,expanded: !isCollapsed)
    }
    @objc private func contentSizeChanged() { lastFit = nil; updateFit() }
    private func updateFit() {
        guard isViewLoaded,view.bounds.width > 0 else { return }
        // Product tuning, not measurements of Apple's column widths.
        let scale = min(1.45,max(1,UIFont.preferredFont(forTextStyle: .body).pointSize/17))
        let listMinimum = 280*scale, detailMinimum = 400*min(scale,1.2)
        let width = view.bounds.inset(by: view.safeAreaInsets).width
        let overlay = view.bounds.height > view.bounds.width || width < listMinimum+detailMinimum+1
        if metricsScale != scale {
            metricsScale = scale
            minimumPrimaryColumnWidth = listMinimum
            maximumPrimaryColumnWidth = max(420,listMinimum)
            if #available(iOS 26.0,*) { minimumSecondaryColumnWidth = detailMinimum }
        }
        let preferred = min(maximumPrimaryColumnWidth,max(listMinimum,overlay ? 340 : floor(width/3)))
        if preferredPrimaryColumnWidth != preferred { preferredPrimaryColumnWidth = preferred }
        usesOverlay = overlay
        let mode: DisplayMode = overlay ? .oneOverSecondary : .oneBesideSecondary
        if lastFit != mode {
            lastFit = mode
            preferredSplitBehavior = overlay ? .overlay : .tile
            preferredDisplayMode = mode
            if overlay && !showsEmptyDetail { hide(.primary) } else { show(.primary) }
            refreshColumns()
        }
    }
    func splitViewController(_ svc: UISplitViewController,topColumnForCollapsingToProposedTopColumn proposedTopColumn: Column) -> Column {
        state.map { ["note","shared-opening","history","history-preview","settings","trash"].contains($0.kind) } == true ? .secondary : .primary
    }
    func splitViewController(_ svc: UISplitViewController,willChangeTo displayMode: DisplayMode) {
        sidebarMode = displayMode
        DispatchQueue.main.async { [weak self] in self?.updateSidebarGestures() }
        if let state { updateSidebarButton(state) }
    }
    func splitViewControllerDidCollapse(_ svc: UISplitViewController) { refreshColumns() }
    func splitViewControllerDidExpand(_ svc: UISplitViewController) { refreshColumns() }
    private func refreshColumns() {
        updateSidebarGestures()
        browser.notesList?.configureSearch(in: browser,expanded: !isCollapsed)
        if !usesOverlay || isCollapsed { browser.navigationItem.rightBarButtonItems?.removeAll { $0.accessibilityIdentifier == "fei.ipad.dismiss-sidebar" } }
        else if browser.notesList?.isSelecting != true && browser.notesList?.tableView.isEditing != true && browser.navigationItem.rightBarButtonItems?.contains(where: { $0.accessibilityIdentifier == "fei.ipad.dismiss-sidebar" }) != true {
            browser.navigationItem.rightBarButtonItems = (browser.navigationItem.rightBarButtonItems ?? [])+[sidebarDismissButton()]
        }
        editor.refreshEditorLayout()
        editor.navigationCoordinator?.refreshPadButtons()
    }
}
