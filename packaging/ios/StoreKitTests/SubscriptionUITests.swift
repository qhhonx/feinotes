import XCTest
import StoreKitTest

final class SubscriptionUITests: XCTestCase {
    private func shot(_ name: String) {
        let attachment = XCTAttachment(screenshot: XCUIScreen.main.screenshot())
        attachment.name = name
        attachment.lifetime = .keepAlways
        add(attachment)
    }
    func testPurchaseSheetCancellationAndSharedSettings() throws {
        continueAfterFailure = false
        let url = try XCTUnwrap(Bundle(for: Self.self).url(forResource: "FeiNotes", withExtension: "storekit"))
        let session = try SKTestSession(contentsOf: url)
        session.resetToDefaultState()
        session.disableDialogs = false
        session.askToBuyEnabled = false
        session.clearTransactions()
        defer { session.clearTransactions() }
        let app = XCUIApplication()
        app.launchEnvironment["FEI_NOTES_MODE"] = "local"
        app.launch()
        for owner in [app, XCUIApplication(bundleIdentifier: "com.apple.springboard")] {
            let cancel = owner.alerts.buttons.matching(NSPredicate(format: "label IN %@", ["取消", "Cancel"])).firstMatch
            if cancel.waitForExistence(timeout: 2) { cancel.tap() }
        }
        XCTAssertTrue(app.buttons["fei.settings"].waitForExistence(timeout: 15))
        app.buttons["fei.settings"].tap()
        let cloud = app.cells["fei.setting.page:Cloud"]
        XCTAssertTrue(cloud.waitForExistence(timeout: 5))
        cloud.tap()
        let viewPlans = app.cells["fei.setting.page:CloudAccess"]
        XCTAssertTrue(viewPlans.waitForExistence(timeout: 5))
        shot("storekit-00-view-plans-entry")
        viewPlans.tap()
        let monthly = app.buttons["fei.subscription.plan.monthly"]
        XCTAssertTrue(monthly.waitForExistence(timeout: 15))
        monthly.tap()
        let billing = app.staticTexts["fei.subscription.billing"]
        XCTAssertTrue(billing.label.contains("每月") || billing.label.contains("per month"))
        shot("storekit-01-subscription-sheet")
        XCTAssertTrue(session.allTransactions().isEmpty)
        app.buttons["fei.subscription.purchase"].tap()
        let cancelLabel = NSPredicate(format: "label IN %@", ["取消", "Cancel", "关闭", "Close"])
        let inApp = app.buttons.matching(cancelLabel).firstMatch
        let system = XCUIApplication(bundleIdentifier: "com.apple.springboard").buttons.matching(cancelLabel).firstMatch
        let cancel = inApp.waitForExistence(timeout: 3) ? inApp : system
        XCTAssertTrue(cancel.waitForExistence(timeout: 12), app.debugDescription)
        shot("storekit-02-native-purchase-sheet")
        cancel.tap()
        let close = app.buttons["fei.subscription.close"]
        XCTAssertTrue(close.waitForExistence(timeout: 5))
        XCTAssertTrue(close.isEnabled)
        XCTAssertTrue(session.allTransactions().isEmpty)
        close.tap()
        XCTAssertTrue(viewPlans.waitForExistence(timeout: 5))
        XCTAssertTrue(viewPlans.isHittable)
    }
}
