import UIKit

struct FeiFindState: Decodable, Equatable {
    let serial: UInt64
    let generation: UInt64
    let query: String
    let replacement: String
    let replace: Bool
    let count: Int
    let current: Int?
    let can_replace: Bool
}

@MainActor
private final class FeiFindSession: UIFindSession {
    weak var coordinator: FeiFindCoordinator?
    var state: FeiFindState
    private var requestedQuery: String
    init(_ state: FeiFindState, coordinator: FeiFindCoordinator) { self.state = state; requestedQuery = state.query; self.coordinator = coordinator; super.init() }
    override var resultCount: Int { state.count }
    override var highlightedResultIndex: Int { state.current ?? NSNotFound }
    override var supportsReplacement: Bool { state.can_replace }
    override var allowsReplacementForCurrentlyHighlightedResult: Bool { state.can_replace && state.count > 0 && state.current != nil && state.query == requestedQuery }

    func send(_ action: String, query: String? = nil, replacement: String? = nil) {
        let fields: [String: Any] = ["query":query ?? requestedQuery,"replacement":replacement ?? coordinator?.interaction.replacementText ?? state.replacement,"generation":state.generation]
        guard let data = try? JSONSerialization.data(withJSONObject: fields),let value = String(data: data,encoding: .utf8),
              let command = try? JSONSerialization.data(withJSONObject: ["action":action,"selection_serial":state.serial,"value":value]),let json = String(data: command,encoding: .utf8) else { return }
        // UIKit callbacks may arrive while GPUI still holds its app lease.
        DispatchQueue.main.async { json.withCString { fei_ios_command($0) } }
    }
    override func performSearch(query: String, options: UITextSearchOptions?) { requestedQuery = query; send("find-query",query: query) }
    override func performSingleReplacement(query: String, replacementString: String, options: UITextSearchOptions?) {
        guard allowsReplacementForCurrentlyHighlightedResult,state.query == query else { return }
        send("find-replace",query: query,replacement: replacementString)
    }
    override func replaceAll(searchQuery: String, replacementString: String, options: UITextSearchOptions?) {
        guard allowsReplacementForCurrentlyHighlightedResult,state.query == searchQuery else { return }
        send("find-replace-all",query: searchQuery,replacement: replacementString)
    }
    override func highlightNextResult(in direction: UITextStorageDirection) { send(direction == .forward ? "find-next" : "find-previous") }
    override func invalidateFoundResults() {
        requestedQuery = coordinator?.interaction.searchText ?? ""
        send("find-query",query: requestedQuery)
    }
}

