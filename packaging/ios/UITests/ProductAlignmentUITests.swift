import XCTest

final class ProductAlignmentUITests: XCTestCase {
    private func shot(_ name:String) {
        let a = XCTAttachment(screenshot: XCUIScreen.main.screenshot()); a.name = name; a.lifetime = .keepAlways; add(a)
    }
    func testSharedAppearancePage() {
        continueAfterFailure = false
        let app = XCUIApplication()
        app.launchEnvironment["FEI_NOTES_MODE"] = "local"
        XCUIDevice.shared.orientation = .portrait
        app.launch()
        XCTAssertTrue(app.buttons["fei.settings"].waitForExistence(timeout: 15))
        app.buttons["fei.settings"].tap()
        XCTAssertTrue(app.navigationBars["设置"].waitForExistence(timeout: 5))
        app.coordinate(withNormalizedOffset: CGVector(dx: 0.3,dy: 0.236)).tap()
        XCTAssertTrue(app.navigationBars["外观"].waitForExistence(timeout: 5),app.debugDescription)
        shot("alignment32-appearance-default")
        app.coordinate(withNormalizedOffset: CGVector(dx: 0.86,dy: 0.416)).tap()
        shot("alignment32-appearance-large")
        app.terminate(); app.launch()
        XCTAssertTrue(app.buttons["fei.settings"].waitForExistence(timeout: 15)); app.buttons["fei.settings"].tap()
        XCTAssertTrue(app.navigationBars["设置"].waitForExistence(timeout: 5))
        app.coordinate(withNormalizedOffset: CGVector(dx: 0.3,dy: 0.236)).tap()
        XCTAssertTrue(app.navigationBars["外观"].waitForExistence(timeout: 5))
        shot("alignment32-appearance-large-restarted")
        app.navigationBars.buttons.element(boundBy: 0).tap(); app.navigationBars.buttons.element(boundBy: 0).tap()
        XCTAssertTrue(app.tables["fei.notes.list"].waitForExistence(timeout: 5))
        shot("alignment32-large-native-home")
    }
}
