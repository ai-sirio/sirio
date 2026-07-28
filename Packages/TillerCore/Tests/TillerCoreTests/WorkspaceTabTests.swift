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
    #expect(tab.fileURL == url)
}

@Test func codeTabHasNoLeavesAndExposesSharedFileURL() {
    let url = URL(fileURLWithPath: "/tmp/App.swift")
    let tab = WorkspaceTab(id: UUID(), title: "App.swift", content: .code(fileURL: url))
    #expect(tab.leafIds.isEmpty)
    #expect(tab.activityPaneIds.isEmpty)
    #expect(tab.markdownFileURL == nil)
    #expect(tab.codeFileURL == url)
    #expect(tab.fileURL == url)
}

@Test func nextShellTitleIgnoresMarkdownAndCodeTabs() {
    let existing = [
        WorkspaceTab(id: UUID(), title: "Terminale 1", tree: .leaf(id: UUID())),
        WorkspaceTab(id: UUID(), title: "README.md",
                     content: .markdown(fileURL: URL(fileURLWithPath: "/tmp/README.md"))),
        WorkspaceTab(id: UUID(), title: "App.swift",
                     content: .code(fileURL: URL(fileURLWithPath: "/tmp/App.swift")))
    ]
    #expect(WorkspaceTab.nextShellTitle(existing: existing) == "Terminale 2")
}

@Test func chatTabExposesAgentIdAndActivityPane() {
    let id = UUID()
    let tab = WorkspaceTab(id: id, title: "Claude Code",
                           content: .chat(agentId: "claude"))
    #expect(tab.chatAgentId == "claude")
    #expect(tab.leafIds == [])
    #expect(tab.terminalTree == nil)
    #expect(tab.markdownFileURL == nil)
    #expect(tab.codeFileURL == nil)
    #expect(tab.activityPaneIds == [id])
}

@Test func activityPaneIdsForTerminalMatchesLeafIds() {
    let leaf = UUID()
    let tab = WorkspaceTab(id: UUID(), title: "shell", tree: .leaf(id: leaf))
    #expect(tab.activityPaneIds == [leaf])
}
@Test func newTabDefaultsToAutoNamed() {
    let tab = WorkspaceTab(id: UUID(), title: "Terminale 1", tree: SplitTree.leaf(id: UUID()))
    #expect(tab.titleIsAutoNamed == true)
}

@Test func explicitTitleIsAutoNamedOverridesDefault() {
    let tab = WorkspaceTab(id: UUID(), title: "my tab", tree: SplitTree.leaf(id: UUID()), titleIsAutoNamed: false)
    #expect(tab.titleIsAutoNamed == false)
}
