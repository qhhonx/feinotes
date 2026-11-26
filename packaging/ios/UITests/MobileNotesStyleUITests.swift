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

    private func launchLightNotes(fixture: URL? = nil,stayInSettings: Bool = false) -> XCUIApplication {
        let app = XCUIApplication()
        app.launchEnvironment["FEI_NOTES_MODE"] = "local"
        if let fixture { app.launchEnvironment["FEI_MOBILE_QA_FIXTURES"] = fixture.path }
        XCUIDevice.shared.orientation = UIDevice.current.userInterfaceIdiom == .pad ? .landscapeLeft : .portrait
        app.launch()
        XCTAssertTrue(app.buttons["fei.settings"].waitForExistence(timeout: 15))
        app.buttons["fei.settings"].tap()
        tapRow("page:Appearance",app)
        choose("interface-style","notes",app)
        choose("appearance","light",app)
        if stayInSettings {
            let appearance = app.navigationBars.matching(NSPredicate(format: "identifier IN %@", ["外观","Appearance"])).firstMatch
            XCTAssertTrue(appearance.waitForExistence(timeout: 5))
            appearance.buttons.firstMatch.tap()
        } else { returnToList(app) }
        return app
    }

    private func noteMenu(_ app: XCUIApplication,_ names: [String],parent: [String] = []) {
        func find(_ titles: [String]) -> XCUIElement {
            app.descendants(matching: .any).matching(NSPredicate(format: "label IN %@",titles)).firstMatch
        }
        app.buttons["fei.more"].tap()
        let item = find(names)
        if !item.waitForExistence(timeout: 2),!parent.isEmpty {
            let group = find(parent)
            XCTAssertTrue(group.waitForExistence(timeout: 4),app.debugDescription)
            group.tap()
        }
        XCTAssertTrue(item.waitForExistence(timeout: 5),app.debugDescription)
        item.tap()
    }

    func testLightAppearanceNewFolderAndReadingMode() throws {
        continueAfterFailure = false
        let title = "Appearance QA " + String(UUID().uuidString.prefix(6))
        let fixtures = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: fixtures,withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: fixtures) }
        try "<title>\(title)</title><h2>Appearance test</h2><p>Synthetic body</p>"
            .write(to: fixtures.appendingPathComponent("appearance.xml"),atomically: true,encoding: .utf8)
        let app = launchLightNotes(fixture: fixtures)
        app.buttons["fei.library-actions"].tap()
        let newFolder = app.buttons.matching(NSPredicate(format: "label IN %@", ["新建文件夹","New folder"])).firstMatch
        XCTAssertTrue(newFolder.waitForExistence(timeout: 5))
        newFolder.tap()
        let field = app.textFields["fei.rename.title"]
        XCTAssertTrue(field.waitForExistence(timeout: 5))
        field.tap()
        field.typeText("Light appearance QA")
        shot("light-appearance-create-folder")
        let cancel = app.buttons.matching(NSPredicate(format: "label IN %@", ["取消","Cancel"])).firstMatch
        XCTAssertTrue(cancel.waitForExistence(timeout: 5))
        cancel.tap()

        let note = app.tables["fei.notes.list"].cells.containing(.staticText,identifier: title).firstMatch
        XCTAssertTrue(note.waitForExistence(timeout: 8))
        note.tap()
        let mode = app.buttons["fei.note-mode"]
        XCTAssertTrue(mode.waitForExistence(timeout: 8))
        app.switchToReadingMode()
        shot("light-appearance-reading-mode")
        let reading = XCTNSPredicateExpectation(predicate: NSPredicate(format: "label IN %@", ["阅读模式","Reading mode"]),object: mode)
        XCTAssertEqual(XCTWaiter.wait(for: [reading],timeout: 8),.completed)
        for id in ["mode","quick-checklist","quick-insert","list","insert","styles","table"] {
            XCTAssertFalse(app.buttons["fei.edit."+id].isHittable,id)
        }
        XCTAssertFalse(app.keyboards.firstMatch.exists)
        let toast = app.descendants(matching: .any)["fei.share.toast"].firstMatch
        if toast.exists { XCTAssertGreaterThanOrEqual(toast.frame.minY,mode.frame.maxY) }

        noteMenu(app,["文档目录","Outline"])
        XCTAssertTrue(app.tables["fei.outline.controls"].waitForExistence(timeout: 5))
        shot("light-appearance-outline")
        app.buttons["fei.outline.done"].tap()

        noteMenu(app,["图片和附件","Images and attachments"],parent: ["文档信息","Document information"])
        XCTAssertTrue(app.tables["fei.attachments.controls"].waitForExistence(timeout: 5))
        shot("light-appearance-attachments")
        app.buttons["fei.attachments.done"].tap()

        app.buttons["fei.more"].tap()
        XCTAssertFalse(app.descendants(matching: .any).matching(NSPredicate(format: "label IN %@",["编辑和格式","Editing and formatting","删除","Delete"])).firstMatch.isHittable)
        let options = app.descendants(matching: .any).matching(NSPredicate(format: "label IN %@",["文档选项","Note options"])).firstMatch
        if options.exists { options.tap() }
        for titles in [["重命名","Rename"],["移动到","Move to"]] {
            XCTAssertFalse(app.descendants(matching: .any).matching(NSPredicate(format: "label IN %@",titles)).firstMatch.isHittable)
        }
        shot("light-appearance-reading-menu")
        app.coordinate(withNormalizedOffset: CGVector(dx: 0.27,dy: 0.8)).tap()
        app.switchToEditingMode()
        let editing = XCTNSPredicateExpectation(predicate: NSPredicate(format: "label IN %@", ["编辑模式","Editing mode"]),object: mode)
        XCTAssertEqual(XCTWaiter.wait(for: [editing],timeout: 8),.completed)
        XCTAssertFalse(app.keyboards.firstMatch.exists)
        shot("light-appearance-editing-before-keyboard")
        noteMenu(app,["移动到","Move to"],parent: ["文档选项","Note options"])
        let move = app.navigationBars.matching(NSPredicate(format: "identifier IN %@", ["移动到","Move to"])).firstMatch
        XCTAssertTrue(move.waitForExistence(timeout: 5))
        shot("light-appearance-move")
        move.buttons.firstMatch.tap()
    }

    func testTitleOnlyPDFStaysInEditor() throws {
        continueAfterFailure = false
        let title = "Empty PDF QA " + String(UUID().uuidString.prefix(6))
        let fixtures = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: fixtures,withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: fixtures) }
        try "<title>\(title)</title><p></p>"
            .write(to: fixtures.appendingPathComponent("empty.xml"),atomically: true,encoding: .utf8)
        let app = launchLightNotes(fixture: fixtures)
        let note = app.tables["fei.notes.list"].cells.containing(.staticText,identifier: title).firstMatch
        XCTAssertTrue(note.waitForExistence(timeout: 8))
        note.tap()
        XCTAssertTrue(app.buttons["fei.more"].waitForExistence(timeout: 5))
        noteMenu(app,["导出 PDF","Export PDF"],parent: ["导出与打印","Export and print"])
        shot("light-appearance-empty-pdf")
        XCTAssertTrue(app.buttons["fei.more"].isHittable)
        XCTAssertFalse(app.buttons["DOCPicker.actionButton"].exists)
    }

    func testPreparedMarkdownAndPDFCanCancelAndReopen() throws {
        continueAfterFailure = false
        let title = "Native export QA " + String(UUID().uuidString.prefix(6))
        let fixtures = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: fixtures,withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: fixtures) }
        try "<title>\(title)</title><p>This synthetic document has exportable content.</p>"
            .write(to: fixtures.appendingPathComponent("export.xml"),atomically: true,encoding: .utf8)
        let app = launchLightNotes(fixture: fixtures)
        let note = app.tables["fei.notes.list"].cells.containing(.staticText,identifier: title).firstMatch
        XCTAssertTrue(note.waitForExistence(timeout: 8))
        note.tap()

        let save = app.buttons["DOCPicker.actionButton"]
        func cancelPicker(_ suffix: String) {
            XCTAssertTrue(save.waitForExistence(timeout: 25),app.debugDescription)
            shot("light-appearance-export-"+suffix)
            let cancel = app.buttons.matching(NSPredicate(format: "label IN %@",["取消","Cancel"])).firstMatch
            if cancel.waitForExistence(timeout: 2) { cancel.tap() }
            else {
                let top = app.coordinate(withNormalizedOffset: CGVector(dx: 0.5,dy: 0.087))
                top.press(forDuration: 0.1,thenDragTo: app.coordinate(withNormalizedOffset: CGVector(dx: 0.5,dy: 0.85)))
            }
            XCTAssertEqual(XCTWaiter.wait(for: [XCTNSPredicateExpectation(predicate: NSPredicate(format: "exists == false"),object: save)],timeout: 5),.completed)
            XCTAssertTrue(app.buttons["fei.more"].isHittable)
        }
        for attempt in 1...2 {
            noteMenu(app,["导出 Markdown","Export Markdown"],parent: ["导出与打印","Export and print"])
            let text = app.descendants(matching: .any).matching(NSPredicate(format: "label IN %@",["仅文本","Text only"])).firstMatch
            XCTAssertTrue(text.waitForExistence(timeout: 5),app.debugDescription)
            text.tap()
            cancelPicker("markdown-\(attempt)")
        }
        noteMenu(app,["导出 PDF","Export PDF"],parent: ["导出与打印","Export and print"])
        cancelPicker("pdf")
    }

    func testExistingBoardAndMarkdownExportWithoutShortcut() throws {
        continueAfterFailure = false
        let title = "Standalone board QA " + String(UUID().uuidString.prefix(6))
        let fixtures = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: fixtures,withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: fixtures) }
        try #"{"type":"excalidraw","version":2,"elements":[],"appState":{},"files":{}}"#
            .write(to: fixtures.appendingPathComponent(title+".excalidraw"),atomically: true,encoding: .utf8)
        let app = launchLightNotes(fixture: fixtures)
        let compose = app.buttons["fei.new"].firstMatch
        XCTAssertTrue(compose.waitForExistence(timeout: 8),app.debugDescription)
        XCTAssertTrue(compose.isHittable)
        XCTAssertFalse(app.buttons["fei.new-board"].exists)
        shot("home-document-compose-without-board")

        let list = app.tables["fei.notes.list"].firstMatch
        let row = list.cells.containing(.staticText,identifier: title).firstMatch
        XCTAssertTrue(row.waitForExistence(timeout: 8),app.debugDescription)
        row.tap()
        let done = app.buttons["fei.board.done"]
        XCTAssertTrue(done.waitForExistence(timeout: 15),app.debugDescription)
        XCTAssertFalse(app.keyboards.firstMatch.exists)
        shot("standalone-board-open")
        done.tap()
        XCTAssertTrue(row.waitForExistence(timeout: 8),app.debugDescription)
        func action(_ labels: [String]) -> XCUIElement {
            app.descendants(matching: .any).matching(NSPredicate(format: "label IN %@",labels)).firstMatch
        }
        for (index, labels) in [["仅文本","Text only"],["包含图片和附件","Include images and attachments"]].enumerated() {
            row.press(forDuration: 1.1)
            let export = action(["导出与打印","Export and print"])
            XCTAssertTrue(export.waitForExistence(timeout: 8),app.debugDescription)
            export.tap()
            let markdown = action(["导出 Markdown","Export Markdown"])
            XCTAssertTrue(markdown.waitForExistence(timeout: 8),app.debugDescription)
            markdown.tap()
            let choice = action(labels)
            XCTAssertTrue(choice.waitForExistence(timeout: 8),app.debugDescription)
            choice.tap()
            let save = app.buttons["DOCPicker.actionButton"]
            XCTAssertTrue(save.waitForExistence(timeout: 20),app.debugDescription)
            shot("standalone-board-markdown-\(index)")
            let cancel = app.buttons.matching(NSPredicate(format: "label IN %@",["取消","Cancel"])).firstMatch
            if cancel.waitForExistence(timeout: 2) { cancel.tap() }
            else {
                let grip = app.coordinate(withNormalizedOffset: CGVector(dx: 0.5,dy: 0.087))
                grip.press(forDuration: 0.1,thenDragTo: app.coordinate(withNormalizedOffset: CGVector(dx: 0.5,dy: 0.85)))
            }
            XCTAssertEqual(XCTWaiter.wait(for: [XCTNSPredicateExpectation(predicate: NSPredicate(format: "exists == false"),object: save)],timeout: 5),.completed)
            XCTAssertTrue(row.isHittable)
        }
    }

    func testLocalSubscriptionPlanAppearance() throws {
        continueAfterFailure = false
        let app = launchLightNotes(stayInSettings: true)
        tapRow("page:Cloud",app)
        let plans = app.cells["fei.setting.page:CloudAccess"]
        guard plans.waitForExistence(timeout: 3) else { throw XCTSkip("Requires the local subscription preview fixture") }
        plans.tap()
        let annual = app.buttons["fei.subscription.plan.annual"]
        let monthly = app.buttons["fei.subscription.plan.monthly"]
        XCTAssertTrue(annual.waitForExistence(timeout: 10))
        XCTAssertTrue(monthly.isHittable)
        shot("subscription-annual")
        monthly.tap()
        let billing = app.staticTexts["fei.subscription.billing"]
        XCTAssertTrue(billing.label.contains("每月") || billing.label.contains("per month"))
        shot("subscription-monthly")
        app.buttons["fei.subscription.close"].tap()
        XCTAssertTrue(app.cells["fei.setting.page:CloudAccess"].waitForExistence(timeout: 5))
    }

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
        XCTAssertTrue(app.staticTexts["No matching documents"].waitForExistence(timeout: 5))
        if app.frame.width < 600 { XCTAssertTrue(app.keyboards.firstMatch.exists) }
        shot("mobile-notes-03-scoped-search-keyboard")
        cancelSearch(app)
        XCTAssertFalse((search.value as? String ?? "").contains("FEI_NO_MATCH"))
        create.tap()
        XCTAssertTrue(app.buttons["fei.more"].waitForExistence(timeout: 5))
        XCTAssertTrue(app.buttons["fei.share"].exists)
        let toolbar = app.toolbars["fei.editor.toolbar"]
        if app.frame.width >= 600 { XCTAssertLessThan(toolbar.frame.maxY,app.frame.height*0.35) }
        for id in ["styles","list","table","insert","keyboard"] { XCTAssertTrue(app.buttons["fei.edit."+id].isHittable,id) }
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
        app.switchToReadingMode()
        let compose = app.buttons["fei.new.editor"]
        XCTAssertTrue(compose.waitForExistence(timeout: 5))
        XCTAssertFalse(toolbar.buttons["fei.edit.quick-checklist"].isHittable)
        shot("mobile-notes-05-reading-compose")
        compose.tap()
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
        XCTAssertTrue(app.staticTexts["No matching documents"].waitForExistence(timeout: 5))
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
        XCTAssertTrue(app.staticTexts["No matching documents"].waitForExistence(timeout: 5))
        search.buttons.matching(NSPredicate(format: "label IN {'Clear text','清除文本'}")).firstMatch.tap()
        XCTAssertFalse(app.staticTexts["No matching documents"].exists)
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
        XCTAssertTrue(app.staticTexts["No matching documents"].waitForExistence(timeout: 5)); cancelSearch(app)
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
        let button = app.buttons["fei.edit.table"]
        XCTAssertTrue(button.waitForExistence(timeout: 5)); button.tap()
        XCTAssertTrue(app.keyboards.firstMatch.waitForExistence(timeout: 5))
        app.typeText("Synthetic table cell")
        button.tap()
        XCTAssertTrue(app.tables["fei.table.controls"].waitForExistence(timeout: 5))
        shot("mobile-notes-11-current-table-tools")
        app.buttons["fei.table.done"].tap()
    }

}
