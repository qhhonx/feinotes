// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Copyright 2026 Fei Notes contributors. See LICENSES.md.
#if os(macOS)
import AppKit
typealias CloudPurchaseWindow = NSWindow
#elseif os(iOS)
import UIKit
typealias CloudPurchaseWindow = UIViewController
#endif
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
        #if FEI_STOREKIT_TESTING
        NSLog("Fei Notes StoreKit: loading local products")
        #endif
        let products = try await Product.products(for: [annualID, monthlyID])
        #if FEI_STOREKIT_TESTING
        NSLog("Fei Notes StoreKit: loaded %d local products", products.count)
        #endif
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
        offers(in: try await products())
    }

    func offers(in products: [CloudPlan: Product]) -> [CloudOffer] {
        CloudPlan.allCases.compactMap { plan in
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

    #if os(macOS) || os(iOS)
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

    #endif

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

    #if os(macOS) || os(iOS)
    @MainActor func purchase(_ plan: CloudPlan, in window: CloudPurchaseWindow?) async throws -> CloudPurchaseOutcome {
        guard let product = try await products()[plan], let subscription = product.subscription else {
            throw CloudSubscriptionError.unavailable
        }
        let result: Product.PurchaseResult
        #if os(macOS)
        if #available(macOS 15.2, *) {
            guard let window else { throw CloudSubscriptionError.missingWindow }
            result = try await product.purchase(confirmIn: window)
        } else {
            result = try await product.purchase()
        }
        #elseif os(iOS)
        guard let window else { throw CloudSubscriptionError.missingWindow }
        #if FEI_STOREKIT_TESTING
        NSLog("Fei Notes StoreKit: presenter %@ attached=%d active=%d transition=%d", String(describing: type(of: window)),
            window.viewIfLoaded?.window != nil, window.viewIfLoaded?.window?.windowScene?.activationState == .foregroundActive,
            window.transitionCoordinator != nil)
        #endif
        guard window.viewIfLoaded?.window != nil else { throw CloudSubscriptionError.missingWindow }
        window.view.window?.endEditing(true)
        if #available(iOS 18.2, *) { result = try await product.purchase(confirmIn: window) }
        else {
            guard let scene = window.view.window?.windowScene else { throw CloudSubscriptionError.missingWindow }
            result = try await product.purchase(confirmIn: scene)
        }
        #endif
        return try await completePurchase(result, requested: plan, subscription: subscription)
    }

    func completePurchase(_ result: Product.PurchaseResult, requested: CloudPlan,
                          subscription: Product.SubscriptionInfo) async throws -> CloudPurchaseOutcome {
        switch result {
        case .success(.verified(let transaction)):
            guard let current = plan(for: transaction) else { throw CloudSubscriptionError.unverified }
            let outcome: CloudPurchaseOutcome
            if current == requested {
                outcome = .purchased
            } else {
                // A deferred plan change returns the current transaction. Verify the new plan against signed renewal info.
                guard try await confirmScheduledPlan(requested, transaction: transaction, subscription: subscription) else {
                    throw CloudSubscriptionError.unverified
                }
                outcome = .scheduled
            }
            await transaction.finish()
            await CloudAccess.shared.invalidate()
            return outcome
        case .success(.unverified): throw CloudSubscriptionError.unverified
        case .pending: return .pending
        case .userCancelled: return .cancelled
        @unknown default: throw CloudSubscriptionError.unavailable
        }
    }

    func nextPlan(for active: CloudPlan, in products: [CloudPlan: Product]) async throws -> CloudPlan? {
        guard let subscription = products[active]?.subscription,
              case .verified(let transaction)? = await Transaction.latest(for: id(for: active)) else { return nil }
        return try await nextPlan(for: transaction, subscription: subscription)
    }

    private func nextPlan(for transaction: Transaction, subscription: Product.SubscriptionInfo) async throws -> CloudPlan? {
        guard let current = plan(for: transaction) else { return nil }
        for status in try await subscription.status {
            guard status.state == .subscribed, case .verified(let active) = status.transaction,
                  active.originalID == transaction.originalID, plan(for: active) == current,
                  let expiration = active.expirationDate, expiration > Date(),
                  case .verified(let renewal) = status.renewalInfo, renewal.willAutoRenew,
                  renewal.currentProductID == transaction.productID else { continue }
            return CloudPlan.allCases.first { $0 != current && id(for: $0) == renewal.autoRenewPreference }
        }
        return nil
    }

    private func confirmScheduledPlan(_ requested: CloudPlan, transaction: Transaction,
                                      subscription: Product.SubscriptionInfo) async throws -> Bool {
        // StoreKit may deliver the purchase result before it updates the cached renewal information.
        for delay in [0, 150, 350, 650, 1_000] {
            if delay > 0 { try await Task.sleep(for: .milliseconds(delay)) }
            if (try? await nextPlan(for: transaction, subscription: subscription)) == requested { return true }
        }
        return false
    }
    #endif
}

#if os(macOS) || os(iOS)
enum CloudPurchaseOutcome: String, Encodable { case loaded, purchased, scheduled, pending, cancelled, restored }

enum CloudSubscriptionError: String, LocalizedError {
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
    var next: CloudPlan?
    var error: String?
    var errorKind: String?
}

