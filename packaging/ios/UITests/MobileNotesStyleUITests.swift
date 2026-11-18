import XCTest

final class MobileNotesStyleUITests: XCTestCase {
    private func shot(_ name: String) {
        if name == "search-02-active-native" || name == "mobile-notes-03-scoped-search-keyboard" {
            let app = XCUIApplication()
            if app.frame.width < 600 {
                var lastFrames: [CGRect] = []; var changed = Date()
                let stable = XCTNSPredicateExpectation(predicate: NSPredicate { _,_ in
                    let field = app.searchFields["fei.notes.search-field"],keyboard = app.keyboards.firstMatch
                    guard field.exists,keyboard.exists else { return false }
                    let frames = [field.frame,keyboard.frame]
                    if frames != lastFrames { lastFrames = frames; changed = Date(); return false }
                    return Date().timeIntervalSince(changed) >= 0.5 && keyboard.frame.height > 100 && keyboard.frame.maxY <= app.frame.maxY+1 && field.frame.maxY <= keyboard.frame.minY
                },object: nil)
                XCTAssertEqual(XCTWaiter.wait(for: [stable],timeout: 10),.completed,"Search must clear the settled keyboard")
            }
        }
        let screenshot = XCUIScreen.main.screenshot()
        let image = XCTAttachment(screenshot: screenshot)
        image.name = name; image.lifetime = .keepAlways; add(image)
        // Keep native PNGs available if Xcode stalls while aggregating attachments.
        let folder = FileManager.default.temporaryDirectory.appendingPathComponent("fei-mobile-style-evidence",isDirectory: true)
        do {
            try FileManager.default.createDirectory(at: folder,withIntermediateDirectories: true)
            try screenshot.pngRepresentation.write(to: folder.appendingPathComponent(name+".png"))
            let metadata: [String:Any] = ["name":name,"timestamp":Date().timeIntervalSince1970]
            try JSONSerialization.data(withJSONObject: metadata).write(to: folder.appendingPathComponent(name+".json"))
        } catch { XCTFail("Could not retain simulator evidence: \(error)") }
    }
    private func tapRow(_ id: String,_ app: XCUIApplication) {
        let row = app.cells["fei.setting."+id]
        for _ in 0..<10 { if row.exists && row.isHittable { break }; app.tables["fei.settings.list"].swipeUp() }
        XCTAssertTrue(row.waitForExistence(timeout: 5),id); row.tap()
    }
    private func choose(_ id: String,_ value: String,_ app: XCUIApplication) {
        tapRow(id,app)
        let choice = app.cells["fei.setting.option."+id+"."+value]
        XCTAssertTrue(choice.waitForExistence(timeout: 5)); choice.tap()
        XCTAssertTrue(app.tables["fei.settings.list"].waitForExistence(timeout: 5))
    }
    private func cancelSearch(_ app: XCUIApplication) {
        let cancel = app.buttons.matching(NSPredicate(format: "label IN {'Cancel','Close','Cancel Search','取消','关闭'}")).firstMatch
        XCTAssertTrue(cancel.waitForExistence(timeout: 5),app.debugDescription)
        cancel.tap()
    }
    private func back(_ app: XCUIApplication) { app.navigationBars.buttons.element(boundBy: 0).tap() }
    private func returnToList(_ app: XCUIApplication) { back(app); back(app); XCTAssertTrue(app.tables["fei.notes.list"].waitForExistence(timeout: 5)) }

