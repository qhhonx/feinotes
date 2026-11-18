import XCTest

final class ObjectFormUITests: XCTestCase {
    private func shot(_ name: String) {
        let attachment = XCTAttachment(screenshot: XCUIScreen.main.screenshot())
        attachment.name = name
        attachment.lifetime = .keepAlways
        add(attachment)
    }
    func testNativeEquationAndButtonForms() {
        continueAfterFailure = false
        let stamp = Int(Date().timeIntervalSince1970)
        print("OBJECT_TOUCH_STAMP=\(stamp)")
        let app = XCUIApplication()
        app.launchEnvironment["FEI_NOTES_MODE"] = "local"
        XCUIDevice.shared.orientation = .portrait
        app.launch()
        XCTAssertTrue(app.buttons["fei.settings"].waitForExistence(timeout: 15))
        func newNote() {
            app.buttons["fei.new"].tap()
            XCTAssertTrue(app.buttons["fei.more"].waitForExistence(timeout: 5))
            app.coordinate(withNormalizedOffset: CGVector(dx: 0.3,dy: 0.28)).tap()
        }
        func insert(_ label: String) {
            app.buttons["fei.more"].tap()
            app.sheets.buttons["插入内容"].tap()
            let menu = app.tables["fei.insert.controls"]
            XCTAssertTrue(menu.waitForExistence(timeout: 5))
            menu.cells["fei.insert.insert-group-1"].tap()
            menu.cells.containing(.staticText,identifier: label).firstMatch.tap()
            XCTAssertTrue(app.tables["fei.object.form"].waitForExistence(timeout: 5),app.debugDescription)
        }
        func close(_ cancel: Bool = false) {
            app.buttons[cancel ? "fei.object.cancel" : "fei.object.save"].tap()
            XCTAssertTrue(XCTWaiter.wait(for: [XCTNSPredicateExpectation(predicate: NSPredicate(format: "exists == false"),object: app.tables["fei.object.form"])],timeout: 5) == .completed)
        }
        func openObject() {
            app.buttons["fei.more"].tap()
            app.sheets.buttons["内容设置"].tap()
            let objects = app.tables["fei.objects.controls"]
            XCTAssertTrue(objects.waitForExistence(timeout: 5),app.debugDescription)
            XCTAssertEqual(objects.cells.count,1)
            objects.cells.element(boundBy: 0).tap()
            XCTAssertTrue(app.tables["fei.object.form"].waitForExistence(timeout: 5))
        }
        func finish(_ title: String) {
            app.buttons["fei.more"].tap()
            app.sheets.buttons["重命名"].tap()
            let name = app.textFields["fei.rename.title"]
            XCTAssertTrue(name.waitForExistence(timeout: 5))
            name.typeText("\(title)_\(stamp)")
            app.buttons["fei.rename.save"].tap()
            app.navigationBars.buttons["返回"].firstMatch.tap()
            XCTAssertTrue(app.buttons["fei.settings"].waitForExistence(timeout: 5))
        }
        func replace(_ field: XCUIElement,_ text: String) {
            field.tap()
            let clear = app.buttons["清除文本"].firstMatch
            if clear.waitForExistence(timeout: 1) { clear.tap() }
            field.typeText(text)
        }
        func choose(_ key: String,_ value: String) {
            let row = app.cells["fei.object.row.\(key)"]
            if !row.isHittable { app.tables["fei.object.form"].swipeUp() }
            row.tap()
            let options = app.tables["fei.object.options"]
            XCTAssertTrue(options.waitForExistence(timeout: 5))
            let cell = options.cells["fei.object.option.\(value)"]
            for _ in 0..<8 { if cell.isHittable { break }; options.swipeUp() }
            XCTAssertTrue(cell.isHittable,app.debugDescription)
            cell.tap()
            XCTAssertTrue(app.tables["fei.object.form"].waitForExistence(timeout: 5))
        }
        newNote()
        insert("公式")
        let source = app.textViews["fei.object.source"]
        XCTAssertTrue(source.waitForExistence(timeout: 5))
        source.typeText("x^2")
        shot("objects-01-live-equation-form")
        close()
        openObject()
        XCTAssertEqual(source.value as? String,"x^2")
        source.typeText("x^2+1")
        close()
        shot("objects-02-shared-equation-render")
        finish("FEI_OBJECT_EQUATION")
        newNote()
        insert("按钮")
        choose("action","DuplicatePage")
        let text = app.textFields["fei.object.text"]
        XCTAssertEqual(text.value as? String,"创建副本")
        XCTAssertFalse(app.textFields["fei.object.url"].exists)
        choose("action","OpenLink")
        XCTAssertEqual(text.value as? String,"打开超链接")
        replace(text,"FEI_BUTTON_\(stamp)")
        let url = app.textFields["fei.object.url"]
        replace(url,"bad url")
        app.buttons["fei.object.save"].tap()
        let error = app.staticTexts["fei.object.error"]
        XCTAssertTrue(error.waitForExistence(timeout: 5),app.debugDescription)
        XCTAssertTrue(XCTWaiter.wait(for: [XCTNSPredicateExpectation(predicate: NSPredicate(format: "hittable == true"),object: error)],timeout: 5) == .completed)
        shot("objects-03-shared-button-validation")
        replace(url,"example.com")
        choose("color","deep-green")
        close()
        openObject()
        XCTAssertEqual(text.value as? String,"FEI_BUTTON_\(stamp)")
        XCTAssertEqual(url.value as? String,"http://example.com")
        replace(text,"DISCARDED_DRAFT_\(stamp)")
        close(true)
        openObject()
        XCTAssertEqual(text.value as? String,"FEI_BUTTON_\(stamp)")
        choose("action","DuplicatePage")
        XCTAssertFalse(url.exists)
        close()
        app.buttons["fei.more"].tap()
        app.sheets.buttons["编辑和格式"].tap()
        app.sheets.buttons["撤销"].tap()
        openObject()
        XCTAssertEqual(text.value as? String,"FEI_BUTTON_\(stamp)")
        XCTAssertEqual(url.value as? String,"http://example.com")
        shot("objects-04-shared-button-after-undo")
        close(true)
        finish("FEI_OBJECT_BUTTON")
        print("OBJECT_TOUCH_COMPLETED=\(stamp)")
    }
}
