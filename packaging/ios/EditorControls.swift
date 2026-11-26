import UIKit

@MainActor
final class FeiEditorControls: UITableViewController, UIAdaptivePresentationControllerDelegate {
    struct Request: Decodable {
        struct Section: Decodable {
            struct Action: Decodable {
                let key: String; let title: String; let enabled: Bool; let checked: Bool; let destructive: Bool
                let submenu: String?
                let swatch: [CGFloat]?
                let depth: Int?
                let dismiss: Bool?
                let detail: String?
            }
            let key: String?
            let hidden: Bool?
            let title: String
            let actions: [Action]
        }
        let serial: UInt64
        let title: String
        let done: String
        let context: String
        let namespace: String?
        let command: String?
        let close_command: String?
        let initial: String?
        let sections: [Section]
        let toolbar: [Section.Action]?
    }
    private static weak var active: FeiEditorControls?
    private static var generation: UInt64 = 0
    private var request: Request
    private var waiting = false
    private let section: String?
    private var namespace: String { request.namespace ?? "table" }
    private var sections: [Request.Section] { request.sections.filter { section == nil ? $0.hidden != true : $0.key == section } }

    private var restoreEditorInput: (() -> Void)?
    init(_ request: Request, section: String? = nil, restoreInput: (() -> Void)? = nil) { self.request = request; self.section = section; restoreEditorInput = restoreInput; super.init(style: .insetGrouped) }
    required init?(coder: NSCoder) { fatalError("init(coder:) has not been implemented") }

    override func viewDidLoad() {
        super.viewDidLoad()
        tableView.rowHeight = UITableView.automaticDimension
        tableView.estimatedRowHeight = 48
        tableView.accessibilityIdentifier = "fei.\(namespace).controls"
        update(request)
    }

