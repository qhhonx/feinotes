import XCTest

final class EditorProbeTests: XCTestCase {
    private func shot(_ name: String) {
        let attachment = XCTAttachment(screenshot: XCUIScreen.main.screenshot())
        attachment.name = name
        attachment.lifetime = .keepAlways
        add(attachment)
    }

    private func gone(_ element: XCUIElement) {
        XCTAssertTrue(NSPredicate(format: "exists == false").evaluate(with: element) ||
            XCTWaiter.wait(for: [XCTNSPredicateExpectation(predicate: NSPredicate(format: "exists == false"), object: element)], timeout: 5) == .completed)
    }

    func testNativeNavigationAndSharedEditor() {
        let stamp = Int(Date().timeIntervalSince1970)
        let titleText = "FEI-ACCEPT-\(stamp)"
        let folderText = "FEI-FOLDER-\(stamp)"
        let app = XCUIApplication()
        app.launchEnvironment["FEI_NOTES_MVP_MODE"] = "local"
        continueAfterFailure = false
        app.launch()
        XCTAssertTrue(app.buttons["fei.settings"].waitForExistence(timeout: 15))
        XCTAssertEqual(app.navigationBars.count, 1)
        shot("01-native-notes")
        app.buttons["fei.library-actions"].tap()
        app.sheets.buttons["新建文件夹"].tap()
        XCTAssertTrue(app.textFields["fei.rename.title"].waitForExistence(timeout: 3))
        app.textFields["fei.rename.title"].typeText(folderText)
        app.buttons["fei.rename.save"].tap()
        gone(app.textFields["fei.rename.title"])
        app.buttons["fei.new"].tap()
        XCTAssertTrue(app.buttons["fei.more"].waitForExistence(timeout: 5))
        app.coordinate(withNormalizedOffset: CGVector(dx: 0.3, dy: 0.28)).tap()
        app.typeText("FEI_NATIVE_ACCEPT4 ")
        app.buttons["fei.more"].tap()
        XCTAssertTrue(app.sheets.firstMatch.waitForExistence(timeout: 5))
        shot("02-native-actions")
        app.sheets.buttons["重命名"].tap()
        XCTAssertTrue(app.textFields["fei.rename.title"].waitForExistence(timeout: 3))
        let title = app.textFields["fei.rename.title"]
        title.tap()
        let length = (title.value as? String)?.count ?? 20
        title.typeText(String(repeating: XCUIKeyboardKey.delete.rawValue, count: length * 2))
        title.typeText(titleText)
        XCTAssertEqual(title.value as? String, titleText)
        app.buttons["fei.rename.save"].tap()
        gone(app.textFields["fei.rename.title"])
        XCTAssertTrue(app.navigationBars[titleText].waitForExistence(timeout: 5))
        shot("03-native-editor")
        app.buttons["fei.more"].tap()
        app.sheets.buttons["笔记选项"].tap()
        XCTAssertTrue(app.sheets.buttons["移动到"].waitForExistence(timeout: 5))
        app.sheets.buttons["移动到"].tap()
        XCTAssertTrue(app.cells.staticTexts[folderText].waitForExistence(timeout: 5))
        app.cells.staticTexts[folderText].tap()
        XCTAssertTrue(app.buttons["fei.more"].waitForExistence(timeout: 5))
        app.buttons["fei.more"].tap()
        app.sheets.buttons["导出 Markdown"].tap()
        XCTAssertTrue(app.sheets.buttons["仅文本"].waitForExistence(timeout: 5))
        app.sheets.buttons["仅文本"].tap()
        XCTAssertTrue(app.buttons["DOCPicker.actionButton"].waitForExistence(timeout: 10))
        shot("04-native-export")
        app.buttons["DOCPicker.actionButton"].tap()
        XCTAssertTrue(app.buttons["fei.more"].waitForExistence(timeout: 10))
        app.navigationBars.buttons.element(boundBy: 0).tap()
        XCTAssertTrue(app.buttons["fei.settings"].waitForExistence(timeout: 5))
        app.buttons["fei.library-actions"].tap()
        app.sheets.buttons["导出文档库备份"].tap()
        XCTAssertTrue(app.buttons["DOCPicker.actionButton"].waitForExistence(timeout: 10))
        shot("05-native-backup")
        app.buttons["DOCPicker.actionButton"].tap()
        XCTAssertTrue(app.buttons["fei.settings"].waitForExistence(timeout: 10))
        app.buttons["fei.settings"].tap()
        XCTAssertTrue(app.buttons["fei.done-settings"].waitForExistence(timeout: 5))
        shot("05-native-settings")
        app.coordinate(withNormalizedOffset: CGVector(dx: 0.4, dy: 0.41)).tap()
        XCTAssertTrue(app.navigationBars["iCloud 同步"].waitForExistence(timeout: 5))
        shot("06-native-cloud")
        app.buttons["fei.done-settings"].tap()
        XCTAssertTrue(app.buttons["fei.settings"].waitForExistence(timeout: 5))
        shot("07-native-back")
        XCUIDevice.shared.orientation = .landscapeLeft
        XCTAssertTrue(app.buttons["fei.settings"].waitForExistence(timeout: 5))
        shot("08-native-landscape")
        XCUIDevice.shared.orientation = .portrait
    }

