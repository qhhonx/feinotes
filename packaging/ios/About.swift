import UIKit

private struct FeiNoticesCatalogue: Decodable {
    struct Entry: Decodable { let title: String; let subtitle: String; let parts: [String] }
    let documents: [String:String]; let entries: [Entry]
}

@MainActor
private func noticeText(_ zh: String,_ en: String) -> String { EditorHost.active?.navigationCoordinator?.localized(zh,en) ?? en }

@MainActor
private final class FeiNoticesIndex: UITableViewController, UISearchResultsUpdating, UISearchControllerDelegate {
    private let catalogue: FeiNoticesCatalogue
    private let search = UISearchController(searchResultsController: nil)
    private var rows: [FeiNoticesCatalogue.Entry] = []
    private var pendingReader: UIViewController?
    init(title: String,catalogue: FeiNoticesCatalogue) { self.catalogue = catalogue; self.rows = catalogue.entries; super.init(style: .insetGrouped); self.title = title }
    required init?(coder: NSCoder) { fatalError("init(coder:) is unsupported") }
    override func viewDidLoad() {
        super.viewDidLoad(); tableView.accessibilityIdentifier = "fei.notices.list"
        tableView.rowHeight = UITableView.automaticDimension; tableView.estimatedRowHeight = 64
        search.obscuresBackgroundDuringPresentation = false; search.searchResultsUpdater = self
        search.hidesNavigationBarDuringPresentation = false; search.delegate = self
        search.searchBar.placeholder = noticeText("搜索组件或许可","Search components or licenses")
        search.searchBar.searchTextField.accessibilityIdentifier = "fei.notices.search"
        navigationItem.searchController = search; navigationItem.hidesSearchBarWhenScrolling = false
        definesPresentationContext = true
        navigationItem.leftBarButtonItem = UIBarButtonItem(barButtonSystemItem: .done,target: self,action: #selector(close))
        navigationItem.leftBarButtonItem?.accessibilityIdentifier = "fei.notices.done"
    }
    override func viewWillAppear(_ animated: Bool) { super.viewWillAppear(animated); navigationController?.setToolbarHidden(true,animated: animated) }
    @objc private func close() { dismiss(animated: true) }
    func updateSearchResults(for searchController: UISearchController) {
        let query = searchController.searchBar.text?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
        rows = catalogue.entries.filter { query.isEmpty || $0.title.localizedCaseInsensitiveContains(query) || $0.subtitle.localizedCaseInsensitiveContains(query) }
        tableView.reloadData()
    }
    override func tableView(_ tableView: UITableView,numberOfRowsInSection section: Int) -> Int { rows.count }
    override func tableView(_ tableView: UITableView,cellForRowAt indexPath: IndexPath) -> UITableViewCell {
        let entry = rows[indexPath.row]; let cell = UITableViewCell(style: .subtitle,reuseIdentifier: nil)
        var content = cell.defaultContentConfiguration(); content.text = entry.title; content.secondaryText = entry.subtitle
        content.textProperties.numberOfLines = 0; content.textProperties.adjustsFontForContentSizeCategory = true
        content.secondaryTextProperties.adjustsFontForContentSizeCategory = true
        cell.contentConfiguration = content; cell.accessoryType = .disclosureIndicator
        return cell
    }
    override func tableView(_ tableView: UITableView,didSelectRowAt indexPath: IndexPath) {
        let entry = rows[indexPath.row]; let pages = entry.parts.compactMap { catalogue.documents[$0] }
        guard !pages.isEmpty else { return }
        let reader = FeiNoticeReader(title: entry.title,pages: pages)
        view.endEditing(true)
        if search.isActive { pendingReader = reader; search.isActive = false }
        else { navigationController?.pushViewController(reader,animated: true) }
    }
    func didDismissSearchController(_ searchController: UISearchController) {
        guard let reader = pendingReader else { return }; pendingReader = nil
        navigationController?.pushViewController(reader,animated: true)
    }
}

@MainActor
private final class FeiNoticeReader: UIViewController {
    private let pages: [String]
    private var page = 0
    private let count = UILabel()
    private var previousPageButton: UIBarButtonItem!; private var nextPageButton: UIBarButtonItem!
    init(title: String,pages: [String]) { self.pages = pages; super.init(nibName: nil,bundle: nil); self.title = title }
    required init?(coder: NSCoder) { fatalError("init(coder:) is unsupported") }
    override func loadView() {
        let reader = UITextView(); reader.isEditable = false; reader.isSelectable = true
        reader.font = .preferredFont(forTextStyle: .body); reader.adjustsFontForContentSizeCategory = true; reader.usesStandardTextScaling = true
        reader.textColor = .label; reader.backgroundColor = .systemBackground
        reader.textContainerInset = UIEdgeInsets(top: 16,left: 16,bottom: 24,right: 16)
        reader.isFindInteractionEnabled = true; reader.alwaysBounceVertical = true; reader.accessibilityIdentifier = "fei.notices.text"
        view = reader
    }
    override func viewDidLoad() {
        super.viewDidLoad()
        let done = UIBarButtonItem(barButtonSystemItem: .done,target: self,action: #selector(close)); done.accessibilityIdentifier = "fei.notices.done"
        let find = UIBarButtonItem(barButtonSystemItem: .search,target: self,action: #selector(showFind)); find.accessibilityIdentifier = "fei.notices.find"
        find.accessibilityLabel = noticeText("在本页查找","Find on this page"); navigationItem.rightBarButtonItems = [done,find]
        previousPageButton = UIBarButtonItem(image: UIImage(systemName: "chevron.left"),style: .plain,target: self,action: #selector(previousPage))
        nextPageButton = UIBarButtonItem(image: UIImage(systemName: "chevron.right"),style: .plain,target: self,action: #selector(nextPage))
        previousPageButton.accessibilityIdentifier = "fei.notices.previous"; previousPageButton.accessibilityLabel = noticeText("上一页","Previous page")
        nextPageButton.accessibilityIdentifier = "fei.notices.next"; nextPageButton.accessibilityLabel = noticeText("下一页","Next page")
        count.font = .preferredFont(forTextStyle: .footnote); count.adjustsFontForContentSizeCategory = true; count.accessibilityIdentifier = "fei.notices.page"
        toolbarItems = [previousPageButton,UIBarButtonItem(systemItem: .flexibleSpace),UIBarButtonItem(customView: count),UIBarButtonItem(systemItem: .flexibleSpace),nextPageButton]
        display()
    }
    override func viewWillAppear(_ animated: Bool) { super.viewWillAppear(animated); navigationController?.setToolbarHidden(false,animated: animated) }
    private func display() {
        let reader = view as! UITextView; reader.findInteraction?.dismissFindNavigator(); reader.text = pages[page]; reader.setContentOffset(.zero,animated: false)
        previousPageButton.isEnabled = page > 0; nextPageButton.isEnabled = page + 1 < pages.count; count.text = "\(page + 1) / \(pages.count)"; count.sizeToFit()
    }
    @objc private func previousPage() { guard page > 0 else { return }; page -= 1; display() }
    @objc private func nextPage() { guard page + 1 < pages.count else { return }; page += 1; display() }
    @objc private func close() { dismiss(animated: true) }
    @objc private func showFind() { (view as? UITextView)?.findInteraction?.presentFindNavigator(showingReplace: false) }
}

@_cdecl("fei_ios_present_notices")
@MainActor
func feiIOSPresentNotices(_ path: UnsafePointer<CChar>,_ title: UnsafePointer<CChar>) {
    let url = URL(fileURLWithPath: String(cString: path)); let title = String(cString: title)
    guard let host = EditorHost.active?.navigationController?.topViewController else { return }
    let controller: UIViewController
    do {
        let catalogue = try JSONDecoder().decode(FeiNoticesCatalogue.self,from: Data(contentsOf: url))
        guard !catalogue.entries.isEmpty, catalogue.entries.allSatisfy({ !$0.parts.isEmpty && $0.parts.allSatisfy { catalogue.documents[$0] != nil } }), catalogue.documents.values.allSatisfy({ $0.count <= 6000 }) else { throw CocoaError(.fileReadCorruptFile) }
        controller = UINavigationController(rootViewController: FeiNoticesIndex(title: title,catalogue: catalogue))
    } catch {
        let alert = UIAlertController(title: title,message: error.localizedDescription,preferredStyle: .alert)
        alert.addAction(UIAlertAction(title: noticeText("完成","Done"),style: .default)); controller = alert
    }
    host.view.endEditing(true)
    let present = { host.present(controller,animated: true) }
    if let previous = host.presentedViewController { previous.dismiss(animated: true,completion: present) } else { present() }
}
