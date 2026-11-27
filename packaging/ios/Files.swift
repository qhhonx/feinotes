import UIKit
import UniformTypeIdentifiers

@MainActor
final class FeiFileExporter: NSObject, UIDocumentPickerDelegate, UIAdaptivePresentationControllerDelegate {
    static let shared = FeiFileExporter()
    private var stagingFolder: URL?
    private weak var activePicker: UIDocumentPickerViewController?
    private weak var activeActivity: UIActivityViewController?

    func present(_ url: URL,sharing: Bool) {
        // Shared Markdown folders are nested inside their per-export staging folder.
        let folder = url.hasDirectoryPath && !sharing ? url : url.deletingLastPathComponent()
        guard stagingFolder == nil,let host = EditorHost.active,let navigation = host.navigationController,
              let controller = navigation.topViewController,let window = controller.viewIfLoaded?.window,
              let presenter = window.rootViewController else {
            discardExportFolder(folder)
            return
        }
        stagingFolder = folder
        // Wait for the initiating menu to settle before presenting a system sheet.
        Task { @MainActor [weak self,weak controller,weak host,weak navigation,weak presenter,weak window] in
            guard let self else { return }
            try? await Task.sleep(for: .milliseconds(300))
            for _ in 0..<30 {
                guard let host,let navigation,let controller,let presenter,let window,
                      host.navigationController === navigation,navigation.topViewController === controller,
                      controller.viewIfLoaded?.window === window,window.rootViewController === presenter else { self.finish(); return }
                if presenter.presentedViewController == nil,controller.presentedViewController == nil,
                   navigation.transitionCoordinator == nil,presenter.transitionCoordinator == nil {
                    if sharing { self.openShareSheet(url,from: presenter) }
                    else { self.openPicker(url,from: presenter,appearance: host.navigationCoordinator?.interfaceAppearance ?? .unspecified) }
                    return
                }
                try? await Task.sleep(for: .milliseconds(50))
            }
            self.finish()
        }
    }

    private func openPicker(_ url: URL,from presenter: UIViewController,appearance: UIUserInterfaceStyle) {
        let picker = UIDocumentPickerViewController(forExporting: [url],asCopy: true)
        picker.delegate = self
        picker.overrideUserInterfaceStyle = appearance
        picker.modalPresentationStyle = presenter.traitCollection.horizontalSizeClass == .regular ? .formSheet : .pageSheet
        picker.sheetPresentationController?.prefersGrabberVisible = true
        activePicker = picker
        presenter.present(picker,animated: true) { [weak self,weak picker] in
            guard let self,let picker,self.activePicker === picker else { return }
            if picker.presentingViewController == nil { self.finish() }
        }
        picker.presentationController?.delegate = self
    }

    private func openShareSheet(_ url: URL,from presenter: UIViewController) {
        let activity = UIActivityViewController(activityItems: [url],applicationActivities: nil)
        if let popover = activity.popoverPresentationController,let view = presenter.viewIfLoaded {
            popover.sourceView = view
            popover.sourceRect = CGRect(x: view.bounds.midX,y: view.bounds.midY,width: 1,height: 1)
            popover.permittedArrowDirections = []
        }
        activeActivity = activity
        activity.completionWithItemsHandler = { [weak self,weak activity] _,_,_,_ in
            guard let self,let activity,self.activeActivity === activity else { return }
            self.finish()
        }
        presenter.present(activity,animated: true) { [weak self,weak activity] in
            guard let self,let activity,self.activeActivity === activity else { return }
            if activity.presentingViewController == nil { self.finish() }
        }
        activity.presentationController?.delegate = self
    }

    func documentPicker(_ controller: UIDocumentPickerViewController,didPickDocumentsAt urls: [URL]) {
        if controller === activePicker { finish() }
    }
    func documentPickerWasCancelled(_ controller: UIDocumentPickerViewController) {
        if controller === activePicker { finish() }
    }
    func presentationControllerDidDismiss(_ presentationController: UIPresentationController) {
        if presentationController.presentedViewController === activePicker
            || presentationController.presentedViewController === activeActivity { finish() }
    }

    private func finish() {
        if let stagingFolder { discardExportFolder(stagingFolder) }
        stagingFolder = nil
        activePicker = nil
        activeActivity = nil
    }

    private func discardExportFolder(_ folder: URL) {
        let root = FileManager.default.temporaryDirectory.appending(path: "FeiNotesExports",directoryHint: .isDirectory)
        if folder.deletingLastPathComponent().standardizedFileURL == root.standardizedFileURL {
            try? FileManager.default.removeItem(at: folder)
        }
    }
}

@_cdecl("fei_ios_present_export")
func feiIOSPresentExport(_ path: UnsafePointer<CChar>,_ sharing: Bool) {
    let path = String(cString: path)
    var directory: ObjCBool = false
    guard FileManager.default.fileExists(atPath: path, isDirectory: &directory) else { return }
    let url = URL(fileURLWithPath: path, isDirectory: directory.boolValue)
    Task { @MainActor in FeiFileExporter.shared.present(url,sharing: sharing) }
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
