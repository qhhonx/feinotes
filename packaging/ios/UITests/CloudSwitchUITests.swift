import XCTest

final class CloudSwitchUITests: XCTestCase {
    private func capture(_ name: String) {
        let image = XCTAttachment(screenshot: XCUIScreen.main.screenshot())
        image.name = name
        image.lifetime = .keepAlways
        add(image)
    }

    private func createNote(_ app:XCUIApplication) {
        let button = app.buttons.matching(identifier: "fei.new").allElementsBoundByIndex.first { $0.isHittable }
        XCTAssertNotNil(button,app.debugDescription)
        button?.tap()
        XCTAssertTrue(app.buttons["fei.more"].waitForExistence(timeout: 8),app.debugDescription)
    }

    func testSilentCloudStartupKeepsHomeUsable() {
        continueAfterFailure = false
        let app = XCUIApplication()
        app.launchEnvironment["FEI_NOTES_CLOUD_CHANNEL"] = UUID().uuidString
        app.launchEnvironment["FEI_QA_SLOW_SIGN_OUT"] = "1"
        XCUIDevice.shared.orientation = UIDevice.current.userInterfaceIdiom == .pad ? .landscapeLeft : .portrait
        app.launch()
        XCTAssertTrue(app.buttons["fei.settings"].waitForExistence(timeout: 6))
        XCTAssertFalse(app.staticTexts["正在打开 iCloud 文档库…"].exists)
        createNote(app)
        XCTAssertFalse(app.alerts.firstMatch.exists)
    }

    func testFailedCloudSwitchKeepsSettingsResponsive() {
        continueAfterFailure = false
        let app = XCUIApplication()
        app.launchArguments += ["-AppleLanguages", "(en)"]
        app.launchEnvironment["FEI_NOTES_MODE"] = "local"
        app.launchEnvironment["FEI_QA_CLOUD_SWITCH_FAILURE"] = "1"
        XCUIDevice.shared.orientation = UIDevice.current.userInterfaceIdiom == .pad ? .landscapeLeft : .portrait
        app.launch()
        XCTAssertTrue(app.buttons["fei.settings"].waitForExistence(timeout: 10))
        app.buttons["fei.settings"].tap()
        let settings = app.tables["fei.settings.list"]
        let library = app.cells["fei.setting.page:Library"]
        for _ in 0..<6 { if library.isHittable { break }; settings.swipeUp() }
        XCTAssertTrue(library.isHittable)
        library.tap()
        let cloud = app.cells["fei.setting.page:Cloud"]
        capture("library-settings")
        app.navigationBars["Library"].buttons.firstMatch.tap()
        XCTAssertTrue(cloud.waitForExistence(timeout: 5))
        cloud.tap()
        let toggle = app.cells["fei.setting.cloud-enable"]
        for _ in 0..<6 { if toggle.isHittable { break }; settings.swipeUp() }
        XCTAssertTrue(toggle.isHittable)
        capture("icloud-off")
        toggle.tap()
        let confirm = app.buttons["fei.confirm.ok"]
        XCTAssertTrue(confirm.waitForExistence(timeout: 5))
        XCTAssertEqual(app.switches["fei.confirm.choice"].value as? String, "0")
        capture("icloud-enable-confirmation")
        confirm.tap()
        let message = app.staticTexts.matching(identifier: "fei.settings.message").firstMatch
        XCTAssertTrue(message.waitForExistence(timeout: 5))
        XCTAssertTrue(message.label.contains("Cloud switch QA failure"),message.label)
        XCTAssertTrue(toggle.isEnabled)
        func back(_ titles: [String]) {
            let bar = app.navigationBars.matching(NSPredicate(format: "identifier IN %@",titles)).firstMatch
            XCTAssertTrue(bar.waitForExistence(timeout: 5),app.debugDescription)
            bar.buttons.firstMatch.tap()
        }
        if UIDevice.current.userInterfaceIdiom == .pad {
            let gear = app.buttons["fei.settings"]
            if !gear.isHittable { app.buttons["fei.ipad.focus"].tap() }
            XCTAssertTrue(gear.waitForExistence(timeout: 5),app.debugDescription)
            gear.tap()
        } else {
            back(["iCloud"])
            XCTAssertTrue(cloud.waitForExistence(timeout: 5))
            if app.buttons["fei.settings.close"].exists { app.buttons["fei.settings.close"].tap() }
            else { back(["Settings","设置"]) }
        }
        XCTAssertTrue(app.tables["fei.notes.list"].waitForExistence(timeout: 5))
        createNote(app)
        XCTAssertFalse(app.alerts.firstMatch.exists)
    }
}
