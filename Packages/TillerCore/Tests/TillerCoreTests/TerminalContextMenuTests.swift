import Testing
@testable import TillerCore

@Suite("TerminalContextMenu") struct TerminalContextMenuTests {
    @Test func itemHoldsValues() {
        let item = TerminalContextMenuItem(title: "Copy", systemImage: "doc.on.doc", action: .copy)
        #expect(item.title == "Copy")
        #expect(item.systemImage == "doc.on.doc")
        #expect(item.action == .copy)
    }

    @Test func actionsAreEquatable() {
        #expect(TerminalContextMenuAction.copy != .paste)
        #expect(TerminalContextMenuAction.splitRight != .splitDown)
        #expect(TerminalContextMenuAction.clear != .close)
        #expect(TerminalContextMenuAction.clear != .copy)
    }
}
