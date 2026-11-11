// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Copyright 2026 Fei Notes contributors. See LICENSES.md.
import AppKit
import Foundation
import StoreKit

enum CloudPlan: String, Codable, CaseIterable {
    case annual, monthly

    var unit: Product.SubscriptionPeriod.Unit { self == .annual ? .year : .month }
}

struct CloudOffer: Encodable {
    let plan: CloudPlan
    let price: String
    var equivalentMonthly: String?
    var savingPercent: Int?
}

struct CloudActiveSubscription: Encodable {
    let plan: CloudPlan
    let validUntil: UInt64

    init(plan: CloudPlan, deadline: Date) {
        self.plan = plan
        validUntil = UInt64(deadline.timeIntervalSince1970 * 1_000)
    }
}

struct CloudSubscriptionStore {
    let annualID: String
    let monthlyID: String

    init(annualID: String, monthlyID: String) {
        self.annualID = annualID
        self.monthlyID = monthlyID
    }

    init?(bundle: Bundle) {
        guard let annual = bundle.object(forInfoDictionaryKey: "FeiStoreKitAnnualProduct") as? String, !annual.isEmpty,
              let monthly = bundle.object(forInfoDictionaryKey: "FeiStoreKitMonthlyProduct") as? String, !monthly.isEmpty,
              annual != monthly else { return nil }
        self.init(annualID: annual, monthlyID: monthly)
    }

    private func id(for plan: CloudPlan) -> String { plan == .annual ? annualID : monthlyID }

    func products() async throws -> [CloudPlan: Product] {
        let products = try await Product.products(for: [annualID, monthlyID])
        var matched: [CloudPlan: Product] = [:]
        for plan in CloudPlan.allCases {
            if let product = products.first(where: { $0.id == id(for: plan) }),
               product.type == .autoRenewable, let period = product.subscription?.subscriptionPeriod,
               period.value == 1 && period.unit == plan.unit { matched[plan] = product }
        }
        guard let group = matched[.annual]?.subscription?.subscriptionGroupID,
              matched[.monthly]?.subscription?.subscriptionGroupID == group else { return [:] }
        return matched
    }

    func offers() async throws -> [CloudOffer] {
        let products = try await products()
        return CloudPlan.allCases.compactMap { plan in
            guard let product = products[plan] else { return nil }
            var offer = CloudOffer(plan: plan, price: product.displayPrice)
            if plan == .annual {
                offer.equivalentMonthly = (product.price / 12).formatted(product.priceFormatStyle)
                if let month = products[.monthly], month.priceFormatStyle.currencyCode == product.priceFormatStyle.currencyCode {
                    offer.savingPercent = Self.savingPercent(annual: product.price, monthly: month.price)
                }
            }
            return offer
        }
    }

    static func savingPercent(annual: Decimal, monthly: Decimal) -> Int? {
        let twelveMonths = monthly * 12
        guard annual > 0, twelveMonths > annual else { return nil }
        var percent = (twelveMonths - annual) * 100 / twelveMonths
        var rounded = Decimal.zero
        NSDecimalRound(&rounded, &percent, 0, .down)
        let saving = NSDecimalNumber(decimal: rounded).intValue
        return saving > 0 ? saving : nil
    }

    private func plan(for transaction: Transaction) -> CloudPlan? {
        guard transaction.productType == .autoRenewable, transaction.revocationDate == nil, !transaction.isUpgraded else { return nil }
        return CloudPlan.allCases.first { id(for: $0) == transaction.productID }
    }

    @MainActor func listenForUpdates(onChange: @escaping @MainActor @Sendable () -> Void) {
        guard subscriptionUpdates == nil else { return }
        subscriptionUpdates = Task {
            for await result in Transaction.updates {
                guard case .verified(let transaction) = result, transaction.productType == .autoRenewable,
                      transaction.productID == annualID || transaction.productID == monthlyID else { continue }
                await transaction.finish()
                await CloudAccess.shared.invalidate()
                onChange()
            }
        }
    }

