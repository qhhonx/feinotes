import XCTest

final class LocalHistoryUITests: XCTestCase {
    private func shot(_ name:String) {
        let a = XCTAttachment(screenshot: XCUIScreen.main.screenshot()); a.name = name; a.lifetime = .keepAlways; add(a)
    }
    func testHistoryRetentionSettings() {
        continueAfterFailure = false
        let app = XCUIApplication(); app.launchEnvironment["FEI_NOTES_MODE"] = "local"
        XCUIDevice.shared.orientation = .portrait; app.launch()
        XCTAssertTrue(app.buttons["fei.settings"].waitForExistence(timeout: 15)); app.buttons["fei.settings"].tap()
        XCTAssertTrue(app.navigationBars["设置"].waitForExistence(timeout: 5))
        app.coordinate(withNormalizedOffset: CGVector(dx: 0.5,dy: 0.304)).tap()
        XCTAssertTrue(app.navigationBars["文档库"].waitForExistence(timeout: 5),app.debugDescription)
        shot("history34-07-retention-settings")
        app.coordinate(withNormalizedOffset: CGVector(dx: 0.862,dy: 0.43)).tap()
        app.coordinate(withNormalizedOffset: CGVector(dx: 0.862,dy: 0.43)).tap()
        app.coordinate(withNormalizedOffset: CGVector(dx: 0.594,dy: 0.43)).tap()
        shot("history34-08-retention-increment-decrement")
        app.terminate(); app.launch()
        XCTAssertTrue(app.buttons["fei.settings"].waitForExistence(timeout: 15)); app.buttons["fei.settings"].tap()
        XCTAssertTrue(app.navigationBars["设置"].waitForExistence(timeout: 5))
        app.coordinate(withNormalizedOffset: CGVector(dx: 0.5,dy: 0.304)).tap()
        XCTAssertTrue(app.navigationBars["文档库"].waitForExistence(timeout: 5))
        shot("history34-09-retention-after-relaunch")
    }
    func testNativeHistoryPreviewRestoreCopyAndManage() {
        continueAfterFailure = false
        let app = XCUIApplication(); app.launchEnvironment["FEI_NOTES_MODE"] = "local"
        XCUIDevice.shared.orientation = .portrait; app.launch()
        XCTAssertTrue(app.buttons["fei.new"].waitForExistence(timeout: 15)); app.buttons["fei.new"].tap()
        XCTAssertTrue(app.buttons["fei.more"].waitForExistence(timeout: 5))
        let stamp = String(Int(Date().timeIntervalSince1970)); let prefix = "H34_"+stamp
        app.coordinate(withNormalizedOffset: CGVector(dx: 0.3,dy: 0.28)).tap(); app.typeText("FIRST34_"+stamp)
        app.buttons["fei.more"].tap(); app.sheets.buttons["重命名"].tap()
        let title = app.textFields["fei.rename.title"]
        XCTAssertTrue(title.waitForExistence(timeout: 5)); title.typeText(prefix); app.buttons["fei.rename.save"].tap()
        XCTAssertTrue(app.navigationBars[prefix].waitForExistence(timeout: 5))
        let table = app.tables["fei.local.versions"]
        func history() {
            app.buttons["fei.more"].tap(); app.sheets.buttons["版本历史"].tap()
            XCTAssertTrue(table.waitForExistence(timeout: 5),app.debugDescription)
        }
        func row(_ label:String) -> XCUIElement { table.cells.containing(.staticText,identifier: label).firstMatch }
        func nameFirst(_ label:String) {
            app.buttons["fei.history.save"].tap()
            XCTAssertTrue(table.cells.firstMatch.waitForExistence(timeout: 5))
            table.cells.firstMatch.press(forDuration: 0.8); app.buttons["命名版本"].firstMatch.tap()
            let field = app.textFields["fei.history.rename"]
            XCTAssertTrue(field.waitForExistence(timeout: 5)); field.typeText(label)
            app.alerts.buttons["fei.history.rename.save"].firstMatch.tap()
            XCTAssertTrue(row(label).waitForExistence(timeout: 5),app.debugDescription)
        }
        history(); nameFirst("第一版")
        app.navigationBars.buttons.element(boundBy: 0).tap()
        XCTAssertTrue(app.navigationBars[prefix].waitForExistence(timeout: 5))
        app.coordinate(withNormalizedOffset: CGVector(dx: 0.3,dy: 0.28)).tap(); app.typeText("\nSECOND34_"+stamp)
        history(); nameFirst("第二版")
        app.buttons["fei.history.filter"].tap(); app.buttons.matching(NSPredicate(format: "label CONTAINS %@","仅看命名版本")).firstMatch.tap()
        XCTAssertEqual(table.cells.count,2,app.debugDescription)
        shot("history34-01-named-native-list")
        row("第一版").tap()
        XCTAssertTrue(app.buttons["fei.edit.history-restore"].waitForExistence(timeout: 5),app.debugDescription)
        XCTAssertFalse(app.buttons["fei.edit.keyboard"].exists)
        XCTAssertFalse(app.buttons["fei.edit.insert"].exists)
        app.buttons["fei.edit.history-marks"].tap(); shot("history34-02-read-only-preview")
        app.navigationBars.buttons.element(boundBy: 0).tap()
        XCTAssertTrue(table.waitForExistence(timeout: 5))
        app.navigationBars.buttons.element(boundBy: 0).tap()
        XCTAssertTrue(app.navigationBars[prefix].waitForExistence(timeout: 5)); shot("history34-03-returned-current-body")
        app.terminate(); app.launch()
        let original = app.tables["fei.notes.list"].cells.containing(.staticText,identifier: prefix).firstMatch
        XCTAssertTrue(original.waitForExistence(timeout: 15)); original.tap()
        history(); row("第一版").tap()
        XCTAssertTrue(app.buttons["fei.edit.history-restore"].waitForExistence(timeout: 5)); app.buttons["fei.edit.history-restore"].tap()
        app.alerts.buttons["取消"].firstMatch.tap()
        XCTAssertTrue(app.buttons["fei.edit.history-restore"].waitForExistence(timeout: 5))
        app.buttons["fei.edit.history-restore"].tap(); app.alerts.buttons["fei.history.confirm"].firstMatch.tap()
        XCTAssertTrue(app.navigationBars[prefix].waitForExistence(timeout: 5),app.debugDescription)
        XCTAssertTrue(app.buttons["fei.edit.insert"].waitForExistence(timeout: 5)); shot("history34-04-restored-body")
        history(); row("第二版").tap()
        XCTAssertTrue(app.buttons["fei.edit.history-copy"].waitForExistence(timeout: 5)); app.buttons["fei.edit.history-copy"].tap()
        XCTAssertTrue(app.navigationBars[prefix+"（第二版）"].waitForExistence(timeout: 5),app.debugDescription)
        shot("history34-05-independent-copy")
        app.navigationBars.buttons.element(boundBy: 0).tap()
        XCTAssertTrue(original.waitForExistence(timeout: 5)); original.tap(); history()
        row("第二版").press(forDuration: 0.8); app.buttons["删除此版本"].firstMatch.tap()
        app.alerts.buttons["取消"].firstMatch.tap(); XCTAssertTrue(row("第二版").exists)
        row("第二版").press(forDuration: 0.8); app.buttons["删除此版本"].firstMatch.tap()
        app.alerts.buttons["fei.history.confirm"].firstMatch.tap()
        XCTAssertTrue(row("第一版").waitForExistence(timeout: 5)); XCTAssertFalse(row("第二版").exists)
        shot("history34-06-version-deleted-note-preserved")
        print("HISTORY34_COMPLETED="+prefix)
    }
}
