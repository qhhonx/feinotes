import XCTest

final class NativeSettingsUITests: XCTestCase {
    private func shot(_ name:String) { let a = XCTAttachment(screenshot: XCUIScreen.main.screenshot()); a.name = name; a.lifetime = .keepAlways; add(a) }
    private func launch() -> XCUIApplication {
        continueAfterFailure = false
        let app = XCUIApplication(); app.launchEnvironment["FEI_NOTES_MODE"] = "local"
        XCUIDevice.shared.orientation = .portrait; app.launch()
        XCTAssertTrue(app.buttons["fei.settings"].waitForExistence(timeout: 20)); app.buttons["fei.settings"].tap()
        XCTAssertTrue(app.tables["fei.settings.list"].waitForExistence(timeout: 5))
        tapRow("page:General",app); choose("language","zh",app); back(app)
        return app
    }
    private func row(_ id:String,_ app:XCUIApplication) -> XCUIElement { app.cells["fei.setting."+id] }
    private func tapRow(_ id:String,_ app:XCUIApplication) {
        let item = row(id,app)
        for _ in 0..<8 { if item.exists && item.isHittable { break }; app.tables["fei.settings.list"].swipeUp() }
        XCTAssertTrue(item.waitForExistence(timeout: 5),app.debugDescription); item.tap()
    }
    private func back(_ app:XCUIApplication) { app.navigationBars.buttons.element(boundBy: 0).tap() }
    private func choose(_ id:String,_ value:String,_ app:XCUIApplication) {
        tapRow(id,app); XCTAssertTrue(app.tables["fei.settings.choices"].waitForExistence(timeout: 5))
        let item = app.cells["fei.setting.option."+id+"."+value]
        for _ in 0..<12 { if item.exists && item.isHittable { break }; app.tables["fei.settings.choices"].swipeUp() }
        XCTAssertTrue(item.waitForExistence(timeout: 5),app.debugDescription); item.tap()
        XCTAssertTrue(app.tables["fei.settings.list"].waitForExistence(timeout: 5))
    }
    func testLanguageRefresh() {
        let app = launch(); tapRow("page:General",app)
        choose("language","en",app)
        shot("settings38-language-debug")
        XCTAssertTrue(app.navigationBars["General"].waitForExistence(timeout: 5),app.debugDescription)
    }
    func testNativePreferencesAndPersistence() {
        let app = launch(); shot("settings38-01-root")
        for page in ["General","Appearance","Library","Shortcuts","Cloud","Shared","About"] { XCTAssertTrue(row("page:"+page,app).exists) }
        tapRow("page:General",app); shot("settings38-02-general")
        let remote = app.switches["fei.setting.switch.remote-images"]; XCTAssertTrue(remote.exists)
        let old = remote.value as? String; remote.tap()
        choose("new-default","markdown",app); choose("search-engine","bing",app)
        choose("language","en",app); XCTAssertTrue(app.navigationBars["General"].waitForExistence(timeout: 5),app.debugDescription); shot("settings38-03-language")
        app.terminate(); app.launch()
        XCTAssertTrue(app.buttons["fei.settings"].waitForExistence(timeout: 15)); app.buttons["fei.settings"].tap(); tapRow("page:General",app)
        XCTAssertNotEqual(app.switches["fei.setting.switch.remote-images"].value as? String,old)
        XCTAssertTrue(row("new-default",app).staticTexts["Markdown"].exists)
        XCTAssertTrue(row("search-engine",app).staticTexts["Bing"].exists)
        choose("language","zh",app); back(app)
        tapRow("page:Appearance",app); choose("appearance","dark",app); choose("ui-font","large",app); choose("motion","high",app); choose("theme","feishu",app)
        shot("settings38-04-appearance")
        XCTAssertTrue(row("theme-preview",app).exists)
        XCTAssertFalse(row("theme-edit",app).exists,"The product baseline disables CSS editing on both platforms")
        choose("appearance","",app); choose("ui-font","",app)
        back(app); tapRow("page:About",app); shot("settings38-05-about")
        XCTAssertTrue(row("identity",app).staticTexts.matching(NSPredicate(format:"label BEGINSWITH %@","0.1.9 (38)")).firstMatch.exists,app.debugDescription)
        tapRow("notices",app); XCTAssertTrue(app.tables["fei.notices.list"].waitForExistence(timeout: 5)); app.buttons["fei.notices.done"].tap()
        XCTAssertTrue(app.tables["fei.settings.list"].waitForExistence(timeout: 5))
        print("NATIVE_SETTINGS38_PREFERENCES_PASSED")
    }
    func testLibraryCloudAndShortcutsNavigation() {
        let app = launch(); tapRow("page:Library",app)
        choose("history-limit","22",app); shot("settings38-06-library")
        tapRow("restore-backup",app)
        let picker = app.navigationBars.matching(NSPredicate(format:"identifier CONTAINS %@","Browse")).firstMatch
        _ = picker.waitForExistence(timeout: 2)
        app.coordinate(withNormalizedOffset: CGVector(dx: 0.5,dy: 0.075)).press(forDuration: 0.1,thenDragTo: app.coordinate(withNormalizedOffset: CGVector(dx: 0.5,dy: 0.7)))
        XCTAssertTrue(row("history-limit",app).waitForExistence(timeout: 5))
        back(app); tapRow("page:Cloud",app); XCTAssertTrue(row("library-switch",app).exists)
        tapRow("library-switch",app); XCTAssertTrue(app.alerts.firstMatch.waitForExistence(timeout: 5)); shot("settings38-07-switch-confirm")
        app.alerts.firstMatch.buttons["取消"].firstMatch.tap(); back(app)
        tapRow("page:Shared",app); XCTAssertTrue(row("cloud-required",app).exists); back(app)
        tapRow("page:Shortcuts",app); shot("settings38-08-shortcuts")
        let search = app.searchFields["fei.settings.search"]; XCTAssertTrue(search.waitForExistence(timeout: 5))
        search.tap(); search.typeText("Undo"); shot("settings38-09-shortcuts-search")
        search.buttons.firstMatch.tap()
        let item = app.cells.matching(NSPredicate(format:"identifier BEGINSWITH %@","fei.setting.shortcut:")).firstMatch
        XCTAssertTrue(item.waitForExistence(timeout: 5)); item.tap(); shot("settings38-10-shortcut-recorder")
        XCTAssertTrue(app.staticTexts["fei.settings.shortcut.value"].waitForExistence(timeout: 5)); back(app)
        back(app); back(app); XCTAssertTrue(app.tables["fei.notes.list"].waitForExistence(timeout: 5))
        print("NATIVE_SETTINGS38_NAVIGATION_PASSED")
    }
}