    func testBackupRestoreInsideExistingWindow() {
        continueAfterFailure = false
        let stamp = Int(Date().timeIntervalSince1970)
        let app = XCUIApplication()
        app.launchEnvironment["FEI_NOTES_MODE"] = "local"
        app.launch()
        XCTAssertTrue(app.buttons["fei.settings"].waitForExistence(timeout: 15))
        func newNote(_ text: String) {
            app.buttons["fei.new"].tap()
            XCTAssertTrue(app.buttons["fei.more"].waitForExistence(timeout: 5))
            app.coordinate(withNormalizedOffset: CGVector(dx: 0.3, dy: 0.28)).tap()
            app.typeText(text + " ")
            app.navigationBars.buttons.element(boundBy: 0).tap()
            XCTAssertTrue(app.buttons["fei.settings"].waitForExistence(timeout: 5))
        }
        newNote("FEI_RESTORE_KEEP_\(stamp)")
        app.buttons["fei.library-actions"].tap()
        app.sheets.buttons["导出文档库备份"].tap()
        XCTAssertTrue(app.buttons["DOCPicker.actionButton"].waitForExistence(timeout: 10))
        let name = app.textFields.firstMatch
        XCTAssertTrue(name.waitForExistence(timeout: 5), app.debugDescription)
        let filename = name.value as? String ?? ""
        XCTAssertTrue(filename.contains("Fei Notes"), app.debugDescription)
        let prefix = filename.replacingOccurrences(of: ".feinotesbackup", with: "")
        print("BACKUP_FIXTURE_NAME=\(filename)")
        app.buttons["DOCPicker.actionButton"].tap()
        XCTAssertTrue(app.buttons["fei.settings"].waitForExistence(timeout: 10))
        newNote("FEI_RESTORE_REMOVE_\(stamp)")
        app.buttons["fei.library-actions"].tap()
        app.sheets.buttons["从备份恢复"].tap()
        let file = app.descendants(matching: .any).matching(NSPredicate(format: "label CONTAINS %@", prefix)).firstMatch
        XCTAssertTrue(file.waitForExistence(timeout: 10), app.debugDescription)
        shot("restore-01-file-picker")
        file.tap()
        XCTAssertTrue(app.alerts.firstMatch.waitForExistence(timeout: 10), app.debugDescription)
        shot("restore-02-verified-confirmation")
        app.alerts.buttons["恢复"].firstMatch.tap()
        gone(app.alerts.firstMatch)
        XCTAssertTrue(app.buttons["fei.settings"].waitForExistence(timeout: 15), app.debugDescription)
        XCTAssertEqual(app.state, .runningForeground)
        shot("restore-03-reopened-library")
        newNote("FEI_AFTER_RESTORE_\(stamp)")
        print("RESTORE_STAMP=\(stamp)")
        shot("restore-04-editor-still-working")
    }

