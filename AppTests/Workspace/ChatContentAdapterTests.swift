import AppKit
import Foundation
import Testing
import TillerCore
import TillerWorkspace

@testable import Tiller

/// Chat identity on the universal engine: a chat tab's ChatContentID *is* the
/// ChatSessionStore session id, so the tab can always find the conversation it
/// owns (and, through the session row, its agent). A chat tab that cannot get
/// a session id must not exist at all.
@MainActor
struct ChatContentAdapterTests {
    private func worktree() -> Worktree {
        Worktree(id: UUID(), projectId: UUID(), branch: "main", path: "/tmp/chat-adapter-tests")
    }

    /// Records what the adapter asked for, and answers with a fixed id.
    private final class SessionFactorySpy {
        private(set) var calls: [(worktreeID: UUID, agentID: String)] = []
        var result: Result<String, Error> = .success("session-fixed")

        func make(worktreeID: UUID, agentID: String) throws -> String {
            calls.append((worktreeID, agentID))
            return try result.get()
        }
    }

    private struct SessionFactoryFailure: Error {}

    @Test func newChatMintsTheContentIDFromTheCreatedSession() throws {
        let adapter = ChatContentAdapter()
        let spy = SessionFactorySpy()
        spy.result = .success("session-42")
        adapter.makeSession = spy.make

        let tab = try #require(
            adapter.prepareTab(request: .newChat(agentID: "claude-acp"), worktreeID: UUID()))

        guard case .chat(let contentID) = tab.content else {
            Issue.record("expected chat content")
            return
        }
        #expect(contentID.rawValue == "session-42")
    }

    @Test func newChatPassesTheWorktreeIDAndAgentIDToTheSessionFactory() throws {
        let adapter = ChatContentAdapter()
        let spy = SessionFactorySpy()
        adapter.makeSession = spy.make
        let worktreeID = UUID()

        _ = adapter.prepareTab(request: .newChat(agentID: "codex-acp"), worktreeID: worktreeID)

        #expect(spy.calls.count == 1)
        #expect(spy.calls.first?.worktreeID == worktreeID)
        #expect(spy.calls.first?.agentID == "codex-acp")
    }

    @Test func resumeChatReusesTheGivenContentIDWithoutCreatingASession() throws {
        let adapter = ChatContentAdapter()
        let spy = SessionFactorySpy()
        adapter.makeSession = spy.make

        let tab = try #require(
            adapter.prepareTab(request: .resumeChat(ChatContentID("old-session")),
                               worktreeID: UUID()))

        guard case .chat(let contentID) = tab.content else {
            Issue.record("expected chat content")
            return
        }
        #expect(contentID.rawValue == "old-session")
        #expect(spy.calls.isEmpty)
    }

    /// No store (fresh launch before the database opens, or a test harness):
    /// preparation fails, so WorkspaceCoordinator commits no tab, group,
    /// split, or focus change.
    @Test func newChatWithoutASessionFactoryCreatesNoTab() async {
        let adapter = ChatContentAdapter()

        #expect(adapter.prepareTab(request: .newChat(agentID: "claude-acp"),
                                   worktreeID: UUID()) == nil)
        await #expect(throws: (any Error).self) {
            try await adapter.prepare(request: .newChat(agentID: "claude-acp"),
                                      worktree: worktree())
        }
    }

    @Test func newChatWhoseSessionFactoryThrowsCreatesNoTab() async {
        let adapter = ChatContentAdapter()
        let spy = SessionFactorySpy()
        spy.result = .failure(SessionFactoryFailure())
        adapter.makeSession = spy.make

        #expect(adapter.prepareTab(request: .newChat(agentID: "claude-acp"),
                                   worktreeID: UUID()) == nil)
        await #expect(throws: (any Error).self) {
            try await adapter.prepare(request: .newChat(agentID: "claude-acp"),
                                      worktree: worktree())
        }
    }

    @Test func makeHostBuildsItsViewControllerOncePerTab() throws {
        let adapter = ChatContentAdapter()
        adapter.makeSession = { _, _ in "session-1" }
        var builds = 0
        adapter.makeContentViewController = { _, _, _ in
            builds += 1
            return NSViewController()
        }
        let tab = try #require(
            adapter.prepareTab(request: .newChat(agentID: "claude-acp"), worktreeID: UUID()))
        let worktree = worktree()

        let first = adapter.makeHost(tab: tab, worktree: worktree)
        let second = adapter.makeHost(tab: tab, worktree: worktree)

        #expect(builds == 1)
        #expect(first.viewController === second.viewController)
    }

    /// The legacy tree degrades a chat whose agent cannot be resolved to a
    /// visible "Agent not available" placeholder. A blank pane would read as
    /// a hang.
    @Test func makeHostStillReturnsAHostWhenNoViewControllerCanBeBuilt() throws {
        let adapter = ChatContentAdapter()
        adapter.makeSession = { _, _ in "session-1" }
        adapter.makeContentViewController = { _, _, _ in nil }
        let tab = try #require(
            adapter.prepareTab(request: .newChat(agentID: "claude-acp"), worktreeID: UUID()))

        let host = adapter.makeHost(tab: tab, worktree: worktree())

        #expect(host.tabID == tab.id)
    }

    /// Closing must reach the app's chat teardown (stop the agent, drop an
    /// empty session row) exactly once, and forget the cached view controller
    /// so a reopened tab does not show a dead transcript.
    @Test func closeReleasesTheContentExactlyOnce() async throws {
        let adapter = ChatContentAdapter()
        adapter.makeSession = { _, _ in "session-1" }
        adapter.makeContentViewController = { _, _, _ in NSViewController() }
        var released: [WorkspaceTabID] = []
        adapter.releaseContent = { released.append($0) }
        let tab = try #require(
            adapter.prepareTab(request: .newChat(agentID: "claude-acp"), worktreeID: UUID()))
        _ = adapter.makeHost(tab: tab, worktree: worktree())

        await adapter.close(tab: tab)
        await adapter.close(tab: tab)

        #expect(released == [tab.id])
    }

    /// Legacy openChatTab titles a fresh chat "Chat" and lets auto-rename
    /// replace it after the first turn. Titling it with the raw catalog id
    /// would leave "claude-acp" on screen forever for a chat that never
    /// completes a turn.
    @Test func newChatTabIsTitledChatSoAutoRenameCanReplaceIt() throws {
        let adapter = ChatContentAdapter()
        adapter.makeSession = { _, _ in "session-1" }

        let tab = try #require(
            adapter.prepareTab(request: .newChat(agentID: "claude-acp"), worktreeID: UUID()))

        #expect(tab.title == "Chat")
        #expect(tab.titleIsAutoNamed)
    }
}
