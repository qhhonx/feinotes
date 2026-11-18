import UIKit
import CloudKit

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
        window.rootViewController = UIDevice.current.userInterfaceIdiom == .pad ? FeiPadWorkspaceController(host: host) : UINavigationController(rootViewController: host)
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
        FeiReminderNotifications.shared.refresh()
    }

    func sceneWillResignActive(_ scene: UIScene) {
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
    private var editorToolbarVisible = false
    private var toolbarState: FeiNavigationState?
    private var inputWasFocused = false
    private let layoutProbeEnabled = ProcessInfo.processInfo.environment["FEI_NOTES_LAYOUT_PROBE"] == "1"
    private var findImmersive = false
    private var toolbarAtTop = false
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
        Self.active = self
        navigationCoordinator = FeiNavigationCoordinator(host: self)
        NotificationCenter.default.addObserver(self,selector: #selector(contentSizeChanged),name: UIContentSizeCategory.didChangeNotification,object: nil)
        view.backgroundColor = .systemBackground

        let library = URL.applicationSupportDirectory.appending(path: "FeiNotesEditorProbe")
        let environment = ProcessInfo.processInfo.environment
        let defaults = UserDefaults.standard
        #if FEI_UPSTREAM_PREVIEW
        let requested = environment["FEI_NOTES_CLOUD_CHANNEL"] ?? Bundle.main.object(forInfoDictionaryKey: "FeiUpstreamPreviewCloudChannel") as? String
        #else
        let requested = environment["FEI_NOTES_CLOUD_CHANNEL"] ?? environment["FEI_NOTES_MVP_CHANNEL"]
            ?? defaults.string(forKey: Self.testChannelKey) ?? defaults.string(forKey: "FeiNotesMVPCloudChannel")
        #endif
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
            else { showStatus("本机文档库暂时无法打开，原有笔记已保留。") }
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
        frameView.isHidden = parent.hasNativeContent || (parent === self && padWorkspace?.showsEmptyDetail == true)
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
            editorToolbarContainer.addSubview(editorToolbar)
            NSLayoutConstraint.activate([editorToolbar.topAnchor.constraint(equalTo: editorToolbarContainer.topAnchor),editorToolbar.leadingAnchor.constraint(equalTo: editorToolbarContainer.leadingAnchor),editorToolbar.trailingAnchor.constraint(equalTo: editorToolbarContainer.trailingAnchor),editorToolbar.heightAnchor.constraint(equalToConstant: FeiEditorToolbar.preferredHeight)])
        }
        parent.view.addSubview(editorToolbarContainer)
        configureEditorLayout(in: parent,frameView: frameView)
        controller.didMove(toParent: parent)
        parent.view.setNeedsLayout()
    }

    func prepareEditorTransition(to parent: FeiRouteController) {
        let visible = !parent.hasNativeContent && (parent.routeKey.hasPrefix("note:") || parent.routeKey.hasPrefix("draft:") || parent.routeKey.hasPrefix("history-preview")) && toolbarState?.find_active != true
        editorToolbarContainer.isHidden = !visible || toolbarAtTop
        editorToolbarHeight?.constant = visible && !toolbarAtTop ? FeiEditorToolbar.preferredHeight : 0
    }

    private func configureEditorLayout(in parent: FeiRouteController,frameView: UIView) {
        NSLayoutConstraint.deactivate(frameConstraints)
        parent.view.keyboardLayoutGuide.followsUndockedKeyboard = false
        toolbarAtTop = padWorkspace?.usesTopEditorTools == true && parent.view.bounds.width >= 360
        let height = editorToolbarContainer.heightAnchor.constraint(equalToConstant: editorToolbarVisible && !toolbarAtTop ? FeiEditorToolbar.preferredHeight : 0)
        editorToolbarHeight = height
        let top = findImmersive ? parent.view.topAnchor : toolbarAtTop ? editorToolbarContainer.bottomAnchor : parent.view.safeAreaLayoutGuide.topAnchor
        frameConstraints = [frameView.topAnchor.constraint(equalTo: top),height,
            editorToolbarContainer.leadingAnchor.constraint(equalTo: parent.view.safeAreaLayoutGuide.leadingAnchor),
            editorToolbarContainer.trailingAnchor.constraint(equalTo: parent.view.safeAreaLayoutGuide.trailingAnchor),
            frameView.leadingAnchor.constraint(equalTo: parent.view.safeAreaLayoutGuide.leadingAnchor),
            frameView.trailingAnchor.constraint(equalTo: parent.view.safeAreaLayoutGuide.trailingAnchor)]
        if toolbarAtTop {
            frameConstraints += [editorToolbarContainer.topAnchor.constraint(equalTo: parent.view.safeAreaLayoutGuide.topAnchor),frameView.bottomAnchor.constraint(equalTo: parent.view.keyboardLayoutGuide.topAnchor)]
        } else {
            frameConstraints += [frameView.bottomAnchor.constraint(equalTo: editorToolbarContainer.topAnchor),editorToolbarContainer.bottomAnchor.constraint(equalTo: parent.view.keyboardLayoutGuide.topAnchor)]
        }
        NSLayoutConstraint.activate(frameConstraints)
    }
    func refreshEditorLayout() {
        guard let frameView = editorView,let parent = frameView.controller.parent as? FeiRouteController else { return }
        let expanded = padWorkspace?.usesTopEditorTools == true && parent.view.bounds.width >= 360
        if expanded != toolbarAtTop { configureEditorLayout(in: parent,frameView: frameView) }
    }

    func updateEditorToolbar(_ state: FeiNavigationState) {
        toolbarState = state
        #if FEI_MOBILE_QA
        if let qa = state.qa {
            editorView?.isAccessibilityElement=true; editorView?.accessibilityValue=qa.scroll
            editorToolbar.accessibilityValue = "draft=\(qa.draft) gen=\(qa.generation) title=\(qa.title) body=\(qa.body) selection=\(qa.selection.anchor.block):\(qa.selection.anchor.offset)-\(qa.selection.head.block):\(qa.selection.head.offset) scroll=\(qa.scroll)"
        }
        #endif
        editorToolbarVisible = (state.kind == "note" || state.kind == "history-preview") && !state.find_active && (editorView?.controller.parent as? FeiRouteController)?.hasNativeContent != true
        editorToolbarContainer.isHidden = !editorToolbarVisible || toolbarAtTop
        editorToolbarHeight?.constant = editorToolbarVisible && !toolbarAtTop ? FeiEditorToolbar.preferredHeight : 0
        editorToolbar.update(state,coordinator: navigationCoordinator,typing: editorHasKeyboardFocus)
        editorToolbarContainer.superview?.setNeedsLayout()
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
            let metrics = ["keyboard":rect(parent.keyboardLayoutGuide.layoutFrame),"toolbar":rect(editorToolbar.convert(editorToolbar.bounds,to: parent)),"editor":rect(editorView.convert(editorView.bounds,to: parent)),"safeArea":rect(parent.safeAreaLayoutGuide.layoutFrame)]
            if let data = try? JSONSerialization.data(withJSONObject: metrics,options: [.sortedKeys]),let json = String(data: data,encoding: .utf8),editorToolbar.accessibilityValue != json { editorToolbar.accessibilityValue = json }
        }
        let focused = editorHasKeyboardFocus
        guard focused != inputWasFocused else { return }
        inputWasFocused = focused
        if focused { padWorkspace?.beginEditorInput() }
        if let state = toolbarState { editorToolbar.update(state,coordinator: navigationCoordinator,typing: focused) }
        navigationCoordinator?.refreshInputControls()
    }
    func setBrowserTapGuard(_ action: (() -> Void)?) {
        browserTapAction = action
        guard let editorView else { return }
        if action == nil { browserTapGuard.removeFromSuperview(); return }
        if browserTapGuard.gestureRecognizers?.isEmpty != false { browserTapGuard.addGestureRecognizer(UITapGestureRecognizer(target: self,action: #selector(editorBrowserTap))) }
        browserTapGuard.accessibilityIdentifier = "fei.ipad.reading-surface"
        browserTapGuard.isAccessibilityElement = true; browserTapGuard.accessibilityLabel = navigationCoordinator?.localized("专注笔记","Focus note")
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
    var editorHasKeyboardFocus: Bool {
        func focused(_ view: UIView) -> Bool { view.isFirstResponder || view.subviews.contains(where: focused) }
        return editorView.map(focused) ?? false
    }

    func setFindImmersive(_ active: Bool) {
        guard findImmersive != active else { return }
        findImmersive = active
        guard let frameView = editorView,let parent = frameView.superview,!frameConstraints.isEmpty else { return }
        frameConstraints[0].isActive = false
        frameConstraints[0] = frameView.topAnchor.constraint(equalTo: active ? parent.topAnchor : toolbarAtTop ? editorToolbarContainer.bottomAnchor : parent.safeAreaLayoutGuide.topAnchor)
        frameConstraints[0].isActive = true
        parent.setNeedsLayout()
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

    @objc private func drawFrame() {
        guard UIApplication.shared.applicationState == .active else { return }
        editorView?.drawFrame()
        refreshEditorToolbarKeyboard()
    }
}

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