    func testTouchSelectionKeepsRichCopyAndUndo() {
        continueAfterFailure = false
        let stamp = Int(Date().timeIntervalSince1970)
        print("TOUCH_COPY_STAMP=\(stamp)")
        let app = XCUIApplication()
        app.launchEnvironment["FEI_NOTES_MODE"] = "local"
        app.launch()
        XCTAssertTrue(app.buttons["fei.settings"].waitForExistence(timeout: 15))
        app.buttons["fei.new"].tap()
        XCTAssertTrue(app.buttons["fei.more"].waitForExistence(timeout: 5))
        app.coordinate(withNormalizedOffset: CGVector(dx: 0.3,dy: 0.28)).tap()
        app.typeText("Alpha **Bravo** Charlie \(stamp)")
        app.buttons["fei.more"].tap()
        app.coordinate(withNormalizedOffset: CGVector(dx: 0.05,dy: 0.8)).tap()
        shot("touch-01-rich-source")
        app.coordinate(withNormalizedOffset: CGVector(dx: 0.24,dy: 0.28)).press(forDuration: 0.8)
        let copy = app.menuItems["复制"].firstMatch
        XCTAssertTrue(copy.waitForExistence(timeout: 5),app.debugDescription)
        shot("touch-02-native-edit-menu")
        copy.tap()
        app.navigationBars.buttons.element(boundBy: 0).tap()
        XCTAssertTrue(app.buttons["fei.settings"].waitForExistence(timeout: 5))
        app.buttons["fei.new"].tap()
        XCTAssertTrue(app.buttons["fei.more"].waitForExistence(timeout: 5))
        app.coordinate(withNormalizedOffset: CGVector(dx: 0.3,dy: 0.28)).press(forDuration: 0.8)
        let paste = app.menuItems["粘贴"].firstMatch
        XCTAssertTrue(paste.waitForExistence(timeout: 5),app.debugDescription)
        paste.tap()
        shot("touch-03-rich-paste")
        app.buttons["fei.more"].tap()
        app.sheets.buttons["编辑和格式"].tap()
        app.sheets.buttons["撤销"].tap()
        RunLoop.current.run(until: Date().addingTimeInterval(0.4))
        shot("touch-04-paste-undo")
        app.buttons["fei.more"].tap()
        app.sheets.buttons["编辑和格式"].tap()
        app.sheets.buttons["重做"].tap()
        RunLoop.current.run(until: Date().addingTimeInterval(0.4))
        shot("touch-05-paste-redo")
        app.navigationBars.buttons.element(boundBy: 0).tap()
        XCTAssertTrue(app.buttons["fei.settings"].waitForExistence(timeout: 5))
    }

