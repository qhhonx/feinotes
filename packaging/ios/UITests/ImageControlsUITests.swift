import XCTest

final class ImageControlsUITests: XCTestCase {
    private func shot(_ name: String) {
        let attachment = XCTAttachment(screenshot: XCUIScreen.main.screenshot())
        attachment.name = name; attachment.lifetime = .keepAlways; add(attachment)
    }
    func testNativeImageSettingsAndCrop() {
        continueAfterFailure = false
        let stamp = Int(Date().timeIntervalSince1970)
        print("IMAGE_TOUCH_STAMP=\(stamp)")
        let app = XCUIApplication()
        app.launchEnvironment["FEI_NOTES_MODE"] = "local"
        XCUIDevice.shared.orientation = .portrait
        app.launch()
        XCTAssertTrue(app.buttons["fei.settings"].waitForExistence(timeout: 15))
        app.buttons["fei.new"].tap()
        XCTAssertTrue(app.buttons["fei.more"].waitForExistence(timeout: 5))
        app.coordinate(withNormalizedOffset: CGVector(dx: 0.3,dy: 0.28)).tap()
        app.buttons["fei.more"].tap()
        app.sheets.buttons["插入内容"].tap()
        let insert = app.tables["fei.insert.controls"]
        XCTAssertTrue(insert.waitForExistence(timeout: 5))
        insert.cells["fei.insert.image"].tap()
        let file = app.cells.matching(NSPredicate(format: "label CONTAINS %@","FEI_MEDIA_FIXTURE11_image")).firstMatch
        if !file.waitForExistence(timeout: 2) {
            let browse = app.buttons["浏览"].firstMatch
            if browse.waitForExistence(timeout: 2) { browse.tap() }
            if !file.waitForExistence(timeout: 2) {
                let folder = app.cells.matching(NSPredicate(format: "label BEGINSWITH %@","飞记")).firstMatch
                if !folder.waitForExistence(timeout: 2) {
                    let phone = app.cells.matching(NSPredicate(format: "label CONTAINS %@","iPhone")).firstMatch
                    XCTAssertTrue(phone.waitForExistence(timeout: 5),app.debugDescription); phone.tap()
                }
                XCTAssertTrue(folder.waitForExistence(timeout: 5),app.debugDescription); folder.tap()
            }
        }
        XCTAssertTrue(file.waitForExistence(timeout: 5),app.debugDescription)
        file.tap()
        let pickerOpen = app.buttons["打开"].firstMatch
        if pickerOpen.waitForExistence(timeout: 2) { pickerOpen.tap() }
        XCTAssertTrue(app.buttons["fei.more"].waitForExistence(timeout: 10))
        func open() {
            app.buttons["fei.more"].tap()
            app.sheets.buttons["内容设置"].tap()
            let list = app.tables["fei.objects.controls"]
            XCTAssertTrue(list.waitForExistence(timeout: 5))
            XCTAssertEqual(list.cells.count,1)
            list.cells.element(boundBy: 0).tap()
            XCTAssertTrue(app.tables["fei.object.form"].waitForExistence(timeout: 5))
        }
        func close(_ cancel: Bool = false) {
            app.buttons[cancel ? "fei.object.cancel" : "fei.object.save"].tap()
            XCTAssertTrue(XCTWaiter.wait(for: [XCTNSPredicateExpectation(predicate: NSPredicate(format: "exists == false"),object: app.tables["fei.object.form"])],timeout: 5) == .completed)
        }
        func crop() {
            app.cells["fei.object.row.crop"].tap()
            XCTAssertTrue(app.otherElements["fei.image.crop.-1.-1"].waitForExistence(timeout: 5),app.debugDescription)
        }
        func drag() {
            let handle = app.otherElements["fei.image.crop.-1.-1"]
            XCTAssertTrue(handle.waitForExistence(timeout: 5),app.debugDescription)
            let start = handle.coordinate(withNormalizedOffset: CGVector(dx: 0.5,dy: 0.5))
            let end = start.withOffset(CGVector(dx: 38,dy: 26))
            start.press(forDuration: 0.05,thenDragTo: end)
        }
        open()
        let caption = app.textFields["fei.object.caption"]
        caption.tap(); caption.typeText("Native image \(stamp)")
        app.cells["fei.object.row.alignment"].tap()
        let options = app.tables["fei.object.options"]
        XCTAssertTrue(options.waitForExistence(timeout: 5))
        options.cells["fei.object.option.right"].tap()
        let width = app.sliders["fei.object.width"]
        XCTAssertTrue(width.waitForExistence(timeout: 5))
        width.adjust(toNormalizedSliderPosition: 0.62)
        shot("image-01-native-settings")
        close()
        open()
        XCTAssertEqual(caption.value as? String,"Native image \(stamp)")
        XCTAssertTrue(app.cells["fei.object.row.alignment"].staticTexts["右对齐"].exists)
        print("IMAGE_WIDTH_BEFORE_CROP=\(width.value ?? "")")
        crop(); drag()
        shot("image-02-crop-draft")
        XCUIDevice.shared.orientation = .landscapeLeft
        let handle = app.otherElements["fei.image.crop.-1.-1"]
        XCTAssertTrue(handle.waitForExistence(timeout: 5))
        XCTAssertTrue(app.windows.firstMatch.frame.contains(handle.frame))
        shot("image-03-landscape-crop")
        XCUIDevice.shared.orientation = .portrait
        close(true)
        open()
        XCTAssertFalse(app.cells["fei.object.row.reset"].exists)
        crop(); drag(); close()
        open()
        XCTAssertTrue(app.cells["fei.object.row.reset"].exists)
        app.cells["fei.object.row.reset"].tap()
        XCTAssertTrue(XCTWaiter.wait(for: [XCTNSPredicateExpectation(predicate: NSPredicate(format: "exists == false"),object: app.cells["fei.object.row.reset"])],timeout: 5) == .completed)
        close()
        app.buttons["fei.more"].tap()
        app.sheets.buttons["编辑和格式"].tap()
        app.sheets.buttons["撤销"].tap()
        open()
        XCTAssertTrue(app.cells["fei.object.row.reset"].waitForExistence(timeout: 5))
        crop()
        shot("image-04-shared-crop-after-undo")
        close(true)
        app.buttons["fei.more"].tap()
        app.sheets.buttons["重命名"].tap()
        let title = app.textFields["fei.rename.title"]
        XCTAssertTrue(title.waitForExistence(timeout: 5))
        title.typeText("FEI_IMAGE_CONTROLS_\(stamp)")
        app.buttons["fei.rename.save"].tap()
        app.navigationBars.buttons["返回"].firstMatch.tap()
        XCTAssertTrue(app.buttons["fei.settings"].waitForExistence(timeout: 5))
        print("IMAGE_TOUCH_COMPLETED=\(stamp)")
    }
}