    func testThemeNavigationSearchAndReadingShortcuts() {
        continueAfterFailure = false
        var app = XCUIApplication(); app.launchEnvironment["FEI_NOTES_MODE"] = "local"
        XCUIDevice.shared.orientation = .portrait; app.launch()
        XCTAssertTrue(app.buttons["fei.settings"].waitForExistence(timeout: 20))
        app.buttons["fei.settings"].tap(); tapRow("page:General",app); choose("language","en",app); back(app)
        tapRow("page:Appearance",app); choose("theme","notes",app); choose("appearance","light",app)
        shot("mobile-notes-01-light-settings"); returnToList(app)
        let search = app.searchFields["fei.notes.search-field"]
        let create = app.buttons["fei.new"]
        XCTAssertTrue(search.isHittable); XCTAssertTrue(create.isHittable)
        XCTAssertEqual(app.buttons.matching(identifier: "fei.new").allElementsBoundByIndex.filter { $0.isHittable }.count,1,"Each size class needs one Compose entry")
        if app.frame.width < 600 {
            XCTAssertGreaterThan(search.frame.minY,app.frame.height*0.6)
            XCTAssertGreaterThan(create.frame.minX,search.frame.maxX)
        }
        let tabs = app.segmentedControls["fei.notes.tabs"]
        if tabs.exists,app.tables["fei.notes.list"].cells.firstMatch.exists {
            XCTAssertGreaterThanOrEqual(app.tables["fei.notes.list"].cells.firstMatch.frame.minY,tabs.frame.maxY)
        }
        shot("mobile-notes-02-light-list")
        search.tap(); search.typeText("FEI_NO_MATCH_MOBILE_STYLE")
        XCTAssertTrue(app.staticTexts["No matching notes"].waitForExistence(timeout: 5))
        if app.frame.width < 600 { XCTAssertTrue(app.keyboards.firstMatch.exists) }
        shot("mobile-notes-03-scoped-search-keyboard")
        cancelSearch(app)
        XCTAssertFalse((search.value as? String ?? "").contains("FEI_NO_MATCH"))
        create.tap()
        XCTAssertTrue(app.buttons["fei.more"].waitForExistence(timeout: 5))
        XCTAssertTrue(app.buttons["fei.share"].exists)
        let toolbar = app.toolbars["fei.editor.toolbar"]
        if app.frame.width >= 600 { XCTAssertLessThan(toolbar.frame.maxY,app.frame.height*0.35) }
        for id in ["styles","list","table","insert","keyboard"] { XCTAssertTrue(toolbar.buttons["fei.edit."+id].isHittable,id) }
        let marker = "FEI_MOBILE_STYLE_"+String(Int(Date().timeIntervalSince1970))
        app.coordinate(withNormalizedOffset: CGVector(dx: 0.3,dy: 0.3)).tap(); app.typeText(marker+" synthetic body")
        toolbar.buttons["fei.edit.keyboard"].tap()
        app.buttons["fei.more"].tap(); app.sheets.buttons["Rename"].tap()
        let title = app.textFields["fei.rename.title"]
        XCTAssertTrue(title.waitForExistence(timeout: 5))
        title.tap()
        if let value = title.value as? String { title.typeText(String(repeating: XCUIKeyboardKey.delete.rawValue,count: value.count)) }
        title.typeText("Notes on design"); app.buttons["fei.rename.save"].tap()
        XCTAssertTrue(app.navigationBars["Notes on design"].waitForExistence(timeout: 5))
        shot("mobile-notes-04-editing-tools")
        app.buttons["fei.more"].tap(); app.sheets.buttons["Note options"].tap(); app.sheets.buttons["Reading mode"].tap()
        XCTAssertTrue(toolbar.buttons["fei.edit.compose"].waitForExistence(timeout: 5))
        XCTAssertTrue(toolbar.buttons["fei.edit.quick-checklist"].isHittable)
        shot("mobile-notes-05-reading-compose")
        toolbar.buttons["fei.edit.compose"].tap()
        XCTAssertTrue(app.buttons["fei.more"].waitForExistence(timeout: 5))
        back(app)
        XCTAssertTrue(app.tables["fei.notes.list"].waitForExistence(timeout: 5))
        app.buttons["fei.settings"].tap(); tapRow("page:Appearance",app); choose("theme","feishu",app)
        shot("mobile-notes-06-feishu-switch")
        choose("theme","notes",app); choose("appearance","dark",app); returnToList(app)
        shot("mobile-notes-07-dark-list")
        app.terminate(); app = XCUIApplication(); app.launchEnvironment["FEI_NOTES_MODE"] = "local"; app.launch()
        XCTAssertTrue(app.buttons["fei.settings"].waitForExistence(timeout: 15))
        app.buttons["fei.settings"].tap(); tapRow("page:Appearance",app)
        XCTAssertTrue(app.cells["fei.setting.theme"].staticTexts["Notes"].exists)
        shot("mobile-notes-08-dark-persisted")
        choose("appearance","",app); returnToList(app)
        XCUIDevice.shared.orientation = .landscapeLeft
        XCTAssertTrue(search.waitForExistence(timeout: 5)); XCTAssertTrue(create.isHittable)
        shot("mobile-notes-09-landscape-adaptation")
        XCUIDevice.shared.orientation = .portrait
        print("MOBILE_NOTES_STYLE_COMPLETED "+marker)
    }

    func testNativeSearchLifecycle() {
        continueAfterFailure = false
        let app = XCUIApplication(); app.launchEnvironment["FEI_NOTES_MODE"] = "local"; app.launch()
        let search = app.searchFields["fei.notes.search-field"]
        XCTAssertTrue(search.waitForExistence(timeout: 15),app.debugDescription)
        shot("search-01-inactive-native")
        search.tap(); search.typeText("FEI_NATIVE_SEARCH_NO_MATCH")
        XCTAssertTrue(app.staticTexts["No matching notes"].waitForExistence(timeout: 5))
        if app.frame.width < 600 { XCTAssertTrue(app.keyboards.firstMatch.exists) }
        XCTAssertFalse(app.buttons["fei.new"].exists && app.buttons["fei.new"].isHittable)
        shot("search-02-active-native")
        cancelSearch(app)
        XCTAssertTrue(app.buttons["fei.new"].waitForExistence(timeout: 5))
        XCTAssertFalse(app.keyboards.firstMatch.exists)
        XCTAssertFalse((search.value as? String ?? "").contains("FEI_NATIVE_SEARCH"))
        shot("search-03-cancel-restored")
    }

