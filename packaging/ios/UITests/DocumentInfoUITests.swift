import XCTest

final class DocumentInfoUITests: XCTestCase {
    private func shot(_ name: String) {
        let attachment = XCTAttachment(screenshot: XCUIScreen.main.screenshot())
        attachment.name = name; attachment.lifetime = .keepAlways; add(attachment)
    }
    func testNativeInformationUsesSharedCountsAndRefreshesAfterRename() {
        continueAfterFailure = false
        let prefix = "FEI_INFO27_1794452552"
        let title = prefix + " 文档验收"
        let app = XCUIApplication()
        app.launchEnvironment["FEI_NOTES_MODE"] = "local"
        XCUIDevice.shared.orientation = .portrait
        app.launch()
        XCTAssertTrue(app.buttons["fei.settings"].waitForExistence(timeout: 15))
        app.buttons["fei.library-actions"].tap(); app.sheets.buttons["导入文档"].tap()
        let file = app.cells.matching(NSPredicate(format: "label CONTAINS %@",prefix)).firstMatch
        if !file.waitForExistence(timeout: 2) {
            if app.buttons["浏览"].firstMatch.waitForExistence(timeout: 2) { app.buttons["浏览"].firstMatch.tap() }
            if !file.waitForExistence(timeout: 2) {
                let folder = app.cells.matching(NSPredicate(format: "label BEGINSWITH %@","飞记")).firstMatch
                if !folder.waitForExistence(timeout: 2) { let phone = app.cells.matching(NSPredicate(format: "label CONTAINS %@","iPhone")).firstMatch; XCTAssertTrue(phone.waitForExistence(timeout: 5)); phone.tap() }
                XCTAssertTrue(folder.waitForExistence(timeout: 5)); folder.tap()
            }
        }
        XCTAssertTrue(file.waitForExistence(timeout: 8)); file.tap()
        if app.buttons["打开"].firstMatch.waitForExistence(timeout: 2) { app.buttons["打开"].firstMatch.tap() }
        XCTAssertTrue(app.navigationBars[title].waitForExistence(timeout: 10))
        func info(_ words: Int,_ characters: Int) {
            app.buttons["fei.more"].tap()
            XCTAssertTrue(app.sheets.buttons["文档信息"].waitForExistence(timeout: 5)); app.sheets.buttons["文档信息"].tap()
            let alert = app.alerts["fei.doc-info"]
            XCTAssertTrue(alert.waitForExistence(timeout: 5),app.debugDescription)
            let message = alert.staticTexts.matching(NSPredicate(format: "label CONTAINS %@","总字数：")).firstMatch
            XCTAssertTrue(message.exists,app.debugDescription)
            let value = message.label
            XCTAssertTrue(value.range(of: "总字数：\(words)(?:\\s|$)",options: .regularExpression) != nil,value)
            XCTAssertTrue(value.range(of: "总字符数：\(characters)(?:\\s|$)",options: .regularExpression) != nil,value)
            XCTAssertTrue(value.contains("创建时间：") && value.contains("最近修改：") && value.contains("大小："),value)
            print("DOCUMENT_INFO27_VALUE=" + value.replacingOccurrences(of: "\n",with: " | "))
        }
        func close() { app.alerts.buttons["fei.doc-info.done"].firstMatch.tap(); XCTAssertTrue(XCTWaiter.wait(for: [XCTNSPredicateExpectation(predicate: NSPredicate(format: "exists == false"),object: app.alerts["fei.doc-info"])],timeout: 5) == .completed) }
        info(12,43); shot("info-01-native-shared-statistics"); close()
        app.buttons["fei.more"].tap(); app.sheets.buttons["重命名"].tap()
        let name = app.textFields["fei.rename.title"]
        XCTAssertTrue(name.waitForExistence(timeout: 5)); name.typeText(title+"新"); app.buttons["fei.rename.save"].tap()
        XCTAssertTrue(app.navigationBars[title+"新"].waitForExistence(timeout: 5))
        info(13,44); shot("info-02-refreshed-after-rename"); close()
        app.switchToReadingMode()
        info(13,44)
        XCTAssertEqual(app.keyboards.count,0)
        shot("info-03-reading-mode-access"); close()
        app.navigationBars.buttons["返回"].firstMatch.tap()
        XCTAssertTrue(app.buttons["fei.settings"].waitForExistence(timeout: 5))
        print("DOCUMENT_INFO27_COMPLETED=\(prefix)")
    }
}
