// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Copyright 2026 Fei Notes contributors. See LICENSES.md.
import AppKit
import CloudKit
import CryptoKit
import Foundation

private func restoreZone(_ channel: String?) -> CKRecordZone.ID {
    CKRecordZone.ID(zoneName: "QuietLibraryRestore-v1" + (channel.map { "-QA-" + $0 } ?? ""))
}
private let maxFile = 25 * 1024 * 1024
private let maxTotal = 100 * 1024 * 1024

private struct Snapshot: Codable, Equatable {
    struct File: Codable, Equatable {
        let path: String
        let sha256: String
        let size: Int
    }

    let version: Int
    let files: [File]

    func validate() throws {
        guard version == 1, files.count <= 200 else { throw RestoreError.invalid("快照版本或文件数") }
        if files.isEmpty { return }
        var paths = Set<String>()
        var total = 0
        for file in files {
            let parts = file.path.split(separator: "/", omittingEmptySubsequences: false)
            let validPath = parts.allSatisfy {
                !$0.isEmpty && !$0.hasPrefix(".") && !$0.contains("\\") && !$0.contains("\0")
            }
            guard validPath,
                  ["format.json", "index.json"].contains(file.path) ||
                    (parts.count >= 2 && ["docs", "trash", "assets"].contains(String(parts[0]))),
                  paths.insert(file.path.precomposedStringWithCanonicalMapping.lowercased()).inserted,
                  file.sha256.count == 64, file.sha256.utf8.allSatisfy({ (48...57).contains($0) || (97...102).contains($0) }),
                  (0...maxFile).contains(file.size) else { throw RestoreError.invalid(file.path) }
            total += file.size
        }
        guard total <= maxTotal, paths.contains("format.json"), paths.contains("index.json") else {
            throw RestoreError.invalid("快照缺少文档库索引，或超过测试版容量")
        }
    }
}

// An empty baseline and a legacy baseline contend for the same record.
// Both replicas use whichever baseline was committed first.
private func emptyLibrary(_ db: CKDatabase, in zone: CKRecordZone.ID) async throws -> Data {
    _ = try await db.save(CKRecordZone(zoneID: zone))
    let manifest = try JSONEncoder().encode(Snapshot(version: 1, files: []))
    let entry = CKRecord(recordType: "QuietRestoreLibrary", recordID: .init(recordName: "library", zoneID: zone))
    entry["snapshot"] = manifest
    do {
        _ = try await db.save(entry)
        return manifest
    } catch let error as CKError where error.code == .serverRecordChanged {
        guard let existing = try await existingLibrary(db, in: zone) else { throw error }
        return existing
    }
}

private enum RestoreError: LocalizedError {
    case invalid(String)
    case account
    case missing
    case exists
    case timeout

    var errorDescription: String? {
        switch self {
        case .invalid(let item): "文档库快照校验失败 · \(item)"
        case .account: "请先在系统设置登录 Apple 账户，再开启 iCloud 同步。"
        case .missing: "当前 Apple 账户的 iCloud 私有库还没有 Quiet 测试快照。"
        case .exists: "目标位置已有文档库，已保留。"
        case .timeout: "iCloud 恢复超时，请联网后重新打开应用。"
        }
    }
}

private func sha256(_ bytes: Data) -> String {
    SHA256.hash(data: bytes).map { String(format: "%02x", $0) }.joined()
}

private func assetRecord(_ hash: String, in zone: CKRecordZone.ID) -> CKRecord.ID {
    CKRecord.ID(recordName: "file-" + hash, zoneID: zone)
}

private func existingLibrary(_ db: CKDatabase, in zone: CKRecordZone.ID) async throws -> Data? {
    let libraryRecord = CKRecord.ID(recordName: "library", zoneID: zone)
    do {
        let record = try await db.record(for: libraryRecord)
        guard let data = record["snapshot"] as? Data, data.count <= 64 * 1024 else {
            throw RestoreError.invalid("库入口")
        }
        return data
    } catch let error as CKError where error.code == .unknownItem || error.code == .zoneNotFound {
        return nil
    }
}