@MainActor
final class FeiFindCoordinator: NSObject, UIFindInteractionDelegate {
    private weak var view: UIView?
    private var session: FeiFindSession?
    // UIKit retains the same find session when navigator mode changes.
    private var transitioning: FeiFindSession?
    private var interactionSerial: UInt64?
    private var pending: FeiFindState?
    private var generation: UInt64 = 0
    private var navigationPresentation: (alpha: CGFloat, interactive: Bool, accessibility: Bool)?
    private(set) lazy var interaction = UIFindInteraction(sessionDelegate: self)
    init(view: UIView) {
        self.view = view
        super.init()
        configureInteraction()
    }
    private func configureInteraction() {
        interaction.optionsMenuProvider = { [weak self] _ in
            guard let self,let session = self.session else { return nil }
            let localized: (String,String) -> String = { EditorHost.active?.navigationCoordinator?.localized($0,$1) ?? $0 }
            var actions: [UIMenuElement] = [UIAction(title: localized("查找","Find"),state: session.state.replace ? .off : .on) { [weak session] _ in session?.send("find-mode-search") }]
            if session.supportsReplacement { actions.append(UIAction(title: localized("查找与替换","Find and replace"),state: session.state.replace ? .on : .off) { [weak session] _ in session?.send("find-mode-replace") }) }
            return UIMenu(children: actions)
        }
        view?.addInteraction(interaction)
    }
    func updateNavigation(for size: CGSize? = nil) {
        guard let navigation = EditorHost.active?.navigationController else { return }
        let bounds = size ?? navigation.view.bounds.size
        let compactFind = (session != nil || pending != nil) && bounds.width > bounds.height
        if compactFind {
            let bar = navigation.navigationBar
            if navigationPresentation == nil { navigationPresentation = (bar.alpha,bar.isUserInteractionEnabled,bar.accessibilityElementsHidden) }
            // Keep UIKit's bar and back item alive across find-mode transitions.
            bar.alpha = 0
            bar.isUserInteractionEnabled = false
            bar.accessibilityElementsHidden = true
            EditorHost.active?.setFindImmersive(true)
        } else if let previous = navigationPresentation {
            navigationPresentation = nil
            let bar = navigation.navigationBar
            bar.alpha = previous.alpha
            bar.isUserInteractionEnabled = previous.interactive
            bar.accessibilityElementsHidden = previous.accessibility
            EditorHost.active?.setFindImmersive(false)
        }
    }
    func enqueue(_ state: FeiFindState) {
        generation &+= 1
        let current = generation
        transitioning = session?.state.serial == state.serial ? session : nil
        pending = state
        DispatchQueue.main.async { [weak self] in
            guard let self,self.generation == current,self.pending != nil else { return }
            if self.session != nil { self.interaction.dismissFindNavigator() }
            else { self.presentPending() }
        }
    }
    private func presentPending() {
        let current = generation
        DispatchQueue.main.async { [weak self] in
            guard let self,self.generation == current,let state = self.pending,let view = self.view,view.window != nil else { return }
            self.pending = nil
            view.endEditing(true)
            if let previous = self.interactionSerial,previous != state.serial {
                view.removeInteraction(self.interaction)
                self.interaction = UIFindInteraction(sessionDelegate: self)
                self.configureInteraction()
            }
            self.interactionSerial = state.serial
            let next = self.transitioning ?? FeiFindSession(state,coordinator: self)
            self.transitioning = nil
            next.state = state
            self.session = next
            self.updateNavigation()
            self.interaction.searchText = state.query
            self.interaction.replacementText = state.replacement
            self.interaction.presentFindNavigator(showingReplace: state.replace && state.can_replace)
            self.interaction.updateResultCount()
        }
    }
    func update(_ state: FeiFindState) {
        if pending?.serial == state.serial { pending = state; return }
        guard let session,session.state.serial == state.serial,session.state != state else { return }
        if session.state.can_replace != state.can_replace { dismiss(); return }
        session.state = state
        interaction.updateResultCount()
    }
    func dismiss(serial: UInt64? = nil, notify: Bool = true) {
        guard serial == nil || session?.state.serial == serial || pending?.serial == serial else { return }
        generation &+= 1
        pending = nil
        transitioning = nil
        let previous = session
        session = nil
        DispatchQueue.main.async { [weak self] in
            guard let self,self.session == nil,self.pending == nil else { return }
            if self.interaction.isFindNavigatorVisible { self.interaction.dismissFindNavigator() }
            self.updateNavigation()
        }
        if notify { previous?.send("find-close") }
    }
    func findInteraction(_ interaction: UIFindInteraction, sessionFor view: UIView) -> UIFindSession? {
        return session
    }
    func findInteraction(_ interaction: UIFindInteraction, didEnd session: UIFindSession) {
        guard session === self.session else { return }
        let previous = self.session
        self.session = nil
        if pending != nil { presentPending() } else { updateNavigation(); previous?.send("find-close") }
    }
}

@_cdecl("fei_ios_present_find")
@MainActor
func feiIOSPresentFind(_ json: UnsafePointer<CChar>) {
    guard let state = try? JSONDecoder().decode(FeiFindState.self,from: Data(String(cString: json).utf8)) else { return }
    EditorHost.active?.findCoordinator?.enqueue(state)
}
@_cdecl("fei_ios_update_find")
@MainActor
func feiIOSUpdateFind(_ json: UnsafePointer<CChar>) {
    guard let state = try? JSONDecoder().decode(FeiFindState.self,from: Data(String(cString: json).utf8)) else { return }
    DispatchQueue.main.async { EditorHost.active?.findCoordinator?.update(state) }
}
@_cdecl("fei_ios_dismiss_find")
@MainActor
func feiIOSDismissFind(_ serial: UInt64) { EditorHost.active?.findCoordinator?.dismiss(serial: serial,notify: false) }