    func testTouchHandlesAndReadOnlySelectAll() {
        continueAfterFailure = false
        let stamp = Int(Date().timeIntervalSince1970)
        print("TOUCH_HANDLE_STAMP=\(stamp)")
        let app = XCUIApplication()
        app.launchEnvironment["FEI_NOTES_MODE"] = "local"
        app.launch()
        XCTAssertTrue(app.buttons["fei.settings"].waitForExistence(timeout: 15))
        app.buttons["fei.new"].tap()
        XCTAssertTrue(app.buttons["fei.more"].waitForExistence(timeout: 5))
        app.coordinate(withNormalizedOffset: CGVector(dx: 0.3,dy: 0.28)).tap()
        app.typeText("Alpha Bravo Charlie \(stamp)")
        app.buttons["fei.more"].tap()
        app.coordinate(withNormalizedOffset: CGVector(dx: 0.05,dy: 0.8)).tap()
        app.coordinate(withNormalizedOffset: CGVector(dx: 0.24,dy: 0.28)).press(forDuration: 0.8)
        XCTAssertTrue(app.menuItems["复制"].waitForExistence(timeout: 5))
        let handle = app.coordinate(withNormalizedOffset: CGVector(dx: 0.275,dy: 0.312))
        handle.press(forDuration: 0.05,thenDragTo: app.coordinate(withNormalizedOffset: CGVector(dx: 0.42,dy: 0.312)))
        XCTAssertTrue(app.menuItems["复制"].waitForExistence(timeout: 5))
        shot("touch-handle-01-expanded-selection")
        app.menuItems["复制"].tap()
        app.navigationBars.buttons.element(boundBy: 0).tap()
        XCTAssertTrue(app.buttons["fei.settings"].waitForExistence(timeout: 5))
        app.buttons["fei.new"].tap()
        XCTAssertTrue(app.buttons["fei.more"].waitForExistence(timeout: 5))
        app.coordinate(withNormalizedOffset: CGVector(dx: 0.3,dy: 0.28)).press(forDuration: 0.8)
        XCTAssertTrue(app.menuItems["粘贴"].waitForExistence(timeout: 5))
        app.menuItems["粘贴"].tap()
        app.buttons["fei.more"].tap()
        app.sheets.buttons["笔记选项"].tap()
        app.sheets.buttons["阅读模式"].tap()
        app.coordinate(withNormalizedOffset: CGVector(dx: 0.1,dy: 0.28)).press(forDuration: 0.8)
        XCTAssertTrue(app.menuItems["复制"].waitForExistence(timeout: 5))
        XCTAssertFalse(app.menuItems["剪切"].exists)
        XCTAssertFalse(app.menuItems["粘贴"].exists)
        XCTAssertEqual(app.keyboards.count,0)
        app.menuItems["复制"].tap()
        app.coordinate(withNormalizedOffset: CGVector(dx: 0.1,dy: 0.28)).press(forDuration: 0.8)
        XCTAssertTrue(app.menuItems["全选"].waitForExistence(timeout: 5))
        app.menuItems["全选"].tap()
        XCTAssertTrue(app.menuItems["复制"].waitForExistence(timeout: 5))
        shot("touch-handle-02-readonly-select-all")
        app.menuItems["复制"].tap()
        app.navigationBars.buttons.element(boundBy: 0).tap()
        XCTAssertTrue(app.buttons["fei.settings"].waitForExistence(timeout: 5))
        app.buttons["fei.new"].tap()
        XCTAssertTrue(app.buttons["fei.more"].waitForExistence(timeout: 5))
        app.coordinate(withNormalizedOffset: CGVector(dx: 0.3,dy: 0.28)).press(forDuration: 0.8)
        XCTAssertTrue(app.menuItems["粘贴"].waitForExistence(timeout: 5))
        app.menuItems["粘贴"].tap()
        shot("touch-handle-03-readonly-copy-pasted")
        app.navigationBars.buttons.element(boundBy: 0).tap()
        XCTAssertTrue(app.buttons["fei.settings"].waitForExistence(timeout: 5))
    }

    func testSharedSessionSelectionRoundTrip() {
        continueAfterFailure = false
        let app = XCUIApplication()
        app.launchEnvironment["FEI_NOTES_MODE"] = "cloud"
        app.launchEnvironment["FEI_NOTES_CLOUD_CHANNEL"] = "44ecb219-6dec-4b51-ae62-bcd575b86813"
        app.launch()
        XCTAssertTrue(app.buttons["fei.settings"].waitForExistence(timeout: 15),app.debugDescription)
        app.buttons["fei.library-actions"].tap()
        XCTAssertTrue(app.sheets.buttons["共享笔记"].waitForExistence(timeout: 5))
        app.sheets.buttons["共享笔记"].tap()
        XCTAssertTrue(app.navigationBars["共享笔记"].waitForExistence(timeout: 5))
        shot("sharing-01-personal-shares")
        app.coordinate(withNormalizedOffset: CGVector(dx: 0.84,dy: 0.295)).tap()
        XCTAssertTrue(app.navigationBars["FEI_SHARED_READONLY"].waitForExistence(timeout: 15),app.debugDescription)
        app.coordinate(withNormalizedOffset: CGVector(dx: 0.3,dy: 0.36)).tap()
        XCTAssertEqual(app.keyboards.count,0)
        app.buttons["fei.more"].tap()
        XCTAssertTrue(app.sheets.buttons["返回我的文档库"].waitForExistence(timeout: 5))
        XCTAssertFalse(app.sheets.buttons["编辑和格式"].exists)
        XCTAssertFalse(app.sheets.buttons["删除笔记"].exists)
        shot("sharing-02-readonly-actions")
        app.sheets.buttons["返回我的文档库"].tap()
        XCTAssertTrue(app.buttons["fei.settings"].waitForExistence(timeout: 15),app.debugDescription)
        app.buttons["fei.library-actions"].tap()
        app.sheets.buttons["共享笔记"].tap()
        XCTAssertTrue(app.navigationBars["共享笔记"].waitForExistence(timeout: 5))
        app.coordinate(withNormalizedOffset: CGVector(dx: 0.66,dy: 0.38)).tap()
        XCTAssertTrue(app.navigationBars["FEI_SHARED_WRITABLE"].waitForExistence(timeout: 15),app.debugDescription)
        app.coordinate(withNormalizedOffset: CGVector(dx: 0.3,dy: 0.36)).tap()
        app.typeText("FEI_SHARED_OFFLINE_BODY ")
        app.buttons["fei.more"].tap()
        XCTAssertTrue(app.sheets.buttons["编辑和格式"].waitForExistence(timeout: 5))
        shot("sharing-03-writable-actions")
        app.sheets.buttons["返回我的文档库"].tap()
        XCTAssertTrue(app.buttons["fei.settings"].waitForExistence(timeout: 15),app.debugDescription)
        XCTAssertEqual(app.state,.runningForeground)
        app.buttons["fei.new"].tap()
        XCTAssertTrue(app.buttons["fei.more"].waitForExistence(timeout: 5))
        app.coordinate(withNormalizedOffset: CGVector(dx: 0.3,dy: 0.28)).tap()
        app.typeText("FEI_PERSONAL_AFTER_SHARED ")
        app.navigationBars.buttons.element(boundBy: 0).tap()
        XCTAssertTrue(app.buttons["fei.settings"].waitForExistence(timeout: 5))
        shot("sharing-04-personal-editor-still-working")
    }

