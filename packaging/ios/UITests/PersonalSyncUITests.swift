import XCTest

final class PersonalSyncUITests: XCTestCase {
    private func capture(_ name: String) {
        let screenshot = XCTAttachment(screenshot: XCUIScreen.main.screenshot())
        screenshot.name = name
        screenshot.lifetime = .keepAlways
        add(screenshot)
    }

    func testSharedItemMenuMatchesBothOrigins() {
        continueAfterFailure = false
        let app = XCUIApplication()
        let pad = UIDevice.current.userInterfaceIdiom == .pad
        XCUIDevice.shared.orientation = pad ? .landscapeLeft : .portrait
        app.launchArguments += ["-AppleLanguages","(en)"]
        app.launchEnvironment["FEI_NOTES_MODE"] = "cloud"
        app.launchEnvironment["FEI_NOTES_CLOUD_CHANNEL"] = "44ecb219-6dec-4b51-ae62-bcd575b86813"
        app.launchEnvironment["FEI_QA_SYNC_BADGE"] = "1"
        app.launchEnvironment["FEI_QA_OFFLINE_INVITATION"] = "1"
        app.launchEnvironment["FEI_QA_SHARED_BROWSER_REFRESH"] = "1"
        app.launch()
        let tabs = app.segmentedControls["fei.notes.tabs"]
        let my = tabs.buttons["My documents"]
        let shared = tabs.buttons["Shared documents"]
        XCTAssertTrue(my.waitForExistence(timeout: 15))

        func dismiss() {
            app.coordinate(withNormalizedOffset: CGVector(dx: 0.50,dy: 0.075)).tap()
        }
        let allLabels: Set<String> = ["Copy to My Documents","Duplicate","Move to","Pin","Unpin","Favorite","Remove favorite","Copy Link","Export",
            "Rename","Members and access","View sharing access","Organize in personal folder","Stop sharing","Leave share",
            "New document","New folder","Folder sharing access","Delete document","Delete folder","Delete shared document","Delete shared folder","Delete subfolder"]
        func menu(_ title: String, expected: [String], name: String) -> [String] {
            let row = moveRow(title,in: app)
            XCTAssertTrue(row.waitForExistence(timeout: 8),title)
            row.press(forDuration: 1)
            XCTAssertTrue(app.buttons[expected.last!].firstMatch.waitForExistence(timeout: 5),app.debugDescription)
            let order = app.buttons.allElementsBoundByIndex.filter { allLabels.contains($0.label) && $0.isHittable }
                .sorted { $0.frame.minY < $1.frame.minY }.map(\.label)
            XCTAssertEqual(order.filter { $0 != "Copy Link" },expected,name)
            capture(name)
            dismiss()
            return order
        }
        if pad {
            shared.tap()
            let other = moveRow("Shared owner fixture",in: app)
            XCTAssertTrue(other.waitForExistence(timeout: 8))
            other.tap()
            XCTAssertTrue(app.buttons["fei.share"].waitForExistence(timeout: 8))
        }
        my.tap()
        let folderMenu = ["New document","New folder","Rename","Folder sharing access","Delete folder","Stop sharing"]
        let rootInMy = menu("Owner",expected: folderMenu,name: "my-shared-folder")
        let childFolderMenu = ["New document","New folder","Rename","Move to","Folder sharing access","Delete subfolder"]
        let childDocMenu = ["Duplicate","Move to","Pin","Favorite","Export","Rename","Folder sharing access","Delete document"]
        func childMenus(_ origin: String) -> [[String]] {
            moveRow("Owner",in: app).tap()
            let source = moveRow("Owner source",in: app)
            XCTAssertTrue(source.waitForExistence(timeout: 8))
            let folderActions = menu("Owner source",expected: childFolderMenu,name: origin + "-child-folder")
            source.tap()
            let actions = menu("Owner document",expected: childDocMenu,name: origin + "-child-document")
            exitMoveFolder(app,pad: pad,title: "Owner source")
            exitMoveFolder(app,pad: pad,title: "Owner")
            return [folderActions,actions]
        }
        let descendantsInMy = childMenus("my")
        let directory = moveRow("QA collaboration folder",in: app)
        XCTAssertTrue(directory.waitForExistence(timeout: 8))
        directory.tap()
        let doc = "QA standalone collaboration"
        let docMenu = ["Copy to My Documents","Pin","Favorite","Export","Rename","Members and access","Stop sharing"]
        let inMy = menu(doc,expected: docMenu,name: "my-shared-document")
        exitMoveFolder(app,pad: pad,title: "QA collaboration folder")
        shared.tap()
        XCTAssertEqual(menu(doc,expected: docMenu,name: "overview-shared-document"),inMy)
        XCTAssertEqual(menu("Owner",expected: folderMenu,name: "overview-shared-folder"),rootInMy)
        XCTAssertEqual(childMenus("overview"),descendantsInMy)
        let reader = moveRow("Reader",in: app)
        XCTAssertTrue(reader.waitForExistence(timeout: 8))
        _ = menu("Reader",expected: ["Folder sharing access","Leave share"],name: "overview-read-only-folder")
        _ = menu("Shared reader fixture",expected: ["Copy to My Documents","Pin","Favorite","Export","View sharing access","Leave share"],
            name: "overview-read-only")
        for (role, write) in [("Editor",true),("Reader",false)] {
            moveRow(role,in: app).tap()
            let source = moveRow(role + " source",in: app)
            XCTAssertTrue(source.waitForExistence(timeout: 8))
            source.tap()
            _ = menu(role + " document",expected: write ? childDocMenu : ["Pin","Favorite","Export","Folder sharing access"],
                name: "overview-" + role.lowercased() + "-child")
            exitMoveFolder(app,pad: pad,title: role + " source")
            exitMoveFolder(app,pad: pad,title: role)
        }

        if pad {
            moveRow("Shared owner fixture",in: app).tap()
            XCTAssertTrue(app.navigationBars["Shared owner fixture"].waitForExistence(timeout: 8))
        }
        let sameShare = moveRow(doc,in: app)
        sameShare.press(forDuration: 1)
        app.buttons["Pin"].firstMatch.tap()
        if pad { XCTAssertTrue(app.navigationBars["Shared owner fixture"].exists,"Pin targeted the open editor") }
        my.tap()
        directory.tap()
        let pinnedMenu = docMenu.map { $0 == "Pin" ? "Unpin" : $0 }
        _ = menu(doc,expected: pinnedMenu,name: "pin-shared-to-my")
        moveRow(doc,in: app).press(forDuration: 1)
        app.buttons["Unpin"].firstMatch.tap()
        exitMoveFolder(app,pad: pad,title: "QA collaboration folder")
        shared.tap()
        _ = menu(doc,expected: docMenu,name: "unpin-my-to-shared")
    }

