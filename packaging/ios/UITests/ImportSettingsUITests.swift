import XCTest

final class ImportSettingsUITests: XCTestCase {
    private func shot(_ name:String) {
        let a = XCTAttachment(screenshot: XCUIScreen.main.screenshot()); a.name = name; a.lifetime = .keepAlways; add(a)
    }
    func testGeneralMenusAndFolderPicker() {
        continueAfterFailure = false
        let app = XCUIApplication(); app.launchEnvironment["FEI_NOTES_MODE"] = "local"
        XCUIDevice.shared.orientation = .portrait; app.launch()
        XCTAssertTrue(app.buttons["fei.settings"].waitForExistence(timeout: 15)); app.buttons["fei.settings"].tap()
        XCTAssertTrue(app.navigationBars["设置"].waitForExistence(timeout: 5))
        app.coordinate(withNormalizedOffset: CGVector(dx: 0.5,dy: 0.167)).tap()
        XCTAssertTrue(app.navigationBars["通用"].waitForExistence(timeout: 5),app.debugDescription)
        shot("import35-01-general")
        app.coordinate(withNormalizedOffset: CGVector(dx: 0.82,dy: 0.173)).tap()
        shot("import35-02-language-menu")
        app.coordinate(withNormalizedOffset: CGVector(dx: 0.72,dy: 0.3)).tap()
        XCTAssertTrue(app.navigationBars["General"].waitForExistence(timeout: 5),app.debugDescription)
        shot("import35-03-english")
        app.terminate(); app.launch()
        XCTAssertTrue(app.buttons["fei.settings"].waitForExistence(timeout: 15)); app.buttons["fei.settings"].tap()
        XCTAssertTrue(app.navigationBars["Settings"].waitForExistence(timeout: 5))
        app.coordinate(withNormalizedOffset: CGVector(dx: 0.5,dy: 0.167)).tap()
        XCTAssertTrue(app.navigationBars["General"].waitForExistence(timeout: 5))
        app.coordinate(withNormalizedOffset: CGVector(dx: 0.82,dy: 0.173)).tap()
        app.coordinate(withNormalizedOffset: CGVector(dx: 0.72,dy: 0.25)).tap()
        XCTAssertTrue(app.navigationBars["通用"].waitForExistence(timeout: 5))
    }
    func testLegacyFolderPickerCancelAndImport() {
        continueAfterFailure = false
        let app = XCUIApplication(); app.launchEnvironment["FEI_NOTES_MODE"] = "local"
        XCUIDevice.shared.orientation = .portrait; app.launch()
        XCTAssertTrue(app.buttons["fei.settings"].waitForExistence(timeout: 15)); app.buttons["fei.settings"].tap()
        XCTAssertTrue(app.navigationBars["设置"].waitForExistence(timeout: 5))
        app.coordinate(withNormalizedOffset: CGVector(dx: 0.5,dy: 0.304)).tap()
        XCTAssertTrue(app.navigationBars["文档库"].waitForExistence(timeout: 5)); shot("import35-04-library-import-entry")
        app.coordinate(withNormalizedOffset: CGVector(dx: 0.8,dy: 0.52)).tap()
        XCTAssertTrue(app.buttons["DOCPicker.actionButton"].waitForExistence(timeout: 5),app.debugDescription)
        shot("import35-05-native-folder-picker")
        app.coordinate(withNormalizedOffset: CGVector(dx: 0.5,dy: 0.075)).press(forDuration: 0.1,thenDragTo: app.coordinate(withNormalizedOffset: CGVector(dx: 0.5,dy: 0.65)))
        XCTAssertEqual(XCTWaiter.wait(for: [XCTNSPredicateExpectation(predicate: NSPredicate(format: "exists == false"),object: app.buttons["DOCPicker.actionButton"])],timeout: 5),.completed)
        app.coordinate(withNormalizedOffset: CGVector(dx: 0.8,dy: 0.52)).tap()
        XCTAssertTrue(app.buttons["DOCPicker.actionButton"].waitForExistence(timeout: 5))
        let folder = app.cells.matching(NSPredicate(format: "label BEGINSWITH %@","FEI_IMPORT35_library")).firstMatch
        XCTAssertTrue(folder.waitForExistence(timeout: 5),app.debugDescription); folder.tap()
        shot("import35-06-selected-synthetic-library")
        app.buttons["DOCPicker.actionButton"].tap()
        XCTAssertEqual(XCTWaiter.wait(for: [XCTNSPredicateExpectation(predicate: NSPredicate(format: "exists == false"),object: app.buttons["DOCPicker.actionButton"])],timeout: 10),.completed)
        let back = app.navigationBars["文档库"].buttons.element(boundBy: 0)
        XCTAssertEqual(XCTWaiter.wait(for: [XCTNSPredicateExpectation(predicate: NSPredicate(format: "enabled == true"),object: back)],timeout: 15),.completed)
        shot("import35-07-import-result")
        back.tap(); app.navigationBars.buttons.element(boundBy: 0).tap()
        let list = app.tables["fei.notes.list"]
        XCTAssertTrue(list.waitForExistence(timeout: 5))
        let importedFolder = list.cells.containing(.staticText,identifier: "导入35目录").firstMatch
        XCTAssertTrue(importedFolder.waitForExistence(timeout: 10),app.debugDescription); importedFolder.tap()
        let importedNote = list.cells.containing(.staticText,identifier: "导入35正文").firstMatch
        XCTAssertTrue(importedNote.waitForExistence(timeout: 5),app.debugDescription); importedNote.tap()
        XCTAssertTrue(app.navigationBars["导入35正文"].waitForExistence(timeout: 5)); shot("import35-08-opened-imported-note")
        print("IMPORT35_COMPLETED")
    }
    func testSharedDefaultTypeMenu() {
        continueAfterFailure = false
        let app = XCUIApplication(); app.launchEnvironment["FEI_NOTES_MODE"] = "local"
        XCUIDevice.shared.orientation = .portrait; app.launch()
        XCTAssertTrue(app.buttons["fei.settings"].waitForExistence(timeout: 15)); app.buttons["fei.settings"].tap()
        XCTAssertTrue(app.navigationBars["设置"].waitForExistence(timeout: 5))
        app.coordinate(withNormalizedOffset: CGVector(dx: 0.5,dy: 0.167)).tap()
        XCTAssertTrue(app.navigationBars["通用"].waitForExistence(timeout: 5))
        app.coordinate(withNormalizedOffset: CGVector(dx: 0.82,dy: 0.393)).tap()
        shot("import35-09-default-type-menu")
        app.coordinate(withNormalizedOffset: CGVector(dx: 0.72,dy: 0.55)).tap()
        app.navigationBars.buttons.element(boundBy: 0).tap(); app.navigationBars.buttons.element(boundBy: 0).tap()
        XCTAssertTrue(app.buttons["fei.new"].waitForExistence(timeout: 5)); app.buttons["fei.new"].tap()
        XCTAssertTrue(app.buttons["fei.more"].waitForExistence(timeout: 5)); app.buttons["fei.more"].tap()
        app.sheets.buttons["重命名"].tap()
        let field = app.textFields["fei.rename.title"]
        XCTAssertTrue(field.waitForExistence(timeout: 5)); field.typeText("默认类型35"); app.buttons["fei.rename.save"].tap()
        XCTAssertTrue(app.navigationBars["默认类型35"].waitForExistence(timeout: 5)); shot("import35-10-created-markdown")
    }
}
