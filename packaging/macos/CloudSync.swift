// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Copyright 2026 Fei Notes contributors. See LICENSES.md.
import AppKit
import CloudKit
import CryptoKit
import Foundation
import Network
import Quartz

private let syncZoneName = "QuietLibraryEdits-v4"
private let legacyZoneName = "QuietLibraryEdits-v3"
private let editType = "QuietNoteEdit"
private let maxUpdate = 16 * 1024 * 1024
#if FEI_CLOUD_QA
private let timeoutAfterUpload = "timeout-after-upload"
private let holdAfterUpload = "hold-after-upload"
#endif

private final class CloudAccountRevision: @unchecked Sendable {
    let lock = NSLock()
    var value: UInt64 = 0
}

private let accountRevision = CloudAccountRevision()

@_cdecl("quiet_cloud_account_revision")
public func quietCloudAccountRevision() -> UInt64 { accountRevision.lock.withLock { accountRevision.value } }

func checkCloudAccountRevision(_ revision: UInt64) throws {
    guard quietCloudAccountRevision() == revision else { throw SyncError.accountChecking }
    try Task.checkCancellation()
}

private final class SyncResumeMonitor: @unchecked Sendable {
    private let path = NWPathMonitor()
    private let queue = DispatchQueue(label: "Fei Notes network recovery")
    private let signal: @convention(c) (Int32) -> Void
    private let workspaceNotifications: NotificationCenter
    private let wakeObserver: any NSObjectProtocol
    private let accountObserver: any NSObjectProtocol
    private let container = CKContainer(identifier: containerID)
    // Read and written only on the path monitor's serial queue.
    private var wasReachable: Bool?

    @MainActor init(signal: @escaping @convention(c) (Int32) -> Void) {
        self.signal = signal
        let workspace = NSWorkspace.shared
        workspaceNotifications = workspace.notificationCenter
        wakeObserver = workspaceNotifications.addObserver(forName: NSWorkspace.didWakeNotification,
            object: workspace, queue: .main) { _ in signal(1) }
        accountObserver = NotificationCenter.default.addObserver(forName: .CKAccountChanged,
            object: nil, queue: nil) { _ in
                accountRevision.lock.withLock { accountRevision.value &+= 1 }
                signal(3)
            }
        path.pathUpdateHandler = { [weak self] path in self?.updatePath(reachable: path.status == .satisfied) }
        path.start(queue: queue)
    }

    private func updatePath(reachable: Bool) {
        let restored = wasReachable == false && reachable
        wasReachable = reachable
        if restored { signal(2) }
    }

    deinit {
        path.cancel()
        workspaceNotifications.removeObserver(wakeObserver)
        NotificationCenter.default.removeObserver(accountObserver)
    }

    #if FEI_CLOUD_QA
    func simulateAccountChange() { NotificationCenter.default.post(name: .CKAccountChanged, object: container) }

    func simulateNetworkResume() {
        queue.async {
            self.updatePath(reachable: false)
            self.updatePath(reachable: true)
            self.updatePath(reachable: true)
        }
    }
    #endif
}

@MainActor private var syncResumeMonitor: SyncResumeMonitor?

@MainActor
@_cdecl("quiet_cloud_watch_sync_resume")
public func quietCloudWatchSyncResume(_ signal: @escaping @convention(c) (Int32) -> Void) {
    if syncResumeMonitor == nil { syncResumeMonitor = SyncResumeMonitor(signal: signal) }
}

#if FEI_CLOUD_QA
@MainActor
@_cdecl("quiet_cloud_simulate_sync_resume")
public func quietCloudSimulateSyncResume(_ kind: Int32) {
    if kind == 1 {
        let workspace = NSWorkspace.shared
        workspace.notificationCenter.post(name: NSWorkspace.didWakeNotification, object: workspace)
    }
    else if kind == 2 { syncResumeMonitor?.simulateNetworkResume() }
    else if kind == 3 { syncResumeMonitor?.simulateAccountChange() }
}
#endif

private struct Edit: Codable {
    let id: String
    let note: String
    let data: Data
    let collaborator: Bool?

    func recordID(in zone: CKRecordZone.ID) -> CKRecord.ID { CKRecord.ID(recordName: id, zoneID: zone) }
    var digest: Data { Data(SHA256.hash(data: data)) }

    func validate() throws {
        guard UUID(uuidString: id) != nil, UUID(uuidString: note) != nil,
              !data.isEmpty, data.count <= maxUpdate else { throw SyncError.invalid }
    }
}