    func testPadSharedRefreshRetainsPersonalFolder() throws {
        try XCTSkipUnless(UIDevice.current.userInterfaceIdiom == .pad)
        continueAfterFailure = false
        let app = XCUIApplication()
        XCUIDevice.shared.orientation = .landscapeLeft
        app.launchArguments += ["-AppleLanguages", "(en)"]
        app.launchEnvironment["FEI_NOTES_MODE"] = "cloud"
        app.launchEnvironment["FEI_NOTES_CLOUD_CHANNEL"] = "44ecb219-6dec-4b51-ae62-bcd575b86813"
        app.launchEnvironment["FEI_QA_SYNC_BADGE"] = "1"
        app.launchEnvironment["FEI_QA_SHARED_BROWSER_REFRESH"] = "1"
        app.launch()
        let folder = moveRow("QA collaboration folder",in: app)
        XCTAssertTrue(folder.waitForExistence(timeout: 12))
        folder.tap()
        let row = moveRow("QA standalone collaboration",in: app)
        XCTAssertTrue(row.waitForExistence(timeout: 8))
        row.tap()
        let share = app.buttons["fei.share"]
        XCTAssertTrue(share.waitForExistence(timeout: 8))
        XCTAssertEqual(share.label,"Shared")
        let update = app.descendants(matching: .any).matching(NSPredicate(format: "identifier == %@ AND value CONTAINS %@",
            "fei.editor.surface","Remote fixture paragraph")).firstMatch
        XCTAssertTrue(update.waitForExistence(timeout: 12))
        XCTAssertTrue(row.exists)
        XCTAssertTrue(row.isSelected)
        XCTAssertTrue(app.buttons["fei.browser.parent"].exists)
        XCTAssertEqual(share.label,"Shared")
        capture("shared-refresh-keeps-personal-folder")
    }

    func testPhoneReturnsToPersonalFolderAfterSharedRefresh() throws {
        try XCTSkipUnless(UIDevice.current.userInterfaceIdiom == .phone)
        continueAfterFailure = false
        let app = XCUIApplication()
        XCUIDevice.shared.orientation = .portrait
        app.launchArguments += ["-AppleLanguages", "(en)"]
        app.launchEnvironment["FEI_NOTES_MODE"] = "cloud"
        app.launchEnvironment["FEI_NOTES_CLOUD_CHANNEL"] = "44ecb219-6dec-4b51-ae62-bcd575b86813"
        app.launchEnvironment["FEI_QA_SYNC_BADGE"] = "1"
        app.launchEnvironment["FEI_QA_SHARED_BROWSER_REFRESH"] = "1"
        app.launchEnvironment["FEI_QA_STALE_POP_SNAPSHOT"] = "1"
        app.launchEnvironment["FEI_QA_REBUILD_SHARED_ANCESTOR"] = "1"
        app.launch()
        let folder = moveRow("QA collaboration folder",in: app)
        XCTAssertTrue(folder.waitForExistence(timeout: 12))
        folder.tap()
        let row = moveRow("QA standalone collaboration",in: app)
        XCTAssertTrue(row.waitForExistence(timeout: 8))
        row.tap()
        let update = app.descendants(matching: .any).matching(NSPredicate(format: "identifier == %@ AND value CONTAINS %@",
            "fei.editor.surface","Remote fixture paragraph")).firstMatch
        XCTAssertTrue(update.waitForExistence(timeout: 12))
        app.navigationBars.buttons.firstMatch.tap()
        XCTAssertTrue(app.navigationBars["QA collaboration folder"].waitForExistence(timeout: 8))
        XCTAssertTrue(row.waitForExistence(timeout: 8))
        XCTAssertTrue(app.buttons["fei.library-actions"].exists)
        XCTAssertTrue(app.searchFields["fei.notes.search-field"].exists)
        capture("phone-personal-folder-after-shared-refresh")
    }

