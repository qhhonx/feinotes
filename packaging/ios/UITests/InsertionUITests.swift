import XCTest

final class InsertionUITests: XCTestCase {
    private func shot(_ name: String) {
        let attachment = XCTAttachment(screenshot: XCUIScreen.main.screenshot())
        attachment.name = name
        attachment.lifetime = .keepAlways
        add(attachment)
    }
    func testSharedInsertionCatalogueAndRichContainers() {
        continueAfterFailure = false
        let stamp = Int(Date().timeIntervalSince1970)
        print("INSERT_TOUCH_STAMP=\(stamp)")
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
        func insertMenu() -> XCUIElement {
            app.buttons["fei.more"].tap()
            app.sheets.buttons["插入内容"].tap()
            let menu = app.tables["fei.insert.controls"]
            XCTAssertTrue(menu.waitForExistence(timeout: 5),app.debugDescription)
            return menu
        }
        func tap(_ cell: XCUIElement,in table: XCUIElement) {
            for _ in 0..<8 {
                if cell.isHittable { break }
                if cell.exists && cell.frame.minY < table.frame.minY { table.swipeDown() } else { table.swipeUp() }
            }
            XCTAssertTrue(cell.isHittable,app.debugDescription)
            cell.tap()
        }
        func category(_ index: Int,in table: XCUIElement) { tap(table.cells["fei.insert.insert-group-\(index)"],in: table) }
        func choose(_ label: String,in table: XCUIElement) { tap(table.cells.containing(.staticText,identifier: label).firstMatch,in: table) }
        func backFromCategory() { app.navigationBars["fei.insert.navigation"].buttons.element(boundBy: 0).tap() }
        func finish(_ title: String) {
            XCTAssertTrue(app.buttons["fei.more"].waitForExistence(timeout: 5))
            shot("insert-result-\(title)")
            app.buttons["fei.more"].tap()
            app.sheets.buttons["重命名"].tap()
            let field = app.textFields["fei.rename.title"]
            XCTAssertTrue(field.waitForExistence(timeout: 5))
            field.typeText("\(title)_\(stamp)")
            app.buttons["fei.rename.save"].tap()
            let back = app.navigationBars.buttons["返回"].firstMatch
            XCTAssertTrue(back.waitForExistence(timeout: 5),app.debugDescription)
            back.tap()
            XCTAssertTrue(app.buttons["fei.settings"].waitForExistence(timeout: 5))
        }
        newNote()
        let menu = insertMenu()
        XCTAssertTrue(menu.cells["fei.insert.image"].exists)
        XCTAssertTrue(menu.cells["fei.insert.file"].exists)
        shot("insert-01-native-catalogue")
        category(1,in: menu)
        XCTAssertTrue(menu.cells.containing(.staticText,identifier: "分栏").firstMatch.exists)
        XCTAssertTrue(menu.cells.containing(.staticText,identifier: "公式").firstMatch.exists)
        XCTAssertTrue(menu.cells.containing(.staticText,identifier: "按钮").firstMatch.exists)
        shot("insert-02-shared-common-content")
        backFromCategory()
        category(2,in: menu)
        XCTAssertTrue(menu.cells.containing(.staticText,identifier: "日期提醒").firstMatch.exists)
        backFromCategory()
        category(3,in: menu)
        XCTAssertTrue(menu.cells.containing(.staticText,identifier: "倒计时").firstMatch.exists)
        backFromCategory()
        category(0,in: menu)
        choose("高亮块",in: menu)
        XCTAssertTrue(app.buttons["fei.more"].waitForExistence(timeout: 5))
        XCTAssertTrue(app.keyboards.firstMatch.waitForExistence(timeout: 5))
        app.typeText("Callout body \(stamp)")
        finish("FEI_INSERT_CALLOUT")
        newNote()
        let gridMenu = insertMenu()
        category(1,in: gridMenu)
        choose("分栏",in: gridMenu)
        XCTAssertTrue(app.buttons["fei.more"].waitForExistence(timeout: 5))
        XCTAssertTrue(app.keyboards.firstMatch.waitForExistence(timeout: 5))
        app.typeText("First column \(stamp)")
        finish("FEI_INSERT_GRID")
        newNote()
        let codeMenu = insertMenu()
        category(0,in: codeMenu)
        choose("代码块",in: codeMenu)
        XCTAssertTrue(app.buttons["fei.more"].waitForExistence(timeout: 5))
        XCTAssertTrue(app.keyboards.firstMatch.waitForExistence(timeout: 5))
        app.typeText("first line\nsecond line")
        finish("FEI_INSERT_CODE")
        print("INSERT_TOUCH_COMPLETED=\(stamp)")
    }
}