    func testOfflineCloudSessionSwitchesToLocalWithoutRestart() {
        continueAfterFailure = false
        let app = XCUIApplication()
        app.launchEnvironment["FEI_NOTES_MODE"] = "cloud"
        app.launchEnvironment["FEI_NOTES_CLOUD_CHANNEL"] = "61d4fa9f-8938-468b-a920-fdb41ba3afe0"
        app.launch()
        XCTAssertTrue(app.buttons["fei.settings"].waitForExistence(timeout: 15))
        app.buttons["fei.library-actions"].tap()
        app.sheets.buttons["文档库"].tap()
        XCTAssertTrue(app.sheets.buttons["本机文档库"].waitForExistence(timeout: 5))
        XCTAssertTrue(app.sheets.buttons["本机文档库"].isEnabled)
        XCTAssertFalse(app.sheets.buttons["iCloud 文档库"].isEnabled)
        shot("session-01-offline-cloud")
        app.sheets.buttons["本机文档库"].tap()
        gone(app.sheets.firstMatch)
        XCTAssertTrue(app.buttons["fei.settings"].waitForExistence(timeout: 15))
        app.buttons["fei.library-actions"].tap()
        app.sheets.buttons["文档库"].tap()
        XCTAssertTrue(app.sheets.buttons["iCloud 文档库"].waitForExistence(timeout: 5))
        XCTAssertFalse(app.sheets.buttons["本机文档库"].isEnabled)
        XCTAssertTrue(app.sheets.buttons["iCloud 文档库"].isEnabled)
        shot("session-02-local-library")
        app.coordinate(withNormalizedOffset: CGVector(dx: 0.1, dy: 0.9)).tap()
        app.buttons["fei.new"].tap()
        XCTAssertTrue(app.buttons["fei.more"].waitForExistence(timeout: 5))
        app.coordinate(withNormalizedOffset: CGVector(dx: 0.3, dy: 0.28)).tap()
        app.typeText("FEI_AFTER_SESSION_SWITCH ")
        app.navigationBars.buttons.element(boundBy: 0).tap()
        XCTAssertTrue(app.buttons["fei.settings"].waitForExistence(timeout: 5))
        shot("session-03-local-edit")
    }