    func testEmptyMixedTrash() {
        continueAfterFailure = false
        let app = XCUIApplication()
        let pad = UIDevice.current.userInterfaceIdiom == .pad
        XCUIDevice.shared.orientation = pad ? .landscapeLeft : .portrait
        app.launchArguments += ["-AppleLanguages", "(en)"]
        app.launchEnvironment["FEI_NOTES_MODE"] = "cloud"
        app.launchEnvironment["FEI_NOTES_CLOUD_CHANNEL"] = "44ecb219-6dec-4b51-ae62-bcd575b86813"
        app.launchEnvironment["FEI_QA_SYNC_BADGE"] = "1"
        app.launchEnvironment["FEI_QA_PERSONAL_SYNC_STATE"] = "ready"
        app.launch()
        let settings = app.buttons["fei.settings"]
        XCTAssertTrue(settings.waitForExistence(timeout: 15))
        settings.tap()
        let trashEntry = app.cells["fei.setting.page:Trash"]
        XCTAssertTrue(trashEntry.waitForExistence(timeout: 5))
        trashEntry.tap()
        let trash = app.navigationBars["Trash"]
        XCTAssertTrue(trash.waitForExistence(timeout: 5))
        let empty = trash.buttons["fei.empty-trash"]
        XCTAssertTrue(empty.isEnabled)
        for title in ["QA personal trash", "QA shared trash", "QA stale shared trash"] {
            XCTAssertTrue(app.staticTexts[title].exists, title)
        }
        let items = app.tables["fei.notes.list"]
        let legacySingle = items.cells.containing(.staticText,identifier: "QA ended standalone single").firstMatch
        let legacyBatch = app.staticTexts["QA ended standalone batch"]
        XCTAssertTrue(legacySingle.exists && legacyBatch.exists)
        let personal = items.cells.containing(.staticText,identifier: "QA personal trash").firstMatch
        let metadata = personal.staticTexts.matching(NSPredicate(format: "label CONTAINS[c] %@", "My documents · ")).firstMatch
        XCTAssertTrue(metadata.exists)
        XCTAssertTrue(metadata.label.contains("days"))
        let personalFolder = items.cells.containing(.staticText,identifier: "QA personal folder").firstMatch
        let sharedFolder = items.cells.containing(.staticText,identifier: "QA shared trash").firstMatch
        let sharedDocument = items.cells.containing(.staticText,identifier: "QA shared document").firstMatch
        XCTAssertEqual(personal.value as? String,"Document")
        XCTAssertEqual(personalFolder.value as? String,"Folder")
        XCTAssertEqual(sharedDocument.value as? String,"Document, Shared")
        XCTAssertEqual(sharedFolder.value as? String,"Folder, Shared folder")
        for (cell,title) in [(personalFolder,"QA personal folder"),(sharedDocument,"QA shared document"),(sharedFolder,"QA shared trash")] {
            XCTAssertEqual(cell.staticTexts[title].frame.minX,personal.staticTexts["QA personal trash"].frame.minX,accuracy: 1)
            let subtitle = cell.staticTexts.matching(NSPredicate(format: "label CONTAINS %@", "days")).firstMatch
            XCTAssertTrue(subtitle.exists)
            XCTAssertEqual(subtitle.frame.minX,metadata.frame.minX,accuracy: 1)
        }
        let retention = app.staticTexts["fei.notes.trash-retention"]
        XCTAssertTrue(retention.exists)
        XCTAssertEqual(personal.frame.minX,retention.frame.minX,accuracy: 24)
        XCTAssertLessThanOrEqual(personal.frame.maxX,app.frame.maxX + 1)
        capture("trash-mixed-before")
        let restore = app.buttons["Restore"].firstMatch
        let purge = app.buttons["Delete permanently"].firstMatch
        personal.tap()
        XCTAssertFalse(restore.exists)
        XCTAssertFalse(purge.exists)
        personal.press(forDuration: 1)
        XCTAssertTrue(restore.waitForExistence(timeout: 5))
        XCTAssertTrue(purge.exists)
        capture("trash-row-menu")
        purge.tap()
        let alert = app.alerts["fei.confirm"]
        XCTAssertTrue(alert.waitForExistence(timeout: 5))
        alert.buttons["fei.confirm.cancel"].firstMatch.tap()
        XCTAssertTrue(personal.exists)
        legacySingle.press(forDuration: 1)
        XCTAssertTrue(purge.waitForExistence(timeout: 5))
        purge.tap()
        XCTAssertTrue(alert.waitForExistence(timeout: 5))
        alert.buttons["fei.confirm.ok"].firstMatch.tap()
        let removed = XCTNSPredicateExpectation(predicate: NSPredicate(format: "exists == false"),object: legacySingle)
        XCTAssertEqual(XCTWaiter.wait(for: [removed],timeout: 10),.completed)
        XCTAssertTrue(legacyBatch.exists)
        sharedFolder.press(forDuration: 1)
        XCTAssertTrue(app.buttons["Restore folder"].firstMatch.waitForExistence(timeout: 5))
        let purgeFolder = app.buttons["Delete folder permanently"].firstMatch
        XCTAssertTrue(purgeFolder.exists)
        capture("trash-folder-menu")
        purgeFolder.tap()
        XCTAssertTrue(alert.staticTexts["Permanently delete folder?"].waitForExistence(timeout: 5))
        alert.buttons["fei.confirm.cancel"].firstMatch.tap()
        XCTAssertTrue(sharedFolder.exists)
        empty.tap()
        XCTAssertTrue(alert.waitForExistence(timeout: 5))
        alert.buttons["fei.confirm.cancel"].firstMatch.tap()
        XCTAssertTrue(empty.isEnabled)
        empty.tap()
        XCTAssertTrue(alert.waitForExistence(timeout: 5))
        capture("trash-mixed-confirm")
        alert.buttons["fei.confirm.ok"].firstMatch.tap()
        XCTAssertTrue(app.staticTexts["Trash is empty"].waitForExistence(timeout: 12))
        XCTAssertFalse(empty.isEnabled)
        capture("trash-mixed-cleared")
        app.terminate()
        app.launch()
        XCTAssertTrue(settings.waitForExistence(timeout: 15))
        settings.tap()
        XCTAssertTrue(trashEntry.waitForExistence(timeout: 5))
        trashEntry.tap()
        XCTAssertTrue(app.staticTexts["Trash is empty"].waitForExistence(timeout: 5))
        XCTAssertFalse(empty.isEnabled)
    }

