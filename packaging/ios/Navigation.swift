import UIKit

struct FeiNavigationState: Decodable, Equatable, Sendable {
    struct Listing: Decodable, Equatable, Sendable {
        struct Row: Decodable, Equatable, Sendable { let id:String; let title:String; let summary:String; let folder:Bool; let pinned:Bool; let reorderable:Bool; let shared_badge:Bool; let shared_folder:Bool; let unread_update:Bool }
        struct Selection: Decodable,Equatable,Sendable { let serial:UInt64; let ids:[String]; let selected:[String]; let limit:Int; let can_move:Bool; let moving:Bool }
        let kind:String; let can_select:Bool; let can_sort:Bool; let can_reorder:Bool; let shared_create:Bool; let sort_rule:String; let folders_first:Bool?; let shared_count:Int; let selection:Selection?; let notice_kind:String?
        let interface_size:String; let notice:String?; let scope:String; let query:String; let tab:Int; let tabs:Bool; let header_detail:String?; let empty:String; let rows:[Row]
    }
    struct Browser: Decodable,Equatable,Sendable { let folder:String?; let favorites:Bool; let shared:Bool; let personal:Bool; let can_create:Bool; let title:String; let path:[Route]; let listing:Listing }
    struct QA: Decodable,Equatable,Sendable {
        struct Pos: Decodable,Equatable,Sendable { let block:UInt64; let offset:Int }
        struct Selection: Decodable,Equatable,Sendable { let anchor:Pos; let head:Pos }
        let draft:Bool; let generation:UInt64; let title:String; let body:String; let selection:Selection; let scroll:String; let board:String?
    }
    let qa: QA?
    let browser: Browser?
    let settings: FeiSettingsState?
    let restore_shared_sync: Bool
    let local_history: LocalHistory?
    struct SharedOpening: Decodable, Equatable, Sendable { let message: String; let loading: Bool; let retry: String?; let back: String }
    let sharing_title: String?
    let folder_access: Bool
    let document_needs_attention: Bool
    let conflict_review: Bool

    // Conflict controls precede the scroll view that normally insets the editor content.
    var editorOverlapsChrome: Bool {
        ["note","history-preview"].contains(kind) && !board_open && !find_active && !document_needs_attention
    }
    let shared_opening: SharedOpening?
    struct SharedSync: Decodable, Equatable, Sendable {
        let title: String; let scope: String; let detail: String; let rows: [[String]]
        let attention: Bool; let action: String?
    }
    let shared_sync: SharedSync?
    let slash_available: Bool
    let listing: Listing?
    let editor_scope:String
    let reading:Bool
    let write_allowed:Bool
    let block_actions:Bool
    let block_selected:Bool
    let find_active:Bool
    let board_open:Bool
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
        let offline_recovery: Bool
        let personal: Bool
        let inherited_share: Bool
        let share_end: String?
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
        let enable_cloud: Bool
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
    let document_shared: Bool
    let editing_collaborators: [String]
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
class FeiRouteController: UIViewController {
    var isWorkspaceBrowser = false
    var routeKey = "notes"
    var frozenContent: UIView?
    var notesList: FeiNotesListController?
    var settingsList: FeiSettingsController?
    struct BarIdentity: Equatable { let kind:String; let shared:Bool; let editing:Bool; let typing:Bool; let blockSelected:Bool; let expanded:Bool; let topTools:Bool; let composing:Bool; let board:Bool; let collaborators:[String]; let appearance:String; let theme:FeiMobileTheme?; let labels:[String:String] }
    var barIdentity: BarIdentity?
    var overlapsChrome = false
    var chromeTheme: FeiMobileTheme?
    private var sharedOpening: UIStackView?
    private let openingSpinner = UIActivityIndicatorView(style: .medium)
    private let openingMessage = UILabel()
    private let openingRetry = UIButton(type: .system)
    private let openingBack = UIButton(type: .system)
    var hasNativeContent: Bool { settingsList?.view.isHidden == false || notesList?.view.isHidden == false || sharedOpening?.isHidden == false }

    @discardableResult
    func configureNavigationTitleMode() -> Bool {
        let large = isWorkspaceBrowser || ["notes","shared"].contains(routeKey) || routeKey.hasPrefix("folder:")
        let mode: UINavigationItem.LargeTitleDisplayMode = large ? .always : .never
        if navigationItem.largeTitleDisplayMode != mode { navigationItem.largeTitleDisplayMode = mode }
        return large
    }

    func updateSharedOpening(_ state: FeiNavigationState, coordinator: FeiNavigationCoordinator) {
        guard let opening = state.shared_opening else {
            sharedOpening?.isHidden = true
            openingSpinner.stopAnimating()
            return
        }
        if sharedOpening == nil {
            openingMessage.font = .preferredFont(forTextStyle: .body)
            openingMessage.adjustsFontForContentSizeCategory = true
            openingMessage.textColor = .secondaryLabel
            openingMessage.textAlignment = .center
            openingMessage.numberOfLines = 0
            openingMessage.accessibilityIdentifier = "fei.shared-opening"
            openingRetry.accessibilityIdentifier = "fei.shared-opening.retry"
            openingBack.accessibilityIdentifier = "fei.shared-opening.back"
            openingBack.addAction(UIAction { [weak coordinator] _ in coordinator?.send("back") }, for: .touchUpInside)
            let stack = UIStackView(arrangedSubviews: [openingSpinner, openingMessage, openingRetry, openingBack])
            stack.axis = .vertical
            stack.alignment = .center
            stack.spacing = 12
            stack.translatesAutoresizingMaskIntoConstraints = false
            view.addSubview(stack)
            let guide = view.safeAreaLayoutGuide
            NSLayoutConstraint.activate([
                stack.centerXAnchor.constraint(equalTo: guide.centerXAnchor),
                stack.centerYAnchor.constraint(equalTo: guide.centerYAnchor),
                stack.leadingAnchor.constraint(greaterThanOrEqualTo: guide.leadingAnchor, constant: 24),
                stack.trailingAnchor.constraint(lessThanOrEqualTo: guide.trailingAnchor, constant: -24),
            ])
            sharedOpening = stack
        }
        sharedOpening?.isHidden = false
        openingMessage.text = opening.message
        if opening.loading { openingSpinner.startAnimating() } else { openingSpinner.stopAnimating() }
        openingRetry.isHidden = opening.retry == nil
        openingRetry.setTitle(opening.retry, for: .normal)
        openingRetry.addAction(UIAction(identifier: UIAction.Identifier("shared-open-retry")) { [weak coordinator, scope = state.editor_scope] _ in
            coordinator?.send("shared-open-retry", scope: scope)
        }, for: .touchUpInside)
        openingBack.setTitle(opening.back, for: .normal)
    }

    func updateListing(_ listing: FeiNavigationState.Listing?,labels: [String:String],busy: Bool,coordinator: FeiNavigationCoordinator) {
        guard let listing else {
            if let notesList,!notesList.view.isHidden {
                notesList.deactivate()
                if settingsList?.view.isHidden != false { setContentScrollView(nil,for: .all) }
            }
            return
        }
        if isWorkspaceBrowser { view.backgroundColor = coordinator.theme.workspaceListColor }
        configureNavigationTitleMode()
        if notesList == nil {
            let list = FeiNotesListController(coordinator: coordinator,theme: coordinator.theme)
            list.browsesWorkspace = isWorkspaceBrowser
            let usesSafeWidth = isWorkspaceBrowser || listing.kind == "trash"
            addChild(list); list.view.translatesAutoresizingMaskIntoConstraints = false; view.addSubview(list.view)
            NSLayoutConstraint.activate([list.view.topAnchor.constraint(equalTo: view.topAnchor),list.view.bottomAnchor.constraint(equalTo: view.bottomAnchor),list.view.leadingAnchor.constraint(equalTo: usesSafeWidth ? view.safeAreaLayoutGuide.leadingAnchor : view.leadingAnchor),list.view.trailingAnchor.constraint(equalTo: usesSafeWidth ? view.safeAreaLayoutGuide.trailingAnchor : view.trailingAnchor)])
            list.didMove(toParent: self); notesList = list
            setContentScrollView(list.tableView,for: .all)
        }
        guard let notesList else { return }
        if notesList.view.isHidden { setContentScrollView(notesList.tableView,for: .all) }
        notesList.view.isHidden = false
        notesList.update(listing,labels: labels,busy: busy,canCompose: coordinator.showsComposeShortcut,theme: coordinator.theme)
    }

    func updateSettings(_ settings:FeiSettingsState?,busy:Bool,coordinator:FeiNavigationCoordinator) {
        guard let settings else {
            if let settingsList,!settingsList.view.isHidden {
                settingsList.deactivate()
                if notesList?.view.isHidden != false { setContentScrollView(nil,for: .all) }
            }
            return
        }
        configureNavigationTitleMode()
        if settingsList == nil {
            let list = FeiSettingsController(coordinator: coordinator)
            addChild(list); list.view.translatesAutoresizingMaskIntoConstraints = false; view.addSubview(list.view)
            NSLayoutConstraint.activate([list.view.topAnchor.constraint(equalTo: view.topAnchor),list.view.bottomAnchor.constraint(equalTo: view.bottomAnchor),list.view.leadingAnchor.constraint(equalTo: view.leadingAnchor),list.view.trailingAnchor.constraint(equalTo: view.trailingAnchor)])
            list.didMove(toParent: self); settingsList = list
            setContentScrollView(list.tableView,for: .all)
        }
        guard let settingsList else { return }
        if settingsList.view.isHidden { setContentScrollView(settingsList.tableView,for: .all) }
        settingsList.view.isHidden = false
        settingsList.update(settings,busy: busy,theme: coordinator.theme)
    }

