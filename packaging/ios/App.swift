import UIKit
import CloudKit
import GameController

@main
final class AppDelegate: UIResponder, UIApplicationDelegate {
    func application(_ application: UIApplication,didFinishLaunchingWithOptions launchOptions: [UIApplication.LaunchOptionsKey: Any]?) -> Bool { FeiReminderNotifications.shared.start(); return true }
    func applicationWillTerminate(_ application: UIApplication) { EditorHost.active?.stopForTermination() }
    func application(
        _ application: UIApplication,
        configurationForConnecting session: UISceneSession,
        options: UIScene.ConnectionOptions
    ) -> UISceneConfiguration {
        let config = UISceneConfiguration(name: nil, sessionRole: session.role)
        config.delegateClass = EditorScene.self
        return config
    }
}

final class EditorScene: UIResponder, UIWindowSceneDelegate {
    var window: UIWindow?

    func scene(_ scene: UIScene, willConnectTo session: UISceneSession, options: UIScene.ConnectionOptions) {
        guard let scene = scene as? UIWindowScene else { return }
        let window = UIWindow(windowScene: scene)
        let host = EditorHost()
        if UIDevice.current.userInterfaceIdiom == .pad {
            window.rootViewController = FeiPadWorkspaceController(host: host)
        } else {
            host.configureNavigationTitleMode()
            let navigation = UINavigationController(rootViewController: host)
            navigation.navigationBar.prefersLargeTitles = true
            window.rootViewController = navigation
        }
        self.window = window
        window.makeKeyAndVisible()
        #if FEI_MOBILE_QA
        if let value = ProcessInfo.processInfo.environment["FEI_MOBILE_QA_INVITATION_URL"],let url = URL(string: value) { EditorHost.receiveInvitation(url) }
        #endif
        if let url = options.urlContexts.first?.url { EditorHost.receiveInvitation(url) }
        if let metadata = options.cloudKitShareMetadata,let url = metadata.share.url { EditorHost.receiveInvitation(url) }
        for activity in options.userActivities { if let url = activity.webpageURL { EditorHost.receiveInvitation(url) } }
    }

    func scene(_ scene: UIScene,openURLContexts contexts: Set<UIOpenURLContext>) {
        if let url = contexts.first?.url { EditorHost.receiveInvitation(url) }
    }
    func scene(_ scene: UIScene,continue userActivity: NSUserActivity) {
        if let url = userActivity.webpageURL { EditorHost.receiveInvitation(url) }
    }
    func windowScene(_ windowScene: UIWindowScene,userDidAcceptCloudKitShareWith metadata: CKShare.Metadata) {
        if let url = metadata.share.url { EditorHost.receiveInvitation(url) }
    }

    func sceneDidBecomeActive(_ scene: UIScene) {
        gpui_ios_did_become_active(nil)
        fei_ios_presence_scene_active(true)
        FeiReminderNotifications.shared.refresh()
    }

    func sceneWillResignActive(_ scene: UIScene) {
        fei_ios_presence_scene_active(false)
        fei_ipad_flush()
        FeiReminderNotifications.shared.refresh()
        gpui_ios_will_resign_active(nil)
    }
}

final class EditorHost: FeiRouteController {
    static weak var active: EditorHost?
    private static var pendingInvitation: URL?
    static func receiveInvitation(_ url: URL) {
        // Rust validates the full invitation. Do not accept a share from a URL callback.
        guard url.scheme == "feinotes" || (url.scheme == "https" && ["feinotes.app","www.icloud.com","icloud.com","www.icloud.com.cn","icloud.com.cn"].contains(url.host ?? "")) else { return }
        pendingInvitation = url; active?.deliverPendingInvitation()
    }
    func deliverPendingInvitation() {
        guard let coordinator = navigationCoordinator,coordinator.readyForInvitation,let url = Self.pendingInvitation else { return }
        DispatchQueue.main.async { [weak self] in
            guard let self,Self.pendingInvitation == url,let coordinator=self.navigationCoordinator,coordinator.readyForInvitation else { return }
            Self.pendingInvitation = nil
            coordinator.send("receive-invitation",value: url.absoluteString)
        }
    }
    weak var padWorkspace: FeiPadWorkspaceController?
    var navigationCoordinator: FeiNavigationCoordinator?
    var findCoordinator: FeiFindCoordinator?
    private var frameConstraints: [NSLayoutConstraint] = []
    private let editorToolbar = FeiEditorToolbar()
    private let editorToolbarContainer = UIView()
    private var editorToolbarHeight: NSLayoutConstraint?
    private var editorComponentRowHeight: NSLayoutConstraint?
    private var editorToolbarVisible = false
    private var toolbarState: FeiNavigationState?
    private var inputWasFocused = false
    private var inputPresenceScope: String?
    private var keyboardEndFrame = CGRect.null
    private var lastSystemDark: Bool?
    private let layoutProbeEnabled = ProcessInfo.processInfo.environment["FEI_NOTES_LAYOUT_PROBE"] == "1"
    private var findImmersive = false
    private var toolbarAtTop = false
    private var editorOverlapsChrome = false
    private var editorExtendsTop = false
    private var lastEditorInsets = UIEdgeInsets.zero
    private static let testChannelKey = "FeiNotesCloudChannel"
    private var editorView: EditorFrameView?
    private var displayLink: CADisplayLink?
    private let browserTapGuard = UIView()
    private var browserTapAction: (() -> Void)?

