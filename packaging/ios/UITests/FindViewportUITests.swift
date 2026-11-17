import XCTest

final class FindViewportUITests: XCTestCase {
    func testCompactNavigationRestores() {
        continueAfterFailure = false
        let app = XCUIApplication()
        app.launchEnvironment["FEI_NOTES_MODE"] = "local"
        XCUIDevice.shared.orientation = .portrait
        app.launch()
        XCTAssertTrue(app.buttons["fei.settings"].waitForExistence(timeout: 15))
        app.buttons["fei.new"].tap()
        XCTAssertTrue(app.buttons["fei.more"].waitForExistence(timeout: 5))
        app.buttons["fei.more"].tap()
        app.sheets.buttons["文档内查找"].tap()
        XCTAssertTrue(app.searchFields["find.searchField"].waitForExistence(timeout: 5))
        XCUIDevice.shared.orientation = .landscapeLeft
        app.buttons["find.settingsButton"].firstMatch.tap()
        app.buttons["查找与替换"].firstMatch.tap()
        XCTAssertTrue(app.textFields["find.replaceField"].waitForExistence(timeout: 5))
        XCUIDevice.shared.orientation = .portrait
        let back = app.navigationBars.buttons["返回"].firstMatch
        XCTAssertTrue(back.waitForExistence(timeout: 5),app.debugDescription)
        app.buttons["find.doneButton"].firstMatch.tap()
        XCTAssertTrue(back.waitForExistence(timeout: 5),app.debugDescription)
        back.tap()
        XCTAssertTrue(app.buttons["fei.settings"].waitForExistence(timeout: 5))
    }
    private func capture(_ stage: String, app: XCUIApplication) {
        RunLoop.current.run(until: Date().addingTimeInterval(0.3))
        let query = app.searchFields["find.searchField"]
        let surface = app.otherElements["fei.editor.surface"].firstMatch
        XCTAssertTrue(surface.exists,app.debugDescription)
        let top = min(query.frame.minY,app.buttons["find.doneButton"].firstMatch.frame.minY)
        let values: [String: Any] = ["stage":stage,"width":app.frame.width,"height":app.frame.height,"surface_x":surface.frame.minX,"surface_y":surface.frame.minY,"surface_width":surface.frame.width,"surface_height":surface.frame.height,"panel_top":top,"counter":app.staticTexts["find.resultLabel"].label]
        let data = try! JSONSerialization.data(withJSONObject: values,options: .sortedKeys)
        print("VIEWPORT_METRICS=\(String(data: data,encoding: .utf8)!)")
        let attachment = XCTAttachment(screenshot: XCUIScreen.main.screenshot())
        attachment.name = stage
        attachment.lifetime = .keepAlways
        add(attachment)
    }
    func testCurrentMatchStaysVisibleAcrossKeyboardAndRotation() {
        continueAfterFailure = false
        let stamp = Int(Date().timeIntervalSince1970)
        print("VIEWPORT_TOUCH_STAMP=\(stamp)")
        let app = XCUIApplication()
        app.launchEnvironment["FEI_NOTES_MODE"] = "local"
        app.launch()
        XCTAssertTrue(app.buttons["fei.settings"].waitForExistence(timeout: 15))
        app.buttons["fei.new"].tap()
        XCTAssertTrue(app.buttons["fei.more"].waitForExistence(timeout: 5))
        app.coordinate(withNormalizedOffset: CGVector(dx: 0.3,dy: 0.28)).tap()
        let lines = (0..<45).map { index in [5,23,44].contains(index) ? "BEACON14 line \(index) \(stamp)" : "Line \(index) for viewport \(stamp)" }
        app.typeText(lines.joined(separator: "\n"))
        app.buttons["fei.more"].tap()
        app.sheets.buttons["重命名"].tap()
        let title = app.textFields["fei.rename.title"]
        XCTAssertTrue(title.waitForExistence(timeout: 5))
        title.typeText("FEI_VIEWPORT_\(stamp)")
        app.buttons["fei.rename.save"].tap()
        XCTAssertTrue(app.navigationBars["FEI_VIEWPORT_\(stamp)"].waitForExistence(timeout: 5))
        app.buttons["fei.more"].tap()
        app.sheets.buttons["文档内查找"].tap()
        let query = app.searchFields["find.searchField"]
        XCTAssertTrue(query.waitForExistence(timeout: 5))
        query.typeText("BEACON14")
        let counter = app.staticTexts["find.resultLabel"]
        XCTAssertTrue(XCTWaiter.wait(for: [XCTNSPredicateExpectation(predicate: NSPredicate(format: "label ENDSWITH %@","/3"),object: counter)],timeout: 5) == .completed,app.debugDescription)
        capture("viewport-01-portrait-search",app: app)
        XCUIDevice.shared.orientation = .landscapeLeft
        XCTAssertTrue(query.waitForExistence(timeout: 5))
        for index in 0..<3 {
            app.buttons["find.nextButton"].firstMatch.tap()
            capture("viewport-02-landscape-search-\(index)",app: app)
        }
        app.buttons["find.settingsButton"].firstMatch.tap()
        let replace = app.buttons["查找与替换"].firstMatch
        XCTAssertTrue(replace.waitForExistence(timeout: 5))
        replace.tap()
        XCTAssertTrue(app.textFields["find.replaceField"].waitForExistence(timeout: 5))
        for index in 0..<3 {
            app.buttons["find.nextButton"].firstMatch.tap()
            capture("viewport-03-landscape-replace-\(index)",app: app)
        }
        XCUIDevice.shared.orientation = .portrait
        XCTAssertTrue(query.waitForExistence(timeout: 5))
        capture("viewport-04-portrait-replace",app: app)
        app.buttons["find.doneButton"].firstMatch.tap()
        XCTAssertTrue(XCTWaiter.wait(for: [XCTNSPredicateExpectation(predicate: NSPredicate(format: "exists == false"),object: query)],timeout: 5) == .completed)
        let back = app.navigationBars.buttons["返回"].firstMatch
        XCTAssertTrue(back.waitForExistence(timeout: 5),app.debugDescription)
        back.tap()
        XCTAssertTrue(app.buttons["fei.settings"].waitForExistence(timeout: 5))
        print("VIEWPORT_TOUCH_COMPLETED=\(stamp)")
    }
}