    func testTrashUsesSharedRootRowMenu() {
        continueAfterFailure = false
        let app = XCUIApplication()
        XCUIDevice.shared.orientation = UIDevice.current.userInterfaceIdiom == .pad ? .landscapeLeft : .portrait
        app.launchArguments += ["-AppleLanguages", "(en)"]
        app.launchEnvironment["FEI_NOTES_MODE"] = "cloud"
        app.launchEnvironment["FEI_NOTES_CLOUD_CHANNEL"] = "44ecb219-6dec-4b51-ae62-bcd575b86813"
        app.launchEnvironment["FEI_QA_SYNC_BADGE"] = "1"
        app.launch()

        let tabs = app.segmentedControls["fei.notes.tabs"]
        let shared = tabs.buttons["Shared documents"]
        XCTAssertTrue(shared.waitForExistence(timeout: 15))
        shared.tap()
        let title = "QA shared root folder"
        let liveFolder = app.tables["fei.notes.list"].cells.containing(.staticText,identifier: title).firstMatch
        XCTAssertTrue(liveFolder.waitForExistence(timeout: 8))
        let identifier = liveFolder.identifier
        liveFolder.press(forDuration: 1)
        let manage = app.buttons["Folder sharing access"].firstMatch
        XCTAssertTrue(manage.waitForExistence(timeout: 5))
        XCTAssertFalse(app.buttons["Restore folder"].exists)
        capture("active-shared-root-menu")
        app.coordinate(withNormalizedOffset: CGVector(dx: 0.9,dy: 0.92)).tap()

        let settings = app.buttons["fei.settings"]
        XCTAssertTrue(settings.waitForExistence(timeout: 8))
        settings.tap()
        let entry = app.cells["fei.setting.page:Trash"]
        XCTAssertTrue(entry.waitForExistence(timeout: 5))
        entry.tap()
        XCTAssertTrue(app.navigationBars["Trash"].waitForExistence(timeout: 5))
        let trashFolder = app.tables["fei.notes.list"].cells.containing(.staticText,identifier: title)
            .matching(NSPredicate(format: "value == %@", "Folder, Shared folder")).firstMatch
        XCTAssertTrue(trashFolder.waitForExistence(timeout: 5))
        XCTAssertEqual(trashFolder.identifier,identifier)
        XCTAssertEqual(trashFolder.value as? String,"Folder, Shared folder")
        trashFolder.tap()
        let restore = app.buttons["Restore folder"].firstMatch
        let purge = app.buttons["Delete folder permanently"].firstMatch
        XCTAssertFalse(restore.exists)
        trashFolder.press(forDuration: 1)
        XCTAssertTrue(restore.waitForExistence(timeout: 5))
        XCTAssertTrue(purge.exists)
        XCTAssertFalse(manage.exists)
        XCTAssertFalse(app.buttons["Organize in personal folder"].exists)
        capture("trash-shared-root-menu")
        purge.tap()
        let confirm = app.alerts["fei.confirm"]
        XCTAssertTrue(confirm.staticTexts["Permanently delete folder?"].waitForExistence(timeout: 5))
        confirm.buttons["fei.confirm.cancel"].firstMatch.tap()
        XCTAssertTrue(trashFolder.exists)
    }

    func testMoveWithinPersonalLibrary() {
        let (app,pad) = launchMoveFixture()
        checkMoveBetweenFolders(app,pad: pad,prefix: "Move personal",root: "My documents")
    }

    func testMoveWithinSharedFolder() {
        let (app,pad) = launchMoveFixture()
        app.segmentedControls["fei.notes.tabs"].buttons["Shared documents"].tap()
        let share = moveRow("Move shared A",in: app)
        XCTAssertTrue(share.waitForExistence(timeout: 8))
        share.tap()
        checkMoveBetweenFolders(app,pad: pad,prefix: "Move shared A",root: "Move shared A",
                                excluded: ["My documents","Move personal target","Move shared B","Move shared B target"])
    }

    func testSharedFolderReordersDocumentsAndFolders() {
        checkSharedFolderOrder()
    }

    func testSharedFolderReordersInPortraitOnPad() {
        guard UIDevice.current.userInterfaceIdiom == .pad else { return }
        checkSharedFolderOrder(compact: true)
    }

    private func checkSharedFolderOrder(compact: Bool = false) {
        let (app, _) = launchMoveFixture(compact: compact, offline: true)
        func openFolder() {
            app.segmentedControls["fei.notes.tabs"].buttons["Shared documents"].tap()
            for title in ["Move shared A", "Move shared A source"] {
                let row = moveRow(title,in: app)
                XCTAssertTrue(row.waitForExistence(timeout: 8), title)
                row.tap()
            }
        }
        openFolder()
        let document = moveRow("Move shared A moving document",in: app)
        let folder = moveRow("Move shared A moving folder",in: app)
        XCTAssertTrue(document.waitForExistence(timeout: 8))
        XCTAssertTrue(folder.exists)
        let more = app.buttons["fei.library-actions"].firstMatch
        more.tap()
        let reorder = app.buttons["Reorder"].firstMatch
        XCTAssertTrue(reorder.waitForExistence(timeout: 5))
        capture("shared-order-menu")
        reorder.tap()
        let done = app.buttons["fei.notes.reorder.done"]
        XCTAssertTrue(done.waitForExistence(timeout: 5))
        capture("shared-order-handles")
        let upper = document.frame.midY < folder.frame.midY ? document : folder
        let lower = document.frame.midY < folder.frame.midY ? folder : document
        let handle = lower.descendants(matching: .any).matching(NSPredicate(format: "label BEGINSWITH %@", "Reorder")).firstMatch
        XCTAssertTrue(handle.exists)
        handle.coordinate(withNormalizedOffset: CGVector(dx: 0.5,dy: 0.5))
            .press(forDuration: 0.25,thenDragTo: upper.coordinate(withNormalizedOffset: CGVector(dx: 0.94,dy: 0.05)))
        XCTAssertEqual(XCTWaiter.wait(for: [XCTNSPredicateExpectation(predicate: NSPredicate { _,_ in
            lower.exists && upper.exists && lower.frame.midY < upper.frame.midY
        },object: nil)],timeout: 8),.completed)
        capture("shared-order-dragged")
        done.tap()
        app.terminate()
        app.launch()
        XCTAssertTrue(app.segmentedControls["fei.notes.tabs"].waitForExistence(timeout: 15))
        openFolder()
        XCTAssertTrue(lower.waitForExistence(timeout: 8))
        XCTAssertLessThan(lower.frame.midY,upper.frame.midY)
    }

