import XCTest

final class BatchUITests: XCTestCase {
    private func shot(_ name: String) {
        let attachment = XCTAttachment(screenshot: XCUIScreen.main.screenshot())
        attachment.name = name; attachment.lifetime = .keepAlways; add(attachment)
    }
    func testNativeBatchCopyMoveFolderAndDelete() {
        continueAfterFailure = false
        let prefix = "FEI_BATCH23_1794446635"
        let app = XCUIApplication()
        app.launchEnvironment["FEI_NOTES_MODE"] = "local"
        XCUIDevice.shared.orientation = .portrait
        app.launch()
        XCTAssertTrue(app.buttons["fei.settings"].waitForExistence(timeout: 15))
        func home() { app.navigationBars.buttons["返回"].firstMatch.tap(); XCTAssertTrue(app.buttons["fei.settings"].waitForExistence(timeout: 5)) }
        for kind in ["A","B"] {
            app.buttons["fei.library-actions"].tap(); app.sheets.buttons["导入笔记"].tap()
            let file = app.cells.matching(NSPredicate(format: "label CONTAINS %@",prefix+"_"+kind)).firstMatch
            if !file.waitForExistence(timeout: 2) {
                if app.buttons["浏览"].firstMatch.waitForExistence(timeout: 2) { app.buttons["浏览"].firstMatch.tap() }
                if !file.waitForExistence(timeout: 2) {
                    let folder = app.cells.matching(NSPredicate(format: "label BEGINSWITH %@","飞记")).firstMatch
                    if !folder.waitForExistence(timeout: 2) {
                        let phone = app.cells.matching(NSPredicate(format: "label CONTAINS %@","iPhone")).firstMatch
                        XCTAssertTrue(phone.waitForExistence(timeout: 5)); phone.tap()
                    }
                    XCTAssertTrue(folder.waitForExistence(timeout: 5)); folder.tap()
                }
            }
            XCTAssertTrue(file.waitForExistence(timeout: 8)); file.tap()
            if app.buttons["打开"].firstMatch.waitForExistence(timeout: 2) { app.buttons["打开"].firstMatch.tap() }
            XCTAssertTrue(app.navigationBars[prefix+"_"+kind].waitForExistence(timeout: 10)); home()
        }
        app.buttons["fei.library-actions"].tap(); app.sheets.buttons["新建文件夹"].tap()
        let rename = app.textFields["fei.rename.title"]
        XCTAssertTrue(rename.waitForExistence(timeout: 5)); rename.typeText(prefix+"_TARGET")
        app.buttons["fei.rename.save"].tap()
        XCTAssertTrue(app.buttons["fei.settings"].waitForExistence(timeout: 5))
        app.coordinate(withNormalizedOffset: CGVector(dx: 0.3,dy: 0.155)).tap()
        XCTAssertTrue(app.keyboards.firstMatch.waitForExistence(timeout: 5)); app.typeText(prefix)
        func batch() {
            app.buttons["fei.library-actions"].tap(); app.sheets.buttons["选择笔记"].tap()
            XCTAssertTrue(app.tables["fei.batch.controls"].waitForExistence(timeout: 5),app.debugDescription)
        }
        func select(_ title: String) {
            let row = app.tables["fei.batch.controls"].cells.containing(.staticText,identifier: title).firstMatch
            XCTAssertTrue(row.waitForExistence(timeout: 5),app.debugDescription); row.tap()
        }
        func closed() { XCTAssertTrue(XCTWaiter.wait(for: [XCTNSPredicateExpectation(predicate: NSPredicate(format: "exists == false"),object: app.tables["fei.batch.controls"])],timeout: 5) == .completed) }
        batch()
        XCTAssertFalse(app.buttons["fei.batch.copy"].isEnabled)
        XCTAssertFalse(app.buttons["fei.batch.move"].isEnabled)
        XCTAssertFalse(app.buttons["fei.batch.delete"].isEnabled)
        app.cells["fei.batch.all"].tap()
        XCTAssertTrue(app.buttons["fei.batch.copy"].isEnabled)
        shot("batch-01-native-selection-and-toolbar")
        app.cells["fei.batch.all"].tap()
        XCTAssertFalse(app.buttons["fei.batch.copy"].isEnabled)
        app.buttons["fei.batch.done"].tap(); closed()
        batch(); select(prefix+"_A"); select(prefix+"_B")
        shot("batch-02-two-note-selection")
        app.buttons["fei.batch.copy"].tap(); closed()
        batch(); select(prefix+"_A"); select(prefix+"_B")
        app.buttons["fei.batch.move"].tap()
        XCTAssertTrue(app.navigationBars["移动到"].waitForExistence(timeout: 5),app.debugDescription)
        let target = app.cells.containing(.staticText,identifier: prefix+"_TARGET").firstMatch
        XCTAssertTrue(target.waitForExistence(timeout: 5)); shot("batch-03-native-destination-picker"); target.tap()
        XCTAssertTrue(app.buttons["fei.library-actions"].waitForExistence(timeout: 5))
        batch(); select(prefix+"_TARGET"); app.buttons["fei.batch.copy"].tap(); closed()
        batch(); select(prefix+"_TARGET 副本"); app.buttons["fei.batch.delete"].tap()
        XCTAssertTrue(app.alerts["fei.confirm"].waitForExistence(timeout: 5)); shot("batch-04-folder-delete-confirmation")
        app.alerts.buttons["fei.confirm.cancel"].firstMatch.tap()
        batch(); select(prefix+"_TARGET 副本"); app.buttons["fei.batch.delete"].tap()
        XCTAssertTrue(app.alerts["fei.confirm"].waitForExistence(timeout: 5)); app.alerts.buttons["fei.confirm.ok"].firstMatch.tap()
        XCTAssertTrue(app.buttons["fei.settings"].waitForExistence(timeout: 5))
        print("BATCH23_COMPLETED=\(prefix)")
    }
}
