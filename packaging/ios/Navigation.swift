import UIKit

struct FeiNavigationState: Decodable, Equatable, Sendable {
    struct Listing: Decodable, Equatable, Sendable {
        struct Row: Decodable, Equatable, Sendable { let id:String; let title:String; let summary:String; let folder:Bool; let pinned:Bool }
        struct Selection: Decodable,Equatable,Sendable { let serial:UInt64; let ids:[String]; let selected:[String]; let limit:Int; let can_move:Bool; let moving:Bool }
        let kind:String; let can_reorder:Bool; let shared_count:Int; let selection:Selection?; let notice_kind:String?
        let interface_size:String; let notice:String?; let scope:String; let query:String; let tab:Int; let tabs:Bool; let empty:String; let rows:[Row]
    }
    struct Browser: Decodable,Equatable,Sendable { let folder:String?; let favorites:Bool; let shared:Bool?; let title:String; let listing:Listing }
    struct QA: Decodable,Equatable,Sendable {
        struct Pos: Decodable,Equatable,Sendable { let block:UInt64; let offset:Int }
        struct Selection: Decodable,Equatable,Sendable { let anchor:Pos; let head:Pos }
        let draft:Bool; let generation:UInt64; let title:String; let body:String; let selection:Selection; let scroll:String
    }
    let qa: QA?
    let browser: Browser?
    let settings: FeiSettingsState?
    let local_history: LocalHistory?
    let listing: Listing?
    let editor_scope:String
    let reading:Bool
    let write_allowed:Bool
    let find_active:Bool
    struct Route: Decodable, Equatable, Sendable {
        let key: String
        let title: String
    }
    struct Item: Decodable, Equatable, Sendable {
        let id: String
        let title: String
        let parent: String?
        let folder: Bool
        let pinned: Bool
        let starred: Bool
        let trashed: Bool
        let shared: Bool
        let owned: Bool
    }
    struct Confirmation: Decodable, Equatable, Sendable {
        struct Version: Decodable, Equatable, Sendable { let sequence: Int64; let time: String; let preview: String }
        let versions: [Version]?
        let selected_version: Int64?
        let serial: UInt64
        let title: String
        let message: String
        let action: String
        let danger: Bool
        let choice: String?
        let choice_on: Bool
        let save_failed: Bool
    }
    struct Reorder: Decodable, Equatable, Sendable {
        struct Item: Decodable, Equatable, Sendable { let id:String; let title:String; let folder:Bool; let pinned:Bool }
        let serial:UInt64; let response:UInt64; let title:String; let prompt:String; let done:String; let saving:Bool; let items:[Item]
    }
    struct Information: Decodable, Equatable, Sendable { let serial:UInt64; let note:String; let title:String; let message:String; let done:String }
    let information: Information?
    let can_reorder: Bool
    let reorder: Reorder?
    let revision: UInt64
    let confirmation: Confirmation?
    let routes: [Route]
    let items: [Item]
    let current: String?
    let busy: Bool
    let cloud_history: Bool
    let cloud_start_pending: Bool
    let cloud_active: Bool
    let shared_session: Bool
    let can_create: Bool
    let can_edit: Bool
    let table_active: Bool
    let has_code: Bool
    let appearance: String
    let theme: FeiMobileTheme?
    let creation_ack: UInt64?
    let kind: String
    let title: String
    let labels: [String: String]
}

@MainActor
private final class FeiBrowserRootView: UIView {
    override func point(inside point: CGPoint,with event: UIEvent?) -> Bool {
        // Triple-column UIKit extends supplementary content beneath primary.
        // Its transparent leading extension must not intercept sidebar taps.
        point.x >= safeAreaInsets.left && super.point(inside: point,with: event)
    }
}

@MainActor
class FeiRouteController: UIViewController {
    override func loadView() { view = isWorkspaceBrowser ? FeiBrowserRootView() : UIView() }
    var isWorkspaceBrowser = false
    var routeKey = "notes"
    var frozenContent: UIView?
    var notesList: FeiNotesListController?
    var historyList: FeiLocalHistoryController?
    var settingsList: FeiSettingsController?
    struct BarIdentity: Equatable { let kind:String; let shared:Bool; let editing:Bool; let typing:Bool; let expanded:Bool; let labels:[String:String] }
    var barIdentity: BarIdentity?
    var hasNativeContent: Bool { settingsList?.view.isHidden == false || notesList?.view.isHidden == false || historyList?.view.isHidden == false }

    func updateListing(_ listing: FeiNavigationState.Listing?,labels: [String:String],busy: Bool,coordinator: FeiNavigationCoordinator) {
        guard let listing else { notesList?.deactivate(); return }
        if notesList == nil {
            let list = FeiNotesListController(coordinator: coordinator)
            list.browsesWorkspace = isWorkspaceBrowser
            addChild(list); list.view.translatesAutoresizingMaskIntoConstraints = false; view.addSubview(list.view)
            NSLayoutConstraint.activate([list.view.topAnchor.constraint(equalTo: view.topAnchor),list.view.bottomAnchor.constraint(equalTo: view.bottomAnchor),list.view.leadingAnchor.constraint(equalTo: isWorkspaceBrowser ? view.safeAreaLayoutGuide.leadingAnchor : view.leadingAnchor),list.view.trailingAnchor.constraint(equalTo: isWorkspaceBrowser ? view.safeAreaLayoutGuide.trailingAnchor : view.trailingAnchor)])
            list.didMove(toParent: self); notesList = list
        }
        notesList?.view.isHidden = false
        notesList?.update(listing,labels: labels,busy: busy)
        setContentScrollView(notesList?.tableView,for: .all)
    }

    func updateSettings(_ settings:FeiSettingsState?,busy:Bool,coordinator:FeiNavigationCoordinator) {
        guard let settings else { settingsList?.deactivate(); return }
        if settingsList == nil {
            let list = FeiSettingsController(coordinator: coordinator)
            addChild(list); list.view.translatesAutoresizingMaskIntoConstraints = false; view.addSubview(list.view)
            NSLayoutConstraint.activate([list.view.topAnchor.constraint(equalTo: view.topAnchor),list.view.bottomAnchor.constraint(equalTo: view.bottomAnchor),list.view.leadingAnchor.constraint(equalTo: view.leadingAnchor),list.view.trailingAnchor.constraint(equalTo: view.trailingAnchor)])
            list.didMove(toParent: self); settingsList = list
        }
        settingsList?.view.isHidden = false; settingsList?.update(settings,busy: busy)
    }

    func updateHistory(_ history: FeiNavigationState.LocalHistory?,labels:[String:String],busy:Bool,coordinator: FeiNavigationCoordinator) {
        guard let history else { historyList?.deactivate(); return }
        if historyList == nil {
            let list = FeiLocalHistoryController(coordinator: coordinator)
            addChild(list); list.view.translatesAutoresizingMaskIntoConstraints = false; view.addSubview(list.view)
            NSLayoutConstraint.activate([list.view.topAnchor.constraint(equalTo: view.topAnchor),list.view.bottomAnchor.constraint(equalTo: view.bottomAnchor),list.view.leadingAnchor.constraint(equalTo: view.leadingAnchor),list.view.trailingAnchor.constraint(equalTo: view.trailingAnchor)])
            list.didMove(toParent: self); historyList = list
        }
        historyList?.view.isHidden = false; historyList?.update(history,labels: labels,busy: busy)
    }

    override func viewDidLoad() {
        super.viewDidLoad()
        view.backgroundColor = .systemBackground
    }

    override func viewDidLayoutSubviews() {
        super.viewDidLayoutSubviews()
        EditorHost.active?.refreshEditorToolbarKeyboard()
        EditorHost.active?.refreshEditorLayout()
        EditorHost.active?.navigationCoordinator?.refreshLayoutControls()
    }

    override func viewWillTransition(to size: CGSize, with coordinator: UIViewControllerTransitionCoordinator) {
        super.viewWillTransition(to: size,with: coordinator)
        coordinator.animate(alongsideTransition: { _ in EditorHost.active?.findCoordinator?.updateNavigation(for: size) })
    }

    func freezeContent() {
        frozenContent?.removeFromSuperview()
        guard let image = view.snapshotView(afterScreenUpdates: false) else { return }
        image.frame = view.bounds
        image.autoresizingMask = [.flexibleWidth, .flexibleHeight]
        image.isUserInteractionEnabled = false
        frozenContent = image
        view.addSubview(image)
    }

    func thawContent() {
        frozenContent?.removeFromSuperview()
        frozenContent = nil
    }
}

