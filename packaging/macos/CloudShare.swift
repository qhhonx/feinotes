// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Copyright 2026 Fei Notes contributors. See LICENSES.md.
import CloudKit
import CryptoKit
import Foundation

private let sharePrefix = "QuietSharedNote-v1-"
let sharedAssetScope = "00000000-0000-0000-0000-000000000002"

struct ShareScope: Codable, Equatable {
    let note: String
    let owner: String?

    func zone() throws -> CKRecordZone.ID {
        guard let id = UUID(uuidString: note), id.uuidString.lowercased() == note,
              owner.map({ !$0.isEmpty && $0.utf8.count <= 256 && $0 != CKCurrentUserDefaultName }) ?? true
        else { throw ShareError.invalid }
        return CKRecordZone.ID(zoneName: sharePrefix + note, ownerName: owner ?? CKCurrentUserDefaultName)
    }
}

enum ShareAccess: String, Codable { case owner, readWrite, readOnly, revoked }

enum ShareError: LocalizedError {
    case invalid, account, signedOut, ownerOnly, removed, timeout

    var errorDescription: String? {
        switch self {
        case .invalid: "This is not a valid shared note."
        case .account: "The iCloud account does not match this library."
        case .signedOut: "The Mac is signed out of iCloud or iCloud is restricted."
        case .ownerOnly: "Only the owner can manage this invitation."
        case .removed: "Access to this shared note has ended."
        case .timeout: "CloudKit sharing did not respond. Try again."
        }
    }
}

