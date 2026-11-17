import UIKit

@MainActor
private final class FeiNotesDataSource: UITableViewDiffableDataSource<Int,String> {
    var movable: ((IndexPath) -> Bool)?
    var moved: ((IndexPath,IndexPath) -> Void)?
    override func tableView(_ tableView: UITableView,canMoveRowAt indexPath: IndexPath) -> Bool { movable?(indexPath) == true }
    override func tableView(_ tableView: UITableView,moveRowAt sourceIndexPath: IndexPath,to destinationIndexPath: IndexPath) { moved?(sourceIndexPath,destinationIndexPath) }
}

@MainActor
final class FeiNotesListController: UIViewController, UITableViewDelegate, UISearchBarDelegate, UISearchResultsUpdating, UISearchControllerDelegate {
    let tableView = UITableView(frame: .zero,style: .insetGrouped)
    private weak var coordinator: FeiNavigationCoordinator?
    var browsesWorkspace = false
    private var selectedNote: String?
    var allowsCloudResume: Bool { !searchController.isActive && !tableView.isEditing && !isSelecting && pendingMove == nil && !openingResult && !pendingSelection }
    private var reorderButtons: [UIBarButtonItem]?
    private struct PendingMove { let scope:String; let order:[String]; var writing = false }
    private var pendingMove: PendingMove?
    private struct ListAnchor { let id: String?; let delta: CGFloat; let fallback: CGPoint }
    private var browserOffsets: [String:ListAnchor] = [:]
    private var listing: FeiNavigationState.Listing?
    private var rows: [String:FeiNavigationState.Listing.Row] = [:]
    private var source: FeiNotesDataSource!
    private let searchController = UISearchController(searchResultsController: nil)
    private var search: UISearchBar { searchController.searchBar }
    private lazy var compose: UIBarButtonItem = {
        let item = UIBarButtonItem(barButtonSystemItem: .compose,target: self,action: #selector(newNote))
        item.accessibilityIdentifier = "fei.new"; return item
    }()
    private lazy var closeSearch: UIBarButtonItem = {
        let item = UIBarButtonItem(barButtonSystemItem: .close,target: self,action: #selector(endSearch))
        item.accessibilityIdentifier = "fei.notes.search-close"; return item
    }()
    private var selectionLeft: [UIBarButtonItem]?
    private var selectionRight: [UIBarButtonItem]?
    private lazy var selectAllItem = UIBarButtonItem(title: nil,style: .plain,target: self,action: #selector(selectAllDocuments))
    private lazy var selectionDone = UIBarButtonItem(title: nil,style: .done,target: self,action: #selector(closeSelection))
    private lazy var copySelection = UIBarButtonItem(image: UIImage(systemName: "doc.on.doc"),style: .plain,target: self,action: #selector(copySelected))
    private lazy var moveSelection = UIBarButtonItem(image: UIImage(systemName: "folder"),style: .plain,target: self,action: #selector(moveSelected))
    private lazy var deleteSelection = UIBarButtonItem(image: UIImage(systemName: "trash"),style: .plain,target: self,action: #selector(deleteSelected))
    private let tabs = UISegmentedControl(items: ["","",""])
    private let header = UIView()
    private let headerStack = UIStackView()
    private var collapsedPinned = Set(UserDefaults.standard.stringArray(forKey: "FeiCollapsedPinnedScopes") ?? [])
    private var isBusy = false
    private weak var pinnedHeaderButton: UIButton?
    var isSelecting: Bool { listing?.selection != nil }
    private var pendingSelection = false
    private var labels: [String:String] = [:]
    private let count = UILabel()
    private let empty = UILabel()
    private let notice = UIVisualEffectView(effect: UIBlurEffect(style: .systemMaterial))
    private let noticeText = UILabel()
    private let noticeIcon = UIImageView()
    private var palette = FeiMobileTheme.system
    private struct SearchOrigin { let scope: String; let query: String; let anchor: ListAnchor }
    private var searchOrigin: SearchOrigin?
    private var restoreOffset: SearchOrigin?
    private var openingResult = false
    private var resultToOpen: (id: String,scope: String)?
    private var resultReturn: SearchOrigin?
    private var searchLayout: String?

    init(coordinator: FeiNavigationCoordinator) {
        self.coordinator = coordinator
        super.init(nibName: nil,bundle: nil)
    }
    required init?(coder: NSCoder) { fatalError("init(coder:) is unsupported") }
    private func interfaceFont(_ style: UIFont.TextStyle) -> UIFont {
        let base = UIFont.preferredFont(forTextStyle: style)
        let delta: CGFloat = listing?.interface_size == "large" ? 2 : listing?.interface_size == "small" ? -1 : 0
        return base.withSize(base.pointSize+delta)
    }
    override func viewDidLoad() {
        super.viewDidLoad()
        tableView.translatesAutoresizingMaskIntoConstraints = false
        tableView.delegate = self
        tableView.dragInteractionEnabled = false
        tableView.accessibilityIdentifier = "fei.notes.list"
        tableView.register(UITableViewCell.self,forCellReuseIdentifier: "note")
        tableView.estimatedRowHeight = 86; tableView.rowHeight = UITableView.automaticDimension
        tableView.keyboardDismissMode = .interactive; tableView.alwaysBounceVertical = true
        source = FeiNotesDataSource(tableView: tableView) { [weak self] table,index,id in
            guard let self else { return nil }
            guard let row=self.rows[id] else { return nil }
            let cell = table.dequeueReusableCell(withIdentifier: "note",for: index)
            var content = cell.defaultContentConfiguration()
            content.text = row.title; content.textProperties.numberOfLines = 1; content.textProperties.font = self.interfaceFont(.headline)
            content.textProperties.color = self.palette.textColor
            content.textProperties.adjustsFontForContentSizeCategory = true
            content.secondaryText = row.summary
            content.secondaryTextProperties.font = self.interfaceFont(.subheadline)
            content.secondaryTextProperties.color = .secondaryLabel
            content.secondaryTextProperties.numberOfLines = 1
            content.secondaryTextProperties.lineBreakMode = .byTruncatingTail
            content.textToSecondaryTextVerticalPadding = 4
            content.secondaryTextProperties.adjustsFontForContentSizeCategory = true
            content.directionalLayoutMargins = NSDirectionalEdgeInsets(top: 12,leading: 16,bottom: 12,trailing: 16)
            content.image = row.folder ? UIImage(systemName: "folder") : nil
            content.imageProperties.tintColor = self.palette.folderColor
            cell.contentConfiguration = content; cell.backgroundColor = self.palette.rowColor
            cell.automaticallyUpdatesBackgroundConfiguration=false
            cell.configurationUpdateHandler = { [weak self] cell,state in
                guard let self,var content=cell.contentConfiguration as? UIListContentConfiguration else { return }
                let highlight=state.isSelected && !self.isSelecting
                content.textProperties.color=highlight ? self.palette.selectedTextColor : self.palette.textColor
                content.secondaryTextProperties.color=highlight ? self.palette.selectedTextColor.withAlphaComponent(0.75) : .secondaryLabel
                if self.isSelecting {
                    content.image=UIImage(systemName: state.isSelected ? "checkmark.circle.fill" : "circle")
                    content.imageProperties.reservedLayoutSize=CGSize(width: 24,height: 24)
                    content.imageProperties.tintColor=state.isSelected ? self.palette.accentColor : .tertiaryLabel
                } else { content.imageProperties.tintColor=highlight ? self.palette.selectedTextColor : self.palette.folderColor }
                var background=UIBackgroundConfiguration.listGroupedCell()
                background.backgroundColor=self.isSelecting && state.isSelected ? .tertiarySystemFill : highlight ? self.palette.selectedColor : self.palette.rowColor
                background.backgroundColorTransformer=nil
                cell.backgroundConfiguration=background; cell.contentConfiguration=content
            }
            cell.accessoryType = row.folder ? .disclosureIndicator : .none
            cell.showsReorderControl = !self.isSelecting
            cell.accessibilityIdentifier = "fei.note."+id
            return cell
        }
        source.movable = { [weak self] path in
            guard let self,!self.isSelecting,self.listing?.can_reorder == true,let id=self.source.itemIdentifier(for: path) else { return false }
            return self.coordinator?.rowItem(id)?.shared != true
        }
        source.moved = { [weak self] from,to in self?.moveRow(from: from,to: to) }
        search.delegate = self
        searchController.searchResultsUpdater = self; searchController.delegate = self
        searchController.obscuresBackgroundDuringPresentation = false
        searchController.hidesNavigationBarDuringPresentation = false
        search.accessibilityIdentifier = "fei.notes.search"
        search.searchTextField.accessibilityIdentifier = "fei.notes.search-field"
        search.searchTextField.adjustsFontForContentSizeCategory = true
        search.returnKeyType = .search
        tabs.apportionsSegmentWidthsByContent=true
        tabs.accessibilityIdentifier = "fei.notes.tabs"
        tabs.addTarget(self,action: #selector(changeTab),for: .valueChanged)
        count.font = .preferredFont(forTextStyle: .subheadline); count.adjustsFontForContentSizeCategory = true; count.textColor = .secondaryLabel
        count.accessibilityIdentifier = "fei.notes.count"
        headerStack.axis = .vertical; headerStack.spacing = 8
        headerStack.translatesAutoresizingMaskIntoConstraints = false; header.addSubview(headerStack)
        headerStack.addArrangedSubview(count); headerStack.addArrangedSubview(tabs)
        NSLayoutConstraint.activate([headerStack.topAnchor.constraint(equalTo: header.topAnchor,constant: 14),headerStack.leadingAnchor.constraint(equalTo: header.leadingAnchor,constant: 20),headerStack.trailingAnchor.constraint(equalTo: header.trailingAnchor,constant: -20),headerStack.bottomAnchor.constraint(equalTo: header.bottomAnchor,constant: -12),tabs.heightAnchor.constraint(greaterThanOrEqualToConstant: 32)])
        tableView.tableHeaderView = header
        empty.font = .preferredFont(forTextStyle: .body); empty.textColor = .secondaryLabel
        empty.adjustsFontForContentSizeCategory = true; empty.textAlignment = .center; empty.numberOfLines = 0; empty.lineBreakMode = .byWordWrapping
        let emptyBackground = UIView()
        empty.translatesAutoresizingMaskIntoConstraints = false; emptyBackground.addSubview(empty)
        NSLayoutConstraint.activate([empty.leadingAnchor.constraint(equalTo: emptyBackground.leadingAnchor,constant: 20),empty.trailingAnchor.constraint(equalTo: emptyBackground.trailingAnchor,constant: -20),empty.centerYAnchor.constraint(equalTo: emptyBackground.centerYAnchor)])
        tableView.backgroundView = emptyBackground
        view.addSubview(tableView)
        NSLayoutConstraint.activate([tableView.topAnchor.constraint(equalTo: view.topAnchor),tableView.leadingAnchor.constraint(equalTo: view.leadingAnchor),tableView.trailingAnchor.constraint(equalTo: view.trailingAnchor),tableView.bottomAnchor.constraint(equalTo: view.keyboardLayoutGuide.topAnchor)])
        noticeText.font = .preferredFont(forTextStyle: .subheadline); noticeText.adjustsFontForContentSizeCategory = true
        noticeText.textColor = .label; noticeText.numberOfLines = 2
        noticeIcon.translatesAutoresizingMaskIntoConstraints=false; noticeIcon.contentMode = .scaleAspectFit
        let status=UIStackView(arrangedSubviews: [noticeIcon,noticeText]); status.axis = .horizontal; status.spacing=8; status.alignment = .center
        status.translatesAutoresizingMaskIntoConstraints=false; notice.contentView.addSubview(status)
        notice.layer.cornerRadius=22; notice.clipsToBounds=true; notice.translatesAutoresizingMaskIntoConstraints=false; notice.isHidden=true
        notice.accessibilityIdentifier="fei.notes.notice"; notice.isAccessibilityElement=true
        view.addSubview(notice)
        NSLayoutConstraint.activate([notice.topAnchor.constraint(equalTo: view.safeAreaLayoutGuide.topAnchor,constant: 8),notice.centerXAnchor.constraint(equalTo: view.centerXAnchor),notice.widthAnchor.constraint(lessThanOrEqualTo: view.widthAnchor,constant: -40),notice.heightAnchor.constraint(greaterThanOrEqualToConstant: 44),status.topAnchor.constraint(equalTo: notice.contentView.topAnchor,constant: 10),status.bottomAnchor.constraint(equalTo: notice.contentView.bottomAnchor,constant: -10),status.leadingAnchor.constraint(equalTo: notice.contentView.leadingAnchor,constant: 16),status.trailingAnchor.constraint(equalTo: notice.contentView.trailingAnchor,constant: -16),noticeIcon.widthAnchor.constraint(equalToConstant: 18),noticeIcon.heightAnchor.constraint(equalToConstant: 18)])
    }
    func applyTheme(_ theme: FeiMobileTheme) {
        loadViewIfNeeded()
        let changed = palette != theme; palette = theme
        view.tintColor = theme.accentColor; view.backgroundColor = theme.listColor
        pinnedHeaderButton?.tintColor = theme.textColor
        tableView.backgroundColor = theme.listColor
        search.tintColor = theme.accentColor
        compose.tintColor = theme.accentColor
        if changed {
            var snapshot = source.snapshot(); snapshot.reconfigureItems(snapshot.itemIdentifiers)
            source.apply(snapshot,animatingDifferences: false)
        }
    }
    func update(_ next: FeiNavigationState.Listing,labels: [String:String],busy: Bool) {
        loadViewIfNeeded()
        let labelsChanged = self.labels != labels
        self.labels = labels; isBusy=busy
        let previous = listing
        if browsesWorkspace,let previous,previous.scope != next.scope { browserOffsets[previous.scope] = captureAnchor() }
        listing = next
        let ordered = listingSnapshot(next).itemIdentifiers
        if var pending = pendingMove {
            if pending.scope != next.scope || ordered == pending.order && next.can_reorder || pending.writing && next.can_reorder || next.notice != nil && next.notice != previous?.notice { pendingMove = nil }
            else { pending.writing = pending.writing || !next.can_reorder; pendingMove = pending }
        }
        if let previous,previous.scope != next.scope {
            searchOrigin = nil; restoreOffset = nil
            resultReturn = nil
            searchController.isActive = false
        }
        tableView.rowHeight = ceil(interfaceFont(.headline).lineHeight)+ceil(interfaceFont(.subheadline).lineHeight)+28
        search.searchTextField.font = interfaceFont(.body)
        empty.font = interfaceFont(.body); noticeText.font = interfaceFont(.subheadline)
        rows = Dictionary(uniqueKeysWithValues: next.rows.map { ($0.id,$0) })
        let key = !next.query.isEmpty ? "result-count" : next.rows.contains(where: { $0.folder }) ? "item-count" : "note-count"
        count.text = String(format: labels[key] ?? "%d items",next.rows.count)
        search.placeholder = labels["search"]; search.accessibilityLabel = labels["search"]
        if !searchController.isActive,search.text != next.query { search.text = next.query }
        compose.accessibilityLabel = labels["new"]
        compose.isEnabled = !busy && coordinator?.allowsCompose == true
        tabs.setTitle(labels["all-notes"],forSegmentAt: 0); tabs.setTitle(labels["favorites"],forSegmentAt: 1); tabs.setTitle(labels["sharing"],forSegmentAt: 2)
        tabs.selectedSegmentIndex = next.tab; tabs.isHidden = browsesWorkspace || !next.tabs || isSelecting
        empty.text = next.rows.isEmpty ? next.empty : nil
        tableView.isUserInteractionEnabled = !busy && pendingMove == nil; search.isUserInteractionEnabled = !busy && pendingMove == nil
        if pendingMove == nil && (previous?.rows != next.rows || previous?.interface_size != next.interface_size || previous?.shared_count != next.shared_count || previous?.selection?.serial != next.selection?.serial || labelsChanged || source.snapshot().itemIdentifiers != ordered) {
            var snapshot = listingSnapshot(next)
            snapshot.reconfigureItems(Array(Set(source.snapshot().itemIdentifiers).intersection(snapshot.itemIdentifiers)))
            source.apply(snapshot,animatingDifferences: previous != nil && previous?.scope == next.scope && previous?.query == next.query && view.window != nil) { [weak self] in self?.restoreSearchAnchor(); self?.restoreSelection() }
        }
        if browsesWorkspace,previous?.scope != next.scope {
            restoreOffset = SearchOrigin(scope: next.scope,query: next.query,anchor: browserOffsets[next.scope] ?? ListAnchor(id: nil,delta: 0,fallback: CGPoint(x: 0,y: -tableView.adjustedContentInset.top)))
        }
        restoreSelection()
        let message = next.notice
        noticeText.text=message; notice.accessibilityLabel=message; notice.isHidden=message == nil
        let danger=next.notice_kind == "error"
        noticeIcon.image=UIImage(systemName: danger ? "exclamationmark.circle.fill" : next.notice_kind == "success" ? "checkmark.circle.fill" : "info.circle.fill")
        noticeIcon.tintColor=danger ? .systemRed : palette.accentColor
        if message != nil && message != previous?.notice { UIAccessibility.post(notification: .announcement,argument: message) }
        pinnedHeaderButton?.configuration?.title=labels["pinned"]
        updateSelection(previous: previous?.selection)
        view.setNeedsLayout()
    }
    func selectNote(_ id: String?) { selectedNote = id; restoreSelection() }
    private func restoreSelection() {
        guard isViewLoaded else { return }
        if let selection=listing?.selection {
            for path in tableView.indexPathsForSelectedRows ?? [] { tableView.deselectRow(at: path,animated: false) }
            for id in selection.selected { if let path=source.indexPath(for: id) { tableView.selectRow(at: path,animated: false,scrollPosition: .none) } }
            return
        }
        guard browsesWorkspace else { return }
        for path in tableView.indexPathsForSelectedRows ?? [] { tableView.deselectRow(at: path,animated: false) }
        if let selectedNote,let path = source.indexPath(for: selectedNote) { tableView.selectRow(at: path,animated: false,scrollPosition: .none) }
    }
    func deactivate() {
        if tableView.isEditing && !isSelecting { endReordering() }
        searchOrigin = nil; searchController.isActive = false
        parent?.navigationItem.searchController = nil
        view.endEditing(true); view.isHidden = true; notice.isHidden = true
    }
    private func updateCloudResumeProbe() {
        #if FEI_MOBILE_QA
        tableView.accessibilityValue = "cloudResume=\(coordinator?.readyForCloudResume == true) cloudPending=\(coordinator?.cloudStartPending == true)"
        #endif
    }
    func configureSearch(in controller: UIViewController,expanded: Bool) {
        guard isViewLoaded,!view.isHidden,controller.navigationController?.topViewController === controller else { return }
        if let selection=listing?.selection {
            controller.navigationItem.searchController=nil
            controller.navigationItem.title=String(format: labels["selected-count"] ?? "%d selected",selection.selected.count)
            selectAllItem.title=labels[selection.selected.count == min(selection.ids.count,selection.limit) && !selection.ids.isEmpty ? "deselect-all" : "select-all"]
            selectAllItem.accessibilityIdentifier="fei.notes.selection.all"
            selectionDone.title=labels["done"]; selectionDone.accessibilityIdentifier="fei.notes.selection.done"
            controller.navigationItem.leftBarButtonItems=[selectAllItem]; controller.navigationItem.rightBarButtonItems=[selectionDone]
            let enabled = !isBusy && !selection.moving
            selectionDone.isEnabled=enabled; selectAllItem.isEnabled=enabled && !selection.ids.isEmpty
            for (item,id,key) in [(copySelection,"copy","duplicate"),(moveSelection,"move","move"),(deleteSelection,"delete","delete")] {
                item.accessibilityIdentifier="fei.notes.selection."+id; item.accessibilityLabel=labels[key]
                item.isEnabled=enabled && !selection.selected.isEmpty && (id != "move" || selection.can_move)
            }
            deleteSelection.tintColor = .systemRed
            controller.toolbarItems=[copySelection,.flexibleSpace(),moveSelection,.flexibleSpace(),deleteSelection]
            controller.navigationController?.setToolbarHidden(false,animated: false)
            updateCloudResumeProbe(); searchLayout=nil; return
        }
        if listing?.kind == "trash" {
            controller.navigationItem.searchController = nil; controller.toolbarItems = nil
            controller.navigationController?.setToolbarHidden(true,animated: false)
            tabs.isHidden = true
            return
        }
        controller.navigationItem.title=browsesWorkspace ? coordinator?.browserTitle : coordinator?.listingTitle
        controller.navigationItem.largeTitleDisplayMode = .always
        controller.navigationController?.navigationBar.prefersLargeTitles = true
        controller.definesPresentationContext = true
        if controller.navigationItem.searchController !== searchController { controller.navigationItem.searchController = searchController }
        controller.navigationItem.hidesSearchBarWhenScrolling = expanded
        let compactToolbar: Bool
        if #available(iOS 26.0,*) {
            compactToolbar = !expanded
            let placement: UINavigationItem.SearchBarPlacement = expanded ? .stacked : .integrated
            if controller.navigationItem.preferredSearchBarPlacement != placement { controller.navigationItem.preferredSearchBarPlacement = placement }
            if controller.navigationItem.searchBarPlacementAllowsToolbarIntegration != !expanded { controller.navigationItem.searchBarPlacementAllowsToolbarIntegration = !expanded }
        } else {
            compactToolbar = !expanded
            if controller.navigationItem.preferredSearchBarPlacement != .stacked { controller.navigationItem.preferredSearchBarPlacement = .stacked }
        }
        let hasClose = controller.navigationItem.rightBarButtonItems?.contains(where: { $0 === closeSearch }) == true
        if expanded && searchController.isActive && !hasClose {
            controller.navigationItem.rightBarButtonItems = [closeSearch]+(controller.navigationItem.rightBarButtonItems ?? [])
        } else if (!expanded || !searchController.isActive) && hasClose {
            controller.navigationItem.rightBarButtonItems?.removeAll(where: { $0 === closeSearch })
        }
        let showCompose = !searchController.isActive && listing?.kind != "shared"
        updateCloudResumeProbe()
        let canCompose = coordinator?.allowsCompose == true && showCompose
        compose.isEnabled = canCompose
        controller.navigationItem.rightBarButtonItems?.filter { $0.accessibilityIdentifier == "fei.new" }.forEach { $0.isHidden = searchController.isActive }
        let key = "\(compactToolbar)-\(showCompose)"
        if key != searchLayout {
            searchLayout = key
            if #available(iOS 26.0,*),compactToolbar {
                controller.toolbarItems = showCompose ? [controller.navigationItem.searchBarPlacementBarButtonItem,.flexibleSpace(),compose] : [controller.navigationItem.searchBarPlacementBarButtonItem]
            } else { controller.toolbarItems = compactToolbar && showCompose ? [.flexibleSpace(),compose] : [] }
        }
        if controller.navigationController?.transitionCoordinator == nil,controller.navigationController?.isToolbarHidden != !compactToolbar { controller.navigationController?.setToolbarHidden(!compactToolbar,animated: false) }
    }
    private func updateSelection(previous: FeiNavigationState.Listing.Selection?) {
        if isSelecting {
            if previous == nil {
                if tableView.isEditing { endReordering() }
                selectionLeft=parent?.navigationItem.leftBarButtonItems; selectionRight=parent?.navigationItem.rightBarButtonItems
                tableView.allowsMultipleSelection=true
                tableView.setEditing(false,animated: false)
            }
            restoreSelection()
        } else if previous != nil {
            tableView.setEditing(false,animated: false); tableView.allowsMultipleSelection=false
            parent?.navigationItem.leftBarButtonItems=selectionLeft; parent?.navigationItem.rightBarButtonItems=selectionRight
            selectionLeft=nil; selectionRight=nil
            if let selected=tableView.indexPathForSelectedRow { tableView.deselectRow(at: selected,animated: false) }
        }
    }
    func beginSelection() {
        guard let listing,!isBusy,!isSelecting else { return }
        view.endEditing(true)
        if searchController.isActive {
            pendingSelection=true; searchOrigin=nil; resultReturn=nil; searchController.isActive=false
        } else { coordinator?.send("batch-open",scope: listing.scope) }
    }
    private func batch(_ value: String) { guard let selection=listing?.selection else { return }; coordinator?.send("batch-action",value: value,serial: selection.serial) }
    @objc private func selectAllDocuments() { batch("all") }
    @objc private func closeSelection() { guard let selection=listing?.selection else { return }; coordinator?.send("batch-close",serial: selection.serial) }
    @objc private func copySelected() { batch("copy") }
    @objc private func moveSelected() { batch("move") }
    @objc private func deleteSelected() { batch("delete") }
    func tableView(_ tableView: UITableView,didDeselectRowAt indexPath: IndexPath) { if isSelecting,let id=source.itemIdentifier(for: indexPath) { batch("select:"+id) } }
    private func refreshSearchControls() {
        guard let parent else { return }
        configureSearch(in: parent,expanded: browsesWorkspace && splitViewController?.isCollapsed == false)
    }
    override func viewDidLayoutSubviews() {
        super.viewDidLayoutSubviews()
        let size = headerStack.systemLayoutSizeFitting(CGSize(width: max(0,tableView.bounds.width-40),height: UIView.layoutFittingCompressedSize.height),withHorizontalFittingPriority: .required,verticalFittingPriority: .fittingSizeLevel)
        let height = ceil(size.height)+26
        if header.frame.size != CGSize(width: tableView.bounds.width,height: height) {
            header.frame.size = CGSize(width: tableView.bounds.width,height: height); tableView.tableHeaderView = header
        }
        restoreSearchAnchor()
    }
    override func viewDidAppear(_ animated: Bool) {
        super.viewDidAppear(animated)
        if let origin = resultReturn,origin.scope == listing?.scope,origin.query == listing?.query {
            restoreOffset = origin; restoreSearchAnchor()
        }
        resultReturn = nil
        refreshSearchControls()
    }

    func tableView(_ tableView: UITableView,didSelectRowAt indexPath: IndexPath) {
        guard let id = source.itemIdentifier(for: indexPath),let listing else { return }
        if isSelecting { batch("select:"+id); return }
        if !browsesWorkspace { tableView.deselectRow(at: indexPath,animated: true) }
        if listing.kind == "trash" { coordinator?.showItemActions(id); return }
        if searchController.isActive {
            resultReturn = SearchOrigin(scope: listing.scope,query: listing.query,anchor: captureAnchor())
            resultToOpen = (id,listing.scope); openingResult = true; searchController.isActive = false
        } else { view.endEditing(true); coordinator?.send(browsesWorkspace ? "browse-open" : "list-open",id: id,scope: listing.scope) }
    }
    func willPresentSearchController(_ searchController: UISearchController) {
        if tableView.isEditing && !isSelecting { endReordering() }
        resultReturn = nil
        if let listing,searchOrigin == nil { searchOrigin = SearchOrigin(scope: listing.scope,query: listing.query,anchor: captureAnchor()) }
        openingResult = false; refreshSearchControls()
    }
    func searchBar(_ searchBar: UISearchBar,textDidChange searchText: String) {
        // UIKit can clear an inactive field before presenting Search again after pop.
        // The text-change delegate also covers this transition; the results updater
        // intentionally ignores inactive presentation updates.
        guard !openingResult,let listing,(searchText.isEmpty || searchBar.searchTextField.markedTextRange == nil) else { return }
        resultReturn = nil
        commitSearchClear(searchText,in: listing.scope)
        coordinator?.send(browsesWorkspace ? "browse-search" : "list-search",value: searchText,scope: listing.scope)
    }
    func updateSearchResults(for searchController: UISearchController) {
        guard searchController.isActive,!openingResult,let listing,search.searchTextField.markedTextRange == nil else { return }
        commitSearchClear(search.text ?? "",in: listing.scope)
        coordinator?.send(browsesWorkspace ? "browse-search" : "list-search",value: search.text ?? "",scope: listing.scope)
    }
    private func commitSearchClear(_ text: String,in scope: String) {
        guard text.isEmpty else { return }
        // Clear is committed, even if UIKit briefly re-presents Search after pop.
        // Closing must not roll it back to the query captured before the clear.
        if let origin = searchOrigin,origin.scope == scope {
            searchOrigin = SearchOrigin(scope: scope,query: "",anchor: origin.anchor)
        }
        resultReturn = nil; restoreOffset = nil
    }
    func didPresentSearchController(_ searchController: UISearchController) { refreshSearchControls(); coordinator?.searchPresentationDidEnd() }
    func didDismissSearchController(_ searchController: UISearchController) {
        if !openingResult,let origin = searchOrigin,origin.scope == listing?.scope {
            restoreOffset = origin; search.text = origin.query
            coordinator?.send(browsesWorkspace ? "browse-search" : "list-search",value: origin.query,scope: origin.scope)
            restoreSearchAnchor()
        }
        let target = openingResult ? resultToOpen : nil
        searchOrigin = nil; openingResult = false; resultToOpen = nil; refreshSearchControls()
        if let target,listing?.scope == target.scope { coordinator?.send(browsesWorkspace ? "browse-open" : "list-open",id: target.id,scope: target.scope) }
        if pendingSelection,let listing { pendingSelection=false; coordinator?.send("batch-open",scope: listing.scope) }
        coordinator?.searchPresentationDidEnd()
    }
    @objc private func endSearch() { searchController.isActive = false }
    func searchBarCancelButtonClicked(_ searchBar: UISearchBar) { endSearch() }
    private func captureAnchor() -> ListAnchor {
        let first = tableView.indexPathsForVisibleRows?.sorted().first
        let id = first.flatMap { source.itemIdentifier(for: $0) }
        let top = first.map { tableView.rectForRow(at: $0).minY } ?? tableView.contentOffset.y
        return ListAnchor(id: id,delta: tableView.contentOffset.y-top,fallback: tableView.contentOffset)
    }
    private func restoreSearchAnchor() {
        guard let origin = restoreOffset,origin.scope == listing?.scope,origin.query == listing?.query else { return }
        restoreOffset = nil; tableView.layoutIfNeeded()
        let minimum = -tableView.adjustedContentInset.top
        let maximum = max(minimum,tableView.contentSize.height-tableView.bounds.height+tableView.adjustedContentInset.bottom)
        let target = origin.anchor.id.flatMap { source.indexPath(for: $0) }.map { tableView.rectForRow(at: $0).minY+origin.anchor.delta } ?? origin.anchor.fallback.y
        tableView.setContentOffset(CGPoint(x: origin.anchor.fallback.x,y: min(maximum,max(minimum,target))),animated: false)
    }
    func searchBarSearchButtonClicked(_ searchBar: UISearchBar) { searchBar.resignFirstResponder() }
    @objc private func newNote() { guard let listing else { return }; view.endEditing(true); coordinator?.composeNote(listingScope: listing.scope) }
    @objc private func changeTab() {
        guard let listing else { return }; coordinator?.send("list-tab",value: String(tabs.selectedSegmentIndex),scope: listing.scope)
    }
    func tableView(_ tableView: UITableView,contextMenuConfigurationForRowAt indexPath: IndexPath,point: CGPoint) -> UIContextMenuConfiguration? {
        guard !isSelecting,let id = source.itemIdentifier(for: indexPath) else { return nil }
        return UIContextMenuConfiguration(identifier: id as NSString,previewProvider: nil) { [weak self] _ in self?.coordinator?.itemMenu(id,browsingWorkspace: self?.browsesWorkspace == true) }
    }
    private func listingSnapshot(_ listing: FeiNavigationState.Listing) -> NSDiffableDataSourceSnapshot<Int,String> {
        var snapshot=NSDiffableDataSourceSnapshot<Int,String>()
        let pinned=listing.rows.filter { $0.pinned && !$0.folder }.map(\.id)
        if !pinned.isEmpty {
            snapshot.appendSections([0])
            if !collapsedPinned.contains(listing.scope) || !listing.query.isEmpty { snapshot.appendItems(pinned,toSection: 0) }
        }
        snapshot.appendSections([1]); snapshot.appendItems(listing.rows.filter { !$0.pinned || $0.folder }.map(\.id),toSection: 1)
        return snapshot
    }
    func tableView(_ tableView: UITableView,heightForRowAt indexPath: IndexPath) -> CGFloat { tableView.rowHeight }
    func tableView(_ tableView: UITableView,heightForHeaderInSection section: Int) -> CGFloat { source.snapshot().sectionIdentifiers[section]==0 ? 44 : .leastNormalMagnitude }
    func tableView(_ tableView: UITableView,viewForHeaderInSection section: Int) -> UIView? {
        guard source.snapshot().sectionIdentifiers[section]==0,let listing else { return nil }
        let container=UIView(), button=UIButton(type: .system)
        var config=UIButton.Configuration.plain(); config.title=labels["pinned"]
        config.image=UIImage(systemName: collapsedPinned.contains(listing.scope) ? "chevron.right" : "chevron.down")
        config.imagePlacement = .trailing; config.imagePadding=10; config.contentInsets = .zero
        config.titleTextAttributesTransformer=UIConfigurationTextAttributesTransformer { incoming in var out=incoming; out.font=UIFont.preferredFont(forTextStyle: .headline); return out }
        pinnedHeaderButton=button
        button.configuration=config; button.contentHorizontalAlignment = .fill; button.tintColor=palette.textColor
        button.accessibilityIdentifier="fei.notes.pinned-header"; button.accessibilityValue=collapsedPinned.contains(listing.scope) ? "collapsed" : "expanded"
        button.addTarget(self,action: #selector(togglePinned),for: .touchUpInside); button.translatesAutoresizingMaskIntoConstraints=false
        container.addSubview(button); NSLayoutConstraint.activate([button.topAnchor.constraint(equalTo: container.topAnchor),button.bottomAnchor.constraint(equalTo: container.bottomAnchor),button.leadingAnchor.constraint(equalTo: container.leadingAnchor),button.trailingAnchor.constraint(equalTo: container.trailingAnchor)])
        return container
    }
    @objc private func togglePinned() {
        guard let listing else { return }
        if collapsedPinned.contains(listing.scope) { collapsedPinned.remove(listing.scope) } else { collapsedPinned.insert(listing.scope) }
        UserDefaults.standard.set(Array(collapsedPinned),forKey: "FeiCollapsedPinnedScopes")
        source.apply(listingSnapshot(listing),animatingDifferences: !UIAccessibility.isReduceMotionEnabled) { [weak self] in self?.restoreSelection() }
        pinnedHeaderButton?.configuration?.image=UIImage(systemName: collapsedPinned.contains(listing.scope) ? "chevron.right" : "chevron.down")
        pinnedHeaderButton?.accessibilityValue=collapsedPinned.contains(listing.scope) ? "collapsed" : "expanded"
    }
    private func swipe(_ keys: [(String,String,UIColor)],id: String) -> UISwipeActionsConfiguration? {
        guard let listing,!isSelecting else { return nil }
        let config = UISwipeActionsConfiguration(actions: keys.map { key,symbol,color in
            let item = coordinator?.rowItem(id)
            let labelKey = key == "delete" && item?.shared == true ? (item?.owned == true ? "delete-shared" : "leave-shared") : key == "pin" && item?.pinned == true ? "unpin" : key == "star" && item?.starred == true ? "unstar" : key
            let action = UIContextualAction(style: key == "delete" || key == "purge" ? .destructive : .normal,title: labels[labelKey]) { [weak self] _,_,done in
                guard let self else { done(false); return }
                self.coordinator?.rowAction(key,id: id,scope: listing.scope,browser: self.browsesWorkspace); done(true)
            }
            action.image = UIImage(systemName: symbol); action.backgroundColor = color; return action
        })
        config.performsFirstActionWithFullSwipe = false; return config
    }
    func tableView(_ tableView: UITableView,leadingSwipeActionsConfigurationForRowAt indexPath: IndexPath) -> UISwipeActionsConfiguration? {
        guard let id=source.itemIdentifier(for: indexPath),let row=rows[id] else { return nil }
        if listing?.kind == "trash" { return swipe([("restore","arrow.uturn.backward",.systemGreen)],id: id) }
        return row.folder ? nil : swipe([("pin","pin",palette.accentColor),("star","star",.systemOrange)],id: id)
    }
    func tableView(_ tableView: UITableView,trailingSwipeActionsConfigurationForRowAt indexPath: IndexPath) -> UISwipeActionsConfiguration? {
        guard let id=source.itemIdentifier(for: indexPath) else { return nil }
        if listing?.kind == "trash" { return swipe([("purge","trash",.systemRed)],id: id) }
        return swipe(coordinator?.rowItem(id)?.shared == true ? [("delete","trash",.systemRed)] : [("delete","trash",.systemRed),("move","folder",.systemBlue)],id: id)
    }
    func tableView(_ tableView: UITableView,targetIndexPathForMoveFromRowAt sourceIndexPath: IndexPath,toProposedIndexPath proposedDestinationIndexPath: IndexPath) -> IndexPath {
        sourceIndexPath.section == proposedDestinationIndexPath.section ? proposedDestinationIndexPath : sourceIndexPath
    }
    private func moveRow(from: IndexPath,to destination: IndexPath) {
        guard let listing,listing.can_reorder,from.section==destination.section,from != destination,let id=source.itemIdentifier(for: from) else { return }
        var snapshot=source.snapshot()
        let section=snapshot.sectionIdentifiers[from.section]
        var items=snapshot.itemIdentifiers(inSection: section); items.removeAll { $0==id }
        let index=min(destination.row,items.count), before=index<items.count ? items[index] : nil
        snapshot.deleteItems([id])
        if let before { snapshot.insertItems([id],beforeItem: before) } else { snapshot.appendItems([id],toSection: section) }
        pendingMove = PendingMove(scope: listing.scope,order: snapshot.itemIdentifiers)
        tableView.isUserInteractionEnabled = false
        source.apply(snapshot,animatingDifferences: false)
        coordinator?.send("list-reorder",value: before ?? "",id: id,scope: listing.scope)
    }
    func beginReordering() {
        guard listing?.can_reorder == true,let parent,!tableView.isEditing else { return }
        view.endEditing(true)
        reorderButtons = parent.navigationItem.rightBarButtonItems
        let done=UIBarButtonItem(title: labels["done"],style: .done,target: self,action: #selector(endReordering))
        done.accessibilityIdentifier = "fei.notes.reorder.done"
        parent.navigationItem.rightBarButtonItems = [done]
        tableView.setEditing(true,animated: !UIAccessibility.isReduceMotionEnabled)
        updateCloudResumeProbe()
    }
    @objc private func endReordering() {
        tableView.setEditing(false,animated: !UIAccessibility.isReduceMotionEnabled)
        parent?.navigationItem.rightBarButtonItems = reorderButtons; reorderButtons = nil
        updateCloudResumeProbe()
    }
    func tableView(_ tableView: UITableView,editingStyleForRowAt indexPath: IndexPath) -> UITableViewCell.EditingStyle { isSelecting ? .delete : .none }
    func tableView(_ tableView: UITableView,shouldIndentWhileEditingRowAt indexPath: IndexPath) -> Bool { false }
    func anchor(for id: String) -> CGRect? {
        guard let index = source.indexPath(for: id) else { return nil }; return tableView.convert(tableView.rectForRow(at: index),to: parent?.view)
    }
}
