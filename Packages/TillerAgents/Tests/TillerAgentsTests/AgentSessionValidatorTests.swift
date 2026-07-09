import Testing
import Foundation
@testable import TillerAgents

private func makeTempDir() throws -> URL {
    let dir = FileManager.default.temporaryDirectory
        .appendingPathComponent(UUID().uuidString)
    try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
    return dir
}

@Test func claudeSlugReplacesNonAlphanumerics() {
    #expect(AgentSessionValidator.claudeProjectSlug("/Users/x/my.proj")
        == "-Users-x-my-proj")
}

@Test func claudeValidWhenSessionFileExists() throws {
    let configDir = try makeTempDir()
    defer { try? FileManager.default.removeItem(at: configDir) }
    let slug = AgentSessionValidator.claudeProjectSlug("/tmp/demo")
    let sessionDir = configDir.appendingPathComponent("projects/\(slug)")
    try FileManager.default.createDirectory(at: sessionDir, withIntermediateDirectories: true)
    try Data().write(to: sessionDir.appendingPathComponent("abc-123.jsonl"))

    #expect(AgentSessionValidator.isLikelyValid(
        agentId: "claude", sessionRef: "abc-123", worktreePath: "/tmp/demo",
        claudeConfigDir: configDir.path, codexHome: "/nonexistent") == true)
    #expect(AgentSessionValidator.isLikelyValid(
        agentId: "claude", sessionRef: "missing", worktreePath: "/tmp/demo",
        claudeConfigDir: configDir.path, codexHome: "/nonexistent") == false)
}

@Test func codexValidWhenRolloutFilenameContainsRef() throws {
    let home = try makeTempDir()
    defer { try? FileManager.default.removeItem(at: home) }
    let sessionsDir = home.appendingPathComponent("sessions/2026/07/08")
    try FileManager.default.createDirectory(at: sessionsDir, withIntermediateDirectories: true)
    let ref = "11111111-2222-3333-4444-555555555555"
    try Data().write(to: sessionsDir.appendingPathComponent("rollout-2026-07-08T12-00-00-\(ref).jsonl"))

    #expect(AgentSessionValidator.isLikelyValid(
        agentId: "codex", sessionRef: ref, worktreePath: "/w",
        claudeConfigDir: "/nonexistent", codexHome: home.path) == true)
    #expect(AgentSessionValidator.isLikelyValid(
        agentId: "codex", sessionRef: "deadbeef-0000-0000-0000-000000000000", worktreePath: "/w",
        claudeConfigDir: "/nonexistent", codexHome: home.path) == false)
}

@Test func agentsWithoutCheckableStoreAreTrusted() {
    for agentId in ["opencode", "pi", "omp"] {
        #expect(AgentSessionValidator.isLikelyValid(
            agentId: agentId, sessionRef: "anything", worktreePath: "/w",
            claudeConfigDir: "/nonexistent", codexHome: "/nonexistent") == true)
    }
}

@Test func claudeSlugReplacesNonASCIILetters() {
    #expect(AgentSessionValidator.claudeProjectSlug("/tmp/progetto-\u{00E9}/citt\u{00E0}")
        == "-tmp-progetto---citt-")
}

@Test func codexInvalidWhenSessionsDirectoryMissing() throws {
    let home = try makeTempDir()
    defer { try? FileManager.default.removeItem(at: home) }
    // no sessions/ subdirectory created
    #expect(AgentSessionValidator.isLikelyValid(
        agentId: "codex", sessionRef: "11111111-2222-3333-4444-555555555555", worktreePath: "/w",
        claudeConfigDir: "/nonexistent", codexHome: home.path) == false)
}
