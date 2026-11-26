import XCTest

final class FindUITests: XCTestCase {
    private func shot(_ name: String) {
        let attachment = XCTAttachment(screenshot: XCUIScreen.main.screenshot())
        attachment.name = name
        attachment.lifetime = .keepAlways
        add(attachment)
    }
    func testNativeFindNavigator() {
        continueAfterFailure = false
        let stamp = Int(Date().timeIntervalSince1970)
        print("FIND_TOUCH_STAMP=\(stamp)")
        let app = XCUIApplication()
        app.launchEnvironment["FEI_NOTES_MODE"] = "local"
        app.launch()
        XCTAssertTrue(app.buttons["fei.settings"].waitForExistence(timeout: 15))
        app.buttons["fei.new"].tap()
        XCTAssertTrue(app.buttons["fei.more"].waitForExistence(timeout: 5))
        app.coordinate(withNormalizedOffset: CGVector(dx: 0.3,dy: 0.28)).tap()
        app.typeText("Alpha alpha beta \(stamp)\nAlpha omega")
        app.buttons["fei.more"].tap()
        app.sheets.buttons["重命名"].tap()
        let title = app.textFields["fei.rename.title"]
        XCTAssertTrue(title.waitForExistence(timeout: 5))
        title.typeText("Alpha Find \(stamp)")
        app.buttons["fei.rename.save"].tap()
        XCTAssertTrue(app.navigationBars["Alpha Find \(stamp)"].waitForExistence(timeout: 5))
        app.buttons["fei.more"].tap()
        app.sheets.buttons["文档内查找"].tap()
        let query = app.searchFields.firstMatch
        XCTAssertTrue(query.waitForExistence(timeout: 10),app.debugDescription)
        query.typeText("alpha")
        RunLoop.current.run(until: Date().addingTimeInterval(0.5))
        shot("find-01-system-navigator")
        let counter = app.staticTexts["find.resultLabel"]
        XCTAssertTrue(counter.waitForExistence(timeout: 5))
        XCTAssertTrue(counter.label.hasSuffix("/4"),app.debugDescription)
        let initial = counter.label
        app.buttons["find.nextButton"].firstMatch.tap()
        XCTAssertNotEqual(counter.label,initial)
        app.buttons["find.previousButton"].firstMatch.tap()
        XCTAssertEqual(counter.label,initial)
        app.buttons["find.settingsButton"].firstMatch.tap()
        let mode = app.buttons["查找与替换"].firstMatch
        XCTAssertTrue(mode.waitForExistence(timeout: 5),app.debugDescription)
        mode.tap()
        let replacement = app.textFields["find.replaceField"]
        XCTAssertTrue(replacement.waitForExistence(timeout: 5),app.debugDescription)
        replacement.tap()
        replacement.typeText("Delta")
        shot("find-02-system-replacement")
        XCTAssertTrue(app.buttons["find.replaceButton"].firstMatch.isEnabled,app.debugDescription)
        app.buttons["find.replaceButton"].firstMatch.tap()
        XCTAssertTrue(XCTWaiter.wait(for: [XCTNSPredicateExpectation(predicate: NSPredicate(format: "label ENDSWITH %@","/3"),object: counter)],timeout: 5) == .completed,app.debugDescription)
        shot("find-03-single-replacement")
        app.buttons["find.replaceButton"].firstMatch.press(forDuration: 0.8)
        let all = app.buttons["全部替换"].firstMatch
        XCTAssertTrue(all.waitForExistence(timeout: 5),app.debugDescription)
        all.tap()
        XCTAssertTrue(app.navigationBars["Delta Find \(stamp)"].waitForExistence(timeout: 5))
        func setQuery(_ value: String) {
            let field = app.searchFields["find.searchField"]
            if !NSPredicate(format: "hasKeyboardFocus == true").evaluate(with: field) { field.tap() }
            let clear = field.buttons["清除文本"].firstMatch
            if clear.exists { clear.tap() }
            field.typeText(value)
        }
        func count(_ expected: Int) {
            XCTAssertTrue(XCTWaiter.wait(for: [XCTNSPredicateExpectation(predicate: NSPredicate(format: "label ENDSWITH %@","/\(expected)"),object: counter)],timeout: 5) == .completed,app.debugDescription)
        }
        func closeFind() {
            app.buttons["find.doneButton"].firstMatch.tap()
            XCTAssertTrue(XCTWaiter.wait(for: [XCTNSPredicateExpectation(predicate: NSPredicate(format: "exists == false"),object: app.searchFields["find.searchField"])],timeout: 5) == .completed)
        }
        setQuery("Delta")
        count(4)
        shot("find-04-all-replacements")
        closeFind()
        app.buttons["fei.more"].tap()
        app.sheets.buttons["编辑和格式"].tap()
        app.sheets.buttons["撤销"].tap()
        XCTAssertTrue(app.navigationBars["Alpha Find \(stamp)"].waitForExistence(timeout: 5))
        app.buttons["fei.more"].tap()
        app.sheets.buttons["文档内查找"].tap()
        XCTAssertTrue(app.searchFields["find.searchField"].waitForExistence(timeout: 5))
        setQuery("alpha")
        count(3)
        shot("find-05-replace-all-undo")
        closeFind()
        app.switchToReadingMode()
        app.buttons["fei.more"].tap()
        app.sheets.buttons["文档内查找"].tap()
        XCTAssertTrue(app.searchFields["find.searchField"].waitForExistence(timeout: 5))
        count(3)
        app.buttons["find.settingsButton"].firstMatch.tap()
        XCTAssertTrue(app.buttons["查找"].firstMatch.waitForExistence(timeout: 5))
        XCTAssertFalse(app.buttons["查找与替换"].exists)
        shot("find-06-read-only-options")
        app.coordinate(withNormalizedOffset: CGVector(dx: 0.01,dy: 0.3)).tap()
        XCUIDevice.shared.orientation = .landscapeLeft
        XCTAssertTrue(app.searchFields["find.searchField"].waitForExistence(timeout: 5))
        count(3)
        shot("find-07-landscape")
        XCUIDevice.shared.orientation = .portrait
        closeFind()
        app.navigationBars.buttons.element(boundBy: 0).tap()
        XCTAssertTrue(app.buttons["fei.settings"].waitForExistence(timeout: 5))
        print("FIND_TOUCH_COMPLETED=\(stamp)")
    }
    func testSoftwareReturnUsesSharedEditorAndSubmitCommands() {
        continueAfterFailure = false
        let stamp = Int(Date().timeIntervalSince1970)
        print("RETURN_TOUCH_STAMP=\(stamp)")
        let app = XCUIApplication()
        app.launchEnvironment["FEI_NOTES_MODE"] = "local"
        app.launch()
        XCTAssertTrue(app.buttons["fei.settings"].waitForExistence(timeout: 15))
        func create() {
            app.buttons["fei.new"].tap()
            XCTAssertTrue(app.buttons["fei.more"].waitForExistence(timeout: 5))
            app.coordinate(withNormalizedOffset: CGVector(dx: 0.3,dy: 0.28)).tap()
        }
        func rename(_ value: String) {
            app.buttons["fei.more"].tap()
            app.sheets.buttons["重命名"].tap()
            let field = app.textFields["fei.rename.title"]
            XCTAssertTrue(field.waitForExistence(timeout: 5))
            field.typeText(value)
            app.buttons["fei.rename.save"].tap()
            XCTAssertTrue(app.navigationBars[value].waitForExistence(timeout: 5))
        }
        func home() {
            app.navigationBars.buttons.element(boundBy: 0).tap()
            XCTAssertTrue(app.buttons["fei.settings"].waitForExistence(timeout: 5))
        }
        create()
        app.typeText("- One\nTwo\n\nAfter \(stamp)")
        rename("FEI_RETURN_LIST_\(stamp)")
        shot("return-01-list-continuation-and-exit")
        home()
        create()
        app.buttons["fei.more"].tap()
        app.sheets.buttons["编辑和格式"].tap()
        app.sheets.buttons["段落类型"].tap()
        let controls = app.tables["fei.format.controls"]
        XCTAssertTrue(controls.waitForExistence(timeout: 5))
        let code = controls.cells["fei.format.code"]
        for _ in 0..<6 { if code.isHittable { break }; controls.swipeUp() }
        XCTAssertTrue(code.isHittable)
        code.tap()
        app.buttons["fei.format.done"].tap()
        shot("return-02-empty-code")
        app.coordinate(withNormalizedOffset: CGVector(dx: 0.3,dy: 0.335)).tap()
        app.typeText("  first\nsecond")
        rename("FEI_RETURN_CODE_\(stamp)")
        shot("return-02-shared-code-indent")
        home()
        app.coordinate(withNormalizedOffset: CGVector(dx: 0.3,dy: 0.155)).tap()
        XCTAssertTrue(app.keyboards.firstMatch.waitForExistence(timeout: 5))
        app.typeText("fei_return_list_\(stamp)\n")
        XCTAssertTrue(app.navigationBars["FEI_RETURN_LIST_\(stamp)"].waitForExistence(timeout: 5),app.debugDescription)
        shot("return-03-single-line-submit")
        home()
        print("RETURN_TOUCH_COMPLETED=\(stamp)")
    }

}
