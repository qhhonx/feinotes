import XCTest

final class InteractionAcceptanceUITests: XCTestCase {
    private func shot(_ name:String) {
        let a = XCTAttachment(screenshot: XCUIScreen.main.screenshot()); a.name = name; a.lifetime = .keepAlways; add(a)
    }
    func testNativeHomeSearchNavigationAndKeyboard() {
        continueAfterFailure = false
        let app = XCUIApplication()
        app.launchEnvironment["FEI_NOTES_MODE"] = "local"
        XCUIDevice.shared.orientation = .portrait
        app.launch()
        let list = app.tables["fei.notes.list"]
        XCTAssertTrue(list.waitForExistence(timeout: 15),app.debugDescription)
        shot("interaction-01-native-home")
        let search = app.searchFields["fei.notes.search-field"]
        XCTAssertTrue(search.exists,app.debugDescription)
        search.tap(); XCTAssertTrue(app.keyboards.firstMatch.waitForExistence(timeout: 5))
        let nativeKeyboard = app.keyboards.firstMatch
        let nativeVisible = XCTNSPredicateExpectation(predicate: NSPredicate { _,_ in nativeKeyboard.exists && nativeKeyboard.isHittable && nativeKeyboard.frame.height > 100 && nativeKeyboard.frame.minY < app.frame.maxY-100 },object: nativeKeyboard)
        let nativeResult = XCTWaiter.wait(for: [nativeVisible],timeout: 5)
        print("INTERACTION30_NATIVE_SEARCH_KEYBOARD frame=\(nativeKeyboard.frame) hittable=\(nativeKeyboard.isHittable)")
        XCTAssertEqual(nativeResult,.completed,"Native search software keyboard must be visible")
        let query = "FEI_NO_MATCH_30"
        search.typeText(query)
        XCTAssertTrue(app.staticTexts["没有找到匹配的结果"].waitForExistence(timeout: 5))
        XCTAssertTrue(app.keyboards.firstMatch.exists,"Search must keep its first responder during result updates")
        search.typeText(String(repeating: XCUIKeyboardKey.delete.rawValue,count: query.count))
        app.buttons["fei.settings"].tap()
        XCTAssertTrue(app.navigationBars["设置"].waitForExistence(timeout: 5))
        XCTAssertFalse(app.buttons["fei.done-settings"].exists)
        app.navigationBars.buttons.element(boundBy: 0).tap()
        XCTAssertTrue(list.waitForExistence(timeout: 5))
        app.buttons["fei.new"].tap()
        XCTAssertTrue(app.buttons["fei.more"].waitForExistence(timeout: 5))
        XCTAssertTrue(app.keyboards.firstMatch.waitForExistence(timeout: 8),"New note must show the keyboard after its push transition")
        let keyboard = app.keyboards.firstMatch
        let visible = XCTNSPredicateExpectation(predicate: NSPredicate { _,_ in keyboard.exists && keyboard.isHittable && keyboard.frame.height > 100 && keyboard.frame.minY < app.frame.maxY-100 },object: keyboard)
        let result = XCTWaiter.wait(for: [visible],timeout: 5)
        print("INTERACTION30_KEYBOARD frame=\(keyboard.frame) hittable=\(keyboard.isHittable) app=\(app.frame)")
        XCTAssertEqual(result,.completed,"The software keyboard must actually be visible and hittable")
        shot("interaction-02-new-note-keyboard")
        let prefix = "FEI_INTERACTION30_"+String(Int(Date().timeIntervalSince1970))
        app.coordinate(withNormalizedOffset: CGVector(dx: 0.3,dy: 0.27)).tap()
        app.typeText(prefix+" body")
        app.buttons["fei.more"].tap(); app.sheets.buttons["重命名"].tap()
        let name = app.textFields["fei.rename.title"]
        XCTAssertTrue(name.waitForExistence(timeout: 5)); name.typeText(prefix); app.buttons["fei.rename.save"].tap()
        XCTAssertTrue(app.navigationBars[prefix].waitForExistence(timeout: 5))
        app.navigationBars.buttons.element(boundBy: 0).tap()
        XCTAssertTrue(list.waitForExistence(timeout: 5))
        let row = list.cells.containing(.staticText,identifier: prefix).firstMatch
        XCTAssertTrue(row.waitForExistence(timeout: 5),app.debugDescription)
        shot("interaction-03-native-row-after-back")
        row.press(forDuration: 0.8)
        XCTAssertTrue(app.buttons["重命名"].firstMatch.waitForExistence(timeout: 5),app.debugDescription)
        shot("interaction-04-anchored-context-menu")
        app.buttons["重命名"].firstMatch.tap()
        XCTAssertTrue(name.waitForExistence(timeout: 5),app.debugDescription)
        app.buttons["取消"].firstMatch.tap()
        XCTAssertTrue(list.waitForExistence(timeout: 5))
        row.tap()
        XCTAssertTrue(app.navigationBars[prefix].waitForExistence(timeout: 5))
        app.coordinate(withNormalizedOffset: CGVector(dx: 0.3,dy: 0.28)).tap()
        XCTAssertTrue(app.keyboards.firstMatch.waitForExistence(timeout: 5))
        shot("interaction-05-reopen-and-tap-keyboard")
        print("INTERACTION30_COMPLETED="+prefix)
    }
}
