import XCTest

final class LibraryUITests: XCTestCase {
    private func shot(_ name: String) {
        let attachment = XCTAttachment(screenshot: XCUIScreen.main.screenshot())
        attachment.name = name
        attachment.lifetime = .keepAlways
        add(attachment)
    }
    private func start() -> XCUIApplication {
        continueAfterFailure = false
        let app = XCUIApplication()
        app.launchEnvironment["FEI_NOTES_MODE"] = "local"
        app.launch()
        XCTAssertTrue(app.buttons["fei.settings"].waitForExistence(timeout: 15))
        return app
    }
    private func rename(_ title: String, in app: XCUIApplication) {
        app.buttons["fei.more"].tap()
        app.sheets.buttons["重命名"].tap()
        let field = app.textFields["fei.rename.title"]
        XCTAssertTrue(field.waitForExistence(timeout: 5))
        field.tap()
        field.typeText(String(repeating: XCUIKeyboardKey.delete.rawValue,count: ((field.value as? String)?.count ?? 20)*2))
        field.typeText(title)
        app.buttons["fei.rename.save"].tap()
        XCTAssertTrue(app.navigationBars[title].waitForExistence(timeout: 5))
    }
    private func newNote(_ title: String, _ body: String, in app: XCUIApplication) {
        app.buttons["fei.new"].tap()
        XCTAssertTrue(app.buttons["fei.more"].waitForExistence(timeout: 5))
        app.coordinate(withNormalizedOffset: CGVector(dx: 0.3,dy: 0.28)).tap()
        app.typeText(body)
        rename(title,in: app)
    }
    private func home(_ app: XCUIApplication) {
        app.navigationBars.buttons.element(boundBy: 0).tap()
        XCTAssertTrue(app.buttons["fei.settings"].waitForExistence(timeout: 5))
    }
    private func options(_ app: XCUIApplication) {
        app.buttons["fei.more"].tap()
        app.sheets.buttons["文档选项"].tap()
        XCTAssertTrue(app.sheets.buttons["创建副本"].waitForExistence(timeout: 5))
    }
    private func confirm(_ app: XCUIApplication, accept: Bool) {
        XCTAssertTrue(app.alerts["fei.confirm"].waitForExistence(timeout: 5),app.debugDescription)
        app.alerts.buttons[accept ? "fei.confirm.ok" : "fei.confirm.cancel"].firstMatch.tap()
        XCTAssertTrue(XCTWaiter.wait(for: [XCTNSPredicateExpectation(predicate: NSPredicate(format: "exists == false"),object: app.alerts["fei.confirm"])],timeout: 5) == .completed)
    }
    private func delete(_ app: XCUIApplication) {
        app.buttons["fei.more"].tap()
        app.sheets.buttons["删除文档"].tap()
    }
    private func trash(_ app: XCUIApplication) {
        app.buttons["fei.library-actions"].tap()
        app.sheets.buttons["回收站"].tap()
        XCTAssertTrue(app.navigationBars["回收站"].waitForExistence(timeout: 5))
    }
    private func search(_ text: String, in app: XCUIApplication) {
        app.coordinate(withNormalizedOffset: CGVector(dx: 0.3,dy: 0.155)).tap()
        XCTAssertTrue(app.keyboards.firstMatch.waitForExistence(timeout: 5),app.debugDescription)
        app.typeText(text)
        RunLoop.current.run(until: Date().addingTimeInterval(0.3))
    }

