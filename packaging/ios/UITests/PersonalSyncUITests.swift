import XCTest

final class PersonalSyncUITests: XCTestCase {
    private func capture(_ name: String) {
        let screenshot = XCTAttachment(screenshot: XCUIScreen.main.screenshot())
        screenshot.name = name
        screenshot.lifetime = .keepAlways
        add(screenshot)
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
        let action = app.buttons["fei.shared-sync.action"]
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