    func stopForTermination() {
        findCoordinator?.dismiss(notify: false)
        displayLink?.invalidate()
        displayLink = nil
        editorView?.removeFromSuperview()
        editorView = nil
        fei_ipad_shutdown()
        Self.active = nil
    }

    override func viewDidLoad() {
        super.viewDidLoad()
        NotificationCenter.default.addObserver(self,selector: #selector(editorKeyboardMoved(_:)),name: UIResponder.keyboardDidChangeFrameNotification,object: nil)
        NotificationCenter.default.addObserver(self,selector: #selector(editorKeyboardHidden),name: UIResponder.keyboardDidHideNotification,object: nil)
        Self.active = self
        navigationCoordinator = FeiNavigationCoordinator(host: self)
        NotificationCenter.default.addObserver(self,selector: #selector(contentSizeChanged),name: UIContentSizeCategory.didChangeNotification,object: nil)
        view.backgroundColor = .systemBackground

        let library = URL.applicationSupportDirectory.appending(path: "FeiNotesEditorProbe")
        let environment = ProcessInfo.processInfo.environment
        let defaults = UserDefaults.standard
        let requested = environment["FEI_NOTES_CLOUD_CHANNEL"] ?? environment["FEI_NOTES_MVP_CHANNEL"]
            ?? defaults.string(forKey: Self.testChannelKey) ?? defaults.string(forKey: "FeiNotesMVPCloudChannel")
        guard let id = requested.flatMap(UUID.init(uuidString:)) ?? (requested == nil ? UUID() : nil) else {
            showStatus("iCloud 文档库标识无效。")
            return
        }
        let channel = id.uuidString.lowercased()
        guard library.path.withCString({ base in channel.withCString { fei_ios_configure_library(base, $0) } }) else {
            showStatus("文档库配置无效。")
            return
        }
        let mode = environment["FEI_NOTES_MODE"] ?? environment["FEI_NOTES_MVP_MODE"]
            ?? defaults.string(forKey: "FeiNotesLibraryMode") ?? (feiIOSProductCloud() || requested != nil ? "cloud" : "local")
        let wantsCloud = mode == "cloud"
        if wantsCloud { defaults.set(channel, forKey: Self.testChannelKey) }
        Task { @MainActor in
            let ready = await Task.detached {
                if wantsCloud && library.path.withCString({ path in channel.withCString { fei_ipad_prepare_cloud(path, $0) } }) { return true }
                return fei_ios_prepare_local()
            }.value
            if ready { startEditor(at: library) }
            else { showStatus("本机文档库暂时无法打开，原有文档已保留。") }
        }
    }

    @discardableResult
    private func showStatus(_ text: String) -> UILabel {
        let label = UILabel()
        label.text = text
        label.textAlignment = .center
        label.numberOfLines = 0
        label.frame = view.bounds
        label.autoresizingMask = [.flexibleWidth, .flexibleHeight]
        view.addSubview(label)
        return label
    }

    private func startEditor(at library: URL) {
        #if FEI_MOBILE_QA
        if Bundle.main.bundleIdentifier == "app.feinotes.mobile.feedback45",ProcessInfo.processInfo.environment["FEI_MOBILE_QA_SEED"] == "1",let fixture = Bundle.main.url(forResource: "MobileFixtures",withExtension: nil) { setenv("FEI_MOBILE_QA_FIXTURES",fixture.path,1) }
        #endif
        guard library.path.withCString(fei_ipad_start),
              let window = gpui_ios_get_window(),
              let pointer = gpui_ios_view_controller(window) else {
            showStatus("飞记编辑器启动失败")
            return
        }
        let controller = Unmanaged<UIViewController>.fromOpaque(pointer).takeUnretainedValue()
        let frameView = EditorFrameView(window: window, controller: controller)
        editorView = frameView
        findCoordinator = FeiFindCoordinator(view: frameView)
        frameView.translatesAutoresizingMaskIntoConstraints = false
        frameView.addSubview(controller.view)
        attachEditor(to: self)
        view.setNeedsLayout()
        startDisplayLink()
    }

    func attachEditor(to parent: FeiRouteController) {
        guard let frameView = editorView else { return }
        parent.loadViewIfNeeded()
        parent.thawContent()
        prepareEditorTransition(to: parent)
        frameView.isHidden = parent.hasNativeContent || !(parent.routeKey.hasPrefix("note:") || parent.routeKey.hasPrefix("history-preview"))
        if frameView.superview === parent.view { return }
        NSLayoutConstraint.deactivate(frameConstraints)
        let controller = frameView.controller
        controller.willMove(toParent: nil)
        frameView.removeFromSuperview()
        controller.removeFromParent()
        parent.addChild(controller)
        parent.view.addSubview(frameView)
        editorToolbarContainer.removeFromSuperview()
        editorToolbarContainer.translatesAutoresizingMaskIntoConstraints = false
        editorToolbarContainer.clipsToBounds = false
        if editorToolbar.superview == nil {
            let components = editorToolbar.componentBar
            editorToolbarContainer.addSubview(components)
            editorToolbarContainer.addSubview(editorToolbar)
            let componentHeight = components.heightAnchor.constraint(equalToConstant: 0)
            editorComponentRowHeight = componentHeight
            editorToolbar.onComponentRowChanged = { [weak self] in self?.layoutEditorToolbar() }
            NSLayoutConstraint.activate([
                components.topAnchor.constraint(equalTo: editorToolbarContainer.topAnchor),
                components.leadingAnchor.constraint(equalTo: editorToolbarContainer.leadingAnchor),
                components.trailingAnchor.constraint(equalTo: editorToolbarContainer.trailingAnchor),
                componentHeight,
                editorToolbar.topAnchor.constraint(equalTo: components.bottomAnchor),
                editorToolbar.leadingAnchor.constraint(equalTo: editorToolbarContainer.leadingAnchor),
                editorToolbar.trailingAnchor.constraint(equalTo: editorToolbarContainer.trailingAnchor),
                editorToolbar.heightAnchor.constraint(equalToConstant: FeiEditorToolbar.preferredHeight)
            ])
        }
        parent.view.addSubview(editorToolbarContainer)
        configureEditorLayout(in: parent,frameView: frameView)
        controller.didMove(toParent: parent)
        parent.view.setNeedsLayout()
    }

    func prepareEditorTransition(to parent: FeiRouteController) {
        layoutEditorToolbar(in: parent)
    }

    private func configureEditorLayout(in parent: FeiRouteController,frameView: UIView) {
        NSLayoutConstraint.deactivate(frameConstraints)
        parent.view.keyboardLayoutGuide.followsUndockedKeyboard = false
        toolbarAtTop = usesTopTools(in: parent)
        let height = editorToolbarContainer.heightAnchor.constraint(equalToConstant: 0)
        editorToolbarHeight = height
        editorOverlapsChrome = shouldOverlapChrome
        editorExtendsTop = shouldExtendTop
        let extendsBehindHomeIndicator = editorOverlapsChrome && UIDevice.current.userInterfaceIdiom == .phone
        parent.view.keyboardLayoutGuide.usesBottomSafeArea = !extendsBehindHomeIndicator
        let top = editorExtendsTop ? parent.view.topAnchor : parent.view.safeAreaLayoutGuide.topAnchor
        frameConstraints = [frameView.topAnchor.constraint(equalTo: top),height,
            editorToolbarContainer.leadingAnchor.constraint(equalTo: parent.view.safeAreaLayoutGuide.leadingAnchor),
            editorToolbarContainer.trailingAnchor.constraint(equalTo: parent.view.safeAreaLayoutGuide.trailingAnchor),
            frameView.leadingAnchor.constraint(equalTo: parent.view.safeAreaLayoutGuide.leadingAnchor),
            frameView.trailingAnchor.constraint(equalTo: parent.view.safeAreaLayoutGuide.trailingAnchor)]
        let bottom = editorOverlapsChrome || toolbarAtTop ? parent.view.keyboardLayoutGuide.topAnchor : editorToolbarContainer.topAnchor
        let docked = editorToolbarContainer.bottomAnchor.constraint(equalTo: parent.view.keyboardLayoutGuide.topAnchor)
        frameConstraints.append(frameView.bottomAnchor.constraint(equalTo: bottom))
        if extendsBehindHomeIndicator {
            docked.priority = .defaultHigh
            frameConstraints.append(editorToolbarContainer.bottomAnchor.constraint(lessThanOrEqualTo: parent.view.safeAreaLayoutGuide.bottomAnchor))
            frameConstraints.append(editorToolbarContainer.bottomAnchor.constraint(lessThanOrEqualTo: parent.view.keyboardLayoutGuide.topAnchor))
        }
        frameConstraints.append(docked)
        NSLayoutConstraint.activate(frameConstraints)
        layoutEditorToolbar(in: parent)
    }
    private func layoutEditorToolbar(in route: FeiRouteController? = nil) {
        let parent = route ?? editorView?.controller.parent as? FeiRouteController
        let isEditor = parent.map { !$0.hasNativeContent && ($0.routeKey.hasPrefix("note:") ||
            $0.routeKey.hasPrefix("draft:") || $0.routeKey.hasPrefix("history-preview")) } ?? false
        let visible = route == nil ? editorToolbarVisible : toolbarState?.find_active != true && toolbarState?.board_open != true
        let show = isEditor && visible && !toolbarAtTop
        editorToolbarContainer.isHidden = !show
        let row = show && !toolbarAtTop && editorToolbar.hasComponentRow
        editorToolbar.componentBar.isHidden = !row
        editorComponentRowHeight?.constant = row ? FeiEditorToolbar.preferredHeight : 0
        editorToolbarHeight?.constant = show ? FeiEditorToolbar.preferredHeight * (row ? 2 : 1) : 0
    }
    private func usesTopTools(in parent: FeiRouteController) -> Bool {
        guard let state = toolbarState, state.kind == "note", padWorkspace?.usesTopEditorTools == true else { return false }
        return parent.view.bounds.width >= 600
    }
    private var shouldOverlapChrome: Bool {
        toolbarState?.editorOverlapsChrome == true && !findImmersive
    }
    private var shouldExtendTop: Bool {
        shouldOverlapChrome || (findImmersive && toolbarState?.document_needs_attention != true)
    }
    var editorChromeInsets: UIEdgeInsets {
        guard editorOverlapsChrome,let frame = editorView,let parent = frame.superview,!frame.isHidden else { return .zero }
        let safe = parent.safeAreaLayoutGuide.layoutFrame
        let keyboardTop = parent.keyboardLayoutGuide.layoutFrame.minY
        let bottom = min(keyboardTop,safe.maxY) - (editorToolbarHeight?.constant ?? 0)
        return UIEdgeInsets(top: max(0,safe.minY-frame.frame.minY),left: 0,bottom: max(0,frame.frame.maxY-bottom),right: 0)
    }
    func refreshEditorLayout() {
        guard let frameView = editorView,let parent = frameView.controller.parent as? FeiRouteController else { return }
        let expanded = usesTopTools(in: parent)
        if expanded != toolbarAtTop || shouldOverlapChrome != editorOverlapsChrome || shouldExtendTop != editorExtendsTop {
            configureEditorLayout(in: parent,frameView: frameView)
        }
    }

    func updateEditorToolbar(_ state: FeiNavigationState) {
        toolbarState = state
        editorToolbarContainer.isUserInteractionEnabled = !state.conflict_review
        refreshEditorLayout()
        #if FEI_MOBILE_QA
        if let qa = state.qa {
            editorView?.isAccessibilityElement=true; editorView?.accessibilityValue=state.board_open ? qa.board : qa.scroll
            if ProcessInfo.processInfo.environment["FEI_QA_SHARED_BROWSER_REFRESH"] == "1" {
                editorView?.accessibilityValue = qa.body
            }
            editorToolbar.accessibilityValue = "draft=\(qa.draft) gen=\(qa.generation) title=\(qa.title) body=\(qa.body) selection=\(qa.selection.anchor.block):\(qa.selection.anchor.offset)-\(qa.selection.head.block):\(qa.selection.head.offset) scroll=\(qa.scroll)"
        }
        #endif
        editorToolbarVisible = (state.kind == "note" || state.kind == "history-preview") && !state.find_active && !state.board_open && (editorView?.controller.parent as? FeiRouteController)?.hasNativeContent != true
        editorToolbar.update(state,coordinator: navigationCoordinator,typing: editorHasKeyboardFocus,topTools: toolbarAtTop)
        layoutEditorToolbar()
        editorToolbarContainer.superview?.setNeedsLayout()
        refreshEditorInput()
    }
    @objc private func contentSizeChanged() {
        navigationCoordinator?.send("type-scale")
        editorToolbar.refreshContentSize()
        navigationController?.topViewController?.view.setNeedsLayout()
    }
    func refreshEditorToolbarKeyboard() {
        editorToolbar.updateKeyboardVisibility()
        if layoutProbeEnabled,let parent = editorToolbarContainer.superview,let editorView {
            func rect(_ frame: CGRect) -> [CGFloat] { [frame.minX,frame.minY,frame.width,frame.height] }
            let metrics = ["keyboard":rect(parent.keyboardLayoutGuide.layoutFrame),"toolbar":rect(editorToolbarContainer.convert(editorToolbarContainer.bounds,to: parent)),"editor":rect(editorView.convert(editorView.bounds,to: parent)),"safeArea":rect(parent.safeAreaLayoutGuide.layoutFrame)]
            if let data = try? JSONSerialization.data(withJSONObject: metrics,options: [.sortedKeys]),let json = String(data: data,encoding: .utf8),editorToolbar.accessibilityValue != json { editorToolbar.accessibilityValue = json }
        }
        refreshEditorInput()
    }

    private func refreshEditorInput() {
        let focused = editorHasKeyboardFocus
        let scope = editorOwnsFirstResponder ? toolbarState?.editor_scope : nil
        let focusChanged = focused != inputWasFocused
        inputWasFocused = focused
        if focusChanged || scope != inputPresenceScope {
            inputPresenceScope = scope
            if let scope { scope.withCString { fei_ios_set_editor_input(focused,$0) } }
            else { fei_ios_set_editor_input(focused,nil) }
        }
        guard focusChanged else { return }
        if focused { padWorkspace?.beginEditorInput() }
        if let state = toolbarState { editorToolbar.update(state,coordinator: navigationCoordinator,typing: focused,topTools: toolbarAtTop) }
        layoutEditorToolbar()
        navigationCoordinator?.refreshInputControls()
    }
    func setBrowserTapGuard(_ action: (() -> Void)?) {
        browserTapAction = action
        guard let editorView else { return }
        if action == nil { browserTapGuard.removeFromSuperview(); return }
        if browserTapGuard.gestureRecognizers?.isEmpty != false { browserTapGuard.addGestureRecognizer(UITapGestureRecognizer(target: self,action: #selector(editorBrowserTap))) }
        browserTapGuard.accessibilityIdentifier = "fei.ipad.reading-surface"
        browserTapGuard.isAccessibilityElement = true; browserTapGuard.accessibilityLabel = navigationCoordinator?.localized("专注正文","Focus on document")
        browserTapGuard.frame = editorView.bounds; browserTapGuard.autoresizingMask = [.flexibleWidth,.flexibleHeight]
        editorView.addSubview(browserTapGuard)
    }
    @objc private func editorBrowserTap() { let action = browserTapAction; setBrowserTapGuard(nil); action?() }
    var navigationEditorTools: [UIBarButtonItem] { toolbarAtTop && toolbarState?.kind == "note" && toolbarState?.find_active != true ? editorToolbar.navigationItems : [] }
    func prepareEditorInput() { view.layoutIfNeeded(); drawFrame(); gpui_ios_show_keyboard(gpui_ios_get_window()) }
    func settleEditorComposition() {
        func input(_ view: UIView) -> (any UITextInput)? {
            if view.isFirstResponder,let input = view as? any UITextInput { return input }
            return view.subviews.lazy.compactMap(input).first
        }
        guard let editorView,let input = input(editorView),input.markedTextRange != nil else { return }
        // Settle UIKit's buffer before entering Rust's scoped toolbar command.
        input.unmarkText()
    }
    @objc private func editorKeyboardMoved(_ notification: Notification) {
        keyboardEndFrame = (notification.userInfo?[UIResponder.keyboardFrameEndUserInfoKey] as? NSValue)?.cgRectValue ?? .null
    }
    @objc private func editorKeyboardHidden() { keyboardEndFrame = .null }

    private var editorOwnsFirstResponder: Bool {
        func focused(_ view: UIView) -> Bool { view.isFirstResponder || view.subviews.contains(where: focused) }
        guard toolbarState?.kind == "note",let editorView,!editorView.isHidden,editorView.window != nil else { return false }
        return focused(editorView)
    }

    var editorHasKeyboardFocus: Bool {
        guard editorOwnsFirstResponder,let window = editorView?.window else { return false }
        let keyboard = window.convert(keyboardEndFrame,from: window.screen.coordinateSpace)
        let softKeyboard = !keyboardEndFrame.isNull && window.bounds.intersects(keyboard) && keyboard.height > 1
        let parent = editorToolbarContainer.superview
        let docked = parent.map { $0.keyboardLayoutGuide.layoutFrame.minY < $0.safeAreaLayoutGuide.layoutFrame.maxY-1 } ?? false
        return softKeyboard || docked || GCKeyboard.coalesced != nil
    }

    func setFindImmersive(_ active: Bool) {
        guard findImmersive != active else { return }
        findImmersive = active
        guard let frameView = editorView,let parent = frameView.controller.parent as? FeiRouteController else { return }
        configureEditorLayout(in: parent,frameView: frameView)
        parent.view.setNeedsLayout()
    }

    override func viewDidAppear(_ animated: Bool) {
        super.viewDidAppear(animated)
        startDisplayLink()
    }

    private func startDisplayLink() {
        guard editorView != nil, displayLink == nil else { return }
        let link = CADisplayLink(target: self, selector: #selector(drawFrame))
        link.add(to: .main, forMode: .common)
        displayLink = link
    }

    func refreshSystemAppearance() {
        let dark = feiIOSSystemDark()
        guard lastSystemDark != dark,editorView != nil else { return }
        lastSystemDark = dark
        navigationCoordinator?.send("system-appearance")
    }

    @objc private func drawFrame() {
        guard UIApplication.shared.applicationState == .active else { return }
        refreshSystemAppearance()
        refreshEditorToolbarKeyboard()
        let insets = editorChromeInsets
        if lastEditorInsets != insets {
            lastEditorInsets = insets
            navigationCoordinator?.send("editor-chrome-layout")
        }
        editorView?.drawFrame()
    }
}

@MainActor
@_cdecl("fei_ios_system_dark")
func feiIOSSystemDark() -> Bool {
    let scene = EditorHost.active?.viewIfLoaded?.window?.windowScene
        ?? UIApplication.shared.connectedScenes.compactMap { $0 as? UIWindowScene }.first
    return scene?.traitCollection.userInterfaceStyle == .dark
}

@MainActor
@_cdecl("fei_ios_editor_top_inset")
func feiIOSEditorTopInset() -> Float { Float(EditorHost.active?.editorChromeInsets.top ?? 0) }

@MainActor
@_cdecl("fei_ios_editor_bottom_inset")
func feiIOSEditorBottomInset() -> Float { Float(EditorHost.active?.editorChromeInsets.bottom ?? 0) }

private final class EditorFrameView: UIView {
    let gpuiWindow: UnsafeMutableRawPointer
    let controller: UIViewController

    init(window: UnsafeMutableRawPointer, controller: UIViewController) {
        self.gpuiWindow = window
        self.controller = controller
        super.init(frame: .zero)
        clipsToBounds = true
        accessibilityIdentifier = "fei.editor.surface"
    }

    required init?(coder: NSCoder) { fatalError("init(coder:) is unsupported") }

    override func layoutSubviews() {
        super.layoutSubviews()
        guard bounds.width > 0, bounds.height > 0 else { return }
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        controller.view.frame = bounds
        gpui_ios_layout_view(gpuiWindow)
        drawFrame()
        CATransaction.commit()
    }

    func drawFrame() { _ = gpui_ios_request_frame(gpuiWindow) }
}


@MainActor
@_cdecl("fei_ios_home_available")
func feiIOSHomeAvailable() -> Bool {
    guard UIApplication.shared.applicationState == .active, let host = EditorHost.active,
          host.navigationCoordinator?.readyForCloudResume == true,
          host.view.window?.rootViewController?.presentedViewController == nil else { return false }
    return host.navigationController?.transitionCoordinator == nil
}

@_cdecl("fei_ios_library_selection_changed")
func feiIOSLibrarySelectionChanged(_ cloud: Bool, _ channel: UnsafePointer<CChar>) {
    let defaults = UserDefaults.standard
    defaults.set(String(cString: channel), forKey: "FeiNotesCloudChannel")
    defaults.set(cloud ? "cloud" : "local", forKey: "FeiNotesLibraryMode")
}
