// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Copyright 2026 Fei Notes contributors. See LICENSES.md.
import CloudKit
import CryptoKit
import Foundation

private let sharePrefix = "QuietSharedNote-v1-"
private let folderPrefix = "QuietSharedFolder-v1-"
private let linkDraftType = "app.feinotes.document-link.draft"
private let linkPublishedType = "app.feinotes.document-link.published"
#if FEI_CLOUD_QA
private let qaShareType = "app.feinotes.qa-share."
#endif
let sharedAssetScope = "00000000-0000-0000-0000-000000000002"

struct ShareScope: Codable, Equatable {
    let note: String
    let owner: String?

    var folder: Bool? = nil

    func permits(_ id: String) -> Bool {
        id == note || id == sharedAssetScope || (folder == true && UUID(uuidString: id)?.uuidString.lowercased() == id)
    }

    func zone() throws -> CKRecordZone.ID {
        guard let id = UUID(uuidString: note), id.uuidString.lowercased() == note,
              owner.map({ !$0.isEmpty && $0.utf8.count <= 256 && $0 != CKCurrentUserDefaultName }) ?? true
        else { throw ShareError.invalid }
        return CKRecordZone.ID(zoneName: (folder == true ? folderPrefix : sharePrefix) + note, ownerName: owner ?? CKCurrentUserDefaultName)
    }
}

enum ShareAccess: String, Codable { case owner, readWrite, readOnly, revoked }

private enum ShareAudience: String, Encodable {
    case invited, linkDraft, linkReadOnly, linkReadWrite, linkClosed

    init(_ share: CKShare) {
        if share.publicPermission == .readOnly { self = .linkReadOnly }
        else if share.publicPermission == .readWrite { self = .linkReadWrite }
        else if share.publicPermission != .none { self = .linkClosed }
        else if share[CKShare.SystemFieldKey.shareType] as? String == linkDraftType { self = .linkDraft }
        else if share[CKShare.SystemFieldKey.shareType] as? String == linkPublishedType { self = .linkClosed }
        else { self = .invited }
    }
}

enum ShareError: LocalizedError {
    case invalid, account, signedOut, ownerOnly, timeout, unavailableLink, deniedLink, invitationOnly, linkOnly, refresh
    case linkRegionRestricted, linkWriteRejected, linkWriteUnconfirmed

    var errorDescription: String? {
        switch self {
        case .invalid: "This is not a valid shared document."
        case .account: "The iCloud account does not match this library."
        case .signedOut: "Sign in to iCloud in System Settings, then reconnect Fei Notes."
        case .ownerOnly: "Only the owner can manage this invitation."
        case .timeout: "CloudKit sharing did not respond. Try again."
        case .unavailableLink: "This share link is invalid or has been closed."
        case .deniedLink: "This account cannot use this link. It may have been closed or sent to a different Apple Account."
        case .invitationOnly: "This document uses account invitations. Its existing access cannot be changed to a public link."
        case .linkOnly: "This document uses link access. Individual invitations and permissions are unavailable."
        case .refresh: "Sharing permissions changed on another device. Refresh and try again."
        case .linkRegionRestricted: "iCloud reported a regional restriction on the public link."
        case .linkWriteRejected: "iCloud rejected the public link change."
        case .linkWriteUnconfirmed: "Could not confirm the public link settings with iCloud."
        }
    }
}

func shareAccount(_ container: CKContainer, expected: String) async throws {
    let account: String
    do { account = try await container.userRecordID().recordName }
    catch let error as CKError where error.code == .notAuthenticated {
        let status = try? await container.accountStatus()
        if status == .noAccount { throw ShareError.signedOut }
        throw error
    }
    let hash = SHA256.hash(data: Data(account.utf8)).map { String(format: "%02x", $0) }.joined()
    guard hash == expected else { throw ShareError.account }
}

func shareDatabase(_ container: CKContainer, _ scope: ShareScope) -> CKDatabase {
    scope.owner == nil ? container.privateCloudDatabase : container.sharedCloudDatabase
}

private func shareID(in zone: CKRecordZone.ID) -> CKRecord.ID {
    CKRecord.ID(recordName: CKRecordNameZoneWideShare, zoneID: zone)
}

