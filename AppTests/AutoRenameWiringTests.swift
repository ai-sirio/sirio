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
        let (model, worktree, tab, controller) = makeChatTab()
        let originalTitle = tab.title

        controller.onStatusChange?(.running)
        controller.onStatusChange?(.needsInput)
        await Task.yield()

        let updated = model.workspaceTabs(for: worktree.id).first { $0.id == tab.id }!
        #expect(updated.title == originalTitle)
    }

    @Test func autoRenameSkipsUnrelatedTransition() async {
        let (model, worktree, tab, controller) = makeChatTab(autoNamingEnabled: true)
        let originalTitle = tab.title

        controller.onStatusChange?(.error)
        await Task.yield()

        let updated = model.workspaceTabs(for: worktree.id).first { $0.id == tab.id }!
        #expect(updated.title == originalTitle)
    }

    @Test func autoRenameSkipsManuallyRenamedTab() async {
        let (model, worktree, tab, controller) = makeChatTab(autoNamingEnabled: true)
        model.renameTab(tab.id, in: worktree.id, to: "Kept by user")

        controller.onStatusChange?(.running)
        controller.onStatusChange?(.needsInput)
        await Task.yield()

        let updated = model.workspaceTabs(for: worktree.id).first { $0.id == tab.id }!
        #expect(updated.title == "Kept by user")
    }

    @Test func autoApplyChangesEligibleTitle() {
        let (model, worktree, tab, _) = makeChatTab()

        model.applyAutoTitle(tab.id, in: worktree.id, title: "Fix login bug")

        let updated = model.workspaceTabs(for: worktree.id).first { $0.id == tab.id }!
        #expect(updated.title == "Fix login bug")
        #expect(updated.titleIsAutoNamed == true)
    }

    /// Usa una suite UserDefaults effimera: i test NON devono mai toccare
    /// UserDefaults.standard — il bundle di test è ospitato dentro Tiller.app,
    /// quindi .standard è il dominio reale dev.tiller.Tiller dell'utente.
    private func makeChatTab(
        autoNamingEnabled: Bool? = nil
    ) -> (AppModel, Worktree, LegacyWorkspaceTab, ChatController) {
        let suiteName = "dev.tiller.Tiller.AutoRenameWiringTests"
        let defaults = UserDefaults(suiteName: suiteName)!
        defaults.removePersistentDomain(forName: suiteName)
        if let autoNamingEnabled {
            defaults.set(autoNamingEnabled, forKey: AppSettings.autoNamingEnabledKey)
        }
        let model = AppModel(
            paneRegistry: PaneRegistry(), registrationTimeoutMs: 100, defaults: defaults)
        let worktree = Worktree(
            id: UUID(), projectId: UUID(), branch: "main", path: "/tmp/auto-rename-wiring")
        model.worktrees = [worktree.projectId: [worktree]]
        let tab = model.openChatTab(agentId: "claude-acp", in: worktree)!
        let controller = model.chatController(for: tab, in: worktree)!
        return (model, worktree, tab, controller)
    }
}
