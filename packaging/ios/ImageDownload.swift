import Foundation

typealias FeiImageDownloadCompletion = @convention(c) (Bool,UnsafeMutableRawPointer?) -> Void

private final class HTTPSRedirects: NSObject, URLSessionTaskDelegate {
    func urlSession(_ session: URLSession, task: URLSessionTask, willPerformHTTPRedirection response: HTTPURLResponse, newRequest request: URLRequest, completionHandler: @escaping @Sendable (URLRequest?) -> Void) {
        completionHandler(request.url?.scheme?.lowercased() == "https" ? request : nil)
    }
}

// URLSession supplies transport; Rust validates and commits the cache file.
@_cdecl("fei_ios_download_file")
func feiIOSDownloadFile(_ source: UnsafePointer<CChar>,_ destination: UnsafePointer<CChar>,_ httpsOnly: Bool,_ complete: @escaping FeiImageDownloadCompletion,_ context: UnsafeMutableRawPointer?) {
    let path = String(cString: destination)
    let schemes = httpsOnly ? ["https"] : ["http","https"]
    guard let url = URL(string: String(cString: source)),schemes.contains(url.scheme?.lowercased() ?? ""),url.host != nil else { complete(false,context); return }
    let config = URLSessionConfiguration.ephemeral
    config.timeoutIntervalForRequest = 60; config.timeoutIntervalForResource = 60
    config.urlCache = nil; config.httpCookieStorage = nil; config.urlCredentialStorage = nil
    config.httpShouldSetCookies = false
    config.requestCachePolicy = .reloadIgnoringLocalCacheData
    let session = URLSession(configuration: config, delegate: httpsOnly ? HTTPSRedirects() : nil, delegateQueue: nil)
    let address = UInt(bitPattern: context)
    var request = URLRequest(url: url)
    request.cachePolicy = .reloadIgnoringLocalCacheData
    let task = session.downloadTask(with: request) { location,response,error in
        defer { session.finishTasksAndInvalidate() }
        var copied = false
        if error == nil,let response = response as? HTTPURLResponse,(200..<300).contains(response.statusCode),let location {
            let target = URL(fileURLWithPath: path)
            do {
                if FileManager.default.fileExists(atPath: path) { try FileManager.default.removeItem(at: target) }
                try FileManager.default.moveItem(at: location,to: target)
                copied = true
            } catch { copied = false }
        }
        complete(copied,UnsafeMutableRawPointer(bitPattern: address))
    }
    task.resume()
}
