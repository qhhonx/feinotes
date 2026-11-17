import XCTest

final class PadWorkspaceUITests: XCTestCase {
    private func shot(_ name: String) {
        let item = XCTAttachment(screenshot: XCUIScreen.main.screenshot())
        item.name = name; item.lifetime = .keepAlways; add(item)
    }
    private func start() -> XCUIApplication {
        continueAfterFailure = false
        let app = XCUIApplication(bundleIdentifier: "app.feinotes.ipad.editorprobe")
        app.launchEnvironment["FEI_NOTES_MODE"] = "local"
        XCUIDevice.shared.orientation = .landscapeLeft
        app.launch()
        XCTAssertTrue(app.collectionViews["fei.sidebar.list"].waitForExistence(timeout: 25))
        return app
    }
    private func chooseEnglish(_ app: XCUIApplication) {
        app.buttons["fei.settings"].tap()
        let general = app.cells["fei.setting.page:General"]
        XCTAssertTrue(general.waitForExistence(timeout: 5)); general.tap()
        app.cells["fei.setting.language"].tap()
        let english = app.cells["fei.setting.option.language.en"]
        XCTAssertTrue(english.waitForExistence(timeout: 5)); english.tap()
        app.cells["fei.sidebar.all-notes"].tap()
        XCTAssertTrue(app.staticTexts["fei.ipad.empty-detail"].waitForExistence(timeout: 5))
    }
    private func assertThreeColumns(_ app: XCUIApplication) {
        let sidebar = app.collectionViews["fei.sidebar.list"]
        let list = app.tables["fei.notes.list"]
        let detail = app.otherElements["fei.ipad.detail"]
        XCTAssertTrue(sidebar.isHittable); XCTAssertTrue(list.isHittable); XCTAssertTrue(detail.exists)
        XCTAssertLessThanOrEqual(sidebar.frame.maxX,list.frame.minX+2)
        XCTAssertLessThanOrEqual(list.frame.maxX,detail.frame.minX+2)
        XCTAssertGreaterThanOrEqual(list.frame.width,240)
        XCTAssertGreaterThanOrEqual(detail.frame.width,360)
        XCTAssertEqual(app.buttons.matching(identifier: "fei.new").allElementsBoundByIndex.filter { $0.isHittable }.count,1)
    }
    func testFolderListEditorSearchAndNativeColumns() {
        let app = start(); chooseEnglish(app)
        XCTAssertTrue(app.cells["fei.sidebar.all-notes"].staticTexts["My notes"].exists)
        assertThreeColumns(app); shot("ipad42-01-three-empty-columns")
        let marker = "IPAD42_"+String(Int(Date().timeIntervalSince1970))
        app.buttons["fei.sidebar.new-folder"].tap()
        let folderName = app.textFields["fei.sidebar.folder-name"]
        XCTAssertTrue(folderName.waitForExistence(timeout: 5)); folderName.tap(); folderName.typeText(marker)
        app.buttons["fei.sidebar.folder-save"].tap()
        let folder = app.collectionViews["fei.sidebar.list"].cells.containing(.staticText,identifier: marker).firstMatch
        XCTAssertTrue(folder.waitForExistence(timeout: 5))
        XCTAssertTrue(app.staticTexts["fei.ipad.empty-detail"].exists)
        app.buttons["fei.new"].tap()
        XCTAssertTrue(app.buttons["fei.more"].waitForExistence(timeout: 8))
        app.buttons["fei.more"].tap(); app.sheets.buttons["Rename"].tap()
        let title = app.textFields["fei.rename.title"]
        XCTAssertTrue(title.waitForExistence(timeout: 5)); title.tap()
        title.typeText(String(repeating: XCUIKeyboardKey.delete.rawValue,count: ((title.value as? String)?.count ?? 30)*2))
        title.typeText(marker+" note"); app.buttons["fei.rename.save"].tap()
        let row = app.tables["fei.notes.list"].cells.containing(.staticText,identifier: marker+" note").firstMatch
        XCTAssertTrue(row.waitForExistence(timeout: 8)); XCTAssertTrue(row.isSelected)
        assertThreeColumns(app); shot("ipad42-02-folder-selected-note")
        let search = app.searchFields["fei.notes.search-field"]
        XCTAssertTrue(search.isHittable); search.tap(); search.typeText("NO_MATCH_"+marker)
        XCTAssertTrue(app.staticTexts["No matching notes"].waitForExistence(timeout: 5))
        XCTAssertTrue(app.buttons["fei.more"].exists,"Searching the list must keep the document open")
        shot("ipad42-03-search-beside-document")
        let close = app.buttons["fei.notes.search-close"]
        XCTAssertTrue(close.waitForExistence(timeout: 5)); close.tap()
        XCTAssertTrue(row.waitForExistence(timeout: 5)); XCTAssertTrue(row.isSelected)
        app.cells["fei.sidebar.favorites"].tap()
        XCTAssertTrue(app.staticTexts["fei.ipad.empty-detail"].waitForExistence(timeout: 5))
        XCTAssertFalse(app.buttons["fei.more"].exists,"An empty detail cannot retain note actions")
        folder.tap(); XCTAssertTrue(row.waitForExistence(timeout: 5)); row.tap()
        XCTAssertTrue(app.buttons["fei.more"].waitForExistence(timeout: 5))
        app.buttons["fei.ipad.focus"].tap()
        XCTAssertTrue(XCTWaiter.wait(for: [XCTNSPredicateExpectation(predicate: NSPredicate { _,_ in app.otherElements["fei.ipad.detail"].frame.width > app.frame.width*0.9 },object: nil)],timeout: 5) == .completed)
        shot("ipad42-04-document-focus")
        app.buttons["fei.ipad.focus"].tap(); assertThreeColumns(app)
        XCUIDevice.shared.orientation = .portrait
        XCTAssertTrue(app.tables["fei.notes.list"].waitForExistence(timeout: 5))
        XCTAssertTrue(app.buttons["fei.more"].exists); shot("ipad42-05-portrait-two-columns")
        XCUIDevice.shared.orientation = .landscapeLeft
        assertThreeColumns(app); XCTAssertTrue(row.isSelected)
        shot("ipad42-06-restored-three-columns")
        print("IPAD_THREE_COLUMN_ACCEPTANCE_COMPLETED "+marker)
    }
}
