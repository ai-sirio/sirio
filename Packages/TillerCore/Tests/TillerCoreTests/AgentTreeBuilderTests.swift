import Foundation
import Testing
@testable import TillerCore

@Suite struct AgentTreeBuilderTests {
    // Fixed UUIDs so ids are assertable.
    static let chatTabId = UUID(uuidString: "11111111-1111-1111-1111-111111111111")!
    static let termTabId = UUID(uuidString: "22222222-2222-2222-2222-222222222222")!
    static let paneId = UUID(uuidString: "33333333-3333-3333-3333-333333333333")!

    func makeTabs() -> [LegacyWorkspaceTab] {
        let chatTab = LegacyWorkspaceTab(id: Self.chatTabId, title: "Claude Code",
                                   content: .chat(agentId: "claude", sessionId: nil))
        let tree = SplitTree.leaf(id: Self.paneId)
        let termTab = LegacyWorkspaceTab(id: Self.termTabId, title: "Terminale 1", tree: tree)
        // Terminal tab listed first: builder must still put chat first.
        return [termTab, chatTab]
    }

    @Test func chatTabsComeFirstThenTerminals() {
        let nodes = AgentTreeBuilder.build(
            tabs: makeTabs(),
            agentStatus: [Self.chatTabId: .running, Self.paneId: .running],
            paneAgents: [Self.chatTabId: "claude", Self.paneId: "codex"],
            chatSubagents: [:], processTrees: [:],
            catalogIds: ["claude", "codex"],
            displayNames: ["claude": "Claude Code", "codex": "Codex"])
        #expect(nodes.count == 2)
        #expect(nodes[0].id == "chat:\(Self.chatTabId.uuidString)")
        #expect(nodes[1].id == "term:\(Self.paneId.uuidString)")
        #expect(nodes[0].title == "Claude Code")
        #expect(nodes[1].agentId == "codex")
    }

    /// Il nodo chat mostra il titolo della tab (che l'auto-rename aggiorna),
    /// mai l'id agente raw — le chat registrano id canonici ("claude-acp")
    /// assenti da displayNames.
    @Test func chatNodeTitleFollowsTabTitle() {
        let tab = LegacyWorkspaceTab(id: Self.chatTabId, title: "Fix login bug",
                               content: .chat(agentId: "claude-acp", sessionId: nil))
        let nodes = AgentTreeBuilder.build(
            tabs: [tab],
            agentStatus: [Self.chatTabId: .running],
            paneAgents: [Self.chatTabId: "claude-acp"],
            chatSubagents: [:], processTrees: [:],
            catalogIds: ["claude"], displayNames: ["claude": "Claude Code"])
        #expect(nodes[0].title == "Fix login bug")
    }

    @Test func panesWithoutAgentAreOmitted() {
        let nodes = AgentTreeBuilder.build(
            tabs: makeTabs(),
            agentStatus: [:], paneAgents: [:],
            chatSubagents: [:], processTrees: [:],
            catalogIds: ["claude"], displayNames: [:])
        #expect(nodes.isEmpty)
    }

    @Test func chatSubagentsBecomeChildren() {
        let sub = ChatSubagentInput(id: "tool-1", title: "Explore repo", status: .running)
        let nodes = AgentTreeBuilder.build(
            tabs: makeTabs(),
            agentStatus: [Self.chatTabId: .running],
            paneAgents: [Self.chatTabId: "claude"],
            chatSubagents: [Self.chatTabId: [sub]],
            processTrees: [:],
            catalogIds: ["claude"], displayNames: ["claude": "Claude Code"])
        #expect(nodes.count == 1)
        #expect(nodes[0].children.count == 1)
        let child = nodes[0].children[0]
        #expect(child.id == "chat:\(Self.chatTabId.uuidString):tool:tool-1")
        #expect(child.title == "Explore repo")
        #expect(child.status == .running)
        #expect(child.kind == .subagent)
    }

    @Test func terminalSubagentsFromProcessTree() {
        // shell -> claude -> claude (subagent). Only catalog names below the
        // first matched agent become subagent nodes.
        let inner = ProcessNode(pid: 300, name: "claude", children: [])
        let agent = ProcessNode(pid: 200, name: "claude", children: [
            ProcessNode(pid: 250, name: "rg", children: [inner])
        ])
        let nodes = AgentTreeBuilder.build(
            tabs: makeTabs(),
            agentStatus: [Self.paneId: .running],
            paneAgents: [Self.paneId: "claude"],
            chatSubagents: [:],
            processTrees: [Self.paneId: [agent]],
            catalogIds: ["claude"], displayNames: ["claude": "Claude Code"])
        #expect(nodes.count == 1)
        #expect(nodes[0].children.count == 1)
        #expect(nodes[0].children[0].id == "term:\(Self.paneId.uuidString):pid:300")
        #expect(nodes[0].children[0].agentId == "claude")
        #expect(nodes[0].children[0].status == .running)
    }

    @Test func nonCatalogProcessesNeverAppear() {
        let agent = ProcessNode(pid: 200, name: "claude", children: [
            ProcessNode(pid: 250, name: "rg", children: []),
            ProcessNode(pid: 260, name: "node", children: []),
        ])
        let nodes = AgentTreeBuilder.build(
            tabs: makeTabs(),
            agentStatus: [Self.paneId: .running],
            paneAgents: [Self.paneId: "claude"],
            chatSubagents: [:],
            processTrees: [Self.paneId: [agent]],
            catalogIds: ["claude"], displayNames: [:])
        #expect(nodes[0].children.isEmpty)
    }

    @Test func doneAndErrorStatusesPassThrough() {
        let nodes = AgentTreeBuilder.build(
            tabs: makeTabs(),
            agentStatus: [Self.chatTabId: .done, Self.paneId: .error],
            paneAgents: [Self.chatTabId: "claude", Self.paneId: "codex"],
            chatSubagents: [:], processTrees: [:],
            catalogIds: ["claude", "codex"], displayNames: [:])
        #expect(nodes[0].status == .done)
        #expect(nodes[1].status == .error)
    }
}
