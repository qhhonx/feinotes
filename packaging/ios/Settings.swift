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

@MainActor
final class FeiShareAcceptController: UIViewController {
    private weak var coordinator: FeiNavigationCoordinator?
    private let link = UITextField()
    private let submit = UIButton(type: .system)
    private let notice = UILabel()
    private var scope: String?
    private var busy = false
    private var projectedLink: String?
    private var canAccept = false

    init(coordinator: FeiNavigationCoordinator) {
        self.coordinator = coordinator
        super.init(nibName: nil,bundle: nil)
    }
    required init?(coder: NSCoder) { fatalError("init(coder:) is unsupported") }

    override func viewDidLoad() {
        super.viewDidLoad()
        view.backgroundColor = .systemGroupedBackground
        link.borderStyle = .roundedRect
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
        submit.configuration = .filled()
        submit.accessibilityIdentifier = "fei.share.accept"
        submit.addTarget(self,action: #selector(accept),for: .touchUpInside)
        notice.numberOfLines = 0
        notice.font = .preferredFont(forTextStyle: .footnote)
        notice.adjustsFontForContentSizeCategory = true
        notice.accessibilityIdentifier = "fei.share.invitation-status"

        let scroll = UIScrollView()
        scroll.keyboardDismissMode = .interactive
        let form = UIStackView(arrangedSubviews: [link,submit,notice])
        form.axis = .vertical
        form.spacing = 12
        scroll.translatesAutoresizingMaskIntoConstraints = false
        form.translatesAutoresizingMaskIntoConstraints = false
        view.addSubview(scroll)
        scroll.addSubview(form)
        let safe = view.safeAreaLayoutGuide
        NSLayoutConstraint.activate([
            scroll.leadingAnchor.constraint(equalTo: safe.leadingAnchor),
            scroll.trailingAnchor.constraint(equalTo: safe.trailingAnchor),
            scroll.topAnchor.constraint(equalTo: safe.topAnchor),
            scroll.bottomAnchor.constraint(equalTo: view.keyboardLayoutGuide.topAnchor),
            form.topAnchor.constraint(equalTo: scroll.contentLayoutGuide.topAnchor,constant: 20),
            form.bottomAnchor.constraint(equalTo: scroll.contentLayoutGuide.bottomAnchor,constant: -20),
            form.leadingAnchor.constraint(equalTo: scroll.frameLayoutGuide.leadingAnchor,constant: 20),
            form.trailingAnchor.constraint(equalTo: scroll.frameLayoutGuide.trailingAnchor,constant: -20),
            form.leadingAnchor.constraint(equalTo: scroll.contentLayoutGuide.leadingAnchor,constant: 20),
            form.trailingAnchor.constraint(equalTo: scroll.contentLayoutGuide.trailingAnchor,constant: -20),
            link.heightAnchor.constraint(greaterThanOrEqualToConstant: 44),
            submit.heightAnchor.constraint(greaterThanOrEqualToConstant: 44),
        ])
    }

    func update(_ state: FeiSettingsState,busy: Bool,theme: FeiMobileTheme) {
        loadViewIfNeeded()
        let rows = state.sections.flatMap { $0.rows }
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
        submit.configuration?.title = field.title
        submit.configuration?.baseBackgroundColor = theme.accentColor
        notice.text = rows.first(where: { $0.id == "share-message" || $0.id == "share-busy" || $0.id == "share-unavailable" })?.title
        notice.textColor = busy || !canAccept ? .secondaryLabel : .systemRed
        notice.isHidden = notice.text?.isEmpty != false
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
        var content = value.isEmpty ? cell.defaultContentConfiguration() : UIListContentConfiguration.valueCell()
        content.text = row.title
        if row.id.hasPrefix("page:") {
            let symbols = ["General":"gearshape","Appearance":"paintpalette","Library":"folder","Shortcuts":"keyboard","Cloud":"icloud","Trash":"trash","Shared":"person.2","About":"info.circle"]
            content.image = UIImage(systemName: symbols[String(row.id.dropFirst(5))] ?? "gearshape")
            content.imageProperties.tintColor = palette?.accentColor ?? view.tintColor
        }
        content.textProperties.font = font(.body)
        content.textProperties.numberOfLines = 0; content.textProperties.adjustsFontForContentSizeCategory = true
        content.secondaryTextProperties.font = font(.subheadline)
        content.secondaryTextProperties.numberOfLines = 0; content.secondaryTextProperties.adjustsFontForContentSizeCategory = true
        content.directionalLayoutMargins = NSDirectionalEdgeInsets(top: 13,leading: 16,bottom: 13,trailing: 16)
        let secondary = value.isEmpty ? row.detail : value
        content.secondaryText = secondary.isEmpty ? nil : secondary
        let enabled = row.enabled && !busy
        content.textProperties.color = row.kind == "info" || !enabled ? .secondaryLabel : row.danger ? .systemRed : .label
        content.secondaryTextProperties.color = .secondaryLabel
        if row.kind == "brand" {
            content.textProperties.font = font(.title2)
            let icons = Bundle.main.infoDictionary?["CFBundleIcons"] as? [String:Any]
            let primary = icons?["CFBundlePrimaryIcon"] as? [String:Any]
            if let file = (primary?["CFBundleIconFiles"] as? [String])?.last { content.image = UIImage(named: file) }
            content.imageProperties.maximumSize = CGSize(width: 64,height: 64); content.imageProperties.cornerRadius = 14
        }
        if !value.isEmpty && !row.detail.isEmpty {
            let margins = content.directionalLayoutMargins
            content.directionalLayoutMargins = .zero
            content.axesPreservingSuperviewLayoutMargins = []
            let detail = UILabel()
            detail.text = row.detail
            detail.font = font(.subheadline)
            detail.textColor = .secondaryLabel
            detail.numberOfLines = 0
            detail.adjustsFontForContentSizeCategory = true
            let stack = UIStackView(arrangedSubviews: [UIListContentView(configuration: content),detail])
            stack.axis = .vertical; stack.spacing = 3
            stack.isLayoutMarginsRelativeArrangement = true
            stack.directionalLayoutMargins = margins
            cell.installContent(stack)
        } else { cell.contentConfiguration = content }
        cell.accessibilityIdentifier = "fei.setting."+row.id
        cell.selectionStyle = enabled && !["info","brand","toggle"].contains(row.kind) ? .default : .none
        if row.kind == "toggle" {
            let toggle = UISwitch(); toggle.isOn = row.value == "true"; toggle.isEnabled = enabled
            toggle.accessibilityLabel = row.title; toggle.accessibilityIdentifier = "fei.setting.switch."+row.id
            let scope = model?.scope ?? ""
            toggle.addAction(UIAction { [weak self,weak toggle] _ in self?.command(row,value: toggle?.isOn == true ? "true" : "false",scope: scope) },for: .valueChanged)
            cell.accessoryView = toggle
        } else if ["page","choice","shortcut","input","link"].contains(row.kind) { cell.accessoryType = .disclosureIndicator }
        if !enabled {
            cell.isUserInteractionEnabled = false
            cell.accessibilityTraits.insert(.notEnabled)
        }
        return cell
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
        else if row.kind == "input" { showInput(row,scope: model.scope) }
        else if !["info","brand","toggle"].contains(row.kind) { command(row,value: "",scope: model.scope) }
    }
    private func command(_ row:FeiSettingsState.Row,value:String,scope:String) {
        let send = { [weak self] in self?.coordinator?.send("settings-command",value: value,id: row.id,scope: scope) }
        guard row.danger else { send(); return }
        let alert = UIAlertController(title: row.title,message: nil,preferredStyle: .alert)
        alert.addAction(UIAlertAction(title: coordinator?.localized("取消","Cancel"),style: .cancel))
        alert.addAction(UIAlertAction(title: row.title,style: .destructive) { _ in send() })
        present(alert,animated: true)
    }
    private func showInput(_ row:FeiSettingsState.Row,scope:String) {
        let alert = UIAlertController(title: row.title,message: row.detail,preferredStyle: .alert)
        alert.addTextField { field in
            field.placeholder = row.detail; field.autocorrectionType = .no; field.autocapitalizationType = .none
            field.text = row.value
            field.keyboardType = row.id == "share-accept" ? .URL : row.id == "share-new-name" ? .default : .emailAddress
            field.accessibilityIdentifier = "fei.settings.input"
        }
        alert.addAction(UIAlertAction(title: coordinator?.localized("取消","Cancel"),style: .cancel))
        alert.addAction(UIAlertAction(title: coordinator?.localized("继续","Continue"),style: .default) { [weak self,weak alert] _ in
            self?.coordinator?.send("settings-command",value: alert?.textFields?.first?.text ?? "",id: row.id,scope: scope)
        })
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
    override func numberOfSections(in tableView:UITableView) -> Int { comparesThemes ? row.choices.count : 1 }
    override func tableView(_ tableView:UITableView,numberOfRowsInSection section:Int) -> Int { comparesThemes ? 1 : row.choices.count }
    private func choiceIndex(_ indexPath:IndexPath) -> Int { comparesThemes ? indexPath.section : indexPath.row }
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
        return cell
    }
    override func tableView(_ tableView:UITableView,didSelectRowAt indexPath:IndexPath) {
        let item = row.choices[choiceIndex(indexPath)]
        let value = item.value
        let send = { [weak self] in guard let self else { return }; self.coordinator?.send("settings-command",value: value,id: self.row.id,scope: self.scope); self.navigationController?.popViewController(animated: !UIAccessibility.isReduceMotionEnabled) }
        if value == "revoke" {
            let alert = UIAlertController(title: item.title,message: row.title,preferredStyle: .alert)
            alert.addAction(UIAlertAction(title: coordinator?.localized("取消","Cancel"),style: .cancel))
            alert.addAction(UIAlertAction(title: row.choices[indexPath.row].title,style: .destructive) { _ in send() }); present(alert,animated: true)
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