@_cdecl("quiet_cloud_subscription_available")
public func quietCloudSubscriptionAvailable() -> Bool { CloudSubscriptionStore(bundle: .main) != nil }

@_cdecl("quiet_cloud_watch_subscription")
@MainActor
public func quietCloudWatchSubscription(_ onChange: @escaping @Sendable @convention(c) () -> Void) {
    subscriptionChanged = onChange
    CloudSubscriptionStore(bundle: .main)?.listenForUpdates { notifyCloudSubscriptionChange() }
}

@MainActor private var subscriptionActionInProgress = false
@MainActor private var subscriptionUpdates: Task<Void, Never>?
@MainActor private var subscriptionChanged: (@Sendable @convention(c) () -> Void)?

@MainActor func notifyCloudSubscriptionChange() { subscriptionChanged?() }

@MainActor func performCloudSubscriptionAction<T>(_ operation: @MainActor () async throws -> T) async throws -> T {
    guard !subscriptionActionInProgress else { throw CloudSubscriptionError.busy }
    subscriptionActionInProgress = true
    defer { subscriptionActionInProgress = false }
    return try await operation()
}

@MainActor func restoreCloudSubscription() async throws {
    try await AppStore.sync()
    await CloudAccess.shared.invalidate()
}

@_cdecl("quiet_cloud_subscription")
public func quietCloudSubscription(_ action: Int32, _ context: UInt,
    _ complete: @escaping @Sendable @convention(c) (UInt, UnsafePointer<CChar>?) -> Void) {
    Task { @MainActor in
        #if os(macOS)
        let window = NSApp.keyWindow
        #elseif os(iOS)
        let scene = UIApplication.shared.connectedScenes.compactMap { $0 as? UIWindowScene }
            .first { $0.activationState == .foregroundActive && $0.windows.contains { $0.isKeyWindow } }
        var window = scene?.windows.first { $0.isKeyWindow }?.rootViewController
        while let presented = window?.presentedViewController { window = presented }
        if let navigation = window as? UINavigationController { window = navigation.topViewController }
        #endif
        await runCloudSubscription(action, context, window, complete)
    }
}

#if os(macOS)
@_cdecl("quiet_cloud_subscription_from_view")
@MainActor
public func quietCloudSubscriptionFromView(_ action: Int32, _ context: UInt, _ view: UInt,
    _ complete: @escaping @Sendable @convention(c) (UInt, UnsafePointer<CChar>?) -> Void) {
    // Resolve and retain the presenting window before returning to the Rust event loop.
    let window = UnsafeRawPointer(bitPattern: view).flatMap {
        Unmanaged<NSView>.fromOpaque($0).takeUnretainedValue().window
    }
    Task { @MainActor in await runCloudSubscription(action, context, window, complete) }
}
#endif

@MainActor
private func runCloudSubscription(_ action: Int32, _ context: UInt, _ window: CloudPurchaseWindow?,
    _ complete: @Sendable @convention(c) (UInt, UnsafePointer<CChar>?) -> Void) async {
    #if FEI_STOREKIT_TESTING
    NSLog("Fei Notes StoreKit: action %d entered", action)
    #endif
    var reply = CloudSubscriptionReply()
    do {
        guard let store = CloudSubscriptionStore(bundle: .main) else { throw CloudSubscriptionError.unavailable }
        #if os(macOS)
        var loadedProducts: [CloudPlan: Product]? = nil
        #endif
        switch action {
        case 0:
            do {
                let products = try await store.products()
                #if os(macOS)
                loadedProducts = products
                #endif
                reply.offers = store.offers(in: products)
                reply.outcome = .loaded
            } catch { reply.error = error.localizedDescription }
        case 1, 2:
            #if os(macOS)
            guard window?.isVisible == true else { throw CloudSubscriptionError.missingWindow }
            #endif
            reply.outcome = try await performCloudSubscriptionAction {
                try await store.purchase(action == 1 ? .annual : .monthly, in: window)
            }
        case 3:
            try await performCloudSubscriptionAction { try await restoreCloudSubscription() }
            reply.outcome = .restored
        case 4: break
        default: throw CloudSubscriptionError.unavailable
        }
        reply.active = await store.activeSubscription()
        #if os(macOS)
        if reply.outcome == .purchased || reply.outcome == .restored { notifyCloudSubscriptionChange() }
        if action != 4, reply.outcome != .cancelled, let active = reply.active {
            let products: [CloudPlan: Product]?
            if let loadedProducts { products = loadedProducts }
            else { products = try? await store.products() }
            if let products { reply.next = try? await store.nextPlan(for: active.plan, in: products) }
        }
        #endif
    } catch StoreKitError.userCancelled {
        reply.outcome = .cancelled
    } catch {
        reply.error = error.localizedDescription
        reply.errorKind = (error as? CloudSubscriptionError)?.rawValue
    }
    guard let data = try? JSONEncoder().encode(reply) else { complete(context, nil); return }
    #if FEI_STOREKIT_TESTING
    NSLog("Fei Notes StoreKit: action %d completed", action)
    #endif
    String(decoding: data, as: UTF8.self).withCString { complete(context, $0) }
}
#endif
