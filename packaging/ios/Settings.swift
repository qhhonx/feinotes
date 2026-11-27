import UIKit

struct FeiSettingsState: Decodable, Equatable, Sendable {
    struct Choice: Decodable, Equatable, Sendable { let value:String; let title:String; let preview:Preview? }
    struct Row: Decodable, Equatable, Sendable {
        let id:String; let title:String; let detail:String; let kind:String; let value:String
        let preview:Preview?
        let choices:[Choice]; let enabled:Bool; let danger:Bool
        var selectedTitle:String { choices.first(where: { $0.value == value })?.title ?? value }
    }
    struct Preview: Decodable, Equatable, Sendable {
        struct TextStyle: Decodable, Equatable, Sendable {
            let font:String; let size:CGFloat; let line:CGFloat; let weight:CGFloat; let ink:[CGFloat]
        }
        struct TaskStyle: Decodable, Equatable, Sendable {
            let size:CGFloat; let radius:CGFloat; let gap:CGFloat
            let fill:[CGFloat]; let check:[CGFloat]; let border:[CGFloat]; let empty:[CGFloat]
            let done_ink:[CGFloat]; let strike:Bool; let done_label:String; let pending_label:String
        }
        let title:String; let body:String; let highlight:String; let heading:String; let completed:String; let pending:String
        let paper:[CGFloat]; let title_style:TextStyle; let body_style:TextStyle; let heading_style:TextStyle
        let paragraph_gap:CGFloat; let heading_gap:CGFloat
        let highlight_fill:[CGFloat]?; let highlight_ink:[CGFloat]; let highlight_weight:CGFloat
        let tasks:TaskStyle
    }
    struct Section: Decodable, Equatable, Sendable { let title:String; let rows:[Row]; let footer:String? }
    let scope:String; let page:String; let sections:[Section]; let message:String?; let interface_size:String

    @MainActor func font(_ style:UIFont.TextStyle) -> UIFont {
        let base = UIFont.preferredFont(forTextStyle: style)
        let delta:CGFloat = interface_size == "large" ? 2 : interface_size == "small" ? -1 : 0
        return base.withSize(base.pointSize+delta)
    }
}

@MainActor
private final class FeiThemePreviewView: UIView {
    init(_ preview:FeiSettingsState.Preview) {
        super.init(frame: .zero)
        backgroundColor = color(preview.paper,fallback: .systemBackground)
        let title = label(preview.title,style: preview.title_style)
        let body = label(preview.body,style: preview.body_style)
        let highlight = label(preview.highlight,style: preview.body_style,weight: preview.highlight_weight,
                              ink: preview.highlight_ink,fill: preview.highlight_fill)
        let heading = label(preview.heading,style: preview.heading_style)
        heading.accessibilityTraits.insert(.header)
        let complete = task(preview.completed,done: true,preview: preview)
        let pending = task(preview.pending,done: false,preview: preview)
        let stack = UIStackView(arrangedSubviews: [title,body,highlight,heading,complete,pending])
        stack.axis = .vertical
        stack.spacing = max(2,preview.paragraph_gap)
        stack.setCustomSpacing(8,after: title)
        stack.setCustomSpacing(10,after: highlight)
        stack.setCustomSpacing(max(2,preview.heading_gap),after: heading)
        stack.setCustomSpacing(max(2,preview.tasks.gap),after: complete)
        directionalLayoutMargins = NSDirectionalEdgeInsets(top: 12,leading: 16,bottom: 16,trailing: 16)
        stack.translatesAutoresizingMaskIntoConstraints = false
        addSubview(stack)
        NSLayoutConstraint.activate([
            stack.topAnchor.constraint(equalTo: layoutMarginsGuide.topAnchor),
            stack.bottomAnchor.constraint(equalTo: layoutMarginsGuide.bottomAnchor),
            stack.leadingAnchor.constraint(equalTo: layoutMarginsGuide.leadingAnchor),
            stack.trailingAnchor.constraint(equalTo: layoutMarginsGuide.trailingAnchor),
        ])
    }

    required init?(coder:NSCoder) { fatalError("init(coder:) is unsupported") }

    private func color(_ rgba:[CGFloat],fallback:UIColor = .label) -> UIColor {
        guard rgba.count == 4 else { return fallback }
        return UIColor(red: rgba[0],green: rgba[1],blue: rgba[2],alpha: rgba[3])
    }

    private func label(_ text:String,style:FeiSettingsState.Preview.TextStyle,weight:CGFloat? = nil,
                       ink:[CGFloat]? = nil,fill:[CGFloat]? = nil,strike:Bool = false) -> UILabel {
        let weights:[UIFont.Weight] = [.ultraLight,.thin,.light,.regular,.medium,.semibold,.bold,.heavy,.black]
        let weightIndex = min(weights.count,max(1,Int(((weight ?? style.weight) / 100).rounded()))) - 1
        let pointSize = max(1,style.size)
        let system = UIFont.systemFont(ofSize: pointSize,weight: weights[weightIndex])
        let family = UIFontDescriptor(fontAttributes: [.family:style.font,.traits: [UIFontDescriptor.TraitKey.weight: weights[weightIndex]]])
        let font = style.font == ".SystemUIFont" ? system : UIFont(descriptor: family,size: pointSize)
        let paragraph = NSMutableParagraphStyle()
        paragraph.minimumLineHeight = style.line
        paragraph.lineBreakMode = .byWordWrapping
        var attributes:[NSAttributedString.Key:Any] = [.font:font,.foregroundColor:color(ink ?? style.ink),.paragraphStyle:paragraph]
        if let fill { attributes[.backgroundColor] = color(fill) }
        if strike { attributes[.strikethroughStyle] = NSUnderlineStyle.single.rawValue }
        let label = UILabel()
        label.attributedText = NSAttributedString(string: text,attributes: attributes)
        label.numberOfLines = 0
        label.lineBreakMode = .byWordWrapping
        return label
    }

    private func task(_ text:String,done:Bool,preview:FeiSettingsState.Preview) -> UIView {
        let style = preview.tasks
        let label = label(text,style: preview.body_style,ink: done ? style.done_ink : nil,strike: done && style.strike)
        label.accessibilityValue = done ? style.done_label : style.pending_label
        let marker = UIView()
        marker.backgroundColor = color(done ? style.fill : style.empty)
        marker.layer.cornerRadius = style.radius
        if done {
            let check = UIImageView(image: UIImage(systemName: "checkmark",withConfiguration: UIImage.SymbolConfiguration(weight: .semibold)))
            check.tintColor = color(style.check)
            check.contentMode = .scaleAspectFit
            check.translatesAutoresizingMaskIntoConstraints = false
            marker.addSubview(check)
            NSLayoutConstraint.activate([
                check.centerXAnchor.constraint(equalTo: marker.centerXAnchor),
                check.centerYAnchor.constraint(equalTo: marker.centerYAnchor),
                check.widthAnchor.constraint(equalTo: marker.widthAnchor,multiplier: 0.58),
                check.heightAnchor.constraint(equalTo: check.widthAnchor),
            ])
        } else {
            marker.layer.borderWidth = 1
            marker.layer.borderColor = color(style.border).cgColor
        }

        let row = UIView()
        marker.translatesAutoresizingMaskIntoConstraints = false
        label.translatesAutoresizingMaskIntoConstraints = false
        row.addSubview(marker)
        row.addSubview(label)
        let markerOffset = max(0,(preview.body_style.line-style.size)/2)
        NSLayoutConstraint.activate([
            marker.leadingAnchor.constraint(equalTo: row.leadingAnchor),
            marker.topAnchor.constraint(equalTo: row.topAnchor,constant: markerOffset),
            marker.widthAnchor.constraint(equalToConstant: style.size),
            marker.heightAnchor.constraint(equalTo: marker.widthAnchor),
            marker.bottomAnchor.constraint(lessThanOrEqualTo: row.bottomAnchor),
            label.leadingAnchor.constraint(equalTo: marker.trailingAnchor,constant: 8),
            label.trailingAnchor.constraint(equalTo: row.trailingAnchor),
            label.topAnchor.constraint(equalTo: row.topAnchor),
            label.bottomAnchor.constraint(equalTo: row.bottomAnchor),
        ])
        return row
    }
}

private extension UITableViewCell {
    func installContent(_ view:UIView) {
        view.translatesAutoresizingMaskIntoConstraints = false
        contentView.addSubview(view)
        NSLayoutConstraint.activate([
            view.topAnchor.constraint(equalTo: contentView.topAnchor),
            view.bottomAnchor.constraint(equalTo: contentView.bottomAnchor),
            view.leadingAnchor.constraint(equalTo: contentView.leadingAnchor),
            view.trailingAnchor.constraint(equalTo: contentView.trailingAnchor),
        ])
    }
}

