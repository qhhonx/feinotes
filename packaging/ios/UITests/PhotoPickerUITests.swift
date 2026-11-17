import XCTest

final class PhotoPickerUITests: XCTestCase {
    private func shot(_ name: String) {
        let attachment = XCTAttachment(screenshot: XCUIScreen.main.screenshot())
        attachment.name = name; attachment.lifetime = .keepAlways; add(attachment)
    }
    func testNativePhotoPickerCancellation() {
        continueAfterFailure = false
        let stamp = Int(Date().timeIntervalSince1970)
        print("PHOTOS_CANCEL_STAMP=\(stamp)")
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
        insert.cells["fei.insert.photos"].tap()
        XCTAssertTrue(app.buttons["取消"].waitForExistence(timeout: 10),app.debugDescription)
        shot("photos-01-native-library")
        print("PHOTOS_PICKER_TREE=\(app.debugDescription)")
        app.buttons["取消"].tap()
        XCTAssertTrue(app.buttons["fei.more"].waitForExistence(timeout: 5))
        app.buttons["fei.more"].tap()
        app.sheets.buttons["重命名"].tap()
        let name = app.textFields["fei.rename.title"]
        XCTAssertTrue(name.waitForExistence(timeout: 5))
        name.typeText("FEI_PHOTOS_CANCEL_\(stamp)")
        app.buttons["fei.rename.save"].tap()
        app.navigationBars.buttons["返回"].firstMatch.tap()
        XCTAssertTrue(app.buttons["fei.settings"].waitForExistence(timeout: 5))
        print("PHOTOS_CANCEL_COMPLETED=\(stamp)")
    }
    func testSelectedPNGAndHEICPhotos() {
        continueAfterFailure = false
        let stamp = Int(Date().timeIntervalSince1970)
        print("PHOTOS_IMPORT_STAMP=\(stamp)")
        let app = XCUIApplication()
        app.launchEnvironment["FEI_NOTES_MODE"] = "local"
        XCUIDevice.shared.orientation = .portrait
        app.launch()
        XCTAssertTrue(app.buttons["fei.settings"].waitForExistence(timeout: 15))
        for (format,index) in [("PNG",1),("HEIC",0)] {
            app.buttons["fei.new"].tap()
            XCTAssertTrue(app.buttons["fei.more"].waitForExistence(timeout: 5))
            app.coordinate(withNormalizedOffset: CGVector(dx: 0.3,dy: 0.28)).tap()
            app.buttons["fei.more"].tap()
            app.sheets.buttons["插入内容"].tap()
            let insert = app.tables["fei.insert.controls"]
            XCTAssertTrue(insert.waitForExistence(timeout: 5))
            insert.cells["fei.insert.photos"].tap()
            XCTAssertTrue(app.buttons["取消"].waitForExistence(timeout: 10),app.debugDescription)
            let photo = app.images.matching(identifier: "PXGGridLayout-Info").element(boundBy: index)
            XCTAssertTrue(photo.waitForExistence(timeout: 5),app.debugDescription)
            let day = DateFormatter(); day.locale = Locale(identifier: "zh_CN"); day.dateFormat = "M月dd日"
            XCTAssertTrue(photo.label.contains(day.string(from: Date())),photo.label)
            print("PHOTOS_SELECTED_\(format)=\(photo.label)")
            photo.tap()
            XCTAssertTrue(app.buttons["fei.more"].waitForExistence(timeout: 15),app.debugDescription)
            app.buttons["fei.more"].tap()
            app.sheets.buttons["图片和附件"].tap()
            let attachments = app.tables["fei.attachments.controls"]
            XCTAssertTrue(attachments.waitForExistence(timeout: 5))
            XCTAssertEqual(attachments.cells.count,1)
            XCTAssertFalse(attachments.cells.element(boundBy: 0).staticTexts["暂无图片或附件"].exists)
            attachments.cells.element(boundBy: 0).tap()
            XCTAssertTrue(app.buttons["fei.file.done"].waitForExistence(timeout: 10),app.debugDescription)
            shot("photos-\(format)-native-preview")
            app.buttons["fei.file.done"].tap()
            app.buttons["fei.more"].tap()
            app.sheets.buttons["重命名"].tap()
            let title = app.textFields["fei.rename.title"]
            XCTAssertTrue(title.waitForExistence(timeout: 5))
            title.typeText("FEI_PHOTOS_\(format)_\(stamp)")
            app.buttons["fei.rename.save"].tap()
            shot("photos-\(format)-shared-editor")
            app.navigationBars.buttons["返回"].firstMatch.tap()
            XCTAssertTrue(app.buttons["fei.settings"].waitForExistence(timeout: 5))
        }
        print("PHOTOS_IMPORT_COMPLETED=\(stamp)")
    }

}
