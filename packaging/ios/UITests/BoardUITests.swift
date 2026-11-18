import XCTest

final class BoardUITests: XCTestCase {
    private let app = XCUIApplication()
    private var surface: XCUIElement { app.otherElements["fei.editor.surface"].firstMatch }
    private var done: XCUIElement { app.buttons["fei.board.done"] }

    private func wait(_ label: String, until condition: @escaping () -> Bool) {
        let expectation = XCTNSPredicateExpectation(predicate: NSPredicate { _, _ in condition() }, object: nil)
        XCTAssertEqual(XCTWaiter.wait(for: [expectation], timeout: 8), .completed, label)
    }

    private func button(_ id: String) -> XCUIElement {
        app.buttons.matching(identifier: id).firstMatch
    }

    private func point(_ x: CGFloat, _ y: CGFloat) -> XCUICoordinate {
        surface.coordinate(withNormalizedOffset: .zero).withOffset(CGVector(dx: x, dy: y))
    }

    private func surfaceState() -> [String: Any]? {
        guard let value = surface.value as? String,
              let data = value.data(using: .utf8) else { return nil }
        return (try? JSONSerialization.jsonObject(with: data)) as? [String: Any]
    }

    private func scenes() -> [[Any]]? { surfaceState()?["elements"] as? [[Any]] }

    private func openInsert() {
        let quick = button("fei.edit.quick-insert")
        if quick.isHittable && quick.isEnabled { quick.tap() }
        else { button("fei.edit.insert").tap() }
        let sheet = app.tables["fei.insert.controls"]
        XCTAssertTrue(sheet.waitForExistence(timeout: 8))
        sheet.cells["fei.insert.insert-group-1"].tap()
        let board = sheet.cells["fei.insert.board"]
        if !board.isHittable { sheet.swipeUp() }
        board.tap()
    }

    private func tool(_ index: Int, desktopIndex: Int) {
        let width = surface.frame.width
        let count: CGFloat = width < 356 ? 6 : 7
        let compact = (width + 48 - (count * 40 + 4)) / 2 + 22 + CGFloat(index) * 40
        let full = (width - 542) / 2 + 71 + CGFloat(desktopIndex) * 40
        point(width < 680 ? compact : full, 30).tap()
    }

    private func documentChip(topPadding: Bool = false) throws {
        func boardRect() -> [CGFloat]? {
            let blocks = surfaceState()?["blocks"] as? [[String: Any]]
            return blocks?.first { ($0["kind"] as? String)?.hasPrefix("Board(") == true }?["rect"] as? [CGFloat]
        }
        wait("Board card is laid out") { boardRect()?.count == 4 }
        let rect = try XCTUnwrap(boardRect())
        // The transparent top padding also opens the board.
        point(rect[0] + rect[2] - 34, rect[1] + (topPadding ? 15 : 32)).tap()
    }

    private func formattingMenu() {
        button("fei.more").tap()
        let format = app.buttons.matching(NSPredicate(format: "label IN %@", ["编辑和格式", "Editing and formatting"])).firstMatch
        XCTAssertTrue(format.waitForExistence(timeout: 5))
        format.tap()
    }

    private func contentSettings() -> XCUIElement {
        formattingMenu()
        let content = app.buttons.matching(NSPredicate(format: "label IN %@", ["内容设置", "Content settings"])).firstMatch
        XCTAssertTrue(content.waitForExistence(timeout: 5))
        content.tap()
        let table = app.tables["fei.objects.controls"]
        XCTAssertTrue(table.waitForExistence(timeout: 8))
        return table.cells.matching(NSPredicate(format: "identifier BEGINSWITH %@", "fei.objects.board:")).firstMatch
    }

    private func enterReadingMode() {
        formattingMenu()
        let reading = app.buttons.matching(NSPredicate(format: "label IN %@", ["阅读模式", "Reading mode"])).firstMatch
        XCTAssertTrue(reading.waitForExistence(timeout: 5))
        reading.tap()
    }

