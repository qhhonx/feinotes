import UIKit

@MainActor
final class FeiVersionHistoryController: UITableViewController, UIAdaptivePresentationControllerDelegate {
    private var state: FeiNavigationState.Confirmation
    private let cancel: String
    private let send: (String,String?,UInt64) -> Void
    private var finishing = false
    private var enabled: Bool

    init(_ state: FeiNavigationState.Confirmation, cancel: String, enabled: Bool, send: @escaping (String,String?,UInt64) -> Void) {
        self.state = state; self.cancel = cancel; self.enabled = enabled; self.send = send
        super.init(style: .insetGrouped)
    }
    required init?(coder: NSCoder) { fatalError("init(coder:) has not been implemented") }
    override func viewDidLoad() {
        super.viewDidLoad()
        tableView.accessibilityIdentifier = "fei.cloud.versions"
        tableView.rowHeight = UITableView.automaticDimension
        tableView.estimatedRowHeight = 76
        update(state,enabled: enabled)
    }
    func update(_ next: FeiNavigationState.Confirmation, enabled: Bool) {
        guard !finishing else { return }
        state = next; self.enabled = enabled; title = next.title
        tableView.allowsSelection = enabled
        navigationItem.prompt = next.message
        navigationItem.leftBarButtonItem = UIBarButtonItem(title: cancel,style: .plain,target: self,action: #selector(close))
        navigationItem.leftBarButtonItem?.accessibilityIdentifier = "fei.cloud.versions.cancel"
        navigationItem.rightBarButtonItem = UIBarButtonItem(title: next.action,style: .done,target: self,action: #selector(restore))
        navigationItem.rightBarButtonItem?.accessibilityIdentifier = "fei.cloud.versions.restore"
        navigationItem.rightBarButtonItem?.isEnabled = enabled && next.selected_version != nil
        if isViewLoaded { tableView.reloadData() }
    }
    override func tableView(_ tableView: UITableView,numberOfRowsInSection section: Int) -> Int { state.versions?.count ?? 0 }
    override func tableView(_ tableView: UITableView,cellForRowAt indexPath: IndexPath) -> UITableViewCell {
        let version = state.versions![indexPath.row]
        let cell = UITableViewCell(style: .subtitle,reuseIdentifier: nil)
        var content = cell.defaultContentConfiguration()
        content.text = version.time; content.secondaryText = version.preview
        content.textProperties.font = .preferredFont(forTextStyle: .body)
        content.textProperties.adjustsFontForContentSizeCategory = true
        content.secondaryTextProperties.font = .preferredFont(forTextStyle: .subheadline)
        content.secondaryTextProperties.adjustsFontForContentSizeCategory = true
        content.secondaryTextProperties.numberOfLines = 2
        cell.contentConfiguration = content
        cell.accessibilityIdentifier = "fei.cloud.versions.\(version.sequence)"
        if state.selected_version == version.sequence { cell.accessoryType = .checkmark; cell.accessibilityTraits.insert(.selected) }
        return cell
    }
    override func tableView(_ tableView: UITableView,didSelectRowAt indexPath: IndexPath) {
        guard !finishing,enabled,let versions = state.versions,versions.indices.contains(indexPath.row) else { return }
        tableView.deselectRow(at: indexPath,animated: true)
        send("cloud-version-select",String(versions[indexPath.row].sequence),state.serial)
    }
    @objc private func close() { finish("cancel-confirm") }
    @objc private func restore() { if enabled && state.selected_version != nil { finish("confirm-action") } }
    private func finish(_ command: String) {
        guard !finishing else { return }
        finishing = true
        let serial = state.serial
        dismiss(animated: true) { self.send(command,nil,serial) }
    }
    func presentationControllerDidDismiss(_ presentationController: UIPresentationController) {
        guard !finishing else { return }
        finishing = true; send("cancel-confirm",nil,state.serial)
    }
    func dismissForReplacement() { finishing = true; dismiss(animated: true) }
}
