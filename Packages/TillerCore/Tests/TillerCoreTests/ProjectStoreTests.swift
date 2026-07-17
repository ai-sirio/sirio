import Testing
import Foundation
import TillerPersistence
@testable import TillerCore

@Test func addAndLoadProjects() async throws {
    let store = ProjectStore(database: try AppDatabase.inMemory())
    let p = try await store.addProject(name: "demo", rootPath: "/tmp/demo")
    let all = try await store.loadAll()
    #expect(all == [p])
}

@Test func worktreeLifecycle() async throws {
    let store = ProjectStore(database: try AppDatabase.inMemory())
    let p = try await store.addProject(name: "demo", rootPath: "/tmp/demo")
    let w = try await store.addWorktree(projectId: p.id, branch: "feat", path: "/tmp/demo-feat")
    #expect(try await store.worktrees(of: p.id) == [w])
    try await store.removeWorktree(w.id)
    #expect(try await store.worktrees(of: p.id).isEmpty)
}

@Test func removingProjectRemovesItsWorktrees() async throws {
    let store = ProjectStore(database: try AppDatabase.inMemory())
    let p = try await store.addProject(name: "demo", rootPath: "/tmp/demo")
    _ = try await store.addWorktree(projectId: p.id, branch: "feat", path: "/tmp/demo-feat")
    try await store.removeProject(p.id)
    #expect(try await store.loadAll().isEmpty)
    #expect(try await store.worktrees(of: p.id).isEmpty)
}
 
@Test func loadAndWorktreesSkipCorruptedIds() async throws {
    let db = try AppDatabase.inMemory()
    let store = ProjectStore(database: db)
 
    // Insert a project with a non-UUID id directly via the database seam
    try db.write { db in
        try ProjectRecord(id: "not-a-uuid", name: "bad", rootPath: "/tmp", createdAt: Date()).insert(db)
    }
 
    // loadAll skips the corrupted row
    let all = try await store.loadAll()
    #expect(all.isEmpty, "loadAll should skip rows with non-UUID ids")
 
    // Insert a valid project and a worktree with a non-UUID id under it
    let goodId = UUID()
    try db.write { db in
        try ProjectRecord(id: goodId.uuidString, name: "good", rootPath: "/tmp", createdAt: Date()).insert(db)
        try WorktreeRecord(id: "bad-worktree-uuid", projectId: goodId.uuidString, branch: "main", path: "/tmp", createdAt: Date()).insert(db)
    }
 
    // loadAll returns the valid project (skip bad project, include good)
    let all2 = try await store.loadAll()
    #expect(all2.count == 1)
    #expect(all2[0].name == "good")
 
    // worktrees(of:) skips the corrupted worktree row
    let worktrees = try await store.worktrees(of: goodId)
    #expect(worktrees.isEmpty, "worktrees should skip rows with non-UUID ids")
}
 
 // MARK: - Data layer v2
 
 @Test func commentRoundTrip() async throws {
     let db = try AppDatabase.inMemory()
     let store = ProjectStore(database: db)
     let project = try await store.addProject(name: "p", rootPath: "/tmp/p")
     let wt = try await store.addWorktree(projectId: project.id, branch: "main", path: "/tmp/p")
 
     try await store.setWorktreeComment(wt.id, comment: "fix in progress")
     let reloaded = try await store.worktrees(of: project.id)
     #expect(reloaded.first?.comment == "fix in progress")
     #expect(reloaded.first?.commentUpdatedAt != nil)
 }
 
 @Test func primaryIsExclusivePerProject() async throws {
     let db = try AppDatabase.inMemory()
     let store = ProjectStore(database: db)
     let project = try await store.addProject(name: "p", rootPath: "/tmp/p")
     let a = try await store.addWorktree(projectId: project.id, branch: "a", path: "/tmp/a")
     let b = try await store.addWorktree(projectId: project.id, branch: "b", path: "/tmp/b")
 
     try await store.setWorktreePrimary(a.id, isPrimary: true)
     try await store.setWorktreePrimary(b.id, isPrimary: true)
     let reloaded = try await store.worktrees(of: project.id)
     #expect(reloaded.first(where: { $0.id == a.id })?.isPrimary == false)
     #expect(reloaded.first(where: { $0.id == b.id })?.isPrimary == true)
 }
 
 @Test func worktreeLookupByPath() async throws {
     let db = try AppDatabase.inMemory()
     let store = ProjectStore(database: db)
     let project = try await store.addProject(name: "p", rootPath: "/tmp/p")
     let wt = try await store.addWorktree(projectId: project.id, branch: "main", path: "/tmp/p/main")
 
     #expect(try await store.worktree(byPath: "/tmp/p/main")?.id == wt.id)
     #expect(try await store.worktree(byPath: "/nope") == nil)
 }

// MARK: - Tab persistence (v3)

