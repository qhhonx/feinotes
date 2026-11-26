import XCTest

final class EditorToolbarUITests: XCTestCase {
    private func shot(_ name:String) {
        let a = XCTAttachment(screenshot: XCUIScreen.main.screenshot()); a.name = name; a.lifetime = .keepAlways; add(a)
    }
    func testNativeToolbarKeyboardMenusAndReadingMode() {
        continueAfterFailure = false
        let app = XCUIApplication()
        app.launchEnvironment["FEI_NOTES_MODE"] = "local"
        XCUIDevice.shared.orientation = .portrait
        app.launch()
        XCTAssertTrue(app.buttons["fei.new"].waitForExistence(timeout: 15)); app.buttons["fei.new"].tap()
        let bar = app.toolbars["fei.editor.toolbar"]
        XCTAssertTrue(bar.waitForExistence(timeout: 5),app.debugDescription)
        func keyboard(_ visible:Bool) {
            let item = app.keyboards.firstMatch
            let expected = XCTNSPredicateExpectation(predicate: NSPredicate { _,_ in
                let onscreen = item.exists && item.isHittable && item.frame.minY < app.frame.maxY-100
                return onscreen == visible
            },object: item)
            XCTAssertEqual(XCTWaiter.wait(for: [expected],timeout: 6),.completed,app.debugDescription)
        }
        keyboard(true)
        for name in ["undo","list","styles","insert","keyboard"] { XCTAssertTrue(app.buttons["fei.edit."+name].isHittable) }
        shot("toolbar-01-native-icons-above-keyboard")
        bar.buttons["fei.edit.keyboard"].tap(); keyboard(false)
        bar.buttons["fei.edit.keyboard"].tap(); keyboard(true)
        let prefix = "FEI_TOOLBAR31_"+String(Int(Date().timeIntervalSince1970))
        app.coordinate(withNormalizedOffset: CGVector(dx: 0.3,dy: 0.28)).tap(); app.typeText(prefix+" body")
        bar.buttons["fei.edit.keyboard"].tap(); keyboard(false)
        app.buttons["fei.more"].tap(); app.sheets.buttons["重命名"].tap()
        let title = app.textFields["fei.rename.title"]
        XCTAssertTrue(title.waitForExistence(timeout: 5)); title.typeText(prefix); app.buttons["fei.rename.save"].tap()
        XCTAssertTrue(app.navigationBars[prefix].waitForExistence(timeout: 5))
        bar.buttons["fei.edit.styles"].tap()
        XCTAssertTrue(app.tables["fei.format.controls"].waitForExistence(timeout: 5),app.debugDescription)
        app.buttons["fei.format.done"].tap()
        bar.buttons["fei.edit.insert"].tap()
        XCTAssertTrue(app.tables["fei.insert.controls"].waitForExistence(timeout: 5),app.debugDescription)
        app.buttons["fei.insert.done"].tap()
        app.switchToReadingMode()
        XCTAssertTrue(app.buttons["fei.note-mode"].waitForExistence(timeout: 5),app.debugDescription)
        XCTAssertFalse(bar.buttons["fei.edit.mode"].exists)
        XCTAssertFalse(bar.buttons["fei.edit.insert"].exists)
        let settled = Date().addingTimeInterval(2)
        let repaint = XCTNSPredicateExpectation(predicate: NSPredicate { _,_ in Date() >= settled },object: nil)
        XCTAssertEqual(XCTWaiter.wait(for: [repaint],timeout: 3),.completed)
        shot("toolbar-02-reading-mode")
        app.switchToEditingMode(); keyboard(false)
        let idleKeyboard = bar.buttons["fei.edit.mode"]
        XCTAssertTrue(idleKeyboard.waitForExistence(timeout: 5))
        idleKeyboard.tap(); keyboard(true)
        XCTAssertTrue(bar.buttons["fei.edit.insert"].isHittable)
        shot("toolbar-03-return-to-editing")
        bar.buttons["fei.edit.keyboard"].tap(); keyboard(false)
        app.buttons["fei.more"].tap(); app.sheets.buttons["文档内查找"].tap()
        XCTAssertTrue(app.searchFields.firstMatch.waitForExistence(timeout: 5))
        XCTAssertFalse(bar.isHittable)
        app.buttons["find.doneButton"].firstMatch.tap()
        XCTAssertTrue(bar.buttons["fei.edit.styles"].waitForExistence(timeout: 5))
        app.navigationBars.buttons.element(boundBy: 0).tap()
        let row = app.tables["fei.notes.list"].cells.containing(.staticText,identifier: prefix).firstMatch
        XCTAssertTrue(row.waitForExistence(timeout: 5),app.debugDescription)
        XCTAssertTrue(row.staticTexts.matching(NSPredicate(format: "label CONTAINS %@",prefix+" body")).firstMatch.exists,app.debugDescription)
        print("TOOLBAR31_COMPLETED="+prefix)
    }
}
