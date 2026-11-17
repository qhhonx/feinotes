import XCTest

final class ImageCacheUITests: XCTestCase {
    private func shot(_ name:String) {
        let a = XCTAttachment(screenshot: XCUIScreen.main.screenshot()); a.name = name; a.lifetime = .keepAlways; add(a)
    }
    private func settle() {
        let ready = Date().addingTimeInterval(3)
        XCTAssertEqual(XCTWaiter.wait(for: [XCTNSPredicateExpectation(predicate: NSPredicate { _,_ in Date() >= ready },object: nil)],timeout: 4),.completed)
    }
    func testLargePictureAfterBackgroundAndReopen() {
        continueAfterFailure = false
        let app = XCUIApplication(); app.launchEnvironment["FEI_NOTES_MODE"] = "local"
        XCUIDevice.shared.orientation = .portrait; app.launch()
        XCTAssertTrue(app.buttons["fei.new"].waitForExistence(timeout: 15)); app.buttons["fei.new"].tap()
        XCTAssertTrue(app.buttons["fei.more"].waitForExistence(timeout: 5))
        app.buttons["fei.edit.insert"].tap()
        let insert = app.tables["fei.insert.controls"]
        XCTAssertTrue(insert.waitForExistence(timeout: 5)); insert.cells["fei.insert.image"].tap()
        let file = app.cells.matching(NSPredicate(format: "label CONTAINS %@","FEI_CACHE33_image")).firstMatch
        if !file.waitForExistence(timeout: 2) {
            let browse = app.buttons["浏览"].firstMatch
            if browse.waitForExistence(timeout: 1) { browse.tap() }
            let folders = app.cells.matching(NSPredicate(format: "label BEGINSWITH %@","飞记"))
            let back = app.buttons["DOC.navBarButton.backInHistory"]
            for _ in 0..<3 {
                if file.exists || folders.count > 0 { break }
                if back.exists && back.isEnabled { back.tap() }
                else {
                    let phone = app.cells.matching(NSPredicate(format: "label CONTAINS %@","iPhone")).firstMatch
                    XCTAssertTrue(phone.waitForExistence(timeout: 5),app.debugDescription); phone.tap()
                }
            }
            if !file.exists {
                let count = folders.count
                XCTAssertGreaterThan(count,0,app.debugDescription)
                for index in 0..<count {
                    folders.element(boundBy: index).tap()
                    if file.waitForExistence(timeout: 2) { break }
                    XCTAssertTrue(back.exists); back.tap()
                }
            }
        }
        XCTAssertTrue(file.waitForExistence(timeout: 5),app.debugDescription); file.tap()
        let pickerOpen = app.buttons["打开"].firstMatch
        if pickerOpen.waitForExistence(timeout: 2) { pickerOpen.tap() }
        XCTAssertTrue(app.buttons["fei.more"].waitForExistence(timeout: 10))
        let prefix = "FEI_CACHE33_"+String(Int(Date().timeIntervalSince1970))
        app.buttons["fei.more"].tap(); app.sheets.buttons["重命名"].tap()
        let title = app.textFields["fei.rename.title"]
        XCTAssertTrue(title.waitForExistence(timeout: 5)); title.typeText(prefix); app.buttons["fei.rename.save"].tap()
        XCTAssertTrue(app.navigationBars[prefix].waitForExistence(timeout: 5))
        settle(); shot("cache33-01-loaded-picture")
        XCUIDevice.shared.press(.home); app.activate()
        XCTAssertTrue(app.navigationBars[prefix].waitForExistence(timeout: 5)); settle(); shot("cache33-02-after-background")
        app.navigationBars.buttons.element(boundBy: 0).tap()
        let row = app.tables["fei.notes.list"].cells.containing(.staticText,identifier: prefix).firstMatch
        XCTAssertTrue(row.waitForExistence(timeout: 5)); row.tap()
        XCTAssertTrue(app.navigationBars[prefix].waitForExistence(timeout: 5)); settle(); shot("cache33-03-reopened-picture")
        app.buttons["fei.more"].tap(); app.sheets.buttons["删除笔记"].tap()
        XCTAssertTrue(app.alerts["fei.confirm"].waitForExistence(timeout: 5))
        app.alerts.buttons["fei.confirm.ok"].firstMatch.tap()
        let removed = XCTNSPredicateExpectation(predicate: NSPredicate(format: "exists == false"),object: app.navigationBars[prefix])
        XCTAssertEqual(XCTWaiter.wait(for: [removed],timeout: 10),.completed)
        if app.buttons["fei.more"].exists { app.navigationBars.buttons.element(boundBy: 0).tap() }
        XCTAssertTrue(app.tables["fei.notes.list"].waitForExistence(timeout: 5))
        XCTAssertFalse(app.tables["fei.notes.list"].cells.containing(.staticText,identifier: prefix).firstMatch.exists)
        print("CACHE33_COMPLETED="+prefix)
    }
}