private func collect(_ root: URL) throws -> [(String, URL)] {
    let manager = FileManager.default
    var result: [(String, URL)] = []
    func visit(_ url: URL, _ relative: String) throws {
        let values = try url.resourceValues(forKeys: [.isSymbolicLinkKey, .isRegularFileKey, .isDirectoryKey, .fileSizeKey])
        guard values.isSymbolicLink != true else { throw RestoreError.invalid(relative) }
        if values.isDirectory == true {
            for child in try manager.contentsOfDirectory(at: url, includingPropertiesForKeys: nil, options: [.skipsHiddenFiles]) {
                let name = child.lastPathComponent
                if name.hasSuffix(".bak") || name.hasSuffix(".tmp") { continue }
                try visit(child, relative + "/" + name)
            }
        } else {
            guard values.isRegularFile == true, let size = values.fileSize, size <= maxFile,
                  result.count < 200 else { throw RestoreError.invalid(relative) }
            result.append((relative, url))
        }
    }
    for name in ["format.json", "index.json", "docs", "assets", "trash"] {
        let url = root.appending(path: name)
        if manager.fileExists(atPath: url.path) { try visit(url, name) }
    }
    return result.sorted { $0.0 < $1.0 }
}

private func publish(_ root: URL, to db: CKDatabase, in zone: CKRecordZone.ID) async throws -> Int {
    let libraryRecord = CKRecord.ID(recordName: "library", zoneID: zone)
    let manager = FileManager.default
    let stage = manager.temporaryDirectory.appending(path: "Quiet-publish-\(UUID())")
    try manager.createDirectory(at: stage, withIntermediateDirectories: true)
    defer { try? manager.removeItem(at: stage) }
    var files: [Snapshot.File] = []
    var total = 0
    for (path, url) in try collect(root) {
        let bytes = try Data(contentsOf: url, options: .mappedIfSafe)
        guard bytes.count <= maxFile, bytes.count <= maxTotal - total else { throw RestoreError.invalid(path) }
        total += bytes.count
        let hash = sha256(bytes)
        let local = stage.appending(path: hash)
        if !manager.fileExists(atPath: local.path) { try bytes.write(to: local) }
        files.append(.init(path: path, sha256: hash, size: bytes.count))
    }
    let snapshot = Snapshot(version: 1, files: files)
    try snapshot.validate()
    let encoder = JSONEncoder()
    encoder.outputFormatting = .sortedKeys
    let manifest = try encoder.encode(snapshot)
    guard manifest.count <= 64 * 1024 else { throw RestoreError.invalid("库入口过大") }
    if let current = try await existingLibrary(db, in: zone) {
        guard current == manifest else { throw RestoreError.exists }
        return files.count
    }

    _ = try await db.save(CKRecordZone(zoneID: zone))
    for hash in Set(files.map(\.sha256)).sorted() {
        let record = CKRecord(recordType: "QuietRestoreFile", recordID: assetRecord(hash, in: zone))
        record["dataFile"] = CKAsset(fileURL: stage.appending(path: hash))
        do { _ = try await db.save(record) }
        catch let error as CKError where error.code == .serverRecordChanged {}
    }

    let entry = CKRecord(recordType: "QuietRestoreLibrary", recordID: libraryRecord)
    entry["snapshot"] = manifest
    do { _ = try await db.save(entry) }
    catch let error as CKError where error.code == .serverRecordChanged {
        guard try await existingLibrary(db, in: zone) == manifest else { throw RestoreError.exists }
    }
    return files.count
}