extension UITableViewCell {
    func installFormRow(_ row: UIView) {
        selectionStyle = .none
        let padding = defaultContentConfiguration().directionalLayoutMargins
        if let button = row as? UIButton {
            button.configuration?.contentInsets = padding
            installContent(button)
            return
        }
        row.translatesAutoresizingMaskIntoConstraints = false
        contentView.addSubview(row)
        let margins = contentView.layoutMarginsGuide
        NSLayoutConstraint.activate([
            row.leadingAnchor.constraint(equalTo: margins.leadingAnchor),
            row.trailingAnchor.constraint(equalTo: margins.trailingAnchor),
            row.topAnchor.constraint(equalTo: contentView.topAnchor,constant: padding.top),
            row.bottomAnchor.constraint(equalTo: contentView.bottomAnchor,constant: -padding.bottom),
        ])
    }
}

private func sharingButtonInk(on accent: UIColor,traits: UITraitCollection) -> UIColor {
    var red: CGFloat = 0, green: CGFloat = 0, blue: CGFloat = 0, alpha: CGFloat = 0
    guard accent.resolvedColor(with: traits).getRed(&red,green: &green,blue: &blue,alpha: &alpha) else { return .white }
    func linear(_ value: CGFloat) -> CGFloat {
        value <= 0.04045 ? value / 12.92 : pow((value + 0.055) / 1.055,2.4)
    }
    let luminance = 0.2126 * linear(red) + 0.7152 * linear(green) + 0.0722 * linear(blue)
    return luminance > 0.179 ? .black : .white
}

