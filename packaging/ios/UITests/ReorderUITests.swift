import XCTest

final class ReorderUITests: XCTestCase {
    private func shot(_ name: String) {
        let attachment = XCTAttachment(screenshot: XCUIScreen.main.screenshot())
        attachment.name = name; attachment.lifetime = .keepAlways; add(attachment)
    }
    func testNativeDragOrderSurvivesRelaunch() {
        continueAfterFailure = false
        let prefix = "FEI_REORDER26_" + String(Int(Date().timeIntervalSince1970))
        print("REORDER26_PREFIX=\(prefix)")
        let app = XCUIApplication()
        app.launchEnvironment["FEI_NOTES_MODE"] = "local"
        XCUIDevice.shared.orientation = .portrait
        app.launch()
        XCTAssertTrue(app.buttons["fei.settings"].waitForExistence(timeout: 15))
        app.buttons["fei.library-actions"].tap(); app.sheets.buttons["新建文件夹"].tap()
        let name = app.textFields["fei.rename.title"]
        XCTAssertTrue(name.waitForExistence(timeout: 5)); name.typeText(prefix+"_F")
        app.buttons["fei.rename.save"].tap()
        XCTAssertTrue(app.navigationBars[prefix+"_F"].waitForExistence(timeout: 5))
        for kind in ["A","B","C"] {
            app.buttons["fei.new"].tap(); XCTAssertTrue(app.buttons["fei.more"].waitForExistence(timeout: 5))
            app.coordinate(withNormalizedOffset: CGVector(dx: 0.3,dy: 0.28)).tap(); app.typeText(prefix+"_BODY_"+kind)
            app.buttons["fei.more"].tap(); app.sheets.buttons["重命名"].tap()
            XCTAssertTrue(name.waitForExistence(timeout: 5)); name.typeText(prefix+"_"+kind); app.buttons["fei.rename.save"].tap()
            XCTAssertTrue(app.navigationBars[prefix+"_"+kind].waitForExistence(timeout: 5))
            app.navigationBars.buttons["返回"].firstMatch.tap()
            XCTAssertTrue(app.navigationBars[prefix+"_F"].waitForExistence(timeout: 5))
        }
        let rows = app.tables["fei.reorder.rows"]
        func open() {
            app.buttons["fei.library-actions"].tap(); app.sheets.buttons["排序"].tap()
            XCTAssertTrue(app.sheets.buttons["调整顺序"].waitForExistence(timeout: 5),app.debugDescription); app.sheets.buttons["调整顺序"].tap()
            XCTAssertTrue(rows.waitForExistence(timeout: 5),app.debugDescription)
            XCTAssertEqual(rows.cells.count,3)
        }
        func ids() -> [String] { rows.cells.allElementsBoundByIndex.map(\.identifier) }
        func ready() {
            let done = app.buttons["fei.reorder.done"]
            XCTAssertTrue(XCTWaiter.wait(for: [XCTNSPredicateExpectation(predicate: NSPredicate(format: "enabled == true"),object: done)],timeout: 8) == .completed)
        }
        func drag(_ from: Int,_ to: Int) {
            let source = rows.cells.element(boundBy: from).coordinate(withNormalizedOffset: CGVector(dx: 0.94,dy: 0.5))
            let target = rows.cells.element(boundBy: to).coordinate(withNormalizedOffset: CGVector(dx: 0.94,dy: to < from ? 0.1 : 0.9))
            source.press(forDuration: 0.3,thenDragTo: target)
            ready()
        }
        open(); let initial = ids(); print("REORDER26_INITIAL=\(initial.joined(separator: ","))")
        shot("reorder-01-native-drag-handles")
        drag(2,0)
        XCTAssertEqual(ids(),[initial[2],initial[0],initial[1]])
        drag(1,2)
        let expected = [initial[2],initial[1],initial[0]]
        XCTAssertEqual(ids(),expected)
        print("REORDER26_EXPECTED=\(expected.joined(separator: ","))")
        shot("reorder-02-after-two-drags")
        app.buttons["fei.reorder.done"].tap()
        XCTAssertTrue(app.navigationBars[prefix+"_F"].waitForExistence(timeout: 5))
        app.terminate(); app.launch()
        XCTAssertTrue(app.buttons["fei.settings"].waitForExistence(timeout: 15))
        app.coordinate(withNormalizedOffset: CGVector(dx: 0.3,dy: 0.155)).tap(); app.typeText(prefix+"_F")
        app.coordinate(withNormalizedOffset: CGVector(dx: 0.3,dy: 0.26)).tap()
        XCTAssertTrue(app.navigationBars[prefix+"_F"].waitForExistence(timeout: 5),app.debugDescription)
        app.coordinate(withNormalizedOffset: CGVector(dx: 0.89,dy: 0.155)).tap()
        open(); XCTAssertEqual(ids(),expected); shot("reorder-03-persisted-after-relaunch")
        app.buttons["fei.reorder.done"].tap()
        app.navigationBars.buttons["返回"].firstMatch.tap()
        XCTAssertTrue(app.buttons["fei.settings"].waitForExistence(timeout: 5))
        print("REORDER26_COMPLETED=\(prefix)")
    }
}