    func testMovePersonalIntoOwnedSharedFolder() {
        let (app,pad) = launchMoveFixture()
        let source = "Move personal source"
        let items = ["Move personal moving document","Move personal moving folder"]
        moveRow(source,in: app).tap()
        for name in items {
            chooseMove(app,item: name,destination: "Move shared A target")
            XCTAssertTrue(app.navigationBars["Move into shared folder"].waitForExistence(timeout: 5))
            XCTAssertTrue(app.staticTexts.containing(NSPredicate(format: "label CONTAINS %@",
                "All members can access")).firstMatch.exists)
            let confirm = app.cells["fei.setting.share-submit"]
            XCTAssertTrue(confirm.waitForExistence(timeout: 5))
            capture("move-and-share-confirmation")
            confirm.tap()
            let removed = XCTNSPredicateExpectation(predicate: NSPredicate(format: "exists == false"),object: moveRow(name,in: app))
            XCTAssertEqual(XCTWaiter.wait(for: [removed],timeout: 12),.completed)
        }
        exitMoveFolder(app,pad: pad,title: source)
        app.segmentedControls["fei.notes.tabs"].buttons["Shared documents"].tap()
        moveRow("Move shared A",in: app).tap()
        let target = moveRow("Move shared A target",in: app)
        XCTAssertTrue(target.waitForExistence(timeout: 8))
        target.tap()
        for name in items {
            XCTAssertTrue(moveRow(name,in: app).waitForExistence(timeout: 6),name)
        }
        moveRow(items[1],in: app).tap()
        XCTAssertTrue(moveRow("Move personal child",in: app).waitForExistence(timeout: 6))
        capture("move-personal-into-shared")
    }

    private func launchMoveFixture(compact: Bool = false, offline: Bool = false) -> (XCUIApplication,Bool) {
        continueAfterFailure = false
        let pad = UIDevice.current.userInterfaceIdiom == .pad
        XCUIDevice.shared.orientation = pad && !compact ? .landscapeLeft : .portrait
        let app = XCUIApplication()
        app.launchArguments += ["-AppleLanguages", "(en)"]
        app.launchEnvironment["FEI_NOTES_MODE"] = "cloud"
        app.launchEnvironment["FEI_NOTES_CLOUD_CHANNEL"] = "44ecb219-6dec-4b51-ae62-bcd575b86813"
        app.launchEnvironment["FEI_QA_SYNC_BADGE"] = "1"
        if offline {
            app.launchEnvironment["FEI_QA_OFFLINE_INVITATION"] = "1"
            app.launchEnvironment["FEI_QA_PERSONAL_SYNC_STATE"] = "ready"
        }
        app.launch()
        XCTAssertTrue(app.segmentedControls["fei.notes.tabs"].waitForExistence(timeout: 15))
        return (app,pad)
    }

    private func moveRow(_ title: String,in app: XCUIApplication) -> XCUIElement {
        app.tables.matching(identifier: "fei.notes.list").cells.containing(.staticText,identifier: title).firstMatch
    }

    private func chooseMove(_ app: XCUIApplication,item: String,destination: String,excluded: [String] = []) {
        let row = moveRow(item,in: app)
        XCTAssertTrue(row.waitForExistence(timeout: 6),item)
        row.press(forDuration: 1)
        let move = app.buttons["Move to"].firstMatch
        XCTAssertTrue(move.waitForExistence(timeout: 6))
        move.tap()
        XCTAssertTrue(app.navigationBars["Move to"].waitForExistence(timeout: 5))
        let picker = app.tables.matching(NSPredicate(format: "identifier != %@", "fei.notes.list")).firstMatch
        for title in excluded + [item] {
            XCTAssertFalse(picker.cells.containing(.staticText,identifier: title).firstMatch.exists,title)
        }
        let target = picker.cells.containing(.staticText,identifier: destination).firstMatch
        XCTAssertTrue(target.waitForExistence(timeout: 5),destination)
        capture("move-destinations")
        target.tap()
    }

    private func exitMoveFolder(_ app: XCUIApplication,pad: Bool,title: String) {
        let back = pad ? app.buttons["fei.browser.parent"] : app.navigationBars[title].buttons.firstMatch
        XCTAssertTrue(back.waitForExistence(timeout: 8),title)
        back.tap()
    }

    private func checkMoveBetweenFolders(_ app: XCUIApplication,pad: Bool,prefix: String,root: String,excluded: [String] = []) {
        let source = prefix + " source"
        let target = prefix + " target"
        let names = [prefix + " moving document",prefix + " moving folder"]
        let sourceRow = moveRow(source,in: app)
        XCTAssertTrue(sourceRow.waitForExistence(timeout: 8))
        sourceRow.tap()
        for name in names { chooseMove(app,item: name,destination: target,excluded: excluded) }
        exitMoveFolder(app,pad: pad,title: source)
        let targetRow = moveRow(target,in: app)
        XCTAssertTrue(targetRow.waitForExistence(timeout: 8))
        targetRow.tap()
        for name in names { chooseMove(app,item: name,destination: root,excluded: excluded) }
        exitMoveFolder(app,pad: pad,title: target)
        for name in names {
            XCTAssertTrue(moveRow(name,in: app).waitForExistence(timeout: 6),name)
        }
        capture("move-items-at-root")
        for name in names { chooseMove(app,item: name,destination: source,excluded: excluded) }
        sourceRow.tap()
        for name in names {
            XCTAssertTrue(moveRow(name,in: app).waitForExistence(timeout: 6),name)
        }
        moveRow(names[1],in: app).tap()
        XCTAssertTrue(moveRow(prefix + " child",in: app).waitForExistence(timeout: 6))
        capture("move-subtree-back-in-source")
    }