    func testBoardTouchEditsSurviveReopen() throws {
        continueAfterFailure = false
        let name = "Board QA " + UUID().uuidString.prefix(8)
        let fixtures = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: fixtures, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: fixtures) }
        let paragraphs = (1...24).map { "<p>Board touch fixture paragraph \($0)</p>" }.joined()
        try "<title>\(name)</title>\(paragraphs)".write(to: fixtures.appendingPathComponent("board.xml"), atomically: true, encoding: .utf8)
        app.launchEnvironment["FEI_NOTES_MODE"] = "local"
        app.launchEnvironment["FEI_MOBILE_QA_FIXTURES"] = fixtures.path
        XCUIDevice.shared.orientation = UIDevice.current.userInterfaceIdiom == .pad ? .landscapeLeft : .portrait
        app.launch()
        let row = app.tables["fei.notes.list"].cells.containing(.staticText, identifier: String(name)).firstMatch
        XCTAssertTrue(row.waitForExistence(timeout: 12), app.debugDescription)
        row.tap()
        let more = button("fei.more")
        XCTAssertTrue(more.waitForExistence(timeout: 12))

        openInsert()
        XCTAssertTrue(done.waitForExistence(timeout: 8), app.debugDescription)
        XCTAssertFalse(app.toolbars["fei.editor.toolbar"].isHittable)
        wait("An empty newly inserted board") { self.scenes()?.isEmpty == true }

        let width = surface.frame.width
        let height = surface.frame.height
        let x = max(245, width * 0.57)
        let y = height * 0.56
        tool(2, desktopIndex: 2)
        point(x, y).press(forDuration: 0.1, thenDragTo: point(x + 65, y + 67))
        wait("Finger drag committed a rectangle") { self.scenes()?.first?[1] as? String == "rectangle" }
        let before = try XCTUnwrap(scenes()?.first)

        tool(1, desktopIndex: 1)
        point(x + 25, y).tap()
        point(x + 25, y).press(forDuration: 0.1, thenDragTo: point(x + 45, y - 63))
        wait("Finger drag moved the same rectangle") {
            guard let after = self.scenes()?.first else { return false }
            return after[0] as? String == before[0] as? String && after[3] as? Double != before[3] as? Double
        }

        tool(4, desktopIndex: 7)
        point(x - 25, y + 90).press(forDuration: 0.1, thenDragTo: point(x + 46, y + 115))
        wait("Finger drew a freehand stroke") { self.scenes()?.last?[1] as? String == "freedraw" }
        let saved = try XCTUnwrap(surface.value as? String)
        done.tap()
        XCTAssertTrue(more.waitForExistence(timeout: 8))
        try documentChip(topPadding: true)
        XCTAssertTrue(done.waitForExistence(timeout: 8), app.debugDescription)
        XCTAssertEqual(surface.value as? String, saved)
        done.tap()
        XCTAssertTrue(more.waitForExistence(timeout: 8))

        app.terminate()
        app.launch()
        XCTAssertTrue(row.waitForExistence(timeout: 8), app.debugDescription)
        row.tap()
        XCTAssertTrue(more.waitForExistence(timeout: 8))
        let boardAction = contentSettings()
        XCTAssertTrue(boardAction.isHittable)
        XCTAssertTrue(boardAction.staticTexts.matching(NSPredicate(format: "label IN %@", ["编辑画板 1", "Edit board 1"])).firstMatch.exists)
        boardAction.tap()
        XCTAssertTrue(done.waitForExistence(timeout: 8), app.debugDescription)
        XCTAssertEqual(surface.value as? String, saved)
        done.tap()
        XCTAssertTrue(more.waitForExistence(timeout: 8))
        enterReadingMode()
        try documentChip()
        XCTAssertFalse(done.exists)
        XCTAssertFalse(contentSettings().exists)
        button("fei.objects.done").tap()
        let beforeScroll = surface.value as? String
        surface.swipeUp()
        wait("The document still scrolls across the board in reading mode") {
            self.surface.value as? String != beforeScroll && !self.done.exists
        }
    }
}
