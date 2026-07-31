import Foundation
import Testing
import TillerCore
import TillerTerminal
import TillerWorkspace

@testable import Tiller

@Suite("AutoRenameWiringTests", .serialized)
@MainActor
struct AutoRenameWiringTests {
    @Test func autoNamingSettingDefaultsToDisabled() {
        #expect(AppSettings.autoNamingEnabled(defaultsValue: nil) == false)
        #expect(AppSettings.autoNamingEnabled(defaultsValue: true) == true)
        #expect(AppSettings.autoNamingEnabled(defaultsValue: false) == false)
    }

    @Test func autoRenameSkipsWhenSettingDisabled() async throws {
        let (fixture, tab, controller) = try await makeChatTab()
        defer { fixture.cleanUp() }
        let originalTitle = tab.title

        controller.onStatusChange?(.running)
        controller.onStatusChange?(.needsInput)
        try await Task.sleep(for: .milliseconds(50))

        #expect(UniversalChatFixture.tab(fixture, id: tab.id)?.title == originalTitle)
    }

    @Test func autoRenameSkipsUnrelatedTransition() async throws {
        let (fixture, tab, controller) = try await makeChatTab(autoNamingEnabled: true)
        defer { fixture.cleanUp() }
        let originalTitle = tab.title

        controller.onStatusChange?(.error)
        try await Task.sleep(for: .milliseconds(50))

        #expect(UniversalChatFixture.tab(fixture, id: tab.id)?.title == originalTitle)
    }

    @Test func autoRenameSkipsManuallyRenamedTab() async throws {
        let (fixture, tab, controller) = try await makeChatTab(autoNamingEnabled: true)
        defer { fixture.cleanUp() }
        fixture.model.renameTab(tab.id.rawValue, in: fixture.worktree.id, to: "Kept by user")
        await UniversalChatFixture.settle(fixture) {
            UniversalChatFixture.tab(fixture, id: tab.id)?.title == "Kept by user"
        }

        controller.onStatusChange?(.running)
        controller.onStatusChange?(.needsInput)
        try await Task.sleep(for: .milliseconds(50))

        #expect(UniversalChatFixture.tab(fixture, id: tab.id)?.title == "Kept by user")
        #expect(UniversalChatFixture.tab(fixture, id: tab.id)?.titleIsAutoNamed == false)
    }

    @Test func autoApplyChangesEligibleTitle() async throws {
        let (fixture, tab, _) = try await makeChatTab()
        defer { fixture.cleanUp() }

        fixture.model.applyAutoTitle(
            tab.id.rawValue, in: fixture.worktree.id, title: "Fix login bug")
        await UniversalChatFixture.settle(fixture) {
            UniversalChatFixture.tab(fixture, id: tab.id)?.title == "Fix login bug"
        }

        let updated = try #require(UniversalChatFixture.tab(fixture, id: tab.id))
        #expect(updated.title == "Fix login bug")
        #expect(updated.titleIsAutoNamed == true)
    }

    /// The history row outlives the tab, so an auto-title has to reach the
    /// session as well — otherwise reopening the conversation shows "Chat".
    @Test func autoApplyAlsoTitlesTheChatSession() async throws {
        let (fixture, tab, _) = try await makeChatTab()
        defer { fixture.cleanUp() }
        guard case .chat(let contentID) = tab.content else {
            Issue.record("expected a chat tab")
            return
        }

        fixture.model.applyAutoTitle(
            tab.id.rawValue, in: fixture.worktree.id, title: "Fix login bug")

        #expect(fixture.model.chatSession(id: contentID.rawValue)?.title == "Fix login bug")
    }

    /// Uses an ephemeral UserDefaults suite: tests must never touch
    /// UserDefaults.standard — the test bundle is hosted inside Tiller.app,
    /// so .standard is the user's real dev.tiller.Tiller domain.
    private func makeChatTab(
        autoNamingEnabled: Bool? = nil
    ) async throws -> (UniversalChatFixture.Handle, WorkspaceTab, ChatController) {
        let fixture = try await UniversalChatFixture.make(
            suiteName: "dev.tiller.Tiller.AutoRenameWiringTests",
            configureDefaults: { defaults in
                if let autoNamingEnabled {
                    defaults.set(autoNamingEnabled, forKey: AppSettings.autoNamingEnabledKey)
                }
            })
        let tab = try #require(await UniversalChatFixture.openChatTab(fixture))
        let controller = try #require(
            fixture.model.chatController(for: tab, in: fixture.worktree))
        return (fixture, tab, controller)
    }
}
