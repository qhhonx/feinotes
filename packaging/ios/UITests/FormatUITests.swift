import XCTest

final class FormatUITests: XCTestCase {
    private func shot(_ name: String) {
        let attachment = XCTAttachment(screenshot: XCUIScreen.main.screenshot())
        attachment.name = name
        attachment.lifetime = .keepAlways
        add(attachment)
    }

    func testSelectedWordFormatPalettesAndParagraphs() {
        continueAfterFailure = false
        let stamp = Int(Date().timeIntervalSince1970)
        print("FORMAT_TOUCH_STAMP=\(stamp)")
        let app = XCUIApplication()
        app.launchEnvironment["FEI_NOTES_MODE"] = "local"
        app.launch()
        XCTAssertTrue(app.buttons["fei.settings"].waitForExistence(timeout: 15))
        app.buttons["fei.new"].tap()
        XCTAssertTrue(app.buttons["fei.more"].waitForExistence(timeout: 5))
        app.coordinate(withNormalizedOffset: CGVector(dx: 0.3,dy: 0.28)).tap()
        app.typeText("Alpha Bravo Charlie \(stamp)")
        app.buttons["fei.more"].tap()
        app.coordinate(withNormalizedOffset: CGVector(dx: 0.05,dy: 0.8)).tap()
        app.coordinate(withNormalizedOffset: CGVector(dx: 0.24,dy: 0.28)).press(forDuration: 0.8)
        let styles = app.menuItems["更多样式…"]
        XCTAssertTrue(styles.waitForExistence(timeout: 5),app.debugDescription)
        styles.tap()
        let table = app.tables["fei.format.controls"]
        XCTAssertTrue(table.waitForExistence(timeout: 5),app.debugDescription)
        func tap(_ key: String) {
            let cell = table.cells["fei.format.\(key)"]
            for _ in 0..<8 {
                if cell.isHittable { break }
                if cell.exists && cell.frame.minY < table.frame.minY { table.swipeDown() }
                else { table.swipeUp() }
            }
            XCTAssertTrue(cell.isHittable,app.debugDescription)
            cell.tap()
            RunLoop.current.run(until: Date().addingTimeInterval(0.3))
            XCTAssertTrue(table.exists,app.debugDescription)
        }
        shot("format-01-selected-word-controls")
        tap("bold")
        tap("italic")
        tap("text-colors")
        XCTAssertEqual(table.cells.count,8)
        tap("text-color:red")
        shot("format-02-full-text-colors")
        app.navigationBars["fei.format.navigation"].buttons.element(boundBy: 0).tap()
        tap("text-highlights")
        XCTAssertEqual(table.cells.count,16)
        tap("highlight:light-yellow")
        shot("format-03-full-highlights")
        app.navigationBars["fei.format.navigation"].buttons.element(boundBy: 0).tap()
        tap("align-center")
        tap("block-types")
        XCTAssertEqual(table.cells.count,15)
        tap("heading4")
        shot("format-04-all-heading-levels")
        app.buttons["fei.format.done"].tap()
        RunLoop.current.run(until: Date().addingTimeInterval(0.4))
        shot("format-05-shared-editor-result")
        app.buttons["fei.more"].tap()
        app.sheets.buttons["编辑和格式"].tap()
        app.sheets.buttons["撤销"].tap()
        RunLoop.current.run(until: Date().addingTimeInterval(0.4))
        shot("format-06-heading-undo")
        app.buttons["fei.more"].tap()
        app.sheets.buttons["编辑和格式"].tap()
        app.sheets.buttons["重做"].tap()
        RunLoop.current.run(until: Date().addingTimeInterval(0.4))
        shot("format-07-heading-redo")
        XCUIDevice.shared.orientation = .landscapeLeft
        RunLoop.current.run(until: Date().addingTimeInterval(0.4))
        shot("format-07-landscape-heading")
        XCUIDevice.shared.orientation = .portrait
        app.buttons["fei.more"].tap()
        app.sheets.buttons["文档目录"].tap()
        let outline = app.tables["fei.outline.controls"]
        XCTAssertTrue(outline.waitForExistence(timeout: 5),app.debugDescription)
        XCTAssertEqual(outline.cells.count,2)
        shot("format-08-native-outline")
        outline.cells.element(boundBy: 1).tap()
        XCTAssertTrue(XCTWaiter.wait(for: [XCTNSPredicateExpectation(predicate: NSPredicate(format: "exists == false"),object: outline)],timeout: 5) == .completed)
        RunLoop.current.run(until: Date().addingTimeInterval(0.4))
        shot("format-09-outline-jump")
        app.buttons["fei.more"].tap()
        app.sheets.buttons["文档选项"].tap()
        app.sheets.buttons["阅读模式"].tap()
        app.buttons["fei.more"].tap()
        XCTAssertTrue(app.sheets.buttons["文档目录"].waitForExistence(timeout: 5),app.debugDescription)
        XCTAssertFalse(app.sheets.buttons["编辑和格式"].exists)
        app.sheets.buttons["文档目录"].tap()
        XCTAssertTrue(outline.waitForExistence(timeout: 5))
        XCTAssertEqual(app.keyboards.count,0)
        outline.cells.element(boundBy: 0).tap()
        XCTAssertTrue(XCTWaiter.wait(for: [XCTNSPredicateExpectation(predicate: NSPredicate(format: "exists == false"),object: outline)],timeout: 5) == .completed)
        app.navigationBars.buttons.element(boundBy: 0).tap()
        XCTAssertTrue(app.buttons["fei.settings"].waitForExistence(timeout: 5))
        print("FORMAT_TOUCH_COMPLETED=\(stamp)")
    }
}
