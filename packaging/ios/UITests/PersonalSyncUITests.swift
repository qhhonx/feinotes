import XCTest

final class PersonalSyncUITests: XCTestCase {
    private func capture(_ name: String) {
        let screenshot = XCTAttachment(screenshot: XCUIScreen.main.screenshot())
        screenshot.name = name
        screenshot.lifetime = .keepAlways
        add(screenshot)
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