@MainActor
final class FeiNavigationCoordinator: NSObject, UINavigationControllerDelegate {
    weak var host: EditorHost?
    private var state: FeiNavigationState?
    var cloudStartPending: Bool { state?.cloud_start_pending == true }
    var readyForCloudResume: Bool {
        state?.kind == "notes" && state?.listing?.kind != "shared" && !applying && pendingCreation == nil
            && (host?.navigationController?.topViewController as? FeiRouteController)?.notesList?.allowsCloudResume != false
            && host?.padWorkspace?.allowsCloudResume != false
            && host?.navigationController?.topViewController?.presentedViewController == nil
    }
    var readyForInvitation: Bool { state?.cloud_start_pending == false && state?.busy == false && !applying }
    private var pendingDrainScheduled = false
    private var pending: FeiNavigationState?
    private var applying = false
    private var nativePopTarget: String?
    private var nativePopSource: String?
    private weak var buttonsController: UIViewController?
    private var buttonsKind: String?
    private var buttonsShared = false
    private var buttonsEditing = false
    private var buttonsTyping = false
    private var buttonsExpanded = false
    private var buttonsLabels: [String: String] = [:]
    var theme: FeiMobileTheme { state?.theme ?? .system }
    var hasPadWorkspace: Bool { host?.padWorkspace != nil }
    var browserTitle: String { state?.browser?.title ?? "" }
    var listingTitle: String { state?.routes.last?.title ?? state?.title ?? "" }
    func captureEditorInputReturn() -> (() -> Void)? {
        guard let state,state.kind == "note",state.can_edit,state.write_allowed,!state.busy,host?.editorHasKeyboardFocus == true else { return nil }
        let id = state.current,scope = state.editor_scope
        return { [weak self] in
            guard let self,let next = self.state,next.kind == "note",next.current == id,next.editor_scope == scope,next.can_edit,next.write_allowed,!next.busy,
                  self.host?.navigationController?.topViewController?.presentedViewController == nil else { return }
            self.send("toolbar-command",value: "keyboard",id: id,scope: scope)
        }
    }
    private var createSerial: UInt64 = 0
    private var pendingCreation: UInt64?
    var allowsCompose: Bool { state?.busy == false && state?.can_create == true && state?.listing?.kind != "shared" && state?.browser?.shared != true && state?.confirmation == nil && pendingCreation == nil }
    func composeNote(listingScope: String? = nil) {
        guard allowsCompose,!applying,host?.navigationController?.transitionCoordinator == nil,let state else { return }
        if let listingScope,(hasPadWorkspace ? state.browser?.listing.scope : state.listing?.scope) != listingScope { return }
        createSerial = max(createSerial,state.revision)+1; pendingCreation = createSerial
        send(hasPadWorkspace ? "browse-compose" : "compose",id: state.current,serial: createSerial,scope: hasPadWorkspace ? state.browser?.listing.scope : state.listing?.scope ?? state.editor_scope)
    }
    private weak var confirmationAlert: UIAlertController?
    private var informationGeneration: UInt64 = 0
    private var informationSerial: UInt64?
    private weak var informationAlert: UIAlertController?
    private var reorderGeneration: UInt64 = 0
    private weak var reorderController: FeiReorderController?
    private weak var versionHistory: FeiVersionHistoryController?
    private var confirmationGeneration: UInt64 = 0
    private var presentationGeneration: UInt64 = 0
    private var focusAfterTransition = false
    private var sheetAnchorID: String?

    init(host: EditorHost) {
        self.host = host
        super.init()
        host.navigationController?.delegate = self
    }

    func receive(_ next: FeiNavigationState) {

        guard next.revision > (state?.revision ?? 0), next.revision > (pending?.revision ?? 0),
              let host, let navigation = host.navigationController else { return }
        if navigation.transitionCoordinator != nil {
            pending = next
            drainPendingAfterTransition()
            return
        }
        let routes = host.padWorkspace?.detailRoutes(next) ?? next.routes
        let oldKeys = navigation.viewControllers.compactMap { ($0 as? FeiRouteController)?.routeKey }
        let newKeys = routes.map(\.key)
        if nativePopTarget != nil && newKeys.last != nativePopSource { nativePopTarget=nil; nativePopSource=nil }
        let oldState = state
        if let pendingCreation,let ack = next.creation_ack,ack >= pendingCreation {
            self.pendingCreation = nil
            if next.current == oldState?.current && next.kind != "note" && next.confirmation == nil { focusAfterTransition = false }
        }
        if oldState?.confirmation != nil && next.confirmation == nil && next.current == oldState?.current { focusAfterTransition = false }
        state = next
        host.padWorkspace?.update(next,coordinator: self)
        if next.kind != "note" || (oldState != nil && next.current != oldState?.current) { host.findCoordinator?.dismiss() }
        if oldKeys != newKeys {
            presentationGeneration &+= 1
            let existing = Dictionary(uniqueKeysWithValues: navigation.viewControllers.compactMap { controller -> (String, FeiRouteController)? in
                guard let controller = controller as? FeiRouteController else { return nil }
                return (controller.routeKey, controller)
            })
            let controllers = routes.enumerated().map { index, route -> FeiRouteController in
                let materialized = oldKeys.last == "note:draft" && route.key.hasPrefix("note:") ? navigation.topViewController as? FeiRouteController : nil
                let controller = index == 0 ? host : existing[route.key] ?? materialized ?? FeiRouteController()
                controller.routeKey = route.key
                controller.title = route.title
                controller.navigationItem.backButtonTitle = next.labels["back"]
                controller.navigationItem.backButtonDisplayMode = .minimal
                if index == routes.count-1 { controller.updateListing(host.padWorkspace == nil ? next.listing : nil,labels: next.labels,busy: next.busy,coordinator: self); controller.updateHistory(next.kind == "history" ? next.local_history : nil,labels: next.labels,busy: next.busy,coordinator: self) }
                if index == routes.count-1 { controller.updateSettings(next.settings,busy: next.busy,coordinator: self) }
                return controller
            }
            if host.padWorkspace == nil { (navigation.topViewController as? FeiRouteController)?.freezeContent() }
            if let target = controllers.last { host.attachEditor(to: target) }
            let replacesControllers = navigation.viewControllers.map(ObjectIdentifier.init) != controllers.map(ObjectIdentifier.init)
            if replacesControllers {
                applying = true
                navigation.setViewControllers(controllers, animated: host.padWorkspace == nil && oldState != nil && !UIAccessibility.isReduceMotionEnabled)
                if oldState == nil || host.padWorkspace != nil { applying = false }
            } else {
                // A draft acquired its persistent ID but retained the same native
                // controllers. UIKit will not send didShow for an unchanged stack.
                applying = false
            }
            if host.padWorkspace != nil && focusAfterTransition && next.kind == "note" {
                focusAfterTransition = false
                DispatchQueue.main.async { [weak host] in host?.prepareEditorInput() }
            }
        }
        for (controller, route) in zip(navigation.viewControllers, routes) {
            if controller.title != route.title { controller.title = route.title }
            if controller.navigationItem.backButtonTitle != next.labels["back"] { controller.navigationItem.backButtonTitle = next.labels["back"] }
        }
        if let top = navigation.viewControllers.compactMap({ $0 as? FeiRouteController }).last {
            top.updateSettings(next.settings,busy: next.busy,coordinator: self)
            top.updateListing(host.padWorkspace == nil || next.kind == "trash" ? next.listing : nil,labels: next.labels,busy: next.busy,coordinator: self)
            top.updateHistory(next.kind == "history" ? next.local_history : nil,labels: next.labels,busy: next.busy,coordinator: self)
            host.attachEditor(to: top)
        }
        let appearance: UIUserInterfaceStyle = next.appearance == "dark" ? .dark : next.appearance == "light" ? .light : .unspecified
        if navigation.overrideUserInterfaceStyle != appearance { navigation.overrideUserInterfaceStyle = appearance }
        let palette = next.theme ?? .system
        if oldState?.theme != next.theme {
            navigation.view.tintColor = palette.accentColor
            navigation.view.window?.tintColor = palette.accentColor
            navigation.navigationBar.tintColor = palette.accentColor
            let barAppearance = UINavigationBarAppearance()
            barAppearance.configureWithDefaultBackground()
            if let color = palette.chromeColor { barAppearance.backgroundColor = color }
            navigation.navigationBar.standardAppearance = barAppearance
            navigation.navigationBar.scrollEdgeAppearance = barAppearance
            navigation.navigationBar.compactAppearance = barAppearance
        }
        for controller in navigation.viewControllers {
            controller.view.tintColor = palette.accentColor
            if let route = controller as? FeiRouteController {
                route.view.backgroundColor = route.hasNativeContent ? palette.listColor : palette.paperColor
                route.notesList?.applyTheme(palette)
            }
        }
        host.updateEditorToolbar(next)
        updateButtons()
        if let choice = navigation.topViewController as? FeiSettingsChoiceController,let settings = next.settings { choice.update(settings,busy: next.busy) }
        if next.information != oldState?.information { showInformation(next.information) }
        if next.reorder != oldState?.reorder { showReorder(next.reorder) }
        if next.confirmation != oldState?.confirmation || (next.confirmation?.versions != nil && (next.busy != oldState?.busy || next.cloud_history != oldState?.cloud_history)) { showConfirmation(next.confirmation) }
        host.deliverPendingInvitation()
    }