@MainActor
final class FeiShareAcceptController: UITableViewController {
    private weak var coordinator: FeiNavigationCoordinator?
    private let link = UITextField()
    private lazy var submit = UIBarButtonItem(title: nil,style: .done,target: self,action: #selector(accept))
    private var scope: String?
    private var busy = false
    private var projectedLink: String?
    private var canAccept = false
    private var notice: String?
    private var noticeIsError = false

    init(coordinator: FeiNavigationCoordinator) {
        self.coordinator = coordinator
        super.init(style: .insetGrouped)
    }
    required init?(coder: NSCoder) { fatalError("init(coder:) is unsupported") }

    override func viewDidLoad() {
        super.viewDidLoad()
        tableView.backgroundColor = .systemGroupedBackground
        tableView.keyboardDismissMode = .interactive
        link.font = .preferredFont(forTextStyle: .body)
        link.adjustsFontForContentSizeCategory = true
        link.keyboardType = .URL
        link.returnKeyType = .go
        link.autocorrectionType = .no
        link.autocapitalizationType = .none
        link.clearButtonMode = .whileEditing
        link.accessibilityIdentifier = "fei.share.invitation-link"
        link.addTarget(self,action: #selector(updateSubmit),for: .editingChanged)
        link.addTarget(self,action: #selector(accept),for: .editingDidEndOnExit)
        submit.accessibilityIdentifier = "fei.share.accept"
        navigationItem.rightBarButtonItem = submit
    }

    override func tableView(_ tableView: UITableView,numberOfRowsInSection section: Int) -> Int { 1 }

    override func tableView(_ tableView: UITableView,cellForRowAt indexPath: IndexPath) -> UITableViewCell {
        let cell = UITableViewCell()
        cell.installFormRow(link)
        return cell
    }

    override func tableView(_ tableView: UITableView,viewForFooterInSection section: Int) -> UIView? {
        guard let notice else { return nil }
        let footer = UITableViewHeaderFooterView(reuseIdentifier: nil)
        var content = UIListContentConfiguration.groupedFooter()
        content.text = notice
        content.textProperties.numberOfLines = 0
        if noticeIsError { content.textProperties.color = .systemRed }
        footer.contentConfiguration = content
        footer.isAccessibilityElement = true
        footer.accessibilityLabel = notice
        footer.accessibilityIdentifier = "fei.share.invitation-status"
        footer.accessibilityTraits = .staticText
        return footer
    }

    func update(_ state: FeiSettingsState,busy: Bool) {
        loadViewIfNeeded()
        let rows = state.sections.flatMap(\.rows)
        guard let field = rows.first(where: { $0.kind == "invitation-link" }) else { return }
        self.busy = busy
        canAccept = field.enabled
        scope = state.scope
        if projectedLink != field.value {
            projectedLink = field.value
            link.text = field.value
        }
        link.isEnabled = !busy
        link.placeholder = field.detail
        link.accessibilityLabel = field.detail

        submit.title = coordinator?.localized("接受","Accept")
        submit.accessibilityLabel = field.title
        let status = rows.first(where: { $0.id == "share-message" || $0.id == "share-busy" || $0.id == "share-unavailable" })
        let message = status?.title ?? rows.first(where: { $0.id == "share-account-requirements" })?.title
        let isError = status != nil && !busy && canAccept
        if notice != message || noticeIsError != isError {
            notice = message
            noticeIsError = isError
            tableView.reloadData()
            if isError,let message { UIAccessibility.post(notification: .announcement,argument: message) }
        }
        updateSubmit()
    }

    @objc private func updateSubmit() { submit.isEnabled = canAccept && !busy && !trimmedLink.isEmpty }
    private var trimmedLink: String { link.text?.trimmingCharacters(in: .whitespacesAndNewlines) ?? "" }

    @objc private func accept() {
        guard canAccept,!busy,let scope,!trimmedLink.isEmpty else { return }
        link.resignFirstResponder()
        coordinator?.send("settings-command",value: trimmedLink,id: "share-accept",scope: scope)
    }
}

@MainActor
final class FeiSharingPanelController: UIViewController, UITextFieldDelegate, UITableViewDataSource, UITableViewDelegate {
    private enum AccessActionStyle { case add, copy, enable, invite, closeLink, end, retry, refresh }
    private final class SharingRowButton: UIButton {
        var rowAccessory: UITableViewCell.AccessoryType = .none
    }

    private weak var coordinator: FeiNavigationCoordinator?
    private let targetRow = UIListContentView(configuration: .subtitleCell())
    private struct Section {
        var title: String? = nil
        var footer: String? = nil
        var rows: [UIView]
    }
    private var sections: [Section] = []
    private var accessSections: [Section] = []
    private var audienceButtons: [UIButton] = []
    private var renderedAudience: FeiSettingsState.Row?
    private var renderedAudienceBusy = false
    private var renderedAudienceScope: String?
    private let permissionLabel = UILabel()
    private let permissionRow = UIStackView()
    private let permission = UIButton(type: .system)
    private let name = UITextField()
    private let email = UITextField()
    private let formError = UILabel()
    private let status = UILabel()
    private let statusBanner = UIStackView()
    private let resultTitle = UILabel()
    private var usesNavigationSubmit = false
    private lazy var createItem: UIBarButtonItem = {
        let item = UIBarButtonItem(title: nil,style: .done,target: self,action: #selector(submitInvitation))
        item.accessibilityIdentifier = "fei.share.create"
        return item
    }()
    var invitationSubmitItem: UIBarButtonItem? { usesNavigationSubmit ? createItem : nil }
    private let accessRefresh = UIRefreshControl()
    private let scroll = UITableView(frame: .zero,style: .insetGrouped)
    private var renderedAccess: FeiSettingsState?
    private var renderedAccessBusy = false
    private var renderedAccent: UIColor?
    private var displayedStage: String?
    private var pendingAction: String?
    private var actionProgress: String?
    private var accessOffsetBeforeInvitation: CGFloat?
    private let alternate = UIButton(type: .system)
    private let submit = UIButton(type: .system)
    private var scope: String?
    private var hasEditedName = false
    private var projectedEmail: String?
    private var primary: FeiSettingsState.Row?
    private var secondary: FeiSettingsState.Row?
    private var busy = false
    private var nameEditable = false
    private var hasRecipient = false

    init(coordinator: FeiNavigationCoordinator) {
        self.coordinator = coordinator
        super.init(nibName: nil,bundle: nil)
    }
    required init?(coder: NSCoder) { fatalError("init(coder:) is unsupported") }

    override func viewDidLoad() {
        super.viewDidLoad()
        view.backgroundColor = .systemGroupedBackground

        for label in [formError,status] {
            label.font = .preferredFont(forTextStyle: .footnote)
            label.textColor = .secondaryLabel
        }
        resultTitle.font = .preferredFont(forTextStyle: .title2)
        for label in [permissionLabel,formError,status,resultTitle] {
            label.adjustsFontForContentSizeCategory = true
            label.numberOfLines = 0
        }
        status.accessibilityIdentifier = "fei.share.invitation-status"
        formError.accessibilityIdentifier = "fei.share.form-error"
        formError.textColor = .systemRed
        resultTitle.accessibilityIdentifier = "fei.share.invitation-result"

        permissionLabel.font = .preferredFont(forTextStyle: .body)
        permissionLabel.setContentHuggingPriority(.required,for: .horizontal)
        permissionRow.addArrangedSubview(permissionLabel)
        permissionRow.addArrangedSubview(permission)
        permissionRow.alignment = .center
        permissionRow.spacing = 8
        permission.showsMenuAsPrimaryAction = true
        let fields = [(name,"fei.setting.share-new-name"),(email,"fei.setting.share-invite")]
        for (field,identifier) in fields {
            field.borderStyle = .none
            field.font = .preferredFont(forTextStyle: .body)
            field.adjustsFontForContentSizeCategory = true
            field.clearButtonMode = .whileEditing
            field.delegate = self
            field.accessibilityIdentifier = identifier
            field.addTarget(self,action: #selector(fieldChanged(_:)),for: .editingChanged)
        }
        email.keyboardType = .emailAddress
        email.textContentType = .emailAddress
        email.autocapitalizationType = .none
        email.autocorrectionType = .no
        email.returnKeyType = .go

        submit.configuration = .filled()
        submit.accessibilityIdentifier = "fei.share.create"
        submit.addTarget(self,action: #selector(submitInvitation),for: .touchUpInside)
        alternate.configuration = .plain()
        alternate.addTarget(self,action: #selector(secondaryAction),for: .touchUpInside)
        for button in [submit,alternate] { applyTouchHeight(button) }

        accessRefresh.addTarget(self,action: #selector(refreshAccess),for: .valueChanged)
        statusBanner.axis = .vertical
        statusBanner.addArrangedSubview(status)
        statusBanner.isLayoutMarginsRelativeArrangement = true
        statusBanner.directionalLayoutMargins = NSDirectionalEdgeInsets(top: 6,leading: 0,bottom: 6,trailing: 0)
        let statusSlot = stack([statusBanner],spacing: 0)
        let footer = stack([submit,alternate],spacing: 4)
        scroll.backgroundColor = .systemGroupedBackground
        scroll.dataSource = self
        scroll.delegate = self
        scroll.rowHeight = UITableView.automaticDimension
        scroll.keyboardDismissMode = .interactive
        scroll.accessibilityIdentifier = "fei.share.panel-scroll"
        for child in [statusSlot,scroll,footer] { child.translatesAutoresizingMaskIntoConstraints = false }
        view.addSubview(statusSlot)
        view.addSubview(scroll)
        view.addSubview(footer)
        let safe = view.safeAreaLayoutGuide
        NSLayoutConstraint.activate([
            statusSlot.topAnchor.constraint(equalTo: safe.topAnchor),
            statusSlot.leadingAnchor.constraint(equalTo: safe.leadingAnchor,constant: 20),
            statusSlot.trailingAnchor.constraint(equalTo: safe.trailingAnchor,constant: -20),
            scroll.topAnchor.constraint(equalTo: statusSlot.bottomAnchor),
            scroll.leadingAnchor.constraint(equalTo: safe.leadingAnchor),
            scroll.trailingAnchor.constraint(equalTo: safe.trailingAnchor),
            scroll.bottomAnchor.constraint(equalTo: footer.topAnchor),
            footer.leadingAnchor.constraint(equalTo: safe.leadingAnchor,constant: 20),
            footer.trailingAnchor.constraint(equalTo: safe.trailingAnchor,constant: -20),
            footer.bottomAnchor.constraint(equalTo: view.keyboardLayoutGuide.topAnchor),
        ])
    }

    private func showSections(_ next: [Section]) {
        let same = sections.count == next.count && zip(sections,next).allSatisfy { old,new in
            old.title == new.title && old.footer == new.footer
                && old.rows.map(ObjectIdentifier.init) == new.rows.map(ObjectIdentifier.init)
        }
        guard !same else { return }
        sections = next
        scroll.reloadData()
    }

    func numberOfSections(in tableView: UITableView) -> Int { sections.count }
    func tableView(_ tableView: UITableView,numberOfRowsInSection section: Int) -> Int { sections[section].rows.count }
    func tableView(_ tableView: UITableView,titleForHeaderInSection section: Int) -> String? { sections[section].title }
    func tableView(_ tableView: UITableView,titleForFooterInSection section: Int) -> String? { sections[section].footer }

    func tableView(_ tableView: UITableView,cellForRowAt indexPath: IndexPath) -> UITableViewCell {
        let cell = UITableViewCell(style: .default,reuseIdentifier: nil)
        let row = sections[indexPath.section].rows[indexPath.row]
        if let list = row as? UIListContentView {
            cell.contentConfiguration = list.configuration
            cell.accessibilityIdentifier = list.accessibilityIdentifier
            cell.selectionStyle = .none
        } else {
            if let button = row as? SharingRowButton { cell.accessoryType = button.rowAccessory }
            cell.installFormRow(row)
        }
        return cell
    }

    private func revealRow(_ row: UIView) {
        for (index,section) in sections.enumerated() {
            if let item = section.rows.firstIndex(where: { $0 === row }) {
                scroll.scrollToRow(at: IndexPath(row: item,section: index),at: .middle,animated: false)
                return
            }
        }
    }

    private func applyTouchHeight(_ view: UIView) {
        let height = view.heightAnchor.constraint(greaterThanOrEqualToConstant: 44)
        height.priority = .defaultHigh
        height.isActive = true
    }

    private func stack(_ views: [UIView],spacing: CGFloat) -> UIStackView {
        let stack = UIStackView(arrangedSubviews: views)
        stack.axis = .vertical
        stack.spacing = spacing
        return stack
    }

    func update(_ model: FeiSettingsState,busy: Bool,theme: FeiMobileTheme) {
        loadViewIfNeeded()
        let rows = model.sections.flatMap(\.rows)
        func row(_ id: String) -> FeiSettingsState.Row? { rows.first { $0.id == id } }
        let result = row("share-result")
        let accessMode = row("share-access-panel")
        let managing = result == nil && accessMode != nil
        let stage = result != nil ? "result" : managing ? "access" : "invitation"
        let pageChanged = displayedStage != stage || scope != model.scope
        let restoreOffset = pageChanged && managing && displayedStage == "invitation" ? accessOffsetBeforeInvitation : nil
        if pageChanged {
            if stage != "invitation" { accessOffsetBeforeInvitation = nil }
            displayedStage = stage
        }
        let target = row("share-target")
        let draftName = row("share-new-name")
        let recipient = row("share-invite")
        let audience = row("share-audience")
        let access = row("share-public-permission") ?? row("share-permission")
        let feedback = row("share-message")
        if !busy { pendingAction = nil }
        actionProgress = busy ? row("share-busy")?.title : nil

        if scope != model.scope {
            hasEditedName = false; projectedEmail = nil
            name.text = nil; email.text = nil
        }
        scope = model.scope
        self.busy = busy
        let kind = result?.value ?? accessMode?.value ?? row("share-create-form")?.value
        var object = UIListContentConfiguration.subtitleCell()
        let targetSymbol: String
        switch kind {
        case "folder": targetSymbol = "folder"
        case "board": targetSymbol = "scribble.variable"
        default: targetSymbol = "doc.text"
        }
        object.image = UIImage(systemName: targetSymbol)
        object.text = target?.title ?? (draftName?.kind == "info" ? draftName?.value : nil) ?? title
        object.secondaryText = (kind == "folder" ? target?.detail : nil) ?? row("share-effect")?.title
        object.textProperties.font = .preferredFont(forTextStyle: .headline,compatibleWith: traitCollection)
        object.textProperties.adjustsFontForContentSizeCategory = true
        object.textProperties.numberOfLines = 0
        object.secondaryTextProperties.numberOfLines = 0
        targetRow.configuration = object
        if let index = sections.firstIndex(where: { $0.rows.first === targetRow }),
           let visible = scroll.cellForRow(at: IndexPath(row: 0,section: index)) {
            visible.contentConfiguration = object
        }

        nameEditable = draftName?.kind == "input"
        name.placeholder = draftName?.title
        name.accessibilityLabel = draftName?.title
        name.isEnabled = !busy && draftName?.enabled == true
        name.returnKeyType = recipient != nil ? .next : .go
        if !hasEditedName, name.text != draftName?.value { name.text = draftName?.value }
        hasRecipient = recipient != nil
        email.placeholder = recipient?.detail
        email.accessibilityLabel = recipient?.detail
        email.isEnabled = !busy && (recipient?.enabled == true || nameEditable)
        if let recipient, projectedEmail != recipient.value {
            projectedEmail = recipient.value
            email.text = recipient.value
        }

        updateAudience(audience,scope: model.scope)
        updatePermission(access,scope: model.scope)
        let help: String?
        if let requirements = row("share-link-details")?.title,!managing {
            help = [row("public-edit-warning")?.title,requirements].compactMap { $0 }.joined(separator: "\n")
        } else {
            help = recipient == nil ? nil : model.sections.compactMap(\.footer).first
        }
        resultTitle.text = result?.title
        navigationItem.leftBarButtonItem = nil
        updateAccess(managing ? model : nil,accent: theme.sharingAccentColor)
        if let back = row("share-back") {
            let item = accessBackButton("fei.share.invitation-back") { [weak self] in
                self?.send(back,scope: model.scope)
            }
            item.isEnabled = !busy && back.enabled
            navigationItem.leftBarButtonItem = item
        }
        navigationItem.title = title
        let message = feedback?.title
        let error = feedback?.danger == true
        let inlineError = stage == "invitation" && error
        let announceError = error && status.text != message && formError.text != message
        formError.text = inlineError ? message : nil
        formError.isHidden = !inlineError
        status.text = inlineError ? nil : message
        status.textColor = error ? .systemRed : .secondaryLabel
        statusBanner.isHidden = status.text?.isEmpty != false
        if announceError {
            UIAccessibility.post(notification: .announcement,argument: message)
            if inlineError {
                DispatchQueue.main.async { [weak self] in
                    guard let self,!self.formError.isHidden else { return }
                    self.revealRow(self.formError)
                }
            }
        }

        primary = row("share-show-members") ?? recipient ?? row("share-submit")
        secondary = result == nil ? nil : row("share-copy")
        usesNavigationSubmit = stage == "invitation" && primary != nil && primary?.danger != true
        createItem.title = row("share-create-form") == nil ? primary?.title : coordinator?.localized("创建","Create")
        createItem.accessibilityLabel = primary?.title
        submit.configuration?.title = primary?.title
        let submitPending = pendingAction.map { primary?.id == $0 || ($0 == "share-end" && primary?.id == "share-submit") } ?? false
        submit.accessibilityValue = submitPending ? actionProgress : nil
        submit.configuration?.showsActivityIndicator = submit.accessibilityValue != nil
        let submitColor: UIColor = primary?.danger == true ? .systemRed : theme.sharingAccentColor
        submit.configuration?.baseBackgroundColor = submitColor
        submit.configuration?.baseForegroundColor = primary?.danger == true
            ? sharingButtonInk(on: submitColor,traits: traitCollection) : .white
        createItem.tintColor = submitColor
        createItem.setTitleTextAttributes([.foregroundColor: UIColor.white],for: .normal)
        if result != nil { submit.accessibilityIdentifier = "fei.share.show-members" }
        else { submit.accessibilityIdentifier = "fei.share.create" }
        alternate.configuration?.title = secondary?.title
        alternate.accessibilityValue = pendingAction != nil && secondary?.id == pendingAction ? actionProgress : nil
        alternate.configuration?.showsActivityIndicator = alternate.accessibilityValue != nil
        alternate.accessibilityIdentifier = secondary.map { "fei.setting." + $0.id }
        alternate.isHidden = secondary == nil
        alternate.isEnabled = !busy && secondary?.enabled == true
        updateSubmit()
        var body = [Section(rows: [targetRow])]
        if managing {
            body += accessSections
        } else if result != nil {
            body.append(Section(rows: [resultTitle]))
        } else {
            if nameEditable { body.append(Section(title: draftName?.title,rows: [name])) }
            if let audience,!audienceButtons.isEmpty { body.append(Section(title: audience.title,rows: audienceButtons)) }
            var fields: [UIView] = []
            if access != nil { fields.append(permissionRow) }
            if hasRecipient { fields.append(email) }
            if inlineError { fields.append(formError) }
            if !fields.isEmpty { body.append(Section(footer: help,rows: fields)) }
        }
        showSections(body)
        if pageChanged { scroll.setContentOffset(CGPoint(x: 0,y: -scroll.adjustedContentInset.top),animated: false) }
        if let restoreOffset,restoreOffset > 0 {
            view.layoutIfNeeded()
            let maximum = max(0,scroll.contentSize.height - scroll.bounds.height + scroll.adjustedContentInset.bottom)
            scroll.setContentOffset(CGPoint(x: 0,y: min(restoreOffset,maximum)),animated: false)
        }
    }

    private func updateAudience(_ row: FeiSettingsState.Row?,scope: String) {
        guard row != renderedAudience || renderedAudienceBusy != busy || renderedAudienceScope != scope else { return }
        renderedAudience = row
        renderedAudienceBusy = busy
        renderedAudienceScope = scope
        audienceButtons = []
        guard let row else { return }
        audienceButtons = row.choices.map { choice in
            let selected = choice.value == row.value
            var config = UIButton.Configuration.plain()
            config.title = choice.title
            config.baseForegroundColor = .label
            config.titleAlignment = .leading
            config.titleLineBreakMode = .byWordWrapping
            let button = SharingRowButton(configuration: config,primaryAction: UIAction { [weak self] _ in
                self?.send(row,value: choice.value,scope: scope)
            })
            button.contentHorizontalAlignment = .leading
            if selected {
                button.rowAccessory = .checkmark
                button.accessibilityTraits.insert(.selected)
            }
            button.isEnabled = row.enabled && !busy
            button.accessibilityIdentifier = "fei.setting.option." + row.id + "." + choice.value
            return button
        }
    }

    private func updatePermission(_ row: FeiSettingsState.Row?,scope: String) {
        guard let row else { return }
        permissionLabel.text = row.title
        var config = UIButton.Configuration.plain()
        config.title = row.selectedTitle
        config.baseForegroundColor = .secondaryLabel
        config.image = UIImage(systemName: "chevron.up.chevron.down",withConfiguration: UIImage.SymbolConfiguration(textStyle: .caption1))
        config.imagePlacement = .trailing
        config.imagePadding = 6
        config.contentInsets = .zero
        permission.configuration = config
        permission.contentHorizontalAlignment = .trailing
        permission.isEnabled = !busy && row.enabled
        permission.accessibilityLabel = row.title + " · " + row.selectedTitle
        permission.accessibilityIdentifier = "fei.setting." + row.id
        permission.menu = UIMenu(title: row.title,children: row.choices.map { choice in
            UIAction(title: choice.title,state: choice.value == row.value ? .on : .off) { [weak self] _ in
                self?.send(row,value: choice.value,scope: scope)
            }
        })
    }

    private func updateAccess(_ model: FeiSettingsState?,accent: UIColor) {
        let sameInputs = renderedAccess == model && renderedAccessBusy == busy && renderedAccent?.isEqual(accent) == true
        guard !sameInputs else { return }
        let previousLinkError = renderedAccess?.sections.lazy.flatMap(\.rows).first { $0.id == "share-link-error" }?.title
        renderedAccess = model; renderedAccessBusy = busy; renderedAccent = accent
        accessSections = []
        let rows = model?.sections.flatMap(\.rows) ?? []
        func row(_ id: String) -> FeiSettingsState.Row? { rows.first { $0.id == id } }
        let refresh = row("share-refresh")
        if refresh == nil || !busy { accessRefresh.endRefreshing() }
        scroll.refreshControl = refresh == nil ? nil : accessRefresh
        scroll.alwaysBounceVertical = refresh != nil
        accessRefresh.accessibilityLabel = refresh?.title
        guard let model else { return }

        if let effect = row("share-effect") {
            accessSections.append(Section(rows: [accessLabel(effect.title)]))
            return
        }
        if let access = row("access"),let source = row("share-source") {
            var facts = [source.value.isEmpty ? accessLabel(source.title,color: .secondaryLabel)
                : accessFact(source.title,value: source.value,id: source.id)]
            facts.append(accessFact(access.title,value: access.value,id: access.id))
            accessSections.append(Section(rows: facts))
            if let retry = row("share-access-retry") {
                accessSections.append(Section(rows: [accessAction(retry,scope: model.scope,accent: accent,style: .retry)]))
            }
            return
        }

        let members = model.sections.first { $0.rows.first?.id == "access" }
        let links = model.sections.first { $0.rows.first?.id == "share-audience-info" }
        let peopleTitle = members?.title ?? coordinator?.localized("参与者","Participants") ?? ""
        let peopleRows = members?.rows.filter { $0.id == "access" || $0.id.hasPrefix("member:") } ?? []
        var people = peopleRows.map { accessInfo($0,scope: model.scope) }
        if let refresh { people.append(accessAction(refresh,scope: model.scope,accent: accent,style: .refresh)) }
        if let add = row("share-add") { people.append(accessAction(add,scope: model.scope,accent: accent,style: .add)) }
        if !people.isEmpty {
            accessSections.append(Section(title: peopleTitle,rows: people))
        }

        let enable = row("share-retry-link")
        let linkError = row("share-link-error")
        let linkTitle = links?.title ?? coordinator?.localized("链接访问","Link access") ?? ""
        var linkRows: [UIView] = []
        if let audience = row("share-audience-info") {
            let scopeLabel = coordinator?.localized("访问范围","Access scope") ?? ""
            linkRows.append(accessFact(scopeLabel,value: audience.title,id: audience.id))
            if let permission = row("share-link-permission") {
                linkRows.append(accessFact(permission.title,value: permission.value,id: permission.id))
            }
        } else {
            let checking = coordinator?.localized("正在读取访问范围…","Checking who can access…") ?? ""
            linkRows.append(accessLabel(checking,style: .footnote,color: .secondaryLabel))
        }
        if let error = linkError {
            let notice = accessLabel(error.title,style: .footnote,color: .systemRed)
            notice.accessibilityIdentifier = "fei.share.link-error"
            linkRows.append(notice)
            if previousLinkError != error.title {
                UIAccessibility.post(notification: .announcement,argument: error.title)
                DispatchQueue.main.async { [weak self,weak notice] in
                    guard let self,let notice else { return }
                    self.revealRow(notice)
                }
            }
        }
        let copy = row("share-copy")
        let close = row("share-close-link")
        if let enable { linkRows.append(accessAction(enable,scope: model.scope,accent: accent,style: .enable)) }
        if let copy { linkRows.append(accessAction(copy,scope: model.scope,accent: accent,style: .copy)) }
        if let close { linkRows.append(accessAction(close,scope: model.scope,accent: accent,style: .closeLink)) }
        if linkError != nil,let invite = row("share-link-invite") {
            linkRows.append(accessAction(invite,scope: model.scope,accent: accent,style: .invite))
        }
        let details = linkError == nil && row("share-link-permission") == nil ? row("share-link-details")?.title : nil
        accessSections.append(Section(title: linkTitle,footer: details,rows: linkRows))
        if let end = row("share-end") {
            accessSections.append(Section(rows: [accessAction(end,scope: model.scope,accent: accent,style: .end)]))
        }
    }

    private func accessLabel(_ text: String,style: UIFont.TextStyle = .body,color: UIColor = .label) -> UILabel {
        let label = UILabel()
        label.text = text
        label.font = .preferredFont(forTextStyle: style)
        label.adjustsFontForContentSizeCategory = true
        label.numberOfLines = 0
        label.textColor = color
        return label
    }

    private func accessFact(_ title: String,value: String,id: String) -> UIView {
        let largeText = traitCollection.preferredContentSizeCategory.isAccessibilityCategory
        var content: UIListContentConfiguration = largeText ? .subtitleCell() : .valueCell()
        let body = UIFont.preferredFont(forTextStyle: .body,compatibleWith: traitCollection)
        content.text = title
        content.secondaryText = value
        content.textProperties.font = body
        content.secondaryTextProperties.font = body
        content.textProperties.adjustsFontForContentSizeCategory = true
        content.secondaryTextProperties.adjustsFontForContentSizeCategory = true
        content.textProperties.numberOfLines = 0
        content.secondaryTextProperties.numberOfLines = 0
        let view = UIListContentView(configuration: content)
        view.accessibilityIdentifier = "fei.setting." + id
        return view
    }

    private func accessAction(_ row: FeiSettingsState.Row,scope: String,accent: UIColor,style: AccessActionStyle) -> UIButton {
        var config = UIButton.Configuration.plain()
        config.baseForegroundColor = style == .end ? .systemRed : style == .closeLink || style == .refresh ? .label : accent
        let symbol: String?
        switch style {
        case .add,.invite: symbol = "plus"
        case .copy: symbol = "doc.on.doc"
        case .enable: symbol = "link"
        case .closeLink,.end,.retry,.refresh: symbol = nil
        }
        config.title = row.title
        config.titleAlignment = .leading
        config.titleLineBreakMode = .byWordWrapping
        config.showsActivityIndicator = actionProgress != nil && pendingAction == row.id
        config.image = symbol.flatMap { UIImage(systemName: $0) }
        config.preferredSymbolConfigurationForImage = UIImage.SymbolConfiguration(textStyle: .body,scale: .small)
        config.imagePadding = UIListContentConfiguration.cell().imageToTextPadding
        let button = SharingRowButton(configuration: config,primaryAction: UIAction { [weak self] _ in
            guard let self else { return }
            if style == .closeLink {
                self.confirm(row.title,detail: row.detail) { self.send(row,value: "confirm",scope: scope) }
            } else { self.send(row,scope: scope) }
        })
        button.contentHorizontalAlignment = .leading
        if style == .add || style == .invite { button.rowAccessory = .disclosureIndicator }
        button.accessibilityIdentifier = "fei.setting." + row.id
        button.isEnabled = row.enabled && !busy
        button.accessibilityValue = config.showsActivityIndicator ? actionProgress : nil
        return button
    }

    @objc private func refreshAccess() {
        guard !busy,let model = renderedAccess,
            let refresh = model.sections.lazy.flatMap(\.rows).first(where: { $0.id == "share-refresh" && $0.enabled })
        else { accessRefresh.endRefreshing(); return }
        send(refresh,scope: model.scope)
    }

    private func accessBackButton(_ identifier: String,action: @escaping () -> Void) -> UIBarButtonItem {
        let item = UIBarButtonItem(title: coordinator?.localized("返回","Back"),image: nil,primaryAction: UIAction { _ in action() })
        item.accessibilityIdentifier = identifier
        return item
    }

    private func accessInfo(_ row: FeiSettingsState.Row,scope: String) -> UIView {
        if row.kind != "choice",row.detail.isEmpty,!row.value.isEmpty {
            return accessFact(row.title,value: row.value,id: row.id)
        }
        let label = UILabel()
        label.text = row.title
        label.font = .preferredFont(forTextStyle: .body)
        label.adjustsFontForContentSizeCategory = true
        label.numberOfLines = 0
        if !row.id.hasPrefix("member:") && row.value.isEmpty && row.detail.isEmpty { label.textColor = .secondaryLabel }
        let detail = UILabel()
        detail.text = row.detail
        detail.font = .preferredFont(forTextStyle: .footnote)
        detail.textColor = .secondaryLabel
        detail.adjustsFontForContentSizeCategory = true
        detail.numberOfLines = 0
        detail.isHidden = row.detail.isEmpty
        let labels = stack([label,detail],spacing: 2)
        let line = UIStackView(arrangedSubviews: [labels])
        line.alignment = .center
        line.spacing = 8
        switch row.kind {
        case "choice":
            var config = UIButton.Configuration.plain()
            config.title = row.selectedTitle
            config.baseForegroundColor = .label
            config.image = UIImage(systemName: "chevron.down",withConfiguration: UIImage.SymbolConfiguration(textStyle: .footnote))
            config.imagePlacement = .trailing
            config.imagePadding = 5
            config.contentInsets = .zero
            let button = UIButton(configuration: config)
            button.showsMenuAsPrimaryAction = true
            button.isEnabled = row.enabled && !busy
            button.accessibilityLabel = row.title + " · " + row.selectedTitle
            button.accessibilityIdentifier = "fei.setting." + row.id
            let changes = row.choices.filter { $0.value != "revoke" }.map { choice in
                UIAction(title: choice.title,state: choice.value == row.value ? .on : .off) { [weak self] _ in
                    if choice.value != row.value { self?.send(row,value: choice.value,scope: scope) }
                }
            }
            var choices: [UIMenuElement] = changes.map { $0 }
            if let removal = row.choices.first(where: { $0.value == "revoke" }) {
                let action = UIAction(title: removal.title,attributes: .destructive) { [weak self] _ in
                    guard let self else { return }
                    let pending = removal.title == self.coordinator?.localized("取消邀请","Cancel invitation")
                    let detail = pending
                        ? self.coordinator?.localized("\(row.title) 将无法再使用这份邀请。","\(row.title) will no longer be able to use this invitation.")
                        : self.coordinator?.localized("\(row.title) 将失去访问权限。","\(row.title) will lose access.")
                    self.confirm(removal.title,detail: detail ?? row.title) { self.send(row,value: removal.value,scope: scope) }
                }
                choices.append(UIMenu(options: .displayInline,children: [action]))
            }
            button.menu = UIMenu(children: choices)
            button.setContentHuggingPriority(.required,for: .horizontal)
            line.addArrangedSubview(button)
        default:
            if !row.value.isEmpty {
                let value = accessLabel(row.value,color: .secondaryLabel)
                value.textAlignment = view.effectiveUserInterfaceLayoutDirection == .rightToLeft ? .left : .right
                value.setContentHuggingPriority(.required,for: .horizontal)
                line.addArrangedSubview(value)
            }
        }
        return line
    }

    private func confirm(_ title: String,detail: String,action: @escaping () -> Void) {
        guard !busy,presentedViewController == nil else { return }
        let alert = UIAlertController(title: title,message: detail,preferredStyle: .alert)
        alert.addAction(UIAlertAction(title: coordinator?.localized("取消","Cancel"),style: .cancel))
        alert.addAction(UIAlertAction(title: title,style: .destructive) { _ in action() })
        present(alert,animated: true)
    }

    private func send(_ row: FeiSettingsState.Row,value: String = "",scope: String) {
        guard !busy,row.enabled,self.scope == scope else { return }
        if row.id == "share-add" || row.id == "share-link-invite" { accessOffsetBeforeInvitation = scroll.contentOffset.y }
        pendingAction = row.id
        coordinator?.send("settings-command",value: value,id: row.id,scope: scope)
    }

    @objc private func fieldChanged(_ sender: UITextField) {
        if sender === name, let scope, !busy {
            // The bridge may deliver earlier snapshots after a newer keystroke.
            hasEditedName = true
            coordinator?.send("settings-command",value: name.text ?? "",id: "share-new-name",scope: scope)
        }
        updateSubmit()
    }

    private func updateSubmit() {
        let hasEmail = !(email.text ?? "").trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
        submit.isHidden = primary == nil || usesNavigationSubmit
        submit.isEnabled = !busy && primary?.enabled == true && (primary?.id != "share-invite" || hasEmail)
        createItem.isEnabled = submit.isEnabled
    }

    @objc private func submitInvitation() {
        guard let primary,let scope,submit.isEnabled else { return }
        view.endEditing(true)
        send(primary,value: primary.id == "share-invite" ? email.text ?? "" : "",scope: scope)
    }

    @objc private func secondaryAction() {
        if let secondary,let scope { send(secondary,scope: scope) }
    }

    func textFieldShouldReturn(_ textField: UITextField) -> Bool {
        if textField === name, hasRecipient { email.becomeFirstResponder() }
        else { submitInvitation() }
        return false
    }
}

@MainActor
final class FeiSettingsController: UITableViewController, UISearchResultsUpdating, UISearchControllerDelegate {
    private weak var coordinator: FeiNavigationCoordinator?
    private var model:FeiSettingsState?
    private var busy = false
    private var sections:[FeiSettingsState.Section] = []
    private let search = UISearchController(searchResultsController: nil)
    private let notice = UILabel()
    private var pendingDestination:UIViewController?
    private var renderedMessage:String?

    init(coordinator:FeiNavigationCoordinator) { self.coordinator = coordinator; super.init(style: .insetGrouped) }
    required init?(coder:NSCoder) { fatalError("init(coder:) is unsupported") }
    override func viewDidLoad() {
        super.viewDidLoad()
        tableView.accessibilityIdentifier = "fei.settings.list"
        tableView.rowHeight = UITableView.automaticDimension; tableView.estimatedRowHeight = 64
        tableView.keyboardDismissMode = .interactive
        notice.font = .preferredFont(forTextStyle: .footnote); notice.adjustsFontForContentSizeCategory = true
        notice.textColor = .secondaryLabel; notice.numberOfLines = 0; notice.textAlignment = .center
        notice.accessibilityIdentifier = "fei.settings.message"
        search.obscuresBackgroundDuringPresentation = false; search.searchResultsUpdater = self; search.delegate = self
        search.searchBar.placeholder = coordinator?.localized("搜索快捷键","Search shortcuts")
        search.searchBar.searchTextField.accessibilityIdentifier = "fei.settings.search"
        search.hidesNavigationBarDuringPresentation = false
        definesPresentationContext = true
    }
    private var palette: FeiMobileTheme?
    func applyTheme(_ theme: FeiMobileTheme) {
        guard palette != theme else { return }
        palette = theme
        view.tintColor = theme.accentColor; tableView.tintColor = theme.accentColor
        refreshRows()
    }

    func update(_ next:FeiSettingsState,busy:Bool,theme:FeiMobileTheme) {
        loadViewIfNeeded()
        applyTheme(theme)
        guard next != model || self.busy != busy else { return }
        let wasShortcuts = model?.page == "Shortcuts"
        model = next; self.busy = busy
        if next.page == "Shortcuts" && !wasShortcuts {
            // The child content controller shares its route's navigation item.
            parent?.navigationItem.searchController = search
            parent?.navigationItem.hidesSearchBarWhenScrolling = false
        } else if next.page != "Shortcuts" && wasShortcuts { parent?.navigationItem.searchController = nil }
        refreshRows()
        if renderedMessage != next.message {
            renderedMessage = next.message; notice.text = next.message
            if let message = next.message { UIAccessibility.post(notification: .announcement,argument: message) }
            updateFooter()
        }
        if let picker = navigationController?.topViewController as? FeiSettingsChoiceController { picker.update(next,busy: busy) }
    }
    private func updateFooter() {
        guard notice.text != nil else { if tableView.tableFooterView != nil { tableView.tableFooterView = nil }; return }
        let width = tableView.bounds.width
        let textWidth = max(1,width-40)
        let textHeight = ceil(notice.sizeThatFits(CGSize(width: textWidth,height: .greatestFiniteMagnitude)).height)
        let frame = CGRect(x: 0,y: 0,width: width,height: textHeight+32)
        if let footer = tableView.tableFooterView,footer.frame.size == frame.size { return }
        let container = tableView.tableFooterView ?? UIView()
        container.frame = frame
        notice.frame = CGRect(x: 20,y: 8,width: textWidth,height: textHeight)
        if notice.superview !== container { container.addSubview(notice) }
        tableView.tableFooterView = container
    }
    override func viewDidLayoutSubviews() { super.viewDidLayoutSubviews(); if notice.text != nil { updateFooter() } }
    func deactivate() { view.endEditing(true); view.isHidden = true }
    func updateSearchResults(for searchController:UISearchController) { refreshRows() }
    private func refreshRows() {
        guard let model else { return }
        let query = search.searchBar.text?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
        sections = model.sections.map { section in
            FeiSettingsState.Section(title: section.title,rows: section.rows.filter { model.page != "Shortcuts" || query.isEmpty || $0.title.localizedCaseInsensitiveContains(query) || $0.value.localizedCaseInsensitiveContains(query) || $0.id.localizedCaseInsensitiveContains(query) || prettyShortcut($0.value).localizedCaseInsensitiveContains(query) },footer: section.footer)
        }.filter { !$0.rows.isEmpty }
        tableView.reloadData()
    }
    override func numberOfSections(in tableView:UITableView) -> Int { sections.count }
    override func tableView(_ tableView:UITableView,numberOfRowsInSection section:Int) -> Int { sections[section].rows.count }
    override func tableView(_ tableView:UITableView,titleForHeaderInSection section:Int) -> String? { sections[section].title.isEmpty ? nil : sections[section].title }
    override func tableView(_ tableView:UITableView,titleForFooterInSection section:Int) -> String? { sections[section].footer }
    override func tableView(_ tableView:UITableView,heightForFooterInSection section:Int) -> CGFloat {
        sections[section].footer == nil ? tableView.sectionFooterHeight : UITableView.automaticDimension
    }
    override func tableView(_ tableView:UITableView,viewForFooterInSection section:Int) -> UIView? {
        guard let text = sections[section].footer else { return nil }
        let footer = UITableViewHeaderFooterView(reuseIdentifier: nil)
        var content = UIListContentConfiguration.groupedFooter()
        content.text = text
        content.textProperties.font = font(.footnote)
        content.textProperties.color = .secondaryLabel
        content.textProperties.numberOfLines = 0
        content.textProperties.adjustsFontForContentSizeCategory = true
        footer.contentConfiguration = content
        footer.isAccessibilityElement = true
        footer.accessibilityLabel = text
        footer.accessibilityTraits = .staticText
        return footer
    }
    override func tableView(_ tableView:UITableView,heightForRowAt indexPath:IndexPath) -> CGFloat { UITableView.automaticDimension }
    override func tableView(_ tableView:UITableView,cellForRowAt indexPath:IndexPath) -> UITableViewCell {
        let row = sections[indexPath.section].rows[indexPath.row]
        if let preview = row.preview { return themePreviewCell(preview) }
        let cell = UITableViewCell(style: .subtitle,reuseIdentifier: nil)
        let value = row.kind == "choice" ? row.selectedTitle : row.kind == "shortcut" ? prettyShortcut(row.value) : row.kind == "toggle" ? "" : row.value
        let hasValueAndDetail = !value.isEmpty && !row.detail.isEmpty
        var content = value.isEmpty || hasValueAndDetail ? cell.defaultContentConfiguration() : UIListContentConfiguration.valueCell()
        content.text = row.title
        if row.id.hasPrefix("page:") {
            let symbols = ["General":"gearshape","Appearance":"paintpalette","Library":"folder","Shortcuts":"keyboard","Cloud":"icloud","CloudAccess":"star.circle","Trash":"trash","Shared":"person.2","About":"info.circle"]
            content.image = UIImage(systemName: symbols[String(row.id.dropFirst(5))] ?? "gearshape")
            content.imageProperties.tintColor = palette?.accentColor ?? view.tintColor
        }
        content.textProperties.font = font(row.kind == "share-target" ? .headline : .body)
        content.textProperties.numberOfLines = 0; content.textProperties.adjustsFontForContentSizeCategory = true
        content.secondaryTextProperties.font = font(.subheadline)
        content.secondaryTextProperties.numberOfLines = 0; content.secondaryTextProperties.adjustsFontForContentSizeCategory = true
        content.directionalLayoutMargins = NSDirectionalEdgeInsets(top: 13,leading: 16,bottom: 13,trailing: 16)
        let secondary = value.isEmpty || hasValueAndDetail ? row.detail : value
        content.secondaryText = secondary.isEmpty ? nil : secondary
        let enabled = row.enabled && !busy
        let informational = ["info","share-target","brand","toggle"].contains(row.kind)
        if row.kind == "info" || !enabled { content.textProperties.color = .secondaryLabel }
        else if row.danger || row.kind == "destructive-page" { content.textProperties.color = .systemRed }
        else if row.kind == "share-email" { content.textProperties.color = palette?.accentColor ?? view.tintColor }
        else { content.textProperties.color = .label }
        content.secondaryTextProperties.color = .secondaryLabel
        if row.kind == "brand" {
            content.textProperties.font = font(.title2)
            let icons = Bundle.main.infoDictionary?["CFBundleIcons"] as? [String:Any]
            let primary = icons?["CFBundlePrimaryIcon"] as? [String:Any]
            if let file = (primary?["CFBundleIconFiles"] as? [String])?.last { content.image = UIImage(named: file) }
            content.imageProperties.maximumSize = CGSize(width: 64,height: 64); content.imageProperties.cornerRadius = 14
        }
        if hasValueAndDetail {
            installSettingValue(value,content: content,in: cell)
        } else { cell.contentConfiguration = content }
        cell.accessibilityIdentifier = "fei.setting."+row.id
        cell.selectionStyle = enabled && !informational ? .default : .none
        if row.kind == "toggle" {
            let toggle = UISwitch(); toggle.isOn = row.value == "true"; toggle.isEnabled = enabled
            toggle.onTintColor = palette?.accentColor ?? view.tintColor
            toggle.accessibilityLabel = row.title; toggle.accessibilityIdentifier = "fei.setting.switch."+row.id
            let scope = model?.scope ?? ""
            toggle.addAction(UIAction { [weak self,weak toggle] _ in self?.command(row,value: toggle?.isOn == true ? "true" : "false",scope: scope) },for: .valueChanged)
            cell.accessoryView = toggle
        } else if ["page","choice","shortcut","input","link","destructive-page","share-email"].contains(row.kind) { cell.accessoryType = .disclosureIndicator }
        if !enabled {
            cell.isUserInteractionEnabled = false
            cell.accessibilityTraits.insert(.notEnabled)
        }
        return cell
    }
    private func installSettingValue(_ value:String,content:UIListContentConfiguration,in cell:UITableViewCell) {
        var textContent = content
        textContent.directionalLayoutMargins.trailing = 8
        let labels = UIListContentView(configuration: textContent)
        guard let title = labels.textLayoutGuide else {
            cell.contentConfiguration = content
            return
        }
        let valueLabel = UILabel()
        valueLabel.text = value
        valueLabel.font = content.secondaryTextProperties.font
        valueLabel.textColor = content.secondaryTextProperties.color
        valueLabel.adjustsFontForContentSizeCategory = true
        valueLabel.lineBreakMode = .byTruncatingMiddle
        valueLabel.setContentHuggingPriority(.required,for: .horizontal)
        valueLabel.setContentCompressionResistancePriority(UILayoutPriority(999),for: .horizontal)
        let rowContent = UIView()
        cell.installContent(rowContent)
        labels.translatesAutoresizingMaskIntoConstraints = false
        valueLabel.translatesAutoresizingMaskIntoConstraints = false
        rowContent.addSubview(labels)
        rowContent.addSubview(valueLabel)
        NSLayoutConstraint.activate([
            labels.topAnchor.constraint(equalTo: rowContent.topAnchor),
            labels.leadingAnchor.constraint(equalTo: rowContent.leadingAnchor),
            labels.bottomAnchor.constraint(equalTo: rowContent.bottomAnchor),
            labels.trailingAnchor.constraint(equalTo: valueLabel.leadingAnchor),
            valueLabel.trailingAnchor.constraint(equalTo: rowContent.trailingAnchor,constant: -content.directionalLayoutMargins.trailing),
            valueLabel.centerYAnchor.constraint(equalTo: title.centerYAnchor),
            valueLabel.widthAnchor.constraint(lessThanOrEqualTo: rowContent.widthAnchor,multiplier: 0.45),
        ])
    }
    private func themePreviewCell(_ preview:FeiSettingsState.Preview) -> UITableViewCell {
        let cell = UITableViewCell(); cell.selectionStyle = .none; cell.accessibilityIdentifier = "fei.setting.theme-preview"
        cell.installContent(FeiThemePreviewView(preview))
        return cell
    }
    private func font(_ style:UIFont.TextStyle) -> UIFont {
        model?.font(style) ?? .preferredFont(forTextStyle: style)
    }
    override func tableView(_ tableView:UITableView,didSelectRowAt indexPath:IndexPath) {
        tableView.deselectRow(at: indexPath,animated: true)
        guard let model else { return }
        let row = sections[indexPath.section].rows[indexPath.row]
        guard row.enabled,!busy else { return }
        if row.kind == "choice" {
            let picker = FeiSettingsChoiceController(row: row,state: model,coordinator: coordinator)
            navigationController?.pushViewController(picker,animated: !UIAccessibility.isReduceMotionEnabled)
        } else if row.kind == "shortcut" { showShortcut(row,scope: model.scope) }
        else if row.kind == "input" || row.kind == "share-email" { showInput(row,scope: model.scope) }
        else if !["info","share-target","brand","toggle"].contains(row.kind) { command(row,value: "",scope: model.scope) }
    }
    private func command(_ row:FeiSettingsState.Row,value:String,scope:String) {
        let send = { [weak self] in self?.coordinator?.send("settings-command",value: value,id: row.id,scope: scope) }
        guard row.danger, row.kind != "destructive-confirm" else { send(); return }
        let alert = UIAlertController(title: row.title,message: nil,preferredStyle: .alert)
        alert.addAction(UIAlertAction(title: coordinator?.localized("取消","Cancel"),style: .cancel))
        alert.addAction(UIAlertAction(title: row.title,style: .destructive) { _ in send() })
        present(alert,animated: true)
    }
    private func showInput(_ row:FeiSettingsState.Row,scope:String) {
        let isInvitation = row.kind == "share-email"
        let title = isInvitation ? coordinator?.localized("创建邀请","Create invitation") : row.title
        let alert = UIAlertController(title: title,message: isInvitation ? nil : row.detail,preferredStyle: .alert)
        let submit = UIAlertAction(title: isInvitation ? title : coordinator?.localized("继续","Continue"),style: .default) { [weak self,weak alert] _ in
            self?.coordinator?.send("settings-command",value: alert?.textFields?.first?.text ?? "",id: row.id,scope: scope)
        }
        submit.isEnabled = !isInvitation || !row.value.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
        alert.addTextField { field in
            field.placeholder = row.detail; field.autocorrectionType = .no; field.autocapitalizationType = .none
            field.text = row.value
            field.keyboardType = row.id == "share-accept" ? .URL : row.id == "share-new-name" ? .default : .emailAddress
            field.accessibilityIdentifier = "fei.settings.input"
            if isInvitation {
                field.textContentType = .emailAddress
                field.addAction(UIAction { [weak field,weak submit] _ in
                    submit?.isEnabled = !(field?.text ?? "").trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
                },for: .editingChanged)
            }
        }
        alert.addAction(UIAlertAction(title: coordinator?.localized("取消","Cancel"),style: .cancel))
        alert.addAction(submit)
        present(alert,animated: true)
    }
    func didDismissSearchController(_ searchController:UISearchController) {
        guard let destination = pendingDestination else { return }; pendingDestination = nil
        navigationController?.pushViewController(destination,animated: !UIAccessibility.isReduceMotionEnabled)
    }
    private func showShortcut(_ row:FeiSettingsState.Row,scope:String) {
        let recorder = FeiShortcutRecorder(row: row,scope: scope,coordinator: coordinator)
        view.endEditing(true)
        if search.isActive { pendingDestination = recorder; search.isActive = false }
        else { navigationController?.pushViewController(recorder,animated: !UIAccessibility.isReduceMotionEnabled) }
    }
}

@MainActor
final class FeiSettingsChoiceController: UITableViewController {
    private var row:FeiSettingsState.Row
    private let scope:String
    private weak var coordinator:FeiNavigationCoordinator?
    private var state:FeiSettingsState
    private var comparesThemes:Bool { row.id == "theme" }
    init(row:FeiSettingsState.Row,state:FeiSettingsState,coordinator:FeiNavigationCoordinator?) {
        self.row = row; self.state = state; self.scope = state.scope; self.coordinator = coordinator
        super.init(style: .insetGrouped); title = row.title
        navigationItem.backButtonDisplayMode = .minimal
    }
    required init?(coder:NSCoder) { fatalError("init(coder:) is unsupported") }
    override func viewDidLoad() { super.viewDidLoad(); tableView.accessibilityIdentifier = "fei.settings.choices"; tableView.rowHeight = UITableView.automaticDimension; tableView.estimatedRowHeight = 52 }
    func update(_ state:FeiSettingsState,busy:Bool) {
        guard scope == state.scope,let next = state.sections.flatMap(\.rows).first(where: { $0.id == row.id }) else { view.isUserInteractionEnabled = false; return }
        let enabled = !busy && next.enabled
        guard row != next || state.interface_size != self.state.interface_size || view.isUserInteractionEnabled != enabled else { return }
        row = next; self.state = state; title = next.title; view.isUserInteractionEnabled = enabled; tableView.reloadData()
    }
    private var memberChoices: Bool { row.id.hasPrefix("member:") && row.choices.last?.value == "revoke" }
    override func numberOfSections(in tableView:UITableView) -> Int { comparesThemes ? row.choices.count : memberChoices ? 2 : 1 }
    override func tableView(_ tableView:UITableView,numberOfRowsInSection section:Int) -> Int {
        if comparesThemes || memberChoices && section == 1 { return 1 }
        return row.choices.count - (memberChoices ? 1 : 0)
    }
    override func tableView(_ tableView:UITableView,titleForHeaderInSection section:Int) -> String? {
        memberChoices && section == 0 ? coordinator?.localized("权限","Permission") : nil
    }
    private func choiceIndex(_ indexPath:IndexPath) -> Int {
        if comparesThemes { return indexPath.section }
        return memberChoices && indexPath.section == 1 ? row.choices.count - 1 : indexPath.row
    }
    override func tableView(_ tableView:UITableView,cellForRowAt indexPath:IndexPath) -> UITableViewCell {
        let item = row.choices[choiceIndex(indexPath)]; let cell = UITableViewCell()
        let selected = row.value == item.value
        cell.accessibilityIdentifier = "fei.setting.option."+row.id+"."+item.value
        if let preview = item.preview {
            let name = UILabel()
            name.text = item.title; name.font = state.font(.headline); name.adjustsFontForContentSizeCategory = true; name.numberOfLines = 0
            let header = UIStackView(arrangedSubviews: [name])
            header.alignment = .center; header.spacing = 8
            header.isLayoutMarginsRelativeArrangement = true
            header.directionalLayoutMargins = NSDirectionalEdgeInsets(top: 11,leading: 16,bottom: 11,trailing: 16)
            header.heightAnchor.constraint(greaterThanOrEqualToConstant: 44).isActive = true
            if selected {
                let check = UIImageView(image: UIImage(systemName: "checkmark",withConfiguration: UIImage.SymbolConfiguration(textStyle: .body,scale: .medium)))
                check.tintColor = .label; check.adjustsImageSizeForAccessibilityContentSizeCategory = true
                check.setContentHuggingPriority(.required,for: .horizontal)
                check.setContentCompressionResistancePriority(.required,for: .horizontal)
                header.addArrangedSubview(check)
            }
            let divider = UIView()
            divider.backgroundColor = .separator
            divider.heightAnchor.constraint(equalToConstant: 1/max(1,tableView.traitCollection.displayScale)).isActive = true
            let sample = FeiThemePreviewView(preview)
            sample.accessibilityElementsHidden = true
            let stack = UIStackView(arrangedSubviews: [header,divider,sample])
            stack.axis = .vertical
            cell.installContent(stack)
            cell.isAccessibilityElement = true
            cell.accessibilityLabel = item.title
            cell.accessibilityTraits.insert(.button)
            if selected { cell.accessibilityTraits.insert(.selected) }
            return cell
        }
        var content = cell.defaultContentConfiguration(); content.text = item.title
        content.textProperties.font = .preferredFont(forTextStyle: .body); content.textProperties.numberOfLines = 0; content.textProperties.adjustsFontForContentSizeCategory = true
        content.directionalLayoutMargins = NSDirectionalEdgeInsets(top: 14,leading: 16,bottom: 14,trailing: 16)
        if item.value == "revoke" { content.textProperties.color = .systemRed }
        cell.contentConfiguration = content; cell.accessoryType = selected ? .checkmark : .none
        if selected { cell.accessibilityTraits.insert(.selected) }
        return cell
    }
    override func tableView(_ tableView:UITableView,didSelectRowAt indexPath:IndexPath) {
        let item = row.choices[choiceIndex(indexPath)]
        let value = item.value
        tableView.deselectRow(at: indexPath,animated: true)
        if memberChoices && value == row.value {
            navigationController?.popViewController(animated: !UIAccessibility.isReduceMotionEnabled)
            return
        }
        let send = { [weak self] in guard let self else { return }; self.coordinator?.send("settings-command",value: value,id: self.row.id,scope: self.scope); self.navigationController?.popViewController(animated: !UIAccessibility.isReduceMotionEnabled) }
        if value == "revoke" {
            let alert = UIAlertController(title: item.title,message: row.title,preferredStyle: .alert)
            alert.addAction(UIAlertAction(title: coordinator?.localized("取消","Cancel"),style: .cancel))
            alert.addAction(UIAlertAction(title: item.title,style: .destructive) { _ in send() }); present(alert,animated: true)
        } else { send() }
    }
}

private func prettyShortcut(_ value:String) -> String {
    value.replacingOccurrences(of: "cmd-",with: "⌘").replacingOccurrences(of: "ctrl-",with: "⌃").replacingOccurrences(of: "alt-",with: "⌥").replacingOccurrences(of: "shift-",with: "⇧").uppercased()
}

@MainActor
final class FeiShortcutRecorder: UIViewController {
    private let row:FeiSettingsState.Row; private let scope:String
    private weak var coordinator:FeiNavigationCoordinator?
    private var strokes:[String] = []; private let label = UILabel()
    override var canBecomeFirstResponder:Bool { true }
    init(row:FeiSettingsState.Row,scope:String,coordinator:FeiNavigationCoordinator?) { self.row = row; self.scope = scope; self.coordinator = coordinator; super.init(nibName: nil,bundle: nil); title = row.title }
    required init?(coder:NSCoder) { fatalError("init(coder:) is unsupported") }
    override func viewDidLoad() {
        super.viewDidLoad(); view.backgroundColor = .systemGroupedBackground
        let hint = UILabel(); hint.text = coordinator?.localized("请在外接键盘上按下组合键（最多四组）。保存后，与它冲突的旧绑定会被移除。","Press up to four strokes on a hardware keyboard. Saving removes conflicting bindings."); hint.numberOfLines = 0
        hint.font = .preferredFont(forTextStyle: .body); hint.adjustsFontForContentSizeCategory = true
        label.font = .preferredFont(forTextStyle: .title2); label.adjustsFontForContentSizeCategory = true; label.numberOfLines = 0; label.text = prettyShortcut(row.value)
        label.accessibilityIdentifier = "fei.settings.shortcut.value"
        let stack = UIStackView(arrangedSubviews: [hint,label]); stack.axis = .vertical; stack.spacing = 24
        for (zh,en,value) in [("恢复默认","Reset","@reset"),("移除快捷键","Remove shortcut","@clear")] {
            let button = UIButton(type: .system); button.setTitle(coordinator?.localized(zh,en),for: .normal); button.titleLabel?.font = .preferredFont(forTextStyle: .body)
            button.heightAnchor.constraint(greaterThanOrEqualToConstant: 48).isActive = true
            button.addAction(UIAction { [weak self] _ in self?.save(value) },for: .touchUpInside); stack.addArrangedSubview(button)
        }
        stack.translatesAutoresizingMaskIntoConstraints = false; view.addSubview(stack)
        NSLayoutConstraint.activate([stack.leadingAnchor.constraint(equalTo: view.safeAreaLayoutGuide.leadingAnchor,constant: 20),stack.trailingAnchor.constraint(equalTo: view.safeAreaLayoutGuide.trailingAnchor,constant: -20),stack.topAnchor.constraint(equalTo: view.safeAreaLayoutGuide.topAnchor,constant: 24)])
        let save = UIBarButtonItem(title: coordinator?.localized("保存","Save"),style: .done,target: self,action: #selector(confirm)); save.isEnabled = false; navigationItem.rightBarButtonItem = save
    }
    override func viewDidAppear(_ animated:Bool) { super.viewDidAppear(animated); becomeFirstResponder() }
    override func pressesBegan(_ presses:Set<UIPress>,with event:UIPressesEvent?) {
        guard let key = presses.first?.key else { super.pressesBegan(presses,with: event); return }
        let modifiers = key.modifierFlags
        if key.keyCode == .keyboardDeleteOrBackspace { if !strokes.isEmpty { strokes.removeLast() } }
        else if key.keyCode == .keyboardEscape { navigationController?.popViewController(animated: true); return }
        else if key.keyCode == .keyboardReturnOrEnter && modifiers.isEmpty { if !strokes.isEmpty { confirm() }; return }
        else {
            guard strokes.count < 4,!key.charactersIgnoringModifiers.isEmpty,modifiers.contains(.command) || modifiers.contains(.control) || modifiers.contains(.alternate) || !strokes.isEmpty else { return }
            let prefix = [(UIKeyModifierFlags.command,"cmd-"),(.control,"ctrl-"),(.alternate,"alt-"),(.shift,"shift-")].filter { modifiers.contains($0.0) }.map(\.1).joined()
            let name = key.charactersIgnoringModifiers.lowercased()
            strokes.append(prefix+name)
        }
        label.text = prettyShortcut(strokes.joined(separator: " ")); navigationItem.rightBarButtonItem?.isEnabled = !strokes.isEmpty
    }
    @objc private func confirm() { save(strokes.joined(separator: " ")) }
    private func save(_ value:String) { coordinator?.send("settings-command",value: value,id: row.id,scope: scope); navigationController?.popViewController(animated: true) }
}
