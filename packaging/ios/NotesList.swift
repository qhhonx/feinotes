import UIKit

@MainActor
final class FeiToastView: UIVisualEffectView {
    let message = UILabel()
    private let icon = UIImageView()

    init(id: String) {
        super.init(effect: UIBlurEffect(style: .systemMaterial))
        message.font = .preferredFont(forTextStyle: .subheadline)
        message.adjustsFontForContentSizeCategory = true
        message.numberOfLines = 0
        icon.contentMode = .scaleAspectFit
        let content = UIStackView(arrangedSubviews: [icon,message])
        content.alignment = .center
        content.spacing = 8
        content.translatesAutoresizingMaskIntoConstraints = false
        contentView.addSubview(content)
        layer.cornerRadius = 22
        clipsToBounds = true
        translatesAutoresizingMaskIntoConstraints = false
        isHidden = true
        isAccessibilityElement = true
        accessibilityIdentifier = id
        isUserInteractionEnabled = false
        NSLayoutConstraint.activate([
            heightAnchor.constraint(greaterThanOrEqualToConstant: 44),
            content.topAnchor.constraint(equalTo: contentView.topAnchor,constant: 10),
            content.bottomAnchor.constraint(equalTo: contentView.bottomAnchor,constant: -10),
            content.leadingAnchor.constraint(equalTo: contentView.leadingAnchor,constant: 16),
            content.trailingAnchor.constraint(equalTo: contentView.trailingAnchor,constant: -16),
            icon.widthAnchor.constraint(equalToConstant: 18),
            icon.heightAnchor.constraint(equalToConstant: 18)
        ])
    }

    required init?(coder: NSCoder) { fatalError() }

    private var anchorInsets: (bottom: CGFloat?,top: CGFloat)?

    func attach(to host: UIView,bottomInset: CGFloat? = nil,topInset: CGFloat = 8) {
        if superview === host,anchorInsets?.bottom == bottomInset,anchorInsets?.top == topInset {
            host.bringSubviewToFront(self)
            return
        }
        anchorInsets = (bottomInset,topInset)
        removeFromSuperview()
        host.addSubview(self)
        let vertical = bottomInset.map { bottomAnchor.constraint(equalTo: host.keyboardLayoutGuide.topAnchor,constant: -$0) }
            ?? topAnchor.constraint(equalTo: host.safeAreaLayoutGuide.topAnchor,constant: topInset)
        NSLayoutConstraint.activate([
            vertical,
            centerXAnchor.constraint(equalTo: host.safeAreaLayoutGuide.centerXAnchor),
            widthAnchor.constraint(lessThanOrEqualTo: host.safeAreaLayoutGuide.widthAnchor,constant: -40)
        ])
    }

    func update(_ text: String?,kind: String?,accent: UIColor) {
        message.text = text
        accessibilityLabel = text
        isHidden = text == nil
        icon.image = UIImage(systemName: kind == "error" ? "exclamationmark.circle.fill"
            : kind == "success" ? "checkmark.circle.fill" : "info.circle.fill")
        icon.tintColor = kind == "error" ? .systemRed : accent
    }
}
import UIKit.UIGestureRecognizerSubclass

/// Gives ordinary pans to the table and stationary holds to UIKit's context menu.
private final class FeiRowReorderGesture: UIGestureRecognizer {
    private static let earliestLift: TimeInterval = 0.12
    private static let latestLift: TimeInterval = 0.55
    private var start: CGPoint?
    private var latest: CGPoint?
    private var startedAt: TimeInterval = 0

    override func touchesBegan(_ touches:Set<UITouch>,with event:UIEvent) {
        guard start == nil,touches.count == 1,let touch = touches.first else { state = .failed; return }
        start = touch.location(in: view?.superview)
        latest = start
        startedAt = touch.timestamp
        let timestamp = startedAt
        DispatchQueue.main.asyncAfter(deadline: .now() + Self.latestLift) { [weak self] in
            guard let self, self.start != nil, self.startedAt == timestamp, self.state == .possible else { return }
            self.state = .failed
        }
    }
    override func touchesMoved(_ touches:Set<UITouch>,with event:UIEvent) {
        guard let start,let touch = touches.first else { return }
        let point = touch.location(in: view?.superview)
        latest = point
        if state == .began || state == .changed { state = .changed; return }
        guard state == .possible else { return }
        let dx = abs(point.x-start.x),dy = abs(point.y-start.y)
        guard max(dx,dy) >= 6 else { return }
        let hold = touch.timestamp-startedAt
        state = hold >= Self.earliestLift && hold < Self.latestLift && dy > dx ? .began : .failed
    }
    override func touchesEnded(_ touches:Set<UITouch>,with event:UIEvent) {
        if let touch = touches.first { latest = touch.location(in: view?.superview) }
        if state == .began || state == .changed { state = .ended }
        else if state == .possible { state = .failed }
    }
    override func touchesCancelled(_ touches:Set<UITouch>,with event:UIEvent) { state = state == .possible ? .failed : .cancelled }
    override func reset() { super.reset(); start = nil; latest = nil }
    func currentPoint(in target:UIView) -> CGPoint? { latest.map { target.convert($0,from: view?.superview) } }
    func originalPoint(in target:UIView) -> CGPoint? { start.map { target.convert($0,from: view?.superview) } }
}

@MainActor
private final class FeiNotesDataSource: UITableViewDiffableDataSource<Int,String> {
    var movable: ((IndexPath) -> Bool)?
    var moved: ((IndexPath,IndexPath) -> Void)?
    override func tableView(_ tableView: UITableView,canMoveRowAt indexPath: IndexPath) -> Bool { movable?(indexPath) == true }
    override func tableView(_ tableView: UITableView,moveRowAt sourceIndexPath: IndexPath,to destinationIndexPath: IndexPath) { moved?(sourceIndexPath,destinationIndexPath) }
}

