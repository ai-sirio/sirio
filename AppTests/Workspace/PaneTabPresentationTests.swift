import Foundation
import Testing
import TillerCore
import TillerWorkspace
@testable import Tiller

@Suite @MainActor
struct PaneTabPresentationTests {
    @Test func terminalPresentationUsesTheLivePaneForAgentState() {
        let tabID = WorkspaceTabID()
        let contentID = TerminalContentID()
        let livePane = UUID()
        let entry = TabMenuEntry(
            tabID: tabID, title: "Codex", isActive: true,
            content: .terminal(contentID))
        let resolver = makeResolver(
            terminalPane: livePane,
            statuses: [livePane: .running],
            agents: [livePane: "codex"])

        let result = resolver.resolve(entry)

        #expect(result.icon == .terminal(agentID: "codex"))
        #expect(result.agentStatus == .running)
        #expect(!result.isDirty)
    }

    @Test func chatUsesItsTabIDAsTheActivityPaneID() {
        let tabID = WorkspaceTabID()
        let entry = TabMenuEntry(
            tabID: tabID, title: "Review", isActive: false,
            content: .chat(ChatContentID("session")))
        let resolver = makeResolver(
            statuses: [tabID.rawValue: .needsInput],
            agents: [tabID.rawValue: "claude"])

        let result = resolver.resolve(entry)

        #expect(result.icon == .chat(agentID: "claude"))
        #expect(result.agentStatus == .needsInput)
    }

    @Test func documentPresentationCarriesEditorAndDirtyState() {
        let tabID = WorkspaceTabID()
        let entry = TabMenuEntry(
            tabID: tabID, title: "README.md", isActive: false,
            content: .document(
                DocumentID.makeCanonical(
                    worktreeID: UUID(), path: "/tmp/README.md"),
                editor: .markdown))
        let resolver = makeResolver(dirtyTabs: [tabID])

        let result = resolver.resolve(entry)

        #expect(result.icon == .document(.markdown))
        #expect(result.isDirty)
        #expect(result.accessibilityLabel(for: entry).contains("modified"))
    }

    @Test func accessibilityNamesSelectionAndAgentStatus() {
        let tabID = WorkspaceTabID()
        let entry = TabMenuEntry(
            tabID: tabID, title: "Fix tests", isActive: true,
            content: .chat(ChatContentID("chat")))
        let resolver = makeResolver(statuses: [tabID.rawValue: .error])

        let label = resolver.resolve(entry).accessibilityLabel(for: entry)

        #expect(label == "Chat, Fix tests, selected, failed")
    }

    private func makeResolver(
        terminalPane: UUID? = nil,
        dirtyTabs: Set<WorkspaceTabID> = [],
        statuses: [UUID: AgentStatus] = [:],
        agents: [UUID: String] = [:]
    ) -> PaneTabPresentationResolver {
        PaneTabPresentationResolver(
            isDirty: { dirtyTabs.contains($0) },
            liveTerminalPane: { _ in terminalPane },
            status: { ids in AgentStatus.highestPriority(in: ids.compactMap { statuses[$0] }) },
            agentID: { ids in ids.compactMap { agents[$0] }.first })
    }
}