private func restore(_ db: CKDatabase, in zone: CKRecordZone.ID, account: String, testRoot: String?,
                     directory: String = "Library", allowEmpty: Bool = false, container: CKContainer? = nil) async throws -> Reply {
    let manager = FileManager.default
    let support = try manager.url(for: .applicationSupportDirectory, in: .userDomainMask, appropriateFor: nil, create: true)
    let accountKey = sha256(Data(account.utf8))
    let parent = testRoot.map { URL(fileURLWithPath: $0, isDirectory: true) }
        ?? support.appending(path: "Quiet Cloud Restore").appending(path: accountKey)
    let destination = parent.appending(path: directory)
    let marker = destination.appending(path: ".cloud-restore-ready")
    func cached() -> Bool {
        (try? String(contentsOf: marker, encoding: .utf8))?.hasPrefix(accountKey + "\n") == true
            && manager.fileExists(atPath: destination.appending(path: "index.json").path)
    }
    if cached() { return Reply(path: destination.path, source: "local") }
    guard !manager.fileExists(atPath: destination.path) else { throw RestoreError.exists }
    let manifest: Data
    if let existing = try await existingLibrary(db, in: zone) {
        manifest = existing
    } else if allowEmpty {
        if let container { try await shareAccount(container, expected: accountKey) }
        try Task.checkCancellation()
        manifest = try await emptyLibrary(db, in: zone)
    } else {
        throw RestoreError.missing
    }
    let snapshot = try JSONDecoder().decode(Snapshot.self, from: manifest)
    try snapshot.validate()

    let stage = parent.appending(path: ".restore-\(UUID())")
    try manager.createDirectory(at: stage, withIntermediateDirectories: true)
    defer { try? manager.removeItem(at: stage) }

    if snapshot.files.isEmpty {
        try Data("{}".utf8).write(to: stage.appending(path: "index.json"))
    }
    for file in snapshot.files {
        try Task.checkCancellation()
        let record = try await db.record(for: assetRecord(file.sha256, in: zone))
        guard let url = (record["dataFile"] as? CKAsset)?.fileURL,
              try url.resourceValues(forKeys: [.fileSizeKey]).fileSize == file.size else {
            throw RestoreError.invalid(file.path)
        }
        let target = stage.appending(path: file.path)
        try manager.createDirectory(at: target.deletingLastPathComponent(), withIntermediateDirectories: true)
        try manager.copyItem(at: url, to: target)
        guard sha256(try Data(contentsOf: target, options: .mappedIfSafe)) == file.sha256 else {
            throw RestoreError.invalid(file.path)
        }
    }
    if let container { try await shareAccount(container, expected: accountKey) }
    try Task.checkCancellation()
    try (accountKey + "\n" + sha256(manifest)).write(to: stage.appending(path: ".cloud-restore-ready"), atomically: true, encoding: .utf8)
    do { try manager.moveItem(at: stage, to: destination) }
    catch { if !cached() { throw error } }
    return Reply(path: destination.path, source: snapshot.files.isEmpty ? "empty" : "cloud")
}

private func syncSupport() throws -> URL {
    let support = try FileManager.default.url(for: .applicationSupportDirectory, in: .userDomainMask, appropriateFor: nil, create: true)
    if cloudEnvironment == "Development" && containerID == "iCloud.app.quietmd.cloudlab" {
        return support.appending(path: "Quiet Cloud Library")
    }
    return support.appending(path: "Fei Notes Library").appending(path: cloudEnvironment).appending(path: containerID)
}

private func cachedSyncRoot(_ parent: URL, accountKey: String? = nil) -> Reply? {
    let verified = parent.appending(path: "verified-account")
    let marker = FileManager.default.fileExists(atPath: verified.path) ? verified : parent.appending(path: "active-account")
    guard let key = accountKey ?? (try? String(contentsOf: marker, encoding: .utf8)),
          key.count == 64, key.utf8.allSatisfy({ (48...57).contains($0) || (97...102).contains($0) }) else { return nil }
    let root = parent.appending(path: key)
    if accountKey == nil && FileManager.default.fileExists(atPath: root.appending(path: "require-account-verification").path) {
        return nil
    }
    let stage = root.appending(path: ".restore-source")
    let ready = (try? String(contentsOf: root.appending(path: ".cloud-restore-ready"), encoding: .utf8))?.hasPrefix(key + "\n") == true
        && FileManager.default.fileExists(atPath: root.appending(path: "library.sqlite3").path)
    let downloaded = (try? String(contentsOf: stage.appending(path: ".cloud-restore-ready"), encoding: .utf8))?.hasPrefix(key + "\n") == true
        && FileManager.default.fileExists(atPath: stage.appending(path: "index.json").path)
    guard ready || downloaded else { return nil }
    return Reply(path: root.path, source: "local")
}

private func unavailableAccountNetwork(_ error: Error) -> Bool {
    guard let error = error as? CKError else { return false }
    return error.code == .networkUnavailable || error.code == .networkFailure || error.code == .serviceUnavailable
}

#if FEI_CLOUD_QA
private func startupFixture(_ root: String?) -> String? {
    guard let root else { return nil }
    return (try? String(contentsOf: URL(fileURLWithPath: root).appending(path: "qa-startup-account"), encoding: .utf8))?
        .trimmingCharacters(in: .whitespacesAndNewlines)
}
#endif

