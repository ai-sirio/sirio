import Foundation
import TillerCore
import TillerTerminal

@MainActor
final class LegacyWorkspaceStore {
    private(set) var tabs: [UUID: [LegacyWorkspaceTab]] = [:]
    private(set) var activeTabIDs: [UUID: UUID] = [:]
    private(set) var paneCaches: [UUID: TerminalPaneCache] = [:]
    private(set) var mutationRevisions: [UUID: Int] = [:]

    func tabs(for worktreeID: UUID) -> [LegacyWorkspaceTab] {
        tabs[worktreeID] ?? []
    }

    func activeTabID(for worktreeID: UUID) -> UUID? {
        activeTabIDs[worktreeID]
    }

    func replaceTabs(_ value: [LegacyWorkspaceTab], for worktreeID: UUID) {
        tabs[worktreeID] = value
        markMutated(worktreeID)
    }

    func appendTab(_ tab: LegacyWorkspaceTab, to worktreeID: UUID, activate: Bool) {
        tabs[worktreeID, default: []].append(tab)
        if activate { activeTabIDs[worktreeID] = tab.id }
        markMutated(worktreeID)
    }

    func setActiveTabID(_ tabID: UUID?, for worktreeID: UUID) {
        activeTabIDs[worktreeID] = tabID
        markMutated(worktreeID)
    }

    func removeWorktree(_ worktreeID: UUID) {
        tabs[worktreeID] = nil
        activeTabIDs[worktreeID] = nil
        paneCaches[worktreeID] = nil
        markMutated(worktreeID)
    }

    func paneCache(for worktreeID: UUID) -> TerminalPaneCache {
        if let cache = paneCaches[worktreeID] { return cache }
        let cache = TerminalPaneCache()
        paneCaches[worktreeID] = cache
        return cache
    }

    func markMutated(_ worktreeID: UUID) {
        mutationRevisions[worktreeID, default: 0] += 1
    }

    func mutationRevision(for worktreeID: UUID) -> Int {
        mutationRevisions[worktreeID] ?? 0
    }
}
