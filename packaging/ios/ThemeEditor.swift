import UIKit

private struct FeiThemeRequest: Decodable {
    let path: String
    let title: String
    let save: String
    let cancel: String
    let discard: String
    let discardMessage: String
    let error: String
    let changed: String
}

@MainActor
private final class FeiThemeEditor: UIViewController, UITextViewDelegate {
    private let request: FeiThemeRequest
    private let original: String
    private let source = UITextView()

    init(_ request: FeiThemeRequest, original: String) {
        self.request = request
        self.original = original
        super.init(nibName: nil, bundle: nil)
        title = request.title
        navigationItem.leftBarButtonItem = UIBarButtonItem(title: request.cancel, style: .plain, target: self, action: #selector(cancel))
        navigationItem.rightBarButtonItem = UIBarButtonItem(title: request.save, style: .done, target: self, action: #selector(save))
        navigationItem.rightBarButtonItem?.accessibilityIdentifier = "fei.theme.save"
        navigationItem.leftBarButtonItem?.accessibilityIdentifier = "fei.theme.cancel"
    }
    required init?(coder: NSCoder) { fatalError("init(coder:) is unsupported") }

    override func viewDidLoad() {
        super.viewDidLoad()
        view.backgroundColor = .systemBackground
        source.translatesAutoresizingMaskIntoConstraints = false
        source.accessibilityIdentifier = "fei.theme.source"
        source.font = .monospacedSystemFont(ofSize: 15, weight: .regular)
        source.adjustsFontForContentSizeCategory = true
        source.text = original
        source.delegate = self
        source.autocorrectionType = .no
        source.spellCheckingType = .no
        source.autocapitalizationType = .none
        source.smartQuotesType = .no
        source.smartDashesType = .no
        source.smartInsertDeleteType = .no
        source.keyboardDismissMode = .interactive
        source.alwaysBounceVertical = true
        source.textContainerInset = UIEdgeInsets(top: 16, left: 12, bottom: 16, right: 12)
        view.addSubview(source)
        NSLayoutConstraint.activate([
            source.topAnchor.constraint(equalTo: view.safeAreaLayoutGuide.topAnchor),
            source.leadingAnchor.constraint(equalTo: view.leadingAnchor),
            source.trailingAnchor.constraint(equalTo: view.trailingAnchor),
            source.bottomAnchor.constraint(equalTo: view.keyboardLayoutGuide.topAnchor),
        ])
    }

    func textViewDidChange(_ textView: UITextView) { navigationController?.isModalInPresentation = source.text != original }

    @objc private func cancel() {
        guard source.text != original else { dismiss(animated: true); return }
        let alert = UIAlertController(title: request.discardMessage, message: nil, preferredStyle: .alert)
        alert.addAction(UIAlertAction(title: request.cancel, style: .cancel))
        alert.addAction(UIAlertAction(title: request.discard, style: .destructive) { [weak self] _ in self?.dismiss(animated: true) })
        present(alert, animated: true)
    }

    @objc private func save() {
        let url = URL(fileURLWithPath: request.path)
        do {
            guard try String(contentsOf: url, encoding: .utf8) == original else { showError(request.changed); return }
            // The existing Rust theme watcher parses this same atomic file,
            // reports invalid rules and refreshes document styles.
            try source.text.write(to: url, atomically: true, encoding: .utf8)
            view.endEditing(true)
            dismiss(animated: true)
        } catch { showError(request.error) }
    }

    private func showError(_ message: String) {
        let alert = UIAlertController(title: request.title, message: message, preferredStyle: .alert)
        alert.addAction(UIAlertAction(title: request.cancel, style: .cancel))
        present(alert, animated: true)
    }
}

@_cdecl("fei_ios_edit_theme")
func feiIOSEditTheme(_ json: UnsafePointer<CChar>) {
    let bytes = Data(String(cString: json).utf8)
    Task { @MainActor in
        guard let request = try? JSONDecoder().decode(FeiThemeRequest.self, from: bytes),
              let original = try? String(contentsOfFile: request.path, encoding: .utf8),
              let host = EditorHost.active?.navigationController?.topViewController else { return }
        let controller = UINavigationController(rootViewController: FeiThemeEditor(request, original: original))
        controller.sheetPresentationController?.detents = [.large()]
        host.present(controller, animated: true)
    }
}
