import UIKit
import WebKit

private struct FeiPrintRequest: Decodable {
    let html: String
    let title: String
    let path: String?
}

@MainActor
private final class FeiPageRenderer: UIPrintPageRenderer {
    static let paper = CGRect(x: 0, y: 0, width: 595.28, height: 841.89)
    override var paperRect: CGRect { Self.paper }
    override var printableRect: CGRect { Self.paper.insetBy(dx: 48, dy: 40) }
}

@MainActor
private final class FeiPrintJob: NSObject, WKNavigationDelegate {
    static var current: FeiPrintJob?
    private let request: FeiPrintRequest
    private var completion: FeiPrintCompletion?
    private let context: UnsafeRawPointer?
    private var web: WKWebView?
    private var timeout: Task<Void, Never>?
    private var rendering = false

    init(_ request: FeiPrintRequest, completion: @escaping FeiPrintCompletion, context: UnsafeRawPointer?) {
        self.request = request
        self.completion = completion
        self.context = context
    }

    private func text(_ zh: String, _ en: String) -> String {
        EditorHost.active?.navigationCoordinator?.localized(zh, en) ?? zh
    }

    func start() {
        guard let host = EditorHost.active?.navigationController?.topViewController else {
            finish(-1, text("无法打开打印窗口，请重试。", "Could not open the print window. Please try again.")); return
        }
        let configuration = WKWebViewConfiguration()
        configuration.websiteDataStore = .nonPersistent()
        let width = (FeiPageRenderer.paper.width - 96) * 96 / 72
        let web = WKWebView(frame: CGRect(x: -20000, y: 0, width: width, height: 1000), configuration: configuration)
        self.web = web
        web.navigationDelegate = self
        web.isUserInteractionEnabled = false
        web.accessibilityElementsHidden = true
        host.view.addSubview(web)
        timeout = Task { [weak self] in
            do { try await Task.sleep(for: .seconds(25)) } catch { return }
            guard let self else { return }
            self.finish(-1, self.text("打印超时，请稍后重试", "Printing timed out. Please try again."))
        }
        web.loadHTMLString(request.html, baseURL: nil)
    }

    func webView(_ webView: WKWebView, didFinish navigation: WKNavigation!) {
        guard !rendering else { return }
        rendering = true
        Task { [weak self] in
            guard let self else { return }
            do {
                // Wait for fonts and embedded or previously uncached remote
                // images, with the same bounded preparation as desktop output.
                _ = try await webView.callAsyncJavaScript("""
                    await Promise.race([
                        Promise.all([document.fonts.ready, ...Array.from(document.images).map(image =>
                            image.complete ? Promise.resolve() : new Promise(resolve => {
                                image.addEventListener('load', resolve, { once: true });
                                image.addEventListener('error', resolve, { once: true });
                            }))]),
                        new Promise(resolve => setTimeout(resolve, 10000))
                    ]);
                    return true;
                    """, arguments: [:], in: nil, contentWorld: .page)
                guard self.completion != nil else { return }
                try self.render(webView)
            } catch {
                self.finish(-1, self.text("导出失败，请重试", "Export failed. Please try again."))
            }
        }
    }

    func webView(_ webView: WKWebView, didFail navigation: WKNavigation!, withError error: Error) {
        finish(-1, error.localizedDescription)
    }
    func webView(_ webView: WKWebView, didFailProvisionalNavigation navigation: WKNavigation!, withError error: Error) {
        finish(-1, error.localizedDescription)
    }
    func webViewWebContentProcessDidTerminate(_ webView: WKWebView) {
        finish(-1, text("导出失败，请重试", "Export failed. Please try again."))
    }

    private func render(_ webView: WKWebView) throws {
        let renderer = FeiPageRenderer()
        renderer.addPrintFormatter(webView.viewPrintFormatter(), startingAtPageAt: 0)
        let pages = renderer.numberOfPages
        guard pages > 0 else { throw NSError(domain: "FeiNotes.Print", code: 1) }
        renderer.prepare(forDrawingPages: NSRange(location: 0, length: pages))
        let format = UIGraphicsPDFRendererFormat()
        format.documentInfo = [kCGPDFContextTitle as String: request.title, kCGPDFContextCreator as String: "Fei Notes"]
        let pdf = UIGraphicsPDFRenderer(bounds: FeiPageRenderer.paper, format: format).pdfData { context in
            for page in 0..<pages {
                autoreleasepool {
                    context.beginPage()
                    renderer.drawPage(at: page, in: renderer.paperRect)
                }
            }
        }
        timeout?.cancel()
        timeout = nil
        webView.navigationDelegate = nil
        webView.removeFromSuperview()
        web = nil
        if let path = request.path {
            try pdf.write(to: URL(fileURLWithPath: path), options: .atomic)
            finish(1)
        } else {
            presentPrint(pdf)
        }
    }

    private func presentPrint(_ pdf: Data) {
        guard let host = EditorHost.active?.navigationController?.topViewController, UIPrintInteractionController.isPrintingAvailable else {
            finish(-1, text("当前无法使用系统打印。", "System printing is unavailable.")); return
        }
        let controller = UIPrintInteractionController.shared
        let info = UIPrintInfo(dictionary: nil)
        info.jobName = request.title
        info.outputType = .general
        controller.printInfo = info
        controller.printingItem = pdf
        let completed: UIPrintInteractionController.CompletionHandler = { [weak self] _, printed, error in
            guard let self else { return }
            if let error { self.finish(-1, error.localizedDescription) }
            else { self.finish(printed ? 1 : 0) }
        }
        let presented: Bool
        if UIDevice.current.userInterfaceIdiom == .pad {
            let anchor = CGRect(x: host.view.bounds.maxX - 40, y: host.view.safeAreaInsets.top + 20, width: 1, height: 1)
            presented = controller.present(from: anchor, in: host.view, animated: true, completionHandler: completed)
        } else {
            presented = controller.present(animated: true, completionHandler: completed)
        }
        if !presented { finish(-1, text("无法打开打印窗口，请重试。", "Could not open the print window. Please try again.")) }
    }

    private func finish(_ status: Int32, _ message: String = "") {
        guard let completion else { return }
        self.completion = nil
        timeout?.cancel()
        timeout = nil
        web?.navigationDelegate = nil
        web?.stopLoading()
        web?.removeFromSuperview()
        web = nil
        if Self.current === self { Self.current = nil }
        message.withCString { completion(status, $0, context) }
    }
}

@_cdecl("fei_ios_print_begin")
func feiIOSPrintBegin(_ json: UnsafePointer<CChar>, _ completion: @escaping FeiPrintCompletion, _ context: UnsafeRawPointer?) {
    let bytes = Data(String(cString: json).utf8)
    Task { @MainActor in
        guard FeiPrintJob.current == nil, let request = try? JSONDecoder().decode(FeiPrintRequest.self, from: bytes) else {
            let message = EditorHost.active?.navigationCoordinator?.localized("另一项导出或打印正在进行中。", "Another export or print job is in progress.") ?? "Another export or print job is in progress."
            message.withCString { completion(-1, $0, context) }
            return
        }
        let job = FeiPrintJob(request, completion: completion, context: context)
        FeiPrintJob.current = job
        job.start()
    }
}
