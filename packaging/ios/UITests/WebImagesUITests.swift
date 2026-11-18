import XCTest

final class WebImagesUITests: XCTestCase {
    private func shot(_ name: String) {
        let attachment = XCTAttachment(screenshot: XCUIScreen.main.screenshot())
        attachment.name = name; attachment.lifetime = .keepAlways; add(attachment)
    }
    func testDownloadRedirectRetryAndRejectInvalidImage() {
        continueAfterFailure = false
        let app = XCUIApplication()
        app.launchEnvironment["FEI_NOTES_MODE"] = "local"
        XCUIDevice.shared.orientation = .portrait
        app.launch()
        XCTAssertTrue(app.buttons["fei.settings"].waitForExistence(timeout: 15))
        func load(_ kind: String) {
            app.buttons["fei.library-actions"].tap()
            app.sheets.buttons["导入笔记"].tap()
            let file = app.cells.matching(NSPredicate(format: "label CONTAINS %@","FEI_WEB22_\(kind)")).firstMatch
            if !file.waitForExistence(timeout: 2) {
                if app.buttons["浏览"].firstMatch.waitForExistence(timeout: 2) { app.buttons["浏览"].firstMatch.tap() }
                if !file.waitForExistence(timeout: 2) {
                    let folder = app.cells.matching(NSPredicate(format: "label BEGINSWITH %@","飞记")).firstMatch
                    if !folder.waitForExistence(timeout: 2) {
                        let phone = app.cells.matching(NSPredicate(format: "label CONTAINS %@","iPhone")).firstMatch
                        XCTAssertTrue(phone.waitForExistence(timeout: 5)); phone.tap()
                    }
                    XCTAssertTrue(folder.waitForExistence(timeout: 5)); folder.tap()
                }
            }
            XCTAssertTrue(file.waitForExistence(timeout: 8)); file.tap()
            if app.buttons["打开"].firstMatch.waitForExistence(timeout: 2) { app.buttons["打开"].firstMatch.tap() }
            XCTAssertTrue(app.navigationBars["FEI_WEB22_\(kind)"].waitForExistence(timeout: 10),app.debugDescription)
        }
        func settings() {
            app.buttons["fei.more"].tap(); app.sheets.buttons["内容设置"].tap()
            let list = app.tables["fei.objects.controls"]
            XCTAssertTrue(list.waitForExistence(timeout: 5)); XCTAssertEqual(list.cells.count,1)
            list.cells.element(boundBy: 0).tap()
            XCTAssertTrue(app.tables["fei.object.form"].waitForExistence(timeout: 5))
        }
        func status(_ value: String) {
            XCTAssertTrue(app.cells["fei.object.row.web-status"].staticTexts[value].waitForExistence(timeout: 15),app.debugDescription)
        }
        func close() { app.buttons["fei.object.save"].tap(); XCTAssertTrue(app.buttons["fei.more"].waitForExistence(timeout: 5)) }
        func home() { app.navigationBars.buttons["返回"].firstMatch.tap(); XCTAssertTrue(app.buttons["fei.settings"].waitForExistence(timeout: 5)) }
        load("redirect"); settings(); status("已缓存"); shot("web-01-cached-redirect"); close()
        app.buttons["fei.more"].tap(); app.sheets.buttons["图片和附件"].tap()
        let attachments = app.tables["fei.attachments.controls"]
        XCTAssertTrue(attachments.waitForExistence(timeout: 5)); XCTAssertTrue(attachments.cells.element(boundBy: 0).isEnabled)
        attachments.cells.element(boundBy: 0).tap()
        XCTAssertTrue(app.buttons["fei.file.done"].waitForExistence(timeout: 10)); shot("web-02-native-cache-preview")
        app.buttons["fei.file.done"].tap(); home()
        load("retry"); settings(); status("加载失败，可稍后重试"); shot("web-03-failure-with-retry")
        app.cells["fei.object.row.retry"].tap(); status("已缓存"); shot("web-04-native-retry-succeeded"); close(); home()
        load("bad"); settings(); status("加载失败，可稍后重试"); XCTAssertFalse(app.cells["fei.object.row.crop"].exists)
        shot("web-05-invalid-image-rejected"); close(); home()
        print("WEB_IMAGES22_COMPLETED")
    }
    func testHTTPSAndInspectDownloadPreference() {
        continueAfterFailure = false
        let app = XCUIApplication()
        app.launchEnvironment["FEI_NOTES_MODE"] = "local"
        app.launch()
        XCTAssertTrue(app.buttons["fei.settings"].waitForExistence(timeout: 15))
        app.buttons["fei.library-actions"].tap(); app.sheets.buttons["导入笔记"].tap()
        let file = app.cells.matching(NSPredicate(format: "label CONTAINS %@","FEI_WEB22_https")).firstMatch
        XCTAssertTrue(file.waitForExistence(timeout: 8),app.debugDescription); file.tap()
        if app.buttons["打开"].firstMatch.waitForExistence(timeout: 2) { app.buttons["打开"].firstMatch.tap() }
        XCTAssertTrue(app.navigationBars["FEI_WEB22_https"].waitForExistence(timeout: 10))
        app.buttons["fei.more"].tap(); app.sheets.buttons["内容设置"].tap()
        let list = app.tables["fei.objects.controls"]
        XCTAssertTrue(list.waitForExistence(timeout: 5)); list.cells.element(boundBy: 0).tap()
        XCTAssertTrue(app.cells["fei.object.row.web-status"].staticTexts["已缓存"].waitForExistence(timeout: 30),app.debugDescription)
        shot("web-06-public-https-cached")
        app.buttons["fei.object.save"].tap(); app.navigationBars.buttons["返回"].firstMatch.tap()
        app.buttons["fei.settings"].tap()
        app.coordinate(withNormalizedOffset: CGVector(dx: 0.3,dy: 0.167)).tap()
        XCTAssertTrue(app.navigationBars["通用"].waitForExistence(timeout: 5),app.debugDescription)
        shot("web-07-shared-download-preference")
        print("WEB_IMAGES22_HTTPS_COMPLETED")
    }