private struct Exchange: Decodable {
    let account: String
    let channel: String?
    let token: Data?
    let edits: [Edit]
    let legacy: Bool
    let room: ShareScope?
    let activityOnly: Bool?
    let expireCursorForQA: Bool?
    let uploadFaultForQA: String?
}

private struct Changes: Encodable {
    var uploaded: [String] = []
    var edits: [Edit] = []
    var token: Data?
    var more = false
    var error: String?
    var errorKind: String?
    var legacy = false
    var roomAccess: ShareAccess?
    var cursorReplayed = false
    var uploadQuotaExceeded = false
}

private func syncErrorKind(_ error: Error) -> String {
    if case SyncError.accountChecking = error { return "accountChecking" }
    if case CloudAccessError.required = error { return "subscriptionRequired" }
    if case SyncError.account = error { return "account" }
    if case SyncError.timeout = error { return "offline" }
    if case ShareError.account = error { return "accountChanged" }
    if case ShareError.signedOut = error { return "signedOut" }
    guard let error = error as? CKError else { return "other" }
    switch error.code {
    case .networkUnavailable, .networkFailure: return "offline"
    case .notAuthenticated: return "account"
    case .quotaExceeded: return "quota"
    default: return "other"
    }
}

private enum SyncError: LocalizedError {
    case account, accountChecking, invalid, timeout

    var errorDescription: String? {
        switch self {
        case .account: "CloudKit Apple account does not match the local test library."
        case .accountChecking: "The iCloud account status changed during this request."
        case .invalid: "CloudKit returned an invalid edit."
        case .timeout: "CloudKit request timed out."
        }
    }
}

private func push(_ edits: [Edit], to db: CKDatabase, in zone: CKRecordZone.ID,
                  uploadFaultForQA: String?, revision: UInt64) async throws -> (uploaded: [String], quotaExceeded: Bool) {
    guard !edits.isEmpty else { return ([], false) }
    let temp = FileManager.default.temporaryDirectory.appending(path: "Quiet-edits-\(UUID())")
    try FileManager.default.createDirectory(at: temp, withIntermediateDirectories: false)
    defer { try? FileManager.default.removeItem(at: temp) }
    let records = try edits.map { edit in
        try edit.validate()
        let file = temp.appending(path: edit.id)
        try edit.data.write(to: file)
        let record = CKRecord(recordType: editType, recordID: edit.recordID(in: zone))
        record["note"] = edit.note
        record["digest"] = edit.digest
        record["update"] = CKAsset(fileURL: file)
        return record
    }
    var saving = records
    #if FEI_CLOUD_QA
    let accepted = uploadFaultForQA == timeoutAfterUpload || uploadFaultForQA == holdAfterUpload ? nil : uploadFaultForQA
    if let accepted { saving = records.filter { $0.recordID.recordName == accepted } }
    #endif
    var results: [CKRecord.ID: Result<CKRecord, any Error>]
    do {
        try checkCloudAccountRevision(revision)
        results = saving.isEmpty ? [:]
            : try await db.modifyRecords(saving: saving, deleting: [], savePolicy: .ifServerRecordUnchanged, atomically: false).saveResults
    } catch let error as CKError where error.code == .quotaExceeded {
        return ([], true)
    }
    #if FEI_CLOUD_QA
    if let accepted {
        for record in records where record.recordID.recordName != accepted {
            results[record.recordID] = .failure(CKError(.quotaExceeded))
        }
    }
    #endif
    var done: [String] = []
    var quotaExceeded = false
    for edit in edits {
        try checkCloudAccountRevision(revision)
        let id = edit.recordID(in: zone)
        guard let result = results[id] else { throw SyncError.invalid }
        do { _ = try result.get() }
        catch let error as CKError where error.code == .quotaExceeded {
            quotaExceeded = true
            continue
        }
        catch let error as CKError where error.code == .serverRecordChanged {
            // An earlier request may have saved the record before the local acknowledgement.
            let existing: CKRecord
            if let server = error.serverRecord { existing = server }
            else { existing = try await db.record(for: id) }
            guard existing["note"] as? String == edit.note, existing["digest"] as? Data == edit.digest else {
                throw SyncError.invalid
            }
        }
        done.append(edit.id)
    }
    return (done, quotaExceeded)
}