    private func update(_ next: Request) {
        request = next
        waiting = false
        title = section == nil ? next.title : sections.first?.title ?? next.title
        navigationItem.prompt = next.context
        navigationItem.rightBarButtonItem = UIBarButtonItem(title: next.done,style: .done,target: self,action: #selector(done))
        navigationItem.rightBarButtonItem?.accessibilityIdentifier = "fei.\(namespace).done"
        updateToolbar()
        if isViewLoaded { tableView.reloadData() }
    }

    private func updateToolbar() {
        var items: [UIBarButtonItem] = []
        for (index,action) in (request.toolbar ?? []).enumerated() {
            if index > 0 { items.append(UIBarButtonItem(systemItem: .flexibleSpace)) }
            let item = UIBarButtonItem(title: action.title,style: .plain,target: self,action: #selector(toolbarAction(_:)))
            item.tag = index; item.isEnabled = action.enabled && !waiting
            item.accessibilityIdentifier = "fei.\(namespace).\(action.key)"
            if action.destructive { item.tintColor = .systemRed }
            items.append(item)
        }
        toolbarItems = items
        navigationController?.setToolbarHidden(request.toolbar == nil,animated: false)
    }
    @objc private func toolbarAction(_ sender: UIBarButtonItem) {
        guard let actions = request.toolbar,actions.indices.contains(sender.tag) else { return }
        perform(actions[sender.tag])
    }

    @objc private func done() {
        Self.generation &+= 1
        Self.active = nil
        dismiss(animated: true) { self.notifyClose(); self.restoreInput() }
    }

    private func restoreInput() { let restore = restoreEditorInput; restoreEditorInput = nil; restore?() }
    private func notifyClose() {
        guard let command = request.close_command,let data = try? JSONSerialization.data(withJSONObject: ["action":command,"selection_serial":request.serial]),let json = String(data: data,encoding: .utf8) else { return }
        json.withCString { fei_ios_command($0) }
    }
    func presentationControllerDidDismiss(_ presentationController: UIPresentationController) {
        Self.generation &+= 1; Self.active = nil
        notifyClose(); restoreInput()
    }

    static func enqueue(_ request: Request) {
        generation &+= 1
        let current = generation
        DispatchQueue.main.async {
            guard generation == current else { return }
            if let active,let navigation = active.navigationController,navigation.presentingViewController != nil {
                for controller in navigation.viewControllers.compactMap({ $0 as? FeiEditorControls }) { controller.update(request) }
                return
            }
            guard let host = EditorHost.active?.navigationController?.topViewController else { return }
            let restore = ["format","insert","block"].contains(request.namespace ?? "") ? EditorHost.active?.navigationCoordinator?.captureEditorInputReturn() : nil
            host.view.endEditing(true)
            let present = {
                guard generation == current else { return }
                let controller = FeiEditorControls(request,restoreInput: restore)
                active = controller
                let navigation = UINavigationController(rootViewController: controller)
                navigation.overrideUserInterfaceStyle = EditorHost.active?.navigationCoordinator?.interfaceAppearance ?? .unspecified
                navigation.navigationBar.accessibilityIdentifier = "fei.\(request.namespace ?? "table").navigation"
                if let section = request.initial { navigation.setViewControllers([controller,FeiEditorControls(request,section: section,restoreInput: restore)],animated: false) }
                navigation.sheetPresentationController?.detents = [.large()]
                navigation.sheetPresentationController?.prefersGrabberVisible = true
                navigation.presentationController?.delegate = controller
                host.present(navigation,animated: true) { navigation.presentationController?.delegate = controller }
            }
            if let previous = host.presentedViewController { previous.dismiss(animated: true,completion: present) }
            else { present() }
        }
    }

    static func invalidate() {
        generation &+= 1
        active?.dismiss(animated: true)
        active = nil
    }

    override func numberOfSections(in tableView: UITableView) -> Int { sections.count }
    override func tableView(_ tableView: UITableView,numberOfRowsInSection section: Int) -> Int { sections[section].actions.count }
    override func tableView(_ tableView: UITableView,titleForHeaderInSection section: Int) -> String? { sections[section].title }
    override func tableView(_ tableView: UITableView,cellForRowAt indexPath: IndexPath) -> UITableViewCell {
        let action = sections[indexPath.section].actions[indexPath.row]
        let cell = UITableViewCell(style: .default,reuseIdentifier: nil)
        var content = cell.defaultContentConfiguration()
        content.text = action.title
        content.secondaryText = action.detail
        content.textProperties.font = .preferredFont(forTextStyle: .body)
        content.textProperties.adjustsFontForContentSizeCategory = true
        content.textProperties.numberOfLines = 0
        content.directionalLayoutMargins.leading += CGFloat(min(8,max(0,action.depth ?? 0))) * 14
        content.textProperties.color = !action.enabled || waiting ? .tertiaryLabel : action.destructive ? .systemRed : .label
        if let color = action.swatch,color.count == 4 {
            content.image = UIImage(systemName: "circle.fill")
            content.imageProperties.tintColor = UIColor(red: color[0],green: color[1],blue: color[2],alpha: color[3])
        }
        cell.contentConfiguration = content
        cell.accessoryType = action.submenu != nil ? .disclosureIndicator : action.checked ? .checkmark : .none
        if action.checked { cell.accessibilityTraits.insert(.selected) }
        cell.accessibilityIdentifier = "fei.\(namespace).\(action.key)"
        cell.isUserInteractionEnabled = action.enabled && !waiting
        if !cell.isUserInteractionEnabled { cell.accessibilityTraits.insert(.notEnabled) }
        return cell
    }
    override func tableView(_ tableView: UITableView,didSelectRowAt indexPath: IndexPath) {
        tableView.deselectRow(at: indexPath,animated: true)
        let action = sections[indexPath.section].actions[indexPath.row]
        perform(action)
    }
    private func perform(_ action: Request.Section.Action) {
        guard action.enabled,!waiting else { return }
        if let section = action.submenu {
            navigationController?.pushViewController(FeiEditorControls(request,section: section,restoreInput: restoreEditorInput),animated: !UIAccessibility.isReduceMotionEnabled)
            return
        }
        waiting = true
        updateToolbar()
        tableView.reloadData()
        let command: [String: Any] = ["action":request.command ?? "table-action","value":action.key,"selection_serial":request.serial]
        guard let data = try? JSONSerialization.data(withJSONObject: command),let json = String(data: data,encoding: .utf8) else { return }
        let perform = { json.withCString { fei_ios_command($0) } }
        if action.dismiss == true {
            Self.generation &+= 1
            Self.active = nil
            dismiss(animated: true,completion: perform)
        } else { perform() }
    }
}

@_cdecl("fei_ios_present_table_controls")
@MainActor
func feiIOSPresentTableControls(_ json: UnsafePointer<CChar>) {
    guard let request = try? JSONDecoder().decode(FeiEditorControls.Request.self,from: Data(String(cString: json).utf8)) else { return }
    FeiEditorControls.enqueue(request)
}

@_cdecl("fei_ios_dismiss_table_controls")
@MainActor
func feiIOSDismissTableControls() { FeiEditorControls.invalidate() }

@_cdecl("fei_ios_present_format_controls")
@MainActor
func feiIOSPresentFormatControls(_ json: UnsafePointer<CChar>) {
    guard let request = try? JSONDecoder().decode(FeiEditorControls.Request.self,from: Data(String(cString: json).utf8)) else { return }
    FeiEditorControls.enqueue(request)
}
