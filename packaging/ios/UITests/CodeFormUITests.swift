import XCTest

final class CodeFormUITests: XCTestCase {
    private func shot(_ name: String) {
        let attachment = XCTAttachment(screenshot: XCUIScreen.main.screenshot())
        attachment.name = name; attachment.lifetime = .keepAlways; add(attachment)
    }
    func testNativeCodeControls() {
        continueAfterFailure = false
        let stamp = Int(Date().timeIntervalSince1970)
        print("CODE_TOUCH_STAMP=\(stamp)")
        let app = XCUIApplication()
        app.launchEnvironment["FEI_NOTES_MODE"] = "local"
        XCUIDevice.shared.orientation = .portrait
        app.launch()
        XCTAssertTrue(app.buttons["fei.settings"].waitForExistence(timeout: 15))
        app.buttons["fei.new"].tap()
        XCTAssertTrue(app.buttons["fei.more"].waitForExistence(timeout: 5))
        app.coordinate(withNormalizedOffset: CGVector(dx: 0.3,dy: 0.28)).tap()
        app.buttons["fei.more"].tap()
        app.sheets.buttons["插入内容"].tap()
        let insert = app.tables["fei.insert.controls"]
        XCTAssertTrue(insert.waitForExistence(timeout: 5))
        insert.cells["fei.insert.insert-group-0"].tap()
        let code = insert.cells.containing(.staticText,identifier: "代码块").firstMatch
        for _ in 0..<5 { if code.isHittable { break }; insert.swipeUp() }
        XCTAssertTrue(code.isHittable,app.debugDescription)
        code.tap()
        app.typeText("fn main() {\nprintln!(\"FEI_CODE_\(stamp)\");\n}")
        func open(_ reading: Bool = false) {
            app.buttons["fei.more"].tap()
            app.sheets.buttons[reading ? "代码块工具" : "内容设置"].tap()
            let list = app.tables["fei.objects.controls"]
            XCTAssertTrue(list.waitForExistence(timeout: 5))
            XCTAssertEqual(list.cells.count,1)
            list.cells.element(boundBy: 0).tap()
            XCTAssertTrue(app.tables["fei.object.form"].waitForExistence(timeout: 5))
        }
        func close() {
            app.buttons["fei.object.save"].tap()
            XCTAssertTrue(XCTWaiter.wait(for: [XCTNSPredicateExpectation(predicate: NSPredicate(format: "exists == false"),object: app.tables["fei.object.form"])],timeout: 5) == .completed)
        }
        open()
        let name = app.textFields["fei.object.caption"]
        name.tap()
        app.buttons["清除文本"].firstMatch.tap()
        name.typeText("Shared Rust \(stamp)")
        app.cells["fei.object.row.language"].tap()
        let search = app.searchFields["fei.object.language-search"]
        XCTAssertTrue(search.waitForExistence(timeout: 5),app.debugDescription)
        search.tap(); search.typeText("rust")
        let options = app.tables["fei.object.options"]
        let rust = options.cells["fei.object.option.Rust"]
        XCTAssertTrue(rust.waitForExistence(timeout: 5),app.debugDescription)
        XCTAssertTrue(XCTWaiter.wait(for: [XCTNSPredicateExpectation(predicate: NSPredicate(format: "count == 1"),object: options.cells)],timeout: 5) == .completed)
        shot("code-01-shared-language-search")
        rust.tap()
        let wrap = app.switches["fei.object.wrap"]
        XCTAssertTrue(wrap.waitForExistence(timeout: 5))
        if wrap.value as? String != "1" { wrap.tap() }
        let folded = app.switches["fei.object.folded"]
        folded.tap()
        shot("code-02-native-code-controls")
        close()
        open()
        XCTAssertEqual(name.value as? String,"Shared Rust \(stamp)")
        XCTAssertTrue(app.cells["fei.object.row.language"].staticTexts["Rust"].exists)
        XCTAssertEqual(wrap.value as? String,"1")
        XCTAssertEqual(folded.value as? String,"1")
        folded.tap()
        close()
        app.buttons["fei.more"].tap()
        app.sheets.buttons["重命名"].tap()
        let title = app.textFields["fei.rename.title"]
        XCTAssertTrue(title.waitForExistence(timeout: 5))
        title.typeText("FEI_CODE_CONTROLS_\(stamp)")
        app.buttons["fei.rename.save"].tap()
        app.buttons["fei.more"].tap()
        app.sheets.buttons["笔记选项"].tap()
        app.sheets.buttons["阅读模式"].tap()
        open(true)
        XCTAssertFalse(name.isEnabled)
        XCTAssertFalse(app.cells["fei.object.row.language"].isEnabled)
        XCTAssertFalse(wrap.isEnabled)
        XCTAssertTrue(folded.isEnabled)
        folded.tap()
        XCTAssertEqual(folded.value as? String,"1")
        XCTAssertTrue(app.cells["fei.object.row.copy"].exists)
        shot("code-03-reading-controls")
        close()
        app.navigationBars.buttons["返回"].firstMatch.tap()
        XCTAssertTrue(app.buttons["fei.settings"].waitForExistence(timeout: 5))
        print("CODE_TOUCH_COMPLETED=\(stamp)")
    }
}
