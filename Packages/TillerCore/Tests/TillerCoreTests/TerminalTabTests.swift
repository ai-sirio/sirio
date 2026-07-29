import Testing
import Foundation
@testable import TillerCore

@Test func nextShellTitleCountsOnlyManualShellTabs() {
    #expect(LegacyWorkspaceTab.nextShellTitle(existing: []) == "Terminale 1")
    let tabs = [
        LegacyWorkspaceTab(id: UUID(), title: "Terminale 1", tree: .leaf(id: UUID())),
        LegacyWorkspaceTab(id: UUID(), title: "Claude Code", tree: .leaf(id: UUID()))
    ]
    #expect(LegacyWorkspaceTab.nextShellTitle(existing: tabs) == "Terminale 2")
}
