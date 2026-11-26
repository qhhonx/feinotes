import XCTest

final class TableBlockActionsUITests: XCTestCase {
    private let app = XCUIApplication(bundleIdentifier: "app.feinotes.mobile.feedback45")
    private var surface: XCUIElement { app.otherElements["fei.editor.surface"] }
    private var blockButton: XCUIElement { app.buttons["fei.edit.block-actions"].firstMatch }
    private var blockMenu: XCUIElement { app.tables["fei.block.controls"] }
    private var keyboard: XCUIElement { app.keyboards.firstMatch }

    private struct Row: Decodable {
        let kind: String
        let text: String
        let rect: [Double]
    }
    private struct Probe: Decodable { let blocks: [Row] }

    private func rows() -> [Row] {
        guard surface.exists,let value = surface.value as? String,
              let probe = try? JSONDecoder().decode(Probe.self,from: Data(value.utf8)) else { return [] }
        return probe.blocks
    }

    private func wait(_ reason: String,_ condition: @escaping () -> Bool) {
        let predicate = XCTNSPredicateExpectation(predicate: NSPredicate { _,_ in condition() },object: nil)
        XCTAssertEqual(XCTWaiter.wait(for: [predicate],timeout: 8),.completed,reason)
    }

    private func tap(_ row: Row,_ offset: Double = 8) {
        surface.coordinate(withNormalizedOffset: .zero).withOffset(CGVector(dx: row.rect[0]+offset,dy: row.rect[1]+row.rect[3]/2)).tap()
        let welcome = app.buttons["Continue"]
        if welcome.exists && welcome.isHittable { welcome.tap() }
    }

    private func openBlockMenu() {
        XCTAssertTrue(blockButton.waitForExistence(timeout: 8))
        wait("Block actions are available from the editor toolbar") { self.blockButton.isEnabled }
        blockButton.tap()
        XCTAssertTrue(blockMenu.waitForExistence(timeout: 8))
    }

    private func closeBlockMenu() {
        app.buttons["fei.block.done"].tap()
        wait("Closing block actions returns to the editor") { !self.blockMenu.exists && self.keyboard.exists }
    }

    private func typeKey(_ character: String) {
        let key = keyboard.keys.matching(NSPredicate(format: "label IN %@",[character,character.uppercased()])).firstMatch
        XCTAssertTrue(key.waitForExistence(timeout: 8))
        key.tap()
    }

    private func enterReadingMode() {
        let done = app.buttons["fei.edit.done"].firstMatch
        if done.isHittable {
            done.tap()
            wait("The keyboard is dismissed before changing document mode") { !self.keyboard.exists }
        }
        app.switchToReadingMode()
        wait("Reading mode hides the write controls") { !self.keyboard.exists && (!self.blockButton.exists || !self.blockButton.isEnabled) }
    }

    func testPhoneTableBlockActions() throws { try exercise(pad: false) }
    func testPadTableBlockActions() throws { try exercise(pad: true) }

    private func exercise(pad: Bool) throws {
        continueAfterFailure = false
        XCUIDevice.shared.orientation = pad ? .landscapeLeft : .portrait
        app.launchEnvironment["FEI_NOTES_MODE"] = "local"
        app.launchEnvironment["FEI_MOBILE_QA_SEED"] = "1"
        app.launch()
        let list = app.tables["fei.notes.list"]
        XCTAssertTrue(list.waitForExistence(timeout: 24))
        let note = list.cells.containing(.staticText,identifier: "QA Table block actions").firstMatch
        if !note.isHittable || note.frame.maxY > list.frame.maxY-5 {
            list.swipeDown()
            let search = app.searchFields["fei.notes.search-field"]
            XCTAssertTrue(search.waitForExistence(timeout: 8))
            search.tap()
            search.typeText("QA Table block actions")
            wait("The synthetic note is available in search") { note.isHittable }
        }
        XCTAssertTrue(note.isHittable)
        note.tap()
        XCTAssertTrue(surface.waitForExistence(timeout: 8))
        wait("The second row has been laid out") { self.rows().contains { $0.text == "Bottom left cell" } }

        // An empty cell must acquire a table target without typing first.
        let empty = try XCTUnwrap(rows().first { $0.kind == "Text" && $0.text.isEmpty })
        tap(empty)
        XCTAssertTrue(keyboard.waitForExistence(timeout: 8))
        openBlockMenu()
        XCTAssertTrue(blockMenu.cells["fei.block.table"].exists)
        closeBlockMenu()
        typeKey("j")
        let emptyMarker = { (row: Row) in row.text.lowercased() == "j" && row.rect[0] == empty.rect[0] }
        wait("The same empty cell receives input after cancellation") { self.rows().contains(where: emptyMarker) }

        let filled = try XCTUnwrap(rows().first { $0.text == "Top right cell" })
        tap(filled)
        typeKey("x")
        let filledMarker = { (row: Row) in row.text.lowercased().contains("x") && row.rect[0] == filled.rect[0] }
        wait("The filled cell has received the latest key") { self.rows().contains(where: filledMarker) }
        let committed = try XCTUnwrap(rows().first(where: filledMarker))
        openBlockMenu()
        XCTAssertFalse(blockMenu.cells["fei.block.convert"].exists)
        blockMenu.cells["fei.block.table"].tap()
        let tableMenu = app.tables["fei.table.controls"]
        XCTAssertTrue(tableMenu.waitForExistence(timeout: 8))
        XCTAssertTrue(tableMenu.cells["fei.table.select-cell"].exists)
        app.buttons["fei.table.done"].tap()

        tap(try XCTUnwrap(rows().first { $0.text == committed.text }))
        let table = try XCTUnwrap(rows().first { $0.kind.hasPrefix("Table(") })
        let follower = try XCTUnwrap(rows().first { $0.text == "After the table" })
        XCTAssertLessThan(table.rect[1],follower.rect[1])
        openBlockMenu()
        blockMenu.cells["fei.block.down"].tap()
        wait("The entire table moves below the next block") {
            let blocks = self.rows()
            guard let t = blocks.first(where: { $0.kind.hasPrefix("Table(") }),
                  let p = blocks.first(where: { $0.text == "After the table" }) else { return false }
            return t.rect[1] > p.rect[1] && blocks.contains { $0.text == committed.text } && blocks.contains(where: emptyMarker)
        }

        tap(try XCTUnwrap(rows().first { $0.text == "Ordinary block" }))
        openBlockMenu()
        XCTAssertTrue(blockMenu.cells["fei.block.convert"].exists)
        XCTAssertFalse(blockMenu.cells["fei.block.table"].exists)
        closeBlockMenu()
        enterReadingMode()
        tap(try XCTUnwrap(rows().first { $0.text == committed.text }))
        XCTAssertFalse(keyboard.exists)
        XCTAssertFalse(blockMenu.exists)
        XCTAssertFalse(blockButton.exists && blockButton.isEnabled)
    }
}