private func exchange(_ request: Exchange, revision: UInt64) async throws -> Changes {
    #if !FEI_CLOUD_QA
    guard request.channel == nil, request.expireCursorForQA != true, request.uploadFaultForQA == nil else { throw SyncError.invalid }
    #else
    if let accepted = request.uploadFaultForQA {
        guard request.channel != nil, accepted == timeoutAfterUpload || accepted == holdAfterUpload || accepted == "all"
                || UUID(uuidString: accepted) != nil else { throw SyncError.invalid }
    }
    #endif
    let name = request.legacy ? legacyZoneName : syncZoneName
    let zoneName: String
    if let channel = request.channel {
        guard UUID(uuidString: channel) != nil else { throw SyncError.invalid }
        zoneName = name + "-QA-" + channel
    } else {
        zoneName = name
    }
    guard request.room == nil || (!request.legacy && request.uploadFaultForQA == nil && request.expireCursorForQA != true),
          request.activityOnly != true || (request.room != nil && request.edits.isEmpty) else { throw SyncError.invalid }
    let syncZone = try request.room?.zone() ?? CKRecordZone.ID(zoneName: zoneName)
    let container = CKContainer(identifier: containerID)
    try await shareAccount(container, expected: request.account)
    try await CloudAccess.shared.require(account: request.account, channel: request.channel)
    try checkCloudAccountRevision(revision)
    let db = request.room.map { shareDatabase(container, $0) } ?? container.privateCloudDatabase
    var access: ShareAccess?
    if let room = request.room {
        access = try await shareAccess(db, room, in: syncZone)
        if access == .revoked { return Changes(roomAccess: .revoked) }
        guard request.edits.allSatisfy({ $0.note == room.note || $0.note == sharedAssetScope }) else {
            throw SyncError.invalid
        }
    }
    // The token is an opaque cache. Replay the zone if it can't be decoded.
    var token = request.token.flatMap { try? NSKeyedUnarchiver.unarchivedObject(ofClass: CKServerChangeToken.self, from: $0) }
    if token == nil && request.room == nil {
        do { _ = try await db.recordZone(for: syncZone) }
        catch let error as CKError where error.code == .zoneNotFound || error.code == .unknownItem {
            if request.legacy { return Changes(legacy: true) }
            try checkCloudAccountRevision(revision)
            _ = try await db.save(CKRecordZone(zoneID: syncZone))
        }
    }
    var uploaded: [String] = []
    var uploadQuotaExceeded = false
    if !request.legacy && access != .readOnly {
        try checkCloudAccountRevision(revision)
        do { (uploaded, uploadQuotaExceeded) = try await push(request.edits, to: db, in: syncZone,
            uploadFaultForQA: request.uploadFaultForQA, revision: revision) }
        catch {
            guard let room = request.room, shareIsGone(error) else { throw error }
            // The owner may change permissions while the upload is in flight.
            access = try await shareAccess(db, room, in: syncZone)
            if access == .revoked { return Changes(roomAccess: .revoked) }
            guard access == .readOnly else { throw error }
        }
    }
    #if FEI_CLOUD_QA
    if request.uploadFaultForQA == timeoutAfterUpload && !uploaded.isEmpty {
        try await Task.sleep(for: .seconds(60))
    }
    if request.uploadFaultForQA == holdAfterUpload && !uploaded.isEmpty {
        FileHandle.standardError.write(Data("CloudKit QA holding upload acknowledgement\n".utf8))
        try await Task.sleep(for: .seconds(7))
    }
    #endif
    var expireCursor = request.expireCursorForQA == true && request.channel != nil
    var replayed = false
    while true {
        do {
            try checkCloudAccountRevision(revision)
            if expireCursor && token != nil {
                expireCursor = false
                throw CKError(.changeTokenExpired)
            }
            let activityOnly = request.activityOnly == true
            let page = try await db.recordZoneChanges(inZoneWith: syncZone, since: token,
                desiredKeys: activityOnly ? ["note"] : nil, resultsLimit: 20)
            var edits: [Edit] = []
            for item in page.modificationResultsByID.values {
                try checkCloudAccountRevision(revision)
                let metadata = try item.get().record
                if request.room != nil && (metadata.recordType == CKRecord.SystemType.share || metadata.recordType == "QuietPresence") { continue }
                let collaborator = request.room != nil && (metadata.creatorUserRecordID.map { creator in
                    // CloudKit can identify the current user by an alias in their private database.
                    if creator.recordName == CKCurrentUserDefaultName { return false }
                    let key = SHA256.hash(data: Data(creator.recordName.utf8)).map { String(format: "%02x", $0) }.joined()
                    return key != request.account
                } ?? false)
                if activityOnly && (!collaborator || metadata["note"] as? String != request.room?.note) { continue }
                let record = activityOnly ? try await db.record(for: metadata.recordID) : metadata
                guard record.recordType == editType, let note = record["note"] as? String,
                      let file = (record["update"] as? CKAsset)?.fileURL,
                      let size = try file.resourceValues(forKeys: [.fileSizeKey]).fileSize, size <= maxUpdate else {
                    throw SyncError.invalid
                }
                let edit = Edit(id: record.recordID.recordName, note: note, data: try Data(contentsOf: file), collaborator: collaborator)
                try edit.validate()
                if let room = request.room, note != room.note && note != sharedAssetScope { throw SyncError.invalid }
                guard record["digest"] as? Data == edit.digest else { throw SyncError.invalid }
                edits.append(edit)
            }
            let next = try NSKeyedArchiver.archivedData(withRootObject: page.changeToken, requiringSecureCoding: true)
            return Changes(uploaded: uploaded, edits: edits, token: next, more: page.moreComing,
                           legacy: request.legacy, roomAccess: access, cursorReplayed: replayed,
                           uploadQuotaExceeded: uploadQuotaExceeded)
        } catch let error as CKError where error.code == .changeTokenExpired && token != nil {
            token = nil // Loro imports are idempotent, so replay is safe.
            replayed = true
        } catch let error as CKError where request.legacy && (error.code == .zoneNotFound || error.code == .unknownItem) {
            return Changes(legacy: true)
        } catch where request.room != nil && shareIsGone(error) {
            return Changes(roomAccess: .revoked)
        }
    }
}

