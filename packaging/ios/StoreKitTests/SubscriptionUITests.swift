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
        app.coordinate(withNormalizedOffset: CGVector(dx: 0.4, dy: 0.41)).tap()
        XCTAssertTrue(app.navigationBars["iCloud 同步"].waitForExistence(timeout: 5))
        shot("storekit-01-shared-settings")
        app.coordinate(withNormalizedOffset: CGVector(dx: 0.8, dy: 0.313)).tap()
        app.swipeUp()
        shot("storekit-02-loaded-plans")
        XCTAssertTrue(session.allTransactions().isEmpty)
        app.coordinate(withNormalizedOffset: CGVector(dx: 0.82, dy: 0.51)).tap()
        let cancel = app.buttons.matching(NSPredicate(format: "label IN %@", ["取消", "Cancel", "关闭", "Close"])).firstMatch
        XCTAssertTrue(cancel.waitForExistence(timeout: 30), app.debugDescription)
        shot("storekit-03-native-purchase-sheet")
        cancel.tap()
        let done = app.buttons["fei.done-settings"]
        XCTAssertTrue(done.waitForExistence(timeout: 5))
        XCTAssertTrue(done.isHittable)
        XCTAssertTrue(session.allTransactions().isEmpty)
        app.coordinate(withNormalizedOffset: CGVector(dx: 0.8, dy: 0.313)).tap()
        shot("storekit-04-cancelled-no-transaction")
    }
}