func shareAccount(_ container: CKContainer, expected: String) async throws {
    let account: String
    do { account = try await container.userRecordID().recordName }
    catch let error as CKError where error.code == .notAuthenticated {
        let status = try? await container.accountStatus()
        if status == .noAccount || status == .restricted { throw ShareError.signedOut }
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
    guard let member = share.currentUserParticipant, member.acceptanceStatus == .accepted else { return .revoked }
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
    var members: [ShareMember] = []
    var error: String?
    var errorKind: String?
}

private func shareReply(_ share: CKShare, scope: ShareScope) -> ShareReply {
    let members: [ShareMember] = share.participants.compactMap { member in
        guard member.role != .owner else { return nil }
        let id = member.participantID
        let name = member.userIdentity.lookupInfo?.emailAddress
            ?? member.userIdentity.nameComponents.map { PersonNameComponentsFormatter().string(from: $0) } ?? id
        return ShareMember(id: id, name: name, permission: member.permission == .readWrite ? .readWrite : .readOnly,
                           accepted: member.acceptanceStatus == .accepted)
    }
    return ShareReply(scope: scope, title: share[CKShare.SystemFieldKey.title] as? String,
                      url: share.url?.absoluteString, members: members)
}

private func saveShare(_ share: CKShare, to db: CKDatabase) async throws -> CKShare {
    let result = try await db.modifyRecords(saving: [share], deleting: [], savePolicy: .ifServerRecordUnchanged, atomically: true)
    guard let saved = try result.saveResults[share.recordID]?.get() as? CKShare else { throw ShareError.invalid }
    return saved
}

private func acceptShare(_ value: String, container: CKContainer, revision: UInt64) async throws -> ShareReply {
    guard let url = URL(string: value), url.scheme == "https", url.user == nil, url.password == nil,
          ["www.icloud.com", "icloud.com", "www.icloud.com.cn", "icloud.com.cn"].contains(url.host?.lowercased()),
          url.path.hasPrefix("/share/") else { throw ShareError.invalid }
    let metadata = try await container.shareMetadata(for: url)
    let zone = metadata.share.recordID.zoneID
    guard metadata.containerIdentifier == containerID, zone.zoneName.hasPrefix(sharePrefix),
          metadata.share.recordID.recordName == CKRecordNameZoneWideShare else { throw ShareError.invalid }
    let scope = ShareScope(note: String(zone.zoneName.dropFirst(sharePrefix.count)),
                           owner: metadata.participantRole == .owner ? nil : zone.ownerName)
    _ = try scope.zone()
    let title = metadata.share[CKShare.SystemFieldKey.title] as? String
    if metadata.participantRole == .owner || metadata.participantStatus == .accepted {
        return ShareReply(scope: scope, title: title)
    }
    guard metadata.participantStatus == .pending else { throw ShareError.removed }
    try checkCloudAccountRevision(revision)
    _ = try await container.accept(metadata)
    return ShareReply(scope: scope, title: title)
}

private func manageShare(_ request: ShareRequest, revision: UInt64) async throws -> ShareReply {
    #if !FEI_CLOUD_QA
    guard request.accessChannel == nil else { throw ShareError.invalid }
    #endif
    let container = CKContainer(identifier: containerID)
    try await shareAccount(container, expected: request.account)
    // Existing participants can still be inspected or removed after access ends.
    if request.action != "members" && request.action != "revoke" {
        try await CloudAccess.shared.require(account: request.account, channel: request.accessChannel)
    }
    try checkCloudAccountRevision(revision)
    if request.action == "checkAccess" { return ShareReply() }
    if request.action == "accept", let url = request.url {
        return try await acceptShare(url, container: container, revision: revision)
    }
    guard let scope = request.scope, scope.owner == nil else { throw ShareError.ownerOnly }
    let zone = try scope.zone()
    let db = container.privateCloudDatabase
    if request.action == "create" {
        do { _ = try await db.recordZone(for: zone) }
        catch where shareIsGone(error) {
            try checkCloudAccountRevision(revision)
            _ = try await db.save(CKRecordZone(zoneID: zone))
        }
        do { return shareReply(try await getShare(db, in: zone), scope: scope) }
        catch let error as CKError where error.code == .unknownItem {
            try checkCloudAccountRevision(revision)
            let share = CKShare(recordZoneID: zone)
            share.publicPermission = .none
            share[CKShare.SystemFieldKey.title] = String((request.title ?? "Shared note").prefix(100))
            return shareReply(try await saveShare(share, to: db), scope: scope)
        }
    }
    let share = try await getShare(db, in: zone)
    guard share.publicPermission == .none else { throw ShareError.invalid }
    switch request.action {
    case "members":
        return shareReply(share, scope: scope)
    case "invite":
        guard let email = request.email, email.utf8.count <= 254, email.contains("@"),
              let access = request.permission, access == .readOnly || access == .readWrite else { throw ShareError.invalid }
        let invited = try await container.shareParticipant(forEmailAddress: email)
        guard !invited.isEqual(share.owner) else { throw ShareError.ownerOnly }
        let member = share.participants.first { $0.isEqual(invited) } ?? invited
        member.permission = access == .readWrite ? .readWrite : .readOnly
        share.addParticipant(member)
    case "permission", "revoke":
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
    return shareReply(try await saveShare(share, to: db), scope: scope)
}

private final class ShareCompletion: @unchecked Sendable {
    let done = DispatchSemaphore(value: 0)
    var reply = ShareReply()
}

@_cdecl("quiet_cloud_share")
public func quietCloudShare(_ json: UnsafePointer<CChar>) -> UnsafeMutablePointer<CChar>? {
    let input = Data(String(cString: json).utf8)
    let completion = ShareCompletion()
    let revision = quietCloudAccountRevision()
    let worker = Task.detached {
        do {
            let reply = try await manageShare(JSONDecoder().decode(ShareRequest.self, from: input), revision: revision)
            try checkCloudAccountRevision(revision)
            completion.reply = reply
        }
        catch {
            completion.reply = ShareReply(error: error.localizedDescription)
            if case CloudAccessError.required = error { completion.reply.errorKind = "subscriptionRequired" }
        }
        completion.done.signal()
    }
    let reply: ShareReply
    if completion.done.wait(timeout: .now() + 45) == .success { reply = completion.reply }
    else {
        worker.cancel()
        reply = ShareReply(error: ShareError.timeout.localizedDescription)
    }
    guard let bytes = try? JSONEncoder().encode(reply) else { return nil }
    return strdup(String(decoding: bytes, as: UTF8.self))
}