    private func showInformation(_ information: FeiNavigationState.Information?) {
        informationGeneration &+= 1
        let generation = informationGeneration
        guard let information else { informationAlert?.dismiss(animated: true); informationAlert = nil; informationSerial = nil; return }
        if let informationAlert,informationSerial == information.serial { informationAlert.title = information.title; informationAlert.message = information.message; return }
        guard let controller = host?.navigationController?.topViewController else { return }
        let alert = UIAlertController(title: information.title,message: information.message,preferredStyle: .alert)
        alert.view.accessibilityIdentifier = "fei.doc-info"
        let done = UIAlertAction(title: information.done,style: .default) { [weak self] _ in self?.send("doc-info-close",id: information.note,serial: information.serial) }
        done.accessibilityIdentifier = "fei.doc-info.done"; alert.addAction(done)
        let present = { [weak self] in
            guard let self,self.informationGeneration == generation,self.state?.information?.serial == information.serial else { return }
            self.informationAlert = alert; self.informationSerial = information.serial
            controller.present(alert,animated: true)
        }
        controller.view.endEditing(true)
        if let previous = controller.presentedViewController { previous.dismiss(animated: true,completion: present) } else { present() }
    }

    private func showReorder(_ request: FeiNavigationState.Reorder?) {
        reorderGeneration &+= 1
        let generation = reorderGeneration
        guard let request else { reorderController?.dismissForReplacement(); reorderController = nil; return }
        if let reorderController { reorderController.update(request); return }
        guard let controller = host?.navigationController?.topViewController else { return }
        let reorder = FeiReorderController(request) { [weak self] action,id,before,serial in self?.send(action,value: before,id: id,serial: serial) }
        let present = { [weak self] in
            guard let self,self.reorderGeneration == generation,self.state?.reorder?.serial == request.serial else { return }
            self.reorderController = reorder
            let navigation = UINavigationController(rootViewController: reorder)
            navigation.sheetPresentationController?.detents = [.large()]
            navigation.sheetPresentationController?.prefersGrabberVisible = true
            navigation.presentationController?.delegate = reorder
            controller.present(navigation,animated: true)
        }
        controller.view.endEditing(true)
        if let previous = controller.presentedViewController { previous.dismiss(animated: true,completion: present) } else { present() }
    }

    private func showConfirmation(_ confirmation: FeiNavigationState.Confirmation?) {
        confirmationGeneration &+= 1
        let generation = confirmationGeneration
        guard let confirmation else {
            confirmationAlert?.dismiss(animated: true); confirmationAlert = nil
            versionHistory?.dismissForReplacement(); versionHistory = nil
            return
        }
        guard let controller = host?.navigationController?.topViewController else { return }
        if confirmation.versions != nil {
            if let versionHistory { versionHistory.update(confirmation,enabled: state?.cloud_history == true && state?.busy == false); return }
            let history = FeiVersionHistoryController(confirmation,cancel: label("cancel"),enabled: state?.cloud_history == true && state?.busy == false) { [weak self] action,value,serial in
                self?.send(action,value: value,serial: serial)
            }
            let present = { [weak self] in
                guard let self,self.confirmationGeneration == generation,self.state?.confirmation == confirmation else { return }
                self.versionHistory = history
                let navigation = UINavigationController(rootViewController: history)
                navigation.sheetPresentationController?.detents = [.large()]
                navigation.sheetPresentationController?.prefersGrabberVisible = true
                navigation.presentationController?.delegate = history
                controller.present(navigation,animated: true)
            }
            controller.view.endEditing(true)
            if let previous = controller.presentedViewController { previous.dismiss(animated: true,completion: present) } else { present() }
            return
        }
        versionHistory?.dismissForReplacement(); versionHistory = nil
        let alert = UIAlertController(title: confirmation.title, message: confirmation.message, preferredStyle: .alert)
        alert.view.accessibilityIdentifier = "fei.confirm"
        func action(_ title: String, _ command: String, _ style: UIAlertAction.Style = .default, _ identifier: String) {
            let item = UIAlertAction(title: title,style: style) { [weak self] _ in self?.send(command,serial: confirmation.serial) }
            item.accessibilityIdentifier = identifier
            alert.addAction(item)
        }
        action(label("cancel"),"cancel-confirm",.cancel,"fei.confirm.cancel")
        if let choice = confirmation.choice { action((confirmation.choice_on ? "✓ " : "")+choice,"confirm-option",.default,"fei.confirm.choice") }
        if confirmation.save_failed {
            action(localized("另存副本","Save a copy"),"confirm-save-copy",.default,"fei.confirm.save-copy")
            action(localized("重新载入","Reload"),"confirm-reload",.destructive,"fei.confirm.reload")
        }
        action(confirmation.action,"confirm-action",confirmation.danger ? .destructive : .default,"fei.confirm.ok")
        fei_ipad_flush()
        controller.view.endEditing(true)
        let present = { [weak self] in
            guard let self,self.confirmationGeneration == generation,self.state?.confirmation == confirmation else { return }
            self.confirmationAlert = alert
            controller.present(alert,animated: true)
        }
        if let previous = controller.presentedViewController { previous.dismiss(animated: true,completion: present) } else { present() }
    }

    func refreshPadButtons() { (host?.navigationController?.topViewController as? FeiRouteController)?.barIdentity = nil; buttonsKind = nil; updateButtons() }
    func refreshInputControls() {
        guard !applying,host?.navigationController?.transitionCoordinator == nil else { return }
        updateButtons()
    }

    func refreshLayoutControls() {
        guard let controller = host?.navigationController?.topViewController else { return }
        let expanded = hasPadWorkspace && controller.traitCollection.horizontalSizeClass == .regular && controller.view.bounds.width >= 600
        if expanded != buttonsExpanded { updateButtons() }
    }