    func testDuplicateFavoriteAndNativeTrashConfirmations() {
        let app = start()
        let stamp = Int(Date().timeIntervalSince1970)
        print("LIBRARY_TOUCH_STAMP=\(stamp)")
        let title = "FEI_LIBRARY_\(stamp)"
        let copy = title+" 副本"
        newNote(title,"FEI_LIBRARY_BODY_\(stamp)",in: app)
        options(app)
        app.sheets.buttons["创建副本"].tap()
        XCTAssertTrue(app.navigationBars[copy].waitForExistence(timeout: 5))
        options(app)
        app.sheets.buttons["置顶"].tap()
        options(app)
        XCTAssertTrue(app.sheets.buttons["取消置顶"].exists)
        app.sheets.buttons["收藏"].tap()
        options(app)
        XCTAssertTrue(app.sheets.buttons["取消收藏"].exists)
        app.coordinate(withNormalizedOffset: CGVector(dx: 0.01,dy: 0.5)).tap()
        delete(app)
        shot("library-01-native-delete-cancel")
        confirm(app,accept: false)
        XCTAssertTrue(app.navigationBars[copy].exists)
        delete(app)
        confirm(app,accept: true)
        home(app)
        trash(app)
        XCTAssertTrue(app.buttons["fei.empty-trash"].isEnabled)
        app.coordinate(withNormalizedOffset: CGVector(dx: 0.4,dy: 0.155)).tap()
        app.sheets.buttons["恢复"].tap()
        shot("library-02-native-restore")
        confirm(app,accept: true)
        XCTAssertFalse(app.buttons["fei.empty-trash"].isEnabled)
        home(app)
        search(copy,in: app)
        app.coordinate(withNormalizedOffset: CGVector(dx: 0.3,dy: 0.26)).tap()
        XCTAssertTrue(app.navigationBars[copy].waitForExistence(timeout: 5),app.debugDescription)
        options(app)
        XCTAssertTrue(app.sheets.buttons["取消置顶"].exists)
        XCTAssertTrue(app.sheets.buttons["取消收藏"].exists)
        app.sheets.buttons["创建副本"].tap()
        XCTAssertTrue(app.navigationBars[copy+" 副本"].waitForExistence(timeout: 5))
        rename("FEI_PURGE_\(stamp)",in: app)
        delete(app)
        confirm(app,accept: true)
        home(app)
        trash(app)
        app.coordinate(withNormalizedOffset: CGVector(dx: 0.4,dy: 0.155)).tap()
        app.sheets.buttons["彻底删除"].tap()
        shot("library-03-native-purge-cancel")
        confirm(app,accept: false)
        XCTAssertTrue(app.buttons["fei.empty-trash"].isEnabled)
        app.coordinate(withNormalizedOffset: CGVector(dx: 0.4,dy: 0.155)).tap()
        app.sheets.buttons["彻底删除"].tap()
        confirm(app,accept: true)
        XCTAssertFalse(app.buttons["fei.empty-trash"].isEnabled)
        home(app)
        newNote("FEI_EMPTY_\(stamp)","FEI_EMPTY_BODY_\(stamp)",in: app)
        delete(app)
        confirm(app,accept: true)
        home(app)
        trash(app)
        app.buttons["fei.empty-trash"].tap()
        shot("library-04-native-empty-cancel")
        confirm(app,accept: false)
        XCTAssertTrue(app.buttons["fei.empty-trash"].isEnabled)
        app.buttons["fei.empty-trash"].tap()
        confirm(app,accept: true)
        XCTAssertFalse(app.buttons["fei.empty-trash"].isEnabled)
        shot("library-05-empty-trash")
        home(app)
        print("LIBRARY_TOUCH_COMPLETED=\(stamp)")
    }

    func testSharedSearchTitleRankAndClear() {
        let app = start()
        let stamp = Int(Date().timeIntervalSince1970)
        print("SEARCH_TOUCH_STAMP=\(stamp)")
        let query = "FEI_NEED12_\(stamp)"
        newNote(query,"FEI_TITLE_RESULT_\(stamp)",in: app)
        home(app)
        newNote("FEI_BODY12_\(stamp)","Before \(query) after",in: app)
        home(app)
        search(query.lowercased(),in: app)
        shot("search-01-title-before-newer-body-hit")
        app.coordinate(withNormalizedOffset: CGVector(dx: 0.3,dy: 0.26)).tap()
        XCTAssertTrue(app.navigationBars[query].waitForExistence(timeout: 5),app.debugDescription)
        home(app)
        app.coordinate(withNormalizedOffset: CGVector(dx: 0.9,dy: 0.155)).tap()
        search("FEI_NO_MATCH_\(stamp)",in: app)
        shot("search-02-empty-results")
        app.coordinate(withNormalizedOffset: CGVector(dx: 0.9,dy: 0.155)).tap()
        search("fei_body12_\(stamp)",in: app)
        app.coordinate(withNormalizedOffset: CGVector(dx: 0.3,dy: 0.26)).tap()
        XCTAssertTrue(app.navigationBars["FEI_BODY12_\(stamp)"].waitForExistence(timeout: 5))
        home(app)
        app.coordinate(withNormalizedOffset: CGVector(dx: 0.9,dy: 0.155)).tap()
        print("SEARCH_TOUCH_COMPLETED=\(stamp)")
        shot("search-03-clear-and-reopen")
    }

    func testNativeConfirmationCanCancelAndReopen() {
        let app = start()
        let stamp = Int(Date().timeIntervalSince1970)
        let title = "FEI_CONFIRM12_\(stamp)"
        newNote(title,"FEI_CONFIRM_BODY_\(stamp)",in: app)
        for attempt in 0..<2 {
            delete(app)
            XCTAssertTrue(app.alerts["fei.confirm"].waitForExistence(timeout: 5))
            shot("confirm-\(attempt)-native-alert")
            confirm(app,accept: false)
            XCTAssertTrue(app.navigationBars[title].exists)
        }
        home(app)
        print("CONFIRM_REOPEN_COMPLETED=\(stamp)")
    }
}
