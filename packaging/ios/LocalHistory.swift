import UIKit

extension FeiNavigationState {
    struct LocalHistory: Decodable, Equatable, Sendable {
        struct Version: Decodable, Equatable, Sendable { let name:String; let title:String; let detail:String; let named:Bool }
        let token:UInt64; let scope:String; let note:String; let selected:String?; let previewing:Bool
        let named_only:Bool; let marks:Bool; let can_restore:Bool; let can_copy:Bool
        let versions:[Version]; let notice:String?; let empty:String; let subtitle:String
    }
}

@MainActor
final class FeiLocalHistoryController: UITableViewController {
    private weak var coordinator: FeiNavigationCoordinator?
    private var state: FeiNavigationState.LocalHistory?
    private var labels:[String:String] = [:]
    private var busy = false
    private let empty = UILabel()
    private var rows:[FeiNavigationState.LocalHistory.Version] { guard let state else { return [] }; return state.versions.filter { !state.named_only || $0.named } }

    init(coordinator: FeiNavigationCoordinator) { self.coordinator = coordinator; super.init(style: .insetGrouped) }
    required init?(coder: NSCoder) { fatalError("init(coder:) is unsupported") }
    override func viewDidLoad() {
        super.viewDidLoad()
        tableView.accessibilityIdentifier = "fei.local.versions"
        tableView.rowHeight = UITableView.automaticDimension; tableView.estimatedRowHeight = 80
        empty.font = .preferredFont(forTextStyle: .body); empty.adjustsFontForContentSizeCategory = true
        empty.numberOfLines = 0; empty.textAlignment = .center; empty.textColor = .secondaryLabel
    }
    func update(_ state: FeiNavigationState.LocalHistory,labels:[String:String],busy:Bool) {
        loadViewIfNeeded()
        if let notice = state.notice, notice != self.state?.notice {
            UIAccessibility.post(notification: .announcement,argument: notice)
        }
        self.state = state; self.labels = labels; self.busy = busy
        tableView.isUserInteractionEnabled = !busy
        empty.text = state.empty; tableView.backgroundView = rows.isEmpty ? empty : nil
        navigationItem.prompt = state.notice ?? state.subtitle
        toolbarItems = barItems()
        tableView.reloadData()
    }
    private func barItems() -> [UIBarButtonItem] {
        guard let state else { return [] }
        let save = UIBarButtonItem(image: UIImage(systemName: "plus"),style: .plain,target: self,action: #selector(saveCurrent))
        save.accessibilityLabel = labels["history-save"]; save.accessibilityIdentifier = "fei.history.save"; save.isEnabled = !busy
        let named = UIAction(title: labels["history-named"] ?? "Named versions only",state: state.named_only ? .on : .off) { [weak self] _ in self?.coordinator?.sendHistory("named",history: state) }
        let filter = UIBarButtonItem(image: UIImage(systemName: "line.3.horizontal.decrease.circle"),menu: UIMenu(children: [named]))
        filter.accessibilityLabel = labels["history-filter"]; filter.accessibilityIdentifier = "fei.history.filter"; filter.isEnabled = !busy
        return [filter,UIBarButtonItem(systemItem: .flexibleSpace),save]
    }
    @objc private func saveCurrent() { if let state,!busy { coordinator?.sendHistory("save",history: state) } }
    override func tableView(_ tableView: UITableView,numberOfRowsInSection section:Int) -> Int { rows.count }
    override func tableView(_ tableView: UITableView,cellForRowAt indexPath:IndexPath) -> UITableViewCell {
        let row = rows[indexPath.row]; let cell = UITableViewCell(style: .subtitle,reuseIdentifier: nil)
        var content = cell.defaultContentConfiguration(); content.text = row.title; content.secondaryText = row.detail
        content.textProperties.font = .preferredFont(forTextStyle: .body); content.textProperties.adjustsFontForContentSizeCategory = true
        content.secondaryTextProperties.font = .preferredFont(forTextStyle: .subheadline); content.secondaryTextProperties.color = .secondaryLabel
        content.secondaryTextProperties.adjustsFontForContentSizeCategory = true; content.secondaryTextProperties.numberOfLines = 0
        cell.contentConfiguration = content; cell.accessoryType = .disclosureIndicator
        cell.accessibilityIdentifier = "fei.local.version."+row.name
        return cell
    }
    override func tableView(_ tableView: UITableView,didSelectRowAt indexPath:IndexPath) {
        guard let state,!busy,rows.indices.contains(indexPath.row) else { return }
        tableView.deselectRow(at: indexPath,animated: true)
        coordinator?.sendHistory("preview",name: rows[indexPath.row].name,history: state)
    }
    override func tableView(_ tableView: UITableView,contextMenuConfigurationForRowAt indexPath:IndexPath,point:CGPoint) -> UIContextMenuConfiguration? {
        guard let state,!busy,rows.indices.contains(indexPath.row) else { return nil }
        let row = rows[indexPath.row]
        return UIContextMenuConfiguration(identifier: row.name as NSString,previewProvider: nil) { [weak self] _ in
            guard let self else { return nil }
            var actions = [UIAction(title: self.labels["history-name"] ?? "Name version",image: UIImage(systemName: "pencil")) { [weak self] _ in self?.coordinator?.renameHistory(row,history: state) }]
            if state.can_copy { actions.append(UIAction(title: self.labels["history-copy"] ?? "Save as a note",image: UIImage(systemName: "doc.on.doc")) { [weak self] _ in self?.coordinator?.sendHistory("copy",name: row.name,history: state) }) }
            actions.append(UIAction(title: self.labels["history-delete"] ?? "Delete version",image: UIImage(systemName: "trash"),attributes: .destructive) { [weak self] _ in self?.coordinator?.confirmHistory("delete",name: row.name,history: state) })
            return UIMenu(children: actions)
        }
    }
}
