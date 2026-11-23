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
        app.sheets.buttons["文档选项"].tap()
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
        app.sheets.buttons["文档选项"].tap()
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

    func testSharedSessionSelectionRoundTrip() { checkSharedMenu(folder: false) }

    func testSharedSyncBadgeAndStableEditor() {
        continueAfterFailure = false
        let app = XCUIApplication()
        let pad = UIDevice.current.userInterfaceIdiom == .pad
        XCUIDevice.shared.orientation = pad ? .landscapeLeft : .portrait
        app.launchArguments += ["-AppleLanguages","(en)"]
        app.launchEnvironment["FEI_NOTES_MODE"] = "cloud"
        app.launchEnvironment["FEI_NOTES_CLOUD_CHANNEL"] = "44ecb219-6dec-4b51-ae62-bcd575b86813"
        app.launchEnvironment["FEI_QA_SYNC_BADGE"] = "1"
        app.launch()
        let shared = app.segmentedControls["fei.notes.tabs"].buttons["Shared documents"]
        XCTAssertTrue(shared.waitForExistence(timeout: 12)); shared.tap()
        let note = app.tables["fei.notes.list"].staticTexts["FEI_SHARED_READONLY"].firstMatch
        XCTAssertTrue(note.waitForExistence(timeout: 8)); note.tap()

        let more = app.buttons["fei.more"]
        let surface = app.otherElements["fei.editor.surface"].firstMatch
        XCTAssertTrue(more.waitForExistence(timeout: 5))
        let buttonFrame = more.frame
        let editorFrame = surface.exists ? surface.frame : nil
        let warning = "Sync needs attention"
        XCTAssertNotEqual(more.value as? String,warning)
        shot("sync-normal")
        let attention = XCTNSPredicateExpectation(predicate: NSPredicate(format: "value == %@",warning),object: more)
        XCTAssertEqual(XCTWaiter.wait(for: [attention],timeout: 17),.completed)
        XCTAssertEqual(more.frame,buttonFrame)
        if let editorFrame { XCTAssertEqual(surface.frame,editorFrame) }
        shot("sync-stalled")
        more.tap()
        let statusEntry = app.descendants(matching: .any).matching(NSPredicate(format: "label == %@", "Sync status")).firstMatch
        XCTAssertTrue(statusEntry.waitForExistence(timeout: 5)); statusEntry.tap()
        let status = app.staticTexts["fei.shared-sync.status"]
        XCTAssertTrue(status.waitForExistence(timeout: 5))
        XCTAssertEqual(status.label,"Upload stalled")
        XCTAssertFalse(app.tables["fei.settings.list"].exists)
        shot("sync-detail")
        let recovered = XCTNSPredicateExpectation(predicate: NSPredicate(format: "label == %@", "Up to date"),object: status)
        XCTAssertEqual(XCTWaiter.wait(for: [recovered],timeout: 24),.completed)
        app.buttons["fei.shared-sync.done"].tap()
        XCTAssertEqual(more.frame,buttonFrame)
        XCTAssertNotEqual(more.value as? String,warning)
        if let editorFrame { XCTAssertEqual(surface.frame,editorFrame) }
        shot("sync-recovered")
    }

    func testFoldersFirstInMyDocuments() {
        continueAfterFailure = false
        let app = XCUIApplication()
        let pad = UIDevice.current.userInterfaceIdiom == .pad
        XCUIDevice.shared.orientation = pad ? .landscapeLeft : .portrait
        app.launchArguments += ["-AppleLanguages","(en)"]
        app.launchEnvironment["FEI_NOTES_MODE"] = "cloud"
        app.launchEnvironment["FEI_NOTES_CLOUD_CHANNEL"] = "44ecb219-6dec-4b51-ae62-bcd575b86813"
        app.launchEnvironment["FEI_QA_SYNC_BADGE"] = "1"
        app.launch()
        let folder = app.tables["fei.notes.list"].staticTexts["FEI_SHARED_FOLDER"].firstMatch
        let more = app.buttons["fei.library-actions"].firstMatch
        XCTAssertTrue(folder.waitForExistence(timeout: 12))
        let personal = app.tables["fei.notes.list"].staticTexts["FEI_PERSONAL_SESSION"].firstMatch
        XCTAssertTrue(personal.exists)
        func sortMenu() {
            more.tap()
            XCTAssertFalse(app.buttons["Reorder"].exists)
            app.buttons["Sort by"].tap()
        }
        sortMenu()
        app.buttons["Title ascending"].tap()
        sortMenu()
        let foldersFirst = app.buttons["Folders first"]
        XCTAssertTrue(foldersFirst.isEnabled)
        shot("sharing-sort-folders-first")
        foldersFirst.tap()
        XCTAssertTrue(XCTWaiter.wait(for: [XCTNSPredicateExpectation(predicate: NSPredicate { _, _ in
            folder.frame.midY < personal.frame.midY
        }, object: nil)], timeout: 5) == .completed)
        sortMenu()
        app.buttons["Manual order"].tap()
        let done = app.buttons["fei.notes.reorder.done"]
        XCTAssertTrue(done.waitForExistence(timeout: 5))
        let cell = app.tables["fei.notes.list"].cells.containing(.staticText,identifier: "FEI_SHARED_FOLDER").firstMatch
        let handle = cell.descendants(matching: .any).matching(NSPredicate(format: "label BEGINSWITH %@", "Reorder")).firstMatch
        XCTAssertTrue(handle.exists,cell.debugDescription)
        shot("sharing-sort-shared-folder-handle")
        done.tap()
        sortMenu()
        XCTAssertFalse(foldersFirst.isEnabled)
        shot("sharing-sort-manual")
    }

    func testPhoneSinglePopStaysOnHomeWithEmojiFixture() {
        guard UIDevice.current.userInterfaceIdiom == .phone else { return }
        continueAfterFailure = false
        let app = XCUIApplication()
        XCUIDevice.shared.orientation = .portrait
        app.launchArguments += ["-AppleLanguages", "(en)"]
        app.launchEnvironment["FEI_NOTES_MODE"] = "cloud"
        app.launchEnvironment["FEI_NOTES_CLOUD_CHANNEL"] = "44ecb219-6dec-4b51-ae62-bcd575b86813"
        app.launchEnvironment["FEI_QA_SYNC_BADGE"] = "1"
        app.launchEnvironment["FEI_QA_STALE_POP_SNAPSHOT"] = "1"
        app.launch()

        let note = app.tables["fei.notes.list"].staticTexts["FEI_PERSONAL_SESSION"]
        XCTAssertTrue(note.waitForExistence(timeout: 12))
        note.tap()
        let more = app.buttons["fei.more"]
        XCTAssertTrue(more.waitForExistence(timeout: 8))
        shot("checklist-emoji")
        app.navigationBars.buttons.firstMatch.tap()
        let tabs = app.segmentedControls["fei.notes.tabs"]
        XCTAssertTrue(tabs.waitForExistence(timeout: 8))
        let reopened = XCTNSPredicateExpectation(predicate: NSPredicate(format: "exists == true"), object: more)
        reopened.isInverted = true
        XCTAssertEqual(XCTWaiter.wait(for: [reopened], timeout: 3), .completed)
        XCTAssertTrue(tabs.buttons["My documents"].isSelected)
        XCTAssertTrue(app.buttons["fei.new"].isHittable)
        shot("single-pop-home")
    }

    func testPhoneSharedReturnPreservesOriginAndCompose() {
        guard UIDevice.current.userInterfaceIdiom == .phone else { return }
        continueAfterFailure = false
        let app = XCUIApplication()
        XCUIDevice.shared.orientation = .portrait
        app.launchArguments += ["-AppleLanguages", "(en)"]
        app.launchEnvironment["FEI_NOTES_MODE"] = "cloud"
        app.launchEnvironment["FEI_NOTES_CLOUD_CHANNEL"] = "44ecb219-6dec-4b51-ae62-bcd575b86813"
        app.launchEnvironment["FEI_QA_SYNC_BADGE"] = "1"
        app.launchEnvironment["FEI_QA_STALE_POP_SNAPSHOT"] = "1"
        app.launch()

        let list = app.tables["fei.notes.list"].firstMatch
        let tabs = app.segmentedControls["fei.notes.tabs"]
        let settings = app.buttons["fei.settings"]
        let more = app.buttons["fei.more"]
        let compose = app.buttons["fei.new"]
        let search = app.searchFields["fei.notes.search-field"]
        let child = list.staticTexts["FEI_SHARED_FOLDER_DOCUMENT"]
        let favorite = tabs.buttons["All Favorites"]
        let personal = tabs.buttons["My documents"]
        let shared = tabs.buttons["Shared documents"]
        func returnFromPage() { app.navigationBars.buttons.firstMatch.tap() }

        XCTAssertTrue(personal.waitForExistence(timeout: 12))
        XCTAssertTrue(compose.isHittable)
        let homeFrame = search.frame
        let note = list.staticTexts["FEI_PERSONAL_SESSION"]
        for _ in 0..<2 {
            note.tap()
            XCTAssertTrue(more.waitForExistence(timeout: 8))
            returnFromPage()
            XCTAssertTrue(settings.waitForExistence(timeout: 8))
            XCTAssertTrue(personal.isSelected)
            XCTAssertTrue(compose.isHittable)
            XCTAssertEqual(search.frame, homeFrame)
            XCTAssertFalse(more.exists)
        }
        shot("pop-personal-home")

        favorite.tap()
        XCTAssertTrue(child.waitForExistence(timeout: 8))
        child.tap()
        XCTAssertTrue(more.waitForExistence(timeout: 8))
        returnFromPage()
        XCTAssertTrue(favorite.waitForExistence(timeout: 8))
        XCTAssertTrue(favorite.isSelected)
        XCTAssertTrue(settings.exists)
        XCTAssertTrue(child.exists)
        XCTAssertFalse(more.exists)
        XCTAssertFalse(compose.isHittable)
        shot("pop-favorite-home")

        shared.tap()
        list.staticTexts["FEI_SHARED_FOLDER"].tap()
        XCTAssertTrue(list.staticTexts["FEI_SHARED_CHILD_FOLDER"].waitForExistence(timeout: 8))
        list.staticTexts["FEI_SHARED_CHILD_FOLDER"].tap()
        XCTAssertTrue(child.waitForExistence(timeout: 8))
        child.tap()
        XCTAssertTrue(more.waitForExistence(timeout: 8))
        returnFromPage()
        XCTAssertTrue(child.waitForExistence(timeout: 8))
        XCTAssertFalse(tabs.exists)
        XCTAssertTrue(app.navigationBars["FEI_SHARED_CHILD_FOLDER"].exists)
        shot("pop-shared-child-folder")
        returnFromPage()
        XCTAssertTrue(list.staticTexts["FEI_SHARED_CHILD_FOLDER"].waitForExistence(timeout: 8))
        returnFromPage()
        XCTAssertTrue(shared.waitForExistence(timeout: 8))
        XCTAssertTrue(shared.isSelected)
        XCTAssertFalse(more.exists)
        shot("pop-shared-home")
    }

    func testPhoneFolderAndEditorTransitionsKeepSearchStable() {
        guard UIDevice.current.userInterfaceIdiom == .phone else { return }
        continueAfterFailure = false
        let app = XCUIApplication()
        XCUIDevice.shared.orientation = .portrait
        app.launchArguments += ["-AppleLanguages", "(en)"]
        app.launchEnvironment["FEI_NOTES_MODE"] = "cloud"
        app.launchEnvironment["FEI_NOTES_CLOUD_CHANNEL"] = "44ecb219-6dec-4b51-ae62-bcd575b86813"
        app.launchEnvironment["FEI_QA_SYNC_BADGE"] = "1"
        app.launch()

        let list = app.tables["fei.notes.list"].firstMatch
        let search = app.searchFields["fei.notes.search-field"]
        let folder = list.staticTexts["FEI_SHARED_FOLDER"]
        let regular = list.staticTexts["FEI_PERSONAL_FOLDER"]
        let editor = app.toolbars["fei.editor.toolbar"]
        XCTAssertTrue(folder.waitForExistence(timeout: 12))
        XCTAssertTrue(regular.exists)
        XCTAssertEqual(folder.frame.minX, regular.frame.minX, accuracy: 1)
        XCTAssertTrue(search.isHittable)
        let homeSearch = search.frame
        shot("motion-home")

        folder.tap()
        XCTAssertTrue(list.staticTexts["FEI_SHARED_CHILD_FOLDER"].waitForExistence(timeout: 8))
        XCTAssertTrue(search.isHittable)
        XCTAssertEqual(search.frame.size.width, homeSearch.width, accuracy: 1)
        XCTAssertEqual(search.frame.size.height, homeSearch.height, accuracy: 1)
        XCTAssertFalse(editor.isHittable)
        shot("motion-folder")
        app.navigationBars["FEI_SHARED_FOLDER"].buttons.firstMatch.tap()

        let document = list.staticTexts["FEI_PERSONAL_SESSION"]
        XCTAssertTrue(document.waitForExistence(timeout: 8))
        document.tap()
        XCTAssertTrue(app.buttons["fei.more"].waitForExistence(timeout: 8))
        XCTAssertTrue(editor.isHittable)
        shot("motion-editor")
        app.navigationBars.buttons.firstMatch.tap()
        XCTAssertTrue(folder.waitForExistence(timeout: 8))
        XCTAssertEqual(search.frame, homeSearch)
        XCTAssertFalse(editor.isHittable)
        shot("motion-return")

        search.tap()
        search.typeText("NO_MATCH_FOR_TRANSITION")
        let noMatches = app.staticTexts["No matching documents"]
        XCTAssertTrue(noMatches.waitForExistence(timeout: 5))
        let keyboard = app.keyboards.firstMatch
        XCTAssertTrue(keyboard.exists)
        XCTAssertLessThanOrEqual(search.frame.maxY, keyboard.frame.minY)
        shot("motion-search-active")
        app.buttons.matching(NSPredicate(format: "label IN {'Cancel', '取消'}")).firstMatch.tap()
        XCTAssertFalse(keyboard.exists)
        XCTAssertTrue(document.exists)
        XCTAssertEqual(search.frame, homeSearch)
    }

    func testPadDocumentToolbarWithAndWithoutSidebar() throws {
        guard UIDevice.current.userInterfaceIdiom == .pad else { return }
        continueAfterFailure = false
        let app = XCUIApplication()
        XCUIDevice.shared.orientation = .landscapeLeft
        app.launchArguments += ["-AppleLanguages", "(en)"]
        app.launchEnvironment["FEI_NOTES_MODE"] = "cloud"
        app.launchEnvironment["FEI_NOTES_CLOUD_CHANNEL"] = "44ecb219-6dec-4b51-ae62-bcd575b86813"
        app.launchEnvironment["FEI_QA_SYNC_BADGE"] = "1"
        app.launch()

        let list = app.tables["fei.notes.list"].firstMatch
        let note = list.staticTexts["FEI_PERSONAL_SESSION"]
        XCTAssertTrue(note.waitForExistence(timeout: 12))
        note.tap()
        let block = app.navigationBars.buttons["fei.edit.block-actions"].firstMatch
        XCTAssertTrue(block.waitForExistence(timeout: 8))
        let compose = app.buttons.matching(identifier: "fei.new")
        let toolbar = app.toolbars["fei.editor.toolbar"]
        let focus = app.buttons["fei.ipad.focus"].firstMatch
        XCTAssertTrue(list.isHittable)
        XCTAssertFalse(toolbar.exists && toolbar.isHittable)
        XCTAssertEqual(compose.allElementsBoundByIndex.filter(\.isHittable).count, 1)
        XCTAssertTrue(app.navigationBars.buttons["fei.new"].isHittable)
        shot("pad-editor-two-columns")

        focus.tap()
        let sidebarHidden = XCTNSPredicateExpectation(predicate: NSPredicate(format: "hittable == false"), object: list)
        XCTAssertEqual(XCTWaiter.wait(for: [sidebarHidden], timeout: 6), .completed)
        XCTAssertTrue(block.isHittable)
        XCTAssertFalse(toolbar.exists && toolbar.isHittable)
        XCTAssertFalse(compose.allElementsBoundByIndex.contains(where: \.isHittable))
        shot("pad-editor-one-column")

        let surface = app.otherElements["fei.editor.surface"]
        XCTAssertTrue(surface.waitForExistence(timeout: 6))
        let layout = try XCTUnwrap(surface.value as? String)
        let probe = try XCTUnwrap(JSONSerialization.jsonObject(with: Data(layout.utf8)) as? [String: Any])
        let rows = try XCTUnwrap(probe["blocks"] as? [[String: Any]])
        let row = try XCTUnwrap(rows.first { ($0["kind"] as? String) == "Text" || ($0["kind"] as? String)?.hasPrefix("Todo") == true })
        let rect = try XCTUnwrap(row["rect"] as? [CGFloat])
        XCTAssertEqual(rect.count, 4)
        surface.coordinate(withNormalizedOffset: .zero)
            .withOffset(CGVector(dx: rect[0] + 36, dy: rect[1] + rect[3] / 2)).tap()
        shot("pad-editor-focused-block")
        let available = XCTNSPredicateExpectation(predicate: NSPredicate(format: "enabled == true"), object: block)
        XCTAssertEqual(XCTWaiter.wait(for: [available], timeout: 6), .completed)
        block.tap()
        XCTAssertTrue(app.tables["fei.block.controls"].waitForExistence(timeout: 6))
        shot("pad-editor-block-actions")
    }

    func testPadFolderTransitionsShowSharedAndPersonalRows() {
        guard UIDevice.current.userInterfaceIdiom == .pad else { return }
        continueAfterFailure = false
        let app = XCUIApplication()
        app.launchArguments += ["-AppleLanguages", "(en)"]
        app.launchEnvironment["FEI_NOTES_MODE"] = "cloud"
        app.launchEnvironment["FEI_NOTES_CLOUD_CHANNEL"] = "44ecb219-6dec-4b51-ae62-bcd575b86813"
        app.launchEnvironment["FEI_QA_FOLDER_TRANSITIONS"] = "1"
        app.launchEnvironment["FEI_QA_SYNC_BADGE"] = "1"
        XCUIDevice.shared.orientation = .landscapeLeft
        app.launch()

        let list = app.tables["fei.notes.list"].firstMatch
        let ordinary = list.staticTexts["FEI_PERSONAL_FOLDER"]
        let shared = list.staticTexts["FEI_SHARED_FOLDER"]
        XCTAssertTrue(shared.waitForExistence(timeout: 12))
        XCTAssertEqual(shared.frame.minX, ordinary.frame.minX, accuracy: 1)
        shot("pad-folder-alignment")
        let tabs = app.segmentedControls["fei.notes.tabs"]
        let child = list.staticTexts["FEI_SHARED_CHILD_FOLDER"]
        for orientation in [UIDeviceOrientation.landscapeLeft, .portrait] {
            XCUIDevice.shared.orientation = orientation
            tabs.buttons["Shared documents"].tap()
            shared.tap()
            XCTAssertTrue(child.waitForExistence(timeout: 8))
            let more = app.buttons["fei.library-actions"]
            XCTAssertEqual(more.value as? String, "animated push")
            shot(orientation == .portrait ? "pad-shared-portrait" : "pad-shared-landscape")
            app.buttons["fei.browser.parent"].tap()
            XCTAssertTrue(shared.waitForExistence(timeout: 8))
            XCTAssertEqual(more.value as? String, "animated pop")
            tabs.buttons["My documents"].tap()
        }
    }

    func testFeedbackSearchAndInsertVisuals() {
        guard UIDevice.current.userInterfaceIdiom == .phone else { return }
        continueAfterFailure = false
        let app = XCUIApplication()
        XCUIDevice.shared.orientation = .portrait
        app.launchArguments += ["-AppleLanguages","(en)"]
        app.launchEnvironment["FEI_NOTES_MODE"] = "cloud"
        app.launchEnvironment["FEI_NOTES_CLOUD_CHANNEL"] = "44ecb219-6dec-4b51-ae62-bcd575b86813"
        app.launchEnvironment["FEI_QA_SYNC_BADGE"] = "1"
        app.launch()
        let list = app.tables["fei.notes.list"].firstMatch
        let folder = list.staticTexts["FEI_SHARED_FOLDER"]
        XCTAssertTrue(folder.waitForExistence(timeout: 12))
        shot("feedback-search-home")
        folder.tap()
        XCTAssertTrue(list.staticTexts["FEI_SHARED_CHILD_FOLDER"].waitForExistence(timeout: 12))
        shot("feedback-search-folder")
        app.navigationBars["FEI_SHARED_FOLDER"].buttons.firstMatch.tap()
        let document = list.staticTexts["FEI_PERSONAL_SESSION"]
        XCTAssertTrue(document.waitForExistence(timeout: 10))
        document.tap()
        let more = app.buttons["fei.more"]
        XCTAssertTrue(more.waitForExistence(timeout: 10))
        shot("feedback-emoji-and-photo")
        more.tap()
        let export = app.buttons["Export and print"]
        XCTAssertTrue(export.waitForExistence(timeout: 5))
        export.tap()
        XCTAssertTrue(app.buttons["Export Markdown"].exists)
        shot("feedback-export-and-print")
        app.coordinate(withNormalizedOffset: CGVector(dx: 0.1,dy: 0.6)).tap()
        more.tap()
        let editing = app.buttons["Editing and formatting"]
        XCTAssertTrue(editing.waitForExistence(timeout: 5))
        editing.tap()
        app.collectionViews.buttons["Insert content"].firstMatch.tap()
        let insert = app.tables["fei.insert.controls"]
        XCTAssertTrue(insert.waitForExistence(timeout: 5))
        for title in ["Photo Library","Image from Files","Video or attachment from Files"] {
            XCTAssertTrue(insert.staticTexts[title].exists,title)
        }
        shot("feedback-attachment-source")
    }

    func testPersonalDocumentSharingAudienceAndPermissions() {
        continueAfterFailure = false
        let app = XCUIApplication()
        let pad = UIDevice.current.userInterfaceIdiom == .pad
        XCUIDevice.shared.orientation = pad ? .landscapeLeft : .portrait
        app.launchArguments += ["-AppleLanguages", "(en)"]
        app.launchEnvironment["FEI_NOTES_MODE"] = "cloud"
        app.launchEnvironment["FEI_NOTES_CLOUD_CHANNEL"] = "44ecb219-6dec-4b51-ae62-bcd575b86813"
        app.launchEnvironment["FEI_QA_SYNC_BADGE"] = "1"
        app.launchEnvironment["FEI_QA_PERSONAL_SYNC_STATE"] = "ready"
        app.launchEnvironment["FEI_QA_OFFLINE_INVITATION"] = "1"
        app.launch()

        let tabs = app.segmentedControls["fei.notes.tabs"]
        let list = app.tables["fei.notes.list"]
        let personal = list.staticTexts["FEI_PERSONAL_SESSION"]
        let sharedEditor = app.navigationBars["FEI_SHARED_READONLY"]
        XCTAssertTrue(personal.waitForExistence(timeout: 12))
        if pad {
            tabs.buttons["Shared documents"].tap()
            let received = list.staticTexts["FEI_SHARED_READONLY"]
            XCTAssertTrue(received.waitForExistence(timeout: 8)); received.tap()
            XCTAssertTrue(sharedEditor.waitForExistence(timeout: 8))
            tabs.buttons["My documents"].tap()
            XCTAssertTrue(personal.waitForExistence(timeout: 8))
            XCTAssertTrue(sharedEditor.exists)
        }
        personal.press(forDuration: 0.8)
        let share = app.buttons["Share"]
        XCTAssertTrue(share.waitForExistence(timeout: 5)); share.tap()
        XCTAssertTrue(app.navigationBars["Share document"].waitForExistence(timeout: 8))
        XCTAssertTrue(app.staticTexts["FEI_PERSONAL_SESSION"].exists)
        shot("personal-share-invited-default")

        func setting(_ id: String) -> XCUIElement { app.cells["fei.setting." + id] }
        func option(_ id: String, _ value: String) -> XCUIElement { setting("option.\(id).\(value)") }
        func change(_ id: String, from: String, to: String) {
            let row = setting(id)
            XCTAssertTrue(row.waitForExistence(timeout: 5)); row.tap()
            let current = option(id, from)
            XCTAssertTrue(current.waitForExistence(timeout: 5)); XCTAssertTrue(current.isSelected)
            option(id, to).tap()
            XCTAssertTrue(row.waitForExistence(timeout: 5))
        }

        XCTAssertTrue(setting("share-audience").exists, "Sharing a personal document must show both audiences")
        XCTAssertTrue(setting("share-invite").staticTexts["Create invitation"].exists)
        change("share-permission", from: "read", to: "write")
        change("share-permission", from: "write", to: "read")
        change("share-audience", from: "invited", to: "link")
        XCTAssertFalse(setting("share-invite").exists)
        XCTAssertTrue(setting("share-submit").isEnabled)
        XCTAssertTrue(setting("share-submit").staticTexts["Create invitation"].exists)
        change("share-public-permission", from: "read", to: "write")
        XCTAssertTrue(setting("public-edit-warning").exists)
        shot("personal-share-public-editable")
        change("share-public-permission", from: "write", to: "read")
        XCTAssertFalse(setting("public-edit-warning").exists)
        change("share-audience", from: "link", to: "invited")
        XCTAssertTrue(setting("share-invite").exists)
        app.buttons["fei.share.done"].tap()
        XCTAssertTrue(personal.waitForExistence(timeout: 8))
        XCTAssertEqual(sharedEditor.exists, pad)
    }

    func testSharingOverviewInvitationAndCreation() {
        continueAfterFailure = false
        let app = XCUIApplication()
        let pad = UIDevice.current.userInterfaceIdiom == .pad
        XCUIDevice.shared.orientation = pad ? .landscapeLeft : .portrait
        app.launchArguments += ["-AppleLanguages","(en)"]
        app.launchEnvironment["FEI_NOTES_MODE"] = "cloud"
        app.launchEnvironment["FEI_NOTES_CLOUD_CHANNEL"] = "44ecb219-6dec-4b51-ae62-bcd575b86813"
        app.launchEnvironment["FEI_QA_SYNC_BADGE"] = "1"
        app.launch()
        let tabs = app.segmentedControls["fei.notes.tabs"]
        let more = app.buttons["fei.library-actions"].firstMatch
        XCTAssertTrue(tabs.waitForExistence(timeout: 12))
        tabs.buttons["Shared documents"].tap()
        XCTAssertFalse(app.buttons["fei.new"].exists)
        let owner = app.tables["fei.notes.list"].staticTexts["FEI_SHARED_WRITABLE"].firstMatch
        if pad {
            XCTAssertTrue(owner.waitForExistence(timeout: 8))
            owner.tap()
            XCTAssertTrue(app.navigationBars["FEI_SHARED_WRITABLE"].waitForExistence(timeout: 8))
            app.buttons["fei.more"].tap()
            XCTAssertFalse(app.buttons["Accept invitation"].exists)
            app.navigationBars["FEI_SHARED_WRITABLE"].tap()
        }
        more.tap()
        app.buttons["Accept invitation"].tap()
        let link = app.textFields["fei.share.invitation-link"]
        let accept = app.buttons["fei.share.accept"]
        XCTAssertTrue(link.waitForExistence(timeout: 8),app.debugDescription)
        XCTAssertFalse(accept.isEnabled)
        XCTAssertFalse(app.tables["fei.settings.list"].exists)
        shot("sharing-invitation-form")
        link.tap()
        link.typeText("invalid")
        XCTAssertTrue(accept.isEnabled)
        accept.tap()
        XCTAssertTrue(app.staticTexts["fei.share.invitation-status"].waitForExistence(timeout: 8))
        XCTAssertEqual(link.value as? String, "invalid")
        shot("sharing-invitation-invalid")
        app.buttons["fei.share.done"].tap()
        XCTAssertTrue(more.waitForExistence(timeout: 8))
        if pad {
            XCTAssertTrue(app.navigationBars["FEI_SHARED_WRITABLE"].exists)
            XCTAssertTrue(owner.exists)
        }
        more.tap()
        app.buttons["Create and share"].tap()
        shot("sharing-overview-create-menu")
        app.buttons["New shared folder"].tap()
        XCTAssertTrue(app.cells["fei.setting.share-new-name"].waitForExistence(timeout: 8))
        shot("sharing-new-folder-form")
        app.buttons["fei.share.done"].tap()
        XCTAssertFalse(app.tables["fei.settings.list"].exists)
        tabs.buttons["All Favorites"].tap()
        XCTAssertFalse(app.buttons["fei.new"].exists)
        more.tap()
        XCTAssertFalse(app.buttons["Import documents"].exists)
        XCTAssertFalse(app.buttons["Create and share"].exists)
        shot("sharing-favorites-menu")
    }

    func testSharedFolderMenuRoundTrip() { checkSharedMenu(folder: true) }
    func testSharedFolderMenuCompactRoundTrip() {
        guard UIDevice.current.userInterfaceIdiom == .pad else { return }
        checkSharedMenu(folder: true,compact: true)
    }

    private func checkSharedMenu(folder: Bool,compact: Bool = false) {
        continueAfterFailure = false
        let app = XCUIApplication()
        let pad = UIDevice.current.userInterfaceIdiom == .pad
        XCUIDevice.shared.orientation = pad && !compact ? .landscapeLeft : .portrait
        app.launchArguments += ["-AppleLanguages", "(en)"]
        app.launchEnvironment["FEI_NOTES_MODE"] = "cloud"
        app.launchEnvironment["FEI_NOTES_CLOUD_CHANNEL"] = "44ecb219-6dec-4b51-ae62-bcd575b86813"
        if folder && pad { app.launchEnvironment["FEI_QA_FOLDER_TRANSITIONS"] = "1" }
        app.launch()

        let list = app.tables["fei.notes.list"]
        let more = app.buttons["fei.more"]
        let folderMore = app.buttons["fei.library-actions"]
        func row(_ name: String) -> XCUIElement {
            list.staticTexts[name].firstMatch
        }
        func menu(_ label: String) -> XCUIElement {
            app.descendants(matching: .any).matching(NSPredicate(format: "label == %@", label)).firstMatch
        }
        func tap(_ item: XCUIElement) {
            XCTAssertTrue(item.waitForExistence(timeout: 12), "\(item.debugDescription)\n\(app.debugDescription)")
            if compact && !item.isHittable {
                let frame = item.frame
                XCTAssertFalse(frame.isEmpty)
                app.coordinate(withNormalizedOffset: .zero).withOffset(CGVector(
                    dx: frame.midX - app.frame.minX,dy: frame.midY - app.frame.minY
                )).tap()
            } else {
                item.tap()
            }
        }
        func back(_ title: String) {
            tap(app.navigationBars[title].buttons.firstMatch)
        }
        func tapSharedTab() { tap(app.segmentedControls["fei.notes.tabs"].buttons["Shared documents"]) }

        tapSharedTab()
        shot("sharing-home")
        if !folder {
            tap(row("FEI_SHARED_READONLY"))
            XCTAssertTrue(app.navigationBars["FEI_SHARED_READONLY"].waitForExistence(timeout: 12))
            XCTAssertEqual(app.buttons["fei.share"].label, "Shared")
            shot("sharing-received-editor")
            tap(app.buttons["fei.share"])
            XCTAssertTrue(app.navigationBars["Document sharing access"].waitForExistence(timeout: 5))
            XCTAssertTrue(app.cells["fei.setting.access"].exists)
            XCTAssertTrue(app.cells["fei.setting.share-source"].exists)
            shot("sharing-received-access")
            XCTAssertFalse(app.cells["fei.setting.invite-writer"].exists)
            tap(app.buttons["fei.share.done"])
            tap(more)
            XCTAssertFalse(menu("Return to my library").exists)
            XCTAssertFalse(menu("Document sharing access").exists)
            shot("sharing-received-menu")
            tap(menu("Sync status"))
            XCTAssertTrue(app.staticTexts["fei.shared-sync.status"].waitForExistence(timeout: 5))
            XCTAssertFalse(app.cells["fei.setting.history-limit"].exists)
            XCTAssertFalse(app.cells["fei.setting.library-switch"].exists)
            shot("sharing-received-sync")
            tap(app.buttons["fei.shared-sync.done"])
            XCTAssertTrue(app.navigationBars["FEI_SHARED_READONLY"].waitForExistence(timeout: 5))
            if !pad { back("FEI_SHARED_READONLY") }
            XCTAssertTrue(row("FEI_SHARED_FOLDER").waitForExistence(timeout: 8))
            return
        }

        tap(row("FEI_SHARED_FOLDER"))
        XCTAssertTrue(row("FEI_SHARED_CHILD_FOLDER").waitForExistence(timeout: 8), app.debugDescription)
        if pad { XCTAssertEqual(folderMore.value as? String,"animated push") }
        shot("sharing-root-toolbar")
        tap(app.buttons["fei.folder.share"])
        XCTAssertTrue(app.navigationBars["Folder sharing access"].waitForExistence(timeout: 5))
        XCTAssertTrue(app.cells["fei.setting.access"].exists)
        tap(app.buttons["fei.share.done"])
        XCTAssertTrue(row("FEI_SHARED_CHILD_FOLDER").waitForExistence(timeout: 5))
        tap(folderMore)
        XCTAssertFalse(app.buttons.matching(NSPredicate(format: "label == %@ AND identifier != %@", "Folder sharing access", "fei.folder.share")).firstMatch.exists)
        shot("sharing-root-menu")
        tap(menu("Sync status"))
        XCTAssertTrue(app.staticTexts["fei.shared-sync.status"].waitForExistence(timeout: 5))
        tap(app.buttons["fei.shared-sync.done"])
        tap(row("FEI_SHARED_CHILD_FOLDER"))
        if pad { XCTAssertEqual(folderMore.value as? String,"animated push") }
        tap(app.buttons["fei.folder.share"])
        XCTAssertTrue(app.navigationBars["Folder sharing access"].waitForExistence(timeout: 5))
        tap(app.buttons["fei.share.done"])
        tap(folderMore)
        XCTAssertFalse(app.buttons.matching(NSPredicate(format: "label == %@ AND identifier != %@", "Folder sharing access", "fei.folder.share")).firstMatch.exists)
        shot("sharing-child-menu")
        tap(menu("Sync status"))
        XCTAssertTrue(app.staticTexts["fei.shared-sync.status"].waitForExistence(timeout: 5))
        tap(app.buttons["fei.shared-sync.done"])
        tap(row("FEI_SHARED_FOLDER_DOCUMENT"))
        XCTAssertTrue(app.navigationBars["FEI_SHARED_FOLDER_DOCUMENT"].waitForExistence(timeout: 5))
        XCTAssertEqual(app.buttons["fei.share"].label, "Shared")
        if pad {
            let parent = app.buttons["fei.browser.parent"]
            if compact && !parent.exists { tap(app.buttons["fei.ipad.focus"]) }
            tap(parent)
            XCTAssertTrue(row("FEI_SHARED_CHILD_FOLDER").waitForExistence(timeout: 5))
            XCTAssertEqual(folderMore.value as? String,"animated pop")
            tap(parent)
        } else {
            back("FEI_SHARED_FOLDER_DOCUMENT")
            XCTAssertTrue(row("FEI_SHARED_FOLDER_DOCUMENT").waitForExistence(timeout: 5))
            back("FEI_SHARED_CHILD_FOLDER")
            XCTAssertTrue(row("FEI_SHARED_CHILD_FOLDER").waitForExistence(timeout: 5))
            back("FEI_SHARED_FOLDER")
        }
        XCTAssertTrue(row("FEI_SHARED_READONLY").waitForExistence(timeout: 8))
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
        app.sheets.buttons["导入文档"].tap()
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
