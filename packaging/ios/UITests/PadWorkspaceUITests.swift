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
        XCTAssertFalse(app.buttons["fei.settings.close"].exists)
        app.navigationBars["General"].buttons.element(boundBy: 0).tap()
        if pad { app.buttons["fei.settings"].tap() }
        else { app.navigationBars["Settings"].buttons["BackButton"].tap() }
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
        XCTAssertGreaterThanOrEqual(list.frame.width,280)
        XCTAssertEqual(list.frame.width/app.frame.width,1.0/3,accuracy: 0.06)
        print("FEI_PAD_COLUMNS=\(list.frame.width)/\(app.frame.width)")
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
        tabs.buttons["All Favorites"].tap(); XCTAssertTrue(tabs.buttons["All Favorites"].isSelected)
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
        let edge = list.coordinate(withNormalizedOffset: CGVector(dx: 0.35,dy: 0.4))
        let inside = list.coordinate(withNormalizedOffset: CGVector(dx: 0.72,dy: 0.4))
        edge.press(forDuration: 0.04,thenDragTo: inside)
        XCTAssertTrue(app.navigationBars[marker].waitForExistence(timeout: 5))
        app.buttons["fei.browser.parent"].tap(); XCTAssertTrue(tabs.waitForExistence(timeout: 5)); XCTAssertTrue(tabs.buttons["My documents"].isSelected)
        XCUIDevice.shared.orientation = .portrait; XCTAssertTrue(list.waitForExistence(timeout: 5)); shot("ipad-home-portrait")
        XCUIDevice.shared.orientation = .landscapeLeft; wait("Landscape restores two columns") { self.list.isHittable }; assertTwoColumns()
        app.buttons["fei.library-actions"].tap(); app.buttons["Select documents"].tap()
        XCTAssertTrue(app.buttons["fei.notes.selection.done"].waitForExistence(timeout: 5)); XCTAssertTrue(tabs.exists)
        app.buttons["fei.notes.selection.done"].tap(); XCTAssertTrue(tabs.waitForExistence(timeout: 5))
        app.buttons["fei.settings"].tap(); XCTAssertTrue(app.cells["fei.setting.page:Library"].waitForExistence(timeout: 5))
        app.cells["fei.setting.page:Trash"].tap()
        XCTAssertTrue(app.buttons["fei.empty-trash"].waitForExistence(timeout: 5)); shot("ipad-home-trash-from-settings")
    }
    func testPadOverlaySidebarReturnsToParent() {
        start()
        let folder = "PADOVERLAY_" + String(Int(Date().timeIntervalSince1970))
        createFolder(folder)
        XCUIDevice.shared.orientation = .portrait
        wait("The folder stays visible in the overlay") { self.list.isHittable && self.app.navigationBars[folder].exists }
        let edge = list.coordinate(withNormalizedOffset: CGVector(dx: 0.33,dy: 0.38))
        edge.press(forDuration: 0.04,thenDragTo: list.coordinate(withNormalizedOffset: CGVector(dx: 0.73,dy: 0.38)))
        XCTAssertTrue(tabs.waitForExistence(timeout: 6))
        XCTAssertTrue(tabs.buttons["My documents"].isSelected)
        XCTAssertFalse(app.navigationBars[folder].exists)
    }

    func testPadSettingsSidebarFollowsLandscapeState() {
        start()
        let toggle = app.buttons["fei.ipad.focus"]
        XCTAssertEqual(toggle.value as? String,"Expanded")
        toggle.tap()
        wait("The detail can hide the fixed sidebar") { toggle.value as? String == "Collapsed" }
        toggle.tap()
        wait("The fixed sidebar reopens") { toggle.value as? String == "Expanded" && self.list.isHittable }
        app.buttons["fei.settings"].tap()
        XCTAssertTrue(app.cells["fei.setting.page:Library"].waitForExistence(timeout: 5))
        XCTAssertEqual(toggle.value as? String,"Expanded")
        XCTAssertFalse(app.buttons["fei.settings.back"].exists)
        XCTAssertFalse(app.navigationBars["Settings"].buttons["BackButton"].exists)
        app.cells["fei.setting.page:Library"].tap()
        XCTAssertTrue(app.cells["fei.setting.library-info"].waitForExistence(timeout: 5))
        XCTAssertFalse(toggle.isHittable)
        XCTAssertFalse(app.buttons["fei.settings.close"].exists)
        app.navigationBars["Library"].buttons.element(boundBy: 0).tap()
        XCTAssertTrue(app.cells["fei.setting.page:Library"].waitForExistence(timeout: 5))
        XCTAssertEqual(toggle.value as? String,"Expanded")
        shot("ipad-landscape-sidebar-one-third")
    }

    func testPadFolderAndDocumentShareRowHeight() { folderAndDocumentShareRowHeight(pad: true) }
    func testPhoneFolderAndDocumentShareRowHeight() { folderAndDocumentShareRowHeight(pad: false) }
    private func folderAndDocumentShareRowHeight(pad: Bool) {
        start(pad: pad)
        let name = "QA Folder " + String(Int(Date().timeIntervalSince1970))
        createFolder(name)
        if pad { app.buttons["fei.browser.parent"].tap() }
        else { app.navigationBars[name].buttons["BackButton"].tap() }
        let folder = list.cells.containing(.staticText,identifier: name).firstMatch
        let note = list.cells.containing(.staticText,identifier: "QA49 Linked source").firstMatch
        func compare(_ size: String) {
            XCTAssertTrue(folder.waitForExistence(timeout: 5))
            XCTAssertTrue(note.waitForExistence(timeout: 5))
            XCTAssertEqual(folder.frame.height,note.frame.height,accuracy: 1)
            XCTAssertFalse(folder.staticTexts["Folder"].exists)
            print("FEI_LIST_HEIGHT_\(size)=\(folder.frame.height)/\(note.frame.height)")
            shot((pad ? "ipad" : "iphone")+"-folder-and-document-"+size)
        }
        compare("normal")
        app.buttons["fei.settings"].tap()
        app.cells["fei.setting.page:Appearance"].tap()
        app.cells["fei.setting.ui-font"].tap()
        app.cells["fei.setting.option.ui-font.large"].tap()
        app.navigationBars["Appearance"].buttons["BackButton"].tap()
        if pad { app.buttons["fei.settings"].tap() }
        else { app.navigationBars["Settings"].buttons["BackButton"].tap() }
        compare("large")
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
        header.tap(); wait("Collapse hides pinned notes") { !note.exists && header.value as? String == "collapsed" }
        shot(pad ? "ipad-pinned-collapsed" : "iphone-pinned-collapsed")
        header.tap(); wait("Expand shows pinned notes") { note.exists && header.value as? String == "expanded" }
    }
}