    func testSharedDocumentPDFExportAndPrintCancellation() {
        continueAfterFailure = false
        let exportName = "FEI-PDF-ACCEPT6-\(Int(Date().timeIntervalSince1970))"
        let app = XCUIApplication()
        app.launchEnvironment["FEI_NOTES_MODE"] = "local"
        app.launch()
        XCTAssertTrue(app.buttons["fei.settings"].waitForExistence(timeout: 15))
        app.buttons["fei.library-actions"].tap()
        app.sheets.buttons["导入笔记"].tap()
        let fixture = app.cells.matching(NSPredicate(format: "label CONTAINS %@", "FEI-PDF-ACCEPT6")).firstMatch
        if !fixture.waitForExistence(timeout: 2) {
            let browse = app.buttons["浏览"]
            if browse.waitForExistence(timeout: 3) { browse.tap() }
            if !fixture.waitForExistence(timeout: 2) {
                let folder = app.cells.matching(NSPredicate(format: "label BEGINSWITH %@", "飞记")).firstMatch
                if !folder.waitForExistence(timeout: 2) {
                    let phone = app.cells.matching(NSPredicate(format: "label CONTAINS %@", "iPhone")).firstMatch
                    XCTAssertTrue(phone.waitForExistence(timeout: 5), app.debugDescription)
                    phone.tap()
                }
                XCTAssertTrue(folder.waitForExistence(timeout: 5), app.debugDescription)
                folder.tap()
            }
        }
        XCTAssertTrue(fixture.waitForExistence(timeout: 10), app.debugDescription)
        fixture.tap()
        let open = app.buttons["打开"]
        if open.waitForExistence(timeout: 3) { open.tap() }
        XCTAssertTrue(app.navigationBars["FEI-PDF-ACCEPT6"].waitForExistence(timeout: 10), app.debugDescription)
        app.buttons["fei.more"].tap()
        app.sheets.buttons["导出 PDF"].tap()
        XCTAssertTrue(app.buttons["DOCPicker.actionButton"].waitForExistence(timeout: 30), app.debugDescription)
        let name = app.textFields.firstMatch
        XCTAssertTrue(name.waitForExistence(timeout: 3))
        name.tap()
        name.typeText(String(repeating: XCUIKeyboardKey.delete.rawValue, count: ((name.value as? String)?.count ?? 40) * 2))
        name.typeText(exportName)
        shot("pdf-01-native-file-export")
        app.buttons["DOCPicker.actionButton"].tap()
        XCTAssertTrue(app.buttons["fei.more"].waitForExistence(timeout: 10))
        app.buttons["fei.more"].tap()
        app.sheets.buttons["打印"].tap()
        let cancel = app.buttons.matching(NSPredicate(format: "label IN %@", ["取消", "Cancel"])).firstMatch
        XCTAssertTrue(cancel.waitForExistence(timeout: 30), app.debugDescription)
        shot("pdf-02-native-print-preview")
        cancel.tap()
        XCTAssertTrue(app.buttons["fei.more"].waitForExistence(timeout: 10))
        XCTAssertEqual(app.state, .runningForeground)
        print("PDF_EXPORT_NAME=\(exportName)")
        shot("pdf-03-print-cancelled-editor-retained")
    }

    func testThemeCSSUsesNativeEditorAndSharedFile() {
        continueAfterFailure = false
        let app = XCUIApplication()
        app.launchEnvironment["FEI_NOTES_MODE"] = "local"
        app.launch()
        XCTAssertTrue(app.buttons["fei.settings"].waitForExistence(timeout: 15))
        app.buttons["fei.settings"].tap()
        app.coordinate(withNormalizedOffset: CGVector(dx: 0.4, dy: 0.23)).tap()
        XCTAssertTrue(app.navigationBars["外观"].waitForExistence(timeout: 5))
        app.swipeUp()
        shot("theme-01-css-entry")
        app.coordinate(withNormalizedOffset: CGVector(dx: 0.53, dy: 0.905)).tap()
        let source = app.textViews["fei.theme.source"]
        XCTAssertTrue(source.waitForExistence(timeout: 5), app.debugDescription)
        let original = source.value as? String ?? ""
        XCTAssertTrue(original.hasPrefix("/*"))
        let marker = "\n * FEI_CSS_SAVE6_\(Int(Date().timeIntervalSince1970))\n"
        source.coordinate(withNormalizedOffset: CGVector(dx: 0.25, dy: 0.025)).tap()
        source.typeText(marker)
        let edited = source.value as? String ?? ""
        XCTAssertTrue(edited.contains(marker))
        XCTAssertEqual(edited.replacingOccurrences(of: marker, with: ""), original)
        shot("theme-02-native-source-edit")
        app.buttons["fei.theme.save"].tap()
        gone(source)
        XCTAssertTrue(app.navigationBars["外观"].exists)
        XCTAssertEqual(app.state, .runningForeground)
        print("CSS_SAVE_MARKER=\(marker.trimmingCharacters(in: .whitespacesAndNewlines))")
        shot("theme-03-saved-shared-settings")
    }

}
