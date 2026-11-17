import XCTest

/// Uses only the local QA application and bundled synthetic comparison notes.
final class MobileFeedbackUITests: XCTestCase {
    private let app = XCUIApplication(bundleIdentifier: "app.feinotes.mobile.feedback45")
    override func tearDown() { app.terminate(); super.tearDown() }
    private func wait(_ reason: String,_ condition: @escaping () -> Bool) {
        XCTAssertEqual(XCTWaiter.wait(for: [XCTNSPredicateExpectation(predicate: NSPredicate { _,_ in condition() },object: nil)],timeout: 10),.completed,reason)
    }
    private func shot(_ name: String) {
        let until = Date().addingTimeInterval(0.5); wait("Drawable settles") { Date() >= until }
        let item = XCTAttachment(screenshot: XCUIScreen.main.screenshot()); item.name = name; item.lifetime = .keepAlways; add(item)
    }
    private var list: XCUIElement { app.tables["fei.notes.list"] }
    private var search: XCUIElement { app.searchFields["fei.notes.search-field"] }
    private var surface: XCUIElement { app.otherElements["fei.editor.surface"] }
    private func back() {
        func navigationState() -> String { app.navigationBars.matching(NSPredicate(format:"value BEGINSWITH 'fei-nav'")).firstMatch.value as? String ?? "" }
        let before=navigationState()
        app.navigationBars.buttons.matching(NSPredicate(format:"label IN {'Back','返回','Settings','设置','General','通用','My notes','My documents','Home','主页','我的文档','我的笔记'}")).firstMatch.tap()
        if !before.isEmpty { wait("Native pop is acknowledged before the next action") { let after=navigationState();return after != before && after.hasSuffix("pop=none") } }
    }
    private func start(pad: Bool = false) {
        continueAfterFailure = false
        app.launchEnvironment["FEI_NOTES_MODE"] = "local"; app.launchEnvironment["FEI_MOBILE_QA_SEED"] = "1"
        XCUIDevice.shared.orientation = pad ? .landscapeLeft : .portrait; app.launch()
        XCTAssertTrue(app.buttons["fei.settings"].waitForExistence(timeout: 30))
        app.buttons["fei.settings"].tap()
        XCTAssertTrue(app.cells["fei.setting.page:General"].waitForExistence(timeout: 5)); app.cells["fei.setting.page:General"].tap()
        app.cells["fei.setting.language"].tap(); app.cells["fei.setting.option.language.en"].tap()
        if pad { app.buttons["fei.settings.close"].tap() } else { back(); back() }
        XCTAssertTrue(list.waitForExistence(timeout: 8))
        // Each visual comparison starts from an explicit appearance, not the
        // previous test's persisted preference.
        app.buttons["fei.settings"].tap(); app.cells["fei.setting.page:Appearance"].tap()
        app.cells["fei.setting.appearance"].tap(); app.cells["fei.setting.option.appearance.light"].tap()
        if pad { app.buttons["fei.settings.close"].tap() } else { back(); back() }
    }
    private func row(_ title: String) -> XCUIElement { list.cells.containing(.staticText,identifier: title).firstMatch }
    private func open(_ title: String) {
        let r = row(title)
        for _ in 0..<8 { if r.isHittable && r.frame.maxY < list.frame.maxY-5 { break }; list.swipeUp() }
        XCTAssertTrue(r.isHittable,title); r.tap(); XCTAssertTrue(app.buttons["fei.more"].waitForExistence(timeout: 8))
    }
    private func appearance49(_ appearance:String,pad:Bool,theme:String="notes") {
        app.buttons["fei.settings"].tap(); XCTAssertTrue(app.cells["fei.setting.page:Appearance"].waitForExistence(timeout:5)); app.cells["fei.setting.page:Appearance"].tap()
        app.cells["fei.setting.theme"].tap(); app.cells["fei.setting.option.theme."+theme].tap()
        app.cells["fei.setting.appearance"].tap(); app.cells["fei.setting.option.appearance."+appearance].tap()
        if pad { app.buttons["fei.settings.close"].tap() } else { back(); back() }
    }
    private func probe49() -> [String:Any] {
        if surface.exists,let value=surface.value as? String,let data=value.data(using:.utf8),let p=try? JSONSerialization.jsonObject(with:data) as? [String:Any] { return p }
        guard app.toolbars["fei.editor.toolbar"].exists else { return [:] }
        let value=app.toolbars["fei.editor.toolbar"].value as? String ?? ""
        guard let r=value.range(of:" scroll="),let data=String(value[r.upperBound...]).data(using:.utf8),let p=try? JSONSerialization.jsonObject(with:data) as? [String:Any] else { return [:] }; return p
    }
    func testPhoneFeedback49Navigation() { navigation49() }
    func testPadFeedback49Navigation() { navigation49(pad:true) }
    private func navigation49(pad:Bool=false) {
        start(pad:pad); appearance49("light",pad:pad)
        let prefix=pad ? "ipad49" : "iphone49"
        if !pad {
            let tabs=app.segmentedControls["fei.notes.tabs"]
            XCTAssertEqual(tabs.buttons.count,3)
            tabs.buttons["Shared documents"].tap(); XCTAssertTrue(tabs.buttons["Shared documents"].isSelected)
            XCTAssertFalse(app.cells["fei.shared-list"].exists); shot(prefix+"-shared-tab")
            tabs.buttons["My documents"].tap()
            list.swipeDown();list.swipeDown()
            func titleFrames() -> [CGRect] { app.navigationBars["Home"].staticTexts.matching(identifier:"Home").allElementsBoundByIndex.filter { $0.isHittable }.map(\.frame) }
            let large=titleFrames().map(\.minY).max() ?? 0
            XCTAssertGreaterThan(large,100)
            list.swipeUp(); wait("The system large title collapses into the navigation bar") { titleFrames().contains { $0.minY < large-20 && $0.height < 30 } }
            shot(prefix+"-collapsed-native-title"); list.swipeDown();list.swipeDown();list.swipeDown()
        }
        for style in ["light","dark"] {
            appearance49(style,pad:pad)
            app.buttons["fei.library-actions"].tap(); app.buttons["Select documents"].tap()
            XCTAssertTrue(app.buttons["fei.notes.selection.done"].waitForExistence(timeout:5))
            app.buttons["fei.notes.selection.all"].tap(); shot(prefix+"-neutral-selection-"+style)
            XCTAssertGreaterThan(list.cells.matching(NSPredicate(format:"selected == true")).count,0)
            app.buttons["fei.notes.selection.all"].tap(); XCTAssertEqual(list.cells.matching(NSPredicate(format:"selected == true")).count,0)
            app.buttons["fei.notes.selection.done"].tap()
        }
        app.buttons["fei.settings"].tap(); app.cells["fei.setting.page:Library"].tap(); app.cells["fei.setting.open-trash"].tap()
        XCTAssertTrue(app.buttons["fei.empty-trash"].waitForExistence(timeout:5));back()
        XCTAssertTrue(app.navigationBars["Documents"].waitForExistence(timeout:5)); shot(prefix+"-trash-returns-library")
        if pad { app.buttons["fei.settings.close"].tap() } else { back();back() }
        open("QA49 Linked source")
        func follow(_ target:String) {
          for _ in 0..<(pad ? 2 : 1) {
            wait("Document link has live geometry") { (self.probe49()["blocks"] as? [[String:Any]] ?? []).contains { ($0["text"] as? String)?.contains("\u{fffc}")==true } }
            let block=(probe49()["blocks"] as! [[String:Any]]).first { ($0["text"] as? String)?.contains("\u{fffc}")==true }!
            let rect=block["rect"] as! [Double]
            surface.coordinate(withNormalizedOffset:.zero).withOffset(CGVector(dx:rect[0]+60,dy:rect[1]+min(16,rect[3]/2))).tap()
            if app.navigationBars[target].waitForExistence(timeout:2) { break }
          }
          XCTAssertTrue(app.navigationBars[target].exists)
        }
        follow("QA49 Linked middle");follow("QA49 Linked target");back()
        XCTAssertTrue(app.navigationBars["QA49 Linked middle"].waitForExistence(timeout:5));back()
        XCTAssertTrue(app.navigationBars["QA49 Linked source"].waitForExistence(timeout:5));shot(prefix+"-internal-link-return-source")
    }
    func testPhoneFeedback49ThemesAndCatalog() { themes49() }
    func testPadFeedback49ThemesAndCatalog() { themes49(pad:true) }
    private func themes49(pad:Bool=false) {
        start(pad:pad);let prefix=pad ? "ipad49" : "iphone49"
        for theme in ["notes","feishu"] { for style in ["light","dark"] {
            appearance49(style,pad:pad,theme:theme);open("QA49 Theme containers")
            for _ in 0..<5 { if (probe49()["page"] as? [Double])?.first == 0 { break }; surface.swipeDown() }
            wait("Compact code is laid out") { (self.probe49()["blocks"] as? [[String:Any]] ?? []).contains { ($0["text"] as? String)=="let value = 42;" } }
            let code=(probe49()["blocks"] as! [[String:Any]]).first { ($0["text"] as? String)=="let value = 42;" }!
            let rect=code["rect"] as! [Double]
            if theme=="notes" { XCTAssertLessThan(rect[3],75,"A single code line does not reserve a blank header") }
            XCTAssertGreaterThanOrEqual(probe49()["code_size"] as? Double ?? 0,15)
            shot(prefix+"-"+theme+"-"+style+"-headings-code-inline-quote")
            surface.swipeUp();shot(prefix+"-"+theme+"-"+style+"-lists-multiline-code-table")
            if pad { app.cells["fei.sidebar.all-notes"].coordinate(withNormalizedOffset:CGVector(dx:0.5,dy:0.5)).tap() } else { back() }
            open("QA49 Nested containers");shot(prefix+"-"+theme+"-"+style+"-tasks-callout-grid")
            surface.swipeUp();shot(prefix+"-"+theme+"-"+style+"-nested-table")
            if pad { app.cells["fei.sidebar.all-notes"].coordinate(withNormalizedOffset:CGVector(dx:0.5,dy:0.5)).tap() } else { back() }
        } }
        appearance49("dark",pad:pad);app.buttons["fei.new"].tap()
        XCTAssertTrue(app.buttons["fei.edit.styles"].waitForExistence(timeout:5));app.buttons["fei.edit.styles"].tap()
        wait("Aa opens the original complete inline catalog") { self.probe49()["catalog"] as? Bool == true }
        XCTAssertFalse(app.sheets.firstMatch.exists);XCTAssertFalse(app.tables["fei.format.controls"].exists)
        XCTAssertGreaterThan(probe49()["catalog_items"] as? Int ?? 0,20)
        XCTAssertTrue((probe49()["catalog_labels"] as? [String] ?? []).contains("Countdown"));shot(prefix+"-unified-catalog-from-aa")
        app.buttons["fei.edit.styles"].tap();wait("Aa closes its catalog") { self.probe49()["catalog"] as? Bool == false }
        app.buttons["fei.edit.insert"].tap();wait("Insert uses the same original catalog") { self.probe49()["catalog"] as? Bool == true }
        XCTAssertFalse(app.sheets.firstMatch.exists);shot(prefix+"-unified-catalog-from-insert")
    }
    func testPhoneFeedback49Preview() { preview49() }
    func testPadFeedback49Preview() { preview49(pad:true) }
    private func preview49(pad:Bool=false) {
        start(pad:pad);appearance49("dark",pad:pad);open("QA45 Media")
        shot(pad ? "ipad49-reader-compose-accent" : "iphone49-reader-compose-accent")
        app.buttons["fei.more"].tap();app.buttons["Document information"].tap();app.buttons["Images and attachments"].tap()
        let attachments=app.tables["fei.attachments.controls"]
        XCTAssertTrue(attachments.waitForExistence(timeout:5));attachments.cells.containing(.staticText,identifier:"two-pages.pdf").firstMatch.tap()
        let close=app.buttons["fei.file.done"];XCTAssertTrue(close.waitForExistence(timeout:8))
        XCTAssertTrue(["Close","关闭"].contains(close.label));shot(pad ? "ipad49-preview-close" : "iphone49-preview-close")
        close.tap();XCTAssertTrue(app.buttons["fei.more"].waitForExistence(timeout:5))
    }
    func testPhoneFeedback49BilingualDemo() {
        start();appearance49("light",pad:false);open("Product launch plan");shot("iphone49-demo-en-light")
        back();app.buttons["fei.settings"].tap();app.cells["fei.setting.page:General"].tap()
        app.cells["fei.setting.language"].tap();app.cells["fei.setting.option.language.zh"].tap();back();back()
        open("产品发布计划");shot("iphone49-demo-zh-light")
        back();appearance49("dark",pad:false);open("产品发布计划");shot("iphone49-demo-zh-dark")
    }
    func testPadFeedback49TableGestureAxes() {
        start(pad:true);appearance49("light",pad:true);open("QA49 Tablet overflow table")
        app.buttons["fei.ipad.focus"].tap()
        wait("Table geometry is available") { !(self.probe49()["tables"] as? [[String:Any]] ?? []).isEmpty }
        func tableX() -> Double { ((probe49()["tables"] as? [[String:Any]])?.first?["x"] as? Double) ?? 0 }
        let initial=probe49(),rect=((initial["tables"] as? [[String:Any]])!.first!["rect"] as! [Double])
        let y=rect[1]+min(100,rect[3]/2),x=min(surface.frame.width-60,rect[0]+rect[2]-30)
        let from=surface.coordinate(withNormalizedOffset:.zero).withOffset(CGVector(dx:x,dy:y))
        from.press(forDuration:0.05,thenDragTo:surface.coordinate(withNormalizedOffset:.zero).withOffset(CGVector(dx:60,dy:y)))
        wait("iPad table scroll is horizontal only") { tableX() < -30 }
        XCTAssertEqual(initial["page"] as? [Double],probe49()["page"] as? [Double])
        let horizontal=tableX(),page=probe49()["page"] as? [Double]
        from.press(forDuration:0.05,thenDragTo:surface.coordinate(withNormalizedOffset:.zero).withOffset(CGVector(dx:x,dy:max(20,y-150))))
        wait("Vertical drag inside the iPad table scrolls the page") { self.probe49()["page"] as? [Double] != page }
        XCTAssertEqual(tableX(),horizontal,accuracy:2);shot("ipad49-table-horizontal-page-vertical")
    }
    func testPhoneFeedback48NativeLists() { feedback48() }
    func testPadFeedback48NativeLists() { feedback48(pad: true) }
    private func feedback48(pad: Bool = false) {
        start(pad: pad)
        if !pad { XCTAssertEqual(app.staticTexts["fei.notes.scope-title"].label,"My documents") }
        let title="QA48 English typography gyjpq", note=row(title)
        for _ in 0..<12 { if note.isHittable && note.frame.maxY < list.frame.maxY-5 { break }; list.swipeUp() }
        note.swipeRight()
        let pin=app.buttons["Pin"],unpin=app.buttons["Unpin"]
        if unpin.exists { unpin.tap(); for _ in 0..<12 { if note.isHittable { break }; list.swipeUp() }; note.swipeRight() }
        XCTAssertTrue(pin.waitForExistence(timeout: 5)); pin.tap()
        let toast=app.descendants(matching: .any)["fei.notes.notice"]
        XCTAssertTrue(toast.waitForExistence(timeout: 2))
        XCTAssertGreaterThan(toast.frame.width,toast.frame.height+25); XCTAssertGreaterThanOrEqual(toast.frame.height,44)
        shot(pad ? "ipad48-material-status" : "iphone48-material-status")
        list.swipeDown(); list.swipeDown(); list.swipeDown()
        let header=app.buttons["fei.notes.pinned-header"]
        XCTAssertTrue(header.waitForExistence(timeout: 5))
        if header.value as? String == "collapsed" { header.tap() }
        XCTAssertTrue(note.exists)
        let secondary=note.staticTexts.element(boundBy: 1)
        XCTAssertLessThanOrEqual(secondary.frame.maxY,note.frame.maxY-8)
        header.tap(); wait("Pinned group collapses without unpinning") { !note.exists }
        header.tap(); XCTAssertTrue(note.waitForExistence(timeout: 5))
        shot(pad ? "ipad48-pinned-disclosure-and-typography" : "iphone48-shared-row-pinned-disclosure-typography")
        if !pad { XCTAssertTrue(app.cells["fei.shared-list"].exists) }
        func select() {
            app.buttons["fei.library-actions"].tap(); app.buttons["Select documents"].tap()
            XCTAssertTrue(app.buttons["fei.notes.selection.done"].waitForExistence(timeout: 5))
            XCTAssertFalse(app.sheets.firstMatch.exists); XCTAssertTrue(list.exists)
        }
        select(); note.tap(); wait("Selection stays in the original list") { note.isSelected }
        shot(pad ? "ipad48-inline-selection" : "iphone48-inline-selection")
        app.buttons["fei.notes.selection.move"].tap()
        XCTAssertTrue(app.navigationBars.buttons["Cancel"].waitForExistence(timeout: 5)); app.navigationBars.buttons["Cancel"].tap()
        wait("Cancel Move preserves the in-place selection") { self.app.buttons["fei.notes.selection.done"].exists && note.isSelected }
        app.buttons["fei.notes.selection.all"].tap()
        app.buttons["fei.notes.selection.done"].tap()
        wait("Done restores normal list controls") { self.app.buttons["fei.library-actions"].exists }
        let beforeCopy=app.staticTexts["fei.notes.count"].label
        select(); note.tap(); app.buttons["fei.notes.selection.copy"].tap()
        wait("Copy exits selection and creates a document") { !self.app.buttons["fei.notes.selection.done"].exists && self.app.staticTexts["fei.notes.count"].label != beforeCopy }
        if pad {
            note.tap(); XCTAssertTrue(app.buttons["fei.more"].waitForExistence(timeout: 5))
            app.cells["fei.sidebar.sharing"].coordinate(withNormalizedOffset: CGVector(dx: 0.5,dy: 0.5)).tap()
            wait("Shared documents can be opened from a live document") { self.app.navigationBars.staticTexts["Shared documents"].exists }
            shot("ipad48-shared-documents-from-editor")
            app.cells["fei.sidebar.all-notes"].coordinate(withNormalizedOffset: CGVector(dx: 0.5,dy: 0.5)).tap()
        } else {
            app.cells["fei.shared-list"].tap()
            XCTAssertTrue(app.navigationBars["Shared documents"].waitForExistence(timeout: 5)); back()
        }
        app.buttons["fei.settings"].tap(); app.cells["fei.setting.page:About"].tap()
        XCTAssertTrue(app.staticTexts.matching(NSPredicate(format: "label CONTAINS %@", "Take notes offline, with optional iCloud sync.")).firstMatch.waitForExistence(timeout: 5))
        shot(pad ? "ipad48-about-product-copy" : "iphone48-about-product-copy")
        if pad { app.buttons["fei.settings.close"].tap() } else { back(); back() }
        app.buttons["fei.settings"].tap(); app.cells["fei.setting.page:Appearance"].tap()
        app.cells["fei.setting.appearance"].tap(); app.cells["fei.setting.option.appearance.dark"].tap()
        if pad { app.buttons["fei.settings.close"].tap() } else { back(); back() }
        shot(pad ? "ipad48-dark-list" : "iphone48-dark-list")
        if pad {
            XCUIDevice.shared.orientation = .portrait
            XCTAssertTrue(list.waitForExistence(timeout: 5)); shot("ipad48-portrait-list")
            XCUIDevice.shared.orientation = .landscapeLeft
            app.cells["fei.sidebar.trash"].coordinate(withNormalizedOffset: CGVector(dx: 0.5,dy: 0.5)).tap()
        } else {
            app.buttons["fei.settings"].tap(); app.cells["fei.setting.page:Library"].tap(); app.cells["fei.setting.open-trash"].tap()
        }
        XCTAssertTrue(app.buttons["fei.empty-trash"].waitForExistence(timeout: 5)); XCTAssertEqual(app.buttons["fei.empty-trash"].label,"Empty")
        shot(pad ? "ipad48-trash-empty-label" : "iphone48-trash-empty-label")
    }
    func testPhoneCloudResumeWaitsForNativeInteractions() { cloudResumeGuards() }
    func testPadCloudResumeWaitsForNativeInteractions() { cloudResumeGuards(pad: true) }
    private func cloudResumeGuards(pad: Bool = false) {
        start(pad: pad)
        if pad && !search.isHittable { list.swipeDown() }
        wait("Idle home can resume cloud") { (self.list.value as? String)?.hasPrefix("cloudResume=true") == true }
        search.tap()
        wait("Even an empty native search protects its focus") { (self.list.value as? String)?.hasPrefix("cloudResume=false") == true }
        app.buttons.matching(NSPredicate(format: "label IN {'Cancel','Close','Cancel Search','取消','关闭'}")).firstMatch.tap()
        wait("Closing Search makes home available") { (self.list.value as? String)?.hasPrefix("cloudResume=true") == true }
        app.buttons["fei.library-actions"].tap(); app.buttons["Reorder"].tap()
        XCTAssertTrue(app.buttons["fei.notes.reorder.done"].waitForExistence(timeout: 5))
        XCTAssertTrue((list.value as? String)?.hasPrefix("cloudResume=false") == true)
        app.buttons["fei.notes.reorder.done"].tap()
        wait("Ending native reorder releases the cloud gate") { (self.list.value as? String)?.hasPrefix("cloudResume=true") == true }
        shot(pad ? "ipad-cloud-startup-native-gates" : "iphone-cloud-startup-native-gates")
        app.terminate()
        app.launchEnvironment["FEI_NOTES_MODE"]="cloud"; app.launchEnvironment["FEI_NOTES_CLOUD_CHANNEL"]=UUID().uuidString
        app.launchEnvironment["FEI_QA_SLOW_SIGN_OUT"]="1"
        let began=ProcessInfo.processInfo.systemUptime; app.launch()
        XCTAssertTrue(list.waitForExistence(timeout: 6)); XCTAssertFalse(app.staticTexts["正在打开 iCloud 文档库…"].exists)
        XCTAssertTrue((list.value as? String)?.contains("cloudPending=true") == true,"Account startup is still waiting while home is usable")
        app.buttons["fei.new"].tap(); XCTAssertTrue(app.buttons["fei.more"].waitForExistence(timeout: 3))
        XCTAssertLessThan(ProcessInfo.processInfo.systemUptime-began,15)
        shot(pad ? "ipad-editor-before-cloud-account-response" : "iphone-editor-before-cloud-account-response")
        app.launchEnvironment.removeValue(forKey: "FEI_QA_SLOW_SIGN_OUT")
    }
    func testPhoneColdLaunchComposeAndReturnToolbar() {
        continueAfterFailure = false
        app.launchEnvironment["FEI_NOTES_MODE"]="local"; app.launchEnvironment["FEI_MOBILE_QA_SEED"]="1"
        XCUIDevice.shared.orientation = .portrait; app.launch()
        XCTAssertTrue(list.waitForExistence(timeout: 30))
        XCTAssertTrue(app.buttons["fei.new"].isHittable,"Compose is present before opening Settings")
        shot("iphone47-cold-launch-compose")
        app.buttons["fei.new"].tap()
        XCTAssertTrue(app.buttons["fei.edit.done"].waitForExistence(timeout: 8))
        app.buttons["fei.edit.done"].tap(); back()
        wait("Native list restored") { self.list.isHittable && self.app.buttons["fei.new"].isHittable }
        XCTAssertFalse(app.toolbars["fei.editor.toolbar"].exists && app.toolbars["fei.editor.toolbar"].isHittable)
        for _ in 0..<3 {
            open("QA45 和 Search"); back()
            wait("Only list controls survive pop") { self.list.isHittable && !(self.app.toolbars["fei.editor.toolbar"].exists && self.app.toolbars["fei.editor.toolbar"].isHittable) }
        }
        shot("iphone47-return-list-without-editor-toolbar")
    }
    func testPhoneListActionsPinDragAndTrash() {
        start()
        app.buttons["fei.library-actions"].tap()
        XCTAssertFalse(app.buttons["New note type"].exists); XCTAssertFalse(app.buttons["New Markdown"].exists)
        XCTAssertFalse(app.buttons["Library"].exists)
        shot("iphone47-simplified-list-menu")
        app.coordinate(withNormalizedOffset: CGVector(dx: 0.1,dy: 0.65)).tap()
        let title="QA45 和 Search", note=row(title)
        for _ in 0..<8 { if note.isHittable { break }; list.swipeUp() }
        note.press(forDuration: 1)
        XCTAssertTrue(app.buttons["Pin"].waitForExistence(timeout: 5)); shot("iphone47-grouped-context-menu")
        app.buttons["Pin"].tap()
        list.swipeDown(); list.swipeDown()
        wait("Pinned note is first") { self.list.cells.firstMatch.staticTexts[title].exists }
        shot("iphone47-pinned-section")
        note.swipeRight(); XCTAssertTrue(app.buttons["Unpin"].waitForExistence(timeout: 5)); app.buttons["Unpin"].tap()
        for _ in 0..<10 { if note.isHittable && note.frame.maxY < list.frame.maxY-5 { break }; list.swipeUp() }
        note.swipeLeft(); XCTAssertTrue(app.buttons["Move to"].waitForExistence(timeout: 5)); shot("iphone47-trailing-swipe-actions")
        list.tap()
        app.buttons["fei.settings"].tap(); app.cells["fei.setting.page:Library"].tap()
        XCTAssertTrue(app.cells["fei.setting.open-trash"].exists)
        shot("iphone47-library-settings-groups")
        app.cells["fei.setting.open-trash"].tap()
        XCTAssertTrue(list.waitForExistence(timeout: 5)); XCTAssertFalse(search.exists)
        XCTAssertFalse(app.buttons["fei.new"].exists)
        shot("iphone47-native-trash")
    }
    func testPhoneSearchReturnClearAndStableNavigation() {
        start()
        shot("iphone45-01-list")
        let heights = list.cells.matching(NSPredicate(format: "identifier BEGINSWITH 'fei.note.'")).allElementsBoundByIndex.map { $0.frame.height }
        XCTAssertGreaterThan(heights.count,2); XCTAssertLessThan((heights.max() ?? 0)-(heights.min() ?? 0),2)
        let originalIcons = ["fei.new","fei.library-actions"].map { app.buttons[$0].frame }
        for _ in 0..<3 {
            open("QA45 和 Search")
            app.buttons["fei.more"].tap(); shot("iphone45-02-grouped-note-menu")
            XCTAssertTrue(app.buttons["Share and export"].waitForExistence(timeout: 5))
            app.coordinate(withNormalizedOffset: CGVector(dx: 0.1,dy: 0.65)).tap()
            back(); wait("List restored") { self.list.isHittable }
        }
        for (i,id) in ["fei.new","fei.library-actions"].enumerated() { XCTAssertEqual(app.buttons[id].frame.minX,originalIcons[i].minX,accuracy: 1) }
        search.tap(); search.typeText("和"); search.typeText("\n")
        XCTAssertTrue(row("QA45 和 Search").waitForExistence(timeout: 8)); row("QA45 和 Search").tap()
        XCTAssertTrue(app.buttons["fei.more"].waitForExistence(timeout: 8)); back()
        wait("Search survives result return") { (self.search.value as? String) == "和" }
        search.buttons.matching(NSPredicate(format: "label IN {'Clear text','清除文本'}")).firstMatch.tap()
        wait("Clear recomputes the list after pop") { self.row("QA45 Very long synthetic title which must be truncated on a single line in every list cell").exists }
        shot("iphone45-03-cleared-after-result-return")
        app.buttons.matching(NSPredicate(format: "label IN {'Cancel','Close','Cancel Search','取消','关闭'}")).firstMatch.tap()
        app.buttons["fei.settings"].tap(); XCTAssertTrue(app.navigationBars["Settings"].waitForExistence(timeout: 5))
        app.cells["fei.setting.page:General"].tap(); XCTAssertTrue(app.navigationBars["General"].exists)
        shot("iphone45-04-settings-section-title")
    }
    func testPhoneClearedSearchDoesNotReturnOnClose() {
        start()
        for attempt in 0..<3 {
            search.tap(); search.typeText("和"); search.typeText("\n")
            let first = list.cells.firstMatch
            XCTAssertTrue(first.waitForExistence(timeout: 8)); first.tap()
            XCTAssertTrue(app.buttons["fei.more"].waitForExistence(timeout: 8)); back()
            wait("Query survives result return") { (self.search.value as? String) == "和" }
            search.buttons.matching(NSPredicate(format: "label IN {'Clear text','清除文本'}")).firstMatch.tap()
            wait("Clear restores all notes before closing") { self.row("QA45 Very long synthetic title which must be truncated on a single line in every list cell").exists }
            app.buttons.matching(NSPredicate(format: "label IN {'Cancel','Close','Cancel Search','取消','关闭'}")).firstMatch.tap()
            wait("Closing Search must not resurrect the cleared query") {
                (self.search.value as? String) != "和" && self.row("QA45 Very long synthetic title which must be truncated on a single line in every list cell").exists
            }
            shot("iphone46-search-clear-close-\(attempt)")
        }
    }
    func testPhoneIntegratedSettingsAndSharedList() {
        start()
        app.cells["fei.shared-list"].tap()
        wait("Sharing is a document list, not a settings page") { self.list.isHittable && self.app.navigationBars["Shared documents"].exists }
        XCTAssertFalse(app.cells["fei.setting.page:Shared"].exists)
        shot("iphone46-native-shared-list")
        back(); wait("Back restores personal notes") { self.row("QA45 一、标题与正文").exists }
        app.buttons["fei.settings"].tap()
        XCTAssertFalse(app.cells["fei.setting.page:Cloud"].exists)
        XCTAssertFalse(app.cells["fei.setting.page:Shared"].exists)
        app.cells["fei.setting.page:Library"].tap()
        XCTAssertTrue(app.cells["fei.setting.library-switch"].waitForExistence(timeout: 5))
        shot("iphone46-documents-inline-sync-settings")
    }
    func testPadSharedSidebarKeepsWorkspace() {
        start(pad: true)
        app.cells["fei.sidebar.sharing"].coordinate(withNormalizedOffset: CGVector(dx: 0.5,dy: 0.5)).tap()
        wait("Shared list keeps the native workspace") { self.app.navigationBars["Shared documents"].exists && self.list.isHittable }
        XCTAssertFalse(app.cells["fei.setting.page:Shared"].exists)
        shot("ipad46-shared-browser")
        app.cells["fei.sidebar.all-notes"].coordinate(withNormalizedOffset: CGVector(dx: 0.5,dy: 0.5)).tap()
        wait("Personal sidebar remains available") { self.row("QA45 一、标题与正文").exists }
    }
    func testInvitationWaitsForCloudStartup() {
        continueAfterFailure=false
        app.launchEnvironment["FEI_NOTES_MODE"]="cloud"; app.launchEnvironment["FEI_MOBILE_QA_SEED"]="1"
        app.launchEnvironment["FEI_NOTES_CLOUD_CHANNEL"]=UUID().uuidString
        app.launchEnvironment["FEI_QA_SLOW_SIGN_OUT"]="1"
        app.launchEnvironment["FEI_MOBILE_QA_INVITATION_URL"]="feinotes://share/?c=QAStartupPendingInvitation"
        XCUIDevice.shared.orientation = .portrait; app.launch()
        XCTAssertTrue(list.waitForExistence(timeout: 6))
        XCTAssertTrue((list.value as? String)?.contains("cloudPending=true") == true)
        XCTAssertFalse(app.cells["fei.setting.cloud-required"].exists,"Invitation stays queued during account startup")
        XCTAssertTrue(app.cells["fei.setting.cloud-required"].waitForExistence(timeout: 25),"Queued invitation is delivered after local fallback")
        XCTAssertTrue(app.cells["fei.setting.cloud-required"].isHittable,"Restored invitation must be visible above the portrait browser")
        shot("invitation-restored-after-cloud-startup")
        if app.buttons["fei.settings.close"].exists {
            app.buttons["fei.settings.close"].tap()
            wait("Portrait Home restores the usable middle list") { self.list.isHittable }
            shot("ipad-portrait-home-after-invitation")
        }
    }
    func testPhoneColdInvitationUsesNativeConfirmation() {
        start(); app.terminate()
        app.launchEnvironment["FEI_MOBILE_QA_INVITATION_URL"] = "feinotes://share/?c=QA46SyntheticInvitation"
        app.launch()
        XCTAssertTrue(app.cells["fei.setting.cloud-required"].waitForExistence(timeout: 20))
        XCTAssertTrue(app.navigationBars["Shared Notes"].exists)
        shot("iphone46-cold-invitation-native-route")
        app.launchEnvironment.removeValue(forKey: "FEI_MOBILE_QA_INVITATION_URL")
    }
    func testPhoneCodeControlsAndVideoPreview() {
        start(); open("QA45 四、颜色与正文布局")
        func codeForm() {
            app.buttons["fei.more"].tap(); app.buttons["Editing and formatting"].tap(); app.buttons["Content settings"].tap()
            let objects = app.tables["fei.objects.controls"]
            XCTAssertTrue(objects.waitForExistence(timeout: 5)); objects.cells.firstMatch.tap()
            XCTAssertTrue(app.tables["fei.object.form"].waitForExistence(timeout: 5))
        }
        codeForm()
        let wrap = app.switches["fei.object.wrap"]
        XCTAssertTrue(wrap.isEnabled); let original = wrap.value as? String
        wrap.tap(); app.buttons["fei.object.save"].tap()
        codeForm(); XCTAssertNotEqual(wrap.value as? String,original)
        shot("iphone46-native-code-controls")
        wrap.tap(); app.buttons["fei.object.save"].tap()
        back(); XCTAssertTrue(list.waitForExistence(timeout: 5)); list.swipeDown(); list.swipeDown(); open("QA45 Media")
        app.buttons["fei.more"].tap(); app.buttons["Document information"].tap(); app.buttons["Images and attachments"].tap()
        let attachments = app.tables["fei.attachments.controls"]
        XCTAssertTrue(attachments.waitForExistence(timeout: 5))
        let video = attachments.cells.containing(.staticText,identifier: "video-15s.mp4").firstMatch
        XCTAssertTrue(video.isHittable); video.tap()
        XCTAssertTrue(app.buttons["fei.file.done"].waitForExistence(timeout: 10))
        shot("iphone46-native-video-preview")
        app.buttons["fei.file.done"].tap()
        XCTAssertTrue(app.buttons["fei.more"].waitForExistence(timeout: 5))
    }
    func testPhoneManualDragPersists() { manualDrag() }
    func testPadManualDragPersists() { manualDrag(pad: true) }
    private func manualDrag(pad: Bool = false) {
        start(pad: pad)
        list.swipeDown(); list.swipeDown(); list.swipeDown()
        app.buttons["fei.library-actions"].tap(); app.buttons["Reorder"].tap()
        XCTAssertTrue(app.buttons["fei.notes.reorder.done"].waitForExistence(timeout: 5))
        let cells=list.cells.matching(NSPredicate(format: "identifier BEGINSWITH 'fei.note.'")).allElementsBoundByIndex.filter { $0.isHittable && $0.frame.maxY < list.frame.maxY-5 }
        XCTAssertGreaterThan(cells.count,2)
        let id=cells[0].identifier
        // UIKit's reorder AX child reports an offscreen frame on this runtime.
        // Use the visible, inspected row's trailing native handle instead.
        let handle=cells[0].coordinate(withNormalizedOffset: CGVector(dx: 0.91,dy: 0.5))
        let target=cells[2].coordinate(withNormalizedOffset: CGVector(dx: 0.91,dy: 0.8))
        handle.press(forDuration: 0.3,thenDragTo: target,withVelocity: .slow,thenHoldForDuration: 0.5)
        wait("Native drag changes the displayed order") { self.list.cells.matching(NSPredicate(format: "identifier BEGINSWITH 'fei.note.'")).firstMatch.identifier != id }
        app.buttons["fei.notes.reorder.done"].tap()
        let order=list.cells.allElementsBoundByIndex.map(\.identifier)
        shot("iphone47-native-row-drag")
        app.terminate(); app.launch(); XCTAssertTrue(list.waitForExistence(timeout: 20))
        XCTAssertEqual(Array(list.cells.allElementsBoundByIndex.map(\.identifier).prefix(3)),Array(order.prefix(3)),"Drag order survives restart")
    }
    func testPhoneDarkListsAndCompleteInsertCatalog() {
        start()
        app.buttons["fei.settings"].tap(); app.cells["fei.setting.page:Appearance"].tap()
        app.cells["fei.setting.appearance"].tap(); app.cells["fei.setting.option.appearance.dark"].tap(); back(); back()
        XCTAssertTrue(app.buttons["fei.new"].isHittable); shot("iphone47-dark-home")
        app.buttons["fei.settings"].tap(); app.cells["fei.setting.page:Library"].tap(); shot("iphone47-dark-library-settings")
        app.cells["fei.setting.open-trash"].tap(); XCTAssertTrue(list.waitForExistence(timeout: 8)); shot("iphone47-dark-native-trash")
        back(); open("QA45 一、标题与正文")
        app.buttons["fei.edit.mode"].tap(); app.buttons["fei.edit.styles"].tap()
        XCTAssertFalse(app.tables["fei.format.controls"].exists); shot("iphone47-dark-inline-format")
        app.buttons["fei.edit.insert"].tap()
        XCTAssertTrue(app.cells["fei.insert.insert-group-3"].waitForExistence(timeout: 5)); app.cells["fei.insert.insert-group-3"].tap()
        XCTAssertTrue(app.staticTexts["Countdown"].waitForExistence(timeout: 5)); shot("iphone47-complete-insert-countdown")
        app.buttons["fei.insert.done"].tap(); if app.buttons["fei.edit.done"].exists { app.buttons["fei.edit.done"].tap() }; back()
    }
    func testPhoneTableGestureAxes() {
        start(); open("QA47 Wide table gestures")
        let bar=app.toolbars["fei.editor.toolbar"]
        func probe() -> [String:Any] {
            let value=bar.value as? String ?? ""
            guard let range=value.range(of: " scroll="),let data=String(value[range.upperBound...]).data(using: .utf8),let object=try? JSONSerialization.jsonObject(with: data) as? [String:Any] else { return [:] }
            return object
        }
        func tableX() -> Double { ((probe()["tables"] as? [[String:Any]])?.first?["x"] as? Double) ?? 0 }
        wait("Table geometry is published") { !(probe()["tables"] as? [[String:Any]] ?? []).isEmpty }
        let initial=probe(), rect=((initial["tables"] as? [[String:Any]])!.first!["rect"] as! [Double])
        let y=surface.frame.minY+rect[1]+min(120,rect[3]/2)
        let from=app.coordinate(withNormalizedOffset: .zero).withOffset(CGVector(dx: app.frame.width-70,dy: y))
        let horizontal=app.coordinate(withNormalizedOffset: .zero).withOffset(CGVector(dx: 70,dy: y))
        from.press(forDuration: 0.05,thenDragTo: horizontal)
        wait("Horizontal table gesture moves only table") { tableX() < -30 }
        XCTAssertEqual(initial["page"] as? [Double],probe()["page"] as? [Double])
        let x=tableX(), page=probe()["page"] as? [Double]
        let up=app.coordinate(withNormalizedOffset: .zero).withOffset(CGVector(dx: app.frame.width-70,dy: max(surface.frame.minY+20,y-100)))
        from.press(forDuration: 0.05,thenDragTo: up)
        wait("Vertical gesture starting in a table scrolls the page") { probe()["page"] as? [Double] != page }
        XCTAssertEqual(tableX(),x,accuracy: 2)
        shot("iphone47-table-vertical-page-horizontal-table")
    }
    func testPhoneInlineFormatAndContentComparison() {
        start()
        open("QA45 一、标题与正文"); shot("iphone45-05-headings-inline-marks-reader")
        app.toolbars["fei.editor.toolbar"].buttons["fei.edit.mode"].tap()
        wait("Software keyboard appears") { self.app.keyboards.firstMatch.exists }
        app.buttons["fei.edit.styles"].tap()
        XCTAssertFalse(app.tables["fei.format.controls"].exists,"Aa must use the original inline controls")
        XCTAssertFalse(app.sheets.firstMatch.exists); shot("iphone45-06-original-inline-format")
        app.buttons["fei.edit.done"].tap(); wait("Done dismisses editor input") { !self.app.keyboards.firstMatch.exists }
        back(); open("QA45 二、列表"); shot("iphone45-07-bullets-dashes-numbering")
        back(); open("QA45 三、表格操作"); shot("iphone45-08-table")
        back(); open("QA45 四、颜色与正文布局"); shot("iphone45-09-highlights-alignment-softbreak-code")
        XCUIDevice.shared.orientation = .landscapeLeft; shot("iphone45-10-landscape-content")
        XCUIDevice.shared.orientation = .portrait
    }
    func testPhoneFreshDraftLifecycleAndNestedFolders() {
        start()
        let before = app.staticTexts["fei.notes.count"].label
        app.buttons["fei.new"].tap()
        XCTAssertTrue(app.buttons["fei.more"].waitForExistence(timeout: 8))
        back(); wait("Untouched new draft never enters the library") { self.list.isHittable && self.app.staticTexts["fei.notes.count"].label == before }
        app.buttons["fei.new"].tap()
        wait("New draft owns input") { self.app.keyboards.firstMatch.exists }
        surface.coordinate(withNormalizedOffset: CGVector(dx: 0.3,dy: 0.2)).tap()
        app.typeText("X\n")
        shot("iphone45-draft-input"); print("DRAFT_INPUT \(app.toolbars["fei.editor.toolbar"].value ?? "nil")")
        app.buttons["fei.edit.done"].tap(); back()
        wait("First committed edit persists the new note") { self.app.staticTexts["fei.notes.count"].label != before }
        let edited = list.cells.containing(.staticText,identifier: "X").firstMatch
        XCTAssertTrue(edited.waitForExistence(timeout: 5)); edited.tap()
        XCTAssertTrue(app.buttons["fei.edit.mode"].waitForExistence(timeout: 5)); app.buttons["fei.edit.mode"].tap()
        app.typeText(XCUIKeyboardKey.delete.rawValue+XCUIKeyboardKey.delete.rawValue)
        app.buttons["fei.edit.done"].tap(); back()
        wait("Edited then emptied note is preserved") { self.list.isHittable && self.app.staticTexts["fei.notes.count"].label != before }
        func createFolder(_ title: String) {
            app.buttons["fei.library-actions"].tap()
            XCTAssertTrue(app.buttons["New folder"].waitForExistence(timeout: 5)); app.buttons["New folder"].tap()
            let field = app.textFields["fei.rename.title"]
            XCTAssertTrue(field.waitForExistence(timeout: 5)); field.typeText(title); app.buttons["fei.rename.save"].tap()
        }
        let parent = "QA45 Folder "+String(Int(Date().timeIntervalSince1970))
        createFolder(parent)
        wait("New parent opens immediately") { self.app.staticTexts["fei.notes.scope-title"].label == parent }
        createFolder("Nested child"); back()
        XCTAssertTrue(row("Nested child").waitForExistence(timeout: 5)); shot("iphone45-11-nested-folder")
        row("Nested child").tap(); XCTAssertTrue(app.buttons["fei.library-actions"].waitForExistence(timeout: 5))
        back(); XCTAssertTrue(row("Nested child").waitForExistence(timeout: 5)); back()
        XCTAssertTrue(row(parent).waitForExistence(timeout: 5))
    }
    func testPhoneDarkContentAndKeyPages() {
        start(); app.buttons["fei.settings"].tap(); app.cells["fei.setting.page:Appearance"].tap()
        XCTAssertTrue(app.cells["fei.setting.theme"].waitForExistence(timeout: 5)); app.cells["fei.setting.theme"].tap()
        app.cells["fei.setting.option.theme.notes"].tap()
        app.cells["fei.setting.appearance"].tap(); app.cells["fei.setting.option.appearance.dark"].tap()
        shot("iphone45-dark-01-appearance"); back(); back(); shot("iphone45-dark-02-list")
        open("QA45 一、标题与正文"); shot("iphone45-dark-03-headings"); back()
        open("QA45 二、列表"); shot("iphone45-dark-04-lists"); back()
        open("QA45 三、表格操作"); shot("iphone45-dark-05-table"); back()
        open("QA45 四、颜色与正文布局"); shot("iphone45-dark-06-highlights-code")
    }
    func testPhoneCaretRestorationAndUndo() {
        start(); app.buttons["fei.new"].tap()
        XCTAssertTrue(app.buttons["fei.edit.done"].waitForExistence(timeout: 8))
        surface.coordinate(withNormalizedOffset: CGVector(dx: 0.25,dy: 0.2)).tap(); app.typeText("caret45")
        app.buttons["fei.edit.done"].tap()
        let bar = app.toolbars["fei.editor.toolbar"]
        wait("Committed body is projected") { (bar.value as? String)?.contains("body=caret45") == true }
        let selection = (bar.value as? String)?.components(separatedBy: " selection=").last
        back(); let saved = list.cells.containing(.staticText,identifier: "caret45").firstMatch
        XCTAssertTrue(saved.waitForExistence(timeout: 5)); saved.tap()
        XCTAssertTrue(app.buttons["fei.edit.mode"].waitForExistence(timeout: 5))
        XCTAssertEqual((bar.value as? String)?.components(separatedBy: " selection=").last,selection)
        app.buttons["fei.edit.mode"].tap(); app.typeText("Z"); app.buttons["fei.edit.undo"].tap()
        wait("Undo restores content and the saved caret") { (bar.value as? String)?.contains("body=caret45 selection=") == true }
        XCTAssertEqual((bar.value as? String)?.components(separatedBy: " selection=").last,selection)
        shot("iphone45-caret-restored-after-undo")
        app.buttons["fei.edit.done"].tap()
    }
    func testPhoneIconLaunchAndMedia() {
        continueAfterFailure = false
        app.terminate(); XCUIDevice.shared.press(.home); shot("iphone45-icon-before-launch")
        start(); shot("iphone45-icon-launch-complete")
        open("QA45 Media"); let settled = Date().addingTimeInterval(2); wait("Native thumbnails settle") { Date() >= settled }; shot("iphone45-media-image-pdf")
        surface.swipeUp(); shot("iphone45-media-audio-file-video")
    }
    func testPadDarkMediaAndAdaptivePortrait() {
        start(pad: true)
        app.buttons["fei.settings"].tap(); app.cells["fei.setting.page:Appearance"].tap()
        app.cells["fei.setting.appearance"].tap(); app.cells["fei.setting.option.appearance.dark"].tap()
        app.buttons["fei.settings.close"].tap()
        open("QA45 四、颜色与正文布局"); shot("ipad45-dark-01-highlights-code")
        open("QA45 Media"); let ready = Date().addingTimeInterval(2); wait("Thumbnails settle") { Date() >= ready }
        shot("ipad45-dark-02-media"); surface.swipeUp(); shot("ipad45-dark-03-media-bottom")
        XCUIDevice.shared.orientation = .portrait; shot("ipad45-dark-04-portrait-overlay")
        XCUIDevice.shared.orientation = .landscapeLeft
    }
    func testPadDiagnostic() {
        continueAfterFailure = false
        app.launchEnvironment["FEI_NOTES_MODE"] = "local"; app.launchEnvironment["FEI_MOBILE_QA_SEED"] = "1"
        XCUIDevice.shared.orientation = .landscapeLeft; app.launch()
        XCTAssertTrue(app.buttons["fei.settings"].waitForExistence(timeout: 30))
        shot("ipad45-diagnostic-root")
        print("PAD_DIAGNOSTIC \(app.frame) \(app.state.rawValue) \(app.debugDescription)")
        let r = row("QA45 一、标题与正文")
        XCTAssertTrue(r.waitForExistence(timeout: 5)); r.tap()
        XCTAssertTrue(app.buttons["fei.more"].waitForExistence(timeout: 8))
        shot("ipad45-diagnostic-note"); print("PAD_NOTE \(app.debugDescription)")
    }
    func testPadColumnsReaderInputAndContentComparison() {
        start(pad: true)
        let sidebar = app.collectionViews["fei.sidebar.list"]
        print("PAD_HIT sidebar=\(sidebar.isHittable) cell=\(app.cells["fei.sidebar.all-notes"].isHittable) search=\(search.exists)")
        if !search.isHittable { list.swipeDown() }
        shot("ipad45-preflight-search")
        app.cells["fei.sidebar.favorites"].coordinate(withNormalizedOffset: CGVector(dx: 0.5,dy: 0.5)).tap()
        wait("Sidebar Favorites receives the actual touch") { self.app.cells["fei.sidebar.favorites"].isSelected }
        app.cells["fei.sidebar.all-notes"].coordinate(withNormalizedOffset: CGVector(dx: 0.5,dy: 0.5)).tap()
        wait("Sidebar My notes receives the actual touch") { self.app.cells["fei.sidebar.all-notes"].isSelected }
        XCTAssertTrue(search.isHittable)
        XCTAssertGreaterThan(app.buttons["fei.settings"].frame.minY,app.frame.height*0.7)
        shot("ipad45-01-native-three-columns")
        open("QA45 一、标题与正文")
        XCTAssertFalse(app.keyboards.firstMatch.exists)
        XCTAssertTrue(app.otherElements["fei.ipad.reading-surface"].exists)
        XCTAssertFalse(app.toolbars["fei.editor.toolbar"].isHittable)
        shot("ipad45-02-three-column-reader-top-actions")
        app.otherElements["fei.ipad.reading-surface"].tap()
        wait("First tap closes sidebar without editing") { !sidebar.isHittable }
        XCTAssertFalse(app.keyboards.firstMatch.exists)
        surface.coordinate(withNormalizedOffset: CGVector(dx: 0.3,dy: 0.2)).tap()
        wait("Second tap enters editor input") { self.app.keyboards.firstMatch.exists }
        let done = app.buttons["fei.edit.done"]
        XCTAssertTrue(done.isHittable); XCTAssertGreaterThan(done.frame.minX,app.buttons["fei.more"].frame.minX)
        app.buttons["fei.edit.styles"].tap(); XCTAssertFalse(app.tables["fei.format.controls"].exists)
        shot("ipad45-03-inline-format-top-actions")
        done.tap(); wait("Pad Done exits input") { !self.app.keyboards.firstMatch.exists && !done.exists }
        app.buttons["fei.ipad.focus"].tap()
        wait("Focused note fills the iPad") { self.app.otherElements["fei.ipad.detail"].frame.width > self.app.frame.width*0.9 }
        shot("ipad45-04-focus-content")
        app.buttons["fei.ipad.focus"].tap()
        XCUIDevice.shared.orientation = .portrait; shot("ipad45-05-portrait-overlay-hierarchy")
        XCUIDevice.shared.orientation = .landscapeLeft
        wait("List restored on rotation") { self.list.isHittable }
        open("QA45 二、列表"); shot("ipad45-06-lists")
        open("QA45 三、表格操作"); shot("ipad45-07-table")
        open("QA45 四、颜色与正文布局"); shot("ipad45-08-highlights-alignment-code")
    }
}
