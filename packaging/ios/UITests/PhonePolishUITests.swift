import XCTest

final class PhonePolishUITests: XCTestCase {
    private let app = XCUIApplication()
    private func shot(_ name: String) {
        let settled = Date().addingTimeInterval(0.4)
        wait({ Date() >= settled },"Allow the GPUI drawable to settle after native chrome updates")
        let item = XCTAttachment(screenshot: XCUIScreen.main.screenshot())
        item.name = name; item.lifetime = .keepAlways; add(item)
    }
    private func wait(_ check: @escaping () -> Bool,_ message: String) {
        XCTAssertEqual(XCTWaiter.wait(for: [XCTNSPredicateExpectation(predicate: NSPredicate { _,_ in check() },object: nil)],timeout: 8),.completed,message)
    }
    private func cancelSearch() {
        let cancel = app.buttons.matching(NSPredicate(format: "label IN {'Cancel','Close','Cancel Search','取消','关闭'}")).firstMatch
        XCTAssertTrue(cancel.waitForExistence(timeout: 5)); cancel.tap()
    }
    private func back() { app.navigationBars.buttons.element(boundBy: 0).tap() }
    private func start() {
        continueAfterFailure = false
        app.launchEnvironment["FEI_NOTES_MODE"] = "local"
        XCUIDevice.shared.orientation = .portrait; app.launch()
        XCTAssertTrue(app.buttons["fei.settings"].waitForExistence(timeout: 25))
        app.buttons["fei.settings"].tap()
        app.cells["fei.setting.page:General"].tap(); app.cells["fei.setting.language"].tap()
        app.cells["fei.setting.option.language.en"].tap(); back(); back()
        XCTAssertTrue(app.tables["fei.notes.list"].waitForExistence(timeout: 5))
    }
    func testSearchNavigationAndToolbarLifecycle() {
        start()
        let search = app.searchFields["fei.notes.search-field"]
        let create = app.buttons["fei.new"]
        XCTAssertTrue(search.isHittable); XCTAssertTrue(create.isHittable)
        XCTAssertGreaterThan(search.frame.minY,app.frame.height*0.6)
        XCTAssertGreaterThan(create.frame.minX,search.frame.maxX)
        shot("iphone43-01-list-search-compose")
        search.tap(); search.typeText("NO_MATCH_43")
        XCTAssertTrue(app.staticTexts["No matching documents"].waitForExistence(timeout: 5))
        wait({ self.app.keyboards.firstMatch.exists },"Search owns the keyboard")
        XCTAssertFalse(create.isHittable)
        wait({ search.frame.maxY <= self.app.keyboards.firstMatch.frame.minY },"Search must clear the keyboard")
        shot("iphone43-02-search-native-input")
        search.buttons.matching(NSPredicate(format: "label IN {'Clear text','清除文本'}")).firstMatch.tap()
        XCTAssertTrue(app.keyboards.firstMatch.exists)
        XCTAssertFalse(app.staticTexts["No matching documents"].exists)
        cancelSearch(); wait({ !self.app.keyboards.firstMatch.exists },"Cancel dismisses search input")
        XCTAssertTrue(create.isHittable)
        create.tap()
        let bar = app.toolbars["fei.editor.toolbar"]
        let compose = app.navigationBars.buttons["fei.new"]
        XCTAssertTrue(app.buttons["fei.more"].waitForExistence(timeout: 8))
        wait({ bar.buttons["fei.edit.styles"].isHittable },"New note opens for typing")
        XCTAssertTrue(app.buttons["fei.edit.done"].isHittable)
        XCTAssertTrue(bar.buttons["fei.edit.undo"].exists)
        XCTAssertFalse(app.navigationBars.buttons["fei.edit.undo"].exists)
        for id in ["styles","list","table","insert","keyboard"] { XCTAssertTrue(app.buttons["fei.edit."+id].isHittable) }
        let marker = "Phone polish "+String(Int(Date().timeIntervalSince1970))
        app.otherElements["fei.editor.surface"].coordinate(withNormalizedOffset: CGVector(dx: 0.25,dy: 0.4)).tap()
        app.typeText("Synthetic toolbar and search body")
        shot("iphone43-03-editing-above-keyboard")
        app.buttons["fei.edit.done"].tap()
        wait({ bar.buttons["fei.edit.mode"].isHittable },"Done restores reading shortcuts")
        XCTAssertFalse(app.buttons["fei.edit.done"].exists)
        XCTAssertFalse(app.keyboards.firstMatch.exists)
        XCTAssertTrue(bar.buttons["fei.edit.quick-checklist"].isHittable)
        XCTAssertTrue(bar.buttons["fei.edit.quick-insert"].isHittable)
        XCTAssertTrue(compose.isHittable)
        XCTAssertLessThan(compose.frame.midY,bar.frame.minY)
        shot("iphone43-04-done-reading-shortcuts")
        bar.buttons["fei.edit.mode"].tap()
        wait({ bar.buttons["fei.edit.styles"].isHittable },"Edit resumes typing")
        bar.buttons["fei.edit.styles"].tap()
        XCTAssertTrue(app.tables["fei.format.controls"].waitForExistence(timeout: 5))
        shot("iphone43-05-format-modal")
        app.buttons["fei.format.done"].tap()
        app.buttons["fei.more"].tap(); app.sheets.buttons["Rename"].tap()
        let title = app.textFields["fei.rename.title"]
        XCTAssertTrue(title.waitForExistence(timeout: 5)); title.tap()
        title.typeText(String(repeating: XCUIKeyboardKey.delete.rawValue,count: ((title.value as? String)?.count ?? 20)*2))
        title.typeText(marker); app.buttons["fei.rename.save"].tap()
        back(); XCTAssertTrue(search.waitForExistence(timeout: 5))
        search.tap(); search.typeText(marker); search.typeText("\n")
        wait({ !self.app.keyboards.firstMatch.exists },"Submit keeps results without keyboard")
        let row = app.tables["fei.notes.list"].cells.containing(.staticText,identifier: marker).firstMatch
        XCTAssertTrue(row.waitForExistence(timeout: 5)); row.tap()
        XCTAssertTrue(app.buttons["fei.more"].waitForExistence(timeout: 5))
        wait({ bar.buttons["fei.edit.mode"].isHittable },"Search result opens without input")
        XCTAssertFalse(app.keyboards.firstMatch.exists)
        back(); XCTAssertEqual(search.value as? String,marker)
        XCTAssertTrue(row.exists); XCTAssertFalse(app.keyboards.firstMatch.exists)
        shot("iphone43-06-search-result-return")
        search.tap(); search.typeText(" no match"); cancelSearch()
        XCTAssertEqual(search.value as? String,marker)
        XCUIDevice.shared.orientation = .landscapeLeft
        XCTAssertTrue(search.waitForExistence(timeout: 5)); XCTAssertTrue(create.isHittable)
        shot("iphone43-07-landscape-list")
        XCUIDevice.shared.orientation = .portrait
        print("IPHONE43_POLISH_ACCEPTANCE_COMPLETED "+marker)
    }
    func testDockedEditorKeyboardGeometryAndTransitions() {
        app.launchEnvironment["FEI_NOTES_LAYOUT_PROBE"] = "1"
        start(); app.buttons["fei.new"].tap()
        let bar = app.toolbars["fei.editor.toolbar"]
        let keyboard = app.keyboards.firstMatch
        let surface = app.otherElements["fei.editor.surface"]
        func docked(_ name:String) {
            wait({ keyboard.exists && keyboard.isHittable },"Editor software keyboard must be visible")
            func geometry() -> [String:[Double]] {
                guard let json = bar.value as? String,let data = json.data(using: .utf8),let result = try? JSONDecoder().decode([String:[Double]].self,from: data) else { return [:] }
                return result
            }
            wait({
                let g = geometry(); guard let k = g["keyboard"],let t = g["toolbar"],let e = g["editor"] else { return false }
                return k[3] > 100 && abs(t[1]+t[3]-k[1]) <= 1 && abs(e[1]+e[3]-t[1]) <= 1
            },"Toolbar and editor follow actual keyboard guide exactly once")
            XCTAssertLessThanOrEqual(bar.frame.maxY,keyboard.frame.minY+2)
            XCTAssertLessThanOrEqual(surface.frame.maxY,bar.frame.minY+2)
            XCTAssertGreaterThan(surface.frame.height,40)
            for id in ["styles","list","table","insert","keyboard"] { XCTAssertTrue(app.buttons["fei.edit."+id].isHittable) }
            print("KEYBOARD44_GUIDE \(name) \(bar.value ?? "nil")")
            print("KEYBOARD44_GEOMETRY \(name) keyboard=\(keyboard.frame) toolbar=\(bar.frame) editor=\(surface.frame)")
            shot("iphone44-"+name)
        }
        docked("01-portrait-docked")
        let letters = Array("abcdefghij")
        let typed = "aa"+String(String(Int(Date().timeIntervalSince1970)).suffix(5).map { letters[$0.wholeNumberValue!] })
        func matchesTitle(_ text:String) -> XCUIElement {
            let pattern = text.map(String.init).joined(separator: "\\s*")
            return app.tables["fei.notes.list"].cells.containing(NSPredicate(format: "label MATCHES %@",pattern)).firstMatch
        }
        for letter in typed { let key = keyboard.keys[String(letter)]; XCTAssertTrue(key.isHittable); key.tap() }
        docked("02-screen-key-input")
        bar.buttons["fei.edit.keyboard"].tap()
        wait({ !keyboard.exists },"Keyboard control ends input")
        wait({ bar.buttons["fei.edit.mode"].isHittable },"Reading tools return")
        let bottom = bar.frame.maxY
        shot("iphone44-03-keyboard-hidden")
        back()
        let composed = matchesTitle(typed)
        XCTAssertTrue(composed.waitForExistence(timeout: 5),"End input preserves unfinished Pinyin as committed text")
        composed.tap()
        XCTAssertTrue(app.buttons["fei.more"].waitForExistence(timeout: 5))
        wait({ bar.buttons["fei.edit.mode"].isHittable && !self.app.tables["fei.notes.list"].isHittable },"Reopened document finishes its native transition")
        bar.buttons["fei.edit.mode"].tap(); docked("04-edit-resumes")
        XCTAssertLessThan(bar.frame.maxY,bottom-100)
        keyboard.keys["b"].tap(); keyboard.keys["b"].tap()
        bar.buttons["fei.edit.styles"].tap()
        XCTAssertTrue(app.tables["fei.format.controls"].waitForExistence(timeout: 5))
        shot("iphone44-05-format")
        app.buttons["fei.format.done"].tap(); docked("06-format-return")
        XCUIDevice.shared.orientation = .landscapeLeft
        docked("07-landscape-docked")
        XCUIDevice.shared.orientation = .portrait
        docked("08-portrait-return")
        app.buttons["fei.edit.done"].tap()
        wait({ !keyboard.exists && bar.buttons["fei.edit.mode"].isHittable },"Done restores reading chrome")
        XCTAssertEqual(bar.frame.maxY,bottom,accuracy: 3)
        shot("iphone44-09-done-reading")
        back()
        XCTAssertTrue(matchesTitle(typed+"bb").waitForExistence(timeout: 5),"Format handoff preserves composition and resumes the same document")
        print("IPHONE44_DOCKED_KEYBOARD_COMPLETED")
    }
    private func setting(_ id: String) {
        let row = app.cells["fei.setting."+id]
        for _ in 0..<10 { if row.exists && row.isHittable { break }; app.tables["fei.settings.list"].swipeUp() }
        XCTAssertTrue(row.waitForExistence(timeout: 5)); row.tap()
    }
    func testLargeTextAndDarkSearchToolbarReachability() {
        app.launchArguments = ["-UIPreferredContentSizeCategoryName","UICTContentSizeCategoryAccessibilityXXXL"]
        start()
        app.buttons["fei.settings"].tap(); setting("page:Appearance")
        setting("theme"); app.cells["fei.setting.option.theme.notes"].tap()
        setting("appearance"); app.cells["fei.setting.option.appearance.dark"].tap()
        back(); back()
        let search = app.searchFields["fei.notes.search-field"]
        XCTAssertTrue(search.isHittable); XCTAssertTrue(app.buttons["fei.new"].isHittable)
        search.tap(); search.typeText("PHONE43_LARGE_NO_MATCH")
        XCTAssertTrue(app.staticTexts["No matching documents"].waitForExistence(timeout: 5))
        XCTAssertFalse(app.buttons["fei.new"].isHittable)
        shot("iphone43-08-large-dark-search")
        cancelSearch(); app.buttons["fei.new"].tap()
        let bar = app.toolbars["fei.editor.toolbar"]
        wait({ bar.buttons["fei.edit.styles"].isHittable },"Large text keeps editing controls")
        for id in ["styles","insert","keyboard"] { XCTAssertTrue(bar.buttons["fei.edit."+id].isHittable) }
        app.buttons["fei.edit.done"].tap()
        wait({ bar.buttons["fei.edit.mode"].isHittable },"Large text keeps reading controls")
        let compose = app.navigationBars.buttons["fei.new"]
        XCTAssertTrue(compose.isHittable)
        XCTAssertLessThan(compose.frame.midY,bar.frame.minY)
        shot("iphone43-09-large-dark-reading")
        back(); app.buttons["fei.settings"].tap(); setting("page:Appearance")
        setting("appearance"); app.cells["fei.setting.option.appearance.light"].tap()
        back(); back()
        print("IPHONE43_LARGE_DARK_ACCEPTANCE_COMPLETED")
    }

}
