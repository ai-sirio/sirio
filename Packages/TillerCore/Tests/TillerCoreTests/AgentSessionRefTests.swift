import Testing
import Foundation
import TillerPersistence
@testable import TillerCore

@Test func agentSessionRefRoundTrip() async throws {
    let store = ProjectStore(database: try AppDatabase.inMemory())
    let p = try await store.addProject(name: "demo", rootPath: "/tmp/demo")
    let w = try await store.addWorktree(projectId: p.id, branch: "main", path: "/tmp/demo")
    let paneId = UUID()

    try await store.saveAgentSessionRef(paneId: paneId, worktreeId: w.id,
                                        agentId: "claude", sessionRef: "abc-123")
    let refs = try await store.agentSessionRefs(of: w.id)
    #expect(refs == [AgentSessionRef(paneId: paneId, agentId: "claude", sessionRef: "abc-123")])
}

@Test func saveAgentSessionRefUpsertsForSamePane() async throws {
    let store = ProjectStore(database: try AppDatabase.inMemory())
    let p = try await store.addProject(name: "demo", rootPath: "/tmp/demo")
    let w = try await store.addWorktree(projectId: p.id, branch: "main", path: "/tmp/demo")
    let paneId = UUID()

    try await store.saveAgentSessionRef(paneId: paneId, worktreeId: w.id,
                                        agentId: "claude", sessionRef: "old")
    try await store.saveAgentSessionRef(paneId: paneId, worktreeId: w.id,
                                        agentId: "claude", sessionRef: "new")
    let refs = try await store.agentSessionRefs(of: w.id)
    #expect(refs.count == 1)
    #expect(refs[0].sessionRef == "new")
}

@Test func deleteAgentSessionRefRemovesRow() async throws {
    let store = ProjectStore(database: try AppDatabase.inMemory())
    let p = try await store.addProject(name: "demo", rootPath: "/tmp/demo")
    let w = try await store.addWorktree(projectId: p.id, branch: "main", path: "/tmp/demo")
    let paneId = UUID()

    try await store.saveAgentSessionRef(paneId: paneId, worktreeId: w.id,
                                        agentId: "codex", sessionRef: "ref1")
    try await store.deleteAgentSessionRef(paneId: paneId)
    #expect(try await store.agentSessionRefs(of: w.id).isEmpty)
}

@Test func agentActivityExposesAgentIdPerPane() {
    let model = AgentActivityModel()
    let paneId = UUID()
    model.agentSpawned(paneId: paneId, agentId: "opencode", now: Date())
    #expect(model.agentId(paneId: paneId) == "opencode")
    #expect(model.agentId(paneId: UUID()) == nil)
}
