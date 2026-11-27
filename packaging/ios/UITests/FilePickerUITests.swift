import XCTest

final class FilePickerUITests: XCTestCase {
    private func shot(_ name:String) {
        let a = XCTAttachment(screenshot: XCUIScreen.main.screenshot()); a.name = name; a.lifetime = .keepAlways; add(a)
    }
    func testImportPickerCanCancelAndReopen() {
        continueAfterFailure = false
        let app = XCUIApplication(); app.launchEnvironment["FEI_NOTES_MODE"] = "local"
        XCUIDevice.shared.orientation = .portrait; app.launch()
        XCTAssertTrue(app.buttons["fei.library-actions"].waitForExistence(timeout: 15)); app.buttons["fei.library-actions"].tap()
        app.sheets.buttons["导入文档"].tap()
        XCTAssertTrue(app.navigationBars["FullDocumentManagerViewControllerNavigationBar"].waitForExistence(timeout: 5),app.debugDescription)
        shot("picker37-01-import-presentation")
        let cancel = app.buttons.matching(NSPredicate(format: "label IN %@",["取消","Cancel"])).firstMatch
        XCTAssertTrue(cancel.waitForExistence(timeout: 2),app.debugDescription)
        cancel.tap()
        let picker = app.navigationBars["FullDocumentManagerViewControllerNavigationBar"]
        XCTAssertEqual(XCTWaiter.wait(for: [XCTNSPredicateExpectation(predicate: NSPredicate(format: "exists == false"),object: picker)],timeout: 5),.completed)
        XCTAssertTrue(app.buttons["fei.library-actions"].waitForExistence(timeout: 5)); app.buttons["fei.library-actions"].tap()
        app.sheets.buttons["导入文档"].tap()
        XCTAssertTrue(picker.waitForExistence(timeout: 5)); XCTAssertTrue(cancel.waitForExistence(timeout: 5)); cancel.tap()
        XCTAssertEqual(XCTWaiter.wait(for: [XCTNSPredicateExpectation(predicate: NSPredicate(format: "exists == false"),object: picker)],timeout: 5),.completed)
        app.buttons["fei.library-actions"].tap(); app.sheets.buttons["从备份恢复"].tap()
        XCTAssertTrue(picker.waitForExistence(timeout: 5)); shot("picker37-02-backup-presentation")
        if cancel.waitForExistence(timeout: 2) { cancel.tap() }
        else { app.coordinate(withNormalizedOffset: CGVector(dx: 0.5,dy: 0.075)).press(forDuration: 0.1,thenDragTo: app.coordinate(withNormalizedOffset: CGVector(dx: 0.5,dy: 0.65))) }
        XCTAssertEqual(XCTWaiter.wait(for: [XCTNSPredicateExpectation(predicate: NSPredicate(format: "exists == false"),object: picker)],timeout: 5),.completed)
        app.buttons["fei.library-actions"].tap(); app.sheets.buttons["从备份恢复"].tap()
        XCTAssertTrue(picker.waitForExistence(timeout: 5))
        if cancel.waitForExistence(timeout: 2) { cancel.tap() }
        else { app.coordinate(withNormalizedOffset: CGVector(dx: 0.5,dy: 0.075)).press(forDuration: 0.1,thenDragTo: app.coordinate(withNormalizedOffset: CGVector(dx: 0.5,dy: 0.65))) }
        XCTAssertEqual(XCTWaiter.wait(for: [XCTNSPredicateExpectation(predicate: NSPredicate(format: "exists == false"),object: picker)],timeout: 5),.completed)
        XCTAssertTrue(app.tables["fei.notes.list"].waitForExistence(timeout: 5)); shot("picker37-03-returned-after-cancels")
    }
    func testSingleImagePickerCancelThenSelectAndExportDismiss() {
        continueAfterFailure = false
        let app = XCUIApplication(); app.launchEnvironment["FEI_NOTES_MODE"] = "local"
        XCUIDevice.shared.orientation = .portrait; app.launch()
        XCTAssertTrue(app.buttons["fei.new"].waitForExistence(timeout: 15)); app.buttons["fei.new"].tap()
        XCTAssertTrue(app.buttons["fei.edit.insert"].waitForExistence(timeout: 5))
        func imagePicker() {
            app.buttons["fei.edit.insert"].tap()
            let menu = app.tables["fei.insert.controls"]
            XCTAssertTrue(menu.waitForExistence(timeout: 5)); menu.cells["fei.insert.image"].tap()
        }
        let picker = app.navigationBars["FullDocumentManagerViewControllerNavigationBar"]
        imagePicker(); XCTAssertTrue(picker.waitForExistence(timeout: 5)); shot("picker37-04-single-image-sheet")
        app.coordinate(withNormalizedOffset: CGVector(dx: 0.5,dy: 0.075)).press(forDuration: 0.1,thenDragTo: app.coordinate(withNormalizedOffset: CGVector(dx: 0.5,dy: 0.65)))
        XCTAssertEqual(XCTWaiter.wait(for: [XCTNSPredicateExpectation(predicate: NSPredicate(format: "exists == false"),object: picker)],timeout: 5),.completed)
        imagePicker(); XCTAssertTrue(picker.waitForExistence(timeout: 5))
        let file = app.cells.matching(NSPredicate(format: "label BEGINSWITH %@","PICKER37_image")).firstMatch
        if !file.waitForExistence(timeout: 2) {
            let back = app.buttons["DOC.navBarButton.backInHistory"]
            let folders = app.cells.matching(NSPredicate(format: "label BEGINSWITH %@","飞记"))
            let local = app.cells["DOC.sidebar.item.我的iPhone"]
            for _ in 0..<5 {
                if file.exists { break }
                if local.waitForExistence(timeout: 1) {
                    local.tap(); XCTAssertTrue(folders.firstMatch.waitForExistence(timeout: 5)); continue
                }
                if folders.count > 0 {
                    let total = folders.count
                    for index in 0..<total {
                        folders.element(boundBy: index).tap()
                        if file.waitForExistence(timeout: 2) { break }
                        back.tap(); XCTAssertTrue(folders.firstMatch.waitForExistence(timeout: 5))
                    }
                    break
                }
                guard back.waitForExistence(timeout: 1) else { break }
                back.tap()
                let known = XCTNSPredicateExpectation(predicate: NSPredicate { _,_ in file.exists || local.exists || folders.count > 0 },object: nil)
                _ = XCTWaiter.wait(for: [known],timeout: 3)
            }
        }
        XCTAssertTrue(file.waitForExistence(timeout: 5),app.debugDescription); file.tap()
        let open = app.buttons["DOCPicker.actionButton"]
        if open.waitForExistence(timeout: 1) { open.tap() }
        XCTAssertEqual(XCTWaiter.wait(for: [XCTNSPredicateExpectation(predicate: NSPredicate(format: "exists == false"),object: picker)],timeout: 10),.completed)
        let title = "P37_"+String(Int(Date().timeIntervalSince1970))
        app.buttons["fei.more"].tap(); app.sheets.buttons["重命名"].tap()
        let name = app.textFields["fei.rename.title"]
        XCTAssertTrue(name.waitForExistence(timeout: 5)); name.typeText(title); app.buttons["fei.rename.save"].tap()
        XCTAssertTrue(app.navigationBars[title].waitForExistence(timeout: 5)); shot("picker37-05-selected-image")
        func export() {
            app.buttons["fei.more"].tap()
            app.buttons["导出文档"].tap()
            app.buttons["导出为 Markdown"].tap()
            app.buttons["仅文本"].tap()
            app.openDocumentExportInFiles()
        }
        for attempt in 0..<2 {
            export()
            let save = app.buttons["DOCPicker.actionButton"]
            XCTAssertTrue(save.waitForExistence(timeout: 10),app.debugDescription)
            if attempt == 0 { shot("picker37-06-export-sheet") }
            let cancel = app.buttons["DOCPicker.cancelButton"]
            if cancel.exists && cancel.isHittable { cancel.tap() }
            else {
                app.coordinate(withNormalizedOffset: CGVector(dx: 0.5,dy: 0.075)).press(forDuration: 0.1,thenDragTo: app.coordinate(withNormalizedOffset: CGVector(dx: 0.5,dy: 0.65)))
            }
            XCTAssertEqual(XCTWaiter.wait(for: [XCTNSPredicateExpectation(predicate: NSPredicate(format: "exists == false"),object: save)],timeout: 10),.completed)
            if app.documentExportSaveAction.isHittable || app.documentExportMoreActions.isHittable {
                let y = UIDevice.current.userInterfaceIdiom == .phone ? 0.20 : 0.52
                app.coordinate(withNormalizedOffset: CGVector(dx: 0.10,dy: y)).tap()
            }
            XCTAssertTrue(app.buttons["fei.more"].isHittable,app.debugDescription)
        }
        print("PICKER37_COMPLETED="+title)
    }
}
