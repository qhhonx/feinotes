import UIKit

@MainActor
final class FeiObjectForm: UITableViewController, UITextFieldDelegate, UITextViewDelegate, UIAdaptivePresentationControllerDelegate {
    struct Request: Decodable {
        struct Field: Decodable, Equatable {
            struct Option: Decodable, Equatable { let value: String; let title: String; let swatch: [CGFloat]? }
            let key: String; let title: String; let kind: String; let value: String
            let options: [Option]?
            let value_title: String?
            let max_length: Int?
            let offset: Int?
            let date_mode: String?
            let enabled: Bool?
            let minimum: Float?; let maximum: Float?
            let path: String?; let rect: [CGFloat]?
            let search_key: String?; let search_text: String?; let search_placeholder: String?
        }
        let serial: UInt64; let kind: String; let title: String
        let live: Bool; let save: String; let cancel: String; let done: String
        let fields: [Field]
        let error: String?
        let initial: String?
    }
    private static weak var active: FeiObjectForm?
    private static var generation: UInt64 = 0
    private var request: Request
    private var inputs: [String: UIView] = [:]
    private var waiting = false
    private var loadedRequest = false
    private var focusedInitialField = false

    init(_ request: Request) { self.request = request; super.init(style: .insetGrouped) }
    required init?(coder: NSCoder) { fatalError("init(coder:) has not been implemented") }
    override func viewDidLoad() {
        super.viewDidLoad()
        tableView.accessibilityIdentifier = "fei.object.form"
        tableView.rowHeight = UITableView.automaticDimension
        tableView.estimatedRowHeight = 80
        navigationItem.rightBarButtonItem = UIBarButtonItem(title: request.live ? request.done : request.save,style: .done,target: self,action: #selector(save))
        navigationItem.rightBarButtonItem?.accessibilityIdentifier = "fei.object.save"
        if !request.live {
            navigationItem.leftBarButtonItem = UIBarButtonItem(title: request.cancel,style: .plain,target: self,action: #selector(cancel))
            navigationItem.leftBarButtonItem?.accessibilityIdentifier = "fei.object.cancel"
        }
        update(request)
    }
    override func viewDidAppear(_ animated: Bool) {
        super.viewDidAppear(animated)
        guard !focusedInitialField else { return }
        focusedInitialField = true
        if let key = request.initial,let field = request.fields.first(where: { $0.key == key && $0.enabled != false }) { showOptions(field); return }
        guard !["code","image","image-crop"].contains(request.kind),let key = request.fields.first(where: { $0.kind != "choice" && $0.enabled != false })?.key else { return }
        if let field = inputs[key] as? UITextField { field.becomeFirstResponder(); field.selectAll(nil) }
        else if let field = inputs[key] as? UITextView { field.becomeFirstResponder(); field.selectedRange = NSRange(location: 0,length: field.text.utf16.count) }
    }
    private func update(_ next: Request) {
        let old = request
        request = next
        waiting = false
        if title != next.title { title = next.title }
        if old.live != next.live {
            navigationItem.rightBarButtonItem?.title = next.live ? next.done : next.save
            if !next.live {
                navigationItem.leftBarButtonItem = UIBarButtonItem(title: next.cancel,style: .plain,target: self,action: #selector(cancel))
                navigationItem.leftBarButtonItem?.accessibilityIdentifier = "fei.object.cancel"
            } else { navigationItem.leftBarButtonItem = nil }
        }
        navigationItem.rightBarButtonItem?.isEnabled = true
        navigationItem.leftBarButtonItem?.isEnabled = true
        tableView.isScrollEnabled = next.kind != "image-crop"
        if let picker = navigationController?.topViewController as? FeiObjectOptions,let field = next.fields.first(where: { $0.key == picker.fieldKey }) { picker.update(field) }
        for field in next.fields {
            if let input = inputs[field.key] as? UIControl { input.isEnabled = field.enabled != false }
            if let input = inputs[field.key] as? UITextView { input.isEditable = field.enabled != false }
            if let input = inputs[field.key] as? UITextField,!input.isFirstResponder,input.text != field.value { input.text = field.value }
            if let input = inputs[field.key] as? UITextView,!input.isFirstResponder,input.text != field.value { input.text = field.value }
            if let input = inputs[field.key] as? FeiImageCrop { input.update(field) }
            if let input = inputs[field.key] as? UISlider,!input.isTracking {
                input.minimumValue = field.minimum ?? input.minimumValue; input.maximumValue = field.maximum ?? input.maximumValue
                input.value = Float(field.value) ?? input.minimumValue
                updateSliderLabel(input)
            }
            if let input = inputs[field.key] as? UISwitch { input.setOn(field.value == "true",animated: false) }
            if let input = inputs[field.key] as? UIDatePicker {
                let mode: UIDatePicker.Mode = field.date_mode == "date" ? .date : .dateAndTime
                if input.datePickerMode != mode { input.datePickerMode = mode }
                if input.timeZone?.secondsFromGMT(for: input.date) != (field.offset ?? 0) { input.timeZone = TimeZone(secondsFromGMT: field.offset ?? 0) }
                if let ms = Double(field.value),abs(input.date.timeIntervalSince1970-ms/1000) > 0.5 { input.setDate(Date(timeIntervalSince1970: ms/1000),animated: false) }
            }
        }
        if isViewLoaded {
            if !loadedRequest || old.fields.map(\.key) != next.fields.map(\.key) { loadedRequest = true; tableView.reloadData() }
            else {
                let rows = next.fields.enumerated().filter { ["choice","action","info"].contains($0.element.kind) && old.fields[$0.offset] != $0.element }.map { IndexPath(row: $0.offset,section: 0) }
                if !rows.isEmpty { tableView.reloadRows(at: rows,with: .none) }
            }
            if let error = next.error,!error.isEmpty {
                let footer = UIView(frame: CGRect(x: 0,y: 0,width: tableView.bounds.width,height: 72))
                let label = UILabel(frame: footer.bounds.insetBy(dx: 20,dy: 12))
                label.autoresizingMask = [.flexibleWidth,.flexibleHeight]
                label.numberOfLines = 0
                label.font = .preferredFont(forTextStyle: .footnote)
                label.textColor = .systemRed
                label.text = error
                label.accessibilityIdentifier = "fei.object.error"
                footer.addSubview(label)
                tableView.tableFooterView = footer
                if old.error != next.error,let sheet = navigationController?.sheetPresentationController {
                    sheet.animateChanges { sheet.selectedDetentIdentifier = .large }
                }
            } else { tableView.tableFooterView = nil }
        }
    }
    @objc private func cancel() { finish("object-close") }
    @objc private func save() { finish("object-submit") }
    private func finish(_ action: String) {
        guard !waiting else { return }
        waiting = true
        view.endEditing(true)
        navigationItem.rightBarButtonItem?.isEnabled = false
        navigationItem.leftBarButtonItem?.isEnabled = false
        send(action)
    }
    private func send(_ action: String, key: String? = nil, value: String? = nil) {
        var command: [String: Any] = ["action":action,"selection_serial":request.serial]
        if let key,let value,let data = try? JSONSerialization.data(withJSONObject: ["key":key,"value":value]) { command["value"] = String(data: data,encoding: .utf8) }
        guard let data = try? JSONSerialization.data(withJSONObject: command),let json = String(data: data,encoding: .utf8) else { return }
        DispatchQueue.main.async { json.withCString { fei_ios_command($0) } }
    }
    @objc private func textChanged(_ field: UITextField) {
        guard let key = inputs.first(where: { $0.value === field })?.key else { return }
        send("object-edit",key: key,value: field.text ?? "")
    }
    @objc private func toggleChanged(_ control: UISwitch) {
        guard let key = inputs.first(where: { $0.value === control })?.key else { return }
        send("object-edit",key: key,value: control.isOn ? "true" : "false")
    }
    @objc private func sliderChanged(_ control: UISlider) {
        guard let key = inputs.first(where: { $0.value === control })?.key else { return }
        send("object-edit",key: key,value: String(control.value))
        updateSliderLabel(control)
    }
    @objc private func sliderEnded(_ control: UISlider) { send("object-action",key: "resize-end",value: "") }
    private func updateSliderLabel(_ control: UISlider) {
        guard let key = inputs.first(where: { $0.value === control })?.key,let field = request.fields.first(where: { $0.key == key }) else { return }
        control.accessibilityValue = String(Int(control.value.rounded()))
        if let cell = control.superview?.superview as? UITableViewCell { (cell.contentView.viewWithTag(71) as? UILabel)?.text = "\(field.title) · \(Int(control.value.rounded()))" }
    }
    @objc private func dateChanged(_ control: UIDatePicker) {
        guard let key = inputs.first(where: { $0.value === control })?.key else { return }
        send("object-edit",key: key,value: String(Int64(control.date.timeIntervalSince1970*1000)))
    }
    func textViewDidChange(_ textView: UITextView) {
        guard let key = inputs.first(where: { $0.value === textView })?.key else { return }
        send("object-edit",key: key,value: textView.text)
    }
    func textFieldShouldReturn(_ textField: UITextField) -> Bool {
        let fields = request.fields.compactMap { inputs[$0.key] as? UITextField }
        if let index = fields.firstIndex(where: { $0 === textField }),index+1 < fields.count { fields[index+1].becomeFirstResponder() }
        else { save() }
        return true
    }
    func textField(_ textField: UITextField, shouldChangeCharactersIn range: NSRange, replacementString string: String) -> Bool {
        guard let key = inputs.first(where: { $0.value === textField })?.key,let field = request.fields.first(where: { $0.key == key }),field.kind == "number" else { return true }
        guard string.unicodeScalars.allSatisfy({ (48...57).contains($0.value) }),let changed = Range(range,in: textField.text ?? "") else { return false }
        let value = (textField.text ?? "").replacingCharacters(in: changed,with: string)
        return field.max_length.map { value.count <= $0 } ?? true
    }
    override func tableView(_ tableView: UITableView,numberOfRowsInSection section: Int) -> Int { request.fields.count }
    override func tableView(_ tableView: UITableView,heightForRowAt indexPath: IndexPath) -> CGFloat {
        guard request.fields[indexPath.row].kind == "image-crop" else { return UITableView.automaticDimension }
        let insets = tableView.adjustedContentInset
        return max(140,tableView.bounds.height-insets.top-insets.bottom-48)
    }
    override func viewWillTransition(to size: CGSize,with coordinator: UIViewControllerTransitionCoordinator) {
        super.viewWillTransition(to: size,with: coordinator)
        if request.kind == "image-crop" { coordinator.animate(alongsideTransition: nil) { [weak self] _ in self?.tableView.reloadData() } }
    }
    override func tableView(_ tableView: UITableView,cellForRowAt indexPath: IndexPath) -> UITableViewCell {
        let field = request.fields[indexPath.row]
        let cell = UITableViewCell(style: .default,reuseIdentifier: nil)
        cell.accessibilityIdentifier = "fei.object.row.\(field.key)"
        if field.kind == "image-crop" {
            let preview = (inputs[field.key] as? FeiImageCrop) ?? FeiImageCrop { [weak self] key,value in self?.send("object-action",key: key,value: value) }
            preview.update(field)
            inputs[field.key] = preview
            preview.translatesAutoresizingMaskIntoConstraints = false
            cell.selectionStyle = .none
            cell.contentView.addSubview(preview)
            NSLayoutConstraint.activate([preview.leadingAnchor.constraint(equalTo: cell.contentView.layoutMarginsGuide.leadingAnchor),preview.trailingAnchor.constraint(equalTo: cell.contentView.layoutMarginsGuide.trailingAnchor),preview.topAnchor.constraint(equalTo: cell.contentView.topAnchor,constant: 8),preview.bottomAnchor.constraint(equalTo: cell.contentView.bottomAnchor,constant: -8)])
            return cell
        }
        if field.kind == "info" {
            var content = cell.defaultContentConfiguration(); content.text = field.title; content.secondaryText = field.value
            cell.contentConfiguration = content; cell.selectionStyle = .none
            return cell
        }
        if field.kind == "choice" || field.kind == "action" {
            var content = cell.defaultContentConfiguration()
            content.text = field.title
            content.textProperties.color = field.enabled == false ? .tertiaryLabel : field.kind == "action" ? view.tintColor : .label
            content.secondaryText = field.options?.first(where: { $0.value == field.value })?.title ?? field.value_title ?? field.value
            content.textProperties.font = .preferredFont(forTextStyle: .body)
            cell.contentConfiguration = content
            cell.accessoryType = field.kind == "choice" ? .disclosureIndicator : .none
            cell.isUserInteractionEnabled = field.enabled != false
            if field.enabled == false { cell.accessibilityTraits.insert(.notEnabled) }
            return cell
        }
        if field.kind == "toggle" {
            var content = cell.defaultContentConfiguration()
            content.text = field.title
            cell.contentConfiguration = content
            cell.selectionStyle = .none
            let toggle = (inputs[field.key] as? UISwitch) ?? UISwitch()
            toggle.isOn = field.value == "true"
            toggle.isEnabled = field.enabled != false
            toggle.accessibilityIdentifier = "fei.object.\(field.key)"
            toggle.accessibilityLabel = field.title
            toggle.removeTarget(self,action: #selector(toggleChanged),for: .valueChanged)
            toggle.addTarget(self,action: #selector(toggleChanged),for: .valueChanged)
            inputs[field.key] = toggle
            cell.accessoryView = toggle
            return cell
        }
        cell.selectionStyle = .none
        let label = UILabel()
        label.translatesAutoresizingMaskIntoConstraints = false
        label.font = .preferredFont(forTextStyle: .subheadline)
        label.adjustsFontForContentSizeCategory = true
        label.text = field.title
        label.tag = 71
        let input: UIView
        if let existing = inputs[field.key] { input = existing }
        else if field.kind == "slider" {
            let slider = UISlider()
            slider.minimumValue = field.minimum ?? 0
            slider.maximumValue = field.maximum ?? 1
            slider.value = Float(field.value) ?? slider.minimumValue
            slider.addTarget(self,action: #selector(sliderChanged),for: .valueChanged)
            slider.addTarget(self,action: #selector(sliderEnded),for: [.touchUpInside,.touchUpOutside,.touchCancel])
            input = slider
        } else if field.kind == "date" {
            let picker = UIDatePicker()
            picker.calendar = Calendar(identifier: .gregorian)
            picker.timeZone = TimeZone(secondsFromGMT: field.offset ?? 0)
            picker.datePickerMode = field.date_mode == "date" ? .date : .dateAndTime
            picker.preferredDatePickerStyle = .wheels
            picker.minimumDate = Date(timeIntervalSince1970: -62_135_596_800)
            picker.maximumDate = Date(timeIntervalSince1970: 253_402_300_799)
            if let ms = Double(field.value) { picker.date = Date(timeIntervalSince1970: ms/1000) }
            picker.addTarget(self,action: #selector(dateChanged),for: .valueChanged)
            input = picker
        } else if field.kind == "multiline" {
            let text = UITextView()
            text.text = field.value
            text.font = .monospacedSystemFont(ofSize: UIFont.preferredFont(forTextStyle: .body).pointSize,weight: .regular)
            text.adjustsFontForContentSizeCategory = true
            text.autocorrectionType = .no
            text.autocapitalizationType = .none
            text.smartQuotesType = .no
            text.smartDashesType = .no
            text.delegate = self
            input = text
        } else {
            let text = UITextField()
            text.text = field.value
            text.font = .preferredFont(forTextStyle: .body)
            text.adjustsFontForContentSizeCategory = true
            text.clearButtonMode = .whileEditing
            text.returnKeyType = .next
            text.delegate = self
            text.addTarget(self,action: #selector(textChanged),for: .editingChanged)
            if field.kind == "url" { text.keyboardType = .URL; text.autocorrectionType = .no; text.autocapitalizationType = .none; text.returnKeyType = .done }
            if field.kind == "number" { text.keyboardType = .numberPad; text.autocorrectionType = .no }
            input = text
        }
        if let control = input as? UIControl { control.isEnabled = field.enabled != false }
        if let text = input as? UITextView { text.isEditable = field.enabled != false }
        input.accessibilityIdentifier = "fei.object.\(field.key)"
        input.accessibilityLabel = field.title
        inputs[field.key] = input
        if let slider = input as? UISlider { label.text = "\(field.title) · \(Int(slider.value.rounded()))"; slider.accessibilityValue = String(Int(slider.value.rounded())) }
        input.translatesAutoresizingMaskIntoConstraints = false
        cell.contentView.addSubview(label)
        cell.contentView.addSubview(input)
        NSLayoutConstraint.activate([
            label.leadingAnchor.constraint(equalTo: cell.contentView.layoutMarginsGuide.leadingAnchor),label.trailingAnchor.constraint(equalTo: cell.contentView.layoutMarginsGuide.trailingAnchor),label.topAnchor.constraint(equalTo: cell.contentView.topAnchor,constant: 12),
            input.leadingAnchor.constraint(equalTo: label.leadingAnchor),input.trailingAnchor.constraint(equalTo: label.trailingAnchor),input.topAnchor.constraint(equalTo: label.bottomAnchor,constant: 4),input.bottomAnchor.constraint(equalTo: cell.contentView.bottomAnchor,constant: -10),input.heightAnchor.constraint(greaterThanOrEqualToConstant: field.kind == "date" ? 216 : field.kind == "multiline" ? 160 : 44),
        ])
        return cell
    }
    override func tableView(_ tableView: UITableView,didSelectRowAt indexPath: IndexPath) {
        tableView.deselectRow(at: indexPath,animated: true)
        let field = request.fields[indexPath.row]
        guard field.enabled != false else { return }
        if field.kind == "action" { view.endEditing(true); send("object-action",key: field.key,value: ""); return }
        showOptions(field)
    }
    private func showOptions(_ field: Request.Field) {
        guard field.kind == "choice",field.options != nil,field.enabled != false else { return }
        view.endEditing(true)
        let picker = FeiObjectOptions(field: field,select: { [weak self] value in self?.send("object-edit",key: field.key,value: value) },search: { [weak self] value in
            if let key = field.search_key { self?.send("object-edit",key: key,value: value) }
        })
        navigationController?.pushViewController(picker,animated: !UIAccessibility.isReduceMotionEnabled)
    }
    func presentationControllerDidDismiss(_ presentationController: UIPresentationController) { send("object-close") }
    static func enqueue(_ request: Request) {
        generation &+= 1
        let current = generation
        DispatchQueue.main.async {
            guard generation == current else { return }
            if let active,active.request.serial == request.serial,active.navigationController?.presentingViewController != nil { active.update(request); return }
            guard let host = EditorHost.active?.navigationController?.topViewController else { return }
            host.view.endEditing(true)
            let present = {
                guard generation == current else { return }
                let controller = FeiObjectForm(request)
                active = controller
                let navigation = UINavigationController(rootViewController: controller)
                navigation.sheetPresentationController?.detents = ["reminder","countdown","code","image","image-crop"].contains(request.kind) ? [.large()] : [.medium(),.large()]
                navigation.sheetPresentationController?.prefersGrabberVisible = true
                navigation.presentationController?.delegate = controller
                host.present(navigation,animated: true)
            }
            if let previous = host.presentedViewController { previous.dismiss(animated: true,completion: present) } else { present() }
        }
    }
    static func dismissCurrent() {
        generation &+= 1
        let current = generation
        DispatchQueue.main.async {
            guard generation == current else { return }
            active?.dismiss(animated: true)
            active = nil
        }
    }
}

@MainActor
private final class FeiObjectOptions: UITableViewController, UISearchResultsUpdating {
    let fieldKey: String
    private var options: [FeiObjectForm.Request.Field.Option]
    private var current: String
    private let select: (String) -> Void
    private let search: (String) -> Void
    private let searchKey: String?
    private let initialSearch: String
    private let searchPlaceholder: String?
    init(field: FeiObjectForm.Request.Field, select: @escaping (String) -> Void, search: @escaping (String) -> Void) {
        fieldKey = field.key; options = field.options ?? []; current = field.value; self.select = select; self.search = search
        searchKey = field.search_key; initialSearch = field.search_text ?? ""; searchPlaceholder = field.search_placeholder
        super.init(style: .insetGrouped)
        title = field.title
    }
    required init?(coder: NSCoder) { fatalError("init(coder:) has not been implemented") }
    override func viewDidLoad() {
        super.viewDidLoad()
        tableView.rowHeight = 48; tableView.accessibilityIdentifier = "fei.object.options"
        if searchKey != nil {
            let controller = UISearchController(searchResultsController: nil)
            controller.obscuresBackgroundDuringPresentation = false
            controller.searchBar.text = initialSearch
            controller.searchBar.placeholder = searchPlaceholder
            controller.searchBar.searchTextField.accessibilityIdentifier = "fei.object.language-search"
            controller.searchResultsUpdater = self
            navigationItem.searchController = controller
            navigationItem.hidesSearchBarWhenScrolling = false
            definesPresentationContext = true
        }
    }
    func update(_ field: FeiObjectForm.Request.Field) {
        let next = field.options ?? []
        if next != options || current != field.value { options = next; current = field.value; if isViewLoaded { tableView.reloadData() } }
    }
    func updateSearchResults(for searchController: UISearchController) { search(searchController.searchBar.text ?? "") }
    override func tableView(_ tableView: UITableView,numberOfRowsInSection section: Int) -> Int { options.count }
    override func tableView(_ tableView: UITableView,cellForRowAt indexPath: IndexPath) -> UITableViewCell {
        let option = options[indexPath.row]
        let cell = UITableViewCell(style: .default,reuseIdentifier: nil)
        var content = cell.defaultContentConfiguration()
        content.text = option.title
        if let color = option.swatch,color.count == 4 { content.image = UIImage(systemName: "circle.fill"); content.imageProperties.tintColor = UIColor(red: color[0],green: color[1],blue: color[2],alpha: color[3]) }
        cell.contentConfiguration = content
        cell.accessoryType = current == option.value ? .checkmark : .none
        cell.accessibilityIdentifier = "fei.object.option.\(option.value)"
        return cell
    }
    override func tableView(_ tableView: UITableView,didSelectRowAt indexPath: IndexPath) { select(options[indexPath.row].value); navigationController?.popViewController(animated: !UIAccessibility.isReduceMotionEnabled) }
}

@_cdecl("fei_ios_present_object_form")
@MainActor
func feiIOSPresentObjectForm(_ json: UnsafePointer<CChar>) {
    guard let request = try? JSONDecoder().decode(FeiObjectForm.Request.self,from: Data(String(cString: json).utf8)) else { return }
    FeiObjectForm.enqueue(request)
}
@_cdecl("fei_ios_dismiss_object_form")
@MainActor
func feiIOSDismissObjectForm() { FeiObjectForm.dismissCurrent() }
