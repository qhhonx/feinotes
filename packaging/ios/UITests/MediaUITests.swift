import XCTest

final class MediaUITests: XCTestCase {
    private func shot(_ name: String) {
        let attachment = XCTAttachment(screenshot: XCUIScreen.main.screenshot())
        attachment.name = name
        attachment.lifetime = .keepAlways
        add(attachment)
    }
    private func gone(_ element: XCUIElement) {
        XCTAssertTrue(XCTWaiter.wait(for: [XCTNSPredicateExpectation(predicate: NSPredicate(format: "exists == false"),object: element)],timeout: 5) == .completed)
    }
    private func choose(_ stem: String, in app: XCUIApplication) {
        let file = app.cells.matching(NSPredicate(format: "label CONTAINS %@",stem)).firstMatch
        if !file.waitForExistence(timeout: 2) {
            let browse = app.buttons["浏览"].firstMatch
            if browse.waitForExistence(timeout: 2) { browse.tap() }
            if !file.waitForExistence(timeout: 2) {
                let folder = app.cells.matching(NSPredicate(format: "label BEGINSWITH %@","飞记")).firstMatch
                if !folder.waitForExistence(timeout: 2) {
                    let phone = app.cells.matching(NSPredicate(format: "label CONTAINS %@","iPhone")).firstMatch
                    XCTAssertTrue(phone.waitForExistence(timeout: 5),app.debugDescription)
                    phone.tap()
                }
                XCTAssertTrue(folder.waitForExistence(timeout: 5),app.debugDescription)
                folder.tap()
            }
        }
        XCTAssertTrue(file.waitForExistence(timeout: 10),app.debugDescription)
        file.tap()
        let open = app.buttons["打开"].firstMatch
        if open.waitForExistence(timeout: 2) { open.tap() }
        XCTAssertTrue(app.buttons["fei.more"].waitForExistence(timeout: 10),app.debugDescription)
    }

    func testNativeFilePickerPreviewAndReadOnlyAttachments() {
        continueAfterFailure = false
        let stamp = Int(Date().timeIntervalSince1970)
        print("MEDIA_TOUCH_STAMP=\(stamp)")
        let app = XCUIApplication()
        app.launchEnvironment["FEI_NOTES_MODE"] = "local"
        app.launch()
        XCTAssertTrue(app.buttons["fei.settings"].waitForExistence(timeout: 15))
        app.buttons["fei.new"].tap()
        XCTAssertTrue(app.buttons["fei.more"].waitForExistence(timeout: 5))
        app.coordinate(withNormalizedOffset: CGVector(dx: 0.3,dy: 0.28)).tap()
        app.typeText("FEI_MEDIA_NOTE_\(stamp)")
        func insert(_ key: String) {
            app.buttons["fei.more"].tap()
            app.sheets.buttons["插入内容"].tap()
            let menu = app.tables["fei.insert.controls"]
            XCTAssertTrue(menu.waitForExistence(timeout: 5),app.debugDescription)
            menu.cells["fei.insert.\(key)"].tap()
        }
        func attachments() -> XCUIElement {
            app.buttons["fei.more"].tap()
            app.sheets.buttons["图片和附件"].tap()
            let menu = app.tables["fei.attachments.controls"]
            XCTAssertTrue(menu.waitForExistence(timeout: 5),app.debugDescription)
            return menu
        }
        insert("image")
        choose("FEI_MEDIA_FIXTURE11_image",in: app)
        RunLoop.current.run(until: Date().addingTimeInterval(0.4))
        shot("media-01-image-in-shared-editor")
        let imageMenu = attachments()
        XCTAssertEqual(imageMenu.cells.count,1)
        imageMenu.cells.element(boundBy: 0).tap()
        XCTAssertTrue(app.buttons["fei.file.done"].waitForExistence(timeout: 10),app.debugDescription)
        shot("media-02-native-image-preview")
        app.buttons["fei.file.share"].tap()
        let share = app.otherElements["ActivityListView"]
        XCTAssertTrue(share.waitForExistence(timeout: 5),app.debugDescription)
        shot("media-03-native-share-sheet")
        app.coordinate(withNormalizedOffset: CGVector(dx: 0.5,dy: 0.8)).tap()
        gone(share)
        app.buttons["fei.file.done"].tap()
        XCTAssertTrue(app.buttons["fei.more"].waitForExistence(timeout: 5))
        insert("file")
        choose("FEI_MEDIA_FIXTURE11_attachment",in: app)
        let menu = attachments()
        XCTAssertEqual(menu.cells.count,2)
        shot("media-04-note-attachments")
        let file = menu.cells.containing(.staticText,identifier: "FEI_MEDIA_FIXTURE11_attachment.txt").firstMatch
        XCTAssertTrue(file.exists,app.debugDescription)
        file.tap()
        XCTAssertTrue(app.buttons["fei.file.done"].waitForExistence(timeout: 10),app.debugDescription)
        let previewText = app.descendants(matching: .any).matching(NSPredicate(format: "label CONTAINS %@","FEI_MEDIA_FILE11_SYNTHETIC")).firstMatch
        XCTAssertTrue(previewText.waitForExistence(timeout: 10),app.debugDescription)
        shot("media-05-native-file-preview")
        app.buttons["fei.file.done"].tap()
        app.buttons["fei.more"].tap()
        app.sheets.buttons["笔记选项"].tap()
        app.sheets.buttons["阅读模式"].tap()
        app.buttons["fei.more"].tap()
        XCTAssertTrue(app.sheets.buttons["图片和附件"].waitForExistence(timeout: 5))
        XCTAssertFalse(app.sheets.buttons["插入内容"].exists)
        app.sheets.buttons["图片和附件"].tap()
        XCTAssertTrue(menu.waitForExistence(timeout: 5))
        XCTAssertEqual(menu.cells.count,2)
        XCTAssertEqual(app.keyboards.count,0)
        shot("media-06-read-only-attachments")
        file.tap()
        XCTAssertTrue(app.buttons["fei.file.done"].waitForExistence(timeout: 10))
        app.buttons["fei.file.done"].tap()
        app.navigationBars.buttons.element(boundBy: 0).tap()
        XCTAssertTrue(app.buttons["fei.settings"].waitForExistence(timeout: 5))
        print("MEDIA_TOUCH_COMPLETED=\(stamp)")
    }
}
