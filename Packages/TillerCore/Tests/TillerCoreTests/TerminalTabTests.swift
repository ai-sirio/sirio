import Testing
import Foundation
@testable import TillerCore

@Test func nextShellTitleCountsOnlyManualShellTabs() {
    #expect(WorkspaceTab.nextShellTitle(existing: []) == "Terminale 1")
    let tabs = [
        WorkspaceTab(id: UUID(), title: "Terminale 1", tree: .leaf(id: UUID())),
        WorkspaceTab(id: UUID(), title: "Claude Code", tree: .leaf(id: UUID()))
    ]
    #expect(WorkspaceTab.nextShellTitle(existing: tabs) == "Terminale 2")
}