    func testManualLoadWhenAutomaticDownloadIsOff() {
        continueAfterFailure = false
        let app = XCUIApplication()
        app.launchEnvironment["FEI_NOTES_MODE"] = "local"
        app.launch()
        XCTAssertTrue(app.buttons["fei.settings"].waitForExistence(timeout: 15))
        func preferences() {
            app.buttons["fei.settings"].tap()
            app.coordinate(withNormalizedOffset: CGVector(dx: 0.3,dy: 0.167)).tap()
            XCTAssertTrue(app.navigationBars["通用"].waitForExistence(timeout: 5))
        }
        preferences()
        app.coordinate(withNormalizedOffset: CGVector(dx: 0.85,dy: 0.306)).tap()
        shot("web-08-automatic-download-off")
        app.navigationBars.buttons["完成"].tap()
        XCTAssertTrue(app.buttons["fei.settings"].waitForExistence(timeout: 5))
        app.buttons["fei.library-actions"].tap(); app.sheets.buttons["导入笔记"].tap()
        let file = app.cells.matching(NSPredicate(format: "label CONTAINS %@","FEI_WEB22_manual")).firstMatch
        XCTAssertTrue(file.waitForExistence(timeout: 8),app.debugDescription); file.tap()
        if app.buttons["打开"].firstMatch.waitForExistence(timeout: 2) { app.buttons["打开"].firstMatch.tap() }
        XCTAssertTrue(app.navigationBars["FEI_WEB22_manual"].waitForExistence(timeout: 10))
        app.buttons["fei.more"].tap(); app.sheets.buttons["内容设置"].tap()
        let list = app.tables["fei.objects.controls"]
        XCTAssertTrue(list.waitForExistence(timeout: 5)); list.cells.element(boundBy: 0).tap()
        XCTAssertTrue(app.cells["fei.object.row.web-status"].staticTexts["尚未下载"].waitForExistence(timeout: 5))
        shot("web-09-no-automatic-request")
        print("WEB_IMAGES22_MANUAL_REQUEST=\(Date().timeIntervalSince1970)")
        app.cells["fei.object.row.retry"].tap()
        XCTAssertTrue(app.cells["fei.object.row.web-status"].staticTexts["已缓存"].waitForExistence(timeout: 15))
        app.buttons["fei.object.save"].tap()
        app.buttons["fei.more"].tap(); app.sheets.buttons["图片和附件"].tap()
        let attachments = app.tables["fei.attachments.controls"]
        XCTAssertTrue(attachments.waitForExistence(timeout: 5)); attachments.cells.element(boundBy: 0).tap()
        XCTAssertTrue(app.buttons["fei.file.done"].waitForExistence(timeout: 10))
        RunLoop.current.run(until: Date().addingTimeInterval(1))
        XCTAssertFalse(app.staticTexts["数据"].exists,app.debugDescription)
        shot("web-10-typed-image-preview")
        app.buttons["fei.file.done"].tap(); app.navigationBars.buttons["返回"].firstMatch.tap()
        preferences(); app.coordinate(withNormalizedOffset: CGVector(dx: 0.85,dy: 0.306)).tap()
        app.navigationBars.buttons["完成"].tap()
        XCTAssertTrue(app.buttons["fei.settings"].waitForExistence(timeout: 5))
        print("WEB_IMAGES22_MANUAL_COMPLETED")
    }

}