@Test func tabsRoundTripPreservingOrderActiveAndTree() async throws {
    let db = try AppDatabase.inMemory()
    let store = ProjectStore(database: db)
    let project = try await store.addProject(name: "p", rootPath: "/tmp/p")
    let worktree = try await store.addWorktree(projectId: project.id, branch: "main", path: "/tmp/p")
    let paneA = UUID(), paneB = UUID(), paneC = UUID()
    let tabs = [
        WorkspaceTab(id: UUID(), title: "Terminale 1", tree: .leaf(id: paneA)),
        WorkspaceTab(
            id: UUID(), title: "Claude Code",
            tree: SplitTree.leaf(id: paneB)
                .splitting(leaf: paneB, axis: .vertical, newLeaf: paneC)
        )
    ]
    try await store.saveTabs(worktreeId: worktree.id, tabs: tabs, activeTabId: tabs[1].id)
    let loaded = try await store.loadTabs(of: worktree.id)
    #expect(loaded.tabs == tabs)
    #expect(loaded.activeTabId == tabs[1].id)
}

@Test func saveTabsReplacesPreviousList() async throws {
    let db = try AppDatabase.inMemory()
    let store = ProjectStore(database: db)
    let project = try await store.addProject(name: "p", rootPath: "/tmp/p")
    let worktree = try await store.addWorktree(projectId: project.id, branch: "main", path: "/tmp/p")
    let first = [WorkspaceTab(id: UUID(), title: "Terminale 1", tree: .leaf(id: UUID()))]
    try await store.saveTabs(worktreeId: worktree.id, tabs: first, activeTabId: first[0].id)
    let second = [WorkspaceTab(id: UUID(), title: "Codex", tree: .leaf(id: UUID()))]
    try await store.saveTabs(worktreeId: worktree.id, tabs: second, activeTabId: second[0].id)
    let loaded = try await store.loadTabs(of: worktree.id)
    #expect(loaded.tabs == second)
}

@Test func removingWorktreeCascadesTabRows() async throws {
    let db = try AppDatabase.inMemory()
    let store = ProjectStore(database: db)
    let project = try await store.addProject(name: "p", rootPath: "/tmp/p")
    let worktree = try await store.addWorktree(projectId: project.id, branch: "main", path: "/tmp/p")
    let tabs = [WorkspaceTab(id: UUID(), title: "Terminale 1", tree: .leaf(id: UUID()))]
    try await store.saveTabs(worktreeId: worktree.id, tabs: tabs, activeTabId: tabs[0].id)
    try await store.removeWorktree(worktree.id)
    let loaded = try await store.loadTabs(of: worktree.id)
    #expect(loaded.tabs.isEmpty)
}

// MARK: - Project settings (v4)

@Test func projectSettingsRoundTrip() async throws {
    let db = try AppDatabase.inMemory()
    let store = ProjectStore(database: db)
    let project = try await store.addProject(name: "demo", rootPath: "/tmp/demo")

    try await store.setProjectDisplayName(project.id, displayName: "My Demo")
    try await store.setProjectColor(project.id, colorHex: "#FF0000")
    try await store.setProjectIcon(project.id, kind: .emoji, value: "🚀", avatarImage: nil)
    try await store.setProjectWorktreeBase(project.id, branch: "develop")
    try await store.setProjectWorktreeLocation(project.id, path: "/tmp/custom")

    let reloaded = try await store.loadAll().first { $0.id == project.id }
    #expect(reloaded?.displayName == "My Demo")
    #expect(reloaded?.colorHex == "#FF0000")
    #expect(reloaded?.iconKind == .emoji)
    #expect(reloaded?.iconValue == "🚀")
    #expect(reloaded?.defaultWorktreeBase == "develop")
    #expect(reloaded?.worktreeLocationOverride == "/tmp/custom")
}

@Test func settingIconToAvatarClearsPreviousEmojiValue() async throws {
    let db = try AppDatabase.inMemory()
    let store = ProjectStore(database: db)
    let project = try await store.addProject(name: "demo", rootPath: "/tmp/demo")
    try await store.setProjectIcon(project.id, kind: .emoji, value: "🚀", avatarImage: nil)

    let pngBytes = Data([0x89, 0x50, 0x4E, 0x47])
    try await store.setProjectIcon(project.id, kind: .avatar, value: nil, avatarImage: pngBytes)

    let reloaded = try await store.loadAll().first { $0.id == project.id }
    #expect(reloaded?.iconKind == .avatar)
    #expect(reloaded?.iconValue == nil)
    #expect(reloaded?.avatarImage == pngBytes)
}


@Test func setWorktreeBranchUpdatesRow() async throws {
    let store = ProjectStore(database: try AppDatabase.inMemory())
    let p = try await store.addProject(name: "demo", rootPath: "/tmp/demo")
    let w = try await store.addWorktree(projectId: p.id, branch: "main", path: "/tmp/demo")
    try await store.setWorktreeBranch(w.id, branch: "master")
    #expect(try await store.worktrees(of: p.id).first?.branch == "master")
}

@Test func saveAndLoadEmptyTabList() async throws {
    let db = try AppDatabase.inMemory()
    let store = ProjectStore(database: db)
    let project = try await store.addProject(name: "p", rootPath: "/tmp/p")
    let worktree = try await store.addWorktree(projectId: project.id, branch: "main", path: "/tmp/p")
    try await store.saveTabs(worktreeId: worktree.id, tabs: [], activeTabId: nil)
    let loaded = try await store.loadTabs(of: worktree.id)
    #expect(loaded.tabs.isEmpty)
    #expect(loaded.activeTabId == nil)
}
