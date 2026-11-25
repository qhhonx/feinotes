import XCTest
import StoreKitTest
import StoreKit
import UIKit
@testable import FeiNotes

private final class SubscriptionResponse: @unchecked Sendable {
    let continuation: CheckedContinuation<Data?, Never>
    init(_ continuation: CheckedContinuation<Data?, Never>) { self.continuation = continuation }
}

private let receiveSubscription: @Sendable @convention(c) (UInt, UnsafePointer<CChar>?) -> Void = { context, json in
    let response = Unmanaged<SubscriptionResponse>.fromOpaque(UnsafeRawPointer(bitPattern: context)!).takeRetainedValue()
    response.continuation.resume(returning: json.map { Data(String(cString: $0).utf8) })
}

@MainActor
final class SubscriptionServiceTests: XCTestCase {
    nonisolated override class func tearDown() {
        if Thread.isMainThread { MainActor.assumeIsolated { EditorHost.active?.stopForTermination() } }
        else { DispatchQueue.main.sync { EditorHost.active?.stopForTermination() } }
        super.tearDown()
    }
    private func request(_ action: Int32) async throws -> [String: Any] {
        let data = await withCheckedContinuation { continuation in
            let response = Unmanaged.passRetained(SubscriptionResponse(continuation)).toOpaque()
            quietCloudSubscription(action, UInt(bitPattern: response), receiveSubscription)
        }
        return try XCTUnwrap(try JSONSerialization.jsonObject(with: XCTUnwrap(data)) as? [String: Any])
    }

    private func session() throws -> SKTestSession {
        let url = try XCTUnwrap(Bundle(for: Self.self).url(forResource: "FeiNotes", withExtension: "storekit"))
        let session = try SKTestSession(contentsOf: url)
        session.resetToDefaultState()
        session.clearTransactions()
        session.disableDialogs = true
        session.storefront = "USA"
        session.locale = Locale(identifier: "en_US")
        return session
    }

    private func readyScene() async throws {
        for _ in 0..<50 {
            if UIApplication.shared.connectedScenes.contains(where: { scene in
                scene.activationState == .foregroundActive && (scene as? UIWindowScene)?.windows.contains(where: { $0.isKeyWindow }) == true
            }) { return }
            try await Task.sleep(for: .milliseconds(100))
        }
        XCTFail("The hosted app did not become active")
    }

    func testLocalStoreKitCannotStartAnICloudTrial() throws {
        for start in [false, true] {
            let response = try XCTUnwrap(quietCloudTrial(start, nil))
            defer { free(response) }
            let data = Data(String(cString: response).utf8)
            let reply = try XCTUnwrap(JSONSerialization.jsonObject(with: data) as? [String: Any])
            XCTAssertNil(reply["trial"])
            if start { XCTAssertNotNil(reply["error"]) } else { XCTAssertNil(reply["error"]) }
        }
    }

    func testAnnualPurchaseStatusAndRestoreUseTheSharedBridge() async throws {
        continueAfterFailure = false
        let session = try session()
        defer { session.askToBuyEnabled = false; session.disableDialogs = false; session.clearTransactions() }
        try await readyScene()
        XCTAssertTrue(quietCloudSubscriptionAvailable())
        let products = try await Product.products(for: ["app.feinotes.testing.cloud.annual", "app.feinotes.testing.cloud.monthly"])
        XCTAssertEqual(products.count, 2, "The local StoreKit configuration must be active before any purchase request")
        let loaded = try await request(0)
        XCTAssertNil(loaded["error"])
        XCTAssertEqual(loaded["outcome"] as? String, "loaded")
        let offers = try XCTUnwrap(loaded["offers"] as? [[String: Any]])
        XCTAssertEqual(Set(offers.compactMap { $0["plan"] as? String }), ["annual", "monthly"])
        XCTAssertTrue(offers.allSatisfy { !(($0["price"] as? String) ?? "").isEmpty })
        XCTAssertNil(loaded["active"])
        let bought = try await request(1)
        XCTAssertNil(bought["error"])
        XCTAssertEqual(bought["outcome"] as? String, "purchased")
        XCTAssertEqual((bought["active"] as? [String: Any])?["plan"] as? String, "annual")
        let deadline = try XCTUnwrap((bought["active"] as? [String: Any])?["validUntil"] as? NSNumber)
        XCTAssertGreaterThan(deadline.doubleValue, Date().timeIntervalSince1970 * 1000)
        XCTAssertTrue(session.allTransactions().contains { $0.productIdentifier == "app.feinotes.testing.cloud.annual" })
        let status = try await request(4)
        XCTAssertNil(status["error"])
        XCTAssertEqual((status["active"] as? [String: Any])?["plan"] as? String, "annual")
        let restored = try await request(3)
        XCTAssertNil(restored["error"])
        XCTAssertEqual(restored["outcome"] as? String, "restored")
        XCTAssertEqual((restored["active"] as? [String: Any])?["plan"] as? String, "annual")
    }

    func testPendingPurchaseDoesNotGrantSubscription() async throws {
        continueAfterFailure = false
        let session = try session()
        defer { session.askToBuyEnabled = false; session.disableDialogs = false; session.clearTransactions() }
        try await readyScene()
        session.askToBuyEnabled = true
        let reply = try await request(2)
        XCTAssertNil(reply["error"])
        XCTAssertEqual(reply["outcome"] as? String, "pending")
        XCTAssertNil(reply["active"])
    }

    func testCrossgradePreservesCurrentPlanUntilRenewal() async throws {
        continueAfterFailure = false
        let session = try session()
        defer { session.disableDialogs = false; session.clearTransactions() }
        let store = try XCTUnwrap(CloudSubscriptionStore(bundle: .main))
        let products = try await store.products()
        let annual = try XCTUnwrap(products[.annual]?.subscription)
        let monthly = try await session.buyProduct(identifier: store.monthlyID)

        do {
            _ = try await store.completePurchase(.success(.verified(monthly)), requested: .annual, subscription: annual)
            XCTFail("An unchanged monthly renewal must not confirm an annual plan change")
        } catch {}

        let crossgrade = try await session.buyProduct(identifier: store.annualID)
        XCTAssertEqual(crossgrade.productID, store.monthlyID)
        let outcome = try await store.completePurchase(.success(.verified(crossgrade)), requested: .annual, subscription: annual)
        XCTAssertEqual(outcome, .scheduled)
        let current = await store.activeSubscription()
        XCTAssertEqual(current?.plan, .monthly)

        try session.forceRenewalOfSubscription(productIdentifier: store.monthlyID)
        for _ in 0..<50 {
            if await store.activeSubscription()?.plan == .annual { return }
            try await Task.sleep(for: .milliseconds(100))
        }
        XCTFail("Annual should become active after StoreKit delivers the renewal")
    }
}
