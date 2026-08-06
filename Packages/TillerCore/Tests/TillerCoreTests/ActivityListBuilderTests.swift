import Foundation
import Testing

@testable import TillerCore

@Suite struct ActivityListBuilderTests {
    // Fixed UUIDs so ids and ordering are assertable.
    static let worktreeA = UUID(uuidString: "AAAAAAAA-AAAA-AAAA-AAAA-AAAAAAAAAAAA")!
    static let worktreeB = UUID(uuidString: "BBBBBBBB-BBBB-BBBB-BBBB-BBBBBBBBBBBB")!
    static let chatTabId = UUID(uuidString: "11111111-1111-1111-1111-111111111111")!
    static let termTabId = UUID(uuidString: "22222222-2222-2222-2222-222222222222")!
    static let paneId = UUID(uuidString: "33333333-3333-3333-3333-333333333333")!
    static let docTabId = UUID(uuidString: "44444444-4444-4444-4444-444444444444")!

    static func chatTab(_ id: UUID = chatTabId, title: String = "Fix login bug") -> WorkspaceTab {
        WorkspaceTab(id: WorkspaceTabID(id), title: title, titleIsAutoNamed: false,
                     content: .chat(ChatContentID("claude-acp-chat")))
    }

    static func terminalTab(_ id: UUID = termTabId, title: String = "zsh") -> WorkspaceTab {
        WorkspaceTab(id: WorkspaceTabID(id), title: title, titleIsAutoNamed: false,
                     content: .terminal(TerminalContentID()))
    }

    static func input(
        worktreeId: UUID = worktreeA,
        label: String = "tiller/main",
        tabs: [WorkspaceTab],
        livePaneIds: [WorkspaceTabID: UUID] = [WorkspaceTabID(termTabId): paneId]
    ) -> ActivityListBuilder.WorktreeInput {
        ActivityListBuilder.WorktreeInput(
            worktreeId: worktreeId, label: label, tabs: tabs, livePaneIds: livePaneIds)
    }

    /// The whole point of the rewrite: the old AgentTreeBuilder dropped this
    /// tab because no agent was ever identified in it.
    @Test func terminalWithoutAnyAgentIsStillListed() {
        let rows = ActivityListBuilder.build(
            worktrees: [Self.input(tabs: [Self.terminalTab()])],
            agentStatus: [:], paneAgents: [:])

        #expect(rows.count == 1)
        #expect(rows[0].kind == .terminal)
        #expect(rows[0].title == "zsh")
        #expect(rows[0].agentId == nil)
        #expect(rows[0].status == .idle)
        #expect(rows[0].tabId == Self.termTabId)
    }

    @Test func terminalRowTakesStatusAndAgentFromItsLivePane() {
        let rows = ActivityListBuilder.build(
            worktrees: [Self.input(tabs: [Self.terminalTab()])],
            agentStatus: [Self.paneId: .running],
            paneAgents: [Self.paneId: "codex"])

        #expect(rows[0].agentId == "codex")
        #expect(rows[0].status == .running)
    }

    /// A chat's status and agent are keyed by the tab id, not by a pane id.
    @Test func chatRowIsKeyedByTabIdAndKeepsTheTabTitle() {
        let rows = ActivityListBuilder.build(
            worktrees: [Self.input(tabs: [Self.chatTab()], livePaneIds: [:])],
            agentStatus: [Self.chatTabId: .needsInput],
            paneAgents: [Self.chatTabId: "claude-acp"])

        #expect(rows.count == 1)
        #expect(rows[0].kind == .chat)
        #expect(rows[0].title == "Fix login bug")
        #expect(rows[0].agentId == "claude-acp")
        #expect(rows[0].status == .needsInput)
    }

    @Test func documentTabsAreExcluded() {
        let doc = WorkspaceTab(
            id: WorkspaceTabID(Self.docTabId), title: "README.md", titleIsAutoNamed: false,
            content: .document(
                DocumentID.make(worktreeID: Self.worktreeA,
                                fileURL: URL(fileURLWithPath: "/tmp/README.md")),
                editor: .markdown))
        let rows = ActivityListBuilder.build(
            worktrees: [Self.input(tabs: [doc, Self.chatTab()], livePaneIds: [:])],
            agentStatus: [:], paneAgents: [:])

        #expect(rows.count == 1)
        #expect(rows[0].kind == .chat)
    }

    /// No live pane means the PTY was never mounted: nothing is running, so
    /// there is nothing to list or to close.
    @Test func terminalWithoutLivePaneIsSkipped() {
        let rows = ActivityListBuilder.build(
            worktrees: [Self.input(tabs: [Self.terminalTab()], livePaneIds: [:])],
            agentStatus: [:], paneAgents: [:])

        #expect(rows.isEmpty)
    }

    @Test func rowsFollowWorktreeOrderThenTabOrder() {
        let rows = ActivityListBuilder.build(
            worktrees: [
                Self.input(worktreeId: Self.worktreeA, label: "tiller/main",
                           tabs: [Self.terminalTab(), Self.chatTab()]),
                Self.input(worktreeId: Self.worktreeB, label: "tiller/feat-x",
                           tabs: [Self.chatTab(Self.docTabId, title: "Other chat")],
                           livePaneIds: [:]),
            ],
            agentStatus: [:], paneAgents: [:])

        #expect(rows.count == 3)
        #expect(rows.map(\.worktreeLabel) == ["tiller/main", "tiller/main", "tiller/feat-x"])
        #expect(rows[0].kind == .terminal)
        #expect(rows[1].kind == .chat)
        #expect(rows[2].title == "Other chat")
    }

    /// Two worktrees can hold tabs with colliding ids only in theory, but the
    /// row id must still be unique per worktree for ForEach to be stable.
    @Test func rowIdCombinesWorktreeAndTab() {
        let rows = ActivityListBuilder.build(
            worktrees: [Self.input(tabs: [Self.chatTab()], livePaneIds: [:])],
            agentStatus: [:], paneAgents: [:])

        #expect(rows[0].id == "\(Self.worktreeA.uuidString):\(Self.chatTabId.uuidString)")
    }
}
