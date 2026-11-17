import XCTest

/// Optional website captures from a caller-provided synthetic library; no account or live cloud data.
final class WebsiteSyncUITests: XCTestCase {
    private let app = XCUIApplication(bundleIdentifier: "app.feinotes.mobile.feedback45")
    override func tearDown() { app.terminate(); super.tearDown() }
    private func back() {
        app.navigationBars.buttons.matching(NSPredicate(format: "label IN {'Back','返回','Settings','设置','General','通用'}")).firstMatch.tap()
    }
    private func capture(_ language: String,_ title: String) throws {
        guard let fixtures = ProcessInfo.processInfo.environment["FEI_WEBSITE_FIXTURES"] else { throw XCTSkip("Synthetic website fixtures were not supplied") }
        continueAfterFailure = false
        app.launchEnvironment["FEI_NOTES_MODE"] = "local"
        app.launchEnvironment["FEI_MOBILE_QA_FIXTURES"] = fixtures + "/" + language
        XCUIDevice.shared.orientation = .portrait
        app.launch()
        XCTAssertTrue(app.buttons["fei.settings"].waitForExistence(timeout: 30))
        app.buttons["fei.settings"].tap()
        app.cells["fei.setting.page:General"].tap()
        app.cells["fei.setting.language"].tap()
        app.cells["fei.setting.option.language." + language].tap()
        back(); back()
        app.buttons["fei.settings"].tap()
        app.cells["fei.setting.page:Appearance"].tap()
        app.cells["fei.setting.appearance"].tap()
        app.cells["fei.setting.option.appearance.light"].tap()
        back(); back()
        let list = app.tables["fei.notes.list"]
        let note = list.cells.containing(.staticText, identifier: title).firstMatch
        XCTAssertTrue(note.waitForExistence(timeout: 10)); note.tap()
        XCTAssertTrue(app.otherElements["fei.editor.surface"].waitForExistence(timeout: 10))
        let settled = Date().addingTimeInterval(2)
        XCTAssertEqual(XCTWaiter.wait(for: [XCTNSPredicateExpectation(predicate: NSPredicate { _, _ in Date() >= settled }, object: nil)], timeout: 5), .completed)
        let screen = XCUIScreen.main.screenshot()
        let output = FileManager.default.urls(for: .documentDirectory, in: .userDomainMask)[0].appendingPathComponent("website-sync-iphone-" + language + ".png")
        try screen.pngRepresentation.write(to: output, options: .atomic)
        let screenshot = XCTAttachment(screenshot: screen)
        screenshot.name = "website-sync-iphone-" + language
        screenshot.lifetime = .keepAlways
        add(screenshot)
    }
    func testEnglish() throws { try capture("en", "A weekend by the sea") }
    func testChinese() throws { try capture("zh", "把周末留给海边") }

}
