import UIKit
import UIKit.UIGestureRecognizerSubclass
import CloudKit
import GameController
import Metal
import os

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
    private struct DisplayFrame {
        let timestamp: CFTimeInterval
        let target: CFTimeInterval
        let scrolling: Bool
        let maximumRate: Float
        let minimumRate: Float
        let preferredRate: Float
        let scrollOffset: Double?
    }

    private struct ScrollCadence {
        private(set) var limit: Float?
        private var fastFrames = 0
        private var overruns = 0
        private var headroomSince: CFTimeInterval?
        private var retriedMaximum = false

        mutating func record(_ frame: DisplayFrame, elapsed: CFTimeInterval,
                             missedDeadline: Bool, drawableDeferred: Bool) {
            // A deferred drawable supplies no timing sample for GPUI work.
            guard frame.scrolling, frame.maximumRate > 80, !drawableDeferred else { return }
            let interval = frame.target - frame.timestamp
            let requested = frame.preferredRate
            if requested > 60, interval > 0, interval < 0.015 {
                fastFrames += 1
                if missedDeadline || elapsed > interval * 1.15 { overruns += 1 }
                if fastFrames == 12 {
                    let overloaded = overruns >= 4
                    fastFrames = 0
                    overruns = 0
                    if overloaded {
                        // If the system was already delivering 80 Hz, step down to 60 Hz.
                        limit = requested > 80 && interval < 0.011 ? 80 : 60
                        headroomSince = nil
                        return
                    }
                }
            } else {
                fastFrames = 0
                overruns = 0
            }

            // Recover one step after sustained headroom. Retry the fastest rate once
            // per scroll sequence to avoid switching back and forth under load.
            let nextRate: Float = requested < 80 ? 80 : frame.maximumRate
            guard requested < frame.maximumRate,
                  (requested < 80 || !retriedMaximum),
                  !missedDeadline, elapsed < 0.75 / Double(nextRate) else {
                headroomSince = nil
                return
            }
            let since = headroomSince ?? frame.timestamp
            headroomSince = since
            if frame.timestamp - since >= 1.0 {
                if nextRate == frame.maximumRate { retriedMaximum = true }
                limit = nextRate < frame.maximumRate ? nextRate : nil
                headroomSince = nil
                fastFrames = 0
                overruns = 0
            }
        }
    }

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
    #if FEI_MOBILE_QA
    private let liveQAProbe = UIView(frame:CGRect(x:0,y:0,width:1,height:1))
    #endif
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
    private var scrollUpdateLink: AnyObject?
    private var pendingScrollFrame: DisplayFrame?
    private var scrollUpdateMissed = false
    private var lastScrollActivity: CFTimeInterval = 0
    private var scrollCadence = ScrollCadence()
    #if FEI_LOCAL_PREVIEW || FEI_MOBILE_QA
    private var scrollFrames: [[Double]] = []
    private var recentScrollFrames: [[Double]] = []
    private let scrollLogQueue = DispatchQueue(label: "app.feinotes.scroll-log", qos: .utility)
    #endif
    private let browserTapGuard = UIView()
    private var browserTapAction: (() -> Void)?

    func stopForTermination() {
        findCoordinator?.dismiss(notify: false)
        displayLink?.invalidate()
        displayLink = nil
        if #available(iOS 18.0, *) { (scrollUpdateLink as? UIUpdateLink)?.isEnabled = false }
        scrollUpdateLink = nil
        pendingScrollFrame = nil
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
        attachEditor(to: self)
        view.setNeedsLayout()
        startDisplayLink()
    }

    func attachEditor(to parent: FeiRouteController) {
        guard let frameView = editorView else { return }
        parent.loadViewIfNeeded()
        parent.thawContent()
        frameView.bindHorizontalScrollGuard(to: parent.navigationController)
        prepareEditorTransition(to: parent)
        frameView.isHidden = parent.hasNativeContent || !(parent.routeKey.hasPrefix("note:") || parent.routeKey.hasPrefix("history-preview"))
        frameView.setScrollRoute(frameView.isHidden ? nil : parent.routeKey)
        frameView.backgroundColor = parent.view.backgroundColor
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
            var probe = qa.scroll.data(using:.utf8).flatMap { try? JSONSerialization.jsonObject(with:$0) as? [String:Any] } ?? [:]
            probe["qa_title"]=qa.title;probe["qa_body"]=qa.body
            if ProcessInfo.processInfo.environment["FEI_LIVE_CLOUD_PROBE"] == "1" {
                editorView?.isAccessibilityElement=false
                liveQAProbe.isAccessibilityElement=true;liveQAProbe.isUserInteractionEnabled=false
                liveQAProbe.accessibilityIdentifier="fei.qa.editor-state"
                liveQAProbe.accessibilityValue=(try? JSONSerialization.data(withJSONObject:probe,options:[.sortedKeys])).flatMap { String(data:$0,encoding:.utf8) }
                if liveQAProbe.superview !== editorView { liveQAProbe.removeFromSuperview();editorView?.addSubview(liveQAProbe) }
            } else {
                editorView?.isAccessibilityElement=true
                if editorView?.horizontalScrollProbe == nil {
                    editorView?.accessibilityValue = ProcessInfo.processInfo.environment["FEI_QA_SHARED_BROWSER_REFRESH"] == "1"
                        ? qa.body : state.board_open ? qa.board : qa.scroll
                }
            }
            editorToolbar.accessibilityValue = "draft=\(qa.draft) gen=\(qa.generation) title=\(qa.title) body=\(qa.body) selection=\(qa.selection.anchor.block):\(qa.selection.anchor.offset)-\(qa.selection.head.block):\(qa.selection.head.offset) scroll=\(qa.scroll)"
        }
        #endif
        editorToolbarVisible = (state.kind == "note" || state.kind == "history-preview") && !state.find_active && !state.board_open && (editorView?.controller.parent as? FeiRouteController)?.hasNativeContent != true
        editorView?.backgroundColor = editorView?.superview?.backgroundColor
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

    func returnDocumentToTop(scope: String) {
        guard toolbarState?.editor_scope == scope,editorView?.isHidden == false else { return }
        editorView?.returnDocumentToTop()
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
        guard let editorView, displayLink == nil else { return }
        if #available(iOS 18.0, *) {
            #if FEI_LOCAL_PREVIEW || FEI_MOBILE_QA
            if ProcessInfo.processInfo.environment["FEI_SCROLL_USE_DISPLAY_LINK"] != "1" {
                attachScrollUpdateLink(to: editorView)
            }
            #else
            attachScrollUpdateLink(to: editorView)
            #endif
        }
        let link = CADisplayLink(target: self, selector: #selector(displayFrame(_:)))
        link.add(to: .main, forMode: .common)
        displayLink = link
    }

    @available(iOS 18.0, *)
    private func attachScrollUpdateLink(to editorView: EditorFrameView) {
        let link = UIUpdateLink(view: editorView)
        link.addAction(to: .afterCADisplayLinkDispatch) { [weak self] _, info in
            guard let self, let frame = pendingScrollFrame else { return }
            pendingScrollFrame = nil
            renderDisplayFrame(frame, deadline: info.completionDeadlineTime)
        }
        link.isEnabled = true
        scrollUpdateLink = link
    }

    func refreshSystemAppearance() {
        let dark = feiIOSSystemDark()
        guard lastSystemDark != dark,editorView != nil else { return }
        lastSystemDark = dark
        navigationCoordinator?.send("system-appearance")
    }

    @objc private func displayFrame(_ link: CADisplayLink) {
        let maximum = Float(editorView?.window?.screen.maximumFramesPerSecond ?? view.window?.screen.maximumFramesPerSecond ?? 60)
        let scrolling = editorView?.isScrolling == true
        let recentScroll = link.timestamp - lastScrollActivity < 0.25
        if !recentScroll {
            scrollUpdateMissed = false
            scrollCadence = ScrollCadence()
        }
        if scrolling { lastScrollActivity = link.timestamp }
        let activeScroll = scrolling || recentScroll
        let limitedRate = scrollCadence.limit.map { min($0, maximum) }
        let preferred = activeScroll ? (limitedRate ?? maximum) : min(60, maximum)
        let upperRate = limitedRate ?? maximum
        // A broad range allows the system to spend part of an active scroll
        // at 40 Hz, even while the document keeps up. Request a tighter range
        // while moving, including a steady 80 or 60 Hz request in fallback.
        let lowerRate = activeScroll ? min(80, preferred) : min(30, upperRate)
        let range = link.preferredFrameRateRange
        let requestedRange = CAFrameRateRange(minimum: lowerRate, maximum: upperRate, preferred: preferred)
        if range != requestedRange {
            link.preferredFrameRateRange = requestedRange
        }
        #if FEI_LOCAL_PREVIEW || FEI_MOBILE_QA
        let offset = editorView?.diagnosticScrollOffsets?.native
        #else
        let offset: Double? = nil
        #endif
        let frame = DisplayFrame(timestamp: link.timestamp, target: link.targetTimestamp, scrolling: scrolling,
                                 maximumRate: maximum, minimumRate: lowerRate, preferredRate: preferred, scrollOffset: offset)
        if pendingScrollFrame != nil {
            pendingScrollFrame = nil
            scrollUpdateMissed = true
        }
        if scrolling, !scrollUpdateMissed, scrollUpdateLink != nil, editorView?.window != nil {
            // UIKit can advance a scroll in other display-link handlers.
            // Take the editor's one sample after those handlers have run.
            pendingScrollFrame = frame
        } else {
            renderDisplayFrame(frame)
        }
    }

    private func renderDisplayFrame(_ frame: DisplayFrame, deadline: CFTimeInterval? = nil) {
        let before = CACurrentMediaTime()
        let deferred = drawFrame(displayInterval: frame.target - frame.timestamp)
        let finished = CACurrentMediaTime()
        let elapsed = finished - before
        scrollCadence.record(frame, elapsed: elapsed,
                             missedDeadline: deadline.map { finished > $0 } ?? false,
                             drawableDeferred: deferred)
        #if FEI_LOCAL_PREVIEW || FEI_MOBILE_QA
        if let offset = editorView?.diagnosticScrollOffsets {
            scrollFrames.append([frame.timestamp,frame.target,elapsed*1000,offset.native,offset.forwarded,offset.gpui,
                                 Double(frame.minimumRate),Double(frame.preferredRate),frame.scrollOffset ?? offset.native,
                                 finished,deadline ?? 0,deferred ? 1 : 0,editorView?.lastDrawableAcquisition ?? 0])
            if scrollFrames.count >= 1200 { saveScrollFrames() }
        } else if !scrollFrames.isEmpty {
            saveScrollFrames()
        }
        #endif
    }

    #if FEI_LOCAL_PREVIEW || FEI_MOBILE_QA
    private func saveScrollFrames() {
        let segment = scrollFrames
        scrollFrames.removeAll(keepingCapacity: true)
        guard let first = segment.first, segment.contains(where: { $0[3] != first[3] }) else { return }
        if let previous = recentScrollFrames.last, first[0] - previous[0] > 30 {
            recentScrollFrames.removeAll(keepingCapacity: true)
        }
        recentScrollFrames.append(contentsOf: segment)
        if recentScrollFrames.count > 2400 {
            recentScrollFrames.removeFirst(recentScrollFrames.count - 2400)
        }
        let samples = recentScrollFrames
        scrollLogQueue.async {
            guard let directory = FileManager.default.urls(for:.cachesDirectory,in:.userDomainMask).first,
                  let data = try? JSONSerialization.data(withJSONObject:samples) else { return }
            try? data.write(to:directory.appendingPathComponent("FeiScrollFrames.json"),options:.atomic)
        }
    }
    #endif

    @discardableResult
    private func drawFrame(displayInterval: CFTimeInterval? = nil) -> Bool {
        guard UIApplication.shared.applicationState == .active else { return false }
        // Layout and keyboard notifications still drive chrome updates while
        // the document pans. Keep the display callback for rendering itself.
        if editorView?.isScrolling != true {
            refreshSystemAppearance()
            refreshEditorToolbarKeyboard()
        }
        let insets = editorChromeInsets
        if lastEditorInsets != insets {
            lastEditorInsets = insets
            navigationCoordinator?.send("editor-chrome-layout")
        }
        return editorView?.drawFrame(displayInterval: displayInterval) ?? false
    }
}

