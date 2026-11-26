import UIKit

@MainActor
final class FeiReorderController: UITableViewController, UIAdaptivePresentationControllerDelegate {
    private var state: FeiNavigationState.Reorder
    private var rows: [FeiNavigationState.Reorder.Item]
    private let send: (String,String?,String?,UInt64) -> Void
    private let accent: UIColor
    private var waiting = false
    private var finishing = false

    init(_ state: FeiNavigationState.Reorder, accent: UIColor, send: @escaping (String,String?,String?,UInt64) -> Void) {
        self.state = state; rows = state.items; self.send = send; self.accent = accent
        super.init(style: .insetGrouped)
    }
    required init?(coder: NSCoder) { fatalError("init(coder:) has not been implemented") }
    override func viewDidLoad() {
        super.viewDidLoad()
        tableView.accessibilityIdentifier = "fei.reorder.rows"
        tableView.rowHeight = UITableView.automaticDimension
        tableView.estimatedRowHeight = 56
        tableView.allowsSelection = false
        setEditing(true,animated: false)
        update(state)
    }
    func update(_ next: FeiNavigationState.Reorder) {
        guard !finishing else { return }
        let acknowledged = next.response != state.response
        let refresh = acknowledged || state.saving || next.saving
        state = next
        if acknowledged { waiting = false }
        // Keep row identities stable while a drag might be in progress.
        if refresh && rows != next.items { rows = next.items; if isViewLoaded { tableView.reloadData() } }
        title = next.title; navigationItem.prompt = next.prompt
        navigationItem.rightBarButtonItem = UIBarButtonItem(title: next.done,style: .done,target: self,action: #selector(done))
        navigationItem.rightBarButtonItem?.tintColor = accent
        navigationItem.rightBarButtonItem?.setTitleTextAttributes([.foregroundColor: UIColor.white],for: .normal)
        navigationItem.rightBarButtonItem?.accessibilityIdentifier = "fei.reorder.done"
        navigationItem.rightBarButtonItem?.isEnabled = !next.saving && !waiting
        navigationController?.isModalInPresentation = next.saving || waiting
        if isViewLoaded { tableView.isUserInteractionEnabled = !next.saving && !waiting }
    }
    override func tableView(_ tableView: UITableView,numberOfRowsInSection section: Int) -> Int { rows.count }
    override func tableView(_ tableView: UITableView,cellForRowAt indexPath: IndexPath) -> UITableViewCell {
        let row = rows[indexPath.row]
        let cell = UITableViewCell(style: .default,reuseIdentifier: nil)
        var content = cell.defaultContentConfiguration()
        content.text = row.title
        content.textProperties.font = .preferredFont(forTextStyle: .body)
        content.textProperties.adjustsFontForContentSizeCategory = true
        content.textProperties.numberOfLines = 0
        content.image = UIImage(systemName: row.folder ? "folder" : "doc.text")
        cell.contentConfiguration = content
        cell.accessibilityIdentifier = "fei.reorder.row.\(row.id)"
        cell.showsReorderControl = true
        if row.pinned { cell.accessoryView = UIImageView(image: UIImage(systemName: "pin.fill")) }
        return cell
    }
    override func tableView(_ tableView: UITableView,editingStyleForRowAt indexPath: IndexPath) -> UITableViewCell.EditingStyle { .none }
    override func tableView(_ tableView: UITableView,shouldIndentWhileEditingRowAt indexPath: IndexPath) -> Bool { false }
    override func tableView(_ tableView: UITableView,canMoveRowAt indexPath: IndexPath) -> Bool { !state.saving && !waiting }
    override func tableView(_ tableView: UITableView,moveRowAt sourceIndexPath: IndexPath,to destinationIndexPath: IndexPath) {
        guard !state.saving,!waiting,rows.indices.contains(sourceIndexPath.row),rows.indices.contains(destinationIndexPath.row) else { return }
        let item = rows.remove(at: sourceIndexPath.row)
        rows.insert(item,at: destinationIndexPath.row)
        let before = rows.indices.contains(destinationIndexPath.row+1) ? rows[destinationIndexPath.row+1].id : ""
        waiting = true; update(state)
        send("reorder-move",item.id,before,state.serial)
    }
    @objc private func done() { if !state.saving && !waiting { close() } }
    private func close() {
        guard !finishing else { return }; finishing = true
        let serial = state.serial
        dismiss(animated: true) { self.send("reorder-close",nil,nil,serial) }
    }
    func presentationControllerShouldDismiss(_ presentationController: UIPresentationController) -> Bool { !state.saving && !waiting }
    func presentationControllerDidDismiss(_ presentationController: UIPresentationController) {
        guard !finishing else { return }; finishing = true; send("reorder-close",nil,nil,state.serial)
    }
    func dismissForReplacement() { close() }
}
