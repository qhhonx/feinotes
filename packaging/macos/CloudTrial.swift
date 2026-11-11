// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Copyright 2026 Fei Notes contributors. See LICENSES.md.
import CloudKit
import CryptoKit
import Foundation
#if FEI_TESTFLIGHT_BETA
import StoreKit
#endif

private struct CloudTrial: Encodable {
    let startedAt: UInt64
    let expiresAt: UInt64
}

private struct CloudTrialReply: Encodable {
    var trial: CloudTrial?
    var error: String?
}

private enum CloudTrialError: LocalizedError {
    case invalid, timeout

    var errorDescription: String? {
        switch self {
        case .invalid: "The iCloud trial record could not be verified."
        case .timeout: "iCloud did not respond. Check your connection and try again."
        }
    }
}

private func readTrial(start: Bool, channel: String?, expectedAccount: String? = nil) async throws -> CloudTrial? {
    let revision = quietCloudAccountRevision()
    let zoneName: String
    if let channel {
        #if FEI_CLOUD_QA
        guard let id = UUID(uuidString: channel) else { throw CloudTrialError.invalid }
        zoneName = "FeiCloudAccess-v1-QA-" + id.uuidString.lowercased()
        #else
        throw CloudTrialError.invalid
        #endif
    } else { zoneName = "FeiCloudAccess-v1" }
    let zone = CKRecordZone.ID(zoneName: zoneName)
    let id = CKRecord.ID(recordName: "trial", zoneID: zone)
    let container = CKContainer(identifier: containerID)
    let account = try await container.userRecordID().recordName
    let accountKey = SHA256.hash(data: Data(account.utf8)).map { String(format: "%02x", $0) }.joined()
    if let expectedAccount, accountKey != expectedAccount { throw ShareError.account }
    let database = container.privateCloudDatabase

    let record: CKRecord
    do { record = try await database.record(for: id) }
    catch let error as CKError where error.code == .zoneNotFound || error.code == .unknownItem {
        guard start else {
            try await shareAccount(container, expected: accountKey)
            try checkCloudAccountRevision(revision)
            return nil
        }
        try Task.checkCancellation()
        try await shareAccount(container, expected: accountKey)
        try checkCloudAccountRevision(revision)
        if error.code == .zoneNotFound { _ = try await database.save(CKRecordZone(zoneID: zone)) }
        try Task.checkCancellation()
        try await shareAccount(container, expected: accountKey)
        try checkCloudAccountRevision(revision)
        let first = CKRecord(recordType: "FeiCloudTrial", recordID: id)
        first["version"] = 1
        // Saving a new record with an existing ID conflicts instead of resetting its date.
        do { record = try await database.save(first) }
        catch let conflict as CKError where conflict.code == .serverRecordChanged {
            if let existing = conflict.serverRecord { record = existing }
            else { record = try await database.record(for: id) }
        }
    }
    try await shareAccount(container, expected: accountKey)
    try checkCloudAccountRevision(revision)
    guard record.recordType == "FeiCloudTrial", record.recordID == id, record["version"] as? Int == 1,
          let created = record.creationDate, created.timeIntervalSince1970 > 0,
          let creator = record.creatorUserRecordID?.recordName,
          creator == account || creator == CKCurrentUserDefaultName else { throw CloudTrialError.invalid }
    let startedAt = UInt64(created.timeIntervalSince1970 * 1_000)
    return CloudTrial(startedAt: startedAt, expiresAt: startedAt + 7 * 24 * 60 * 60 * 1_000)
}

enum CloudAccessError: LocalizedError {
    case required

    var errorDescription: String? { "iCloud sync and collaboration require an active trial or subscription." }
}

actor CloudAccess {
    static let shared = CloudAccess()

    private struct Scope: Hashable { let account: String; let channel: String? }
    private struct Check {
        let deadline: UInt64?
        let refreshAt: ContinuousClock.Instant
    }
    private var checks: [Scope: Check] = [:]
    private var generation = 0
    #if FEI_TESTFLIGHT_BETA
    private var verifiedTestFlight = false
    #endif

    func invalidate() {
        checks.removeAll()
        generation += 1
    }

    private func now(channel: String?) -> UInt64 {
        #if FEI_CLOUD_QA
        if channel != nil, let value = ProcessInfo.processInfo.environment["QUIET_SYNC_QA_TRIAL_TIME_MS"],
           let simulated = UInt64(value) { return simulated }
        #endif
        return UInt64(max(0, Date().timeIntervalSince1970 * 1_000))
    }

    func require(account: String, channel: String?) async throws {
        #if FEI_TESTFLIGHT_BETA
        try Task.checkCancellation()
        if verifiedTestFlight { return }
        guard case .verified(let transaction) = try? await AppTransaction.shared,
              transaction.bundleID == Bundle.main.bundleIdentifier,
              transaction.environment == .sandbox else { throw CloudAccessError.required }
        try Task.checkCancellation()
        verifiedTestFlight = true
        #else
        let store = CloudSubscriptionStore(bundle: .main)
        if store == nil && cloudEnvironment == "Development" { return }
        let scope = Scope(account: account, channel: channel)
        while true {
            try Task.checkCancellation()
            let time = now(channel: channel)
            if let check = checks[scope], check.refreshAt > .now {
                if let deadline = check.deadline, time < deadline { return }
                if check.deadline == nil { throw CloudAccessError.required }
            }
            let version = generation
            let paid = await store?.activeSubscription()
            let deadline: UInt64?
            if let paid, paid.validUntil > now(channel: channel) { deadline = paid.validUntil }
            else {
                let trial = try await readTrial(start: false, channel: channel, expectedAccount: account)
                deadline = trial.flatMap { $0.expiresAt > now(channel: channel) ? $0.expiresAt : nil }
            }
            try Task.checkCancellation()
            guard version == generation else { continue }
            checks[scope] = Check(deadline: deadline, refreshAt: .now.advanced(by: deadline == nil ? .seconds(5) : .seconds(20)))
            if let deadline, now(channel: channel) < deadline { return }
            throw CloudAccessError.required
        }
        #endif
    }
}

private final class CloudTrialCompletion: @unchecked Sendable {
    let done = DispatchSemaphore(value: 0)
    var reply = CloudTrialReply()
}

@_cdecl("quiet_cloud_trial")
public func quietCloudTrial(_ start: Bool, _ scope: UnsafePointer<CChar>?) -> UnsafeMutablePointer<CChar>? {
    let channel = scope.map { String(cString: $0) }
    let completion = CloudTrialCompletion()
    let worker = Task.detached {
        do {
            completion.reply.trial = try await readTrial(start: start, channel: channel)
            if start { await CloudAccess.shared.invalidate() }
        }
        catch { completion.reply.error = error.localizedDescription }
        completion.done.signal()
    }
    let reply: CloudTrialReply
    if completion.done.wait(timeout: .now() + 35) == .success { reply = completion.reply }
    else {
        worker.cancel()
        reply = CloudTrialReply(error: CloudTrialError.timeout.localizedDescription)
    }
    guard let data = try? JSONEncoder().encode(reply) else { return nil }
    return strdup(String(decoding: data, as: UTF8.self))
}
