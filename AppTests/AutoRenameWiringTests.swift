import Foundation
import Testing
import TillerCore
import TillerTerminal

@testable import Tiller

@Suite("AutoRenameWiringTests", .serialized)
@MainActor
struct AutoRenameWiringTests {
    @Test func autoNamingSettingDefaultsToDisabled() {
        #expect(AppSettings.autoNamingEnabled(defaultsValue: nil) == false)
        #expect(AppSettings.autoNamingEnabled(defaultsValue: true) == true)
        #expect(AppSettings.autoNamingEnabled(defaultsValue: false) == false)
    }

    @Test func autoRenameSkipsWhenSettingDisabled() async {
        UserDefaults.standard.removeObject(forKey: AppSettings.autoNamingEnabledKey)
        let (model, worktree, tab, controller) = makeChatTab()
        let originalTitle = tab.title

        controller.onStatusChange?(.running)
        controller.onStatusChange?(.needsInput)
        await Task.yield()

        let updated = model.tabs[worktree.id]!.first { $0.id == tab.id }!
        #expect(updated.title == originalTitle)
    }

    @Test func autoRenameSkipsUnrelatedTransition() async {
        UserDefaults.standard.set(true, forKey: AppSettings.autoNamingEnabledKey)
        defer { UserDefaults.standard.removeObject(forKey: AppSettings.autoNamingEnabledKey) }
        let (model, worktree, tab, controller) = makeChatTab()
        let originalTitle = tab.title

        controller.onStatusChange?(.error)
        await Task.yield()

        let updated = model.tabs[worktree.id]!.first { $0.id == tab.id }!
        #expect(updated.title == originalTitle)
    }

    @Test func autoRenameSkipsManuallyRenamedTab() async {
        UserDefaults.standard.set(true, forKey: AppSettings.autoNamingEnabledKey)
        defer { UserDefaults.standard.removeObject(forKey: AppSettings.autoNamingEnabledKey) }
        let (model, worktree, tab, controller) = makeChatTab()
        model.renameTab(tab.id, in: worktree.id, to: "Kept by user")

        controller.onStatusChange?(.running)
        controller.onStatusChange?(.needsInput)
        await Task.yield()

        let updated = model.tabs[worktree.id]!.first { $0.id == tab.id }!
        #expect(updated.title == "Kept by user")
    }

    @Test func autoApplyChangesEligibleTitle() {
        let (model, worktree, tab, _) = makeChatTab()

        model.applyAutoTitle(tab.id, in: worktree.id, title: "Fix login bug")

        let updated = model.tabs[worktree.id]!.first { $0.id == tab.id }!
        #expect(updated.title == "Fix login bug")
        #expect(updated.titleIsAutoNamed == true)
    }

    private func makeChatTab() -> (AppModel, Worktree, WorkspaceTab, ChatController) {
        let model = AppModel(paneRegistry: PaneRegistry(), registrationTimeoutMs: 100)
        let worktree = Worktree(
            id: UUID(), projectId: UUID(), branch: "main", path: "/tmp/auto-rename-wiring")
        model.worktrees = [worktree.projectId: [worktree]]
        let tab = model.openChatTab(agentId: "claude", in: worktree)!
        let controller = model.chatController(for: tab, in: worktree)!
        return (model, worktree, tab, controller)
    }
}
