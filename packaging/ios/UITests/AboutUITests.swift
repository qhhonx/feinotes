import XCTest

final class AboutUITests: XCTestCase {
    private func shot(_ name:String) {
        let a = XCTAttachment(screenshot: XCUIScreen.main.screenshot()); a.name = name; a.lifetime = .keepAlways; add(a)
    }
    func testProductVersionAndBundledNotices() {
        continueAfterFailure = false
        let app = XCUIApplication(); app.launchEnvironment["FEI_NOTES_MODE"] = "local"
        XCUIDevice.shared.orientation = .portrait; app.launch()
        XCTAssertTrue(app.buttons["fei.settings"].waitForExistence(timeout: 15)); app.buttons["fei.settings"].tap()
        XCTAssertTrue(app.navigationBars["设置"].waitForExistence(timeout: 5))
        app.coordinate(withNormalizedOffset: CGVector(dx: 0.5,dy: 0.579)).tap()
        XCTAssertTrue(app.navigationBars["关于"].waitForExistence(timeout: 5),app.debugDescription)
        shot("about36-01-product-version-and-links")
        app.coordinate(withNormalizedOffset: CGVector(dx: 0.84,dy: 0.654)).tap()
        let done = app.navigationBars.buttons["fei.notices.done"].firstMatch
        XCTAssertTrue(done.waitForExistence(timeout: 10))
        let list = app.tables["fei.notices.list"]
        XCTAssertTrue(list.waitForExistence(timeout: 5))
        let components = app.searchFields["fei.notices.search"]
        components.tap(); components.typeText("accesskit 0.24.1")
        XCTAssertEqual(list.cells.count,1); shot("about36-02-notices-component-search")
        list.cells.firstMatch.tap()
        let reader = app.textViews["fei.notices.text"]
        XCTAssertTrue(reader.waitForExistence(timeout: 5))
        let first = reader.value as? String
        XCTAssertTrue(first?.hasPrefix("accesskit 0.24.1") == true)
        shot("about36-02-bundled-notices-preview")
        XCTAssertTrue(app.buttons["fei.notices.next"].isEnabled); app.buttons["fei.notices.next"].tap()
        XCTAssertNotEqual(reader.value as? String,first); app.buttons["fei.notices.previous"].tap(); XCTAssertEqual(reader.value as? String,first)
        app.buttons["fei.notices.find"].tap()
        let search = app.searchFields.firstMatch
        XCTAssertTrue(search.waitForExistence(timeout: 5)); search.typeText("accesskit")
        shot("about36-03-notices-native-find")
        done.tap()
        XCTAssertEqual(XCTWaiter.wait(for: [XCTNSPredicateExpectation(predicate: NSPredicate(format: "exists == false"),object: done)],timeout: 5),.completed)
        XCTAssertTrue(app.navigationBars["关于"].waitForExistence(timeout: 5)); shot("about36-04-returned-to-about")
    }
}
