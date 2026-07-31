import Testing
import Foundation
import TillerPersistence
@testable import TillerCore

/// Refs are keyed by TerminalContentID, not by the live pane id: the pane id
/// is a ResourceGenerationID, minted fresh on every relaunch, so a ref stored
/// under it could never be matched again after a restart.
@Test func agentSessionRefRoundTrip() async throws {
    let store = ProjectStore(database: try AppDatabase.inMemory())
    let p = try await store.addProject(name: "demo", rootPath: "/tmp/demo")
    let w = try await store.addWorktree(projectId: p.id, branch: "main", path: "/tmp/demo")
    let contentID = TerminalContentID()

    try await store.saveAgentSessionRef(contentID: contentID, worktreeId: w.id,
                                        agentId: "claude", sessionRef: "abc-123")
    let refs = try await store.agentSessionRefs(of: w.id)
    #expect(refs == [AgentSessionRef(contentID: contentID, agentId: "claude",
                                     sessionRef: "abc-123")])
}

@Test func saveAgentSessionRefUpsertsForSameContent() async throws {
    let store = ProjectStore(database: try AppDatabase.inMemory())
    let p = try await store.addProject(name: "demo", rootPath: "/tmp/demo")
    let w = try await store.addWorktree(projectId: p.id, branch: "main", path: "/tmp/demo")
    let contentID = TerminalContentID()

    try await store.saveAgentSessionRef(contentID: contentID, worktreeId: w.id,
                                        agentId: "claude", sessionRef: "old")
    try await store.saveAgentSessionRef(contentID: contentID, worktreeId: w.id,
                                        agentId: "claude", sessionRef: "new")
    let refs = try await store.agentSessionRefs(of: w.id)
    #expect(refs.count == 1)
    #expect(refs[0].sessionRef == "new")
}

@Test func deleteAgentSessionRefRemovesRow() async throws {
    let store = ProjectStore(database: try AppDatabase.inMemory())
    let p = try await store.addProject(name: "demo", rootPath: "/tmp/demo")
    let w = try await store.addWorktree(projectId: p.id, branch: "main", path: "/tmp/demo")
    let contentID = TerminalContentID()

    try await store.saveAgentSessionRef(contentID: contentID, worktreeId: w.id,
                                        agentId: "codex", sessionRef: "ref1")
    try await store.deleteAgentSessionRef(contentID: contentID)
    #expect(try await store.agentSessionRefs(of: w.id).isEmpty)
}

/// Two terminals in the same worktree keep independent refs.
@Test func agentSessionRefsAreScopedPerTerminalContent() async throws {
    let store = ProjectStore(database: try AppDatabase.inMemory())
    let p = try await store.addProject(name: "demo", rootPath: "/tmp/demo")
    let w = try await store.addWorktree(projectId: p.id, branch: "main", path: "/tmp/demo")
    let first = TerminalContentID()
    let second = TerminalContentID()

    try await store.saveAgentSessionRef(contentID: first, worktreeId: w.id,
                                        agentId: "claude", sessionRef: "one")
    try await store.saveAgentSessionRef(contentID: second, worktreeId: w.id,
                                        agentId: "codex", sessionRef: "two")
    try await store.deleteAgentSessionRef(contentID: first)

    let refs = try await store.agentSessionRefs(of: w.id)
    #expect(refs == [AgentSessionRef(contentID: second, agentId: "codex", sessionRef: "two")])
}

@Test func agentActivityExposesAgentIdPerPane() {
    let model = AgentActivityModel()
    let paneId = UUID()
    model.agentSpawned(paneId: paneId, agentId: "opencode", now: Date())
    #expect(model.agentId(paneId: paneId) == "opencode")
    #expect(model.agentId(paneId: UUID()) == nil)
}
