import UIKit

struct FeiSettingsState: Decodable, Equatable, Sendable {
    struct Choice: Decodable, Equatable, Sendable { let value:String; let title:String }
    struct Row: Decodable, Equatable, Sendable {
        let id:String; let title:String; let detail:String; let kind:String; let value:String
        let preview:Preview?
        let choices:[Choice]; let enabled:Bool; let danger:Bool
        var selectedTitle:String { choices.first(where: { $0.value == value })?.title ?? value }
    }
    struct Preview: Decodable, Equatable, Sendable { let font:String; let mono:String; let size:CGFloat; let heading_size:CGFloat; let line:CGFloat; let text:[CGFloat]; let heading:[CGFloat]; let background:[CGFloat]; let link:[CGFloat]; let code_background:[CGFloat]; let title:String; let body:String; let list:String; let quote:String }
    struct Section: Decodable, Equatable, Sendable { let title:String; let rows:[Row] }
    let scope:String; let page:String; let sections:[Section]; let message:String?; let interface_size:String
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
    func update(_ next:FeiSettingsState,busy:Bool) {
        loadViewIfNeeded()
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
        guard notice.text != nil else { tableView.tableFooterView = nil; return }
        let width = tableView.bounds.width
        let size = notice.sizeThatFits(CGSize(width: max(1,width-40),height: .greatestFiniteMagnitude))
        let container = UIView(frame: CGRect(x: 0,y: 0,width: width,height: size.height+32))
        notice.frame = CGRect(x: 20,y: 8,width: max(1,width-40),height: size.height); container.addSubview(notice)
        tableView.tableFooterView = container
    }
    override func viewDidLayoutSubviews() { super.viewDidLayoutSubviews(); if notice.text != nil { updateFooter() } }
    func deactivate() { view.endEditing(true); view.isHidden = true }
    func updateSearchResults(for searchController:UISearchController) { refreshRows() }
    private func refreshRows() {
        guard let model else { return }
        let query = search.searchBar.text?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
        sections = model.sections.map { section in
            FeiSettingsState.Section(title: section.title,rows: section.rows.filter { model.page != "Shortcuts" || query.isEmpty || $0.title.localizedCaseInsensitiveContains(query) || $0.value.localizedCaseInsensitiveContains(query) || $0.id.localizedCaseInsensitiveContains(query) || prettyShortcut($0.value).localizedCaseInsensitiveContains(query) })
        }.filter { !$0.rows.isEmpty }
        tableView.reloadData()
    }
    override func numberOfSections(in tableView:UITableView) -> Int { sections.count }
    override func tableView(_ tableView:UITableView,numberOfRowsInSection section:Int) -> Int { sections[section].rows.count }
    override func tableView(_ tableView:UITableView,titleForHeaderInSection section:Int) -> String? { sections[section].title.isEmpty ? nil : sections[section].title }
    override func tableView(_ tableView:UITableView,heightForRowAt indexPath:IndexPath) -> CGFloat { UITableView.automaticDimension }
    override func tableView(_ tableView:UITableView,cellForRowAt indexPath:IndexPath) -> UITableViewCell {
        let row = sections[indexPath.section].rows[indexPath.row]
        if let preview = row.preview { return themePreviewCell(preview) }
        let cell = UITableViewCell(style: .subtitle,reuseIdentifier: nil)
        var content = cell.defaultContentConfiguration()
        content.text = row.title
        if row.id.hasPrefix("page:") {
            let symbols = ["General":"gearshape","Appearance":"paintpalette","Library":"folder","Shortcuts":"keyboard","Cloud":"icloud","Shared":"person.2","About":"info.circle"]
            content.image = UIImage(systemName: symbols[String(row.id.dropFirst(5))] ?? "gearshape")
            content.imageProperties.tintColor = coordinator?.theme.accentColor ?? view.tintColor
        }
        content.textProperties.font = font(.body)
        content.textProperties.numberOfLines = 0; content.textProperties.adjustsFontForContentSizeCategory = true
        content.secondaryTextProperties.font = font(.subheadline)
        content.secondaryTextProperties.numberOfLines = 0; content.secondaryTextProperties.adjustsFontForContentSizeCategory = true
        content.directionalLayoutMargins = NSDirectionalEdgeInsets(top: 13,leading: 16,bottom: 13,trailing: 16)
        let value = row.kind == "choice" ? row.selectedTitle : row.kind == "shortcut" ? prettyShortcut(row.value) : row.kind == "toggle" ? "" : row.value
        content.secondaryText = [value,row.detail].filter { !$0.isEmpty }.joined(separator: "\n")
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
        cell.contentConfiguration = content
        cell.accessibilityIdentifier = "fei.setting."+row.id
        cell.selectionStyle = enabled && !["info","brand","toggle"].contains(row.kind) ? .default : .none
        if row.kind == "toggle" {
            let toggle = UISwitch(); toggle.isOn = row.value == "true"; toggle.isEnabled = enabled
            toggle.accessibilityLabel = row.title; toggle.accessibilityIdentifier = "fei.setting.switch."+row.id
            let scope = model?.scope ?? ""
            toggle.addAction(UIAction { [weak self,weak toggle] _ in self?.command(row,value: toggle?.isOn == true ? "true" : "false",scope: scope) },for: .valueChanged)
            cell.accessoryView = toggle
        } else if ["page","choice","shortcut","input","link"].contains(row.kind) { cell.accessoryType = .disclosureIndicator }
        if !enabled { cell.isUserInteractionEnabled = false }
        return cell
    }
    private func themePreviewCell(_ preview:FeiSettingsState.Preview) -> UITableViewCell {
        let cell = UITableViewCell(); cell.selectionStyle = .none; cell.accessibilityIdentifier = "fei.setting.theme-preview"
        let text = UITextView(); text.isEditable = false; text.isSelectable = false; text.isScrollEnabled = false
        func color(_ c:[CGFloat]) -> UIColor { guard c.count == 4 else { return .label }; return UIColor(red: c[0],green: c[1],blue: c[2],alpha: c[3]) }
        let font = UIFont(name: preview.font,size: preview.size) ?? .systemFont(ofSize: preview.size)
        let heading = UIFont(name: preview.font,size: preview.heading_size) ?? .boldSystemFont(ofSize: preview.heading_size)
        let paragraph = NSMutableParagraphStyle(); paragraph.minimumLineHeight = preview.line; paragraph.paragraphSpacing = 8
        let body:[NSAttributedString.Key:Any] = [.font:font,.foregroundColor:color(preview.text),.paragraphStyle:paragraph]
        let content = NSMutableAttributedString(string: preview.title+"\n",attributes: [.font:heading,.foregroundColor:color(preview.heading),.paragraphStyle:paragraph])
        content.append(NSAttributedString(string: preview.body+"\n• "+preview.list+"\n❝ "+preview.quote+"\n",attributes: body))
        content.append(NSAttributedString(string: "let ideas = []",attributes: [.font:UIFont(name: preview.mono,size: preview.size) ?? .monospacedSystemFont(ofSize: preview.size,weight: .regular),.foregroundColor:color(preview.text),.backgroundColor:color(preview.code_background)]))
        text.attributedText = content; text.backgroundColor = color(preview.background)
        text.textContainerInset = UIEdgeInsets(top: 20,left: 12,bottom: 20,right: 12)
        text.translatesAutoresizingMaskIntoConstraints = false; cell.contentView.addSubview(text)
        NSLayoutConstraint.activate([text.topAnchor.constraint(equalTo: cell.contentView.topAnchor),text.bottomAnchor.constraint(equalTo: cell.contentView.bottomAnchor),text.leadingAnchor.constraint(equalTo: cell.contentView.leadingAnchor),text.trailingAnchor.constraint(equalTo: cell.contentView.trailingAnchor)])
        return cell
    }
    private func font(_ style:UIFont.TextStyle) -> UIFont {
        let base = UIFont.preferredFont(forTextStyle: style)
        let delta:CGFloat = model?.interface_size == "large" ? 2 : model?.interface_size == "small" ? -1 : 0
        return base.withSize(base.pointSize+delta)
    }
    override func tableView(_ tableView:UITableView,didSelectRowAt indexPath:IndexPath) {
        tableView.deselectRow(at: indexPath,animated: true)
        guard let model else { return }
        let row = sections[indexPath.section].rows[indexPath.row]
        guard row.enabled,!busy else { return }
        if row.kind == "choice" {
            let picker = FeiSettingsChoiceController(row: row,scope: model.scope,coordinator: coordinator)
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
            field.keyboardType = row.id == "share-accept" ? .URL : .emailAddress
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
    init(row:FeiSettingsState.Row,scope:String,coordinator:FeiNavigationCoordinator?) {
        self.row = row; self.scope = scope; self.coordinator = coordinator
        super.init(style: .insetGrouped); title = row.title
        navigationItem.backButtonDisplayMode = .minimal
    }
    required init?(coder:NSCoder) { fatalError("init(coder:) is unsupported") }
    override func viewDidLoad() { super.viewDidLoad(); tableView.accessibilityIdentifier = "fei.settings.choices"; tableView.rowHeight = UITableView.automaticDimension; tableView.estimatedRowHeight = 52 }
    func update(_ state:FeiSettingsState,busy:Bool) {
        guard scope == state.scope,let next = state.sections.flatMap(\.rows).first(where: { $0.id == row.id }) else { view.isUserInteractionEnabled = false; return }
        guard row != next || view.isUserInteractionEnabled == busy else { return }
        row = next; title = next.title; view.isUserInteractionEnabled = !busy && next.enabled; tableView.reloadData()
    }
    override func tableView(_ tableView:UITableView,numberOfRowsInSection section:Int) -> Int { row.choices.count }
    override func tableView(_ tableView:UITableView,cellForRowAt indexPath:IndexPath) -> UITableViewCell {
        let item = row.choices[indexPath.row]; let cell = UITableViewCell()
        var content = cell.defaultContentConfiguration(); content.text = item.title
        content.textProperties.font = .preferredFont(forTextStyle: .body); content.textProperties.numberOfLines = 0; content.textProperties.adjustsFontForContentSizeCategory = true
        content.directionalLayoutMargins = NSDirectionalEdgeInsets(top: 14,leading: 16,bottom: 14,trailing: 16)
        if item.value == "revoke" { content.textProperties.color = .systemRed }
        cell.contentConfiguration = content; cell.accessoryType = row.value == item.value ? .checkmark : .none
        cell.accessibilityIdentifier = "fei.setting.option."+row.id+"."+item.value
        return cell
    }
    override func tableView(_ tableView:UITableView,didSelectRowAt indexPath:IndexPath) {
        let value = row.choices[indexPath.row].value
        let send = { [weak self] in guard let self else { return }; self.coordinator?.send("settings-command",value: value,id: self.row.id,scope: self.scope); self.navigationController?.popViewController(animated: !UIAccessibility.isReduceMotionEnabled) }
        if value == "revoke" {
            let alert = UIAlertController(title: row.choices[indexPath.row].title,message: row.title,preferredStyle: .alert)
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