    func activeSubscription(now: Date = Date()) async -> CloudActiveSubscription? {
        var monthly: CloudActiveSubscription?
        for await result in Transaction.currentEntitlements {
            guard case .verified(let transaction) = result, let plan = plan(for: transaction),
                  let expiration = transaction.expirationDate, expiration > now else { continue }
            let active = CloudActiveSubscription(plan: plan, deadline: expiration)
            if plan == .annual { return active }
            monthly = active
        }
        if let monthly { return monthly }
        // StoreKit's renewal state can still authorize a verified grace period
        // after the last paid transaction has dropped out of currentEntitlements.
        for plan in CloudPlan.allCases {
            guard case .verified(let transaction)? = await Transaction.latest(for: id(for: plan)),
                  self.plan(for: transaction) == plan, let status = await transaction.subscriptionStatus,
                  status.state == .inGracePeriod, case .verified(let current) = status.transaction,
                  current.originalID == transaction.originalID, self.plan(for: current) == plan,
                  case .verified(let renewal) = status.renewalInfo,
                  let grace = renewal.gracePeriodExpirationDate, grace > now else { continue }
            return CloudActiveSubscription(plan: plan, deadline: grace)
        }
        return nil
    }

    @MainActor func purchase(_ plan: CloudPlan, in window: NSWindow?) async throws -> CloudPurchaseOutcome {
        guard let product = try await products()[plan] else { throw CloudSubscriptionError.unavailable }
        let result: Product.PurchaseResult
        if #available(macOS 15.2, *) {
            guard let window else { throw CloudSubscriptionError.missingWindow }
            result = try await product.purchase(confirmIn: window)
        } else {
            result = try await product.purchase()
        }
        switch result {
        case .success(.verified(let transaction)):
            guard self.plan(for: transaction) == plan else { throw CloudSubscriptionError.unverified }
            await transaction.finish()
            await CloudAccess.shared.invalidate()
            return .purchased
        case .success(.unverified): throw CloudSubscriptionError.unverified
        case .pending: return .pending
        case .userCancelled: return .cancelled
        @unknown default: throw CloudSubscriptionError.unavailable
        }
    }
}

enum CloudPurchaseOutcome: String, Encodable { case loaded, purchased, pending, cancelled, restored }

private enum CloudSubscriptionError: LocalizedError {
    case unavailable, missingWindow, unverified, busy

    var errorDescription: String? {
        switch self {
        case .unavailable: "The subscription plan is unavailable."
        case .missingWindow: "Reopen subscription settings and try again."
        case .unverified: "The App Store purchase could not be verified."
        case .busy: "Another subscription request is in progress."
        }
    }
}

private struct CloudSubscriptionReply: Encodable {
    var offers: [CloudOffer]?
    var active: CloudActiveSubscription?
    var outcome: CloudPurchaseOutcome?
    var error: String?
}

@_cdecl("quiet_cloud_subscription_available")
public func quietCloudSubscriptionAvailable() -> Bool { CloudSubscriptionStore(bundle: .main) != nil }

@_cdecl("quiet_cloud_watch_subscription")
@MainActor
public func quietCloudWatchSubscription(_ onChange: @escaping @Sendable @convention(c) () -> Void) {
    CloudSubscriptionStore(bundle: .main)?.listenForUpdates { onChange() }
}

@MainActor private var subscriptionActionInProgress = false
@MainActor private var subscriptionUpdates: Task<Void, Never>?

@_cdecl("quiet_cloud_subscription")
public func quietCloudSubscription(_ action: Int32, _ context: UInt,
    _ complete: @escaping @Sendable @convention(c) (UInt, UnsafePointer<CChar>?) -> Void) {
    Task { @MainActor in
        var reply = CloudSubscriptionReply()
        do {
            guard let store = CloudSubscriptionStore(bundle: .main) else { throw CloudSubscriptionError.unavailable }
            let interactive = action == 1 || action == 2 || action == 3
            if interactive {
                guard !subscriptionActionInProgress else { throw CloudSubscriptionError.busy }
                subscriptionActionInProgress = true
            }
            defer { if interactive { subscriptionActionInProgress = false } }
            switch action {
            case 0:
                do {
                    reply.offers = try await store.offers()
                    reply.outcome = .loaded
                } catch { reply.error = error.localizedDescription }
            case 1: reply.outcome = try await store.purchase(.annual, in: NSApp.keyWindow)
            case 2: reply.outcome = try await store.purchase(.monthly, in: NSApp.keyWindow)
            case 3:
                try await AppStore.sync()
                await CloudAccess.shared.invalidate()
                reply.outcome = .restored
            case 4: break
            default: throw CloudSubscriptionError.unavailable
            }
            reply.active = await store.activeSubscription()
        } catch { reply.error = error.localizedDescription }
        guard let data = try? JSONEncoder().encode(reply) else { complete(context, nil); return }
        String(decoding: data, as: UTF8.self).withCString { complete(context, $0) }
    }
}