func shareIsGone(_ error: Error) -> Bool {
    guard let error = error as? CKError else { return false }
    switch error.code {
    case .unknownItem, .zoneNotFound, .permissionFailure, .userDeletedZone: return true
    default: return false
    }
}

func getShare(_ db: CKDatabase, in zone: CKRecordZone.ID) async throws -> CKShare {
    guard let share = try await db.record(for: shareID(in: zone)) as? CKShare else { throw ShareError.invalid }
    return share
}

func shareAccess(_ db: CKDatabase, _ scope: ShareScope, in zone: CKRecordZone.ID) async throws -> ShareAccess {
    let share: CKShare
    do { share = try await getShare(db, in: zone) }
    catch where shareIsGone(error) { return .revoked }
    if scope.owner == nil { return .owner }
    return participantAccess(share)
}

private func participantAccess(_ share: CKShare) -> ShareAccess {
    guard let member = share.currentUserParticipant, member.acceptanceStatus == .accepted else { return .revoked }
    if member.role == .publicUser {
        if share.publicPermission == .none { return .revoked }
        if share.publicPermission == .readOnly { return .readOnly }
    }
    switch member.permission {
    case .readWrite: return .readWrite
    case .readOnly: return .readOnly
    default: return .revoked
    }
}

private struct ShareRequest: Decodable {
    let account: String
    let action: String
    let scope: ShareScope?
    let title: String?
    let email: String?
    let permission: ShareAccess?
    let participant: String?
    let url: String?
    let accessChannel: String?
    let knownOwned: [String]?
    let trace: ShareTraceTag?
}

// Traces are selected by exact synthetic sample title in Rust and require a
// fresh launch token. Keep this file separate from general app logging.
private struct ShareTraceTag: Decodable, Sendable {
    enum Sample: String, Decodable, Sendable { case publicLink, privateInvite }
    let sample: Sample
    let attempt: UUID
}

private struct ShareTrace: Sendable {
    private static let lock = NSLock()
    private let tag: ShareTraceTag
    private let action: String
    private let file: URL

    init?(_ request: ShareRequest) {
        guard let tag = request.trace, ["create", "createLinkDraft", "publishLink", "members", "invite"].contains(request.action),
              let token = ProcessInfo.processInfo.environment["FEI_SHARE_TRACE_TOKEN"], let session = UUID(uuidString: token)
        else { return nil }
        self.tag = tag
        self.action = request.action
        self.file = FileManager.default.temporaryDirectory.appendingPathComponent("fei-share-trace-\(session.uuidString.lowercased()).ndjson")
    }

    func event(_ phase: String, _ status: String, share: CKShare? = nil, audience: ShareAudience? = nil,
               error: Error? = nil, record: CKRecord.ID? = nil, boundary: String? = nil, errorKind: String? = nil) {
        var event: [String: Any] = [
            "schema": 1, "timeMs": Int64(Date().timeIntervalSince1970 * 1000), "attempt": tag.attempt.uuidString,
            "sample": tag.sample.rawValue, "action": action, "phase": phase, "status": status,
            "container": containerID, "environment": cloudEnvironment, "database": "ownerPrivate",
        ]
        if let build = Bundle.main.object(forInfoDictionaryKey: "CFBundleVersion") as? String,
           build.count <= 20, build.utf8.allSatisfy({ (48...57).contains($0) || $0 == 46 }) {
            event["build"] = build
        }
        if let share {
            event["audience"] = ShareAudience(share).rawValue
            event["invitationHostClass"] = switch share.url?.host?.lowercased() {
            case "www.icloud.com.cn", "icloud.com.cn": "icloudCN"
            case "www.icloud.com", "icloud.com": "icloudGlobal"
            case nil: "absent"
            default: "other"
            }
        } else if let audience { event["audience"] = audience.rawValue }
        if let boundary { event["boundary"] = boundary }
        if let errorKind { event["errorKind"] = errorKind }
        if let error {
            let top = error as NSError
            var errors = [Self.errorFields(top, layer: "outer")]
            if let underlying = top.userInfo[NSUnderlyingErrorKey] as? NSError {
                errors.append(Self.errorFields(underlying, layer: "outerUnderlying"))
            }
            if let record, let item = (error as? CKError)?.partialErrorsByItemID?[AnyHashable(record)] {
                let nested = item as NSError
                errors.append(Self.errorFields(nested, layer: "targetRecord"))
                if let underlying = nested.userInfo[NSUnderlyingErrorKey] as? NSError {
                    errors.append(Self.errorFields(underlying, layer: "targetUnderlying"))
                }
            }
            event["errors"] = errors
        }
        guard var line = try? JSONSerialization.data(withJSONObject: event, options: [.sortedKeys]) else { return }
        line.append(10)
        Self.lock.withLock {
            if !FileManager.default.fileExists(atPath: file.path) {
                _ = FileManager.default.createFile(atPath: file.path, contents: nil, attributes: [.posixPermissions: 0o600])
            }
            guard let handle = try? FileHandle(forWritingTo: file) else { return }
            defer { try? handle.close() }
            do {
                guard try handle.seekToEnd() + UInt64(line.count) <= 128 * 1024 else { return }
                try handle.write(contentsOf: line)
            } catch { }
        }
    }