private func accountStatus(_ container: CKContainer, startupTestRoot: String?) async throws -> CKAccountStatus {
    #if FEI_CLOUD_QA
    if let state = startupFixture(startupTestRoot) {
        switch state {
        case "signed-out": return .noAccount
        case "unavailable": return .couldNotDetermine
        case "timeout": try await Task.sleep(for: .seconds(10))
        default: break
        }
    }
    #endif
    return try await container.accountStatus()
}

private struct Reply: Codable {
    var path: String?
    var source: String?
    var count: Int?
    var error: String?
    var fallbackReason: String?
}

private func localReply(_ parent: URL, reason: String? = nil) -> Reply {
    Reply(path: parent.appending(path: "local-only").path, source: "localOnly", fallbackReason: reason)
}

// The worker publishes its result before signaling. The main thread reads it after waiting.
private final class Completion: @unchecked Sendable {
    let done = DispatchSemaphore(value: 0)
    var reply = Reply()
    private let lock = NSLock()
    private var timedOut = false
    private var startupFallback: Reply?
    private var startupParent: URL?

    func allowLocalFallback(_ parent: URL) {
        lock.withLock {
            startupParent = parent
            startupFallback = localReply(parent, reason: "unavailable")
        }
    }

    func fallback(_ reason: String) -> Reply? {
        lock.withLock {
            guard var fallback = startupFallback else { return nil }
            fallback.fallbackReason = reason
            return fallback
        }
    }

    func persist(_ value: String, as name: String, in parent: URL) throws {
        try lock.withLock {
            guard !timedOut else { return }
            try FileManager.default.createDirectory(at: parent, withIntermediateDirectories: true)
            try value.write(to: parent.appending(path: name), atomically: true, encoding: .utf8)
        }
    }

    func clearAccountVerification(in root: URL) throws {
        try lock.withLock {
            guard !timedOut else { return }
            do { try FileManager.default.removeItem(at: root.appending(path: "require-account-verification")) }
            catch let error as CocoaError where error.code == .fileNoSuchFile { }
        }
    }

    func timeout() -> Reply {
        lock.withLock {
            timedOut = true
            return startupParent.flatMap { cachedSyncRoot($0) }
                ?? startupFallback ?? Reply(error: RestoreError.timeout.localizedDescription)
        }
    }
}

private func signedOut(_ completion: Completion, parent: URL?, action: Int32, reason: String) throws -> Reply {
    if let parent, action != 7 {
        try completion.persist("signed-out", as: "verified-account", in: parent)
    }
    guard let fallback = completion.fallback(reason) else { throw RestoreError.account }
    return fallback
}

