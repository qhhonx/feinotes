import XCTest

final class ProductBaselineUITests: XCTestCase {
    private func shot(_ name: String) {
        let attachment = XCTAttachment(screenshot: XCUIScreen.main.screenshot())
        attachment.name = name; attachment.lifetime = .keepAlways; add(attachment)
    }
    func testLatestProductCreationAndMarkdownColumnAlignment() {
        continueAfterFailure = false
        let prefix = "FEI_PRODUCT24_" + String(Int(Date().timeIntervalSince1970))
        print("PRODUCT24_PREFIX=\(prefix)")
        let app = XCUIApplication()
        app.launchEnvironment["FEI_NOTES_MODE"] = "local"
        XCUIDevice.shared.orientation = .portrait
        app.launch()
        XCTAssertTrue(app.buttons["fei.settings"].waitForExistence(timeout: 15))
        func name(_ suffix: String) {
            app.buttons["fei.more"].tap(); app.sheets.buttons["重命名"].tap()
            let field = app.textFields["fei.rename.title"]
            XCTAssertTrue(field.waitForExistence(timeout: 5)); field.typeText(prefix+suffix)
            app.buttons["fei.rename.save"].tap()
            XCTAssertTrue(app.navigationBars[prefix+suffix].waitForExistence(timeout: 5))
        }
        func body(_ suffix: String) {
            app.coordinate(withNormalizedOffset: CGVector(dx: 0.3,dy: 0.28)).tap(); app.typeText(prefix+suffix)
        }
        func home() { app.navigationBars.buttons["返回"].firstMatch.tap(); XCTAssertTrue(app.buttons["fei.settings"].waitForExistence(timeout: 5)) }
        app.buttons["fei.library-actions"].tap(); app.sheets.buttons["新建 Markdown"].tap()
        XCTAssertTrue(app.buttons["fei.more"].waitForExistence(timeout: 5)); body("_MD_BODY"); name("_MD")
        app.buttons["fei.more"].tap(); app.sheets.buttons["插入内容"].tap()
        XCTAssertTrue(app.tables["fei.insert.controls"].waitForExistence(timeout: 5)); app.cells["fei.insert.table"].tap()
        XCTAssertTrue(app.keyboards.firstMatch.waitForExistence(timeout: 5)); app.typeText(prefix+"_CELL")
        app.buttons["fei.more"].tap(); app.sheets.buttons["表格操作"].tap()
        let controls = app.tables["fei.table.controls"]
        XCTAssertTrue(controls.waitForExistence(timeout: 5))
        controls.cells["fei.table.column-after"].tap()
        let paragraph = controls.cells["fei.table.cell-paragraph"]
        for _ in 0..<6 { if paragraph.isHittable { break }; controls.swipeUp() }
        XCTAssertTrue(paragraph.isHittable,app.debugDescription); paragraph.tap()
        let center = controls.cells["fei.table.format:align-center"]
        XCTAssertTrue(center.waitForExistence(timeout: 5),app.debugDescription); center.tap()
        shot("product-01-markdown-column-alignment")
        app.buttons["fei.table.done"].tap(); shot("product-02-shared-table-and-touch-scrollbar"); home()
        app.buttons["fei.new"].tap(); XCTAssertTrue(app.buttons["fei.more"].waitForExistence(timeout: 5)); body("_QUICK_BODY"); name("_QUICK"); home()
        app.buttons["fei.library-actions"].tap(); app.sheets.buttons["新建文档"].tap()
        XCTAssertTrue(app.buttons["fei.more"].waitForExistence(timeout: 5)); body("_DOC_BODY"); name("_DOC"); home()
        app.buttons["fei.library-actions"].tap(); app.sheets.buttons["新建文件夹"].tap()
        let field = app.textFields["fei.rename.title"]
        XCTAssertTrue(field.waitForExistence(timeout: 5)); field.typeText(prefix+"_FOLDER"); app.buttons["fei.rename.save"].tap()
        XCTAssertTrue(app.navigationBars[prefix+"_FOLDER"].waitForExistence(timeout: 5),app.debugDescription)
        app.buttons["fei.new"].tap(); XCTAssertTrue(app.buttons["fei.more"].waitForExistence(timeout: 5)); body("_CHILD_BODY"); name("_CHILD")
        app.navigationBars.buttons["返回"].firstMatch.tap()
        XCTAssertTrue(app.navigationBars[prefix+"_FOLDER"].waitForExistence(timeout: 5)); home()
        app.buttons["fei.settings"].tap(); app.coordinate(withNormalizedOffset: CGVector(dx: 0.3,dy: 0.167)).tap()
        XCTAssertTrue(app.navigationBars["通用"].waitForExistence(timeout: 5)); shot("product-03-shared-default-type-preference")
        app.navigationBars.buttons["完成"].tap()
        XCTAssertTrue(app.buttons["fei.settings"].waitForExistence(timeout: 5))
        print("PRODUCT24_COMPLETED=\(prefix)")
    }
}
