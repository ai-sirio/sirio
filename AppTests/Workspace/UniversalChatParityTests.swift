import Foundation
import Testing
import TillerCore
import TillerWorkspace

@testable import Tiller

/// Behaviours the legacy chat path had that the universal engine must keep.
/// Each of these fails silently rather than loudly if it regresses, which is
/// why they are pinned here rather than left to the manual checklist.
@Suite("UniversalChatParityTests", .serialized)
@MainActor
struct UniversalChatParityTests {
    private func fixture(_ name: String) async throws -> UniversalChatFixture.Handle {
        try await UniversalChatFixture.make(
            suiteName: "dev.tiller.Tiller.UniversalChatParityTests.\(name)")
    }

    private func chatAdapter(_ handle: UniversalChatFixture.Handle) -> ChatContentAdapter? {
        handle.model.workspaceCoordinator.adapters[.chat] as? ChatContentAdapter
    }

    /// LC-3, and the highest-value test here. A chat tab coming back from a
    /// previous run must not build a controller, because the controller is
    /// what launches the agent. If this regresses, the symptom is every agent
    /// in every restored worktree starting at app launch — it throws no error
    /// and reads only as a slow, busy startup, so nothing else would catch it.
    ///
    /// Opening a chat *is* eager by contrast, and correctly so: the user asked
    /// for it. The distinction is what this pins.
    @Test func aRestoredChatTabBuildsNoControllerUntilItIsViewed() async throws {
        let handle = try await fixture("restoredLazy")
        defer { handle.cleanUp() }
        // A real session row, so restore reaches the code that resolves the
        // tab's agent. Without it the restore path bails out early and this
        // test would pass without ever exercising anything.
        let session = try #require(
            try handle.model.chatStore?.createSession(
                worktreeId: handle.worktree.id.uuidString, agentId: "claude-acp"))
        let tab = WorkspaceTab(
            id: WorkspaceTabID(), title: "Chat", titleIsAutoNamed: true,
            content: .chat(ChatContentID(session.id)))
        let groupID = PaneGroupID()
        guard case .success(let layout) = WorkspaceLayout.make(
            root: .group(groupID),
            groups: [groupID: PaneGroup(id: groupID, tabs: [tab], activeTabID: tab.id)],
            activeGroupID: groupID
        ) else {
            Issue.record("could not build the restored layout")
            return
        }
        handle.persistence.restored = RestoredWorkspace(
            layout: layout, tabs: [tab.id: tab], revision: 1, diagnostics: [])

        await handle.model.workspaceCoordinator.restore(worktree: handle.worktree)
        handle.model.registerRestoredChatAgents(in: handle.worktree.id)

        #expect(UniversalChatFixture.tabs(handle).contains { $0.id == tab.id })
        // Identity is registered — proving the restore path ran in full — but
        // no controller exists, so no agent can have been launched.
        #expect(handle.model.agentActivity.agentId(paneId: tab.id.rawValue) == "claude-acp")
        #expect(handle.model.chatControllers.isEmpty)
    }

    /// Closing must reach the app's chat teardown through the adapter's
    /// releaseContent hook, not only through the legacy closeTab body.
    @Test func closingAChatTabTearsDownItsController() async throws {
        let handle = try await fixture("teardown")
        defer { handle.cleanUp() }
        let tab = try #require(await UniversalChatFixture.openChatTab(handle))
        _ = try #require(handle.model.chatController(for: tab, in: handle.worktree))
        #expect(handle.model.chatControllers[tab.id.rawValue] != nil)

        handle.model.closeTab(tab.id.rawValue, in: handle.worktree)
        await UniversalChatFixture.settle(handle) {
            handle.model.chatControllers[tab.id.rawValue] == nil
        }

        #expect(handle.model.chatControllers[tab.id.rawValue] == nil)
        #expect(UniversalChatFixture.tabs(handle).isEmpty)
    }

    /// A chat closed before its first turn leaves a history row nothing can
    /// show, so teardown drops it.
    @Test func closingAChatTabDeletesItsEmptySessionRow() async throws {
        let handle = try await fixture("emptySession")
        defer { handle.cleanUp() }
        let tab = try #require(await UniversalChatFixture.openChatTab(handle))
        guard case .chat(let contentID) = tab.content else {
            Issue.record("expected a chat tab")
            return
        }
        _ = try #require(handle.model.chatController(for: tab, in: handle.worktree))
        #expect(handle.model.chatSession(id: contentID.rawValue) != nil)

        handle.model.closeTab(tab.id.rawValue, in: handle.worktree)
        await UniversalChatFixture.settle(handle) {
            handle.model.chatSession(id: contentID.rawValue) == nil
        }

        #expect(handle.model.chatSession(id: contentID.rawValue) == nil)
    }

    /// Deleting from the history menu closes the tab showing that
    /// conversation, so no pane is left rendering a transcript that is gone.
    @Test func deletingASessionClosesTheTabShowingIt() async throws {
        let handle = try await fixture("deleteSession")
        defer { handle.cleanUp() }
        let tab = try #require(await UniversalChatFixture.openChatTab(handle))
        guard case .chat(let contentID) = tab.content else {
            Issue.record("expected a chat tab")
            return
        }

        handle.model.deleteChatSession(sessionId: contentID.rawValue, in: handle.worktree)
        await UniversalChatFixture.settle(handle) {
            UniversalChatFixture.tabs(handle).isEmpty
        }

        #expect(UniversalChatFixture.tabs(handle).isEmpty)
        #expect(handle.model.chatSession(id: contentID.rawValue) == nil)
    }

    /// Reopening a conversation already on screen focuses it instead of
    /// creating a second tab bound to the same session.
    @Test func reopeningAnOpenConversationDoesNotDuplicateItsTab() async throws {
        let handle = try await fixture("noDuplicate")
        defer { handle.cleanUp() }
        let tab = try #require(await UniversalChatFixture.openChatTab(handle))
        guard case .chat(let contentID) = tab.content else {
            Issue.record("expected a chat tab")
            return
        }

        handle.model.openExistingChatSession(
            sessionId: contentID.rawValue, in: handle.worktree)
        try await Task.sleep(for: .milliseconds(100))

        #expect(UniversalChatFixture.tabs(handle).count == 1)
    }

    /// The agent a chat belongs to is registered for the Agents panel and the
    /// status badges as soon as its controller resolves the session row.
    @Test func buildingAChatControllerRegistersItsAgentIdentity() async throws {
        let handle = try await fixture("identity")
        defer { handle.cleanUp() }
        let tab = try #require(
            await UniversalChatFixture.openChatTab(handle, agentId: "codex-acp"))

        _ = try #require(handle.model.chatController(for: tab, in: handle.worktree))

        #expect(handle.model.agentActivity.agentId(paneId: tab.id.rawValue) == "codex-acp")
    }

    /// The adapter mints one session per fresh chat: two new chats must not
    /// end up sharing a conversation.
    @Test func twoFreshChatsOwnDistinctSessions() async throws {
        let handle = try await fixture("distinctSessions")
        defer { handle.cleanUp() }

        let first = try #require(await UniversalChatFixture.openChatTab(handle))
        let second = try #require(await UniversalChatFixture.openChatTab(handle))

        guard case .chat(let firstID) = first.content,
              case .chat(let secondID) = second.content else {
            Issue.record("expected chat tabs")
            return
        }
        #expect(firstID != secondID)
        #expect(UniversalChatFixture.tabs(handle).count == 2)
    }
}
