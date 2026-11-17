import XCTest

final class ProductStartupUITests: XCTestCase {
    func testHomeAndEditorAreUsableDuringSlowCloudStartup() {
        continueAfterFailure = false
        let app = XCUIApplication()
        app.launchEnvironment["FEI_NOTES_MODE"] = "cloud"
        app.launchEnvironment["FEI_NOTES_CLOUD_CHANNEL"] = UUID().uuidString
        app.launchEnvironment["FEI_QA_SLOW_SIGN_OUT"] = "1"
        let start = ProcessInfo.processInfo.systemUptime
        app.launch()
        XCTAssertTrue(app.tables["fei.notes.list"].waitForExistence(timeout: 6))
        XCTAssertFalse(app.staticTexts["正在打开 iCloud 文档库…"].exists)
        app.buttons["fei.new"].tap()
        XCTAssertTrue(app.buttons["fei.more"].waitForExistence(timeout: 3))
        XCTAssertLessThan(ProcessInfo.processInfo.systemUptime - start, 15)
        XCTAssertFalse(app.alerts.firstMatch.exists)
    }

    func testProductNamespaceOpensLocalLibraryAndRelaunches() {
        continueAfterFailure = false
        var app = XCUIApplication()
        app.launchEnvironment["FEI_NOTES_MODE"] = "local"
        app.launch()
        XCTAssertTrue(app.buttons["fei.settings"].waitForExistence(timeout: 20),"The signed configuration must pass the Swift/Rust library configuration check")
        XCTAssertFalse(app.staticTexts["文档库配置无效。"].exists)
        app.terminate(); app = XCUIApplication(); app.launchEnvironment["FEI_NOTES_MODE"] = "local"; app.launch()
        XCTAssertTrue(app.tables["fei.notes.list"].waitForExistence(timeout: 15))
        let image = XCTAttachment(screenshot: XCUIScreen.main.screenshot())
        image.name = "product40-native-library-startup"; image.lifetime = .keepAlways; add(image)
    }
}
