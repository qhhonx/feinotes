import XCTest

final class CloudSwitchUITests: XCTestCase {
    private func createNote(_ app:XCUIApplication) {
        let button = app.buttons.matching(identifier: "fei.new").allElementsBoundByIndex.first { $0.isHittable }
        XCTAssertNotNil(button,app.debugDescription)
        button?.tap()
        XCTAssertTrue(app.buttons["fei.more"].waitForExistence(timeout: 8),app.debugDescription)
    }

    func testSilentCloudStartupKeepsHomeUsable() {
        continueAfterFailure = false
        let app = XCUIApplication()
        app.launchEnvironment["FEI_NOTES_CLOUD_CHANNEL"] = UUID().uuidString
        app.launchEnvironment["FEI_QA_SLOW_SIGN_OUT"] = "1"
        XCUIDevice.shared.orientation = UIDevice.current.userInterfaceIdiom == .pad ? .landscapeLeft : .portrait
        app.launch()
        XCTAssertTrue(app.buttons["fei.settings"].waitForExistence(timeout: 6))
        XCTAssertFalse(app.staticTexts["正在打开 iCloud 文档库…"].exists)
        createNote(app)
        XCTAssertFalse(app.alerts.firstMatch.exists)
    }

    func testFailedCloudSwitchKeepsSettingsResponsive() {
        continueAfterFailure = false
        let app = XCUIApplication()
        app.launchEnvironment["FEI_NOTES_MODE"] = "local"
        app.launchEnvironment["FEI_QA_CLOUD_SWITCH_FAILURE"] = "1"
        XCUIDevice.shared.orientation = UIDevice.current.userInterfaceIdiom == .pad ? .landscapeLeft : .portrait
        app.launch()
        XCTAssertTrue(app.buttons["fei.settings"].waitForExistence(timeout: 10))
        app.buttons["fei.settings"].tap()
        let cloud = app.cells["fei.setting.page:Library"]
        for _ in 0..<6 { if cloud.isHittable { break }; app.tables["fei.settings.list"].swipeUp() }
        XCTAssertTrue(cloud.isHittable); cloud.tap()
        let toggle = app.cells["fei.setting.library-switch"]
        for _ in 0..<6 { if toggle.isHittable { break }; app.tables["fei.settings.list"].swipeUp() }
        XCTAssertTrue(toggle.isHittable); toggle.tap()
        XCTAssertTrue(app.alerts.firstMatch.waitForExistence(timeout: 5))
        app.alerts.firstMatch.buttons.matching(identifier: "fei.confirm.ok").firstMatch.tap()
        let message = app.staticTexts.matching(identifier: "fei.settings.message").firstMatch
        XCTAssertTrue(message.waitForExistence(timeout: 5))
        XCTAssertTrue(message.label.contains("Cloud switch QA failure"),message.label)
        XCTAssertTrue(toggle.isEnabled)
        if app.buttons["fei.settings.close"].exists { app.buttons["fei.settings.close"].tap() }
        else {
            app.navigationBars.buttons.element(boundBy: 0).tap()
            XCTAssertTrue(app.cells["fei.setting.page:Library"].waitForExistence(timeout: 5))
            app.navigationBars.buttons.element(boundBy: 0).tap()
        }
        XCTAssertTrue(app.tables["fei.notes.list"].waitForExistence(timeout: 5))
        createNote(app)
        XCTAssertFalse(app.alerts.firstMatch.exists)
    }
}
