import UIKit

@MainActor
final class FeiLinkForm: UITableViewController, UITextFieldDelegate, UIAdaptivePresentationControllerDelegate {
    struct Request: Decodable {
        let serial: UInt64
        let title: String
        let save: String
        let cancel: String
        let text_label: String
        let url_label: String
        let text: String
        let url: String
        let text_editable: Bool
        let remove: String?
    }
    private static weak var active: FeiLinkForm?
    private static var generation: UInt64 = 0
    private let request: Request
    private let textField = UITextField()
    private let urlField = UITextField()
    private var waiting = false
    private let errorLabel = UILabel()
    private let errorFooter = UIView()

    init(_ request: Request) { self.request = request; super.init(style: .insetGrouped) }
    required init?(coder: NSCoder) { fatalError("init(coder:) has not been implemented") }

    override func viewDidLoad() {
        super.viewDidLoad()
        title = request.title
        tableView.rowHeight = 56
        tableView.accessibilityIdentifier = "fei.link.form"
        navigationItem.leftBarButtonItem = UIBarButtonItem(title: request.cancel,style: .plain,target: self,action: #selector(cancel))
        navigationItem.rightBarButtonItem = UIBarButtonItem(title: request.save,style: .done,target: self,action: #selector(save))
        navigationItem.leftBarButtonItem?.accessibilityIdentifier = "fei.link.cancel"
        navigationItem.rightBarButtonItem?.accessibilityIdentifier = "fei.link.save"
        for field in [textField,urlField] {
            field.translatesAutoresizingMaskIntoConstraints = false
            field.font = .preferredFont(forTextStyle: .body)
            field.adjustsFontForContentSizeCategory = true
            field.clearButtonMode = .whileEditing
            field.delegate = self
            field.addTarget(self,action: #selector(changed),for: .editingChanged)
        }
        textField.text = request.text
        textField.isEnabled = request.text_editable
        textField.returnKeyType = .next
        textField.accessibilityIdentifier = "fei.link.text"
        urlField.text = request.url
        urlField.keyboardType = .URL
        urlField.textContentType = .URL
        urlField.autocapitalizationType = .none
        urlField.autocorrectionType = .no
        urlField.returnKeyType = .done
        urlField.accessibilityIdentifier = "fei.link.url"
        changed()
    }
    override func viewDidAppear(_ animated: Bool) {
        super.viewDidAppear(animated)
        if request.text_editable && request.text.isEmpty { textField.becomeFirstResponder() }
        else { urlField.becomeFirstResponder() }
    }
    @objc private func changed() { navigationItem.rightBarButtonItem?.isEnabled = !waiting && !(urlField.text ?? "").trimmingCharacters(in: .whitespacesAndNewlines).isEmpty }
    @objc private func cancel() { send("link-cancel") }
    @objc private func save() {
        guard navigationItem.rightBarButtonItem?.isEnabled == true else { return }
        guard let data = try? JSONSerialization.data(withJSONObject: ["text":textField.text ?? "","url":urlField.text ?? ""]),let value = String(data: data,encoding: .utf8) else { return }
        send("link-submit",value: value)
    }
    private func send(_ action: String, value: String? = nil) {
        guard !waiting else { return }
        waiting = true
        changed()
        var command: [String: Any] = ["action":action,"selection_serial":request.serial]
        if let value { command["value"] = value }
        guard let data = try? JSONSerialization.data(withJSONObject: command),let json = String(data: data,encoding: .utf8) else { return }
        json.withCString { fei_ios_command($0) }
    }
    func textFieldShouldReturn(_ textField: UITextField) -> Bool {
        if textField === self.textField { urlField.becomeFirstResponder() } else { save() }
        return true
    }
    override func numberOfSections(in tableView: UITableView) -> Int { request.remove == nil ? 1 : 2 }
    override func tableView(_ tableView: UITableView,numberOfRowsInSection section: Int) -> Int { section == 0 ? 2 : 1 }
    override func tableView(_ tableView: UITableView,cellForRowAt indexPath: IndexPath) -> UITableViewCell {
        let cell = UITableViewCell(style: .default,reuseIdentifier: nil)
        if indexPath.section == 1 {
            var content = cell.defaultContentConfiguration()
            content.text = request.remove
            content.textProperties.color = .systemRed
            cell.contentConfiguration = content
            cell.accessibilityIdentifier = "fei.link.remove"
            return cell
        }
        cell.selectionStyle = .none
        let label = UILabel()
        label.translatesAutoresizingMaskIntoConstraints = false
        label.font = .preferredFont(forTextStyle: .body)
        label.adjustsFontForContentSizeCategory = true
        label.text = indexPath.row == 0 ? request.text_label : request.url_label
        let field = indexPath.row == 0 ? textField : urlField
        field.placeholder = label.text
        cell.contentView.addSubview(label)
        cell.contentView.addSubview(field)
        NSLayoutConstraint.activate([
            label.leadingAnchor.constraint(equalTo: cell.contentView.layoutMarginsGuide.leadingAnchor),label.centerYAnchor.constraint(equalTo: cell.contentView.centerYAnchor),label.widthAnchor.constraint(equalToConstant: 72),
            field.leadingAnchor.constraint(equalTo: label.trailingAnchor,constant: 12),field.trailingAnchor.constraint(equalTo: cell.contentView.layoutMarginsGuide.trailingAnchor),field.centerYAnchor.constraint(equalTo: cell.contentView.centerYAnchor),field.heightAnchor.constraint(greaterThanOrEqualToConstant: 44),
        ])
        return cell
    }
    override func tableView(_ tableView: UITableView,didSelectRowAt indexPath: IndexPath) {
        tableView.deselectRow(at: indexPath,animated: true)
        if indexPath.section == 1 { send("link-remove") }
    }
    static func enqueue(_ request: Request) {
        generation &+= 1
        let current = generation
        DispatchQueue.main.async {
            guard generation == current,let host = EditorHost.active?.navigationController?.topViewController else { return }
            let present = {
                guard generation == current else { return }
                let controller = FeiLinkForm(request)
                active = controller
                let navigation = UINavigationController(rootViewController: controller)
                navigation.sheetPresentationController?.detents = [.medium(),.large()]
                navigation.presentationController?.delegate = controller
                host.present(navigation,animated: true)
            }
            if let previous = host.presentedViewController { previous.dismiss(animated: true,completion: present) } else { present() }
        }
    }
    static func dismissCurrent() { generation &+= 1; active?.dismiss(animated: true); active = nil }
    func presentationControllerDidDismiss(_ presentationController: UIPresentationController) { send("link-cancel") }
    static func fail(_ serial: UInt64, message: String) {
        guard let active,active.request.serial == serial else { return }
        active.waiting = false
        active.changed()
        active.errorLabel.text = message
        active.errorLabel.numberOfLines = 0
        active.errorLabel.font = .preferredFont(forTextStyle: .footnote)
        active.errorLabel.textColor = .systemRed
        active.errorLabel.accessibilityIdentifier = "fei.link.error"
        let width = active.tableView.bounds.width
        let height = active.errorLabel.sizeThatFits(CGSize(width: max(1,width-40),height: .greatestFiniteMagnitude)).height
        active.errorFooter.frame = CGRect(x: 0,y: 0,width: width,height: height+24)
        active.errorLabel.frame = CGRect(x: 20,y: 12,width: max(1,width-40),height: height)
        active.errorFooter.addSubview(active.errorLabel)
        active.tableView.tableFooterView = active.errorFooter
    }
}

@_cdecl("fei_ios_present_link_form")
@MainActor
func feiIOSPresentLinkForm(_ json: UnsafePointer<CChar>) {
    guard let request = try? JSONDecoder().decode(FeiLinkForm.Request.self,from: Data(String(cString: json).utf8)) else { return }
    FeiLinkForm.enqueue(request)
}
@_cdecl("fei_ios_dismiss_link_form")
@MainActor
func feiIOSDismissLinkForm() { FeiLinkForm.dismissCurrent() }
@_cdecl("fei_ios_link_error")
@MainActor
func feiIOSLinkError(_ serial: UInt64,_ message: UnsafePointer<CChar>) { FeiLinkForm.fail(serial,message: String(cString: message)) }