    private static func errorFields(_ error: NSError, layer: String) -> [String: Any] {
        let allowedDomains = [CKErrorDomain, "CKInternalErrorDomain", "CloudKitErrorDomain", NSURLErrorDomain, NSCocoaErrorDomain]
        var fields: [String: Any] = ["layer": layer, "domain": allowedDomains.contains(error.domain) ? error.domain : "other",
                                     "code": error.code]
        guard error.domain == CKErrorDomain || error.domain == "CKInternalErrorDomain" || error.domain == "CloudKitErrorDomain"
        else { return fields }
        let messages = [error.localizedDescription, error.localizedFailureReason ?? ""]
        fields["regionRejectionText"] = messages.contains { $0.localizedCaseInsensitiveContains("Cannot perform action due to region") }
        for message in messages {
            if fields["requestUUID"] == nil,
               let match = message.firstMatch(of: #/\bRequestUUID\s*[:=]\s*([0-9a-fA-F]{8}(?:-[0-9a-fA-F]{4}){3}-[0-9a-fA-F]{12})\b/#),
               let uuid = UUID(uuidString: String(match.1)) {
                fields["requestUUID"] = uuid.uuidString
            }
            if fields["operationID"] == nil, let match = message.firstMatch(of: #/\bOperationID\s*[:=]\s*([0-9a-fA-F]{16})\b/#) {
                fields["operationID"] = String(match.1).uppercased()
            }
        }
        return fields
    }
}

private struct ShareMember: Encodable {
    let id: String
    let name: String
    let permission: ShareAccess
    let accepted: Bool
}

private struct ShareReply: Encodable {
    var scope: ShareScope?
    var title: String?
    var url: String?
    var audience: ShareAudience?
    var members: [ShareMember] = []
    var error: String?
    var errorKind: String?
    var discoveredRooms: [DiscoveredRoom]?
    var ownerName: String?
    var verifiedAccess: ShareAccess?
}

private struct DiscoveredRoom: Encodable {
    let scope: ShareScope
    let title: String
    let ownerName: String?
}

private func discoverRooms(_ db: CKDatabase, joined: Bool, known: Set<String>, channel: String?, revision: UInt64) async throws -> [DiscoveredRoom] {
    let zones = try await db.allRecordZones()
    let targets = zones.compactMap { zone -> (CKRecord.ID, ShareScope)? in
        let name = zone.zoneID.zoneName
        let folder = name.hasPrefix(folderPrefix)
        let prefix = folder ? folderPrefix : sharePrefix
        guard name.hasPrefix(prefix) else { return nil }
        let id = String(name.dropFirst(prefix.count))
        guard !known.contains(id), UUID(uuidString: id)?.uuidString.lowercased() == id else { return nil }
        let scope = ShareScope(note: id, owner: joined ? zone.zoneID.ownerName : nil, folder: folder ? true : nil)
        guard !joined || (try? scope.zone()) == zone.zoneID else { return nil }
        return (shareID(in: zone.zoneID), scope)
    }
    var rooms: [DiscoveredRoom] = []
    for offset in stride(from: 0, to: targets.count, by: 50) {
        try checkCloudAccountRevision(revision)
        let batch = targets[offset..<min(offset + 50, targets.count)]
        let records = try await db.records(for: batch.map { $0.0 },
            desiredKeys: [CKShare.SystemFieldKey.title, CKShare.SystemFieldKey.shareType])
        for (id, scope) in batch {
            guard let result = records[id] else { throw ShareError.invalid }
            do {
                guard let share = try result.get() as? CKShare else { continue }
                if joined && participantAccess(share) == .revoked { continue }
                #if FEI_CLOUD_QA
                if let channel, share[CKShare.SystemFieldKey.shareType] as? String != qaShareType + channel { continue }
                #endif
                rooms.append(DiscoveredRoom(scope: scope, title: share[CKShare.SystemFieldKey.title] as? String ?? "",
                    ownerName: joined ? cloudShareDisplayName(share.owner.userIdentity) : nil))
            } catch where shareIsGone(error) { continue }
        }
    }
    return rooms
}

func cloudShareDisplayName(_ identity: CKUserIdentity) -> String? {
    guard let components = identity.nameComponents else { return nil }
    let name = PersonNameComponentsFormatter().string(from: components).trimmingCharacters(in: .whitespacesAndNewlines)
    return name.isEmpty ? nil : name
}

func cloudShareDisplayNames(_ share: CKShare) -> [String: String] {
    share.participants.reduce(into: [:]) { result, member in
        result[member.participantID] = cloudShareDisplayName(member.userIdentity)
    }
}

private func shareReply(_ share: CKShare, scope: ShareScope, revision: UInt64) async -> ShareReply {
    let names = cloudShareDisplayNames(share)
    await PresenceMembers.shared.rememberNames(names, zone: share.recordID.zoneID, revision: revision)
    let members: [ShareMember] = share.participants.compactMap { member in
        guard member.role != .owner else { return nil }
        let id = member.participantID
        let name = names[id] ?? member.userIdentity.lookupInfo?.emailAddress ?? ""
        return ShareMember(id: id, name: name, permission: member.permission == .readWrite ? .readWrite : .readOnly,
                           accepted: member.acceptanceStatus == .accepted)
    }
    let audience = ShareAudience(share)
    let available = audience != .linkDraft && audience != .linkClosed
    return ShareReply(scope: scope, title: share[CKShare.SystemFieldKey.title] as? String,
                      url: available ? share.url?.absoluteString : nil, audience: audience, members: members)
}

private func readManagedShare(_ db: CKDatabase, in zone: CKRecordZone.ID, trace: ShareTrace?, phase: String) async throws -> CKShare {
    trace?.event(phase, "started")
    do {
        let share = try await getShare(db, in: zone)
        trace?.event(phase, "ok", share: share)
        return share
    } catch {
        trace?.event(phase, "error", error: error, record: shareID(in: zone))
        throw error
    }
}

private func saveShare(_ share: CKShare, to db: CKDatabase, trace: ShareTrace?, phase: String) async throws -> CKShare {
    trace?.event(phase, "started", share: share)
    var boundary = "modifyRecords"
    do {
        let result = try await db.modifyRecords(saving: [share], deleting: [], savePolicy: .ifServerRecordUnchanged, atomically: true)
        boundary = "recordResult"
        guard let saved = try result.saveResults[share.recordID]?.get() as? CKShare else { throw ShareError.invalid }
        trace?.event(phase, "ok", share: saved, boundary: boundary)
        return saved
    } catch {
        trace?.event(phase, "error", error: error, record: share.recordID, boundary: boundary)
        if let cloud = error as? CKError, cloud.code == .serverRecordChanged { throw ShareError.refresh }
        throw error
    }
}

private func publicLinkWriteError(_ error: Error, id: CKRecord.ID) -> ShareError {
    if case ShareError.refresh = error { return .refresh }
    let item = (error as? CKError)?.partialErrorsByItemID?[AnyHashable(id)] ?? error
    // CloudKit's public error codes have no region-specific case. Use the
    // observed server message only to explain this failed share write.
    if let cloudError = item as? CKError {
        let underlying = (item as NSError).userInfo[NSUnderlyingErrorKey] as? Error
        if [item.localizedDescription, underlying?.localizedDescription ?? ""]
            .contains(where: { $0.localizedCaseInsensitiveContains("Cannot perform action due to region") }) {
            return .linkRegionRestricted
        }
        switch cloudError.code {
        case .serverRecordChanged:
            return .refresh
        case .serverRejectedRequest, .permissionFailure, .managedAccountRestricted, .invalidArguments, .batchRequestFailed:
            return .linkWriteRejected
        default: break
        }
    }
    return .linkWriteUnconfirmed
}

private func acceptShare(_ value: String, container: CKContainer, revision: UInt64) async throws -> ShareReply {
    guard let url = URL(string: value), url.scheme == "https", url.user == nil, url.password == nil,
          ["www.icloud.com", "icloud.com", "www.icloud.com.cn", "icloud.com.cn"].contains(url.host?.lowercased()),
          url.path.hasPrefix("/share/") else { throw ShareError.invalid }
    let metadata = try await container.shareMetadata(for: url)
    let zone = metadata.share.recordID.zoneID
    let folder = zone.zoneName.hasPrefix(folderPrefix)
    let prefix = folder ? folderPrefix : sharePrefix
    guard metadata.containerIdentifier == containerID, zone.zoneName.hasPrefix(prefix),
          metadata.share.recordID.recordName == CKRecordNameZoneWideShare else { throw ShareError.invalid }
    let scope = ShareScope(note: String(zone.zoneName.dropFirst(prefix.count)),
                           owner: metadata.participantRole == .owner ? nil : zone.ownerName, folder: folder ? true : nil)
    _ = try scope.zone()
    let title = metadata.share[CKShare.SystemFieldKey.title] as? String
    if metadata.participantRole != .owner {
        let audience = ShareAudience(metadata.share)
        guard (audience != .linkDraft && audience != .linkClosed) || metadata.participantRole == .privateUser else {
            throw ShareError.unavailableLink
        }
    }
    let ownerName = scope.owner == nil ? nil : cloudShareDisplayName(metadata.ownerIdentity)
    let reply = ShareReply(scope: scope, title: title, ownerName: ownerName)
    if metadata.participantRole == .owner || metadata.participantStatus == .accepted { return reply }
    guard metadata.participantStatus == .pending else { throw ShareError.deniedLink }
    try checkCloudAccountRevision(revision)
    _ = try await container.accept(metadata)
    return reply
}

private func manageShare(_ request: ShareRequest, revision: UInt64, trace: ShareTrace?) async throws -> ShareReply {
    #if !FEI_CLOUD_QA
    guard request.accessChannel == nil else { throw ShareError.invalid }
    #endif
    let container = CKContainer(identifier: containerID)
    try await shareAccount(container, expected: request.account)
    // Existing participants can still be inspected or removed after access ends.
    if !["members", "revoke", "closeLink", "delete", "leave"].contains(request.action) {
        try await CloudAccess.shared.require(account: request.account, channel: request.accessChannel)
    }
    try checkCloudAccountRevision(revision)
    trace?.event("accountAndAccess", "ok")
    if request.action == "checkAccess" { return ShareReply() }
    if request.action == "checkRoomAccess" || request.action == "inspectRoomAccess" {
        guard let scope = request.scope else { throw ShareError.invalid }
        let access = try await shareAccess(shareDatabase(container, scope), scope, in: scope.zone())
        try checkCloudAccountRevision(revision)
        guard access != .revoked || request.action == "inspectRoomAccess" else { throw ShareError.deniedLink }
        return ShareReply(scope: scope, verifiedAccess: access)
    }
    if request.action == "discoverOwned" || request.action == "discoverJoined" {
        let joined = request.action == "discoverJoined"
        let db = joined ? container.sharedCloudDatabase : container.privateCloudDatabase
        return ShareReply(discoveredRooms: try await discoverRooms(db, joined: joined,
            known: Set(request.knownOwned ?? []), channel: request.accessChannel, revision: revision))
    }
    if request.action == "accept", let url = request.url {
        do { return try await acceptShare(url, container: container, revision: revision) }
        catch let error as CKError where error.code == .permissionFailure { throw ShareError.deniedLink }
        catch where shareIsGone(error) { throw ShareError.unavailableLink }
    }
    if request.action == "delete" || request.action == "leave" {
        guard let scope = request.scope,
              (request.action == "delete" && scope.owner == nil) || (request.action == "leave" && scope.owner != nil)
        else { throw ShareError.ownerOnly }
        let zone = try scope.zone()
        let db = shareDatabase(container, scope)
        try checkCloudAccountRevision(revision)
        do {
            if request.action == "delete" { _ = try await db.deleteRecordZone(withID: zone) }
            else { _ = try await db.deleteRecord(withID: shareID(in: zone)) }
        } catch let error as CKError where [.unknownItem, .zoneNotFound, .userDeletedZone].contains(error.code) { }
        return ShareReply(scope: scope)
    }
    guard let scope = request.scope, scope.owner == nil else { throw ShareError.ownerOnly }
    let zone = try scope.zone()
    let db = container.privateCloudDatabase
    if request.action == "create" || request.action == "createLinkDraft" {
        guard request.action == "create" || scope.folder != true else { throw ShareError.invalid }
        do {
            _ = try await db.recordZone(for: zone)
            trace?.event("draftZone", "reused")
        } catch where shareIsGone(error) {
            trace?.event("draftZoneLookup", "error", error: error)
            try checkCloudAccountRevision(revision)
            do {
                _ = try await db.save(CKRecordZone(zoneID: zone))
                trace?.event("draftZone", "ok")
            } catch {
                trace?.event("draftZone", "error", error: error)
                throw error
            }
        } catch {
            trace?.event("draftZoneLookup", "error", error: error)
            throw error
        }
        do { return await shareReply(try await readManagedShare(db, in: zone, trace: trace, phase: "draftRead"), scope: scope, revision: revision) }
        catch let error as CKError where error.code == .unknownItem {
            try checkCloudAccountRevision(revision)
            let share = CKShare(recordZoneID: zone)
            share.publicPermission = .none
            if request.action == "createLinkDraft" { share[CKShare.SystemFieldKey.shareType] = linkDraftType }
            share[CKShare.SystemFieldKey.title] = String((request.title ?? "Shared document").prefix(100))
            do { return await shareReply(try await saveShare(share, to: db, trace: trace, phase: "draftSave"), scope: scope, revision: revision) }
            catch where request.action == "createLinkDraft" { throw publicLinkWriteError(error, id: share.recordID) }
        }
    }
    if request.action == "publishLink" { trace?.event("clientSeedGate", "passed") }
    let share: CKShare
    do { share = try await readManagedShare(db, in: zone, trace: trace, phase: "currentShareRead") }
    catch where request.action == "publishLink" { throw ShareError.linkWriteUnconfirmed }
    let audience = ShareAudience(share)
    switch request.action {
    case "members":
        return await shareReply(share, scope: scope, revision: revision)
    case "publishLink":
        guard scope.folder != true else { throw ShareError.invalid }
        if share[CKShare.SystemFieldKey.shareType] as? String == linkPublishedType {
            // A retried publication can observe an earlier success or a subsequent closure.
            return await shareReply(share, scope: scope, revision: revision)
        }
        guard audience == .linkDraft, share.participants.allSatisfy({ $0.role == .owner }) else { throw ShareError.invitationOnly }
        let permission = request.permission ?? .readOnly
        guard permission == .readOnly || permission == .readWrite else { throw ShareError.invalid }
        share.publicPermission = permission == .readWrite ? .readWrite : .readOnly
        share[CKShare.SystemFieldKey.shareType] = linkPublishedType
    case "closeLink":
        guard scope.folder != true else { throw ShareError.invalid }
        if audience == .linkClosed { return await shareReply(share, scope: scope, revision: revision) }
        guard audience == .linkReadOnly || audience == .linkReadWrite else { throw ShareError.linkOnly }
        share.publicPermission = .none
        share[CKShare.SystemFieldKey.shareType] = linkPublishedType
    case "invite":
        guard audience == .invited || audience == .linkDraft else { throw ShareError.linkOnly }
        guard let email = request.email, email.utf8.count <= 254, email.contains("@"),
              let access = request.permission, access == .readOnly || access == .readWrite else { throw ShareError.invalid }
        let invited: CKShare.Participant
        do {
            invited = try await container.shareParticipant(forEmailAddress: email)
            trace?.event("inviteeLookup", "ok")
        } catch {
            trace?.event("inviteeLookup", "error", error: error)
            throw error
        }
        guard !invited.isEqual(share.owner) else { throw ShareError.ownerOnly }
        let member = share.participants.first { $0.isEqual(invited) } ?? invited
        member.permission = access == .readWrite ? .readWrite : .readOnly
        share.addParticipant(member)
        // The owner explicitly chose an account invitation. Keep the scope
        // private and record that choice only if the invitation save succeeds.
        if audience == .linkDraft { share[CKShare.SystemFieldKey.shareType] = nil }
        #if FEI_CLOUD_QA
        if let channel = request.accessChannel, share[CKShare.SystemFieldKey.shareType] == nil {
            share[CKShare.SystemFieldKey.shareType] = qaShareType + channel
        }
        #endif
    case "permission", "revoke":
        guard audience == .invited || audience == .linkDraft else { throw ShareError.linkOnly }
        guard let member = share.participants.first(where: {
            $0.role != .owner && $0.participantID == request.participant
        }) else { throw ShareError.invalid }
        if request.action == "revoke" { share.removeParticipant(member) }
        else {
            guard let access = request.permission, access == .readOnly || access == .readWrite else { throw ShareError.invalid }
            member.permission = access == .readWrite ? .readWrite : .readOnly
            share.addParticipant(member)
        }
    default: throw ShareError.invalid
    }
    try checkCloudAccountRevision(revision)
    if request.action == "publishLink" {
        do { _ = try await saveShare(share, to: db, trace: trace, phase: "publicPermissionSave") }
        catch { throw publicLinkWriteError(error, id: share.recordID) }
        try checkCloudAccountRevision(revision)
        do { return await shareReply(try await readManagedShare(db, in: zone, trace: trace, phase: "publicPermissionReadback"), scope: scope, revision: revision) }
        catch { throw ShareError.linkWriteUnconfirmed }
    }
    let saved = try await saveShare(share, to: db, trace: trace, phase: "privateInvitationSave")
    if request.action == "closeLink" {
        return await shareReply(try await getShare(db, in: zone), scope: scope, revision: revision)
    }
    return await shareReply(saved, scope: scope, revision: revision)
}

private final class ShareCompletion: @unchecked Sendable {
    let done = DispatchSemaphore(value: 0)
    var reply = ShareReply()
}

@_cdecl("quiet_cloud_share")
public func quietCloudShare(_ json: UnsafePointer<CChar>) -> UnsafeMutablePointer<CChar>? {
    let input = Data(String(cString: json).utf8)
    let request = Result { try JSONDecoder().decode(ShareRequest.self, from: input) }
    let trace = (try? request.get()).flatMap(ShareTrace.init)
    trace?.event("command", "started")
    let completion = ShareCompletion()
    let revision = quietCloudAccountRevision()
    let worker = Task.detached {
        do {
            let reply = try await manageShare(request.get(), revision: revision, trace: trace)
            try checkCloudAccountRevision(revision)
            completion.reply = reply
            trace?.event("command", "ok", audience: reply.audience)
        }
        catch {
            completion.reply = ShareReply(error: error.localizedDescription)
            if let sharing = error as? ShareError {
                switch sharing {
                case .unavailableLink: completion.reply.errorKind = "unavailableLink"
                case .deniedLink: completion.reply.errorKind = "deniedLink"
                case .signedOut: completion.reply.errorKind = "signedOut"
                case .account: completion.reply.errorKind = "accountMismatch"
                case .refresh: completion.reply.errorKind = "shareConflict"
                case .invitationOnly: completion.reply.errorKind = "invitationOnly"
                case .linkOnly: completion.reply.errorKind = "linkOnly"
                case .linkRegionRestricted: completion.reply.errorKind = "linkRegionRestricted"
                case .linkWriteRejected: completion.reply.errorKind = "linkWriteRejected"
                case .linkWriteUnconfirmed: completion.reply.errorKind = "linkWriteUnconfirmed"
                default: break
                }
            }
            if case CloudAccessError.required = error { completion.reply.errorKind = "subscriptionRequired" }
            if case CloudAccessError.testflightVerification = error { completion.reply.errorKind = "testflightVerification" }
            trace?.event("command", "error", error: error, errorKind: completion.reply.errorKind)
        }
        completion.done.signal()
    }
    let reply: ShareReply
    if completion.done.wait(timeout: .now() + 45) == .success { reply = completion.reply }
    else {
        worker.cancel()
        trace?.event("command", "timeout")
        reply = ShareReply(error: ShareError.timeout.localizedDescription, errorKind: "shareTimeout")
    }
    guard let bytes = try? JSONEncoder().encode(reply) else { return nil }
    return strdup(String(decoding: bytes, as: UTF8.self))
}