    func testSettingsLibraryAndTrashNavigation() {
        continueAfterFailure = false
        let app = XCUIApplication()
        let pad = UIDevice.current.userInterfaceIdiom == .pad
        XCUIDevice.shared.orientation = pad ? .landscapeLeft : .portrait
        app.launchArguments += ["-AppleLanguages", "(en)"]
        app.launchEnvironment["FEI_NOTES_CLOUD_CHANNEL"] = "44ecb219-6dec-4b51-ae62-bcd575b86813"
        app.launchEnvironment["FEI_QA_SYNC_BADGE"] = "1"
        app.launchEnvironment["FEI_QA_PERSONAL_SYNC_STATE"] = "ready"
        for (mode, libraryLabel) in [("cloud", "iCloud"), ("local", "On this device")] {
            app.launchEnvironment["FEI_NOTES_MODE"] = mode
            app.launch()
            let settings = app.buttons["fei.settings"]
            XCTAssertTrue(settings.waitForExistence(timeout: 12))
            settings.tap()
            let table = app.tables["fei.settings.list"]
            XCTAssertTrue(table.waitForExistence(timeout: 5))
            let rows = ["General", "Appearance", "Shortcuts", "Library", "Cloud", "Trash", "About"]
            let actual = table.cells.allElementsBoundByIndex.map(\.identifier)
            XCTAssertEqual(actual, rows.map { "fei.setting.page:" + $0 })
            let settingsTitles = app.navigationBars["Settings"].staticTexts.matching(identifier: "Settings").allElementsBoundByIndex.filter { $0.isHittable }
            XCTAssertFalse(settingsTitles.isEmpty)
            XCTAssertTrue(settingsTitles.allSatisfy { $0.frame.height < 32 }, "Settings uses an inline navigation title")
            if pad { XCTAssertTrue(app.tables["fei.notes.list"].firstMatch.isHittable) }
            capture("settings-root-" + mode)

            table.cells["fei.setting.page:Library"].tap()
            let library = app.navigationBars["Library"]
            XCTAssertTrue(library.waitForExistence(timeout: 5))
            let active = table.cells["fei.setting.library-info"]
            let title = active.staticTexts["Current library"]
            let value = active.staticTexts[libraryLabel]
            XCTAssertTrue(value.waitForExistence(timeout: 5))
            XCTAssertGreaterThan(value.frame.minX, title.frame.maxX)
            XCTAssertEqual(title.frame.midY, value.frame.midY, accuracy: 3)
            XCTAssertFalse(table.cells["fei.setting.page:Trash"].exists)
            XCTAssertFalse(table.cells["fei.setting.open-trash"].exists)
            XCTAssertTrue(table.cells["fei.setting.backup"].exists)
            let history = table.cells["fei.setting.history-limit"]
            assertSettingRow(history,title: "Version history",value: "20",detail: "Snapshots kept for each document")
            let historyTitle = history.staticTexts["Version history"]
            let historyDetail = history.staticTexts.matching(NSPredicate(format: "label BEGINSWITH %@", "Snapshots kept")).firstMatch
            let migration = table.cells["fei.setting.import-legacy"]
            let migrationTitle = migration.staticTexts["Migrate from Quiet.md"]
            let migrationDetail = migration.staticTexts["Copy documents into your current library."]
            XCTAssertTrue(migrationTitle.exists && migrationDetail.exists)
            XCTAssertEqual(historyDetail.frame.minY - historyTitle.frame.maxY,
                           migrationDetail.frame.minY - migrationTitle.frame.maxY,accuracy: 2)
            capture("documents-library-" + mode)
            library.buttons.firstMatch.tap()
            if mode == "cloud" {
                let appearance = table.cells["fei.setting.page:Appearance"]
                XCTAssertTrue(appearance.waitForExistence(timeout: 5))
                appearance.tap()
                XCTAssertFalse(table.cells["fei.setting.motion"].exists)
                XCTAssertTrue(table.cells["fei.setting.theme"].exists)
                capture("settings-appearance")
                app.navigationBars["Appearance"].buttons.firstMatch.tap()

                let general = table.cells["fei.setting.page:General"]
                XCTAssertTrue(general.waitForExistence(timeout: 5))
                general.tap()
                XCTAssertFalse(table.cells["fei.setting.search-engine"].exists)
                XCTAssertTrue(table.cells["fei.setting.reminders"].exists)
                let images = table.cells["fei.setting.remote-images"]
                XCTAssertTrue(images.switches["fei.setting.switch.remote-images"].exists)
                XCTAssertTrue(images.staticTexts.matching(NSPredicate(format: "label BEGINSWITH %@", "When off")).firstMatch.exists)
                capture("settings-general-value-detail")
                app.navigationBars["General"].buttons.firstMatch.tap()

                table.cells["fei.setting.page:Shortcuts"].tap()
                XCTAssertTrue(app.navigationBars["Shortcuts"].waitForExistence(timeout: 5))
                let newDocument = table.cells["fei.setting.shortcut:workspace::NewDoc"]
                XCTAssertTrue(newDocument.staticTexts.matching(NSPredicate(format: "label CONTAINS %@", "⌘")).firstMatch.exists)
                XCTAssertTrue(table.cells["fei.setting.shortcut:workspace::CloseWindow"].staticTexts["Go back"].exists)
                for action in ["QuickNew","ToggleSidebar","ToggleFullScreen"] {
                    XCTAssertFalse(table.cells.matching(NSPredicate(format: "identifier ENDSWITH %@", "::" + action)).firstMatch.exists)
                }
                capture("settings-shortcuts")
                app.navigationBars["Shortcuts"].buttons.firstMatch.tap()
            }

            let entry = table.cells["fei.setting.page:Trash"]
            XCTAssertTrue(entry.waitForExistence(timeout: 5))
            entry.tap()
            let trash = app.navigationBars["Trash"]
            XCTAssertTrue(trash.waitForExistence(timeout: 5))
            let rule = app.staticTexts["fei.notes.trash-retention"]
            XCTAssertTrue(rule.waitForExistence(timeout: 5))
            XCTAssertTrue(rule.label.contains("30 days"))
            let trashTitles = trash.staticTexts.matching(identifier: "Trash").allElementsBoundByIndex.filter { $0.isHittable }
            XCTAssertFalse(trashTitles.isEmpty)
            XCTAssertTrue(trashTitles.allSatisfy { $0.frame.height < 32 }, "Trash uses an inline navigation title")
            capture("trash-" + mode)
            trash.buttons.firstMatch.tap()
            XCTAssertTrue(entry.waitForExistence(timeout: 5))
            app.terminate()
        }
    }