@_cdecl("quiet_cloud_restore")
public func quietCloudRestore(_ action: Int32, _ path: UnsafePointer<CChar>?, _ scope: UnsafePointer<CChar>?) -> UnsafeMutablePointer<CChar>? {
    let input = path.map { String(cString: $0) }
    let channel = scope.map { String(cString: $0) }
    let completion = Completion()
    let worker = Task.detached {
        defer { completion.done.signal() }
        var syncParent: URL?
        do {
            #if !FEI_CLOUD_QA
            guard [2, 4, 5, 7].contains(action), input == nil, channel == nil else {
                throw RestoreError.invalid("不支持此文档库操作")
            }
            #endif
            if let channel, UUID(uuidString: channel) == nil { throw RestoreError.invalid("测试库标识") }
            let zone = restoreZone(channel)
            syncParent = [2, 4, 5, 6, 7].contains(action)
                ? try input.map { URL(fileURLWithPath: $0, isDirectory: true) } ?? syncSupport() : nil
            if let syncParent {
                if action == 2 { completion.allowLocalFallback(syncParent) }
                let manager = FileManager.default
                let localChoice = syncParent.appending(path: "use-local-library")
                if action == 5 {
                    try manager.createDirectory(at: syncParent, withIntermediateDirectories: true)
                    try Data().write(to: localChoice, options: .atomic)
                }
                if action == 5 || (action == 2 && (manager.fileExists(atPath: localChoice.path)
                    || !manager.fileExists(atPath: syncParent.appending(path: "active-account").path))) {
                    completion.reply = localReply(syncParent)
                    return
                }
            }
            let container = CKContainer(identifier: containerID)
            let account: String
            do {
                let status = try await accountStatus(container, startupTestRoot: action == 2 ? input : nil)
                switch status {
                case .noAccount, .restricted:
                    completion.reply = try signedOut(completion, parent: syncParent, action: action,
                        reason: status == .noAccount ? "signedOut" : "restricted")
                    return
                case .available: break
                default:
                    if action != 4, let syncParent, let cached = cachedSyncRoot(syncParent) {
                        completion.reply = cached
                        return
                    }
                    if let fallback = completion.fallback("unavailable") { completion.reply = fallback; return }
                    throw RestoreError.account
                }
                #if FEI_CLOUD_QA
                if action == 2 && startupFixture(input) == "not-authenticated" { throw CKError(.notAuthenticated) }
                #endif
                account = try await container.userRecordID().recordName
            } catch let error as CKError where error.code == .notAuthenticated {
                // CloudKit can report notAuthenticated while the current account is temporarily unavailable.
                let status = try? await accountStatus(container, startupTestRoot: action == 2 ? input : nil)
                if status == .noAccount || status == .restricted {
                    completion.reply = try signedOut(completion, parent: syncParent, action: action,
                        reason: status == .noAccount ? "signedOut" : "restricted")
                    return
                }
                if action != 4, let syncParent, let cached = cachedSyncRoot(syncParent) { completion.reply = cached; return }
                if let fallback = completion.fallback("unavailable") { completion.reply = fallback; return }
                throw error
            } catch where unavailableAccountNetwork(error) {
                if action != 4, let syncParent, let cached = cachedSyncRoot(syncParent) {
                    completion.reply = cached
                    return
                }
                if let fallback = completion.fallback("unavailable") { completion.reply = fallback; return }
                throw error
            }
            try Task.checkCancellation()
            if action == 1, let input {
                let count = try await publish(URL(fileURLWithPath: input), to: container.privateCloudDatabase, in: zone)
                completion.reply = Reply(source: "published", count: count)
            } else if let syncParent {
                let key = sha256(Data(account.utf8))
                if action != 7 { try completion.persist(key, as: "verified-account", in: syncParent) }
                let root = syncParent.appending(path: key)
                let result: Reply
                let cached = cachedSyncRoot(syncParent, accountKey: key)
                if action == 4 || cached == nil {
                    try await CloudAccess.shared.require(account: key, channel: channel)
                }
                if let cached { result = cached }
                else {
                    result = try await restore(container.privateCloudDatabase, in: zone, account: account,
                        testRoot: root.path, directory: ".restore-source", allowEmpty: true, container: container)
                }
                try Task.checkCancellation()
                if action != 7 { try completion.clearAccountVerification(in: root) }
                if action == 2 || action == 6 {
                    try completion.persist(key, as: "active-account", in: syncParent)
                }
                completion.reply = Reply(path: root.path, source: result.source)
            } else {
                completion.reply = try await restore(container.privateCloudDatabase, in: zone, account: account, testRoot: input)
            }
        } catch ShareError.account {
            if let syncParent, action != 7 { try? completion.persist("needs-verification", as: "verified-account", in: syncParent) }
            completion.reply = completion.fallback("accountChanged") ?? Reply(error: ShareError.account.localizedDescription)
        } catch ShareError.signedOut {
            completion.reply = (try? signedOut(completion, parent: syncParent, action: action, reason: "signedOut"))
                ?? Reply(error: ShareError.signedOut.localizedDescription)
        } catch CloudAccessError.required {
            completion.reply = completion.fallback("subscriptionRequired")
                ?? Reply(error: CloudAccessError.required.localizedDescription, fallbackReason: "subscriptionRequired")
        } catch {
            completion.reply = completion.fallback("restoreFailed") ?? Reply(error: error.localizedDescription)
        }
    }
    let reply: Reply
    if completion.done.wait(timeout: .now() + (action == 2 ? 8 : 70)) == .success { reply = completion.reply }
    else {
        reply = completion.timeout()
        worker.cancel()
    }
    guard let bytes = try? JSONEncoder().encode(reply) else { return nil }
    return strdup(String(decoding: bytes, as: UTF8.self))
}

@_cdecl("quiet_cloud_restore_free")
public func quietCloudRestoreFree(_ reply: UnsafeMutablePointer<CChar>?) { free(reply) }

@MainActor
@_cdecl("quiet_cloud_restore_alert")
public func quietCloudRestoreAlert(_ message: UnsafePointer<CChar>) {
    let alert = NSAlert()
    alert.messageText = Bundle.main.object(forInfoDictionaryKey: "CFBundleDisplayName") as? String ?? "Fei Notes"
    alert.informativeText = String(cString: message)
    alert.runModal()
}
