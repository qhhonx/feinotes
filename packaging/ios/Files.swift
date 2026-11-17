import UIKit
import UniformTypeIdentifiers

@MainActor
final class FeiFileExporter: NSObject, UIDocumentPickerDelegate {
    static let shared = FeiFileExporter()
    private var source: URL?

    func present(_ url: URL) {
        guard source == nil, let controller = EditorHost.active?.navigationController?.topViewController else { return }
        source = url
        let picker = UIDocumentPickerViewController(forExporting: [url], asCopy: true)
        picker.delegate = self
        let open = { controller.present(picker, animated: true) }
        if let presented = controller.presentedViewController { presented.dismiss(animated: true, completion: open) }
        else { open() }
    }

    func documentPicker(_ controller: UIDocumentPickerViewController, didPickDocumentsAt urls: [URL]) { finish() }
    func documentPickerWasCancelled(_ controller: UIDocumentPickerViewController) { finish() }
    private func finish() {
        guard let source else { return }
        let root = FileManager.default.temporaryDirectory.appending(path: "FeiNotesExports", directoryHint: .isDirectory)
        let folder = source.hasDirectoryPath ? source : source.deletingLastPathComponent()
        if folder.deletingLastPathComponent().standardizedFileURL == root.standardizedFileURL {
            try? FileManager.default.removeItem(at: folder)
        }
        self.source = nil
    }
}

@_cdecl("fei_ios_present_export")
func feiIOSPresentExport(_ path: UnsafePointer<CChar>) {
    let path = String(cString: path)
    var directory: ObjCBool = false
    guard FileManager.default.fileExists(atPath: path, isDirectory: &directory) else { return }
    let url = URL(fileURLWithPath: path, isDirectory: directory.boolValue)
    Task { @MainActor in FeiFileExporter.shared.present(url) }
}

@MainActor
private final class FeiLegacyLibraryPicker: NSObject, UIDocumentPickerDelegate, UIAdaptivePresentationControllerDelegate {
    static var current: FeiLegacyLibraryPicker?
    private var completion: FeiLibraryCompletion?
    private let context: UnsafeRawPointer?
    private var copying = false

    init(completion: @escaping FeiLibraryCompletion, context: UnsafeRawPointer?) { self.completion = completion; self.context = context }
    func present(_ title: String) {
        guard let host = EditorHost.active?.navigationController?.topViewController, host.presentedViewController == nil else {
            finish(-1, "Could not open the folder picker. Please try again."); return
        }
        let picker = UIDocumentPickerViewController(forOpeningContentTypes: [.folder], asCopy: false)
        picker.delegate = self; picker.allowsMultipleSelection = false; picker.modalPresentationStyle = .pageSheet
        picker.title = title; picker.view.accessibilityIdentifier = "fei.legacy.picker"
        host.present(picker, animated: true)
        picker.presentationController?.delegate = self
    }
    func documentPickerWasCancelled(_ controller: UIDocumentPickerViewController) { if !copying { finish(0, "") } }
    func presentationControllerDidDismiss(_ presentationController: UIPresentationController) { if !copying { finish(0, "") } }
    func documentPicker(_ controller: UIDocumentPickerViewController, didPickDocumentsAt urls: [URL]) {
        guard completion != nil, !copying, let source = urls.first else { return }
        copying = true
        Task {
            do {
                let path = try await Task.detached(priority: .userInitiated) { try Self.stage(source) }.value
                finish(1, path.path)
            } catch { finish(-1, error.localizedDescription) }
        }
    }
    // Keep the security scope and read coordination alive through the copy.
    // Rust receives only our temporary snapshot, never an external URL.
    nonisolated private static func stage(_ source: URL) throws -> URL {
        guard source.startAccessingSecurityScopedResource() else { throw CocoaError(.fileReadNoPermission) }
        defer { source.stopAccessingSecurityScopedResource() }
        let manager = FileManager.default
        let root = manager.temporaryDirectory.appendingPathComponent("FeiNotesImports", isDirectory: true).appendingPathComponent(UUID().uuidString, isDirectory: true)
        let destination = root.appendingPathComponent("library", isDirectory: true)
        try manager.createDirectory(at: root, withIntermediateDirectories: true)
        do {
            var coordinatedError: NSError?
            var copyError: Error?
            NSFileCoordinator().coordinate(readingItemAt: source, options: .withoutChanges, error: &coordinatedError) { readable in
                do { try manager.copyItem(at: readable, to: destination) } catch { copyError = error }
            }
            if let error = coordinatedError ?? copyError as NSError? { throw error }
            return destination
        } catch { try? manager.removeItem(at: root); throw error }
    }
    private func finish(_ status: Int32, _ text: String) {
        guard let completion else { return }
        self.completion = nil
        text.withCString { completion(status, $0, context) }
        Self.current = nil
    }
}

@_cdecl("fei_ios_pick_library")
func feiIOSPickLibrary(_ title: UnsafePointer<CChar>, _ completion: @escaping FeiLibraryCompletion, _ context: UnsafeRawPointer?) {
    let title = String(cString: title)
    Task { @MainActor in
        guard FeiLegacyLibraryPicker.current == nil else { "A folder picker is already open.".withCString { completion(-1, $0, context) }; return }
        let picker = FeiLegacyLibraryPicker(completion: completion, context: context)
        FeiLegacyLibraryPicker.current = picker; picker.present(title)
    }
}