    private func updateButtons() {
        guard let state, let controller = host?.navigationController?.topViewController else { return }
        guard let route = controller as? FeiRouteController,
              (host?.padWorkspace?.detailRoutes(state) ?? state.routes).last?.key == route.routeKey else { return }
        defer {
            #if FEI_MOBILE_QA
            host?.navigationController?.navigationBar.accessibilityValue="fei-nav kind=\(state.kind) top=\(route.routeKey) pop=\(nativePopTarget ?? "none")"
            #endif
            controller.navigationItem.leftBarButtonItems?.forEach { $0.isEnabled = !state.busy }
            controller.navigationItem.rightBarButtonItems?.forEach { $0.isEnabled = !state.busy }
            if state.kind == "note",state.current == nil { controller.navigationItem.rightBarButtonItems?.filter { ["fei.more","fei.share"].contains($0.accessibilityIdentifier ?? "") }.forEach { $0.isEnabled = false } }
            if state.kind == "trash" { controller.navigationItem.rightBarButtonItem?.isEnabled = !state.busy && state.items.contains(where: { $0.trashed }) }
            if controller.navigationItem.hidesBackButton != state.busy { controller.navigationItem.hidesBackButton = state.busy }
            host?.navigationController?.interactivePopGestureRecognizer?.isEnabled = !state.busy && state.listing?.selection == nil
            host?.padWorkspace?.configureDetailButtons(controller,state: state,coordinator: self)
        }
        let navigationTitle = state.kind == "note" || hasPadWorkspace && ["notes","folder"].contains(state.kind) ? "" : state.routes.last?.title ?? state.title
        if controller.navigationItem.title != navigationTitle { controller.navigationItem.title = navigationTitle }
        host?.navigationController?.navigationBar.accessibilityIdentifier = state.kind == "note" ? state.title : state.routes.last?.title
        let expanded = hasPadWorkspace && controller.traitCollection.horizontalSizeClass == .regular && controller.view.bounds.width >= 600
        if let route = controller as? FeiRouteController,let list = route.notesList,!list.view.isHidden {
            list.configureSearch(in: controller,expanded: expanded)
        } else { host?.navigationController?.setToolbarHidden(true,animated: false) }
        // Keep bar items stable when only note state changes.
        let typing = host?.editorHasKeyboardFocus == true
        let identity = FeiRouteController.BarIdentity(kind: state.kind,shared: state.shared_session,editing: state.kind == "note" && state.can_edit,typing: state.kind == "note" && typing,expanded: expanded,labels: state.labels)
        if route.barIdentity == identity { return }
        route.barIdentity = identity
        buttonsController = controller
        buttonsKind = state.kind
        buttonsShared = state.shared_session
        buttonsEditing = state.can_edit
        buttonsTyping = typing
        buttonsExpanded = expanded
        buttonsLabels = state.labels
        let isList=["notes","folder"].contains(state.kind)
        controller.navigationItem.largeTitleDisplayMode = isList ? .always : .never
        host?.navigationController?.navigationBar.prefersLargeTitles = true
        controller.navigationItem.rightBarButtonItems = nil
        if state.kind == "note" {
            let more = UIBarButtonItem(image: UIImage(systemName: "ellipsis"),primaryAction: nil,menu: deferredActions(note: true))
            more.accessibilityLabel = state.labels["note-more"]
            more.accessibilityIdentifier = "fei.more"
            let share = UIBarButtonItem(image: UIImage(systemName: "square.and.arrow.up"),style: .plain,target: self,action: #selector(shareNote))
            share.accessibilityLabel = state.labels["share-note"]; share.accessibilityIdentifier = "fei.share"
            var items = [more,share]+(host?.navigationEditorTools.reversed() ?? [].reversed())
            if state.can_edit && typing {
                let done = UIBarButtonItem(image: UIImage(systemName: "checkmark"),style: .done,target: self,action: #selector(endTyping))
                done.accessibilityLabel = state.labels["end-typing"]
                done.accessibilityIdentifier = "fei.edit.done"; items.insert(done,at: 0)
            }
            if state.can_edit {
                let undo = UIBarButtonItem(image: UIImage(systemName: "arrow.uturn.backward"),style: .plain,target: self,action: #selector(undoNote))
                undo.accessibilityIdentifier = "fei.edit.undo"; undo.accessibilityLabel = state.labels["undo"]
                if #available(iOS 26.0,*) { undo.sharesBackground = false }
                undo.menu = UIMenu(children: [UIAction(title: label("redo"),image: UIImage(systemName: "arrow.uturn.forward")) { [weak self] _ in self?.dispatchUndoNote(redo: true) }])
                items.append(undo)
            }
            controller.navigationItem.rightBarButtonItems = items
        } else if state.kind == "history" {
            controller.navigationItem.rightBarButtonItems = (controller as? FeiRouteController)?.historyList?.barItems()
        } else if state.kind == "history-preview" {
            let close = UIBarButtonItem(image: UIImage(systemName: "xmark"),style: .plain,target: self,action: #selector(closeLocalHistory))
            close.accessibilityLabel = state.labels["history-return"]; close.accessibilityIdentifier = "fei.history.close"
            controller.navigationItem.rightBarButtonItem = close
        } else if state.kind == "trash" {
            let empty = UIBarButtonItem(title: state.labels["empty-trash"],style: .plain,target: self,action: #selector(emptyTrash))
            empty.accessibilityIdentifier = "fei.empty-trash"
            empty.tintColor = .systemRed
            controller.navigationItem.rightBarButtonItem = empty
        } else if state.kind == "settings" {
            if hasPadWorkspace {
                let close = UIBarButtonItem(systemItem: .close,primaryAction: UIAction { [weak self] _ in self?.closeSettings() })
                close.accessibilityIdentifier = "fei.settings.close"; close.accessibilityLabel = state.labels["done"]
                controller.navigationItem.rightBarButtonItem = close
            } else { controller.navigationItem.rightBarButtonItem = nil }
        } else if (state.kind == "notes" || state.kind == "folder") && !hasPadWorkspace {
            let more = UIBarButtonItem(image: UIImage(systemName: "ellipsis"),primaryAction: nil,menu: deferredActions(note: false))
            more.accessibilityLabel = state.labels["folder-more"]
            more.accessibilityIdentifier = "fei.library-actions"
            if expanded && state.can_create {
                let create = UIBarButtonItem(barButtonSystemItem: .compose,target: self,action: #selector(newNote))
                create.accessibilityLabel = state.labels["new"]; create.accessibilityIdentifier = "fei.new"
                controller.navigationItem.rightBarButtonItems = [create,more]
            } else { controller.navigationItem.rightBarButtonItems = [more] }
            if state.kind == "notes" && state.routes.count == 1 {
                let settings = UIBarButtonItem(image: UIImage(systemName: "gearshape"), style: .plain,
                    target: self, action: #selector(openSettings))
                settings.accessibilityLabel = state.labels["settings"]
                settings.accessibilityIdentifier = "fei.settings"
                controller.navigationItem.leftBarButtonItem = settings
            } else { controller.navigationItem.leftBarButtonItem = nil }
        }
    }

    func navigationController(_ navigationController: UINavigationController,willShow viewController: UIViewController,animated: Bool) {
        let list = (viewController as? FeiRouteController)?.notesList
        if let route = viewController as? FeiRouteController {
            let list=route.routeKey == "notes" || route.routeKey == "shared" || route.routeKey.hasPrefix("folder:")
            route.navigationItem.largeTitleDisplayMode=list ? .always : .never
            if list { route.navigationItem.title=route.title }
        }
        if let route = viewController as? FeiRouteController { host?.prepareEditorTransition(to: route) }
        let compactList = list?.view.isHidden == false && state?.kind != "trash" && host?.padWorkspace == nil
        navigationController.setToolbarHidden(!compactList,animated: animated)
    }

    func navigationController(_ navigationController: UINavigationController,
                              didShow viewController: UIViewController, animated: Bool) {
        guard let shown = viewController as? FeiRouteController, let host else {
            applying = false
            drainPendingAfterTransition()
            return
        }
        host.attachEditor(to: shown)
        if !applying, let state, (host.padWorkspace?.detailRoutes(state) ?? state.routes).last?.key != shown.routeKey,nativePopTarget != shown.routeKey {
            nativePopTarget=shown.routeKey; nativePopSource=(host.padWorkspace?.detailRoutes(state) ?? state.routes).last?.key
            let remaining = (host.padWorkspace?.detailRoutes(state) ?? state.routes).count - navigationController.viewControllers.count
            for _ in 0..<max(0, remaining) { send("back") }
        }
        applying = false
        if focusAfterTransition,state?.kind == "note",state?.can_edit == true {
            focusAfterTransition = false
            DispatchQueue.main.async { [weak host] in host?.prepareEditorInput() }
        }
        updateButtons()
        drainPendingAfterTransition()
        host.deliverPendingInvitation()
    }

    func searchPresentationDidEnd() { drainPendingAfterTransition() }

    private func drainPendingAfterTransition() {
        guard pending != nil,!pendingDrainScheduled else { return }
        pendingDrainScheduled = true
        // UIKit can still report a coordinator inside didShow. Apply only after it releases the transition.
        DispatchQueue.main.asyncAfter(deadline: .now()+0.02) { [weak self] in
            guard let self else { return }
            self.pendingDrainScheduled = false
            guard let next = self.pending else { return }
            if self.host?.navigationController?.transitionCoordinator != nil { self.drainPendingAfterTransition(); return }
            self.pending = nil
            self.receive(next)
        }
    }

    func send(_ action: String, value: String? = nil, id: String? = nil, serial: UInt64? = nil, scope: String? = nil) {
        #if FEI_MOBILE_QA
        if let directory=FileManager.default.urls(for:.cachesDirectory,in:.userDomainMask).first {
            let url=directory.appendingPathComponent("qa49-native-commands.log")
            let line="\(action) state=\(state?.kind ?? "nil") top=\((host?.navigationController?.topViewController as? FeiRouteController)?.routeKey ?? "nil")\n"
            if let file=try? FileHandle(forWritingTo:url) { file.seekToEndOfFile(); file.write(Data(line.utf8)); try? file.close() }
            else { try? Data(line.utf8).write(to:url) }
        }
        #endif
        if action == "toolbar-command",["done","styles","insert","undo","redo"].contains(value ?? ""),let state,state.kind == "note",state.current == id,state.editor_scope == scope,state.can_edit,state.write_allowed,!state.busy {
            host?.settleEditorComposition()
        }
        if ["list-open","compose","browse-compose","new","new-document","new-markdown","settings","sharing","trash","back","history-open","history-command"].contains(action),let sender = host?.navigationController?.topViewController as? FeiRouteController {
            sender.freezeContent()
            DispatchQueue.main.asyncAfter(deadline: .now()+0.2) { [weak self,weak sender] in
                guard let sender,self?.host?.navigationController?.topViewController === sender,self?.host?.navigationController?.transitionCoordinator == nil else { return }
                sender.thawContent()
            }
        }
        if ["compose","browse-compose","new","new-document","new-markdown"].contains(action) { focusAfterTransition = true }
        var command: [String: Any] = ["action": action]
        if let scope { command["scope"] = scope }
        if let serial { command["selection_serial"] = serial }
        if let value { command["value"] = value }
        if let id { command["id"] = id }
        guard let bytes = try? JSONSerialization.data(withJSONObject: command),
              let json = String(data: bytes, encoding: .utf8) else { return }
        let perform = { json.withCString { fei_ios_command($0) } }
        if let presented = host?.navigationController?.topViewController?.presentedViewController,
           presented is UIAlertController || ["import","restore-backup","pdf","print","format","table-menu","format-menu","outline-menu","insert-menu","attachments-menu","objects-menu"].contains(action) {
            presented.dismiss(animated: true, completion: perform)
        } else { perform() }
    }

    func sendHistory(_ op:String,name:String? = nil,text:String? = nil,history: FeiNavigationState.LocalHistory? = nil) {
        guard let h = history ?? state?.local_history else { return }
        var payload:[String:String] = ["op":op]
        if let name { payload["name"] = name }; if let text { payload["text"] = text }
        guard let data = try? JSONSerialization.data(withJSONObject: payload),let json = String(data: data,encoding: .utf8) else { return }
        send("history-command",value: json,id: h.note,serial: h.token,scope: h.scope)
    }
    @objc private func closeLocalHistory() { sendHistory("close") }
    func renameHistory(_ row:FeiNavigationState.LocalHistory.Version,history:FeiNavigationState.LocalHistory) {
        let alert = UIAlertController(title: label("history-name"),message: nil,preferredStyle: .alert)
        alert.addTextField { field in field.text = row.named ? row.title : ""; field.accessibilityIdentifier = "fei.history.rename" }
        alert.addAction(UIAlertAction(title: label("cancel"),style: .cancel))
        let save = UIAlertAction(title: label("save"),style: .default) { [weak self,weak alert] _ in self?.sendHistory("rename",name: row.name,text: alert?.textFields?.first?.text ?? "",history: history) }
        save.accessibilityIdentifier = "fei.history.rename.save"; alert.addAction(save); present(alert)
    }
    func confirmHistory(_ op:String,name:String? = nil,history:FeiNavigationState.LocalHistory? = nil) {
        guard let h = history ?? state?.local_history,let version = name ?? h.selected else { return }
        let alert = UIAlertController(title: label(op == "restore" ? "history-restore-confirm" : "history-delete-confirm"),message: nil,preferredStyle: .alert)
        alert.addAction(UIAlertAction(title: label("cancel"),style: .cancel))
        let action = UIAlertAction(title: label(op == "restore" ? "history-restore" : "history-delete"),style: .destructive) { [weak self] _ in self?.sendHistory(op,name: version,history: h) }
        action.accessibilityIdentifier = "fei.history.confirm"; alert.addAction(action); present(alert)
    }

    @objc private func newNote() { guard let scope = state?.listing?.scope else { return }; composeNote(listingScope: scope) }
    @objc private func endTyping() {
        guard let state,state.kind == "note" else { return }
        send("toolbar-command",value: "done",id: state.current,scope: state.editor_scope)
    }
    @objc private func undoNote() { dispatchUndoNote(redo: false) }
    private func dispatchUndoNote(redo: Bool) {
        guard let state,state.kind == "note",state.can_edit,!state.busy else { return }
        send("toolbar-command",value: redo ? "redo" : "undo",id: state.current,scope: state.editor_scope)
    }
    @objc private func shareNote(_ sender: UIBarButtonItem) {
        guard let state,let id = state.current,!state.busy else { return }
        var actions: [(String,UIAlertAction.Style,() -> Void)] = [("export",.default,{ [weak self] in self?.showExport(id) })]
        if state.cloud_active { actions.insert(("share-note",.default,{ [weak self] in self?.send("share-note",id: id) }),at: 0) }
        sheet(actions: actions,sender: sender)
    }
    @objc private func newFolder() { newFolder(parentID: nil) }
    private func newFolder(parentID: String?) { if state?.listing?.kind == "shared" { send("collection-new"); return }; rename(id: parentID,title: "",creatingFolder: true) }
    @objc private func openSettings() { send("settings") }
    @objc private func closeSettings() { send("close-settings") }
    @objc private func emptyTrash() { send("empty-trash") }

    private func label(_ key: String) -> String { state?.labels[key] ?? key }

    func localized(_ zh: String, _ en: String) -> String { state?.labels["cancel"] == "Cancel" ? en : zh }

    private func sheet(_ title: String? = nil, actions: [(String, UIAlertAction.Style, () -> Void)], sender: UIBarButtonItem? = nil) {
        guard let controller = host?.navigationController?.topViewController else { return }
        fei_ipad_flush()
        controller.view.endEditing(true)
        let sheet = UIAlertController(title: title, message: nil, preferredStyle: .actionSheet)
        for (key, style, action) in actions {
            sheet.addAction(UIAlertAction(title: label(key), style: style) { _ in action() })
        }
        sheet.addAction(UIAlertAction(title: label("cancel"), style: .cancel))
        if let popover = sheet.popoverPresentationController {
            if let sender { popover.barButtonItem = sender }
            else {
                popover.sourceView = controller.view
                popover.sourceRect = sheetAnchorID.flatMap { (controller as? FeiRouteController)?.notesList?.anchor(for: $0) } ?? CGRect(x: controller.view.bounds.midX,y: controller.view.bounds.midY,width: 1,height: 1)
            }
        }
        present(sheet)
    }

    private func present(_ view: UIViewController) {
        guard let controller = host?.navigationController?.topViewController else { return }
        presentationGeneration &+= 1
        let generation = presentationGeneration
        let show = { [weak self,weak controller] in
            guard let self,let controller,self.presentationGeneration == generation,self.host?.navigationController?.topViewController === controller else { return }
            controller.present(view,animated: true)
        }
        if let presented = controller.presentedViewController { presented.dismiss(animated: true,completion: show) } else { show() }
    }

    private func deferredActions(note: Bool,browser: Bool = false) -> UIMenu {
        UIMenu(children: [UIDeferredMenuElement.uncached { [weak self] finish in
            guard let self else { finish([]); return }
            finish((note ? self.noteActionsMenu() : self.listActionsMenu(browser: browser)).children)
        }])
    }
    private func menuAction(_ key: String,_ symbol: String,destructive: Bool = false,enabled: Bool = true,perform: @escaping () -> Void) -> UIAction {
        let current = state?.current,scope = state?.editor_scope,listingScope = state?.listing?.scope,browserScope = state?.browser?.listing.scope
        return UIAction(title: label(key),image: symbol == "textformat.size" ? FeiEditorToolbar.formatIcon : UIImage(systemName: symbol),attributes: !enabled ? .disabled : destructive ? .destructive : []) { [weak self] _ in
            guard let self,self.state?.busy == false,self.state?.current == current,self.state?.editor_scope == scope,self.state?.listing?.scope == listingScope,self.state?.browser?.listing.scope == browserScope else { return }
            self.host?.settleEditorComposition(); perform()
        }
    }
    private func listActionsMenu(browser: Bool) -> UIMenu {
        guard let state else { return UIMenu() }
        if state.shared_session {
            var actions: [UIMenuElement] = [menuAction("sharing","person.2") { [weak self] in self?.send("sharing") },menuAction("return-personal","arrow.uturn.backward") { [weak self] in self?.send("return-personal") }]
            if state.can_create {
                actions.insert(menuAction("new","square.and.pencil") { [weak self] in self?.composeNote() },at: 0)
                actions.insert(menuAction("new-folder","folder.badge.plus") { [weak self] in if browser { self?.createSidebarFolder() } else { self?.newFolder() } },at: 1)
            }
            return UIMenu(children: actions)
        }
        if state.listing?.kind == "shared" || browser && state.browser?.shared == true {
            return UIMenu(children: [menuAction("new-personal-folder","folder.badge.plus") { [weak self] in self?.send("collection-new") },menuAction("accept-invitation","envelope.open") { [weak self] in self?.send("accept-invitation") },menuAction("batch","checkmark.circle") { [weak self] in self?.beginListSelection(browser: browser) },menuAction("sort","arrow.up.arrow.down") { [weak self] in self?.showSort() }])
        }
        let create = UIMenu(options: .displayInline,children: [
            menuAction("new","square.and.pencil") { [weak self] in self?.composeNote() },
            menuAction("new-folder","folder.badge.plus") { [weak self] in if browser { self?.createSidebarFolder() } else { self?.newFolder() } }])
        let sort = UIMenu(title: label("sort"),image: UIImage(systemName: "arrow.up.arrow.down"),children: ["manual","visited","modified","created","title"].map { key in
            menuAction(key,"line.3.horizontal") { [weak self] in self?.send("sort",value: key) }
        })
        var organize: [UIMenuElement] = [sort]
        if state.can_reorder { organize.append(menuAction("reorder","line.3.horizontal") { [weak self] in
            if browser { self?.host?.padWorkspace?.beginBrowserReorder() } else { (self?.host?.navigationController?.topViewController as? FeiRouteController)?.notesList?.beginReordering() }
        }) }
        organize.insert(menuAction("batch","checkmark.circle") { [weak self] in self?.beginListSelection(browser: browser) },at: 0)
        if let id = browser ? state.browser?.folder : state.routes.last?.key.replacingOccurrences(of: "folder:",with: ""),let item = state.items.first(where: { $0.id == id && $0.folder }) {
            organize.append(menuAction("rename","pencil") { [weak self] in self?.rename(id: id,title: item.title) })
        }
        let transfer = UIMenu(options: .displayInline,children: [menuAction("import","square.and.arrow.down") { [weak self] in self?.send("import") }])
        return UIMenu(children: [create,UIMenu(options: .displayInline,children: organize),transfer])
    }
    private func beginListSelection(browser: Bool) {
        host?.settleEditorComposition(); host?.view.endEditing(true)
        if browser { host?.padWorkspace?.beginBrowserSelection() }
        else { (host?.navigationController?.topViewController as? FeiRouteController)?.notesList?.beginSelection() }
    }
    private func noteActionsMenu() -> UIMenu {
        guard let state,let id = state.current else { return UIMenu() }
        let search = UIMenu(options: .displayInline,children: [menuAction("find","magnifyingglass") { [weak self] in self?.send("find") },menuAction("outline","list.bullet") { [weak self] in self?.send("outline-menu") }])
        var editing: [UIMenuElement] = [menuAction("format-text","textformat.size",enabled: state.write_allowed) { [weak self] in self?.send("toolbar-command",value: "styles",id: id,scope: state.editor_scope) },menuAction("insert","plus",enabled: state.write_allowed) { [weak self] in self?.send("insert-menu") },menuAction("objects","slider.horizontal.3",enabled: state.write_allowed || state.has_code) { [weak self] in self?.send("objects-menu") }]
        if state.table_active { editing.append(menuAction("table-controls","tablecells",enabled: state.can_edit) { [weak self] in self?.send("table-menu") }) }
        var organization: [UIMenuElement] = []
        if !state.shared_session {
            organization = [menuAction("rename","pencil",enabled: state.write_allowed) { [weak self] in self?.rename(id: id,title: self?.state?.title ?? "") },menuAction("move","folder",enabled: state.write_allowed) { [weak self] in self?.showMove(id) },menuAction("duplicate","doc.on.doc",enabled: state.write_allowed) { [weak self] in self?.send("duplicate",id: id) }]
            if let item = state.items.first(where: { $0.id == id }) {
                organization += [menuAction(item.pinned ? "unpin" : "pin","pin") { [weak self] in self?.send("pin",id: id) },menuAction(item.starred ? "unstar" : "star","star") { [weak self] in self?.send("star",id: id) }]
            }
        }
        if state.shared_session {
            organization = [menuAction("rename","pencil",enabled: state.write_allowed) { [weak self] in self?.rename(id: id,title: self?.state?.title ?? "") },
                menuAction("sharing","person.2") { [weak self] in self?.send("sharing") }]
            if let item = state.items.first(where: { $0.id == id }) {
                organization += [menuAction(item.pinned ? "unpin" : "pin","pin") { [weak self] in self?.send("shared-pin",id: id,scope: state.editor_scope) },
                    menuAction(item.starred ? "unstar" : "star","star") { [weak self] in self?.send("shared-star",id: id,scope: state.editor_scope) },
                    menuAction("duplicate","doc.on.doc") { [weak self] in self?.send("shared-copy",id: id,scope: state.editor_scope) },
                    menuAction(item.owned ? "delete-shared" : "leave-shared","trash",destructive: true) { [weak self] in self?.send("shared-end",id: id,scope: state.editor_scope) }]
            }
        }
        editing.append(menuAction(state.reading ? "editing" : "reading","book",enabled: state.write_allowed) { [weak self] in self?.send(state.reading ? "editing" : "reading",id: id) })
        let info = UIMenu(title: label("doc-info"),image: UIImage(systemName: "info.circle"),children: [menuAction("doc-info","info.circle") { [weak self] in self?.send("doc-info",id: id) },menuAction("history","clock.arrow.circlepath") { [weak self] in self?.send("history-open",id: id,scope: state.editor_scope) },menuAction("attachments","paperclip") { [weak self] in self?.send("attachments-menu") }])
        var export: [UIMenuElement] = [menuAction("export","square.and.arrow.up") { [weak self] in self?.showExport(id) },menuAction("pdf","doc.richtext") { [weak self] in self?.send("pdf",id: id) },menuAction("print","printer") { [weak self] in self?.send("print",id: id) }]
        if state.cloud_active && !state.shared_session { export.insert(menuAction("share-note","person.badge.plus",enabled: state.write_allowed) { [weak self] in self?.send("share-note",id: id) },at: 0) }
        var groups: [UIMenuElement] = [search,UIMenu(title: label("format"),image: FeiEditorToolbar.formatIcon,children: editing)]
        if !organization.isEmpty { groups.append(UIMenu(title: label("options"),image: UIImage(systemName: "folder"),children: organization)) }
        groups += [info,UIMenu(title: localized("分享与导出","Share and export"),image: UIImage(systemName: "square.and.arrow.up"),children: export)]
        if state.shared_session { groups.append(menuAction("return-personal","arrow.uturn.backward") { [weak self] in self?.send("return-personal") }) }
        else { groups.append(UIMenu(options: .displayInline,children: [menuAction("delete","trash",destructive: true,enabled: state.write_allowed) { [weak self] in self?.send("delete",id: id) }])) }
        return UIMenu(children: groups)
    }

    @objc private func showLibraryActions(_ sender: UIBarButtonItem) {
        if state?.shared_session == true {
            sheet(actions: [("return-personal",.default,{ [weak self] in self?.send("return-personal") }),
                ("sharing",.default,{ [weak self] in self?.send("sharing") })],sender: sender)
            return
        }
        sheet(actions: [
            ("library", .default, { [weak self] in self?.showLibraryChoice() }),
            ("sharing", .default, { [weak self] in self?.send("sharing") }),
            ("new-document", .default, { [weak self] in self?.send("new-document") }),
            ("new-folder", .default, { [weak self] in self?.newFolder() }),
            ("batch", .default, { [weak self] in self?.send("batch-open") }),
            ("sort", .default, { [weak self] in self?.showSort() }),
            ("import", .default, { [weak self] in self?.send("import") }),
            ("backup", .default, { [weak self] in self?.send("backup") }),
            ("restore-backup", .default, { [weak self] in self?.send("restore-backup") }),
            ("trash", .default, { [weak self] in self?.send("trash") }),
        ], sender: sender)
    }

    private func showLibraryChoice() {
        guard let state else { return }
        let choice = UIAlertController(title: label("library"), message: nil, preferredStyle: .actionSheet)
        for key in state.cloud_active ? ["library-local","library-cloud"] : ["library-local","library-cloud","library-copy"] {
            let action = UIAlertAction(title: label(key), style: .default) { [weak self] _ in self?.send(key) }
            action.isEnabled = key == "library-local" ? state.cloud_active : !state.cloud_active
            choice.addAction(action)
        }
        choice.addAction(UIAlertAction(title: label("cancel"), style: .cancel))
        if let controller = host?.navigationController?.topViewController, let popover = choice.popoverPresentationController {
            popover.sourceView = controller.view
            popover.sourceRect = CGRect(x: controller.view.bounds.midX, y: 0, width: 1, height: 1)
        }
        present(choice)
    }

    @objc private func showMore(_ sender: UIBarButtonItem) {
        guard let state, let id = state.current else { return }
        if state.shared_session {
            var sharedActions: [(String,UIAlertAction.Style,() -> Void)] = [("find",.default,{ [weak self] in self?.send("find") }),
                ("outline",.default,{ [weak self] in self?.send("outline-menu") }),
                ("attachments",.default,{ [weak self] in self?.send("attachments-menu") }),
                ("doc-info",.default,{ [weak self] in self?.send("doc-info",id: id) }),
                ("sharing",.default,{ [weak self] in self?.send("sharing") }),
                ("export",.default,{ [weak self] in self?.showExport(id) }),
                ("pdf",.default,{ [weak self] in self?.send("pdf",id: id) }),
                ("print",.default,{ [weak self] in self?.send("print",id: id) }),
                ("return-personal",.default,{ [weak self] in self?.send("return-personal") })]
            if state.can_edit || state.has_code { sharedActions.insert((state.can_edit ? "objects" : "code-tools",.default,{ [weak self] in self?.send("objects-menu") }),at: 1) }
            if state.can_edit { sharedActions.insert(("format",.default,{ [weak self] in self?.showFormat() }),at: 1) }
            if state.can_edit { sharedActions.insert(("insert",.default,{ [weak self] in self?.send("insert-menu") }),at: 1) }
            if state.table_active { sharedActions.insert(("table-controls",.default,{ [weak self] in self?.send("table-menu") }),at: 1) }
            sharedActions.insert(("history",.default,{ [weak self] in self?.send("history-open",id: id,scope: state.editor_scope) }),at: 1)
            sheet(actions: sharedActions,sender: sender)
            return
        }
        var actions: [(String,UIAlertAction.Style,() -> Void)] = [
            ("find", .default, { [weak self] in self?.send("find") }),
            ("outline", .default, { [weak self] in self?.send("outline-menu") }),
            ("attachments", .default, { [weak self] in self?.send("attachments-menu") }),
            ("objects", .default, { [weak self] in self?.send("objects-menu") }),
            ("doc-info", .default, { [weak self] in self?.send("doc-info",id: id) }),
            ("rename", .default, { [weak self] in self?.rename(id: id, title: state.title) }),
            ("format", .default, { [weak self] in self?.showFormat() }),
            ("insert", .default, { [weak self] in self?.send("insert-menu") }),
            ("options", .default, { [weak self] in self?.showItemActions(id) }),
            ("export", .default, { [weak self] in self?.showExport(id) }),
            ("pdf", .default, { [weak self] in self?.send("pdf", id: id) }),
            ("print", .default, { [weak self] in self?.send("print", id: id) }),
            ("settings", .default, { [weak self] in self?.send("settings") }),
            ("delete", .destructive, { [weak self] in self?.send("delete", id: id) }),
        ]
        if !state.can_edit && state.has_code { actions.insert(("code-tools",.default,{ [weak self] in self?.send("objects-menu") }),at: 1) }
        if state.table_active { actions.insert(("table-controls",.default,{ [weak self] in self?.send("table-menu") }),at: 3) }
        actions.insert(("history",.default,{ [weak self] in self?.send("history-open",id: id,scope: state.editor_scope) }),at: 1)
        if state.cloud_history { actions.insert(("cloud-versions",.default,{ [weak self] in self?.send("cloud-versions",id: id) }),at: 4) }
        if state.cloud_active { actions.insert(("share-note",.default,{ [weak self] in self?.send("share-note",id: id) }),at: 4) }
        if state.can_create { actions.insert(("new",.default,{ [weak self] in self?.composeNote() }),at: 0) }
        sheet(actions: actions.filter { !["format","insert","objects"].contains($0.0) || state.can_edit },sender: sender)
    }

    private func itemActions(_ id: String) -> [(String,UIAlertAction.Style,() -> Void)] {
        guard let item = state?.items.first(where: { $0.id == id }) else { return [] }
        if item.trashed {
            return [
                ("restore", .default, { [weak self] in self?.send("restore", id: id) }),
                ("purge", .destructive, { [weak self] in self?.send("purge", id: id) }),
            ]
        }
        var actions: [(String, UIAlertAction.Style, () -> Void)] = [
            ("rename", .default, { [weak self] in self?.rename(id: id, title: item.title) }),
            ("move", .default, { [weak self] in self?.showMove(id) }),
            ("duplicate", .default, { [weak self] in self?.send("duplicate", id: id) }),
        ]
        if !item.folder {
            actions += [
                (item.pinned ? "unpin" : "pin", .default, { [weak self] in self?.send("pin", id: id) }),
                (item.starred ? "unstar" : "star", .default, { [weak self] in self?.send("star", id: id) }),
                ("reading", .default, { [weak self] in self?.send("reading", id: id) }),
                ("editing", .default, { [weak self] in self?.send("editing", id: id) }),
                ("export", .default, { [weak self] in self?.showExport(id) }),
                ("pdf", .default, { [weak self] in self?.send("pdf", id: id) }),
                ("print", .default, { [weak self] in self?.send("print", id: id) }),
            ]
        }
        actions.append(("delete", .destructive, { [weak self] in self?.send("delete", id: id) }))
        return actions
    }

    func showItemActions(_ id: String) {
        sheetAnchorID = id
        defer { sheetAnchorID = nil }
        sheet(state?.items.first(where: { $0.id == id })?.title,actions: itemActions(id))
    }
    func itemMenu(_ id: String,browsingWorkspace: Bool = false) -> UIMenu {
        guard let item = state?.items.first(where: { $0.id == id }) else { return UIMenu() }
        let scope = browsingWorkspace ? state?.browser?.listing.scope : state?.listing?.scope
        func action(_ key: String,_ symbol: String,_ command: String? = nil,danger: Bool = false,perform: (() -> Void)? = nil) -> UIAction {
            UIAction(title: label(key),image: UIImage(systemName: symbol),attributes: danger ? .destructive : []) { [weak self] _ in
                guard let self,let scope,self.state?.busy == false,(browsingWorkspace ? self.state?.browser?.listing.scope : self.state?.listing?.scope) == scope else { return }
                if let perform { perform() } else { self.send(command ?? key,id: id,scope: scope) }
            }
        }
        func group(_ items: [UIMenuElement]) -> UIMenu { UIMenu(options: .displayInline,children: items) }
        if item.trashed { return UIMenu(children: [group([action("restore","arrow.uturn.backward")]),group([action("purge","trash",danger: true)])]) }
        if item.id.hasPrefix("collection-") {
            return UIMenu(children: [action("rename","pencil","collection-rename"),action("move","folder",perform: { [weak self] in self?.showMove(id) }),action("remove-personal-folder","folder.badge.minus","collection-remove")])
        }
        let flags: [UIMenuElement] = item.folder ? [] : [action(item.pinned ? "unpin" : "pin","pin",item.shared ? "shared-pin" : "pin"),action(item.starred ? "unstar" : "star","star",item.shared ? "shared-star" : "star")]
        if item.shared {
            var organize: [UIMenuElement] = [action("move","folder",perform: { [weak self] in self?.showMove(id) })]
            if !item.folder { organize.append(action("duplicate","doc.on.doc","shared-copy")) }
            if item.owned { organize.append(action("sharing","person.2","shared-members")) }
            return UIMenu(children: [group(flags),group(organize),group([action(item.owned ? "delete-shared" : "leave-shared","trash","shared-end",danger: true)])])
        }
        var organize: [UIMenuElement] = [action("rename","pencil",perform: { [weak self] in self?.rename(id: id,title: item.title) }),action("move","folder",perform: { [weak self] in self?.showMove(id) }),action("duplicate","doc.on.doc")]
        if item.folder { organize.insert(action("new-folder","folder.badge.plus",perform: { [weak self] in if browsingWorkspace { self?.createSidebarFolder(parentID: id) } else { self?.newFolder(parentID: id) } }),at: 0) }
        var groups: [UIMenuElement] = flags.isEmpty ? [] : [group(flags)]
        groups.append(group(organize))
        if !item.folder { groups.append(UIMenu(title: localized("分享与导出","Share and export"),image: UIImage(systemName: "square.and.arrow.up"),children: [action("export","doc.text",perform: { [weak self] in self?.showExport(id) }),action("pdf","doc.richtext"),action("print","printer")])) }
        groups.append(group([action("delete","trash",danger: true)]))
        return UIMenu(children: groups)
    }

    func rowItem(_ id: String) -> FeiNavigationState.Item? { state?.items.first { $0.id == id } }
    func rowAction(_ key: String,id: String,scope: String,browser: Bool) {
        guard state?.busy == false,(browser ? state?.browser?.listing.scope : state?.listing?.scope)==scope,let item=state?.items.first(where: { $0.id==id }) else { return }
        if key=="more" { showItemActions(id); return }
        if key=="move" { showMove(id); return }
        let command = item.shared ? ["pin":"shared-pin","star":"shared-star","delete":"shared-end"][key] ?? key : key
        send(command,id: id,scope: scope)
    }


    func browserActionsItem() -> UIBarButtonItem {
        let item = UIBarButtonItem(image: UIImage(systemName: "ellipsis"),primaryAction: nil,menu: deferredActions(note: false,browser: true))
        item.accessibilityIdentifier = "fei.library-actions"; item.accessibilityLabel = label("more"); item.isEnabled = state?.busy == false
        return item
    }
    @objc private func showBrowserActions(_ sender: UIBarButtonItem) {
        var actions: [(String,UIAlertAction.Style,() -> Void)] = [
            ("sort",.default,{ [weak self] in self?.showSort() }),
            ("new-folder",.default,{ [weak self] in self?.createSidebarFolder() })]
        if let folder = state?.browser?.folder,let item = state?.items.first(where: { $0.id == folder }) {
            actions.append(("rename",.default,{ [weak self] in self?.rename(id: folder,title: item.title) }))
        }
        sheet(actions: actions,sender: sender)
    }

    func createSidebarFolder(parentID: String? = nil) {
        if state?.browser?.shared == true { send("collection-new"); return }
        guard let state,let scope = state.browser?.listing.scope,!state.busy,state.can_create else { return }
        let alert = UIAlertController(title: state.labels["new-folder"],message: nil,preferredStyle: .alert)
        alert.addTextField { $0.accessibilityIdentifier = "fei.sidebar.folder-name" }
        alert.addAction(UIAlertAction(title: label("cancel"),style: .cancel))
        let save = UIAlertAction(title: label("save"),style: .default) { [weak self,weak alert] _ in
            guard let name = alert?.textFields?.first?.text?.trimmingCharacters(in: .whitespacesAndNewlines),!name.isEmpty else { return }
            self?.send("browse-new-folder",value: name,id: parentID,scope: scope)
        }
        save.accessibilityIdentifier = "fei.sidebar.folder-save"; alert.addAction(save); present(alert)
    }

    private func showSort() {
        var actions: [(String,UIAlertAction.Style,() -> Void)] = ["manual","visited","modified","created","title"].map { key in
            (key, .default, { [weak self] in self?.send("sort", value: key) })
        }
        if state?.can_reorder == true { actions.append(("reorder",.default,{ [weak self] in self?.send("reorder-open") })) }
        sheet(label("sort"),actions: actions)
    }

    private func showMove(_ id: String) { showMove([id],serial: nil) }

    func showMove(_ ids: [String], serial: UInt64?) {
        guard let state else { return }
        if serial != nil && (state.shared_session || !["notes","folder"].contains(state.kind) && !hasPadWorkspace) { return }
        let selected = Set(ids)
        let shortcuts = ids.allSatisfy { id in state.items.contains { $0.id == id && ($0.shared || $0.id.hasPrefix("collection-")) } }
        let folders = state.items.filter { item in
            guard item.folder,!item.trashed,item.id.hasPrefix("collection-") == shortcuts else { return false }
            var parent: String? = item.id
            var seen = Set<String>()
            while let current = parent {
                if selected.contains(current) || !seen.insert(current).inserted { return false }
                parent = state.items.first(where: { $0.id == current })?.parent
            }
            return true
        }
        let picker = FeiFolderPicker(title: label("move"), cancel: label("cancel"), root: label("root"), folders: folders,cancelled: { [weak self] in if let serial { self?.send("batch-move-cancel",serial: serial) } }) { [weak self] folder in
            if let serial { self?.send("batch-move",value: folder ?? "",serial: serial) }
            else if let id = ids.first { self?.send("move",value: folder ?? "",id: id) }
        }
        present(UINavigationController(rootViewController: picker))
    }

    private func showExport(_ id: String) {
        sheet(label("export"), actions: [
            ("text-only", .default, { [weak self] in self?.send("export", value: "text", id: id) }),
            ("all-content", .default, { [weak self] in self?.send("export", value: "all", id: id) }),
        ])
    }

    private func showFormat() {
        sheet(label("format"), actions: [
            ("undo", .default, { [weak self] in self?.send("format", value: "undo") }),
            ("redo", .default, { [weak self] in self?.send("format", value: "redo") }),
            ("format-text", .default, { [weak self] in self?.send("format-menu") }),
            ("format-block", .default, { [weak self] in self?.send("format-menu",value: "blocks") }),
            ("divider", .default, { [weak self] in self?.send("format",value: "divider") }),
            ("table", .default, { [weak self] in self?.send("format",value: "table") }),
        ])
    }

    private func rename(id: String?, title: String, creatingFolder: Bool = false) {
        let form = FeiRenameController(title: label(creatingFolder ? "new-folder" : "rename"),
            value: title, save: label("save"), cancel: label("cancel")) { [weak self] value in
            self?.send(creatingFolder ? "new-folder" : "rename", value: value, id: id)
        }
        let navigation = UINavigationController(rootViewController: form)
        navigation.sheetPresentationController?.detents = [.medium(), .large()]
        present(navigation)
    }

}

@_cdecl("fei_ios_navigation_changed")
func feiIOSNavigationChanged(_ json: UnsafePointer<CChar>) {
    let data = Data(String(cString: json).utf8)
    let state:FeiNavigationState
    do { state = try JSONDecoder().decode(FeiNavigationState.self,from: data) }
    catch { return }
    Task { @MainActor in EditorHost.active?.navigationCoordinator?.receive(state) }
}


@MainActor
private final class FeiFolderPicker: UITableViewController, UIAdaptivePresentationControllerDelegate {
    let folders: [FeiNavigationState.Item]
    let rootTitle: String
    let selected: (String?) -> Void
    let cancelled: (() -> Void)?
    private var completed = false

    init(title: String, cancel: String, root: String, folders: [FeiNavigationState.Item], cancelled: (() -> Void)? = nil, selected: @escaping (String?) -> Void) {
        self.folders = folders.sorted { $0.title.localizedStandardCompare($1.title) == .orderedAscending }
        self.rootTitle = root
        self.selected = selected; self.cancelled=cancelled
        super.init(style: .insetGrouped)
        self.title = title
        navigationItem.leftBarButtonItem = UIBarButtonItem(title: cancel, style: .plain, target: self, action: #selector(close))
    }
    required init?(coder: NSCoder) { fatalError("init(coder:) is unsupported") }
    override func viewDidAppear(_ animated: Bool) { super.viewDidAppear(animated); navigationController?.presentationController?.delegate=self }
    private func cancelOnce() { if !completed { completed=true; cancelled?() } }
    func presentationControllerDidDismiss(_ presentationController: UIPresentationController) { cancelOnce() }
    @objc private func close() { cancelOnce(); dismiss(animated: true) }
    override func tableView(_ tableView: UITableView, numberOfRowsInSection section: Int) -> Int { folders.count + 1 }
    override func tableView(_ tableView: UITableView, cellForRowAt indexPath: IndexPath) -> UITableViewCell {
        let cell = UITableViewCell(style: .subtitle, reuseIdentifier: nil)
        var content = cell.defaultContentConfiguration()
        content.text = indexPath.row == 0 ? rootTitle : folders[indexPath.row - 1].title
        if indexPath.row > 0, let parent = folders[indexPath.row - 1].parent {
            content.secondaryText = folders.first(where: { $0.id == parent })?.title
        }
        content.image = UIImage(systemName: "folder")
        cell.contentConfiguration = content
        return cell
    }
    override func tableView(_ tableView: UITableView, didSelectRowAt indexPath: IndexPath) {
        let folder = indexPath.row == 0 ? nil : folders[indexPath.row - 1].id
        completed = true
        dismiss(animated: true) { self.selected(folder) }
    }
}

@_cdecl("fei_ios_item_actions")
func feiIOSItemActions(_ id: UnsafePointer<CChar>) {
    let id = String(cString: id)
    Task { @MainActor in EditorHost.active?.navigationCoordinator?.showItemActions(id) }
}


@MainActor
private final class FeiRenameController: UIViewController, UITextFieldDelegate {
    private let field = UITextField()
    private let selected: (String) -> Void

    init(title: String, value: String, save: String, cancel: String, selected: @escaping (String) -> Void) {
        self.selected = selected
        super.init(nibName: nil, bundle: nil)
        self.title = title
        field.text = value
        field.accessibilityIdentifier = "fei.rename.title"
        navigationItem.leftBarButtonItem = UIBarButtonItem(title: cancel, style: .plain, target: self, action: #selector(close))
        navigationItem.rightBarButtonItem = UIBarButtonItem(title: save, style: .done, target: self, action: #selector(saveTitle))
        navigationItem.rightBarButtonItem?.accessibilityIdentifier = "fei.rename.save"
    }
    required init?(coder: NSCoder) { fatalError("init(coder:) is unsupported") }
    override func viewDidLoad() {
        super.viewDidLoad()
        view.backgroundColor = .systemGroupedBackground
        field.translatesAutoresizingMaskIntoConstraints = false
        field.borderStyle = .roundedRect
        field.font = .preferredFont(forTextStyle: .body)
        field.adjustsFontForContentSizeCategory = true
        field.returnKeyType = .done
        field.clearButtonMode = .whileEditing
        field.delegate = self
        view.addSubview(field)
        NSLayoutConstraint.activate([
            field.topAnchor.constraint(equalTo: view.safeAreaLayoutGuide.topAnchor, constant: 24),
            field.leadingAnchor.constraint(equalTo: view.safeAreaLayoutGuide.leadingAnchor, constant: 20),
            field.trailingAnchor.constraint(equalTo: view.safeAreaLayoutGuide.trailingAnchor, constant: -20),
            field.heightAnchor.constraint(greaterThanOrEqualToConstant: 44),
        ])
    }
    override func viewDidAppear(_ animated: Bool) {
        super.viewDidAppear(animated)
        field.becomeFirstResponder()
        field.selectAll(nil)
    }
    @objc private func close() { view.endEditing(true); dismiss(animated: true) }
    @objc private func saveTitle() {
        guard let value = field.text?.trimmingCharacters(in: .whitespacesAndNewlines), !value.isEmpty else { return }
        view.endEditing(true)
        dismiss(animated: true) { self.selected(value) }
    }
    func textFieldShouldReturn(_ textField: UITextField) -> Bool { saveTitle(); return true }
}


@_cdecl("fei_ios_is_phone")
func feiIOSIsPhone() -> Bool { UIDevice.current.userInterfaceIdiom == .phone }


@_cdecl("fei_ios_present_batch_move")
@MainActor
func feiIOSPresentBatchMove(_ json: UnsafePointer<CChar>) {
    struct Request: Decodable, Sendable { let serial: UInt64; let ids: [String] }
    guard let request = try? JSONDecoder().decode(Request.self,from: Data(String(cString: json).utf8)) else { return }
    DispatchQueue.main.async { EditorHost.active?.navigationCoordinator?.showMove(request.ids,serial: request.serial) }
}
