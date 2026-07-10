import Testing
import Foundation
@testable import TillerCore

@Test func terminalTabExposesLeafIdsAndTree() {
    let a = UUID(), b = UUID()
    let tree = SplitTree.leaf(id: a).splitting(leaf: a, axis: .horizontal, newLeaf: b)
    let tab = WorkspaceTab(id: UUID(), title: "Terminale 1", tree: tree)
    #expect(tab.leafIds == [a, b])
    #expect(tab.terminalTree == tree)
    #expect(tab.markdownFileURL == nil)
}

@Test func markdownTabHasNoLeavesAndExposesFileURL() {
    let url = URL(fileURLWithPath: "/tmp/README.md")
    let tab = WorkspaceTab(id: UUID(), title: "README.md", content: .markdown(fileURL: url))
    #expect(tab.leafIds == [])
    #expect(tab.terminalTree == nil)
    #expect(tab.markdownFileURL == url)
}

@Test func nextShellTitleIgnoresMarkdownTabs() {
    let existing = [
        WorkspaceTab(id: UUID(), title: "Terminale 1", tree: .leaf(id: UUID())),
        WorkspaceTab(id: UUID(), title: "README.md",
                     content: .markdown(fileURL: URL(fileURLWithPath: "/tmp/README.md")))
    ]
    #expect(WorkspaceTab.nextShellTitle(existing: existing) == "Terminale 2")
}
