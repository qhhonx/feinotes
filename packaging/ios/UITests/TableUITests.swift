import XCTest

final class TableUITests: XCTestCase {
    private func shot(_ name: String) {
        let attachment = XCTAttachment(screenshot: XCUIScreen.main.screenshot())
        attachment.name = name
        attachment.lifetime = .keepAlways
        add(attachment)
    }

    func testWideTableSwipesKeepNativeBackAvailable() throws {
        try checkHorizontalSwipes(kind: "table")
    }

    func testUnwrappedCodeSwipesKeepNativeBackAvailable() throws {
        try checkHorizontalSwipes(kind: "code")
    }

    private func checkHorizontalSwipes(kind: String) throws {
        continueAfterFailure = false
        let fixtures = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: fixtures, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: fixtures) }
        let title = "QA Wide " + kind + " " + UUID().uuidString.prefix(6)
        let content: String
        if kind == "table" {
            let cols = String(repeating: "<col width=\"160\" />", count: 12)
            let rows = (1...9).map { row in
                "<tr>" + (1...12).map { "<td><p>Row \(row), column \($0)</p></td>" }.joined() + "</tr>"
            }.joined()
            content = "<table><colgroup>\(cols)</colgroup><tbody>\(rows)</tbody></table>"
        } else {
            let code = (1...16).map { row in
                "LEFT_BOUNDARY_\(row)  " + (1...14).map { "SEGMENT_\($0)" }.joined(separator: "  ")
            }.joined(separator: "\n")
            content = "<pre lang=\"plaintext\"><code>\(code)</code></pre>"
        }
        try "<title>\(title)</title><p>Text above the scrollable block</p>\(content)<p>Text below the scrollable block</p>"
            .write(to: fixtures.appendingPathComponent("wide-block.xml"), atomically: true, encoding: .utf8)

        let app = XCUIApplication()
        app.launchEnvironment["FEI_NOTES_MODE"] = "local"
        app.launchEnvironment["FEI_MOBILE_QA_FIXTURES"] = fixtures.path
        app.launchEnvironment["FEI_QA_HORIZONTAL_SCROLL"] = kind
        let pad = UIDevice.current.userInterfaceIdiom == .pad
        XCUIDevice.shared.orientation = pad ? .landscapeLeft : .portrait
        defer { XCUIDevice.shared.orientation = .portrait }
        app.launch()
        let list = app.tables["fei.notes.list"].firstMatch
        let surface = app.descendants(matching: .any)["fei.editor.surface"].firstMatch
        let note = list.cells.containing(.staticText, identifier: String(title)).firstMatch

        func eventually(_ message: String, _ check: @escaping () -> Bool) {
            let condition = XCTNSPredicateExpectation(predicate: NSPredicate { _, _ in check() }, object: nil)
            XCTAssertEqual(XCTWaiter.wait(for: [condition], timeout: 10), .completed, message)
        }

        func back() { app.navigationBars.buttons.element(boundBy: 0).tap() }
        func chooseTheme(_ theme: String) {
            let settings = app.buttons.matching(identifier: "fei.settings").firstMatch
            XCTAssertTrue(settings.waitForExistence(timeout: 10))
            settings.tap()
            app.cells["fei.setting.page:Appearance"].tap()
            app.cells["fei.setting.interface-style"].tap()
            app.cells["fei.setting.option.interface-style."+theme].tap()
            app.cells["fei.setting.theme"].tap()
            app.cells["fei.setting.option.theme."+theme].tap()
            back()
            if !list.isHittable {
                let close = app.buttons.matching(identifier: "fei.close-settings").firstMatch
                if close.isHittable { close.tap() } else { back() }
            }
            eventually("Browser visible after changing theme") { list.isHittable }
        }

        func numbers() -> [CGFloat] {
            guard let value = surface.value as? String, value.hasPrefix("horizontal:") else { return [] }
            let numbers = value.dropFirst("horizontal:".count).split(separator: ",").compactMap { Double($0).map { CGFloat($0) } }
            return numbers.count == 5 ? numbers : []
        }

        func drag(from start: CGPoint, to end: CGPoint) {
            let origin = app.coordinate(withNormalizedOffset: .zero)
            origin.withOffset(CGVector(dx: start.x - app.frame.minX, dy: start.y - app.frame.minY))
                .press(forDuration: 0.06, thenDragTo: origin.withOffset(CGVector(dx: end.x - app.frame.minX, dy: end.y - app.frame.minY)),
                       withVelocity: .fast, thenHoldForDuration: 0)
        }

        for theme in ["notes", "feishu"] {
            if theme != "notes" { app.terminate(); app.launch() }
            chooseTheme(theme)
            XCTAssertTrue(note.waitForExistence(timeout: 12))
            note.tap()
            eventually("Horizontal viewport in \(theme)") { surface.isHittable && numbers().count == 5 }
            var measured = numbers()
            let frame = surface.frame
            let left = frame.minX + measured[0]
            let y = frame.minY + measured[1] + min(55, measured[3] * 0.5)
            let right = left + measured[2]
            let startX = left + min(8, measured[2] * 0.1)
            let middleX = left + measured[2] * 0.7
            let original = measured[4]
            print("HORIZONTAL_SCROLL kind=\(kind) theme=\(theme) pad=\(pad) frame=\(frame) viewport=\(measured) y=\(y)")
            shot("\(kind)-\(theme)-01-table-before")
            drag(from: CGPoint(x: right - 20, y: y), to: CGPoint(x: left + measured[2] * 0.4, y: y))
            eventually("Left drag reveals later content in \(theme)") { numbers().last.map { $0 < original - 30 } == true }
            measured = numbers()
            let shifted = try XCTUnwrap(measured.last)
            print("HORIZONTAL_SCROLL_SHIFTED kind=\(kind) theme=\(theme) values=\(measured)")
            shot("\(kind)-\(theme)-02-right-columns")
            drag(from: CGPoint(x: startX, y: y), to: CGPoint(x: middleX, y: y))
            let returned = numbers()
            print("HORIZONTAL_SCROLL_RETURNED kind=\(kind) theme=\(theme) values=\(returned)")
            shot("\(kind)-\(theme)-03-left-columns")
            eventually("Right drag reveals earlier content without leaving the document in \(theme)") {
                numbers().last.map { $0 > shifted + 30 } == true
            }
            XCTAssertTrue(app.buttons["fei.more"].firstMatch.isHittable)
            if !pad {
                let aboveBlock = frame.minY + measured[1] - 38
                drag(from: CGPoint(x: app.frame.minX + 8, y: aboveBlock), to: CGPoint(x: app.frame.maxX - 35, y: aboveBlock))
                eventually("A leading-edge swipe above the block still returns") { list.isHittable && !surface.isHittable }
                shot("\(kind)-\(theme)-04-edge-back")
            }
        }

    }

    func testNativeTableOperationsPreserveSharedDocument() {
        continueAfterFailure = false
        let stamp = Int(Date().timeIntervalSince1970)
        print("TABLE_TOUCH_STAMP=\(stamp)")
        let app = XCUIApplication()
        app.launchEnvironment["FEI_NOTES_MODE"] = "local"
        app.launch()
        XCTAssertTrue(app.buttons["fei.settings"].waitForExistence(timeout: 15))
        app.buttons["fei.new"].tap()
        XCTAssertTrue(app.buttons["fei.more"].waitForExistence(timeout: 5))
        app.coordinate(withNormalizedOffset: CGVector(dx: 0.3,dy: 0.28)).tap()
        app.typeText("FEI_TABLE_\(stamp)")
        app.buttons["fei.more"].tap()
        app.sheets.buttons["编辑和格式"].tap()
        app.sheets.buttons["表格"].tap()
        XCTAssertTrue(XCTWaiter.wait(for: [XCTNSPredicateExpectation(predicate: NSPredicate(format: "exists == false"),object: app.sheets.buttons["表格"])],timeout: 5) == .completed,app.debugDescription)
        XCTAssertTrue(app.keyboards.firstMatch.waitForExistence(timeout: 5),app.debugDescription)
        app.typeText("Cell_A_\(stamp)")
        app.buttons["fei.more"].tap()
        XCTAssertTrue(app.sheets.buttons["表格操作"].waitForExistence(timeout: 5),app.debugDescription)
        app.sheets.buttons["表格操作"].tap()
        let table = app.tables["fei.table.controls"]
        XCTAssertTrue(table.waitForExistence(timeout: 5),app.debugDescription)
        shot("table-01-native-controls")
        func tap(_ key: String, remains: Bool = true) {
            let cell = table.cells["fei.table.\(key)"]
            for _ in 0..<8 {
                if cell.isHittable { break }
                if cell.exists && cell.frame.minY < table.frame.minY { table.swipeDown() }
                else { table.swipeUp() }
            }
            XCTAssertTrue(cell.isHittable,app.debugDescription)
            cell.tap()
            RunLoop.current.run(until: Date().addingTimeInterval(0.3))
            if remains { XCTAssertTrue(table.exists,app.debugDescription) }
        }
        tap("row-after")
        tap("column-after")
        tap("select-cell")
        tap("extend-right")
        tap("merge")
        XCTAssertTrue(table.cells["fei.table.split"].isEnabled)
        shot("table-02-merged-controls")
        app.buttons["fei.table.done"].tap()
        RunLoop.current.run(until: Date().addingTimeInterval(0.4))
        shot("table-02-merged-document")
        app.buttons["fei.more"].tap()
        app.sheets.buttons["表格操作"].tap()
        XCTAssertTrue(table.waitForExistence(timeout: 5))
        tap("split")
        tap("background-palette")
        XCTAssertEqual(table.cells.count,16)
        tap("background-color:purple")
        tap("background-color:light-blue")
        app.navigationBars["fei.table.navigation"].buttons.element(boundBy: 0).tap()
        tap("select-row")
        tap("cell-text")
        tap("format:bold")
        tap("text-colors")
        XCTAssertEqual(table.cells.count,8)
        tap("format:text-color:purple")
        app.navigationBars["fei.table.navigation"].buttons.element(boundBy: 0).tap()
        app.navigationBars["fei.table.navigation"].buttons.element(boundBy: 0).tap()
        tap("cell-paragraph")
        tap("format:align-right")
        app.navigationBars["fei.table.navigation"].buttons.element(boundBy: 0).tap()
        tap("select-cell")
        tap("extend-right")
        tap("vertical-middle")
        tap("header-row")
        tap("width-wide")
        tap("select-cell")
        tap("row-down")
        tap("row-up")
        tap("select-cell")
        tap("column-right")
        tap("column-left")
        app.buttons["fei.table.done"].tap()
        XCTAssertTrue(app.buttons["fei.more"].waitForExistence(timeout: 5))
        RunLoop.current.run(until: Date().addingTimeInterval(0.4))
        shot("table-03-unequal-width-document")
        app.buttons["fei.more"].tap()
        app.sheets.buttons["表格操作"].tap()
        XCTAssertTrue(table.waitForExistence(timeout: 5))
        tap("delete-table",remains: false)
        XCTAssertTrue(XCTWaiter.wait(for: [XCTNSPredicateExpectation(predicate: NSPredicate(format: "exists == false"),object: table)],timeout: 5) == .completed)
        app.buttons["fei.more"].tap()
        app.sheets.buttons["编辑和格式"].tap()
        app.sheets.buttons["撤销"].tap()
        RunLoop.current.run(until: Date().addingTimeInterval(0.4))
        app.buttons["fei.more"].tap()
        XCTAssertTrue(app.sheets.buttons["表格操作"].waitForExistence(timeout: 5),app.debugDescription)
        app.sheets.buttons["表格操作"].tap()
        XCTAssertTrue(table.waitForExistence(timeout: 5))
        shot("table-04-restored-controls")
        app.buttons["fei.table.done"].tap()
        app.switchToReadingMode()
        app.buttons["fei.more"].tap()
        app.sheets.buttons["表格操作"].tap()
        XCTAssertTrue(table.waitForExistence(timeout: 5))
        XCTAssertEqual(table.cells.count,6,app.debugDescription)
        XCTAssertFalse(app.keyboards.firstMatch.exists)
        table.cells["fei.table.select-table"].tap()
        shot("table-05-read-only-controls")
        app.buttons["fei.table.done"].tap()
        RunLoop.current.run(until: Date().addingTimeInterval(0.4))
        shot("table-06-read-only-document")
        app.navigationBars.buttons.element(boundBy: 0).tap()
        XCTAssertTrue(app.buttons["fei.settings"].waitForExistence(timeout: 5))
        print("TABLE_TOUCH_COMPLETED=\(stamp)")
    }
}