    private func assertSettingRow(_ row:XCUIElement,title:String,value:String,detail:String?=nil,file:StaticString=#filePath,line:UInt=#line) {
        XCTAssertTrue(row.waitForExistence(timeout: 5),file: file,line: line)
        let label = row.staticTexts[title]
        let summary = row.staticTexts[value]
        XCTAssertTrue(label.exists && summary.exists,file: file,line: line)
        XCTAssertGreaterThan(summary.frame.minX,label.frame.minX,file: file,line: line)
        XCTAssertEqual(summary.frame.midY,label.frame.midY,accuracy: 3,file: file,line: line)
        XCTAssertGreaterThanOrEqual(summary.frame.width,CGFloat(value.count) * 5,file: file,line: line)
        if let detail {
            let description = row.staticTexts.matching(NSPredicate(format: "label BEGINSWITH %@",detail)).firstMatch
            XCTAssertTrue(description.exists,file: file,line: line)
            XCTAssertGreaterThanOrEqual(description.frame.minY,label.frame.maxY-3,file: file,line: line)
            XCTAssertEqual(description.frame.minX,label.frame.minX,accuracy: 3,file: file,line: line)
        }
    }

    func testPersonalICloudStates() {
        continueAfterFailure = false
        let app = XCUIApplication()
        let pad = UIDevice.current.userInterfaceIdiom == .pad
        XCUIDevice.shared.orientation = pad ? .landscapeLeft : .portrait
        app.launchArguments += ["-AppleLanguages", "(en)"]
        app.launchEnvironment["FEI_NOTES_CLOUD_CHANNEL"] = "44ecb219-6dec-4b51-ae62-bcd575b86813"
        app.launchEnvironment["FEI_QA_SYNC_BADGE"] = "1"
        for (state, label) in [
            ("off", "Sync is off"),
            ("ready", "Up to date"),
            ("offline", "Waiting to retry automatically"),
            ("verifying", "Checking the iCloud account"),
            ("signed-out", "Signed out of iCloud"),
            ("changed", "iCloud account changed"),
            ("no-access", "Sync paused"),
        ] {
            app.launchEnvironment["FEI_NOTES_MODE"] = state == "off" ? "local" : "cloud"
            app.launchEnvironment["FEI_QA_PERSONAL_SYNC_STATE"] = state
            app.launch()
            let gear = app.buttons["fei.settings"]
            XCTAssertTrue(gear.waitForExistence(timeout: 12))
            gear.tap()
            let library = app.cells["fei.setting.page:Library"].firstMatch
            let cloud = app.cells["fei.setting.page:Cloud"].firstMatch
            XCTAssertTrue(library.waitForExistence(timeout: 5))
            XCTAssertTrue(cloud.waitForExistence(timeout: 5))
            XCTAssertLessThan(library.frame.minY,cloud.frame.minY)
            XCTAssertEqual(library.frame.minX,cloud.frame.minX,accuracy: 2)
            if state == "off" || state == "ready" {
                let title = cloud.staticTexts["iCloud"]
                let summary = cloud.staticTexts[state == "off" ? "Off" : "On"]
                XCTAssertTrue(title.exists && summary.exists)
                XCTAssertGreaterThan(summary.frame.minX,title.frame.maxX)
                XCTAssertEqual(summary.frame.midY,title.frame.midY,accuracy: 3)
                if pad { XCTAssertTrue(app.tables["fei.notes.list"].isHittable) }
                capture("settings-index-" + state)
            }
            if state == "ready" {
                library.tap()
                XCTAssertTrue(app.cells["fei.setting.library-info"].waitForExistence(timeout: 5))
                XCTAssertFalse(app.cells["fei.setting.library-switch"].exists)
                XCTAssertFalse(app.cells["fei.setting.cloud-disable"].exists)
                capture("documents-library")
                XCTAssertFalse(cloud.exists)
                app.navigationBars["Library"].buttons.firstMatch.tap()
                XCTAssertTrue(cloud.waitForExistence(timeout: 5))
            }
            cloud.tap()
            let status = app.cells["fei.setting." + (state == "off" ? "cloud-off" : "sync-state")]
            XCTAssertTrue(status.staticTexts[label].waitForExistence(timeout: 5), app.debugDescription)
            if state == "off" {
                XCTAssertTrue(app.cells["fei.setting.cloud-enable"].exists)
                XCTAssertFalse(app.cells["fei.setting.cloud-disable"].exists)
            } else {
                for id in ["sync-last-success", "sync-account", "sync-access"] {
                    XCTAssertTrue(app.cells["fei.setting." + id].exists, id)
                }
            }
            capture("icloud-" + state)
            let syncNow = app.cells["fei.setting.sync-retry"]
            switch state {
            case "ready": XCTAssertTrue(syncNow.exists && syncNow.isEnabled)
            case "offline":
                XCTAssertTrue(app.cells["fei.setting.sync-uploads"].staticTexts["2"].exists)
                XCTAssertTrue(syncNow.exists)
                XCTAssertFalse(syncNow.isEnabled)
            case "signed-out", "changed":
                XCTAssertTrue(app.cells["fei.setting.account-settings"].exists)
                XCTAssertFalse(syncNow.exists)
            case "no-access":
                XCTAssertTrue(app.cells["fei.setting.page:CloudAccess"].staticTexts["View Plans"].exists)
                XCTAssertFalse(syncNow.exists)
            default: XCTAssertFalse(syncNow.exists)
            }
            app.terminate()
        }
    }