    func testSearchClearSubmitAndResultReturn() {
        continueAfterFailure = false
        let app = XCUIApplication(); app.launchEnvironment["FEI_NOTES_MODE"] = "local"; app.launch()
        XCTAssertTrue(app.buttons["fei.new"].waitForExistence(timeout: 15)); app.buttons["fei.new"].tap()
        app.buttons["fei.more"].tap(); app.sheets.buttons["Rename"].tap()
        let title = app.textFields["fei.rename.title"]
        XCTAssertTrue(title.waitForExistence(timeout: 5)); title.tap()
        if let value = title.value as? String { title.typeText(String(repeating: XCUIKeyboardKey.delete.rawValue,count: value.count)) }
        let name = "Search fixture "+String(Int(Date().timeIntervalSince1970))
        title.typeText(name); app.buttons["fei.rename.save"].tap(); back(app)
        let search = app.searchFields["fei.notes.search-field"]
        search.tap(); search.typeText("NO_MATCH_BEFORE_CLEAR")
        XCTAssertTrue(app.staticTexts["No matching notes"].waitForExistence(timeout: 5))
        search.buttons.matching(NSPredicate(format: "label IN {'Clear text','清除文本'}")).firstMatch.tap()
        XCTAssertFalse(app.staticTexts["No matching notes"].exists)
        if app.frame.width < 600 { XCTAssertTrue(app.keyboards.firstMatch.exists) }
        search.typeText(name); search.typeText("\n")
        XCTAssertFalse(app.keyboards.firstMatch.exists)
        let result = app.tables["fei.notes.list"].cells.containing(.staticText,identifier: name).firstMatch
        XCTAssertTrue(result.waitForExistence(timeout: 5)); result.tap()
        XCTAssertTrue(app.buttons["fei.more"].waitForExistence(timeout: 5)); back(app)
        XCTAssertTrue(search.waitForExistence(timeout: 5))
        XCTAssertEqual(search.value as? String,name)
        XCTAssertTrue(result.exists); XCTAssertFalse(app.keyboards.firstMatch.exists)
        shot("search-04-result-return")
        search.tap(); search.typeText(" NO_MATCH_SUFFIX")
        XCTAssertTrue(app.staticTexts["No matching notes"].waitForExistence(timeout: 5)); cancelSearch(app)
        XCTAssertEqual(search.value as? String,name)
        XCTAssertTrue(result.waitForExistence(timeout: 5))
        // Restore the initial unfiltered fixture for the following theme tests.
        search.tap(); search.buttons.matching(NSPredicate(format: "label IN {'Clear text','清除文本'}")).firstMatch.tap()
        result.tap(); back(app)
    }

    func testComposeDoubleTapCreatesOneNote() {
        continueAfterFailure = false
        let app = XCUIApplication(); app.launchEnvironment["FEI_NOTES_MODE"] = "local"; app.launch()
        let list = app.tables["fei.notes.list"]
        XCTAssertTrue(list.waitForExistence(timeout: 15))
        let before = list.cells.count
        app.buttons["fei.new"].doubleTap()
        XCTAssertTrue(app.buttons["fei.more"].waitForExistence(timeout: 5))
        back(app)
        XCTAssertTrue(list.waitForExistence(timeout: 5))
        XCTAssertEqual(list.cells.count,before+1)
    }

    func testLargeTextSearchControlsStayReachable() {
        continueAfterFailure = false
        let app = XCUIApplication(); app.launchEnvironment["FEI_NOTES_MODE"] = "local"
        app.launchArguments = ["-UIPreferredContentSizeCategoryName","UICTContentSizeCategoryAccessibilityXXXL"]
        app.launch()
        let search = app.searchFields["fei.notes.search-field"]
        XCTAssertTrue(search.waitForExistence(timeout: 15))
        XCTAssertTrue(app.buttons["fei.new"].isHittable)
        search.tap(); search.typeText("FEI_LARGE_SEARCH")
        XCTAssertFalse(app.buttons["fei.new"].exists && app.buttons["fei.new"].isHittable)
        XCTAssertGreaterThan(search.frame.width,app.frame.width < 600 ? app.frame.width*0.55 : 200)
        shot("mobile-notes-10-accessibility-size-search")
        cancelSearch(app); XCTAssertTrue(app.buttons["fei.new"].waitForExistence(timeout: 5))
    }

    func testTableShortcutOpensCurrentTableControls() {
        continueAfterFailure = false
        let app = XCUIApplication(); app.launchEnvironment["FEI_NOTES_MODE"] = "local"; app.launch()
        XCTAssertTrue(app.buttons["fei.new"].waitForExistence(timeout: 15)); app.buttons["fei.new"].tap()
        let button = app.toolbars["fei.editor.toolbar"].buttons["fei.edit.table"]
        XCTAssertTrue(button.waitForExistence(timeout: 5)); button.tap()
        XCTAssertTrue(app.keyboards.firstMatch.waitForExistence(timeout: 5))
        app.typeText("Synthetic table cell")
        button.tap()
        XCTAssertTrue(app.tables["fei.table.controls"].waitForExistence(timeout: 5))
        shot("mobile-notes-11-current-table-tools")
        app.buttons["fei.table.done"].tap()
    }

}
