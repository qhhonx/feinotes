import XCTest

final class DateFormUITests: XCTestCase {
    private func shot(_ name: String) {
        let attachment = XCTAttachment(screenshot: XCUIScreen.main.screenshot())
        attachment.name = name
        attachment.lifetime = .keepAlways
        add(attachment)
    }
    func testCountdownNumericLimits() {
        continueAfterFailure = false
        let stamp = Int(Date().timeIntervalSince1970)
        print("LIMIT_TOUCH_STAMP=\(stamp)")
        let app = XCUIApplication()
        app.launchEnvironment["FEI_NOTES_MODE"] = "local"
        app.launch()
        XCTAssertTrue(app.buttons["fei.settings"].waitForExistence(timeout: 15))
        app.buttons["fei.new"].tap()
        XCTAssertTrue(app.buttons["fei.more"].waitForExistence(timeout: 5))
        app.coordinate(withNormalizedOffset: CGVector(dx: 0.3,dy: 0.28)).tap()
        app.buttons["fei.more"].tap()
        app.sheets.buttons["插入内容"].tap()
        let category = app.cells["fei.insert.insert-group-3"]
        XCTAssertTrue(category.waitForExistence(timeout: 5))
        if !category.isHittable { app.tables["fei.insert.controls"].swipeUp() }
        category.tap()
        app.cells.containing(.staticText,identifier: "倒计时").firstMatch.tap()
        let hours = app.textFields["fei.object.hours"]
        XCTAssertTrue(hours.waitForExistence(timeout: 5))
        hours.tap()
        app.buttons["清除文本"].firstMatch.tap()
        for _ in 0..<3 { hours.typeText("9") }
        XCTAssertEqual(hours.value as? String,"99")
        app.buttons["fei.object.save"].tap()
        XCTAssertTrue(XCTWaiter.wait(for: [XCTNSPredicateExpectation(predicate: NSPredicate(format: "exists == false"),object: app.tables["fei.object.form"])],timeout: 5) == .completed)
        app.buttons["fei.more"].tap()
        app.sheets.buttons["内容设置"].tap()
        let list = app.tables["fei.objects.controls"]
        XCTAssertTrue(list.waitForExistence(timeout: 5))
        list.cells.element(boundBy: 0).tap()
        XCTAssertTrue(hours.waitForExistence(timeout: 5))
        XCTAssertEqual(hours.value as? String,"23")
        shot("dates-05-shared-numeric-normalization")
        app.buttons["fei.object.cancel"].tap()
        XCTAssertTrue(XCTWaiter.wait(for: [XCTNSPredicateExpectation(predicate: NSPredicate(format: "exists == false"),object: app.tables["fei.object.form"])],timeout: 5) == .completed)
        app.buttons["fei.more"].tap()
        app.sheets.buttons["重命名"].tap()
        let title = app.textFields["fei.rename.title"]
        XCTAssertTrue(title.waitForExistence(timeout: 5))
        title.typeText("FEI_DATE_LIMITS_\(stamp)")
        app.buttons["fei.rename.save"].tap()
        app.navigationBars.buttons["返回"].firstMatch.tap()
        XCTAssertTrue(app.buttons["fei.settings"].waitForExistence(timeout: 5))
        print("LIMIT_TOUCH_COMPLETED=\(stamp)")
    }
    func testNativeReminderAndCountdownForms() {
        continueAfterFailure = false
        let stamp = Int(Date().timeIntervalSince1970)
        print("DATE_TOUCH_STAMP=\(stamp)")
        let app = XCUIApplication()
        app.launchEnvironment["FEI_NOTES_MODE"] = "local"
        XCUIDevice.shared.orientation = .portrait
        app.launch()
        XCTAssertTrue(app.buttons["fei.settings"].waitForExistence(timeout: 15))
        func scrollTo(_ cell: XCUIElement,in table: XCUIElement) {
            for _ in 0..<8 {
                if cell.isHittable { break }
                if cell.exists && cell.frame.midY < table.frame.minY+100 { table.swipeDown() } else { table.swipeUp() }
            }
            XCTAssertTrue(cell.isHittable,app.debugDescription)
        }
        func insert(_ group: Int,_ label: String) {
            app.buttons["fei.new"].tap()
            XCTAssertTrue(app.buttons["fei.more"].waitForExistence(timeout: 5))
            app.coordinate(withNormalizedOffset: CGVector(dx: 0.3,dy: 0.28)).tap()
            app.buttons["fei.more"].tap()
            app.sheets.buttons["插入内容"].tap()
            let table = app.tables["fei.insert.controls"]
            XCTAssertTrue(table.waitForExistence(timeout: 5))
            let category = table.cells["fei.insert.insert-group-\(group)"]
            scrollTo(category,in: table)
            category.tap()
            table.cells.containing(.staticText,identifier: label).firstMatch.tap()
            XCTAssertTrue(app.tables["fei.object.form"].waitForExistence(timeout: 5),app.debugDescription)
        }
        func close(_ cancel: Bool = false) {
            app.buttons[cancel ? "fei.object.cancel" : "fei.object.save"].tap()
            XCTAssertTrue(XCTWaiter.wait(for: [XCTNSPredicateExpectation(predicate: NSPredicate(format: "exists == false"),object: app.tables["fei.object.form"])],timeout: 5) == .completed)
        }
        func open() {
            app.buttons["fei.more"].tap()
            app.sheets.buttons["内容设置"].tap()
            let list = app.tables["fei.objects.controls"]
            XCTAssertTrue(list.waitForExistence(timeout: 5))
            XCTAssertEqual(list.cells.count,1)
            list.cells.element(boundBy: 0).tap()
            XCTAssertTrue(app.tables["fei.object.form"].waitForExistence(timeout: 5))
        }
        func choose(_ key: String,_ value: String) {
            let table = app.tables["fei.object.form"]
            let row = table.cells["fei.object.row.\(key)"]
            scrollTo(row,in: table)
            row.tap()
            let options = app.tables["fei.object.options"]
            XCTAssertTrue(options.waitForExistence(timeout: 5))
            let option = options.cells["fei.object.option.\(value)"]
            scrollTo(option,in: options)
            option.tap()
        }
        func toggle(_ key: String) {
            let control = app.switches["fei.object.\(key)"]
            scrollTo(control,in: app.tables["fei.object.form"])
            control.tap()
        }
        func number(_ key: String,_ value: String) {
            let field = app.textFields["fei.object.\(key)"]
            scrollTo(field,in: app.tables["fei.object.form"])
            field.tap()
            app.buttons["清除文本"].firstMatch.tap()
            field.typeText(value)
        }
        func finish(_ title: String) {
            app.buttons["fei.more"].tap()
            app.sheets.buttons["重命名"].tap()
            let field = app.textFields["fei.rename.title"]
            XCTAssertTrue(field.waitForExistence(timeout: 5))
            field.typeText("\(title)_\(stamp)")
            app.buttons["fei.rename.save"].tap()
            app.navigationBars.buttons["返回"].firstMatch.tap()
            XCTAssertTrue(app.buttons["fei.settings"].waitForExistence(timeout: 5))
        }
        insert(2,"日期提醒")
        toggle("whole")
        let picker = app.cells["fei.object.row.when"]
        XCTAssertTrue(picker.waitForExistence(timeout: 5))
        let year = Calendar.current.component(.year,from: Date())
        let yearWheel = picker.pickerWheels.matching(NSPredicate(format: "value CONTAINS %@",String(year))).firstMatch
        XCTAssertTrue(yearWheel.waitForExistence(timeout: 5),app.debugDescription)
        let yearText = yearWheel.value as! String
        yearWheel.adjust(toPickerWheelValue: yearText.replacingOccurrences(of: String(year),with: String(year+1)))
        toggle("notify")
        XCTAssertFalse(app.cells["fei.object.row.advance"].exists)
        toggle("notify")
        choose("advance","-140399000")
        shot("dates-01-native-reminder")
        close()
        open()
        XCTAssertEqual(app.switches["fei.object.whole"].value as? String,"1")
        XCTAssertTrue(picker.pickerWheels.matching(NSPredicate(format: "value CONTAINS %@",String(year+1))).firstMatch.exists)
        XCTAssertTrue(app.cells["fei.object.row.advance"].staticTexts["提前 1 天 (09:00)"].exists)
        close()
        finish("FEI_DATE_REMINDER")
        insert(3,"倒计时")
        number("days","0")
        number("hours","1")
        number("minutes","2")
        number("seconds","3")
        if app.switches["fei.object.notify"].value as? String != "1" { toggle("notify") }
        let color = app.cells["fei.object.row.color"]
        scrollTo(color,in: app.tables["fei.object.form"])
        color.tap()
        let blue = app.tables["fei.object.options"].cells.containing(.staticText,identifier: "蓝色").firstMatch
        XCTAssertTrue(blue.waitForExistence(timeout: 5))
        scrollTo(blue,in: app.tables["fei.object.options"])
        blue.tap()
        shot("dates-02-countdown-duration")
        close()
        open()
        XCTAssertEqual(app.textFields["fei.object.hours"].value as? String,"1")
        number("hours","9")
        close(true)
        open()
        XCTAssertEqual(app.textFields["fei.object.hours"].value as? String,"1")
        choose("mode","date")
        let end = app.cells["fei.object.row.end"]
        XCTAssertTrue(end.waitForExistence(timeout: 5))
        app.buttons["fei.object.save"].tap()
        let error = app.staticTexts["fei.object.error"]
        XCTAssertTrue(error.waitForExistence(timeout: 5),app.debugDescription)
        XCTAssertTrue(XCTWaiter.wait(for: [XCTNSPredicateExpectation(predicate: NSPredicate(format: "hittable == true"),object: error)],timeout: 5) == .completed)
        shot("dates-03-shared-past-end-error")
        let dateWheel = end.pickerWheels.element(boundBy: 0)
        let before = dateWheel.value as? String
        dateWheel.swipeUp()
        XCTAssertNotEqual(dateWheel.value as? String,before)
        shot("dates-04-native-future-end")
        close()
        open()
        XCTAssertTrue(end.waitForExistence(timeout: 5))
        XCTAssertTrue(app.cells["fei.object.row.mode"].staticTexts["选择具体日期"].exists)
        close()
        finish("FEI_DATE_COUNTDOWN")
        print("DATE_TOUCH_COMPLETED=\(stamp)")
    }
}