@MainActor
final class FeiNotesListController: UIViewController, UITableViewDelegate, UISearchBarDelegate, UISearchResultsUpdating, UISearchControllerDelegate, UIGestureRecognizerDelegate {
    let tableView = UITableView(frame: .zero,style: .insetGrouped)
    private weak var coordinator: FeiNavigationCoordinator?
    var browsesWorkspace = false
    private var selectedNote: String?
    var allowsCloudResume: Bool { !isSearching && !tableView.isEditing && !isSelecting && pendingMove == nil && !openingResult && !pendingSelection && rowDrag == nil }
    var allowsFolderBackGesture: Bool { !isSearching && !isSelecting && !tableView.isEditing && rowDrag == nil && pendingMove == nil }
    private var reorderButtons: [UIBarButtonItem]?
    private var isReordering = false
    private struct RowDrag {
        let id: String
        let scope: String
        let preview: UIView
        let origin: IndexPath
        let originalOrder: NSDiffableDataSourceSnapshot<Int,String>
        var destination: IndexPath?
    }
    private var rowDrag: RowDrag?
    #if FEI_MOBILE_QA
    private var observedEmptyInsertionSlot = false
    #endif
    private struct PendingMove { let scope:String; let order:[String]; var writing = false }
    private var pendingMove: PendingMove?
    private enum ListAnchor {
        case top
        case position(id: String?,delta: CGFloat,fallback: CGPoint)
    }
    private var browserOffsets: [String:ListAnchor] = [:]
    private var listing: FeiNavigationState.Listing?
    private var canCompose = false
    private var rows: [String:FeiNavigationState.Listing.Row] = [:]
    private var source: FeiNotesDataSource!
    private let searchController = UISearchController(searchResultsController: nil)
    private let compactSearch = UISearchBar()
    private var search: UISearchBar { usesLocalSearch ? compactSearch : searchController.searchBar }
    private var compactSearching = false
    private var isSearching: Bool { usesLocalSearch ? compactSearching : searchController.isActive }
    private let searchGlass = UIVisualEffectView()
    private let composeGlass = UIVisualEffectView()
    private let compactSearchRow = UIStackView()
    private let composeButton = UIButton(type: .system)
    private var compactSearchHeight: NSLayoutConstraint?
    private var listBottom: NSLayoutConstraint?
    private var listKeyboardBottom: NSLayoutConstraint?
    private var usesLocalSearch = false
    private lazy var compose: UIBarButtonItem = {
        let item = UIBarButtonItem(systemItem: .compose,primaryAction: nil)
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
    private weak var pinnedHeaderTitle: UILabel?
    private weak var pinnedHeaderArrow: UIImageView?
    var isSelecting: Bool { listing?.selection != nil }
    private var pendingSelection = false
    private var labels: [String:String] = [:]
    private let count = UILabel()
    private let empty = UILabel()
    private let notice = FeiToastView(id: "fei.notes.notice")
    private var palette = FeiMobileTheme.system
    private struct SearchOrigin { let scope: String; let query: String; let anchor: ListAnchor }
    private var searchOrigin: SearchOrigin?
    private var restoreOffset: SearchOrigin?
    private var openingResult = false
    private var resultToOpen: (id: String,scope: String)?
    private var resultReturn: SearchOrigin?
    private var searchLayout: String?

    init(coordinator: FeiNavigationCoordinator,theme: FeiMobileTheme) {
        self.coordinator = coordinator
        palette = theme
        super.init(nibName: nil,bundle: nil)
    }
    required init?(coder: NSCoder) { fatalError("init(coder:) is unsupported") }
    private func interfaceFont(_ style: UIFont.TextStyle) -> UIFont {
        let base = UIFont.preferredFont(forTextStyle: style)
        let delta: CGFloat = listing?.interface_size == "large" ? 2 : listing?.interface_size == "small" ? -1 : 0
        return base.withSize(base.pointSize+delta)
    }
    private func updateSearchTypography() {
        let field = search.searchTextField
        let font = interfaceFont(.body)
        if field.font != font { field.font = font }
        guard let title = labels["search"] else { return }
        let placeholder = NSAttributedString(string: title, attributes: [
            .font: font, .foregroundColor: UIColor.placeholderText
        ])
        if field.attributedPlaceholder != placeholder { field.attributedPlaceholder = placeholder }
    }
    private var disclosureSize: CGFloat { UIFontMetrics(forTextStyle: .body).scaledValue(for: 13) }
    private func disclosureImage(expanded: Bool = false) -> UIImage? {
        UIImage(systemName: expanded ? "chevron.down" : "chevron.forward",
            withConfiguration: UIImage.SymbolConfiguration(pointSize: disclosureSize,weight: .semibold))
    }

    private func leadingImage(for row: FeiNavigationState.Listing.Row,inTrash: Bool) -> UIImage? {
        guard row.folder || inTrash else { return nil }
        let size = UIFontMetrics(forTextStyle: .headline).scaledValue(for: 22)
        let configuration = UIImage.SymbolConfiguration(pointSize: size)
        guard let folder = UIImage(systemName: "folder",withConfiguration: configuration) else { return nil }
        let glyph = row.folder ? folder : UIImage(systemName: "doc.text",withConfiguration: configuration)
        guard let glyph else { return nil }
        let canvas = CGSize(width: folder.size.width,height: max(folder.size.height,glyph.size.height,size))
        return UIGraphicsImageRenderer(size: canvas).image { renderer in
            glyph.draw(at: CGPoint(x: (canvas.width - glyph.size.width) / 2,y: (canvas.height - glyph.size.height) / 2))
            guard row.shared_folder,
                  let people = UIImage(systemName: "person.2.fill",withConfiguration: UIImage.SymbolConfiguration(pointSize: size * 0.34)) else { return }
            let badge = CGRect(x: canvas.width - people.size.width,y: canvas.height - people.size.height,width: people.size.width,height: people.size.height)
            renderer.cgContext.setBlendMode(.clear)
            UIBezierPath(roundedRect: badge.insetBy(dx: -1,dy: -1),cornerRadius: 2).fill()
            renderer.cgContext.setBlendMode(.normal)
            people.draw(in: badge)
        }.withRenderingMode(.alwaysTemplate)
    }

    private func applyPinnedHeader() {
        guard let listing else { return }
        let collapsed = listing.query.isEmpty && collapsedPinned.contains(listing.scope)
        pinnedHeaderArrow?.image = disclosureImage(expanded: !collapsed)
        pinnedHeaderArrow?.tintColor = .secondaryLabel
        pinnedHeaderTitle?.text = labels["pinned"]
        pinnedHeaderTitle?.textColor = palette.textColor
        pinnedHeaderTitle?.font = interfaceFont(.headline)
        pinnedHeaderButton?.isEnabled = listing.query.isEmpty
        pinnedHeaderButton?.accessibilityLabel = labels["pinned"]
        pinnedHeaderButton?.accessibilityValue = collapsed ? "collapsed" : "expanded"
    }

    override func viewDidLoad() {
        super.viewDidLoad()
        NotificationCenter.default.addObserver(self,selector: #selector(contentSizeChanged),name: UIContentSizeCategory.didChangeNotification,object: nil)
        tableView.translatesAutoresizingMaskIntoConstraints = false
        tableView.contentInsetAdjustmentBehavior = .always
        tableView.delegate = self
        tableView.dragInteractionEnabled = false
        let reorderPress = FeiRowReorderGesture(target: self,action: #selector(reorderByTouch(_:)))
        reorderPress.cancelsTouchesInView = true
        reorderPress.delegate = self
        tableView.addGestureRecognizer(reorderPress)
        tableView.panGestureRecognizer.require(toFail: reorderPress)
        tableView.accessibilityIdentifier = "fei.notes.list"
        tableView.register(UITableViewCell.self,forCellReuseIdentifier: "note")
        tableView.estimatedRowHeight = 86; tableView.rowHeight = UITableView.automaticDimension
        tableView.keyboardDismissMode = .interactive; tableView.alwaysBounceVertical = true
        source = FeiNotesDataSource(tableView: tableView) { [weak self] table,index,id in
            guard let self else { return nil }
            guard let row=self.rows[id] else { return nil }
            let isTrash = self.listing?.kind == "trash"
            let cell = table.dequeueReusableCell(withIdentifier: "note",for: index)
            var content = cell.defaultContentConfiguration()
            content.text = row.title; content.textProperties.numberOfLines = 1; content.textProperties.font = self.interfaceFont(.headline)
            content.textProperties.color = self.palette.textColor
            content.textProperties.adjustsFontForContentSizeCategory = true
            content.secondaryText = row.summary.isEmpty ? nil : row.summary
            content.secondaryTextProperties.font = self.interfaceFont(.subheadline)
            content.secondaryTextProperties.color = .secondaryLabel
            content.secondaryTextProperties.numberOfLines = 1
            content.secondaryTextProperties.lineBreakMode = isTrash ? .byTruncatingMiddle : .byTruncatingTail
            content.textToSecondaryTextVerticalPadding = 4
            content.secondaryTextProperties.adjustsFontForContentSizeCategory = true
            content.directionalLayoutMargins = NSDirectionalEdgeInsets(top: 12,leading: 16,bottom: 12,trailing: 16)
            content.image = self.leadingImage(for: row,inTrash: isTrash)
            content.imageProperties.tintColor = row.folder ? self.palette.folderColor : self.palette.textColor
            cell.automaticallyUpdatesContentConfiguration = false
            cell.automaticallyUpdatesBackgroundConfiguration = false
            cell.selectionStyle = self.isSelecting || isTrash ? .none : .default
            cell.accessoryType = .none
            let baseContent = content
            let configure: (UITableViewCell,UICellConfigurationState) -> Void = { [weak self] cell,state in
                guard let self else { return }
                let isInsertionSlot = self.rowDrag?.id == id
                cell.accessibilityIdentifier = isInsertionSlot ? "fei.notes.insertion-slot" : "fei.note."+id
                cell.accessibilityElementsHidden = isInsertionSlot
                if isInsertionSlot {
                    cell.contentConfiguration = nil
                    cell.backgroundConfiguration = .clear()
                    cell.accessoryView = nil
                    return
                }
                var content = baseContent
                let highlight = state.isSelected && !self.isSelecting
                content.textProperties.color = highlight ? self.palette.selectedTextColor : self.palette.textColor
                content.secondaryTextProperties.color = highlight ? self.palette.selectedTextColor : .secondaryLabel
                content.imageProperties.tintColor = highlight ? self.palette.selectedTextColor : row.folder ? self.palette.folderColor : self.palette.textColor
                var background = UIBackgroundConfiguration.listGroupedCell()
                background.backgroundColor = highlight ? self.palette.selectedColor : self.palette.rowColor
                background.backgroundColorTransformer = nil
                if cell.backgroundConfiguration != background { cell.backgroundConfiguration = background }
                if cell.contentConfiguration as? UIListContentConfiguration != content { cell.contentConfiguration = content }

                cell.accessibilityValue = [isTrash ? self.labels[row.folder ? "trash-folder-kind" : "trash-document-kind"] : nil,
                    row.shared_folder ? self.labels["already-shared-folder"] : row.shared_badge ? self.labels["already-shared"] : nil,
                    row.unread_update ? self.labels["unread-update"] : nil].compactMap { $0 }.joined(separator: ", ")
                var image: UIImage?
                let tint: UIColor
                let size: CGSize
                if self.isSelecting && self.listing?.selection?.ids.contains(id) == true {
                    image = UIImage(systemName: state.isSelected ? "checkmark.circle.fill" : "circle",withConfiguration: UIImage.SymbolConfiguration(pointSize: 20))
                    if state.isSelected {
                        image = image?.applyingSymbolConfiguration(.init(paletteColors: [.white,self.palette.accentColor]))?.withRenderingMode(.alwaysOriginal)
                    }
                    tint = state.isSelected ? self.palette.accentColor : .tertiaryLabel
                    size = CGSize(width: 24,height: 24)
                } else if row.shared_badge && !self.isSelecting && !self.isReordering {
                    let pointSize = min(17,UIFontMetrics(forTextStyle: .footnote).scaledValue(for: 13,compatibleWith: cell.traitCollection))
                    image = UIImage(systemName: "person.2",withConfiguration: UIImage.SymbolConfiguration(pointSize: pointSize,weight: .regular))
                    tint = highlight ? self.palette.selectedTextColor : .secondaryLabel
                    size = CGSize(width: 30,height: 24)
                } else if row.folder && !isTrash && !self.isSelecting {
                    image = self.disclosureImage()
                    tint = highlight ? self.palette.selectedTextColor : .secondaryLabel
                    size = CGSize(width: max(16,self.disclosureSize),height: max(24,self.disclosureSize))
                } else if row.unread_update && !self.isSelecting && !self.isReordering {
                    image = UIImage(systemName: "circle.fill",withConfiguration: UIImage.SymbolConfiguration(pointSize: 7))
                    tint = .systemBlue
                    size = CGSize(width: 20,height: 24)
                } else {
                    cell.accessoryView = nil
                    return
                }
                let accessory = cell.accessoryView as? UIImageView ?? UIImageView()
                accessory.image = image; accessory.tintColor = tint
                accessory.contentMode = .center; accessory.frame.size = size
                if cell.accessoryView !== accessory { cell.accessoryView = accessory }
            }
            configure(cell,cell.configurationState)
            cell.configurationUpdateHandler = configure
            cell.showsReorderControl = self.isReordering && !self.isSelecting
            return cell
        }
        source.movable = { [weak self] path in
            guard let self,self.isReordering,!self.isSelecting,self.listing?.can_reorder == true else { return false }
            return self.canDragRow(path)
        }
        source.moved = { [weak self] from,to in self?.moveRow(from: from,to: to) }
        for bar in [searchController.searchBar, compactSearch] {
            bar.delegate = self
            bar.accessibilityIdentifier = "fei.notes.search"
            bar.searchTextField.accessibilityIdentifier = "fei.notes.search-field"
            bar.searchTextField.adjustsFontForContentSizeCategory = true
            bar.returnKeyType = .search
        }
        searchController.searchResultsUpdater = self; searchController.delegate = self
        searchController.obscuresBackgroundDuringPresentation = false
        searchController.hidesNavigationBarDuringPresentation = false
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
        // Behind the floating controls the list fills its entire viewport. The keyboard guide
        // can still sit at the safe-area edge when hidden, so use it only for a docked keyboard.
        let guide = view.keyboardLayoutGuide
        guide.usesBottomSafeArea = false
        guide.followsUndockedKeyboard = false
        let bottom = tableView.bottomAnchor.constraint(equalTo: view.bottomAnchor)
        listBottom = bottom
        listKeyboardBottom = tableView.bottomAnchor.constraint(equalTo: guide.topAnchor)
        NSLayoutConstraint.activate([
            tableView.topAnchor.constraint(equalTo: view.topAnchor),
            tableView.leadingAnchor.constraint(equalTo: view.leadingAnchor),
            tableView.trailingAnchor.constraint(equalTo: view.trailingAnchor),
            bottom
        ])
        notice.message.numberOfLines = 2
        if #available(iOS 26.0, *) { installCompactSearchRow() }
        notice.attach(to: view)
    }
    @available(iOS 26.0, *)
    private func installCompactSearchRow() {
        for glass in [searchGlass, composeGlass] {
            let effect = UIGlassEffect()
            effect.isInteractive = true
            glass.effect = effect
            glass.cornerConfiguration = .capsule()
            compactSearchRow.addArrangedSubview(glass)
        }
        compactSearch.searchBarStyle = .minimal
        compactSearch.setSearchFieldBackgroundImage(UIImage(), for: .normal)
        compactSearch.translatesAutoresizingMaskIntoConstraints = false
        let searchContent = searchGlass.contentView
        searchContent.addSubview(compactSearch)
        composeButton.setImage(UIImage(systemName: "square.and.pencil",
            withConfiguration: UIImage.SymbolConfiguration(pointSize: 22, weight: .regular)), for: .normal)
        composeButton.accessibilityIdentifier = "fei.new"
        composeButton.addAction(UIAction { [weak self] _ in self?.coordinator?.composeNote() }, for: .touchUpInside)
        composeButton.translatesAutoresizingMaskIntoConstraints = false
        composeGlass.contentView.addSubview(composeButton)
        compactSearchRow.spacing = 10
        compactSearchRow.translatesAutoresizingMaskIntoConstraints = false
        compactSearchRow.isHidden = true
        view.addSubview(compactSearchRow)
        let height = compactSearchRow.heightAnchor.constraint(equalToConstant: 48)
        compactSearchHeight = height
        let composeWidth = composeGlass.widthAnchor.constraint(equalTo: compactSearchRow.heightAnchor)
        composeWidth.priority = UILayoutPriority(999)
        let safe = view.safeAreaLayoutGuide
        let docked = compactSearchRow.bottomAnchor.constraint(equalTo: view.keyboardLayoutGuide.topAnchor, constant: -8)
        docked.priority = .defaultHigh
        // The list and search move together during a push, pop or cancelled swipe.
        NSLayoutConstraint.activate([
            compactSearchRow.leadingAnchor.constraint(equalTo: safe.leadingAnchor, constant: 26),
            compactSearchRow.trailingAnchor.constraint(equalTo: safe.trailingAnchor, constant: -26),
            compactSearchRow.bottomAnchor.constraint(lessThanOrEqualTo: safe.bottomAnchor, constant: -8),
            compactSearchRow.bottomAnchor.constraint(lessThanOrEqualTo: view.keyboardLayoutGuide.topAnchor, constant: -8),
            docked,
            composeWidth,
            compactSearch.leadingAnchor.constraint(equalTo: searchContent.leadingAnchor, constant: 5),
            compactSearch.trailingAnchor.constraint(equalTo: searchContent.trailingAnchor, constant: -5),
            compactSearch.topAnchor.constraint(equalTo: searchContent.topAnchor),
            compactSearch.bottomAnchor.constraint(equalTo: searchContent.bottomAnchor),
            composeButton.leadingAnchor.constraint(equalTo: composeGlass.contentView.leadingAnchor),
            composeButton.trailingAnchor.constraint(equalTo: composeGlass.contentView.trailingAnchor),
            composeButton.topAnchor.constraint(equalTo: composeGlass.contentView.topAnchor),
            composeButton.bottomAnchor.constraint(equalTo: composeGlass.contentView.bottomAnchor),
            height,
        ])
    }

    @available(iOS 26.0, *)
    private func updateCompactSearch(in controller: UIViewController, visible: Bool, showCompose: Bool) {
        if visible, !usesLocalSearch {
            searchController.isActive = false
            controller.navigationItem.searchController = nil
        } else if !visible, usesLocalSearch {
            compactSearch.searchTextField.resignFirstResponder()
            compactSearch.setShowsCancelButton(false, animated: false)
            compactSearching = false
        }
        usesLocalSearch = visible
        if !isSearching, let listing {
            if search.text != listing.query { search.text = listing.query }
            search.isUserInteractionEnabled = !isBusy && pendingMove == nil
        }
        search.accessibilityLabel = labels["search"]
        compactSearchRow.isHidden = !visible
        if composeGlass.isHidden == showCompose { composeGlass.isHidden = !showCompose }
        composeButton.isEnabled = !isBusy
        composeButton.accessibilityLabel = labels["new"]
        let height = max(48, ceil(interfaceFont(.body).lineHeight) + 16)
        compactSearchHeight?.constant = height
        let inset = visible ? height + 16 : 0
        if tableView.contentInset.bottom != inset {
            tableView.contentInset.bottom = inset
            tableView.verticalScrollIndicatorInsets.bottom = inset
        }
    }

    func applyTheme(_ theme: FeiMobileTheme) {
        loadViewIfNeeded()
        let changed = palette != theme
        palette = theme
        let background = browsesWorkspace ? theme.workspaceListColor : theme.listColor
        view.tintColor = theme.accentColor; view.backgroundColor = background
        tableView.backgroundColor = background
        searchController.searchBar.tintColor = theme.toolbarColor
        compactSearch.tintColor = theme.toolbarColor
        compose.tintColor = theme.toolbarColor
        composeButton.tintColor = theme.toolbarColor
        applyPinnedHeader()
        if changed {
            UIView.performWithoutAnimation {
                for cell in tableView.visibleCells { cell.configurationUpdateHandler?(cell,cell.configurationState) }
            }
        }
    }
    func update(_ next: FeiNavigationState.Listing,labels: [String:String],busy: Bool,canCompose: Bool,theme: FeiMobileTheme) {
        applyTheme(theme)
        let labelsChanged = self.labels != labels
        self.labels = labels; isBusy=busy; self.canCompose=canCompose
        let previous = listing
        if browsesWorkspace,let previous,previous.scope != next.scope { browserOffsets[previous.scope] = captureAnchor() }
        if let previous,previous.scope == next.scope, (previous.selection == nil) != (next.selection == nil) {
            restoreOffset = SearchOrigin(scope: next.scope,query: next.query,anchor: captureAnchor())
        }
        if previous?.scope != next.scope || previous?.rows != next.rows || busy || !next.can_reorder || next.selection != nil {
            finishRowDrag(restore: previous?.scope == next.scope)
        }
        listing = next
        if previous?.kind != next.kind {
            tableView.contentInsetAdjustmentBehavior = next.kind == "trash" ? .scrollableAxes : .always
        }
        let ordered = listingSnapshot(next).itemIdentifiers
        if var pending = pendingMove {
            if pending.scope != next.scope || ordered == pending.order && next.can_reorder || pending.writing && next.can_reorder || next.notice != nil && next.notice != previous?.notice { pendingMove = nil }
            else { pending.writing = pending.writing || !next.can_reorder; pendingMove = pending }
        }
        if let previous,previous.scope != next.scope {
            if isReordering { endReordering() }
            searchOrigin = nil; restoreOffset = nil
            resultReturn = nil
            resetSearch()
        }
        if previous == nil || browsesWorkspace && previous?.scope != next.scope {
            restoreOffset = SearchOrigin(scope: next.scope,query: next.query,anchor: browserOffsets[next.scope] ?? .top)
        }
        updateRowHeight()
        updateSearchTypography()
        empty.font = interfaceFont(.body); notice.message.font = interfaceFont(.subheadline)
        rows = Dictionary(uniqueKeysWithValues: next.rows.map { ($0.id,$0) })
        let key = !next.query.isEmpty ? "result-count" : next.rows.contains(where: { $0.folder }) ? "item-count" : "note-count"
        if let selection = next.selection {
            count.text = String(format: labels["selected-count"] ?? "%d selected",selection.selected.count)
            count.numberOfLines = 1
            count.accessibilityIdentifier = "fei.notes.count"
        } else {
            count.text = next.header_detail ?? String(format: labels[key] ?? "%d items",next.rows.count)
            count.numberOfLines = next.header_detail == nil ? 1 : 0
            count.accessibilityIdentifier = next.header_detail == nil ? "fei.notes.count" : "fei.notes.trash-retention"
        }
        search.accessibilityLabel = labels["search"]
        if !isSearching,search.text != next.query { search.text = next.query }
        tabs.setTitle(labels["all-notes"],forSegmentAt: 0); tabs.setTitle(labels["favorites"],forSegmentAt: 1); tabs.setTitle(labels["sharing"],forSegmentAt: 2)
        tabs.selectedSegmentIndex = next.tab; tabs.isHidden = !next.tabs
        tabs.isUserInteractionEnabled = !isSelecting; tabs.accessibilityTraits = isSelecting ? [.notEnabled] : []
        empty.text = next.rows.isEmpty ? next.empty : nil
        tableView.isUserInteractionEnabled = !busy && pendingMove == nil; search.isUserInteractionEnabled = !busy && pendingMove == nil
        if rowDrag == nil && pendingMove == nil && (previous?.rows != next.rows || previous?.interface_size != next.interface_size || previous?.shared_count != next.shared_count || previous?.selection?.serial != next.selection?.serial || labelsChanged || source.snapshot().itemIdentifiers != ordered) {
            var snapshot = listingSnapshot(next)
            let settled: () -> Void = { [weak self] in self?.restoreSearchAnchor(); self?.restoreSelection() }
            if previous == nil {
                view.layoutIfNeeded()
                source.applySnapshotUsingReloadData(snapshot,completion: settled)
            } else {
                snapshot.reconfigureItems(Array(Set(source.snapshot().itemIdentifiers).intersection(snapshot.itemIdentifiers)))
                let animates = previous?.selection?.serial == next.selection?.serial && previous?.scope == next.scope && previous?.query == next.query &&
                    source.snapshot().itemIdentifiers != ordered && view.window != nil && !UIAccessibility.isReduceMotionEnabled
                source.apply(snapshot,animatingDifferences: animates,completion: settled)
            }
        }
        restoreSelection()
        let message = next.notice
        notice.update(message,kind: next.notice_kind,accent: palette.accentColor)
        if message != nil && message != previous?.notice { UIAccessibility.post(notification: .announcement,argument: message) }
        applyPinnedHeader()
        updateSelection(previous: previous?.selection)
        view.setNeedsLayout()
    }
    private func updateRowHeight() {
        tableView.rowHeight = ceil(interfaceFont(.headline).lineHeight)+ceil(interfaceFont(.subheadline).lineHeight)+28
    }

    @objc private func contentSizeChanged() {
        guard let listing else { return }
        restoreOffset = SearchOrigin(scope: listing.scope,query: listing.query,anchor: captureAnchor())
        updateRowHeight()
        applyPinnedHeader()
        tableView.reloadData()
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
        finishRowDrag()
        if tableView.isEditing && !isSelecting { endReordering() }
        searchOrigin = nil; resetSearch()
        parent?.navigationItem.searchController = nil
        view.endEditing(true); view.isHidden = true; notice.isHidden = true
    }
    private func updateCloudResumeProbe() {
        #if FEI_MOBILE_QA
        tableView.accessibilityValue = "cloudResume=\(coordinator?.readyForCloudResume == true) cloudPending=\(coordinator?.cloudStartPending == true) emptyDragSlot=\(observedEmptyInsertionSlot)"
        #endif
    }
    var requiresListToolbar: Bool {
        guard isViewLoaded,!view.isHidden,listing?.kind != "trash" else { return false }
        if #available(iOS 26.0, *) { return isSelecting }
        return true
    }
    func configureSearch(in controller: UIViewController,expanded: Bool,preparing: Bool = false) {
        let isTop = controller.navigationController?.topViewController === controller
        guard isViewLoaded,!view.isHidden,preparing || isTop else { return }
        if let selection=listing?.selection {
            if #available(iOS 26.0, *) { updateCompactSearch(in: controller,visible: false,showCompose: false) }
            controller.navigationItem.searchController=nil
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
            controller.toolbarItems=[.flexibleSpace(),copySelection,.flexibleSpace(),moveSelection,.flexibleSpace(),deleteSelection,.flexibleSpace()]
            if !preparing { controller.navigationController?.setToolbarHidden(false,animated: false) }
            updateCloudResumeProbe(); searchLayout=nil; return
        }
        if listing?.kind == "trash" {
            if #available(iOS 26.0, *) { updateCompactSearch(in: controller,visible: false,showCompose: false) }
            if controller.navigationItem.searchController != nil { controller.navigationItem.searchController = nil }
            if controller.toolbarItems != nil { controller.toolbarItems = nil }
            if !preparing,controller.navigationController?.isToolbarHidden == false { controller.navigationController?.setToolbarHidden(true,animated: false) }
            tabs.isHidden = true
            return
        }
        controller.definesPresentationContext = true
        definesPresentationContext = true
        let showCompose = !isSearching && canCompose
        let compactToolbar: Bool
        if #available(iOS 26.0, *) {
            compactToolbar = false
            updateCompactSearch(in: controller,visible: !expanded,showCompose: showCompose)
            controller.navigationItem.searchBarPlacementAllowsToolbarIntegration = false
        } else { compactToolbar = !expanded }
        if !usesLocalSearch,controller.navigationItem.searchController !== searchController {
            controller.navigationItem.searchController = searchController
        }
        controller.navigationItem.hidesSearchBarWhenScrolling = expanded
        if controller.navigationItem.preferredSearchBarPlacement != .stacked { controller.navigationItem.preferredSearchBarPlacement = .stacked }
        let hasClose = controller.navigationItem.rightBarButtonItems?.contains(where: { $0 === closeSearch }) == true
        if expanded && searchController.isActive && !hasClose {
            controller.navigationItem.rightBarButtonItems = [closeSearch]+(controller.navigationItem.rightBarButtonItems ?? [])
        } else if (!expanded || !isSearching) && hasClose {
            controller.navigationItem.rightBarButtonItems?.removeAll(where: { $0 === closeSearch })
        }
        updateCloudResumeProbe()
        if compactToolbar && showCompose { coordinator?.configureListCompose(compose) }
        controller.navigationItem.rightBarButtonItems?.filter {
            $0.accessibilityIdentifier == "fei.new"
        }.forEach { $0.isHidden = searchController.isActive }
        let key = "\(compactToolbar)-\(showCompose)"
        if key != searchLayout {
            searchLayout = key
            controller.toolbarItems = compactToolbar && showCompose ? [.flexibleSpace(),compose] : nil
        }
        if !preparing,controller.navigationController?.transitionCoordinator == nil,
           controller.navigationController?.isToolbarHidden != !compactToolbar {
            controller.navigationController?.setToolbarHidden(!compactToolbar,animated: false)
        }
        updateSearchTypography()
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
        if isSearching {
            pendingSelection=true; searchOrigin=nil; resultReturn=nil; endSearch()
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
        let keyboard = view.keyboardLayoutGuide.layoutFrame
        let docked = keyboard.width > 0 && keyboard.height > view.safeAreaInsets.bottom + 1
        if let listBottom, let listKeyboardBottom, listKeyboardBottom.isActive != docked {
            (docked ? listBottom : listKeyboardBottom).isActive = false
            (docked ? listKeyboardBottom : listBottom).isActive = true
        }
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
        if listing.kind == "trash" { return }
        if isSearching {
            resultReturn = SearchOrigin(scope: listing.scope,query: listing.query,anchor: captureAnchor())
            resultToOpen = (id,listing.scope); openingResult = true; endSearch()
        } else { view.endEditing(true); coordinator?.send(browsesWorkspace ? "browse-open" : "list-open",id: id,scope: listing.scope) }
    }
    func willPresentSearchController(_ searchController: UISearchController) { beginSearch() }
    func searchBarTextDidBeginEditing(_ searchBar: UISearchBar) {
        guard searchBar === compactSearch, !compactSearching else { return }
        compactSearching = true
        compactSearch.setShowsCancelButton(true, animated: false)
        beginSearch()
    }
    private func beginSearch() {
        finishRowDrag()
        if tableView.isEditing && !isSelecting { endReordering() }
        resultReturn = nil
        if let listing,searchOrigin == nil { searchOrigin = SearchOrigin(scope: listing.scope,query: listing.query,anchor: captureAnchor()) }
        openingResult = false; refreshSearchControls()
    }
    func searchBar(_ searchBar: UISearchBar,textDidChange searchText: String) {
        // UIKit can clear an inactive field before presenting Search again after pop.
        // The text-change delegate also covers this transition; the results updater
        // intentionally ignores inactive presentation updates.
        guard searchBar === search,!openingResult,!pendingSelection,!isSelecting,let listing,(searchText.isEmpty || searchBar.searchTextField.markedTextRange == nil) else { return }
        resultReturn = nil
        commitSearchClear(searchText,in: listing.scope)
        coordinator?.send(browsesWorkspace ? "browse-search" : "list-search",value: searchText,scope: listing.scope)
    }
    func updateSearchResults(for searchController: UISearchController) {
        guard searchController.isActive,!openingResult,!pendingSelection,!isSelecting,let listing,search.searchTextField.markedTextRange == nil else { return }
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
    func didDismissSearchController(_ searchController: UISearchController) { finishSearch() }
    private func finishSearch() {
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
    private func resetSearch() {
        if compactSearching {
            compactSearching = false
            compactSearch.searchTextField.resignFirstResponder()
            compactSearch.setShowsCancelButton(false, animated: false)
        }
        searchController.isActive = false
    }
    @objc private func endSearch() {
        if usesLocalSearch {
            resetSearch()
            finishSearch()
        } else { searchController.isActive = false }
    }
    func searchBarCancelButtonClicked(_ searchBar: UISearchBar) { endSearch() }
    private func captureAnchor() -> ListAnchor {
        if tableView.contentOffset.y <= -tableView.adjustedContentInset.top { return .top }
        let first = tableView.indexPathsForVisibleRows?.sorted().first
        let id = first.flatMap { source.itemIdentifier(for: $0) }
        let top = first.map { tableView.rectForRow(at: $0).minY } ?? tableView.contentOffset.y
        return .position(id: id,delta: tableView.contentOffset.y-top,fallback: tableView.contentOffset)
    }
    private func restoreSearchAnchor() {
        guard view.window != nil,let origin = restoreOffset,origin.scope == listing?.scope,origin.query == listing?.query else { return }
        restoreOffset = nil; tableView.layoutIfNeeded()
        let minimum = -tableView.adjustedContentInset.top
        let maximum = max(minimum,tableView.contentSize.height-tableView.bounds.height+tableView.adjustedContentInset.bottom)
        switch origin.anchor {
        case .top:
            tableView.setContentOffset(CGPoint(x: tableView.contentOffset.x,y: minimum),animated: false)
        case let .position(id,delta,fallback):
            let target = id.flatMap { source.indexPath(for: $0) }.map { tableView.rectForRow(at: $0).minY+delta } ?? fallback.y
            tableView.setContentOffset(CGPoint(x: fallback.x,y: min(maximum,max(minimum,target))),animated: false)
        }
    }
    func searchBarSearchButtonClicked(_ searchBar: UISearchBar) { searchBar.searchTextField.resignFirstResponder() }
    @objc private func changeTab() {
        guard let listing else { return }
        if browsesWorkspace { coordinator?.send("browse-scope",value: ["all","favorites","shared"][tabs.selectedSegmentIndex],scope: listing.scope) }
        else { coordinator?.send("list-tab",value: String(tabs.selectedSegmentIndex),scope: listing.scope) }
    }
    func tableView(_ tableView: UITableView,contextMenuConfigurationForRowAt indexPath: IndexPath,point: CGPoint) -> UIContextMenuConfiguration? {
        guard !isSelecting,rowDrag == nil,let listing,let id = source.itemIdentifier(for: indexPath) else { return nil }
        let browser = browsesWorkspace
        return UIContextMenuConfiguration(identifier: id as NSString,previewProvider: nil) { [weak self] _ in
            self?.coordinator?.itemMenu(id,scope: listing.scope,browsingWorkspace: browser)
        }
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
    func tableView(_ tableView: UITableView,heightForHeaderInSection section: Int) -> CGFloat { source.snapshot().sectionIdentifiers[section]==0 ? max(44,ceil(interfaceFont(.headline).lineHeight)+12) : .leastNormalMagnitude }
    func tableView(_ tableView: UITableView,viewForHeaderInSection section: Int) -> UIView? {
        guard source.snapshot().sectionIdentifiers[section]==0,listing != nil else { return nil }
        let container=UIView(), button=UIButton(type: .system)
        let title = UILabel()
        title.isAccessibilityElement = false
        let arrow = UIImageView()
        arrow.contentMode = .center
        let stack = UIStackView(arrangedSubviews: [title,arrow]); stack.alignment = .center
        arrow.setContentHuggingPriority(.required,for: .horizontal)
        stack.isUserInteractionEnabled = false; stack.translatesAutoresizingMaskIntoConstraints = false
        pinnedHeaderButton=button; pinnedHeaderTitle=title; pinnedHeaderArrow=arrow
        button.accessibilityIdentifier="fei.notes.pinned-header"
        applyPinnedHeader()
        button.addTarget(self,action: #selector(togglePinned),for: .touchUpInside); button.translatesAutoresizingMaskIntoConstraints=false
        container.addSubview(button); button.addSubview(stack)
        NSLayoutConstraint.activate([button.topAnchor.constraint(equalTo: container.topAnchor),button.bottomAnchor.constraint(equalTo: container.bottomAnchor),button.leadingAnchor.constraint(equalTo: container.leadingAnchor),button.trailingAnchor.constraint(equalTo: container.trailingAnchor),
            stack.leadingAnchor.constraint(equalTo: button.leadingAnchor,constant: 16),stack.trailingAnchor.constraint(equalTo: button.trailingAnchor,constant: UIDevice.current.userInterfaceIdiom == .phone ? -23 : -16),stack.centerYAnchor.constraint(equalTo: button.centerYAnchor),
            arrow.widthAnchor.constraint(greaterThanOrEqualToConstant: 16),arrow.heightAnchor.constraint(greaterThanOrEqualToConstant: 24)])
        return container
    }
    @objc private func togglePinned() {
        guard let listing,listing.query.isEmpty else { return }
        if collapsedPinned.contains(listing.scope) { collapsedPinned.remove(listing.scope) } else { collapsedPinned.insert(listing.scope) }
        UserDefaults.standard.set(Array(collapsedPinned),forKey: "FeiCollapsedPinnedScopes")
        source.apply(listingSnapshot(listing),animatingDifferences: !UIAccessibility.isReduceMotionEnabled) { [weak self] in self?.restoreSelection() }
        applyPinnedHeader()
    }
    private func swipe(_ keys: [(String,String,UIColor)],id: String) -> UISwipeActionsConfiguration? {
        guard let listing,!isSelecting else { return nil }
        let config = UISwipeActionsConfiguration(actions: keys.map { key,symbol,color in
            let item = coordinator?.rowItem(id,in: listing)
            let labelKey: String
            switch key {
            case "delete" where item?.menu_root == id: labelKey = item?.share_end ?? "leave-shared"
            case "delete" where item?.folder == true && item?.menu_root != nil: labelKey = "delete-child-folder"
            case "delete","restore","purge": labelKey = item?.folder == true ? key + "-folder" : key
            case "pin" where item?.pinned == true: labelKey = "unpin"
            case "star" where item?.starred == true: labelKey = "unstar"
            default: labelKey = key
            }
            let action = UIContextualAction(style: key == "delete" || key == "purge" || key == "discard-recovery" ? .destructive : .normal,title: labels[labelKey]) { [weak self] _,_,done in
                guard let self else { done(false); return }
                self.coordinator?.rowAction(key,id: id,scope: listing.scope,browser: self.browsesWorkspace); done(true)
            }
            action.image = UIImage(systemName: symbol); action.backgroundColor = color; return action
        })
        config.performsFirstActionWithFullSwipe = false; return config
    }
    func tableView(_ tableView: UITableView,leadingSwipeActionsConfigurationForRowAt indexPath: IndexPath) -> UISwipeActionsConfiguration? {
        guard rowDrag == nil,let listing,let id=source.itemIdentifier(for: indexPath),let row=rows[id] else { return nil }
        if coordinator?.rowItem(id,in: listing)?.offline_recovery == true { return swipe([("keep-recovery","doc.on.doc",.systemGreen)],id: id) }
        if listing.kind == "trash" { return swipe([("restore","arrow.uturn.backward",.systemGreen)],id: id) }
        return row.folder ? nil : swipe([("pin","pin",palette.accentColor),("star","star",.systemOrange)],id: id)
    }
    func tableView(_ tableView: UITableView,trailingSwipeActionsConfigurationForRowAt indexPath: IndexPath) -> UISwipeActionsConfiguration? {
        guard rowDrag == nil,let listing,let id=source.itemIdentifier(for: indexPath) else { return nil }
        let item = coordinator?.rowItem(id,in: listing)
        if item?.offline_recovery == true { return swipe([("discard-recovery","trash",.systemRed)],id: id) }
        if listing.kind == "trash" { return swipe([("purge","trash",.systemRed)],id: id) }
        guard let item,!id.hasPrefix("collection-") else { return nil }
        if item.menu_root == id { return item.share_end == nil ? nil : swipe([("delete","trash",.systemRed)],id: id) }
        if (item.menu_root != nil || item.inherited_share) && !item.menu_writable { return nil }
        return swipe([("move","folder",.systemBlue),("delete","trash",.systemRed)],id: id)
    }
    private func draggableRow(_ path: IndexPath) -> Bool {
        guard let id = source.itemIdentifier(for: path) else { return false }
        return rows[id]?.reorderable == true
    }

    private func canDragRow(_ path: IndexPath) -> Bool {
        guard draggableRow(path) else { return false }
        let snapshot = source.snapshot()
        guard snapshot.sectionIdentifiers.indices.contains(path.section) else { return false }
        let siblings = snapshot.itemIdentifiers(inSection: snapshot.sectionIdentifiers[path.section])
        return siblings.contains { id in id != source.itemIdentifier(for: path) && rows[id]?.reorderable == true }
    }

    func gestureRecognizer(_ recognizer: UIGestureRecognizer,shouldReceive touch: UITouch) -> Bool {
        guard recognizer is FeiRowReorderGesture else { return true }
        guard listing?.can_reorder == true,!isBusy,!isSelecting,!isReordering,!isSearching,pendingMove == nil,
              let path = tableView.indexPathForRow(at: touch.location(in: tableView)) else { return false }
        return canDragRow(path)
    }

    @objc private func reorderByTouch(_ gesture: FeiRowReorderGesture) {
        let point = gesture.currentPoint(in: tableView)
        switch gesture.state {
        case .began:
            guard let scope = listing?.scope,let origin = gesture.originalPoint(in: tableView),
                  let path = tableView.indexPathForRow(at: origin),let id = source.itemIdentifier(for: path),
                  let cell = tableView.cellForRow(at: path),let preview = cell.snapshotView(afterScreenUpdates: false) else { return }
            preview.frame = tableView.convert(cell.frame,to: view)
            preview.layer.shadowOpacity = 0.16; preview.layer.shadowRadius = 10
            preview.isUserInteractionEnabled = false
            preview.accessibilityElementsHidden = true
            view.addSubview(preview)
            rowDrag = RowDrag(id: id,scope: scope,preview: preview,origin: path,originalOrder: source.snapshot(),destination: path)
            #if FEI_MOBILE_QA
            observedEmptyInsertionSlot = false
            #endif
            UIView.performWithoutAnimation {
                cell.configurationUpdateHandler?(cell,cell.configurationState)
                cell.layoutIfNeeded()
            }
            if let point { updateRowDrag(to: point) }
        case .changed: if let point { updateRowDrag(to: point) }
        case .ended:
            if let point { updateRowDrag(to: point) }
            guard let drag = rowDrag,drag.scope == listing?.scope,let to = drag.destination,to != drag.origin else { finishRowDrag(); return }
            finishRowDrag(restore: false)
            moveRow(from: drag.origin,to: to,originalOrder: drag.originalOrder)
        case .cancelled,.failed: finishRowDrag()
        default: break
        }
    }

    private func updateRowDrag(to point:CGPoint) {
        guard var drag = rowDrag,drag.scope == listing?.scope else { return }
        drag.preview.center.y = tableView.convert(point,to: view).y
        let visible = tableView.bounds.inset(by: tableView.adjustedContentInset)
        guard point.x >= visible.minX,point.x <= visible.maxX,point.y >= visible.minY,point.y <= visible.maxY,
              let target = tableView.indexPathForRow(at: CGPoint(x: tableView.bounds.midX,y: point.y)),
              target.section == drag.origin.section,draggableRow(target) else {
            drag.destination = nil
            rowDrag = drag
            return
        }
        guard target != drag.destination else { return }
        drag.destination = target
        rowDrag = drag
        if let proposal = reordered(drag.originalOrder,from: drag.origin,to: target) {
            let animated = !UIAccessibility.isReduceMotionEnabled
            #if FEI_MOBILE_QA
            source.apply(proposal.snapshot,animatingDifferences: animated) { [weak self] in
                self?.observeInsertionSlot(for: proposal.id)
            }
            #else
            source.apply(proposal.snapshot,animatingDifferences: animated)
            #endif
        }
    }

    #if FEI_MOBILE_QA
    private func observeInsertionSlot(for id: String) {
        guard let drag = rowDrag,drag.id == id,drag.preview.window != nil,
              let slotPath = source.indexPath(for: id),slotPath != drag.origin,
              let slot = tableView.cellForRow(at: slotPath) else { return }
        observedEmptyInsertionSlot = slot.contentConfiguration == nil && slot.accessoryView == nil &&
            slot.accessibilityIdentifier == "fei.notes.insertion-slot"
        updateCloudResumeProbe()
    }
    #endif

    private func finishRowDrag(restore: Bool = true) {
        guard let drag = rowDrag else { return }
        rowDrag = nil
        drag.preview.removeFromSuperview()
        if let path = source.indexPath(for: drag.id),let cell = tableView.cellForRow(at: path) {
            cell.configurationUpdateHandler?(cell,cell.configurationState)
        }
        if restore && source.snapshot().itemIdentifiers != drag.originalOrder.itemIdentifiers {
            source.apply(drag.originalOrder,animatingDifferences: false)
        }
    }

    func tableView(_ tableView: UITableView,targetIndexPathForMoveFromRowAt sourceIndexPath: IndexPath,toProposedIndexPath proposedDestinationIndexPath: IndexPath) -> IndexPath {
        sourceIndexPath.section == proposedDestinationIndexPath.section && draggableRow(proposedDestinationIndexPath) ? proposedDestinationIndexPath : sourceIndexPath
    }
    private func reordered(_ original: NSDiffableDataSourceSnapshot<Int,String>,from: IndexPath,to destination: IndexPath) -> (snapshot:NSDiffableDataSourceSnapshot<Int,String>,id:String,before:String?)? {
        guard from.section == destination.section,original.sectionIdentifiers.indices.contains(from.section) else { return nil }
        let section = original.sectionIdentifiers[from.section]
        var items = original.itemIdentifiers(inSection: section)
        guard items.indices.contains(from.row),destination.row >= 0 else { return nil }
        let id = items.remove(at: from.row)
        let index = min(destination.row,items.count)
        let before = index < items.count ? items[index] : nil
        var snapshot = original
        if let before { snapshot.moveItem(id,beforeItem: before) }
        else if let last = items.last { snapshot.moveItem(id,afterItem: last) }
        return (snapshot,id,before)
    }

    private func moveRow(from: IndexPath,to destination: IndexPath,originalOrder: NSDiffableDataSourceSnapshot<Int,String>? = nil) {
        guard let listing,listing.can_reorder,pendingMove == nil,from != destination,
              let (snapshot,id,before) = reordered(originalOrder ?? source.snapshot(),from: from,to: destination),
              rows[id]?.reorderable == true else { return }
        pendingMove = PendingMove(scope: listing.scope,order: snapshot.itemIdentifiers)
        tableView.isUserInteractionEnabled = false
        source.apply(snapshot,animatingDifferences: false)
        coordinator?.send("list-reorder",value: before ?? "",id: id,scope: listing.scope)
        DispatchQueue.main.asyncAfter(deadline: .now()+2) { [weak self] in
            guard let self,self.pendingMove?.order == snapshot.itemIdentifiers,let current = self.listing else { return }
            self.pendingMove = nil
            self.tableView.isUserInteractionEnabled = !self.isBusy
            self.source.apply(self.listingSnapshot(current),animatingDifferences: false)
        }
    }
    func beginReordering() {
        guard listing?.can_reorder == true,let parent,!tableView.isEditing else { return }
        view.endEditing(true)
        reorderButtons = parent.navigationItem.rightBarButtonItems
        let done=UIBarButtonItem(title: labels["done"],style: .done,target: self,action: #selector(endReordering))
        done.tintColor = palette.accentColor
        done.setTitleTextAttributes([.foregroundColor: UIColor.white],for: .normal)
        done.accessibilityIdentifier = "fei.notes.reorder.done"
        parent.navigationItem.rightBarButtonItems = [done]
        isReordering = true
        tableView.visibleCells.forEach { $0.showsReorderControl = true }
        tableView.setEditing(true,animated: !UIAccessibility.isReduceMotionEnabled)
        updateCloudResumeProbe()
    }
    @objc private func endReordering() {
        guard isReordering else { return }
        isReordering = false
        tableView.setEditing(false,animated: !UIAccessibility.isReduceMotionEnabled)
        tableView.visibleCells.forEach { $0.showsReorderControl = false }
        parent?.navigationItem.rightBarButtonItems = reorderButtons; reorderButtons = nil
        updateCloudResumeProbe()
    }
    func tableView(_ tableView: UITableView,editingStyleForRowAt indexPath: IndexPath) -> UITableViewCell.EditingStyle { isSelecting ? .delete : .none }
    func tableView(_ tableView: UITableView,shouldIndentWhileEditingRowAt indexPath: IndexPath) -> Bool { false }
    func anchor(for id: String) -> CGRect? {
        guard let index = source.indexPath(for: id),let cell = tableView.cellForRow(at: index),let view = parent?.view else { return nil }
        let visible = tableView.convert(tableView.bounds,to: view).intersection(view.bounds)
        let rect = cell.convert(cell.bounds,to: view).intersection(visible)
        return rect.isEmpty ? nil : rect
    }
}