    func testTurnOffICloudSyncFromSettings() {
        continueAfterFailure = false
        let app = XCUIApplication()
        XCUIDevice.shared.orientation = UIDevice.current.userInterfaceIdiom == .pad ? .landscapeLeft : .portrait
        app.launchArguments += ["-AppleLanguages", "(en)"]
        app.launchEnvironment["FEI_NOTES_MODE"] = "cloud"
        app.launchEnvironment["FEI_NOTES_CLOUD_CHANNEL"] = "44ecb219-6dec-4b51-ae62-bcd575b86813"
        app.launchEnvironment["FEI_QA_SYNC_BADGE"] = "1"
        app.launchEnvironment["FEI_QA_PERSONAL_SYNC_STATE"] = "ready"
        app.launch()
        let gear = app.buttons["fei.settings"]
        let cloud = app.cells["fei.setting.page:Cloud"]
        let settings = app.tables["fei.settings.list"]
        let off = app.cells["fei.setting.cloud-disable"]
        XCTAssertTrue(gear.waitForExistence(timeout: 12))
        gear.tap()
        XCTAssertTrue(cloud.waitForExistence(timeout: 5))
        cloud.tap()
        XCTAssertTrue(app.navigationBars["iCloud"].waitForExistence(timeout: 5))
        for _ in 0..<5 { if off.isHittable { break }; settings.swipeUp() }
        XCTAssertTrue(off.isHittable)
        capture("icloud-disable")
        off.tap()
        let alert = app.alerts["Turn off iCloud sync?"]
        XCTAssertTrue(alert.waitForExistence(timeout: 5))
        XCTAssertTrue(alert.staticTexts.containing(NSPredicate(format: "label CONTAINS %@", "pending changes")).firstMatch.exists)
        capture("icloud-disable-confirmation")
        alert.buttons["fei.confirm.cancel"].firstMatch.tap()
        XCTAssertTrue(off.isHittable)
        off.tap()
        XCTAssertTrue(alert.waitForExistence(timeout: 5))
        alert.buttons["fei.confirm.ok"].firstMatch.tap()
        let enable = app.cells["fei.setting.cloud-enable"]
        if !enable.waitForExistence(timeout: 5) {
            XCTAssertTrue(gear.waitForExistence(timeout: 8))
            if !cloud.exists { gear.tap() }
            XCTAssertTrue(cloud.waitForExistence(timeout: 5))
            cloud.tap()
        }
        XCTAssertTrue(enable.waitForExistence(timeout: 5))
        XCTAssertFalse(off.exists)
        capture("icloud-disabled-after-switch")
    }

    func testSharedSyncReturnsFromICloud() {
        continueAfterFailure = false
        let app = XCUIApplication()
        XCUIDevice.shared.orientation = UIDevice.current.userInterfaceIdiom == .pad ? .landscapeLeft : .portrait
        app.launchArguments += ["-AppleLanguages", "(en)"]
        app.launchEnvironment["FEI_NOTES_MODE"] = "cloud"
        app.launchEnvironment["FEI_NOTES_CLOUD_CHANNEL"] = "44ecb219-6dec-4b51-ae62-bcd575b86813"
        app.launchEnvironment["FEI_QA_SYNC_BADGE"] = "1"
        app.launchEnvironment["FEI_QA_SHARED_SYNC_SETTINGS"] = "1"
        app.launch()

        let shared = app.segmentedControls["fei.notes.tabs"].buttons["Shared documents"]
        XCTAssertTrue(shared.waitForExistence(timeout: 12))
        shared.tap()
        let note = app.tables["fei.notes.list"].staticTexts["FEI_SHARED_READONLY"].firstMatch
        XCTAssertTrue(note.waitForExistence(timeout: 8))
        note.tap()
        let more = app.buttons["fei.more"]
        XCTAssertTrue(more.waitForExistence(timeout: 8))
        more.tap()
        let sync = app.descendants(matching: .any).matching(NSPredicate(format: "label == %@", "Sync status")).firstMatch
        XCTAssertTrue(sync.waitForExistence(timeout: 5))
        sync.tap()
        let action = app.descendants(matching: .any)["fei.shared-sync.action"].firstMatch
        XCTAssertTrue(action.waitForExistence(timeout: 5))
        XCTAssertEqual(action.label,"Open iCloud Settings")
        capture("shared-sync-before-icloud")
        action.tap()
        let cloud = app.navigationBars["iCloud"]
        XCTAssertTrue(cloud.waitForExistence(timeout: 5))
        XCTAssertFalse(app.cells["fei.setting.page:Cloud"].exists)
        capture("icloud-from-shared-sync")
        cloud.buttons.firstMatch.tap()
        XCTAssertTrue(action.waitForExistence(timeout: 7))
        capture("shared-sync-returned")
        app.buttons["fei.shared-sync.done"].tap()
        XCTAssertTrue(app.navigationBars["FEI_SHARED_READONLY"].waitForExistence(timeout: 5))
    }
}
