import Foundation
import Testing
import TillerCore
import TillerTerminal

@testable import Tiller

@MainActor
struct OpenChatTabTests {
    @Test func openChatTabUsesGivenAgentId() {
        let suiteName = "OpenChatTabTests-\(UUID().uuidString)"
        let defaults = UserDefaults(suiteName: suiteName)!
        defaults.removePersistentDomain(forName: suiteName)
        let model = AppModel(
            paneRegistry: PaneRegistry(), registrationTimeoutMs: 100, defaults: defaults)
        let worktree = Worktree(
            id: UUID(), projectId: UUID(), branch: "main", path: "/tmp/open-chat-tab")
        model.worktrees = [worktree.projectId: [worktree]]

        let tab = model.openChatTab(agentId: "codex-acp", in: worktree)

        #expect(tab?.chatAgentId == "codex-acp")
        defaults.removePersistentDomain(forName: suiteName)
    }
}