private final class SyncCompletion: @unchecked Sendable {
    let done = DispatchSemaphore(value: 0)
    var reply = Changes()
}

@_cdecl("quiet_cloud_exchange")
public func quietCloudExchange(_ json: UnsafePointer<CChar>) -> UnsafeMutablePointer<CChar>? {
    let input = Data(String(cString: json).utf8)
    let request = Result { try JSONDecoder().decode(Exchange.self, from: input) }
    let completion = SyncCompletion()
    let revision = quietCloudAccountRevision()
    let worker = Task.detached {
        do {
            let reply = try await exchange(request.get(), revision: revision)
            try checkCloudAccountRevision(revision)
            completion.reply = reply
        } catch {
            let failure: Error = quietCloudAccountRevision() == revision ? error : SyncError.accountChecking
            completion.reply = Changes(error: failure.localizedDescription, errorKind: syncErrorKind(failure))
        }
        completion.done.signal()
    }
    #if FEI_CLOUD_QA
    let timeout = (try? request.get())?.uploadFaultForQA == timeoutAfterUpload ? 8.0 : 45.0
    #else
    let timeout = 45.0
    #endif
    let reply: Changes
    if completion.done.wait(timeout: .now() + timeout) == .success {
        reply = completion.reply
    } else {
        worker.cancel()
        reply = Changes(error: SyncError.timeout.localizedDescription, errorKind: syncErrorKind(SyncError.timeout))
    }
    guard let bytes = try? JSONEncoder().encode(reply) else { return nil }
    return strdup(String(decoding: bytes, as: UTF8.self))
}

private struct PresenceRequest: Decodable {
    let account: String
    let channel: String?
    let peer: String
    let token: Data?
    let state: Data?
    let room: ShareScope?
}

private struct PresenceEntry: Encodable {
    let peer: String
    let name: String
    let state: Data
    let age: Double
    let identity: String?
    let displayName: String?
}

private struct PresencePage: Encodable {
    var devices: [PresenceEntry] = []
    var removed: [String] = []
    var token: Data?
    var more = false
    var replay = false
    var editableIdentities: [String]? = nil
    var error: String?
}

private struct PresenceMember: Sendable {
    let identity: String
    let displayName: String?
    let canEdit: Bool
}

