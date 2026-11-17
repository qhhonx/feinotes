import XCTest

final class LinkUITests: XCTestCase {
    private func shot(_ name: String) {
        let attachment = XCTAttachment(screenshot: XCUIScreen.main.screenshot())
        attachment.name = name
        attachment.lifetime = .keepAlways
        add(attachment)
    }
    private func gone(_ element: XCUIElement) {
        XCTAssertTrue(XCTWaiter.wait(for: [XCTNSPredicateExpectation(predicate: NSPredicate(format: "exists == false"),object: element)],timeout: 5) == .completed)
    }

    func testNativeLinkValidationCancelRemovalAndUndo() {
        continueAfterFailure = false
        let stamp = Int(Date().timeIntervalSince1970)
        print("LINK_TOUCH_STAMP=\(stamp)")
        let app = XCUIApplication()
        app.launchEnvironment["FEI_NOTES_MODE"] = "local"
        app.launch()
        XCTAssertTrue(app.buttons["fei.settings"].waitForExistence(timeout: 15))
        app.buttons["fei.new"].tap()
        XCTAssertTrue(app.buttons["fei.more"].waitForExistence(timeout: 5))
        app.coordinate(withNormalizedOffset: CGVector(dx: 0.3,dy: 0.28)).tap()
        app.typeText("Alpha Bravo \(stamp)")
        func wordLink() {
            app.buttons["fei.more"].tap()
            app.coordinate(withNormalizedOffset: CGVector(dx: 0.05,dy: 0.8)).tap()
            app.coordinate(withNormalizedOffset: CGVector(dx: 0.24,dy: 0.28)).press(forDuration: 0.8)
            XCTAssertTrue(app.menuItems["更多样式…"].waitForExistence(timeout: 5),app.debugDescription)
            app.menuItems["更多样式…"].tap()
            let styles = app.tables["fei.format.controls"]
            XCTAssertTrue(styles.waitForExistence(timeout: 5))
            let link = styles.cells["fei.format.link"]
            if !link.isHittable { styles.swipeUp() }
            XCTAssertTrue(link.isHittable,app.debugDescription)
            link.tap()
            XCTAssertTrue(app.textFields["fei.link.url"].waitForExistence(timeout: 5),app.debugDescription)
        }
        wordLink()
        XCTAssertEqual(app.textFields["fei.link.text"].value as? String,"Bravo")
        XCTAssertFalse(app.textFields["fei.link.text"].isEnabled)
        let url = app.textFields["fei.link.url"]
        url.tap()
        url.typeText("not a valid link")
        app.buttons["fei.link.save"].tap()
        XCTAssertTrue(app.staticTexts["fei.link.error"].waitForExistence(timeout: 5),app.debugDescription)
        shot("link-01-shared-validation-error")
        url.tap()
        url.typeText(String(repeating: XCUIKeyboardKey.delete.rawValue,count: 40))
        let target = "https://example.invalid/fei-link/\(stamp)"
        url.typeText(target)
        app.buttons["fei.link.save"].tap()
        gone(url)
        RunLoop.current.run(until: Date().addingTimeInterval(0.4))
        shot("link-02-shared-editor-link")
        wordLink()
        XCTAssertEqual(url.value as? String,target)
        app.buttons["fei.link.cancel"].tap()
        gone(url)
        wordLink()
        XCTAssertEqual(url.value as? String,target)
        XCTAssertTrue(app.cells["fei.link.remove"].waitForExistence(timeout: 5))
        app.cells["fei.link.remove"].tap()
        gone(url)
        app.buttons["fei.more"].tap()
        app.sheets.buttons["编辑和格式"].tap()
        app.sheets.buttons["撤销"].tap()
        RunLoop.current.run(until: Date().addingTimeInterval(0.4))
        shot("link-03-remove-undo")
        app.navigationBars.buttons.element(boundBy: 0).tap()
        XCTAssertTrue(app.buttons["fei.settings"].waitForExistence(timeout: 5))
        app.buttons["fei.new"].tap()
        XCTAssertTrue(app.buttons["fei.more"].waitForExistence(timeout: 5))
        app.coordinate(withNormalizedOffset: CGVector(dx: 0.3,dy: 0.28)).tap()
        app.buttons["fei.more"].tap()
        app.sheets.buttons["插入内容"].tap()
        let insert = app.tables["fei.insert.controls"]
        XCTAssertTrue(insert.waitForExistence(timeout: 5))
        insert.cells["fei.insert.link"].tap()
        let text = app.textFields["fei.link.text"]
        XCTAssertTrue(text.waitForExistence(timeout: 5))
        XCTAssertTrue(text.isEnabled)
        text.tap()
        text.typeText("FEI_NEW_LINK_\(stamp)")
        url.tap()
        url.typeText("example.invalid/new/\(stamp)")
        app.buttons["fei.link.save"].tap()
        gone(url)
        RunLoop.current.run(until: Date().addingTimeInterval(0.4))
        shot("link-04-new-link-shared-normalization")
        app.navigationBars.buttons.element(boundBy: 0).tap()
        XCTAssertTrue(app.buttons["fei.settings"].waitForExistence(timeout: 5))
        print("LINK_TOUCH_COMPLETED=\(stamp)")
    }
}
