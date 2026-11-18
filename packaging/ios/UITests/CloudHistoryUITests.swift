import XCTest

final class CloudHistoryUITests: XCTestCase {
    private func shot(_ name: String) {
        let attachment = XCTAttachment(screenshot: XCUIScreen.main.screenshot())
        attachment.name = name; attachment.lifetime = .keepAlways; add(attachment)
    }
    func testCachedHistoryCancelAndRestoreAsCopy() {
        continueAfterFailure = false
        let app = XCUIApplication()
        app.launchEnvironment["FEI_NOTES_MODE"] = "cloud"
        app.launchEnvironment["FEI_NOTES_CLOUD_CHANNEL"] = "55e8f724-b39a-441f-85d7-5c7263aa0364"
        XCUIDevice.shared.orientation = .portrait
        app.launch()
        XCTAssertTrue(app.buttons["fei.settings"].waitForExistence(timeout: 20),app.debugDescription)
        app.coordinate(withNormalizedOffset: CGVector(dx: 0.3,dy: 0.28)).tap()
        XCTAssertTrue(app.navigationBars["Session source"].waitForExistence(timeout: 8),app.debugDescription)
        func history() {
            app.buttons["fei.more"].tap()
            XCTAssertTrue(app.sheets.buttons["iCloud 历史版本"].waitForExistence(timeout: 5),app.debugDescription)
            app.sheets.buttons["iCloud 历史版本"].tap()
            XCTAssertTrue(app.tables["fei.cloud.versions"].waitForExistence(timeout: 5),app.debugDescription)
        }
        func gone() { XCTAssertTrue(XCTWaiter.wait(for: [XCTNSPredicateExpectation(predicate: NSPredicate(format: "exists == false"),object: app.tables["fei.cloud.versions"])],timeout: 5) == .completed) }
        history()
        XCTAssertFalse(app.buttons["fei.cloud.versions.restore"].isEnabled)
        XCTAssertEqual(app.tables["fei.cloud.versions"].cells.count,2)
        shot("history-01-native-version-list")
        app.cells["fei.cloud.versions.10002"].tap()
        XCTAssertTrue(app.buttons["fei.cloud.versions.restore"].isEnabled)
        app.buttons["fei.cloud.versions.cancel"].tap(); gone()
        XCTAssertTrue(app.navigationBars["Session source"].exists)
        history(); XCTAssertFalse(app.buttons["fei.cloud.versions.restore"].isEnabled)
        app.cells["fei.cloud.versions.10001"].tap()
        XCTAssertTrue(app.buttons["fei.cloud.versions.restore"].isEnabled)
        shot("history-02-older-version-selected")
        app.buttons["fei.cloud.versions.restore"].tap(); gone()
        XCTAssertTrue(app.navigationBars["History source · 恢复副本"].waitForExistence(timeout: 10),app.debugDescription)
        shot("history-03-recovered-copy-opened")
        app.navigationBars.buttons["返回"].firstMatch.tap()
        XCTAssertTrue(app.buttons["fei.settings"].waitForExistence(timeout: 5))
        app.buttons["fei.library-actions"].tap(); app.sheets.buttons["文档库"].tap()
        app.sheets.buttons["本机文档库"].tap()
        XCTAssertTrue(app.buttons["fei.settings"].waitForExistence(timeout: 15))
        app.buttons["fei.new"].tap(); XCTAssertTrue(app.buttons["fei.more"].waitForExistence(timeout: 5))
        app.buttons["fei.more"].tap()
        XCTAssertTrue(app.sheets.firstMatch.waitForExistence(timeout: 5))
        XCTAssertFalse(app.sheets.buttons["iCloud 历史版本"].exists)
        app.coordinate(withNormalizedOffset: CGVector(dx: 0.03,dy: 0.65)).tap()
        app.navigationBars.buttons["返回"].firstMatch.tap()
        print("HISTORY25_COMPLETED")
    }
}
