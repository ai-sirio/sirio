import Testing
import Foundation
@testable import TillerCore

@Test func nextShellTitleCountsOnlyManualShellTabs() {
    #expect(TerminalTab.nextShellTitle(existing: []) == "Terminale 1")
    let tabs = [
        TerminalTab(id: UUID(), title: "Terminale 1", tree: .leaf(id: UUID())),
        TerminalTab(id: UUID(), title: "Claude Code", tree: .leaf(id: UUID()))
    ]
    #expect(TerminalTab.nextShellTitle(existing: tabs) == "Terminale 2")
}
