import XCTest

final class ReminderNotificationUITests: XCTestCase {
    private func shot(_ name: String) {
        let attachment = XCTAttachment(screenshot: XCUIScreen.main.screenshot())
        attachment.name = name; attachment.lifetime = .keepAlways; add(attachment)
    }
    private func openPermission(_ app: XCUIApplication) {
        app.buttons["fei.more"].tap()
        app.sheets.buttons["内容设置"].tap()
        let list = app.tables["fei.objects.controls"]
        XCTAssertTrue(list.waitForExistence(timeout: 5))
        XCTAssertEqual(list.cells.count,1)
        list.cells.element(boundBy: 0).tap()
        let form = app.tables["fei.object.form"]
        XCTAssertTrue(form.waitForExistence(timeout: 5))
        let permission = form.cells["fei.object.row.permission"]
        for _ in 0..<4 { if permission.isHittable { break }; form.swipeUp() }
        XCTAssertTrue(permission.isHittable,app.debugDescription)
        permission.tap()
        XCTAssertTrue(app.tables["fei.reminder.notifications"].waitForExistence(timeout: 5),app.debugDescription)
    }
    func testAllowReminderNotifications() {
        continueAfterFailure = false
        let app = XCUIApplication()
        app.launchEnvironment["FEI_NOTES_MODE"] = "local"
        app.launch()
        XCTAssertTrue(app.buttons["fei.settings"].waitForExistence(timeout: 15))
        app.buttons["fei.new"].tap()
        XCTAssertTrue(app.buttons["fei.more"].waitForExistence(timeout: 5))
        app.coordinate(withNormalizedOffset: CGVector(dx: 0.3,dy: 0.28)).tap()
        app.buttons["fei.more"].tap()
        app.sheets.buttons["插入内容"].tap()
        let insert = app.tables["fei.insert.controls"]
        XCTAssertTrue(insert.waitForExistence(timeout: 5))
        insert.cells["fei.insert.insert-group-2"].tap()
        insert.cells.containing(.staticText,identifier: "日期提醒").firstMatch.tap()
        let form = app.tables["fei.object.form"]
        XCTAssertTrue(form.waitForExistence(timeout: 5))
        let permission = form.cells["fei.object.row.permission"]
        for _ in 0..<4 { if permission.isHittable { break }; form.swipeUp() }
        permission.tap()
        XCTAssertTrue(app.tables["fei.reminder.notifications"].waitForExistence(timeout: 5))
        let status = app.cells["fei.reminder.notifications.status"]
        if status.staticTexts["尚未允许"].exists {
            app.cells["fei.reminder.notifications.permission"].tap()
            let springboard = XCUIApplication(bundleIdentifier: "com.apple.springboard")
            XCTAssertTrue(springboard.buttons["允许"].waitForExistence(timeout: 8),springboard.debugDescription)
            springboard.buttons["允许"].tap()
        }
        XCTAssertTrue(status.staticTexts["已允许"].waitForExistence(timeout: 10),app.debugDescription)
        shot("reminders-01-native-permission")
        app.buttons["fei.reminder.notifications.done"].tap()
        app.buttons["fei.object.save"].tap()
        app.navigationBars.buttons["返回"].firstMatch.tap()
        XCTAssertTrue(app.buttons["fei.settings"].waitForExistence(timeout: 5))
        print("REMINDER_PERMISSION_ALLOWED")
    }
    func testSystemDeliveryAndColdOpen() {
        continueAfterFailure = false
        let app = XCUIApplication()
        app.launchEnvironment["FEI_NOTES_MODE"] = "local"
        XCUIDevice.shared.orientation = .portrait
        app.launch()
        XCTAssertTrue(app.buttons["fei.settings"].waitForExistence(timeout: 15))
        app.buttons["fei.library-actions"].tap()
        app.sheets.buttons["导入笔记"].tap()
        let fixture = app.cells.matching(NSPredicate(format: "label CONTAINS %@ AND NOT (label CONTAINS %@)","FEI_NOTIFICATION21","CANCEL")).firstMatch
        if !fixture.waitForExistence(timeout: 2) {
            if app.buttons["浏览"].firstMatch.waitForExistence(timeout: 2) { app.buttons["浏览"].firstMatch.tap() }
            if !fixture.waitForExistence(timeout: 2) {
                let folder = app.cells.matching(NSPredicate(format: "label BEGINSWITH %@","飞记")).firstMatch
                if !folder.waitForExistence(timeout: 2) {
                    let phone = app.cells.matching(NSPredicate(format: "label CONTAINS %@","iPhone")).firstMatch
                    XCTAssertTrue(phone.waitForExistence(timeout: 5)); phone.tap()
                }
                XCTAssertTrue(folder.waitForExistence(timeout: 5)); folder.tap()
            }
        }
        XCTAssertTrue(fixture.waitForExistence(timeout: 8)); fixture.tap()
        let confirm = app.buttons["打开"].firstMatch
        if confirm.waitForExistence(timeout: 2) { confirm.tap() }
        let note = app.navigationBars.matching(NSPredicate(format: "identifier BEGINSWITH %@","FEI_NOTIFY21_")).firstMatch
        XCTAssertTrue(note.waitForExistence(timeout: 10),app.debugDescription)
        let title = note.identifier
        print("REMINDER_DELIVERY_TITLE=\(title)")
        openPermission(app)
        let pending = app.cells.matching(NSPredicate(format: "identifier BEGINSWITH %@","fei.reminder.notifications.pending.")).containing(.staticText,identifier: title).firstMatch
        XCTAssertTrue(pending.waitForExistence(timeout: 15),app.debugDescription)
        shot("reminders-02-system-pending-request")
        app.buttons["fei.reminder.notifications.done"].tap()
        app.buttons["fei.object.save"].tap()
        app.navigationBars.buttons["返回"].firstMatch.tap()
        XCTAssertTrue(app.buttons["fei.settings"].waitForExistence(timeout: 5))
        app.terminate()
        let springboard = XCUIApplication(bundleIdentifier: "com.apple.springboard")
        let notification = springboard.descendants(matching: .any).matching(NSPredicate(format: "label CONTAINS %@",title)).firstMatch
        XCTAssertTrue(notification.waitForExistence(timeout: 110),springboard.debugDescription)
        shot("reminders-03-delivered-while-terminated")
        notification.tap()
        XCTAssertTrue(app.navigationBars[title].waitForExistence(timeout: 15),app.debugDescription)
        openPermission(app)
        let consumed = app.cells.matching(NSPredicate(format: "identifier BEGINSWITH %@","fei.reminder.notifications.pending.")).containing(.staticText,identifier: title).firstMatch
        XCTAssertTrue(XCTWaiter.wait(for: [XCTNSPredicateExpectation(predicate: NSPredicate(format: "exists == false"),object: consumed)],timeout: 10) == .completed,app.debugDescription)
        XCTAssertTrue(app.cells["fei.reminder.notifications.status"].staticTexts["已允许"].exists)
        shot("reminders-04-delivery-and-cold-open")
        app.buttons["fei.reminder.notifications.done"].tap()
        app.buttons["fei.object.save"].tap()
        app.navigationBars.buttons["返回"].firstMatch.tap()
        XCTAssertTrue(app.buttons["fei.settings"].waitForExistence(timeout: 5))
        print("REMINDER_DELIVERY_COMPLETED=\(title)")
    }
    func testTurningOffCancelsScheduledReminder() {
        continueAfterFailure = false
        let app = XCUIApplication()
        app.launchEnvironment["FEI_NOTES_MODE"] = "local"
        app.launch()
        XCTAssertTrue(app.buttons["fei.settings"].waitForExistence(timeout: 15))
        app.buttons["fei.library-actions"].tap()
        app.sheets.buttons["导入笔记"].tap()
        let fixture = app.cells.matching(NSPredicate(format: "label CONTAINS %@","FEI_NOTIFICATION21_CANCEL")).firstMatch
        if !fixture.waitForExistence(timeout: 2) {
            if app.buttons["浏览"].firstMatch.waitForExistence(timeout: 2) { app.buttons["浏览"].firstMatch.tap() }
            if !fixture.waitForExistence(timeout: 2) {
                let folder = app.cells.matching(NSPredicate(format: "label BEGINSWITH %@","飞记")).firstMatch
                if !folder.waitForExistence(timeout: 2) {
                    let phone = app.cells.matching(NSPredicate(format: "label CONTAINS %@","iPhone")).firstMatch
                    XCTAssertTrue(phone.waitForExistence(timeout: 5)); phone.tap()
                }
                XCTAssertTrue(folder.waitForExistence(timeout: 5)); folder.tap()
            }
        }
        XCTAssertTrue(fixture.waitForExistence(timeout: 8)); fixture.tap()
        if app.buttons["打开"].firstMatch.waitForExistence(timeout: 2) { app.buttons["打开"].firstMatch.tap() }
        let note = app.navigationBars.matching(NSPredicate(format: "identifier BEGINSWITH %@","FEI_NOTIFY_CANCEL21_")).firstMatch
        XCTAssertTrue(note.waitForExistence(timeout: 10),app.debugDescription)
        let title = note.identifier
        print("REMINDER_CANCEL_TITLE=\(title)")
        openPermission(app)
        let pending = app.cells.matching(NSPredicate(format: "identifier BEGINSWITH %@","fei.reminder.notifications.pending.")).containing(.staticText,identifier: title).firstMatch
        XCTAssertTrue(pending.waitForExistence(timeout: 15),app.debugDescription)
        app.buttons["fei.reminder.notifications.done"].tap()
        let toggle = app.switches["fei.object.notify"]
        XCTAssertEqual(toggle.value as? String,"1")
        if !toggle.isHittable { app.tables["fei.object.form"].swipeDown() }
        toggle.tap()
        app.buttons["fei.object.save"].tap()
        openPermission(app)
        XCTAssertTrue(XCTWaiter.wait(for: [XCTNSPredicateExpectation(predicate: NSPredicate(format: "exists == false"),object: pending)],timeout: 10) == .completed,app.debugDescription)
        shot("reminders-05-notify-off-cancels-request")
        app.buttons["fei.reminder.notifications.done"].tap()
        XCTAssertEqual(app.switches["fei.object.notify"].value as? String,"0")
        app.buttons["fei.object.save"].tap()
        app.navigationBars.buttons["返回"].firstMatch.tap()
        XCTAssertTrue(app.buttons["fei.settings"].waitForExistence(timeout: 5))
        print("REMINDER_CANCEL_COMPLETED=\(title)")
    }

}