private final class DrawableBackpressure: @unchecked Sendable {
    private let lock = NSLock()
    private var nextID: UInt64 = 0
    private var pending: [UInt64: CFTimeInterval] = [:]
    private var reserveUntil: CFTimeInterval = 0

    func hasRoom(drawableCount: Int, displayInterval: CFTimeInterval, now: CFTimeInterval) -> Bool {
        lock.withLock {
            // Two outstanding presentations are normal at 60 Hz. At faster
            // cadences or after a measured wait, leave a slot for the display.
            let reserve = displayInterval < 0.015 || now < reserveUntil
            let limit = max(1, drawableCount - (reserve ? 1 : 0))
            guard pending.count >= limit else { return true }
            // Surface reconfiguration or a failed frame can discard a drawable
            // before it is presented. Permit a retry after a brief stall.
            pending = pending.filter { now - $0.value < 0.10 }
            return pending.count < limit
        }
    }

    func didAcquire(at time: CFTimeInterval, waited: CFTimeInterval) -> UInt64 {
        lock.withLock {
            if waited > 0.002 { reserveUntil = time + 0.25 }
            nextID &+= 1
            pending[nextID] = time
            return nextID
        }
    }

    func didPresent(_ id: UInt64) {
        _ = lock.withLock { pending.removeValue(forKey: id) }
    }
}

