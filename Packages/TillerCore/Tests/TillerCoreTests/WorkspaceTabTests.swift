import Testing
import Foundation
@testable import TillerCore

@Test func terminalTabExposesLeafIdsAndTree() {
    let a = UUID(), b = UUID()
    let tree = SplitTree.leaf(id: a).splitting(leaf: a, axis: .horizontal, newLeaf: b)
    let tab = LegacyWorkspaceTab(id: UUID(), title: "Terminale 1", tree: tree)
    #expect(tab.leafIds == [a, b])
    #expect(tab.terminalTree == tree)
    #expect(tab.markdownFileURL == nil)
}

@Test func markdownTabHasNoLeavesAndExposesFileURL() {
    let url = URL(fileURLWithPath: "/tmp/README.md")
    let tab = LegacyWorkspaceTab(id: UUID(), title: "README.md", content: .markdown(fileURL: url))
    #expect(tab.leafIds == [])
    #expect(tab.terminalTree == nil)
    #expect(tab.markdownFileURL == url)
    #expect(tab.fileURL == url)
}

@Test func codeTabHasNoLeavesAndExposesSharedFileURL() {
    let url = URL(fileURLWithPath: "/tmp/App.swift")
    let tab = LegacyWorkspaceTab(id: UUID(), title: "App.swift", content: .code(fileURL: url))
    #expect(tab.leafIds.isEmpty)
    #expect(tab.activityPaneIds.isEmpty)
    #expect(tab.markdownFileURL == nil)
    #expect(tab.codeFileURL == url)
    #expect(tab.fileURL == url)
}

@Test func nextShellTitleIgnoresMarkdownAndCodeTabs() {
    let existing = [
        LegacyWorkspaceTab(id: UUID(), title: "Terminale 1", tree: .leaf(id: UUID())),
        LegacyWorkspaceTab(id: UUID(), title: "README.md",
                     content: .markdown(fileURL: URL(fileURLWithPath: "/tmp/README.md"))),
        LegacyWorkspaceTab(id: UUID(), title: "App.swift",
                     content: .code(fileURL: URL(fileURLWithPath: "/tmp/App.swift")))
    ]
    #expect(LegacyWorkspaceTab.nextShellTitle(existing: existing) == "Terminale 2")
}

@Test func chatTabExposesAgentIdAndActivityPane() {
    let id = UUID()
    let tab = LegacyWorkspaceTab(id: id, title: "Claude Code",
                           content: .chat(agentId: "claude", sessionId: nil))
    #expect(tab.chatAgentId == "claude")
    #expect(tab.leafIds == [])
    #expect(tab.terminalTree == nil)
    #expect(tab.markdownFileURL == nil)
    #expect(tab.codeFileURL == nil)
    #expect(tab.activityPaneIds == [id])
}

@Test func activityPaneIdsForTerminalMatchesLeafIds() {
    let leaf = UUID()
    let tab = LegacyWorkspaceTab(id: UUID(), title: "shell", tree: .leaf(id: leaf))
    #expect(tab.activityPaneIds == [leaf])
}
@Test func newTabDefaultsToAutoNamed() {
    let tab = LegacyWorkspaceTab(id: UUID(), title: "Terminale 1", tree: SplitTree.leaf(id: UUID()))
    #expect(tab.titleIsAutoNamed == true)
}

@Test func explicitTitleIsAutoNamedOverridesDefault() {
    let tab = LegacyWorkspaceTab(id: UUID(), title: "my tab", tree: SplitTree.leaf(id: UUID()), titleIsAutoNamed: false)
    #expect(tab.titleIsAutoNamed == false)
}
