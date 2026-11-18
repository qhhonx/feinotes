import XCTest

final class PadWorkspaceUITests: XCTestCase {
    private let app = XCUIApplication(bundleIdentifier: "app.feinotes.mobile.feedback45")
    private var list: XCUIElement { app.tables["fei.notes.list"] }
    private var tabs: XCUIElement { app.segmentedControls["fei.notes.tabs"] }
    override func tearDown() { app.terminate(); super.tearDown() }
    private func wait(_ reason: String,_ condition: @escaping () -> Bool) {
        XCTAssertEqual(XCTWaiter.wait(for: [XCTNSPredicateExpectation(predicate: NSPredicate { _,_ in condition() },object: nil)],timeout: 8),.completed,reason)
    }
    private func shot(_ name: String) {
        let item = XCTAttachment(screenshot: XCUIScreen.main.screenshot()); item.name = name; item.lifetime = .keepAlways; add(item)
    }
    private func start(pad: Bool = true) {
        continueAfterFailure = false
        app.launchEnvironment["FEI_NOTES_MODE"] = "local"; app.launchEnvironment["FEI_MOBILE_QA_SEED"] = "1"
        XCUIDevice.shared.orientation = pad ? .landscapeLeft : .portrait; app.launch()
        XCTAssertTrue(app.buttons["fei.settings"].waitForExistence(timeout: 30)); app.buttons["fei.settings"].tap()
        app.cells["fei.setting.page:General"].tap(); app.cells["fei.setting.language"].tap(); app.cells["fei.setting.option.language.en"].tap()
        if pad { app.buttons["fei.settings.close"].tap() }
        else { app.navigationBars.buttons.element(boundBy: 0).tap(); app.navigationBars.buttons.element(boundBy: 0).tap() }
        XCTAssertTrue(list.waitForExistence(timeout: 8))
    }
    private func assertTwoColumns() {
        let detail=app.otherElements["fei.ipad.detail"]
        XCTAssertFalse(app.collectionViews["fei.sidebar.list"].exists)
        XCTAssertFalse(app.buttons["fei.sidebar.toggle"].exists)
        XCTAssertTrue(list.isHittable); XCTAssertTrue(detail.exists)
        let content=app.otherElements["fei.editor.surface"]
        if content.exists { XCTAssertLessThanOrEqual(list.frame.maxX,content.frame.minX+2); XCTAssertGreaterThanOrEqual(content.frame.width,400) }
        else { XCTAssertEqual(app.staticTexts["fei.ipad.empty-detail"].frame.midX,(list.frame.maxX+app.frame.maxX)/2,accuracy: 2) }
        XCTAssertGreaterThanOrEqual(list.frame.width,300)
        XCTAssertEqual(app.buttons.matching(identifier: "fei.new").allElementsBoundByIndex.filter { $0.isHittable }.count,1)
    }
    private func createFolder(_ name: String) {
        app.buttons["fei.library-actions"].tap(); app.buttons["New folder"].tap()
        let field=app.textFields["fei.rename.title"]
        XCTAssertTrue(field.waitForExistence(timeout: 5)); field.tap(); field.typeText(name); app.buttons["fei.rename.save"].tap()
        XCTAssertTrue(app.navigationBars[name].waitForExistence(timeout: 5))
    }
    func testHomeTabsNestedFoldersSearchAndRotation() {
        start(); XCTAssertEqual(tabs.buttons.count,3); assertTwoColumns(); shot("ipad-home-two-columns")
        tabs.buttons["Favorites"].tap(); XCTAssertTrue(tabs.buttons["Favorites"].isSelected)
        tabs.buttons["Shared documents"].tap(); XCTAssertTrue(tabs.buttons["Shared documents"].isSelected)
        XCTAssertTrue(app.navigationBars["Home"].exists); shot("ipad-home-shared-tab")
        tabs.buttons["My documents"].tap()
        let marker="PADHOME_"+String(Int(Date().timeIntervalSince1970))
        createFolder(marker); XCTAssertFalse(tabs.exists)
        createFolder(marker+" child"); shot("ipad-home-nested-folder")
        app.buttons["fei.new"].tap(); XCTAssertTrue(app.buttons["fei.more"].waitForExistence(timeout: 8)); assertTwoColumns()
        if app.buttons["fei.edit.done"].exists { app.buttons["fei.edit.done"].tap() }
        list.swipeDown()
        let search=app.searchFields["fei.notes.search-field"]
        XCTAssertTrue(search.waitForExistence(timeout: 5)); search.tap(); search.typeText("NO_MATCH_"+marker)
        XCTAssertTrue(app.staticTexts["No matching documents"].waitForExistence(timeout: 5))
        XCTAssertTrue(app.buttons["fei.more"].exists); shot("ipad-home-search-beside-document")
        app.buttons["fei.notes.search-close"].tap()
        app.buttons["fei.ipad.focus"].tap()
        wait("Focus fills the document column") { self.app.otherElements["fei.ipad.detail"].frame.width > self.app.frame.width*0.9 }
        app.buttons["fei.ipad.focus"].tap(); wait("Home returns after focus") { self.list.isHittable }; assertTwoColumns()
        app.buttons["fei.browser.parent"].tap(); XCTAssertTrue(app.navigationBars[marker].waitForExistence(timeout: 5))
        app.buttons["fei.browser.parent"].tap(); XCTAssertTrue(tabs.waitForExistence(timeout: 5)); XCTAssertTrue(tabs.buttons["My documents"].isSelected)
        XCUIDevice.shared.orientation = .portrait; XCTAssertTrue(list.waitForExistence(timeout: 5)); shot("ipad-home-portrait")
        XCUIDevice.shared.orientation = .landscapeLeft; wait("Landscape restores two columns") { self.list.isHittable }; assertTwoColumns()
        app.buttons["fei.library-actions"].tap(); app.buttons["Select documents"].tap()
        XCTAssertTrue(app.buttons["fei.notes.selection.done"].waitForExistence(timeout: 5)); XCTAssertFalse(tabs.exists)
        app.buttons["fei.notes.selection.done"].tap(); XCTAssertTrue(tabs.waitForExistence(timeout: 5))
        app.buttons["fei.settings"].tap(); XCTAssertTrue(app.cells["fei.setting.page:Library"].waitForExistence(timeout: 5))
        app.cells["fei.setting.page:Library"].tap(); app.cells["fei.setting.open-trash"].tap()
        XCTAssertTrue(app.buttons["fei.empty-trash"].waitForExistence(timeout: 5)); shot("ipad-home-trash-from-settings")
    }
    func testPadPinnedDisclosure() { pinnedDisclosure() }
    func testPhonePinnedDisclosure() { pinnedDisclosure(pad: false) }
    private func pinnedDisclosure(pad: Bool = true) {
        start(pad: pad)
        let note=list.cells.containing(.staticText,identifier: "QA48 English typography gyjpq").firstMatch
        for _ in 0..<12 { if note.isHittable && note.frame.maxY < list.frame.maxY-5 { break }; list.swipeUp() }
        note.swipeRight()
        if app.buttons["Unpin"].exists { app.buttons["Unpin"].tap(); for _ in 0..<12 { if note.isHittable { break }; list.swipeUp() }; note.swipeRight() }
        app.buttons["Pin"].tap(); list.swipeDown(); list.swipeDown(); list.swipeDown()
        let header=app.buttons["fei.notes.pinned-header"]
        XCTAssertTrue(header.waitForExistence(timeout: 5))
        if header.value as? String == "collapsed" { header.tap() }
        XCTAssertTrue(note.exists); shot(pad ? "ipad-home-pinned-disclosure" : "iphone-home-pinned-disclosure")
        header.tap(); wait("Collapse hides pinned notes") { !note.exists }
        header.tap(); XCTAssertTrue(note.waitForExistence(timeout: 5))
    }
}