@objc(FeiEditorMetalLayer)
final class FeiEditorMetalLayer: CAMetalLayer {
    private let backpressure = DrawableBackpressure()
    var usesLegacyPhoneScroll = false
    #if FEI_LOCAL_PREVIEW || FEI_MOBILE_QA
    var lastAcquireMilliseconds = 0.0
    #endif

    func canDrawWithoutQueueing(displayInterval: CFTimeInterval) -> Bool {
        backpressure.hasRoom(drawableCount: maximumDrawableCount, displayInterval: displayInterval,
                             now: CACurrentMediaTime())
    }

    override func nextDrawable() -> (any CAMetalDrawable)? {
        if usesLegacyPhoneScroll { return super.nextDrawable() }
        let signpost = editorFrameLog.signpostsEnabled ? OSSignpostID(log: editorFrameLog) : nil
        if let signpost {
            os_signpost(.begin,log: editorFrameLog,name: "Metal Acquire Drawable",signpostID: signpost)
        }
        let start = CACurrentMediaTime()
        let drawable = super.nextDrawable()
        let now = CACurrentMediaTime()
        if let signpost {
            os_signpost(.end,log: editorFrameLog,name: "Metal Acquire Drawable",signpostID: signpost)
        }
        #if FEI_LOCAL_PREVIEW || FEI_MOBILE_QA
        lastAcquireMilliseconds = (now - start) * 1000
        #endif
        #if !targetEnvironment(simulator)
        if let drawable {
            let state = backpressure
            let id = state.didAcquire(at: now, waited: now - start)
            drawable.addPresentedHandler { _ in state.didPresent(id) }
        }
        #endif
        return drawable
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

private let editorFrameLog = OSLog(subsystem: "app.feinotes",category: "EditorPerformance")

private final class EditorFrameView: UIView {
    private let documentScroll: FeiDocumentScrollView
    let gpuiWindow: UnsafeMutableRawPointer
    let controller: UIViewController
    private weak var guardedNavigation: UINavigationController?
    private var horizontalScrollGuard: FeiHorizontalScrollGuard?

    func bindHorizontalScrollGuard(to navigation: UINavigationController?) {
        guard guardedNavigation !== navigation else { return }
        if let previous = horizontalScrollGuard { previous.view?.removeGestureRecognizer(previous) }
        horizontalScrollGuard = nil
        guardedNavigation = navigation
        guard let navigation else { return }

        let guardGesture = FeiHorizontalScrollGuard(editor: self)
        horizontalScrollGuard = guardGesture
        navigation.view.addGestureRecognizer(guardGesture)
        navigation.interactivePopGestureRecognizer?.require(toFail: guardGesture)
        if #available(iOS 26.0, *) {
            navigation.interactiveContentPopGestureRecognizer?.require(toFail: guardGesture)
        }
    }

    init(window: UnsafeMutableRawPointer, controller: UIViewController) {
        self.gpuiWindow = window
        self.controller = controller
        self.documentScroll = FeiDocumentScrollView(canvas: controller.view)
        super.init(frame: .zero)
        clipsToBounds = true
        accessibilityIdentifier = "fei.editor.surface"
        metalLayer?.usesLegacyPhoneScroll = UIDevice.current.userInterfaceIdiom == .phone
        addSubview(documentScroll)
    }

    func setScrollRoute(_ route: String?) { documentScroll.setRoute(route) }

    var isScrolling: Bool { documentScroll.scrollInProgress }
    var metalLayer: FeiEditorMetalLayer? { controller.view.layer as? FeiEditorMetalLayer }

    func returnDocumentToTop() { documentScroll.returnToTop() }

    #if FEI_LOCAL_PREVIEW || FEI_MOBILE_QA
    var diagnosticScrollOffsets: (native: Double, forwarded: Double, gpui: Double)? {
        documentScroll.diagnosticScrollOffsets
    }
    private(set) var lastDrawableAcquisition = 0.0
    #endif

    required init?(coder: NSCoder) { fatalError("init(coder:) is unsupported") }

    override func layoutSubviews() {
        super.layoutSubviews()
        guard bounds.width > 0, bounds.height > 0 else { return }
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        documentScroll.frame = bounds
        documentScroll.layoutCanvas()
        gpui_ios_layout_view(gpuiWindow)
        drawFrame()
        CATransaction.commit()
    }

    #if FEI_MOBILE_QA
    let horizontalScrollProbe: UInt8? = {
        switch ProcessInfo.processInfo.environment["FEI_QA_HORIZONTAL_SCROLL"] {
        case "table": return 0
        case "code": return 1
        default: return nil
        }
    }()
    #endif

    @discardableResult
    func drawFrame(displayInterval: CFTimeInterval? = nil) -> Bool {
        // Layout can also request a frame. While UIKit is moving, use the
        // display link for both the scroll sample and the render submission.
        guard displayInterval != nil || !isScrolling else { return false }
        let insets = EditorHost.active?.editorChromeInsets ?? .zero
        if documentScroll.verticalScrollIndicatorInsets != insets {
            documentScroll.verticalScrollIndicatorInsets = insets
        }
        let metalLayer = metalLayer
        #if FEI_LOCAL_PREVIEW || FEI_MOBILE_QA
        lastDrawableAcquisition = 0
        metalLayer?.lastAcquireMilliseconds = 0
        #endif
        if let displayInterval {
            if let metalLayer, !metalLayer.usesLegacyPhoneScroll,
               !metalLayer.canDrawWithoutQueueing(displayInterval: displayInterval) {
                os_signpost(.event,log: editorFrameLog,name: "Metal Drawable Deferred")
                return true
            }
            documentScroll.applyScrollForFrame()
        }
        if editorFrameLog.signpostsEnabled {
            let signpost = OSSignpostID(log: editorFrameLog)
            os_signpost(.begin,log: editorFrameLog,name: "GPUI Frame",signpostID: signpost)
            _ = gpui_ios_request_frame(gpuiWindow)
            os_signpost(.end,log: editorFrameLog,name: "GPUI Frame",signpostID: signpost)
        } else {
            _ = gpui_ios_request_frame(gpuiWindow)
        }
        #if FEI_LOCAL_PREVIEW || FEI_MOBILE_QA
        lastDrawableAcquisition = metalLayer?.lastAcquireMilliseconds ?? 0
        #endif
        documentScroll.synchronizeLayout()
        #if FEI_MOBILE_QA
        if let horizontalScrollProbe, !isHidden {
            var values = [Float](repeating: 0,count: 5)
            let value = values.withUnsafeMutableBufferPointer { buffer in
                fei_ios_qa_horizontal_scroll(horizontalScrollProbe, buffer.baseAddress)
                    ? "horizontal:" + buffer.map { String($0) }.joined(separator: ",") : ""
            }
            if accessibilityValue != value { accessibilityValue = value }
        }
        #endif
        return false
    }
}

// Give the editor the original touch stream whenever a drag starts in a horizontally
// scrollable editor block. UIKit keeps its usual pop behavior elsewhere.
private final class FeiHorizontalScrollGuard: UIGestureRecognizer, UIGestureRecognizerDelegate {
    private weak var editor: EditorFrameView?

