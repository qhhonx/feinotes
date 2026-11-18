import XCTest

final class CloudSwitchUITests: XCTestCase {
    func testFailedCloudSwitchKeepsSettingsResponsive() {
        continueAfterFailure = false
        let app = XCUIApplication()
        app.launchEnvironment["FEI_NOTES_MODE"] = "local"
        app.launchEnvironment["FEI_QA_CLOUD_SWITCH_FAILURE"] = "1"
        XCUIDevice.shared.orientation = UIDevice.current.userInterfaceIdiom == .pad ? .landscapeLeft : .portrait
        app.launch()
        XCTAssertTrue(app.buttons["fei.settings"].waitForExistence(timeout: 10))
        app.buttons["fei.settings"].tap()
        let settings = app.tables["fei.settings.list"]
        let library = app.cells["fei.setting.page:Library"]
        for _ in 0..<6 { if library.isHittable { break }; settings.swipeUp() }
        XCTAssertTrue(library.isHittable)
        library.tap()
        let toggle = app.cells["fei.setting.library-switch"]
        for _ in 0..<6 { if toggle.isHittable { break }; settings.swipeUp() }
        XCTAssertTrue(toggle.isHittable)
        toggle.tap()
        XCTAssertTrue(app.alerts.firstMatch.waitForExistence(timeout: 5))
        app.alerts.firstMatch.buttons.matching(identifier: "fei.confirm.ok").firstMatch.tap()
        let message = app.staticTexts.matching(identifier: "fei.settings.message").firstMatch
        XCTAssertTrue(message.waitForExistence(timeout: 5))
        XCTAssertTrue(message.label.contains("Cloud switch QA failure"),message.label)
        XCTAssertTrue(toggle.isEnabled)
        if app.buttons["fei.settings.close"].exists { app.buttons["fei.settings.close"].tap() }
        else {
            app.navigationBars.buttons.element(boundBy: 0).tap()
            XCTAssertTrue(library.waitForExistence(timeout: 5))
            app.navigationBars.buttons.element(boundBy: 0).tap()
        }
        XCTAssertTrue(app.tables["fei.notes.list"].waitForExistence(timeout: 5))
        let newNote = app.buttons.matching(identifier: "fei.new").allElementsBoundByIndex.first { $0.isHittable }
        XCTAssertNotNil(newNote)
        newNote?.tap()
        XCTAssertTrue(app.buttons["fei.more"].waitForExistence(timeout: 8))
    }
}