    override func viewDidLoad() {
        super.viewDidLoad()
        view.backgroundColor = .systemBackground
        registerForTraitChanges([UITraitHorizontalSizeClass.self,UITraitPreferredContentSizeCategory.self]) { (controller: FeiRouteController,_) in
            controller.configureNavigationTitleMode()
        }
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

    func applyTheme(_ theme: FeiMobileTheme) {
        view.tintColor = theme.accentColor
        view.backgroundColor = isWorkspaceBrowser ? theme.workspaceListColor : hasNativeContent ? theme.listColor : theme.paperColor
        notesList?.applyTheme(theme)
        settingsList?.applyTheme(theme)
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
private final class FeiShareOverviewController: UIViewController {
    private let stack = UIStackView()
    private let scroll = UIScrollView()
    private var names: [String]
    private var showsPeople = false
    private let labels: [String:String]
    private let manage: () -> Void

    init(names: [String], labels: [String:String], manage: @escaping () -> Void) {
        self.names = names
        self.labels = labels
        self.manage = manage
        super.init(nibName: nil,bundle: nil)
    }
    required init?(coder: NSCoder) { nil }

    override func viewDidLoad() {
        super.viewDidLoad()
        view.backgroundColor = .systemBackground
        view.accessibilityIdentifier = "fei.share.overview"
        view.accessibilityViewIsModal = true
        scroll.translatesAutoresizingMaskIntoConstraints = false
        stack.translatesAutoresizingMaskIntoConstraints = false
        stack.axis = .vertical
        stack.spacing = 4
        view.addSubview(scroll)
        scroll.addSubview(stack)
        NSLayoutConstraint.activate([
            scroll.topAnchor.constraint(equalTo: view.topAnchor),
            scroll.bottomAnchor.constraint(equalTo: view.bottomAnchor),
            scroll.leadingAnchor.constraint(equalTo: view.leadingAnchor),
            scroll.trailingAnchor.constraint(equalTo: view.trailingAnchor),
            stack.topAnchor.constraint(equalTo: scroll.contentLayoutGuide.topAnchor,constant: 10),
            stack.bottomAnchor.constraint(equalTo: scroll.contentLayoutGuide.bottomAnchor,constant: -10),
            stack.leadingAnchor.constraint(equalTo: scroll.contentLayoutGuide.leadingAnchor,constant: 12),
            stack.trailingAnchor.constraint(equalTo: scroll.contentLayoutGuide.trailingAnchor,constant: -12),
            stack.widthAnchor.constraint(equalTo: scroll.frameLayoutGuide.widthAnchor,constant: -24)
        ])
        render()
    }

    func updateNames(_ names: [String]) {
        guard self.names != names else { return }
        self.names = names
        if isViewLoaded { render() }
    }

    private func text(_ string: String, style: UIFont.TextStyle) -> UILabel {
        let label = UILabel()
        label.text = string
        label.font = .preferredFont(forTextStyle: style)
        label.adjustsFontForContentSizeCategory = true
        label.numberOfLines = 0
        return label
    }

    private func row(_ key: String, id: String, action: @escaping () -> Void) -> UIButton {
        var configuration = UIButton.Configuration.plain()
        configuration.title = labels[key]
        configuration.baseForegroundColor = .label
        configuration.titleLineBreakMode = .byWordWrapping
        configuration.contentInsets = NSDirectionalEdgeInsets(top: 6,leading: 8,bottom: 6,trailing: 8)
        let button = UIButton(configuration: configuration,primaryAction: UIAction { _ in action() })
        button.accessibilityIdentifier = id
        button.contentHorizontalAlignment = .leading
        button.heightAnchor.constraint(greaterThanOrEqualToConstant: 44).isActive = true
        return button
    }

    private func showPeople(_ show: Bool) {
        showsPeople = show
        render()
        scroll.setContentOffset(.zero,animated: false)
        let header = stack.arrangedSubviews.first as? UIStackView
        UIAccessibility.post(notification: .screenChanged,argument: header?.arrangedSubviews.first { $0 is UILabel })
    }

    private func makeHeader() -> UIView {
        let title = text(labels[showsPeople ? "online-collaborators" : "already-shared"] ?? "",style: .headline)
        title.accessibilityTraits.insert(.header)
        let header = UIStackView()
        header.alignment = .center
        header.spacing = 4
        if showsPeople {
            header.addArrangedSubview(headerButton("chevron.backward",label: "back",id: "fei.share.online.back") { [weak self] in
                self?.showPeople(false)
            })
        }
        header.addArrangedSubview(title)
        header.addArrangedSubview(headerButton("xmark",label: "close-share-overview",id: "fei.share.overview.close") { [weak self] in
            self?.dismiss(animated: true)
        })
        return header
    }

    private func headerButton(_ symbol: String,label: String,id: String,action: @escaping () -> Void) -> UIButton {
        var configuration = UIButton.Configuration.plain()
        configuration.image = UIImage(systemName: symbol,withConfiguration: UIImage.SymbolConfiguration(pointSize: 13,weight: .semibold))
        configuration.baseForegroundColor = .secondaryLabel
        let button = UIButton(configuration: configuration,primaryAction: UIAction { _ in action() })
        button.accessibilityLabel = labels[label]
        button.accessibilityIdentifier = id
        NSLayoutConstraint.activate([button.widthAnchor.constraint(equalToConstant: 44),button.heightAnchor.constraint(equalToConstant: 44)])
        return button
    }

    private func avatar(_ name: String,index: Int,size: CGFloat) -> UIView {
        let colors: [UIColor] = [.systemBlue,.systemPurple,.systemTeal,.systemOrange]
        let badge = UIView()
        badge.backgroundColor = colors[index % colors.count].withAlphaComponent(0.22)
        badge.layer.cornerRadius = size / 2
        badge.accessibilityElementsHidden = true
        let initial = text(String(name.prefix(1)),style: .caption1)
        initial.textColor = .label
        initial.numberOfLines = 1
        initial.textAlignment = .center
        initial.adjustsFontSizeToFitWidth = true
        initial.minimumScaleFactor = 0.65
        initial.translatesAutoresizingMaskIntoConstraints = false
        badge.addSubview(initial)
        NSLayoutConstraint.activate([
            badge.widthAnchor.constraint(equalToConstant: size),
            badge.heightAnchor.constraint(equalToConstant: size),
            initial.leadingAnchor.constraint(equalTo: badge.leadingAnchor,constant: 2),
            initial.trailingAnchor.constraint(equalTo: badge.trailingAnchor,constant: -2),
            initial.centerYAnchor.constraint(equalTo: badge.centerYAnchor)
        ])
        return badge
    }

    private func personRow(_ name: String,index: Int) -> UIView {
        let label = text(name,style: .subheadline)
        let status = text(labels["online-status"] ?? "",style: .caption1)
        label.accessibilityIdentifier = "fei.share.online.member"
        status.textColor = .secondaryLabel
        status.setContentHuggingPriority(.required,for: .horizontal)
        status.setContentCompressionResistancePriority(.required,for: .horizontal)
        let content = UIStackView(arrangedSubviews: [avatar(name,index: index,size: 26),label,status])
        content.alignment = .center
        content.spacing = 8
        content.isLayoutMarginsRelativeArrangement = true
        content.directionalLayoutMargins = NSDirectionalEdgeInsets(top: 4,leading: 2,bottom: 4,trailing: 2)
        content.heightAnchor.constraint(greaterThanOrEqualToConstant: 40).isActive = true
        let divider = UIView()
        divider.backgroundColor = .separator
        divider.heightAnchor.constraint(equalToConstant: 0.5).isActive = true
        let row = UIStackView(arrangedSubviews: [divider,content])
        row.axis = .vertical
        return row
    }

    private func disclosure() -> UIImageView {
        let image = UIImageView(image: UIImage(systemName: "chevron.forward",withConfiguration: UIImage.SymbolConfiguration(pointSize: 12,weight: .semibold)))
        image.tintColor = .tertiaryLabel
        return image
    }

    private func attachTrailing(_ view: UIView,to button: UIButton) {
        view.translatesAutoresizingMaskIntoConstraints = false
        view.isUserInteractionEnabled = false
        button.addSubview(view)
        NSLayoutConstraint.activate([
            view.trailingAnchor.constraint(equalTo: button.trailingAnchor,constant: -8),
            view.centerYAnchor.constraint(equalTo: button.centerYAnchor)
        ])
        let fitting = view.systemLayoutSizeFitting(UIView.layoutFittingCompressedSize)
        var configuration = button.configuration
        configuration?.contentInsets.trailing = ceil(fitting.width) + 16
        button.configuration = configuration
    }

    private func render() {
        for view in stack.arrangedSubviews { stack.removeArrangedSubview(view); view.removeFromSuperview() }
        stack.addArrangedSubview(makeHeader())
        if showsPeople {
            if names.isEmpty {
                let empty = text(labels["no-online-collaborators"] ?? "",style: .subheadline)
                empty.textColor = .secondaryLabel
                stack.addArrangedSubview(empty)
            } else {
                let summary = text(String(format: labels["online-context"] ?? "%d",names.count),style: .footnote)
                summary.textColor = .secondaryLabel
                stack.addArrangedSubview(summary)
                for (index,name) in names.enumerated() {
                    stack.addArrangedSubview(personRow(name,index: index))
                }
            }
        } else {
            if !names.isEmpty {
                let online = row("online-collaborators",id: "fei.share.online") { [weak self] in self?.showPeople(true) }
                online.accessibilityValue = String(names.count)
                let avatars = UIStackView()
                avatars.alignment = .center
                avatars.spacing = 3
                avatars.accessibilityElementsHidden = true
                for (index,name) in names.prefix(3).enumerated() {
                    avatars.addArrangedSubview(avatar(name,index: index,size: 22))
                }
                if names.count > 3 {
                    let remaining = text("+\(names.count - 3)",style: .caption1)
                    remaining.textColor = .secondaryLabel
                    avatars.addArrangedSubview(remaining)
                }
                avatars.addArrangedSubview(disclosure())
                attachTrailing(avatars,to: online)
                stack.addArrangedSubview(online)
            }
            let permissions = row("share-access-action",id: "fei.share.permissions",action: manage)
            attachTrailing(disclosure(),to: permissions)
            stack.addArrangedSubview(permissions)
        }
        let width = min(304,max(228,(presentingViewController?.view.bounds.width ?? 336) - 32))
        let height = stack.systemLayoutSizeFitting(CGSize(width: width - 24,height: 0),
            withHorizontalFittingPriority: .required,verticalFittingPriority: .fittingSizeLevel).height + 20
        preferredContentSize = CGSize(width: width,height: min(340,ceil(height)))
    }
}

@MainActor
private final class FeiSharedSyncController: UIViewController {
    private let content = UIStackView()
    private let scroll = UIScrollView()
    private var snapshot: FeiNavigationState.SharedSync?
    private var warningColor: UIColor?
    private var action: ((String)->Void)?

    override func viewDidLoad() {
        super.viewDidLoad()
        view.backgroundColor = .systemBackground
        content.axis = .vertical
        content.spacing = 12
        content.translatesAutoresizingMaskIntoConstraints = false
        scroll.translatesAutoresizingMaskIntoConstraints = false
        view.addSubview(scroll); scroll.addSubview(content)
        NSLayoutConstraint.activate([
            scroll.leadingAnchor.constraint(equalTo: view.leadingAnchor), scroll.trailingAnchor.constraint(equalTo: view.trailingAnchor),
            scroll.topAnchor.constraint(equalTo: view.safeAreaLayoutGuide.topAnchor), scroll.bottomAnchor.constraint(equalTo: view.safeAreaLayoutGuide.bottomAnchor),
            content.leadingAnchor.constraint(equalTo: scroll.contentLayoutGuide.leadingAnchor,constant: 18),
            content.trailingAnchor.constraint(equalTo: scroll.contentLayoutGuide.trailingAnchor,constant: -18),
            content.topAnchor.constraint(equalTo: scroll.contentLayoutGuide.topAnchor,constant: 12),
            content.bottomAnchor.constraint(equalTo: scroll.contentLayoutGuide.bottomAnchor,constant: -18),
            content.widthAnchor.constraint(equalTo: scroll.frameLayoutGuide.widthAnchor,constant: -36)
        ])
    }

    func update(_ status: FeiNavigationState.SharedSync,labels: [String:String],warning: UIColor, action: @escaping (String)->Void) {
        loadViewIfNeeded()
        self.action = action
        guard status != snapshot || warning != warningColor else { return }
        snapshot = status
        warningColor = warning
        content.arrangedSubviews.forEach { $0.removeFromSuperview() }
        func label(_ text: String,style: UIFont.TextStyle,color: UIColor) -> UILabel {
            let label = UILabel()
            label.text = text; label.font = .preferredFont(forTextStyle: style)
            label.adjustsFontForContentSizeCategory = true; label.numberOfLines = 0; label.textColor = color
            return label
        }
        view.accessibilityIdentifier = "fei.shared-sync"
        title = labels["shared-sync-status"]
        content.addArrangedSubview(label(status.scope,style: .subheadline,color: .secondaryLabel))
        let headline = label(status.title,style: .headline,color: status.attention ? warning : .label)
        headline.accessibilityIdentifier = "fei.shared-sync.status"
        content.addArrangedSubview(headline)
        if !status.detail.isEmpty { content.addArrangedSubview(label(status.detail,style: .subheadline,color: .secondaryLabel)) }
        for row in status.rows where row.count == 2 {
            let line = UIStackView()
            line.distribution = .fill; line.spacing = 8; line.alignment = .firstBaseline
            let key = label(row[0],style: .subheadline,color: .secondaryLabel)
            let value = label(row[1],style: .subheadline,color: .label)
            value.textAlignment = .right
            value.setContentHuggingPriority(.defaultHigh,for: .horizontal)
            line.addArrangedSubview(key); line.addArrangedSubview(value)
            content.addArrangedSubview(line)
        }
        if let operation = status.action {
            let key = operation == "retry" ? "sync-retry" : "sync-settings"
            let button = UIButton(type: .system)
            button.addAction(UIAction { [weak self] _ in self?.action?(operation) },for: .touchUpInside)
            button.setTitle(labels[key],for: .normal)
            button.accessibilityIdentifier = "fei.shared-sync.action"
            button.heightAnchor.constraint(greaterThanOrEqualToConstant: 44).isActive = true
            content.addArrangedSubview(button)
        }
        preferredContentSize = CGSize(width: 340,height: 270)
    }
}

@MainActor
final class FeiNavigationCoordinator: NSObject, UINavigationControllerDelegate, UIPopoverPresentationControllerDelegate {
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
    private enum DocumentSheet: Equatable {
        case sharing(String)
        case invitation(String)
        case history(UInt64)
    }
    private weak var shareOverview: FeiShareOverviewController?
    private var shareOverviewScope: String?
    private var documentSheet: UINavigationController?
    private var documentSheetKey: DocumentSheet?
    private var documentSheetChanging = false
    private var pendingDrainScheduled = false
    private var pending: FeiNavigationState?
    private var applying = false
    private var nativePopTarget: String?
    private var postPopRevision: UInt64 = 0
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
    private var visibleListing: FeiNavigationState.Listing? { hasPadWorkspace ? state?.browser?.listing : state?.listing }
    private var isVirtualListing: Bool {
        guard let listing = visibleListing else { return false }
        let personal = hasPadWorkspace ? state?.browser?.personal == true : state?.shared_session == false
        return listing.kind == "shared" || personal && listing.tab == 1
    }
    var showsComposeShortcut: Bool {
        guard let state,let listing = visibleListing,["notes","shared"].contains(listing.kind) else { return false }
        if isVirtualListing { return false }
        return hasPadWorkspace ? state.browser?.can_create == true : state.can_create
    }
    func configureListCompose(_ item: UIBarButtonItem) {
        let title = label("new")
        item.isEnabled = allowsCompose
        guard item.accessibilityLabel != title || item.primaryAction == nil else { return }
        item.accessibilityLabel = title
        item.menu = nil
        item.primaryAction = UIAction(title: title,image: UIImage(systemName: "square.and.pencil")) { [weak self] _ in self?.composeNote() }
    }
    var allowsCompose: Bool {
        guard let state,!state.busy,state.confirmation == nil,pendingCreation == nil else { return false }
        return showsComposeShortcut
    }
    func composeNote() {
        guard allowsCompose,!applying,host?.navigationController?.transitionCoordinator == nil,let state else { return }
        requestComposition(hasPadWorkspace ? "browse-compose" : "compose",state: state,
            scope: hasPadWorkspace ? state.browser?.listing.scope : state.listing?.scope)
    }
    func composeFromEditor() {
        guard let state,state.kind == "note",state.current != nil,state.can_create,!state.busy,!state.conflict_review,
              state.confirmation == nil,pendingCreation == nil,!applying,host?.navigationController?.transitionCoordinator == nil else { return }
        requestComposition("editor-compose",state: state,scope: state.editor_scope)
    }
    private func requestComposition(_ action: String,state: FeiNavigationState,scope: String?) {
        guard let scope else { return }
        createSerial = max(createSerial,state.revision)+1
        pendingCreation = createSerial
        send(action,id: state.current,serial: createSerial,scope: scope)
    }
    private weak var sharedSync: FeiSharedSyncController?
    private var sharedSyncScope: String?
    private var sharedSyncRestoreScope: String?
    private weak var browserMore: UIBarButtonItem?
    private var moreBadgeRevision: (Bool, FeiMobileTheme)?
    private weak var confirmationAlert: UIAlertController?
    private weak var cloudSwitchSheet: UIViewController?
    private var informationGeneration: UInt64 = 0
    private var informationSerial: UInt64?
    private weak var informationAlert: UIAlertController?
    private var reorderGeneration: UInt64 = 0
    private weak var reorderController: FeiReorderController?
    private weak var versionHistory: FeiVersionHistoryController?
    private var confirmationGeneration: UInt64 = 0
    private var presentationGeneration: UInt64 = 0
    private var focusAfterTransition = false
    #if FEI_MOBILE_QA
    private var replacedSharedAncestorForQA = false
    #endif
    private enum SheetAnchor {
        case row(String,browser: Bool)
    }

    init(host: EditorHost) {
        self.host = host
        super.init()
        host.navigationController?.delegate = self
    }

    func receive(_ next: FeiNavigationState) {
        if state == nil { UIView.performWithoutAnimation { applyNavigation(next) } }
        else { applyNavigation(next) }
    }

    private func applyNavigation(_ next: FeiNavigationState) {
        guard next.revision >= postPopRevision,
              next.revision > (state?.revision ?? 0), next.revision > (pending?.revision ?? 0),
              let host, let navigation = host.navigationController else { return }
        if navigation.transitionCoordinator != nil {
            pending = next
            drainPendingAfterTransition()
            return
        }
        let routes = host.padWorkspace?.detailRoutes(next) ?? next.routes
        let listing = host.padWorkspace == nil || next.kind == "trash" ? next.listing : nil
        let settings = next.sharing_title == nil ? next.settings : nil
        #if FEI_MOBILE_QA
        if !replacedSharedAncestorForQA, ProcessInfo.processInfo.environment["FEI_QA_REBUILD_SHARED_ANCESTOR"] == "1",
           host.padWorkspace == nil, next.kind == "note", next.shared_session, next.current == state?.current,
           next.qa?.body.contains("Remote fixture paragraph") == true, navigation.viewControllers.count > 2,
           let editor = navigation.topViewController {
            replacedSharedAncestorForQA = true
            // Exercise route reconciliation when only the editor's ancestor changes.
            navigation.setViewControllers([host, editor], animated: false)
        }
        #endif
        let oldKeys = navigation.viewControllers.compactMap { ($0 as? FeiRouteController)?.routeKey }
        let newKeys = routes.map(\.key)
        if nativePopTarget != nil { nativePopTarget = nil }
        let oldState = state
        if let pendingCreation,let ack = next.creation_ack,ack >= pendingCreation {
            self.pendingCreation = nil
            if next.current == oldState?.current && next.kind != "note" && next.confirmation == nil { focusAfterTransition = false }
        }
        if oldState?.confirmation != nil && next.confirmation == nil && next.current == oldState?.current { focusAfterTransition = false }
        state = next
        if next.restore_shared_sync && oldState?.restore_shared_sync != true { sharedSyncRestoreScope = next.editor_scope }
        else if !next.restore_shared_sync || sharedSyncRestoreScope != next.editor_scope { sharedSyncRestoreScope = nil }
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
        for controller in navigation.viewControllers.compactMap({ $0 as? FeiRouteController }) {
            if oldState?.theme != next.theme { controller.thawContent() }
            controller.applyTheme(palette)
        }
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
                controller.applyTheme(palette)
                if controller.title != route.title { controller.title = route.title }
                controller.configureNavigationTitleMode()
                controller.navigationItem.backButtonTitle = next.labels["back"]
                controller.navigationItem.backButtonDisplayMode = .minimal
                if index == routes.count-1 {
                    controller.updateSettings(settings,busy: next.busy,coordinator: self)
                    controller.updateListing(listing,labels: next.labels,busy: next.busy,coordinator: self)
                    if host.padWorkspace == nil {
                        controller.notesList?.configureSearch(in: controller,expanded: false,preparing: true)
                    }
                    controller.updateSharedOpening(next, coordinator: self)
                }
                return controller
            }
            if host.padWorkspace == nil,let outgoing = navigation.topViewController as? FeiRouteController,
                outgoing.routeKey.hasPrefix("note:") || outgoing.routeKey.hasPrefix("history-preview") { outgoing.freezeContent() }
            if let target = controllers.last { host.attachEditor(to: target) }
            let replacesControllers = navigation.viewControllers.map(ObjectIdentifier.init) != controllers.map(ObjectIdentifier.init)
            if replacesControllers {
                // UIKit does not send didShow when only an ancestor is replaced.
                let changesTop = navigation.topViewController !== controllers.last
                let animated = changesTop && oldState != nil && (host.padWorkspace == nil || next.kind == "settings" || oldState?.kind == "settings") && !UIAccessibility.isReduceMotionEnabled
                applying = true
                navigation.setViewControllers(controllers,animated: animated)
                if !animated { applying = false }
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
            top.updateSettings(settings,busy: next.busy,coordinator: self)
            top.updateListing(listing,labels: next.labels,busy: next.busy,coordinator: self)
            top.updateSharedOpening(next, coordinator: self)
            top.applyTheme(palette)
            host.attachEditor(to: top)
        }
        host.updateEditorToolbar(next)
        updateButtons()
        syncDocumentSheet()
        updateSharedSync(next)
        restoreSharedSyncIfNeeded()
        if let choice = navigation.topViewController as? FeiSettingsChoiceController,let settings = next.settings { choice.update(settings,busy: next.busy) }
        if next.information != oldState?.information { showInformation(next.information) }
        if next.reorder != oldState?.reorder { showReorder(next.reorder) }
        if next.confirmation != oldState?.confirmation || (next.confirmation?.versions != nil && (next.busy != oldState?.busy || next.cloud_history != oldState?.cloud_history)) { showConfirmation(next.confirmation) }
        host.deliverPendingInvitation()
        #if FEI_MOBILE_QA
        if next.kind == "note",next.title == "QA standalone collaboration",next.shared_session,
           oldState?.editor_scope != next.editor_scope,ProcessInfo.processInfo.environment["FEI_QA_SHARED_BROWSER_REFRESH"] == "1" {
            DispatchQueue.main.asyncAfter(deadline: .now() + 5) { [weak self] in
                guard self?.state?.editor_scope == next.editor_scope else { return }
                self?.send("qa-shared-browser-refresh",scope: next.editor_scope)
            }
        }
        if next.kind == "note",next.shared_session,oldState?.editor_scope != next.editor_scope,
           ProcessInfo.processInfo.environment["FEI_QA_SYNC_BADGE"] == "1" {
            let states = ProcessInfo.processInfo.environment["FEI_QA_SHARED_SYNC_SETTINGS"] == "1"
                ? [(0.0,"signed-out")] : [(0.0,"ready"),(5.0,"uploading"),(11.0,"stalled"),(27.0,"ready")]
            for (delay,value) in states {
                DispatchQueue.main.asyncAfter(deadline: .now() + delay) { [weak self] in
                    guard self?.state?.editor_scope == next.editor_scope else { return }
                    self?.send("qa-sync-state",value: value,scope: next.editor_scope)
                }
            }
        }
        #endif
    }

    private func desiredDocumentSheet(_ state: FeiNavigationState) -> DocumentSheet? {
        if state.sharing_title != nil,let settings = state.settings {
            if settings.sections.contains(where: { $0.rows.contains(where: { $0.kind == "invitation-link" }) }) {
                return .invitation(state.editor_scope)
            }
            return .sharing(state.editor_scope)
        }
        if let history = state.local_history, !history.previewing { return .history(history.token) }
        return nil
    }

    private func syncDocumentSheet() {
        guard let state, !documentSheetChanging else { return }
        let desired = desiredDocumentSheet(state)
        if let sheet = documentSheet, documentSheetKey != desired {
            documentSheetChanging = true
            sheet.dismiss(animated: true) { [weak self] in
                guard let self else { return }
                self.documentSheet = nil
                self.documentSheetKey = nil
                self.documentSheetChanging = false
                self.syncDocumentSheet()
            }
            return
        }
        guard let desired, let presenter = host?.navigationController?.topViewController else { return }
        let sheet: UINavigationController
        let needsPresentation = documentSheet == nil
        if let existing = documentSheet { sheet = existing }
        else {
            guard presenter.viewIfLoaded?.window != nil, presenter.presentedViewController == nil else { return }
            if let transition = presenter.transitionCoordinator {
                transition.animate(alongsideTransition: nil) { [weak self] _ in
                    DispatchQueue.main.async { self?.syncDocumentSheet() }
                }
                return
            }
            let content: UIViewController
            switch desired {
            case .sharing: content = FeiSettingsController(coordinator: self)
            case .invitation: content = FeiShareAcceptController(coordinator: self)
            case .history: content = FeiLocalHistoryController(coordinator: self)
            }
            sheet = UINavigationController(rootViewController: content)
            if case .history = desired { sheet.setToolbarHidden(false,animated: false) }
            let isInvitation: Bool
            if case .invitation = desired { isInvitation = true } else { isInvitation = false }
            if (content is FeiSettingsController || isInvitation) && presenter.traitCollection.horizontalSizeClass == .regular {
                sheet.modalPresentationStyle = .formSheet
                sheet.preferredContentSize = CGSize(width: 520,height: isInvitation ? 320 : 600)
            } else {
                sheet.modalPresentationStyle = .pageSheet
                sheet.sheetPresentationController?.detents = isInvitation ? [.medium(),.large()] : [.large()]
                sheet.sheetPresentationController?.prefersGrabberVisible = true
            }
            sheet.presentationController?.delegate = self
            documentSheet = sheet
            documentSheetKey = desired
        }
        guard let content = sheet.viewControllers.first else { return }
        sheet.overrideUserInterfaceStyle = presenter.navigationController?.overrideUserInterfaceStyle ?? .unspecified
        sheet.view.tintColor = theme.accentColor
        sheet.isModalInPresentation = state.busy
        let done = UIBarButtonItem(title: state.labels["done"],style: .done,target: self,action: #selector(closeDocumentSheet))
        done.isEnabled = !state.busy
        switch desired {
        case .sharing:
            content.title = state.sharing_title
            done.accessibilityIdentifier = "fei.share.done"
            if let settings = state.settings {
                let ids = Set(settings.sections.flatMap(\.rows).map(\.id))
                if !ids.isDisjoint(with: ["share-invite","share-submit","share-new-name"]) {
                    done.style = .plain
                    done.title = state.labels["cancel"]
                }
                (content as? FeiSettingsController)?.update(settings,busy: state.busy,theme: theme)
            }
        case .invitation:
            content.title = state.sharing_title
            done.style = .plain
            done.title = state.labels["cancel"]
            done.accessibilityIdentifier = "fei.share.done"
            if let settings = state.settings { (content as? FeiShareAcceptController)?.update(settings,busy: state.busy,theme: theme) }
        case .history:
            content.title = state.labels["history"]
            done.accessibilityIdentifier = "fei.history.close"
            if let history = state.local_history { (content as? FeiLocalHistoryController)?.update(history,labels: state.labels,busy: state.busy) }
        }
        content.navigationItem.rightBarButtonItem = done
        if needsPresentation {
            presenter.view.endEditing(true)
            documentSheetChanging = true
            presenter.present(sheet,animated: true) { [weak self] in
                self?.documentSheetChanging = false
                self?.syncDocumentSheet()
            }
        }
    }

    @objc private func closeDocumentSheet() {
        guard let state, !state.busy, let key = documentSheetKey, desiredDocumentSheet(state) == key else { return }
        switch key {
        case .sharing(let scope), .invitation(let scope): send("close-document-sharing",scope: scope)
        case .history: sendHistory("close")
        }
    }

    func presentationControllerDidDismiss(_ presentationController: UIPresentationController) {
        guard presentationController.presentedViewController === documentSheet else { return }
        closeDocumentSheet()
        documentSheet = nil
        documentSheetKey = nil
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
            cloudSwitchSheet?.dismiss(animated: true); cloudSwitchSheet = nil
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
        if confirmation.enable_cloud,let choice = confirmation.choice {
            let form = UIViewController()
            form.view.backgroundColor = .systemGroupedBackground
            form.title = confirmation.title
            let body = UILabel()
            body.text = confirmation.message; body.numberOfLines = 0
            body.font = .preferredFont(forTextStyle: .body); body.adjustsFontForContentSizeCategory = true
            let caption = UILabel()
            caption.text = choice; caption.numberOfLines = 0
            caption.font = .preferredFont(forTextStyle: .body); caption.adjustsFontForContentSizeCategory = true
            let toggle = UISwitch()
            toggle.isOn = confirmation.choice_on
            toggle.accessibilityLabel = choice; toggle.accessibilityIdentifier = "fei.confirm.choice"
            let row = UIStackView(arrangedSubviews: [caption,toggle]); row.alignment = .center; row.spacing = 10
            let stack = UIStackView(arrangedSubviews: [body,row]); stack.axis = .vertical; stack.spacing = 18
            stack.translatesAutoresizingMaskIntoConstraints = false
            form.view.addSubview(stack)
            NSLayoutConstraint.activate([
                stack.leadingAnchor.constraint(equalTo: form.view.layoutMarginsGuide.leadingAnchor),
                stack.trailingAnchor.constraint(equalTo: form.view.layoutMarginsGuide.trailingAnchor),
                stack.topAnchor.constraint(equalTo: form.view.safeAreaLayoutGuide.topAnchor,constant: 15)
            ])
            let navigation = UINavigationController(rootViewController: form)
            navigation.view.accessibilityIdentifier = "fei.cloud-enable-confirm"
            navigation.overrideUserInterfaceStyle = controller.navigationController?.overrideUserInterfaceStyle ?? .unspecified
            navigation.isModalInPresentation = true
            let regular = controller.traitCollection.horizontalSizeClass == .regular
            navigation.modalPresentationStyle = regular ? .formSheet : .pageSheet
            if regular { navigation.preferredContentSize = CGSize(width: 460,height: 270) }
            else {
                navigation.sheetPresentationController?.detents = [.medium(),.large()]
                navigation.sheetPresentationController?.prefersGrabberVisible = true
            }
            form.navigationItem.leftBarButtonItem = UIBarButtonItem(title: label("cancel"),image: nil,
                primaryAction: UIAction { [weak self] _ in self?.send("cancel-confirm",serial: confirmation.serial) })
            let confirm = UIBarButtonItem(title: confirmation.action,image: nil,primaryAction: UIAction { [weak self,weak toggle] _ in
                self?.send("confirm-enable-cloud",value: toggle?.isOn == true ? "true" : "false",serial: confirmation.serial)
            })
            confirm.style = .done
            confirm.accessibilityIdentifier = "fei.confirm.ok"
            form.navigationItem.rightBarButtonItem = confirm
            let present = { [weak self] in
                guard let self,self.confirmationGeneration == generation,self.state?.confirmation == confirmation else { return }
                self.cloudSwitchSheet = navigation
                controller.present(navigation,animated: true)
            }
            if let previous = controller.presentedViewController { previous.dismiss(animated: true,completion: present) } else { present() }
            return
        }
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
        let topTools = host?.navigationEditorTools.isEmpty == false
        if expanded != buttonsExpanded || (controller as? FeiRouteController)?.barIdentity?.topTools != topTools { updateButtons() }
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
            let editorActionsEnabled = !state.busy && !state.conflict_review
            controller.navigationItem.rightBarButtonItems?.forEach { item in
                let identifier = item.accessibilityIdentifier ?? ""
                item.isEnabled = editorActionsEnabled && (!identifier.hasPrefix("fei.edit.") || state.can_edit)
            }
            controller.navigationItem.rightBarButtonItems?.filter { $0.accessibilityIdentifier == "fei.edit.block-actions" }.forEach {
                $0.isEnabled = editorActionsEnabled && state.can_edit && state.block_actions &&
                    (host?.editorHasKeyboardFocus == true || state.block_selected)
            }
            if state.kind == "note",state.current == nil { controller.navigationItem.rightBarButtonItems?.filter { ["fei.more","fei.share"].contains($0.accessibilityIdentifier ?? "") }.forEach { $0.isEnabled = false } }
            if state.kind == "trash" { controller.navigationItem.rightBarButtonItem?.isEnabled = !state.busy && state.items.contains(where: { $0.trashed }) }
            let hidesBack = state.busy || state.board_open || state.kind == "history-preview"
            if controller.navigationItem.hidesBackButton != hidesBack { controller.navigationItem.hidesBackButton = hidesBack }
            host?.navigationController?.interactivePopGestureRecognizer?.isEnabled = !hidesBack && state.listing?.selection == nil
            if state.board_open { controller.navigationItem.leftBarButtonItems = nil }
            else { host?.padWorkspace?.configureDetailButtons(controller,state: state,coordinator: self) }
        }
        let overlapsChrome = state.editorOverlapsChrome
        if route.overlapsChrome != overlapsChrome || route.chromeTheme != theme {
            route.overlapsChrome = overlapsChrome
            route.chromeTheme = theme
            let appearance = overlapsChrome ? UINavigationBarAppearance() : nil
            appearance?.configureWithTransparentBackground()
            if #unavailable(iOS 26.0), let appearance {
                appearance.backgroundEffect = UIBlurEffect(style: .systemChromeMaterial)
                appearance.backgroundColor = theme.chromeColor?.withAlphaComponent(0.65)
            }
            controller.navigationItem.standardAppearance = appearance
            controller.navigationItem.scrollEdgeAppearance = appearance
            controller.navigationItem.compactAppearance = appearance
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
        let shared = state.kind == "note" ? state.document_shared : state.shared_session
        let identity = FeiRouteController.BarIdentity(kind: state.kind,shared: shared,editing: state.kind == "note" && state.can_edit,typing: state.kind == "note" && typing,blockSelected: state.block_selected,expanded: expanded,topTools: host?.navigationEditorTools.isEmpty == false,composing: state.can_create,board: state.board_open,collaborators: state.editing_collaborators,appearance: state.appearance,theme: state.theme,labels: state.labels)
        if let overview = shareOverview {
            if shareOverviewScope == state.editor_scope && state.kind == "note" && !state.board_open {
                overview.updateNames(state.editing_collaborators)
            } else {
                overview.dismiss(animated: false)
                shareOverview = nil
                shareOverviewScope = nil
            }
        }
        if route.barIdentity == identity {
            controller.navigationItem.rightBarButtonItems?.first { $0.accessibilityIdentifier == "fei.share" || $0.accessibilityIdentifier == "fei.folder.share" }?.isEnabled = !state.busy
            let presentation = (state.shared_sync?.attention == true, state.theme ?? .system)
            if moreBadgeRevision?.0 != presentation.0 || moreBadgeRevision?.1 != presentation.1 {
                moreBadgeRevision = presentation
                updateSharedSyncButton(controller.navigationItem.rightBarButtonItems?.first(where: {
                    $0.accessibilityIdentifier == "fei.more" || $0.accessibilityIdentifier == "fei.library-actions"
                }),state: state,folder: state.kind == "folder")
                if state.browser?.personal == false { updateSharedSyncButton(browserMore,state: state,folder: true) }
            }
            return
        }
        route.barIdentity = identity
        moreBadgeRevision = (state.shared_sync?.attention == true,state.theme ?? .system)
        buttonsController = controller
        buttonsKind = state.kind
        buttonsShared = state.shared_session
        buttonsEditing = state.can_edit
        buttonsTyping = typing
        buttonsExpanded = expanded
        buttonsLabels = state.labels
        controller.navigationItem.rightBarButtonItems = nil
        if state.board_open {
            let done = UIBarButtonItem(title: state.labels["done"],style: .done,target: self,action: #selector(closeBoard))
            done.accessibilityIdentifier = "fei.board.done"
            controller.navigationItem.rightBarButtonItem = done
        } else if state.kind == "note" {
            let more = UIBarButtonItem(image: UIImage(systemName: "ellipsis"),primaryAction: nil,menu: deferredActions(note: true))
            more.accessibilityLabel = state.labels["note-more"]
            more.accessibilityIdentifier = "fei.more"
            updateSharedSyncButton(more,state: state,folder: false)
            let activity = state.theme?.id == "notes" ? 0 : state.editing_collaborators.count
            let share = UIBarButtonItem(image: shareImage(shared: shared,count: activity),style: .plain,target: self,action: #selector(shareNote))
            share.accessibilityLabel = state.labels[shared ? "already-shared" : "share-object"]
            share.accessibilityIdentifier = "fei.share"
            share.isEnabled = !state.busy
            if !state.editing_collaborators.isEmpty {
                share.accessibilityValue = "\(state.editing_collaborators.count) \(label("online-collaborators"))"
            }
            shareOverview?.popoverPresentationController?.sourceItem = share
            var items = [more,share]+(host?.navigationEditorTools.reversed() ?? [].reversed())
            if state.can_edit && typing {
                let done = UIBarButtonItem(image: UIImage(systemName: "checkmark"),style: .done,target: self,action: #selector(endTyping))
                done.accessibilityLabel = state.labels["end-typing"]
                done.accessibilityIdentifier = "fei.edit.done"; items.insert(done,at: 0)
            }
            if state.can_edit && UIDevice.current.userInterfaceIdiom != .phone {
                let undo = UIBarButtonItem(image: UIImage(systemName: "arrow.uturn.backward"),style: .plain,target: self,action: #selector(undoNote))
                undo.accessibilityIdentifier = "fei.edit.undo"; undo.accessibilityLabel = state.labels["undo"]
                if #available(iOS 26.0,*) { undo.sharesBackground = false }
                undo.menu = UIMenu(children: [UIAction(title: label("redo"),image: UIImage(systemName: "arrow.uturn.forward")) { [weak self] _ in self?.dispatchUndoNote(redo: true) }])
                items.append(undo)
            }
            controller.navigationItem.rightBarButtonItems = items
        } else if state.kind == "history-preview" {
            let close = UIBarButtonItem(image: UIImage(systemName: "xmark"),style: .plain,target: self,action: #selector(returnToHistoryList))
            close.accessibilityLabel = state.labels["history-return"]; close.accessibilityIdentifier = "fei.history.close"
            controller.navigationItem.rightBarButtonItem = close
        } else if state.kind == "trash" {
            let empty = UIBarButtonItem(title: state.labels["empty-trash"],style: .plain,target: self,action: #selector(emptyTrash))
            empty.accessibilityIdentifier = "fei.empty-trash"
            empty.tintColor = .systemRed
            controller.navigationItem.rightBarButtonItem = empty
        } else if (state.kind == "notes" || state.kind == "folder") && !hasPadWorkspace {
            let more = UIBarButtonItem(image: UIImage(systemName: "ellipsis"),primaryAction: nil,menu: deferredActions(note: false))
            more.accessibilityLabel = state.labels["folder-more"]
            more.accessibilityIdentifier = "fei.library-actions"
            updateSharedSyncButton(more,state: state,folder: true)
            let share = state.kind == "folder" ? folderShareItem(state,browser: false) : nil
            controller.navigationItem.rightBarButtonItems = [more] + (share.map { [$0] } ?? [])
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
            if route.configureNavigationTitleMode() { route.navigationItem.title=route.title }
            else if route.routeKey.hasPrefix("note:") { route.navigationItem.title = "" }
        }
        if let route = viewController as? FeiRouteController { host?.prepareEditorTransition(to: route) }
        let compactList = list?.requiresListToolbar == true && host?.padWorkspace == nil
        if navigationController.isToolbarHidden == compactList {
            navigationController.setToolbarHidden(!compactList,animated: animated)
        }
    }

    func navigationController(_ navigationController: UINavigationController,
                              didShow viewController: UIViewController, animated: Bool) {
        guard let shown = viewController as? FeiRouteController, let host else {
            applying = false
            drainPendingAfterTransition()
            return
        }
        host.attachEditor(to: shown)
        if !applying, let state, nativePopTarget == nil {
            let expected = (host.padWorkspace?.detailRoutes(state) ?? state.routes).map(\.key)
            let actual = navigationController.viewControllers.compactMap { ($0 as? FeiRouteController)?.routeKey }
            if actual.count < expected.count && expected.starts(with: actual) {
                nativePopTarget = shown.routeKey
                for _ in actual.count..<expected.count { send("back") }
                postPopRevision = fei_ios_publish_navigation_after_pop()
                if let pending, pending.revision < postPopRevision { self.pending = nil }
            }
        }
        applying = false
        if focusAfterTransition,state?.kind == "note",state?.can_edit == true {
            focusAfterTransition = false
            DispatchQueue.main.async { [weak host] in host?.prepareEditorInput() }
        }
        updateButtons()
        syncDocumentSheet()
        drainPendingAfterTransition()
        host.deliverPendingInvitation()
        DispatchQueue.main.async { [weak self] in self?.restoreSharedSyncIfNeeded() }
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
        if action == "toolbar-command",["done","styles","insert","quick-insert","slash","block-actions","undo","redo"].contains(value ?? ""),let state,state.kind == "note",state.current == id,state.editor_scope == scope,state.can_edit,state.write_allowed,!state.busy {
            host?.settleEditorComposition()
        }
        if state?.kind == "note",["share-note","shared-folder-access","settings","shared-library-settings","browse-open","browse-scope","back","history-open"].contains(action) {
            host?.settleEditorComposition()
            host?.navigationController?.topViewController?.view.endEditing(true)
        }
        if ["list-open","compose","browse-compose","new","new-document","new-markdown","settings","shared-folder-access","trash","back"].contains(action),let sender = host?.navigationController?.topViewController as? FeiRouteController {
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
    @objc private func returnToHistoryList() { sendHistory("back-to-list") }
    func renameHistory(_ row:FeiNavigationState.LocalHistory.Version,history:FeiNavigationState.LocalHistory) {
        let alert = UIAlertController(title: label("history-name"),message: nil,preferredStyle: .alert)
        alert.addTextField { field in field.text = row.named ? row.title : ""; field.accessibilityIdentifier = "fei.history.rename" }
        alert.addAction(UIAlertAction(title: label("cancel"),style: .cancel))
        let save = UIAlertAction(title: label("save"),style: .default) { [weak self,weak alert] _ in self?.sendHistory("rename",name: row.name,text: alert?.textFields?.first?.text ?? "",history: history) }
        save.accessibilityIdentifier = "fei.history.rename.save"; alert.addAction(save); present(alert,from: documentSheet?.topViewController)
    }
    func confirmHistory(_ op:String,name:String? = nil,history:FeiNavigationState.LocalHistory? = nil) {
        guard let h = history ?? state?.local_history,let version = name ?? h.selected else { return }
        let alert = UIAlertController(title: label(op == "restore" ? "history-restore-confirm" : "history-delete-confirm"),message: nil,preferredStyle: .alert)
        alert.addAction(UIAlertAction(title: label("cancel"),style: .cancel))
        let action = UIAlertAction(title: label(op == "restore" ? "history-restore" : "history-delete"),style: .destructive) { [weak self] _ in self?.sendHistory(op,name: version,history: h) }
        action.accessibilityIdentifier = "fei.history.confirm"; alert.addAction(action); present(alert,from: documentSheet?.topViewController)
    }

    @objc private func closeBoard() {
        guard let state,state.board_open else { return }
        send("toolbar-command",value: "board-close",id: state.current,scope: state.editor_scope)
    }
    @objc private func endTyping() {
        guard let state,state.kind == "note" else { return }
        send("toolbar-command",value: "done",id: state.current,scope: state.editor_scope)
    }
    @objc private func undoNote() { dispatchUndoNote(redo: false) }
    private func dispatchUndoNote(redo: Bool) {
        guard let state,state.kind == "note",state.can_edit,!state.busy else { return }
        send("toolbar-command",value: redo ? "redo" : "undo",id: state.current,scope: state.editor_scope)
    }
    private lazy var shareToolbarImages = (
        invite: Self.renderShareImage(shared: false),
        shared: Self.renderShareImage(shared: true),
        online: Self.renderShareImage(shared: true,online: true)
    )

    private func shareImage(shared: Bool,count: Int = 0) -> UIImage? {
        guard shared else { return shareToolbarImages.invite }
        return count > 0 ? shareToolbarImages.online : shareToolbarImages.shared
    }

    private static func renderShareImage(shared: Bool,online: Bool = false) -> UIImage? {
        let name = shared ? "person.2" : "person.badge.plus"
        let configuration = UIImage.SymbolConfiguration(pointSize: 16,weight: .regular)
        guard let symbol = UIImage(systemName: name,withConfiguration: configuration) else { return nil }
        guard let ink = visibleSymbolBounds(symbol) else { return symbol }
        let canvas = CGSize(width: 26,height: 24)
        let inkWidth: CGFloat = 22.5
        let scale = inkWidth / ink.width
        return UIGraphicsImageRenderer(size: canvas).image { _ in
            symbol.draw(in: CGRect(x: (canvas.width - inkWidth) / 2 - ink.minX * scale,
                                   y: canvas.height / 2 - ink.midY * scale,
                                   width: symbol.size.width * scale,height: symbol.size.height * scale))
            if online {
                UIColor.black.setFill()
                UIBezierPath(ovalIn: CGRect(x: 22,y: 1.5,width: 4,height: 4)).fill()
            }
        }.withRenderingMode(.alwaysTemplate)
    }

    private static func visibleSymbolBounds(_ symbol: UIImage) -> CGRect? {
        // Measure the symbol's ink rather than its image margins so sharing states align.
        let format = UIGraphicsImageRendererFormat()
        format.scale = 4
        let sample = UIGraphicsImageRenderer(size: symbol.size,format: format).image { _ in symbol.draw(at: .zero) }
        guard let image = sample.cgImage else { return nil }
        let width = image.width
        let height = image.height
        let colorSpace = CGColorSpaceCreateDeviceRGB()
        var pixels = [UInt8](repeating: 0,count: width * height * 4)
        let drewImage = pixels.withUnsafeMutableBytes { buffer -> Bool in
            guard let context = CGContext(data: buffer.baseAddress,width: width,height: height,bitsPerComponent: 8,
                                          bytesPerRow: width * 4,space: colorSpace,
                                          bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue) else { return false }
            context.draw(image,in: CGRect(x: 0,y: 0,width: width,height: height))
            return true
        }
        guard drewImage else { return nil }
        var minX = width, minY = height, maxX = -1, maxY = -1
        for y in 0..<height {
            for x in 0..<width where pixels[(y * width + x) * 4 + 3] >= 128 {
                minX = min(minX,x); minY = min(minY,y)
                maxX = max(maxX,x); maxY = max(maxY,y)
            }
        }
        guard maxX >= minX,maxY >= minY else { return nil }
        return CGRect(x: CGFloat(minX)/sample.scale,y: CGFloat(minY)/sample.scale,
                      width: CGFloat(maxX-minX+1)/sample.scale,height: CGFloat(maxY-minY+1)/sample.scale)
    }

    private func toolbarGlyph(_ symbol: UIImage,size: CGSize,verticalOffset: CGFloat = 0,badge: String?) -> UIImage {
        UIGraphicsImageRenderer(size: size).image { context in
            let origin = CGPoint(x: (size.width-symbol.size.width)/2,y: (size.height-symbol.size.height)/2 + verticalOffset)
            symbol.draw(at: origin)
            guard let badgeText = badge else { return }
            // Older systems use a template badge so the complete icon inherits modal dimming.
            let badge = badgeText as NSString
            let font = UIFont.monospacedDigitSystemFont(ofSize: badge.length > 2 ? 6.5 : 7.5,weight: .semibold)
            let attributes: [NSAttributedString.Key:Any] = [.font:font,.foregroundColor:UIColor.black]
            let textSize = badge.size(withAttributes: attributes)
            let width = max(10,ceil(textSize.width) + 3)
            let bounds = CGRect(x: size.width - width,y: 0,width: width,height: 10)
            context.cgContext.setBlendMode(.clear)
            UIBezierPath(roundedRect: bounds.insetBy(dx: -0.75,dy: -0.75),cornerRadius: 6).fill()
            context.cgContext.setBlendMode(.normal)
            UIColor.black.setFill()
            UIBezierPath(roundedRect: bounds,cornerRadius: 5).fill()
            context.cgContext.setBlendMode(.destinationOut)
            badge.draw(at: CGPoint(x: bounds.midX - textSize.width / 2,y: bounds.midY - textSize.height / 2),withAttributes: attributes)
        }.withRenderingMode(.alwaysTemplate)
    }

    private func hasSharedFolderShortcut(_ state: FeiNavigationState,browser: Bool) -> Bool {
        state.folder_access && (browser ? state.browser?.personal == false && state.browser?.folder != nil : state.kind == "folder")
    }

    func folderShareItem(_ state: FeiNavigationState,browser: Bool) -> UIBarButtonItem? {
        let shared = hasSharedFolderShortcut(state,browser: browser)
        let candidate = browser ? state.browser?.folder : state.routes.last?.key.replacingOccurrences(of: "folder:",with: "")
        let folder = state.items.first { $0.id == candidate && $0.folder && !$0.id.hasPrefix("collection-") }
        guard shared || folder != nil else { return nil }
        let item = UIBarButtonItem(image: shareImage(shared: shared),primaryAction: UIAction { [weak self] _ in
            guard let self,let current = self.state,!current.busy else { return }
            if shared {
                guard current.folder_access else { return }
                self.send("shared-folder-access",id: current.current,scope: current.editor_scope)
            } else if let folder {
                self.send("share-folder",id: folder.id,scope: browser ? current.browser?.listing.scope : current.listing?.scope)
            }
        })
        item.accessibilityIdentifier = "fei.folder.share"
        item.accessibilityLabel = state.labels[shared ? "shared-folder-access" : "share-object"]
        item.isEnabled = !state.busy
        return item
    }

    @objc private func shareNote(_ sender: UIBarButtonItem) {
        guard let state,let id = state.current,!state.busy else { return }
        let scope = state.editor_scope
        if !state.shared_session || state.editing_collaborators.isEmpty {
            send("share-note",id: id,scope: scope)
            return
        }
        guard let controller = host?.navigationController?.topViewController,controller.presentedViewController == nil else { return }
        let overview = FeiShareOverviewController(names: state.editing_collaborators,labels: state.labels) { [weak self] in
            guard let self,let overview = self.shareOverview else { return }
            overview.dismiss(animated: true) { [weak self] in
                guard let self,let next = self.state,next.current == id,next.editor_scope == scope,!next.busy else { return }
                self.send("share-note",id: id,scope: scope)
            }
        }
        overview.modalPresentationStyle = .popover
        overview.popoverPresentationController?.sourceItem = sender
        overview.popoverPresentationController?.delegate = self
        overview.loadViewIfNeeded()
        shareOverview = overview
        shareOverviewScope = scope
        controller.present(overview,animated: true)
    }

    func adaptivePresentationStyle(for controller: UIPresentationController,traitCollection: UITraitCollection) -> UIModalPresentationStyle {
        if controller.presentedViewController is FeiShareOverviewController { return .none }
        if let navigation = controller.presentedViewController as? UINavigationController,
           navigation.viewControllers.first is FeiSharedSyncController {
            return traitCollection.horizontalSizeClass == .compact ? .pageSheet : .none
        }
        return .automatic
    }

    private func newFolder(parentID: String? = nil) {
        guard let state, !state.busy, state.can_create, !isVirtualListing, let listing = state.listing, listing.kind == "notes" else { return }
        presentNewFolder(action: "new-folder", parentID: parentID, scope: listing.scope)
    }
    @objc private func openSettings() { send("settings") }
    @objc private func emptyTrash() { send("empty-trash") }

    private func label(_ key: String) -> String { state?.labels[key] ?? key }

    func localized(_ zh: String, _ en: String) -> String { state?.labels["cancel"] == "Cancel" ? en : zh }

    private func sheet(_ title: String? = nil, actions: [(String, UIAlertAction.Style, () -> Void)],anchor: SheetAnchor? = nil) {
        let rowAnchor: (String,Bool)?
        if case .row(let id,let browser) = anchor { rowAnchor = (id,browser) } else { rowAnchor = nil }
        guard let controller = rowAnchor?.1 == true ? host?.padWorkspace?.visibleBrowserController : host?.navigationController?.topViewController else { return }
        host?.settleEditorComposition()
        fei_ipad_flush()
        controller.view.endEditing(true)
        let sheet = UIAlertController(title: title, message: nil, preferredStyle: .actionSheet)
        for (key, style, action) in actions {
            sheet.addAction(UIAlertAction(title: label(key), style: style) { _ in action() })
        }
        sheet.addAction(UIAlertAction(title: label("cancel"), style: .cancel))
        if let popover = sheet.popoverPresentationController {
            if let (id,_) = rowAnchor {
                guard let rect = (controller as? FeiRouteController)?.notesList?.anchor(for: id) else { return }
                popover.sourceView = controller.view; popover.sourceRect = rect
            } else {
                guard let item = actionButton(in: controller) else { return }
                popover.sourceItem = item
            }
        }
        present(sheet,from: controller)
    }

    private func actionButton(in controller: UIViewController?) -> UIBarButtonItem? {
        controller?.navigationItem.rightBarButtonItems?.first { ["fei.more","fei.library-actions"].contains($0.accessibilityIdentifier ?? "") }
    }

    private func present(_ view: UIViewController,from source: UIViewController? = nil) {
        guard let controller = source ?? host?.navigationController?.topViewController else { return }
        presentationGeneration &+= 1
        let generation = presentationGeneration
        let show = { [weak self,weak controller] in
            guard let self,let controller,self.presentationGeneration == generation,
                self.host?.navigationController?.topViewController === controller || self.host?.padWorkspace?.visibleBrowserController === controller || self.documentSheet?.topViewController === controller else { return }
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
        return UIAction(title: label(key),image: symbol == "textformat.size" ? FeiEditorToolbar.menuFormatIcon(compatibleWith: host?.traitCollection) : UIImage(systemName: symbol),attributes: !enabled ? .disabled : destructive ? .destructive : []) { [weak self] _ in
            guard let self,self.state?.busy == false,self.state?.current == current,self.state?.editor_scope == scope,self.state?.listing?.scope == listingScope,self.state?.browser?.listing.scope == browserScope else { return }
            self.host?.settleEditorComposition(); perform()
        }
    }
    private func menuSection(_ items: [UIMenuElement]) -> UIMenu {
        UIMenu(options: .displayInline,children: items)
    }
    private func collaborationSection(_ access: UIAction?,state: FeiNavigationState,browser: Bool = false) -> UIMenu? {
        var items = access.map { [$0] } ?? []
        if state.shared_sync != nil { items.append(sharedSyncAction(state,browser: browser)) }
        return items.isEmpty ? nil : menuSection(items)
    }
    private func listActionsMenu(browser: Bool) -> UIMenu {
        guard let state else { return UIMenu() }
        if state.shared_session && !(browser && state.browser?.personal == true) {
            var groups: [UIMenuElement] = []
            if browser ? state.browser?.can_create == true : state.can_create {
                let create = [
                    menuAction("new","square.and.pencil") { [weak self] in self?.composeNote() },
                    menuAction("new-folder","folder.badge.plus") { [weak self] in
                        if browser { self?.createSidebarFolder() } else { self?.newFolder() }
                    }
                ]
                groups.append(UIMenu(title: label("new-in-folder"),image: UIImage(systemName: "plus"),children: create))
            }
            let access = hasSharedFolderShortcut(state,browser: browser) ? nil : menuAction("shared-folder-access","person.2") { [weak self] in
                self?.send("shared-folder-access",id: state.current,scope: state.editor_scope)
            }
            if let collaboration = collaborationSection(access,state: state,browser: browser) { groups.append(collaboration) }
            return UIMenu(children: groups)
        }
        let listing = browser ? state.browser?.listing : state.listing
        let shared = listing?.kind == "shared"
        let sortRules = ["manual","visited","modified","created","title"].map { key in
            UIAction(title: label(key),state: listing?.sort_rule == key ? .on : .off) { [weak self] _ in
                guard let self,let listing,self.state?.busy == false,
                      (browser ? self.state?.browser?.listing.scope : self.state?.listing?.scope) == listing.scope else { return }
                self.send("sort",value: key,scope: listing.scope)
                if key == "manual" && listing.can_reorder {
                    if browser { self.host?.padWorkspace?.beginBrowserReorder() }
                    else { (self.host?.navigationController?.topViewController as? FeiRouteController)?.notesList?.beginReordering() }
                }
            }
        }
        var sortEntries: [UIMenuElement] = sortRules
        if let listing,let foldersFirst = listing.folders_first {
            let folders = UIAction(title: label("folders-first"),attributes: listing.sort_rule == "manual" ? .disabled : [],
                                   state: foldersFirst && listing.sort_rule != "manual" ? .on : .off) { [weak self] _ in
                self?.send("sort",value: "folders-first",scope: listing.scope)
            }
            sortEntries.append(menuSection([folders]))
        }
        let sort = UIMenu(title: label("sort"),image: UIImage(systemName: "arrow.up.arrow.down"),children: sortEntries)
        var organize: [UIMenuElement] = []
        if (!browser && !state.shared_session) || listing?.can_select == true {
            organize.append(menuAction("batch","checkmark.circle") { [weak self] in self?.beginListSelection(browser: browser) })
        }
        if listing?.can_sort == true { organize.append(sort) }
        var menus: [UIMenuElement] = []
        if let listing,listing.shared_create {
            menus.append(menuSection([UIMenu(title: label("new-and-share"),image: UIImage(systemName: "person.badge.plus"),children: [
                menuAction("new-shared-document","square.and.pencil") { [weak self] in self?.send("new-shared-document",scope: listing.scope) },
                menuAction("new-shared-folder","folder.badge.plus") { [weak self] in self?.send("new-shared-folder",scope: listing.scope) }
            ])]))
        } else if !isVirtualListing {
            let create = [
                menuAction("new","square.and.pencil") { [weak self] in self?.composeNote() },
                menuAction("new-folder","folder.badge.plus") { [weak self] in
                    if browser { self?.createSidebarFolder() } else { self?.newFolder() }
                }
            ]
            menus.append(menuSection(create))
        }
        if !organize.isEmpty { menus.append(menuSection(organize)) }
        if shared {
            menus.append(menuSection([menuAction("accept-invitation","envelope.open") { [weak self] in self?.send("accept-invitation") }]))
        } else if let id = browser ? state.browser?.folder : state.routes.last?.key.replacingOccurrences(of: "folder:",with: ""),
                  let item = state.items.first(where: { $0.id == id && $0.folder }) {
            let share = menuAction("share-object","person.badge.plus") { [weak self] in self?.send("share-folder",id: id,scope: listing?.scope) }
            menus.append(menuSection([menuAction("rename","pencil") { [weak self] in self?.rename(id: id,title: item.title,browserScope: browser ? listing?.scope : nil) }]))
            menus.append(menuSection([share]))
        }
        if !isVirtualListing {
            menus.append(menuSection([menuAction("import","square.and.arrow.down") { [weak self] in self?.send(browser ? "browse-import" : "import",scope: browser ? listing?.scope : nil) }]))
        }
        return UIMenu(children: menus)
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
        let standaloneShare = state.shared_session && !state.folder_access
        let item = state.items.first(where: { $0.id == id })
        var organization: [UIMenuElement] = []
        if let item {
            let scope: String? = standaloneShare ? state.editor_scope : nil
            organization += [
                menuAction(item.pinned ? "unpin" : "pin","pin") { [weak self] in self?.send(standaloneShare ? "shared-pin" : "pin",id: id,scope: scope) },
                menuAction(item.starred ? "unstar" : "star","star") { [weak self] in self?.send(standaloneShare ? "shared-star" : "star",id: id,scope: scope) }
            ]
        }
        organization.append(menuAction("rename","pencil",enabled: state.write_allowed) { [weak self] in self?.rename(id: id,title: self?.state?.title ?? "") })
        if !standaloneShare {
            organization.append(menuAction("move","folder",enabled: state.write_allowed) { [weak self] in self?.showMove(id) })
        }
        organization.append(menuAction(standaloneShare ? "copy-shared-to-personal" : "duplicate","doc.on.doc",enabled: standaloneShare || state.write_allowed) { [weak self] in
            self?.send(standaloneShare ? "shared-copy" : "duplicate",id: id,scope: standaloneShare ? state.editor_scope : nil)
        })
        editing.append(menuAction(state.reading ? "editing" : "reading","book",enabled: state.write_allowed) { [weak self] in self?.send(state.reading ? "editing" : "reading",id: id) })
        let info = UIMenu(title: label("doc-info"),image: UIImage(systemName: "info.circle"),children: [menuAction("doc-info","info.circle") { [weak self] in self?.send("doc-info",id: id) },menuAction("history","clock.arrow.circlepath") { [weak self] in self?.send("history-open",id: id,scope: state.editor_scope) },menuAction("attachments","paperclip") { [weak self] in self?.send("attachments-menu") }])
        var export: [UIMenuElement] = [menuAction("export","square.and.arrow.up") { [weak self] in self?.showExport(id) },menuAction("pdf","doc.richtext") { [weak self] in self?.send("pdf",id: id) },menuAction("print","printer") { [weak self] in self?.send("print",id: id) }]
        if state.labels["copy-object-link"] != nil {
            export.insert(menuAction("copy-object-link","link") { [weak self] in self?.send("copy-object-link",id: id,scope: state.editor_scope) },at: 0)
        }
        var groups: [UIMenuElement] = [search,UIMenu(title: label("format"),image: FeiEditorToolbar.menuFormatIcon(compatibleWith: host?.traitCollection),children: editing)]
        groups.append(UIMenu(title: label("options"),image: UIImage(systemName: "folder"),children: organization))
        if state.shared_session {
            if let collaboration = collaborationSection(nil,state: state) { groups.append(collaboration) }
        } else {
            groups.append(menuSection([menuAction("share-object","person.badge.plus") { [weak self] in
                self?.send("share-note",id: id,scope: state.editor_scope)
            }]))
        }
        groups += [info,UIMenu(title: label("export-copy"),image: UIImage(systemName: "square.and.arrow.up"),children: export)]
        if standaloneShare {
            if let end = item?.share_end {
                groups.append(menuSection([menuAction(end,"trash",destructive: true) { [weak self] in self?.send("shared-end",id: id,scope: state.editor_scope) }]))
            }
        } else {
            groups.append(menuSection([menuAction("delete","trash",destructive: true,enabled: state.write_allowed) { [weak self] in self?.send("delete",id: id) }]))
        }
        return UIMenu(children: groups)
    }

    private func sharedSyncAction(_ state: FeiNavigationState,browser: Bool = false) -> UIAction {
        menuAction("shared-sync-status",state.shared_sync?.attention == true ? "exclamationmark.circle" : "arrow.triangle.2.circlepath") { [weak self] in
            self?.showSharedSync(scope: state.editor_scope,browser: browser)
        }
    }

    private func updateSharedSyncButton(_ item: UIBarButtonItem?,state: FeiNavigationState,folder: Bool) {
        guard let item else { return }
        item.accessibilityLabel = folder ? state.labels["folder-more"] : state.labels["note-more"]
        let attention = state.shared_sync?.attention == true
        item.accessibilityValue = attention ? state.labels["sync-needs-attention"] : nil
        let symbol = UIImage(systemName: "ellipsis",withConfiguration: UIImage.SymbolConfiguration(pointSize: 17))
        let theme = state.theme ?? .system
        let shared = state.shared_session
        if #available(iOS 26.0,*) {
            var badge = UIBarButtonItem.Badge.string("!")
            badge.backgroundColor = theme.syncWarningColor
            badge.foregroundColor = theme.syncWarningInk
            badge.font = .systemFont(ofSize: 8,weight: .bold)
            item.badge = shared && attention ? badge : nil
            item.image = symbol
            return
        }
        guard shared,let symbol else { item.image = symbol; return }
        // Keep the pre-iOS 26 sync warning in the same slot as the unbadged item.
        item.image = toolbarGlyph(symbol,size: CGSize(width: 27,height: 25),badge: attention ? "!" : nil)
    }

    private func restoreSharedSyncIfNeeded() {
        guard let scope = sharedSyncRestoreScope,let state,state.editor_scope == scope,
              state.shared_sync != nil,!state.busy,!applying,
              let navigation = host?.navigationController,navigation.transitionCoordinator == nil else { return }
        sharedSyncRestoreScope = nil
        showSharedSync(scope: scope,browser: state.kind == "folder" && host?.padWorkspace?.visibleBrowserController != nil)
    }

    private func showSharedSync(scope: String,browser: Bool) {
        guard let state,state.editor_scope == scope,state.shared_sync != nil,!state.busy,
              let host,let presenter = browser ? host.padWorkspace?.visibleBrowserController : host.navigationController?.topViewController else { return }
        let card = FeiSharedSyncController()
        let navigation = UINavigationController(rootViewController: card)
        let anchor = actionButton(in: presenter)
        if presenter.traitCollection.horizontalSizeClass == .regular,let anchor {
            navigation.modalPresentationStyle = .popover
            navigation.popoverPresentationController?.sourceItem = anchor
            navigation.popoverPresentationController?.delegate = self
            navigation.preferredContentSize = CGSize(width: 340,height: 320)
        } else {
            navigation.modalPresentationStyle = .pageSheet
            navigation.sheetPresentationController?.detents = [.medium(),.large()]
            navigation.sheetPresentationController?.prefersGrabberVisible = true
        }
        let done = UIBarButtonItem(title: state.labels["done"],image: nil,
            primaryAction: UIAction { [weak navigation] _ in navigation?.dismiss(animated: true) })
        done.style = .done
        done.accessibilityIdentifier = "fei.shared-sync.done"
        card.navigationItem.rightBarButtonItem = done
        navigation.overrideUserInterfaceStyle = presenter.navigationController?.overrideUserInterfaceStyle ?? .unspecified
        navigation.view.tintColor = (state.theme ?? .system).accentColor
        sharedSync = card
        sharedSyncScope = scope
        updateSharedSync(state)
        present(navigation,from: presenter)
    }

    private func updateSharedSync(_ state: FeiNavigationState) {
        guard let card = sharedSync else { return }
        guard state.editor_scope == sharedSyncScope,let status = state.shared_sync else {
            card.navigationController?.dismiss(animated: false)
            sharedSync = nil; sharedSyncScope = nil
            return
        }
        let scope = state.editor_scope
        let id = state.current
        card.update(status,labels: state.labels,warning: (state.theme ?? .system).syncWarningColor) { [weak self,weak card] operation in
            guard let self,let next = self.state,next.editor_scope == scope,next.current == id,next.shared_sync?.action == operation,!next.busy else { return }
            if operation == "retry" { self.send("shared-sync-retry",id: id,scope: scope) }
            else {
                card?.navigationController?.dismiss(animated: true) { [weak self] in
                    self?.send("shared-sync-settings",id: id,scope: scope)
                }
            }
        }
    }

    func itemMenu(_ id: String,scope: String,browsingWorkspace: Bool) -> UIMenu {
        let fromBrowser = browsingWorkspace
        guard let listing = fromBrowser ? state?.browser?.listing : state?.listing,
              listing.scope == scope,let item = rowItem(id,in: listing) else { return UIMenu() }
        let inherited = isInsideSharedFolder(browser: fromBrowser) && !item.shared
        let personalBrowser = fromBrowser && state?.shared_session == true && state?.browser?.personal == true && item.personal
        func action(_ key: String,_ symbol: String,_ command: String? = nil,danger: Bool = false,perform: (() -> Void)? = nil) -> UIAction {
            UIAction(title: label(key),image: UIImage(systemName: symbol),attributes: danger ? .destructive : []) { [weak self] _ in
                guard let self,self.state?.busy == false,(fromBrowser ? self.state?.browser?.listing.scope : self.state?.listing?.scope) == scope else { return }
                if let perform { perform() }
                else if personalBrowser && ["delete","duplicate","pdf","print"].contains(command ?? key) {
                    self.send("personal-row-action",value: (command ?? key) + ":",id: id,scope: scope)
                } else { self.send(command ?? key,id: id,scope: scope) }
            }
        }
        func group(_ items: [UIMenuElement]) -> UIMenu { UIMenu(options: .displayInline,children: items) }
        if item.offline_recovery { return UIMenu(children: [group([action("keep-recovery","doc.on.doc")]),group([action("discard-recovery","trash",danger: true)])]) }
        if item.trashed {
            let restore = action(item.folder ? "restore-folder" : "restore","arrow.uturn.backward","restore")
            let purge = action(item.folder ? "purge-folder" : "purge","trash","purge",danger: true)
            return UIMenu(children: [group([restore]),group([purge])])
        }
        if item.id.hasPrefix("collection-") {
            return UIMenu(children: [action("rename","pencil","collection-rename"),action("organize-shared-entry","folder",perform: { [weak self] in self?.showMove(id) }),action("remove-personal-folder","folder.badge.minus","collection-remove")])
        }
        var flags: [UIMenuElement] = item.folder ? [] : [action(item.pinned ? "unpin" : "pin","pin",item.shared ? "shared-pin" : "pin"),action(item.starred ? "unstar" : "star","star",item.shared ? "shared-star" : "star")]
        if state?.labels["copy-object-link"] != nil { flags.insert(action("copy-object-link","link"),at: 0) }
        if item.inherited_share && !inherited {
            return UIMenu(children: [group(flags),group([action("shared-folder-access","person.2","shared-child-access")])])
        }
        if item.shared {
            var organize: [UIMenuElement] = [action("organize-shared-entry","folder",perform: { [weak self] in self?.showMove(id) })]
            if !item.folder { organize.append(action("copy-shared-to-personal","doc.on.doc","shared-copy")) }
            let access = action(item.folder ? "shared-folder-access" : item.owned ? "members-and-access" : "view-share-access","person.2","shared-members")
            let groups = (flags.isEmpty ? [] : [group(flags)]) + [group(organize),group([access])]
            return UIMenu(children: groups + [group([action(item.share_end ?? "leave-shared","trash","shared-end",danger: true)])])
        }
        let row = listing.rows.first { $0.id == id }
        let shareActions: [UIMenuElement]
        if inherited {
            shareActions = [action("shared-folder-access","person.2",perform: { [weak self] in
                guard let self,let state = self.state,self.isInsideSharedFolder(browser: fromBrowser) else { return }
                self.send("shared-folder-access",id: state.current,scope: state.editor_scope)
            })]
        } else if let row {
            if item.folder {
                shareActions = [action(row.shared_folder ? "shared-folder-access" : "share-object",row.shared_folder ? "person.2" : "person.badge.plus","share-folder")]
            } else {
                shareActions = [action(row.shared_badge ? "members-and-access" : "share-object",row.shared_badge ? "person.2" : "person.badge.plus",row.shared_badge ? "shared-members" : "share-list-document")]
            }
        } else { shareActions = [] }
        var organize: [UIMenuElement] = [action("rename","pencil",perform: { [weak self] in self?.rename(id: id,title: item.title,browserScope: fromBrowser ? scope : nil) }),action("move","folder",perform: { [weak self] in self?.showMove([id],serial: nil,personalBrowser: personalBrowser) }),action("duplicate","doc.on.doc")]
        if item.folder && !inherited && listing.kind == "notes" {
            organize.insert(action("new-folder","folder.badge.plus",perform: { [weak self] in if browsingWorkspace { self?.createSidebarFolder(parentID: id) } else { self?.newFolder(parentID: id) } }),at: 0)
        }
        var groups: [UIMenuElement] = flags.isEmpty ? [] : [group(flags)]
        let canModify = !inherited || state?.can_create == true
        if canModify { groups.append(group(organize)) }
        if !shareActions.isEmpty { groups.append(group(shareActions)) }
        if !item.folder { groups.append(UIMenu(title: label("export-copy"),image: UIImage(systemName: "square.and.arrow.up"),children: [action("export","doc.text",perform: { [weak self] in self?.showExport(id,anchor: .row(id,browser: browsingWorkspace),personalScope: personalBrowser ? scope : nil) }),action("pdf","doc.richtext"),action("print","printer")])) }
        if canModify { groups.append(group([action(item.folder ? "delete-folder" : "delete","trash","delete",danger: true)])) }
        return UIMenu(children: groups)
    }

    private func isInsideSharedFolder(browser: Bool) -> Bool {
        state?.folder_access == true && (!browser || state?.browser?.personal == false)
    }
    func canModifyFolderContents(browser: Bool) -> Bool { !isInsideSharedFolder(browser: browser) || state?.can_create == true }
    func rowItem(_ id: String,in listing: FeiNavigationState.Listing) -> FeiNavigationState.Item? {
        let inTrash = listing.kind == "trash"
        return state?.items.first { $0.id == id && ($0.trashed || $0.offline_recovery) == inTrash }
    }
    func rowAction(_ key: String,id: String,scope: String,browser: Bool) {
        guard state?.busy == false,let listing = browser ? state?.browser?.listing : state?.listing,
              listing.scope == scope,let item = rowItem(id,in: listing) else { return }
        if ["delete","move"].contains(key) && !canModifyFolderContents(browser: browser) { return }
        if key=="move" { showMove(id); return }
        let command = item.shared ? ["pin":"shared-pin","star":"shared-star","delete":"shared-end"][key] ?? key : key
        send(command,id: id,scope: scope)
    }


    func browserActionsItem() -> UIBarButtonItem {
        let item = UIBarButtonItem(image: UIImage(systemName: "ellipsis"),primaryAction: nil,menu: deferredActions(note: false,browser: true))
        item.accessibilityIdentifier = "fei.library-actions"
        item.accessibilityLabel = label("more")
        item.isEnabled = state?.busy == false
        browserMore = item
        if let state,state.browser?.personal == false { updateSharedSyncButton(item,state: state,folder: true) }
        return item
    }

    func createSidebarFolder(parentID: String? = nil) {
        guard let state, let browser = state.browser, browser.can_create, !state.busy else { return }
        presentNewFolder(action: "browse-new-folder", parentID: parentID, scope: browser.listing.scope)
    }

    private func presentNewFolder(action: String, parentID: String?, scope: String) {
        let form = FeiRenameController(title: label("new-folder"),value: "",save: label("save"),cancel: label("cancel")) { [weak self] name in
            self?.send(action,value: name,id: parentID,scope: scope)
        }
        let navigation = UINavigationController(rootViewController: form)
        navigation.sheetPresentationController?.detents = [.medium(),.large()]
        present(navigation)
    }

    private func showMove(_ id: String) { showMove([id],serial: nil) }

    func showMove(_ ids: [String], serial: UInt64?, personalBrowser: Bool = false) {
        guard let state else { return }
        if serial != nil && (state.shared_session || !["notes","folder"].contains(state.kind) && !hasPadWorkspace) { return }
        let selected = Set(ids)
        let shortcuts = ids.allSatisfy { id in state.items.contains { $0.id == id && ($0.shared || $0.id.hasPrefix("collection-")) } }
        let rootRoute = hasPadWorkspace ? state.browser?.path.first : state.routes.first { $0.key.hasPrefix("folder:") }
        let sharedRoot = !shortcuts && isInsideSharedFolder(browser: hasPadWorkspace) ? rootRoute?.key.replacingOccurrences(of: "folder:",with: "") : nil
        let folders = state.items.filter { item in
            guard item.folder,!item.trashed,item.id != sharedRoot,item.id.hasPrefix("collection-") == shortcuts, !personalBrowser || item.personal else { return false }
            var parent: String? = item.id
            var seen = Set<String>()
            while let current = parent {
                if selected.contains(current) || !seen.insert(current).inserted { return false }
                if current == sharedRoot { return true }
                parent = state.items.first(where: { $0.id == current })?.parent
            }
            return sharedRoot == nil
        }
        let scope = visibleListing?.scope
        let root = shortcuts ? label("sharing") : sharedRoot.flatMap { root in state.items.first { $0.id == root }?.title } ?? label("root")
        let picker = FeiFolderPicker(title: label(shortcuts ? "organize-shared-entry" : "move"), cancel: label("cancel"), root: root, folders: folders,cancelled: { [weak self] in if let serial { self?.send("batch-move-cancel",serial: serial) } }) { [weak self] folder in
            if let serial { self?.send("batch-move",value: folder ?? "",serial: serial) }
            else if let id = ids.first {
                if personalBrowser { self?.send("personal-row-action",value: "move:" + (folder ?? ""),id: id,scope: scope) }
                else { self?.send(shortcuts ? "organize-shared-entry" : "move",value: folder ?? sharedRoot ?? "",id: id,scope: scope) }
            }
        }
        present(UINavigationController(rootViewController: picker))
    }

    private func showExport(_ id: String,anchor: SheetAnchor? = nil,personalScope: String? = nil) {
        let scope = state?.editor_scope
        func export(_ mode: String) {
            guard self.state?.editor_scope == scope else { return }
            if let personalScope { self.send("personal-row-action",value: "export:" + mode,id: id,scope: personalScope) }
            else { self.send("export",value: mode,id: id) }
        }
        sheet(label("export"),actions: [
            ("text-only",.default,{ export("text") }),
            ("all-content",.default,{ export("all") }),
        ],anchor: anchor)
    }

    private func rename(id: String?, title: String,browserScope: String? = nil) {
        let form = FeiRenameController(title: label("rename"),
            value: title, save: label("save"), cancel: label("cancel")) { [weak self] value in
            self?.send("rename",value: value,id: id,scope: browserScope)
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