    init(editor: EditorFrameView) {
        self.editor = editor
        super.init(target: nil, action: nil)
        delegate = self
        cancelsTouchesInView = false
        delaysTouchesBegan = false
        delaysTouchesEnded = false
        allowedTouchTypes = [NSNumber(value: UITouch.TouchType.direct.rawValue)]
    }

    func gestureRecognizer(_ gestureRecognizer: UIGestureRecognizer, shouldReceive touch: UITouch) -> Bool {
        guard let editor, !editor.isHidden,
              let touched = touch.view, touched === editor.controller.view || touched.isDescendant(of: editor.controller.view)
        else { return false }
        let p = touch.location(in: editor.controller.view)
        return fei_ios_horizontal_scroll_at(Float(p.x), Float(p.y))
    }

    override func touchesBegan(_ touches: Set<UITouch>, with event: UIEvent) { state = .recognized }

    override func canPrevent(_ preventedGestureRecognizer: UIGestureRecognizer) -> Bool { false }
    override func canBePrevented(by preventingGestureRecognizer: UIGestureRecognizer) -> Bool { false }

    func gestureRecognizer(_ gestureRecognizer: UIGestureRecognizer, shouldBeRequiredToFailBy other: UIGestureRecognizer) -> Bool {
        // The split-view sidebar may also own an edge swipe when hidden.
        guard let split = editor?.controller.splitViewController else { return false }
        return other is UIScreenEdgePanGestureRecognizer && other.view === split.viewIfLoaded
    }
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
