import UIKit
import QuickLook
import QuickLookThumbnailing
import ImageIO
import UniformTypeIdentifiers

@MainActor
private final class FeiPreviewItem: NSObject, QLPreviewItem {
    let previewItemURL: URL?
    let previewItemTitle: String?
    init(file: URL, title: String?) { previewItemURL = file; previewItemTitle = title; super.init() }
}

@MainActor
private final class FeiPreviewController: QLPreviewController, QLPreviewControllerDataSource, QLPreviewControllerDelegate {
    private let item: FeiPreviewItem
    init(file: URL, title: String?) { item = FeiPreviewItem(file: file,title: title); super.init(nibName: nil,bundle: nil) }
    required init?(coder: NSCoder) { fatalError("init(coder:) has not been implemented") }
    override func viewDidLoad() { super.viewDidLoad(); dataSource = self; delegate = self }
    func numberOfPreviewItems(in controller: QLPreviewController) -> Int { 1 }
    func previewController(_ controller: QLPreviewController,previewItemAt index: Int) -> any QLPreviewItem { item }
    func previewController(_ controller: QLPreviewController,editingModeFor previewItem: any QLPreviewItem) -> QLPreviewItemEditingMode { .disabled }
}

@MainActor
final class FeiFilePreview: UIViewController {
    private let file: URL
    private let preview: FeiPreviewController
    private let previewDirectory: URL?
    init(file: URL, title: String? = nil) {
        var resolved = file
        var directory: URL?
        // The shared remote cache has extensionless names; Quick Look needs a type.
        if file.pathExtension.isEmpty,let source = CGImageSourceCreateWithURL(file as CFURL,nil),let type = CGImageSourceGetType(source),let ext = UTType(type as String)?.preferredFilenameExtension {
            let folder = FileManager.default.temporaryDirectory.appendingPathComponent("fei-preview-" + UUID().uuidString,isDirectory: true)
            do {
                try FileManager.default.createDirectory(at: folder,withIntermediateDirectories: true)
                let copy = folder.appendingPathComponent("image").appendingPathExtension(ext)
                try FileManager.default.copyItem(at: file,to: copy)
                resolved = copy; directory = folder
            } catch { try? FileManager.default.removeItem(at: folder) }
        }
        self.file = resolved; previewDirectory = directory
        preview = FeiPreviewController(file: resolved,title: title)
        super.init(nibName: nil,bundle: nil); self.title = title ?? file.lastPathComponent
    }
    deinit { if let previewDirectory { try? FileManager.default.removeItem(at: previewDirectory) } }
    required init?(coder: NSCoder) { fatalError("init(coder:) has not been implemented") }
    override func viewDidLoad() {
        super.viewDidLoad()
        view.backgroundColor = .systemBackground
        addChild(preview)
        preview.view.translatesAutoresizingMaskIntoConstraints = false
        view.addSubview(preview.view)
        NSLayoutConstraint.activate([
            preview.view.leadingAnchor.constraint(equalTo: view.leadingAnchor),preview.view.trailingAnchor.constraint(equalTo: view.trailingAnchor),
            preview.view.topAnchor.constraint(equalTo: view.safeAreaLayoutGuide.topAnchor),preview.view.bottomAnchor.constraint(equalTo: view.bottomAnchor),
        ])
        preview.didMove(toParent: self)
        let coordinator = EditorHost.active?.navigationCoordinator
        navigationItem.leftBarButtonItem = UIBarButtonItem(title: coordinator?.localized("关闭","Close") ?? "Close",
            style: .plain,target: self,action: #selector(close))
        navigationItem.leftBarButtonItem?.accessibilityIdentifier = "fei.file.done"
        navigationItem.rightBarButtonItem = UIBarButtonItem(title: coordinator?.localized("共享","Share") ?? "Share",
            style: .plain,target: self,action: #selector(share(_:)))
        navigationItem.rightBarButtonItem?.accessibilityIdentifier = "fei.file.share"
    }
    @objc private func close() { dismiss(animated: true) }
    @objc private func share(_ sender: UIBarButtonItem) {
        let sheet = UIActivityViewController(activityItems: [file],applicationActivities: nil)
        sheet.view.accessibilityIdentifier = "fei.file.share-sheet"
        sheet.popoverPresentationController?.sourceItem = sender
        present(sheet,animated: true)
    }
}

@_cdecl("fei_ios_preview_file")
@MainActor
func feiIOSPreviewFile(_ path: UnsafePointer<CChar>) {
    let file = URL(fileURLWithPath: String(cString: path))
    presentFilePreview(file,title: nil)
}

@_cdecl("fei_ios_preview_named_file")
@MainActor
func feiIOSPreviewNamedFile(_ path: UnsafePointer<CChar>,_ title: UnsafePointer<CChar>) {
    presentFilePreview(URL(fileURLWithPath: String(cString: path)),title: String(cString: title))
}

@MainActor
private func presentFilePreview(_ file: URL, title: String?) {
    guard FileManager.default.fileExists(atPath: file.path) else { return }
    DispatchQueue.main.async {
        guard let host = EditorHost.active?.navigationController?.topViewController else { return }
        host.view.endEditing(true)
        let open = { host.present(UINavigationController(rootViewController: FeiFilePreview(file: file,title: title)),animated: true) }
        if let previous = host.presentedViewController { previous.dismiss(animated: true,completion: open) } else { open() }
    }
}

/// Runs only on the Rust thumbnail worker; never waits on UIKit's main thread.
private final class FeiThumbnailResult: @unchecked Sendable {
    let done = DispatchSemaphore(value: 0)
    var saved = false
}

@_cdecl("fei_ios_thumbnail")
func feiIOSThumbnail(_ path: UnsafePointer<CChar>,_ destination: UnsafePointer<CChar>) -> Bool {
    guard !Thread.isMainThread else { return false }
    let source = URL(fileURLWithPath: String(cString: path))
    let output = URL(fileURLWithPath: String(cString: destination))
    let request = QLThumbnailGenerator.Request(fileAt: source,size: CGSize(width: 1200,height: 1200),scale: 1,representationTypes: .thumbnail)
    let result = FeiThumbnailResult()
    QLThumbnailGenerator.shared.generateBestRepresentation(for: request) { representation,_ in
        defer { result.done.signal() }
        guard let representation,let png = UIImage(cgImage: representation.cgImage).pngData() else { return }
        do { try png.write(to: output,options: .atomic); result.saved = true } catch { }
    }
    guard result.done.wait(timeout: .now()+15) == .success else { QLThumbnailGenerator.shared.cancel(request); return false }
    return result.saved
}