private actor PresenceMembers {
    static let shared = PresenceMembers()
    private var cached: (zone: CKRecordZone.ID, revision: UInt64, until: ContinuousClock.Instant, members: [String: PresenceMember])?

    func fetch(_ db: CKDatabase, zone: CKRecordZone.ID, account: String, revision: UInt64, refresh: Bool) async throws -> [String: PresenceMember] {
        let clock = ContinuousClock()
        if !refresh, let cached, cached.zone == zone, cached.revision == revision, cached.until > clock.now { return cached.members }
        let share = try await getShare(db, in: zone)
        try checkCloudAccountRevision(revision)
        var members: [String: PresenceMember] = [:]
        for participant in share.participants where participant.acceptanceStatus == .accepted || participant.role == .owner {
            let current = share.currentUserParticipant?.participantID == participant.participantID
            guard let record = current ? account : participant.userIdentity.userRecordID?.recordName else { continue }
            let key = Data((zone.zoneName + ":" + participant.participantID).utf8)
            let identity = SHA256.hash(data: key).prefix(8).map { String(format: "%02x", $0) }.joined()
            let displayName = participant.userIdentity.nameComponents.map { PersonNameComponentsFormatter().string(from: $0) }
            members[record] = PresenceMember(identity: identity, displayName: displayName,
                canEdit: participant.role == .owner || participant.permission == .readWrite)
        }
        cached = (zone, revision, clock.now.advanced(by: .seconds(15)), members)
        return members
    }
}

private func presence(_ request: PresenceRequest, revision: UInt64) async throws -> PresencePage {
    let container = CKContainer(identifier: containerID)
    #if !FEI_CLOUD_QA
    guard request.channel == nil else { throw SyncError.invalid }
    #endif
    let account = try await container.userRecordID().recordName
    let key = SHA256.hash(data: Data(account.utf8)).map { String(format: "%02x", $0) }.joined()
    guard request.account == key else { throw SyncError.account }
    guard UInt64(request.peer) != nil, request.channel.map({ UUID(uuidString: $0) != nil }) ?? true,
          request.state.map({ $0.count <= 4096 }) ?? true else { throw SyncError.invalid }
    try await CloudAccess.shared.require(account: request.account, channel: request.channel)
    try checkCloudAccountRevision(revision)
    let zone = try request.room?.zone() ?? CKRecordZone.ID(zoneName: "QuietPresence-v1" + (request.channel.map { "-QA-" + $0 } ?? ""))
    let db = request.room.map { shareDatabase(container, $0) } ?? container.privateCloudDatabase
    var token = request.token.flatMap { try? NSKeyedUnarchiver.unarchivedObject(ofClass: CKServerChangeToken.self, from: $0) }
    if token == nil && request.room == nil {
        do { _ = try await db.recordZone(for: zone) }
        catch let error as CKError where error.code == .zoneNotFound || error.code == .unknownItem {
            try checkCloudAccountRevision(revision)
            _ = try await db.save(CKRecordZone(zoneID: zone))
        }
    }
    if let state = request.state {
        try checkCloudAccountRevision(revision)
        let id = CKRecord.ID(recordName: request.peer, zoneID: zone)
        let record = CKRecord(recordType: "QuietPresence", recordID: id)
        record["state"] = state
        record["name"] = request.room == nil ? String((Host.current().localizedName ?? "Mac").prefix(48)) : "Collaborator"
        do {
            let response = try await db.modifyRecords(saving: [record], deleting: [], savePolicy: .allKeys, atomically: true)
            guard let saved = response.saveResults[id] else { throw SyncError.invalid }
            _ = try saved.get()
        } catch where request.room != nil && shareIsGone(error) {
            // The account may still be able to read after a permission change.
        }
    }
    while true {
        do {
            try checkCloudAccountRevision(revision)
            let page = try await db.recordZoneChanges(inZoneWith: zone, since: token, desiredKeys: ["state", "name"], resultsLimit: 100)
            let refresh = page.modificationResultsByID.keys.contains { $0.recordName == CKRecordNameZoneWideShare }
            let hasPresence = page.modificationResultsByID.keys.contains { UInt64($0.recordName) != nil }
            var members: [String: PresenceMember]?
            if request.room != nil && (hasPresence || refresh) {
                members = try? await PresenceMembers.shared.fetch(db, zone: zone, account: account, revision: revision, refresh: refresh)
                try checkCloudAccountRevision(revision)
            }
            var devices: [PresenceEntry] = []
            var removed = page.deletions.map { $0.recordID.recordName }
            for entry in page.modificationResultsByID.values {
                let record = try entry.get().record
                guard record.recordType == "QuietPresence", let state = record["state"] as? Data, state.count <= 4096,
                      let name = record["name"] as? String, let date = record.modificationDate,
                      UInt64(record.recordID.recordName) != nil else { continue }
                var member: PresenceMember?
                if request.room != nil {
                    func author(_ id: CKRecord.ID?) -> String? {
                        id?.recordName == CKCurrentUserDefaultName ? account : id?.recordName
                    }
                    let creator = author(record.creatorUserRecordID)
                    let modifier = author(record.lastModifiedUserRecordID)
                    if let creator, let modifier, creator != modifier {
                        removed.append(record.recordID.recordName)
                        continue
                    }
                    if let creator, creator == modifier { member = members?[creator] }
                    if member?.canEdit == false {
                        removed.append(record.recordID.recordName)
                        continue
                    }
                }
                devices.append(PresenceEntry(peer: record.recordID.recordName, name: request.room == nil ? String(name.prefix(48)) : "Collaborator", state: state,
                                             age: max(0, -date.timeIntervalSinceNow), identity: member?.identity, displayName: member?.displayName))
            }
            let next = try NSKeyedArchiver.archivedData(withRootObject: page.changeToken, requiringSecureCoding: true)
            return PresencePage(devices: devices, removed: removed, token: next,
                                more: page.moreComing, replay: token == nil,
                                editableIdentities: refresh ? members?.values.filter(\.canEdit).map(\.identity) : nil)
        } catch let error as CKError where error.code == .changeTokenExpired && token != nil {
            token = nil
        } catch where request.room != nil && shareIsGone(error) {
            return PresencePage(replay: true)
        }
    }
}

