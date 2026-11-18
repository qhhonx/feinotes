import XCTest

/// Opt-in physical-device tests. Only the fixed Development preview channel and synthetic run titles are used.
final class LiveCloudRoundTripUITests: XCTestCase {
    override func setUpWithError() throws {
        try XCTSkipUnless(ProcessInfo.processInfo.environment["FEI_LIVE_CLOUD_TITLE"]?.hasPrefix("FEI_3WAY_") == true,"Live cloud testing must be explicitly requested")
    }
    private let app = XCUIApplication()
    private var title: String { ProcessInfo.processInfo.environment["FEI_LIVE_CLOUD_TITLE"] ?? "FEI_3WAY_20261118_R1" }
    private var role: String { UIDevice.current.userInterfaceIdiom == .pad ? "IPAD" : "IPHONE" }
    private var body: String {
        let live=app.otherElements["fei.qa.editor-state"]
        if live.exists,let value=live.value as? String,let data=value.data(using:.utf8),let json=try? JSONSerialization.jsonObject(with:data) as? [String:Any],let text=json["qa_body"] as? String { return text }
        let surface=app.otherElements["fei.editor.surface"]
        if surface.exists,let value=surface.value as? String,let data=value.data(using:.utf8),let json=try? JSONSerialization.jsonObject(with:data) as? [String:Any],let text=json["qa_body"] as? String { return text }
        let toolbar=app.toolbars["fei.editor.toolbar"]
        return toolbar.exists ? toolbar.value as? String ?? "" : ""
    }
    private func wait(_ message:String,seconds:TimeInterval=45,_ predicate:@escaping ()->Bool) {
        XCTAssertEqual(XCTWaiter.wait(for:[XCTNSPredicateExpectation(predicate:NSPredicate { _,_ in predicate() },object:nil)],timeout:seconds),.completed,message+"\n"+app.debugDescription)
    }
    private func shot(_ name:String) {
        let a = XCTAttachment(screenshot:XCUIScreen.main.screenshot());a.name=role+"-"+name;a.lifetime = .keepAlways;add(a)
    }
    private func start() {
        continueAfterFailure = false
        XCTAssertTrue(title.hasPrefix("FEI_3WAY_"))
        app.launchEnvironment["FEI_NOTES_MODE"]="cloud"
        app.launchEnvironment["FEI_NOTES_CLOUD_CHANNEL"]="9f064dca-1cbc-45ef-a9f6-6f491db971b8"
        app.launchEnvironment["FEI_LIVE_CLOUD_PROBE"]="1"
        XCUIDevice.shared.orientation = UIDevice.current.userInterfaceIdiom == .pad ? .landscapeLeft : .portrait
        app.launch();XCTAssertTrue(app.tables["fei.notes.list"].waitForExistence(timeout:20))
    }
    private func open(_ name:String) {
        let note=app.tables["fei.notes.list"].cells.containing(.staticText,identifier:name).firstMatch
        wait("Synced note appears: "+name,seconds:60) { note.exists }
        note.tap();XCTAssertTrue(app.buttons["fei.more"].waitForExistence(timeout:10))
        XCTAssertTrue(app.otherElements["fei.editor.surface"].waitForExistence(timeout:10))
    }
    private func editSelection(_ text:String) {
        if app.buttons["fei.edit.mode"].exists { app.buttons["fei.edit.mode"].tap() }
        let surface=app.otherElements["fei.editor.surface"]
        surface.coordinate(withNormalizedOffset:CGVector(dx:0.35,dy:0.30)).tap()
        if !app.keyboards.firstMatch.waitForExistence(timeout:2) { surface.coordinate(withNormalizedOffset:CGVector(dx:0.35,dy:0.30)).tap() }
        XCTAssertTrue(app.keyboards.firstMatch.waitForExistence(timeout:5),app.debugDescription)
        surface.coordinate(withNormalizedOffset:CGVector(dx:0.35,dy:0.30)).press(forDuration:1)
        var select=app.descendants(matching:.any).matching(NSPredicate(format:"label IN %@",["Select All","全选"])).firstMatch
        if !select.waitForExistence(timeout:3) {
            let more=app.buttons.matching(NSPredicate(format:"label IN %@",["More","更多"])).firstMatch
            if more.exists { more.tap() }
            select=app.descendants(matching:.any).matching(NSPredicate(format:"label IN %@",["Select All","全选"])).firstMatch
        }
        XCTAssertTrue(select.waitForExistence(timeout:5),app.debugDescription);select.tap()
        // Selection scopes vary by block; verify the required markers after editing.
        let expand=app.descendants(matching:.any).matching(NSPredicate(format:"label IN %@",["Select All","全选"])).firstMatch
        if expand.waitForExistence(timeout:2) { expand.tap() }
        app.typeText(text)
        if app.buttons["fei.edit.done"].exists { app.buttons["fei.edit.done"].tap() }
        for marker in text.components(separatedBy:"\n") where !marker.isEmpty { wait("Edited marker is present: "+marker) { self.body.contains(marker) } }
        shot("edited")
    }
    func testCreateFromThisDevice() {
        start()
        let new=app.buttons.matching(identifier:"fei.new").allElementsBoundByIndex.first { $0.isHittable }
        XCTAssertNotNil(new);new?.tap();XCTAssertTrue(app.buttons["fei.more"].waitForExistence(timeout:10))
        app.otherElements["fei.editor.surface"].coordinate(withNormalizedOffset:CGVector(dx:0.3,dy:0.30)).tap()
        app.typeText("CREATED_ON_"+role+"\n三端云同步测试 "+role)
        if app.buttons["fei.edit.done"].exists { app.buttons["fei.edit.done"].tap() }
        app.buttons["fei.more"].tap()
        shot("document-menu")
        let options=app.buttons.matching(NSPredicate(format:"label IN %@",["Document options","文档选项"])).firstMatch
        XCTAssertTrue(options.waitForExistence(timeout:5),app.debugDescription);options.tap()
        let rename=app.descendants(matching:.any).matching(NSPredicate(format:"label IN %@",["Rename","重命名"])).firstMatch
        XCTAssertTrue(rename.waitForExistence(timeout:5),app.debugDescription);rename.tap()
        let field=app.textFields["fei.rename.title"];XCTAssertTrue(field.waitForExistence(timeout:5))
        field.tap();field.typeText(String(repeating:XCUIKeyboardKey.delete.rawValue,count:((field.value as? String)?.count ?? 80)*2))
        field.typeText(title+"_"+role);app.buttons["fei.rename.save"].tap()
        shot("created-own-note")
    }
    func testReadMacAndEditOnPhone() {
        start();open(title);wait("Mac content downloaded") { self.body.contains("MAC_INIT") }
        editSelection("MAC_INIT\nIPHONE_EDIT\n中文输入与多行同步 123")
    }
    func testReadPhoneAndEditOnPad() {
        start();open(title);wait("Phone content downloaded") { self.body.contains("IPHONE_EDIT") }
        editSelection("MAC_INIT\nIPHONE_EDIT\nIPAD_EDIT\n中文输入与多行同步 123")
    }
    func testReadFinalAndReopen() {
        start();open(title)
        for marker in ["MAC_INIT","IPHONE_EDIT","IPAD_EDIT","MAC_FINAL"] { wait("Downloaded "+marker) { self.body.contains(marker) } }
        shot("converged-before-reopen");app.terminate();start();open(title)
        for marker in ["MAC_INIT","IPHONE_EDIT","IPAD_EDIT","MAC_FINAL"] { wait("Reopened "+marker) { self.body.contains(marker) } }
        shot("converged-after-reopen")
    }
    func testReadMacReturnAndReopenOwnNote() {
        start();open(title+"_"+role)
        wait("Mac return edit downloaded") { self.body.contains("MAC_RETURN") && self.body.contains("CREATED_ON_"+self.role) }
        shot("own-note-round-trip");app.terminate();start();open(title+"_"+role)
        wait("Mac return edit survives relaunch") { self.body.contains("MAC_RETURN") && self.body.contains("CREATED_ON_"+self.role) }
        shot("own-note-reopened")
    }
}