private final class PresenceCompletion: @unchecked Sendable {
    let done = DispatchSemaphore(value: 0)
    var reply = PresencePage()
}

@_cdecl("quiet_cloud_presence")
public func quietCloudPresence(_ json: UnsafePointer<CChar>) -> UnsafeMutablePointer<CChar>? {
    let input = Data(String(cString: json).utf8)
    let completion = PresenceCompletion()
    let revision = quietCloudAccountRevision()
    let worker = Task.detached {
        do {
            let reply = try await presence(JSONDecoder().decode(PresenceRequest.self, from: input), revision: revision)
            try checkCloudAccountRevision(revision)
            completion.reply = reply
        } catch {
            completion.reply = PresencePage(error: error.localizedDescription)
        }
        completion.done.signal()
    }
    let reply: PresencePage
    if completion.done.wait(timeout: .now() + 15) == .success { reply = completion.reply }
    else {
        worker.cancel()
        reply = PresencePage(error: SyncError.timeout.localizedDescription)
    }
    guard let bytes = try? JSONEncoder().encode(reply) else { return nil }
    return strdup(String(decoding: bytes, as: UTF8.self))
}

@MainActor private var previewPanel: NSPanel?

@MainActor
@_cdecl("quiet_cloud_preview")
public func quietCloudPreview(_ path: UnsafePointer<CChar>) -> Bool {
    let url = URL(fileURLWithPath: String(cString: path))
    guard FileManager.default.isReadableFile(atPath: url.path) else { return false }
    let frame = NSRect(x: 0, y: 0, width: 680, height: 460)
    let content: NSView
    if url.pathExtension.lowercased() == "txt",
       let size = try? url.resourceValues(forKeys: [.fileSizeKey]).fileSize, size <= 1024 * 1024,
       let text = try? String(contentsOf: url, encoding: .utf8) {
        let view = NSTextView(frame: frame)
        view.isEditable = false
        view.isVerticallyResizable = true
        view.autoresizingMask = .width
        view.textContainerInset = NSSize(width: 18, height: 18)
        view.font = .systemFont(ofSize: 15)
        view.string = text
        view.setAccessibilityLabel(text)
        let scroll = NSScrollView(frame: frame)
        scroll.hasVerticalScroller = true
        scroll.documentView = view
        content = scroll
    } else {
        guard let preview = QLPreviewView(frame: frame, style: .normal) else { return false }
        preview.autostarts = true
        preview.previewItem = url as NSURL
        content = preview
    }
    previewPanel?.close()
    let panel = NSPanel(contentRect: frame, styleMask: [.titled, .closable, .resizable], backing: .buffered, defer: false)
    panel.isReleasedWhenClosed = false
    panel.title = url.lastPathComponent
    panel.contentView = content
    panel.center()
    panel.makeKeyAndOrderFront(nil)
    previewPanel = panel
    return true
}
