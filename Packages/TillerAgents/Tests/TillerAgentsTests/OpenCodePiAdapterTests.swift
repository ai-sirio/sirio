import Testing
import Foundation
@testable import TillerAgents

@Test func openCodePrepareWritesWorktreeLocalPluginOnly() throws {
    let dir = FileManager.default.temporaryDirectory
        .appendingPathComponent(UUID().uuidString)
    try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
    defer { try? FileManager.default.removeItem(at: dir) }

    let paneId = UUID()
    let adapter = OpenCodeAdapter()
    try adapter.prepare(worktreePath: dir.path, paneId: paneId, tillerctlPath: "/usr/local/bin/tillerctl", skillMarkdown: try repositorySkill())

    // Skill provisioning precedes the existing worktree-local plugin.
    #expect(Set(try FileManager.default.contentsOfDirectory(atPath: dir.path)) == [".agents", ".opencode"])
    let plugin = try String(
        contentsOf: dir.appendingPathComponent(".opencode/plugin/tiller-session.js"),
        encoding: .utf8)
    #expect(plugin.contains(jsonStringLiteral("/usr/local/bin/tillerctl")))
    #expect(plugin.contains(paneId.uuidString))
    #expect(plugin.contains("session-ref"))
    #expect(adapter.command(worktreePath: dir.path, paneId: paneId,
                            tillerctlPath: "/usr/local/bin/tillerctl") == "opencode")
    #expect(adapter.hasNativeHooks == false)
}

@Test func piPrepareOnlyProvisionsSkillAndCommandIsBare() throws {
    let dir = FileManager.default.temporaryDirectory
        .appendingPathComponent(UUID().uuidString)
    try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
    defer { try? FileManager.default.removeItem(at: dir) }

    let adapter = PiAdapter()
    try adapter.prepare(worktreePath: dir.path, paneId: UUID(), tillerctlPath: "/usr/local/bin/tillerctl", skillMarkdown: try repositorySkill())

    #expect(try FileManager.default.contentsOfDirectory(atPath: dir.path) == [".agents"])
    #expect(adapter.command(worktreePath: dir.path, paneId: UUID(), tillerctlPath: "/usr/local/bin/tillerctl") == "pi")
    #expect(adapter.hasNativeHooks == false)
}

@Test func catalogOrderFinal() {
    #expect(AgentCatalog.all.map(\.id) == ["claude", "codex", "opencode", "pi", "omp"])
}

@Test func ohMyPiPrepareWritesHookFileInsideWorktreeOnly() throws {
    let dir = FileManager.default.temporaryDirectory
        .appendingPathComponent(UUID().uuidString)
    try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
    defer { try? FileManager.default.removeItem(at: dir) }

    let paneId = UUID()
    try OhMyPiAdapter().prepare(worktreePath: dir.path, paneId: paneId, tillerctlPath: "/usr/local/bin/tillerctl", skillMarkdown: try repositorySkill())

    // Skill provisioning precedes the existing worktree-local hook.
    #expect(Set(try FileManager.default.contentsOfDirectory(atPath: dir.path)) == [".agents", ".tiller"])
    let hook = try String(contentsOf: dir.appendingPathComponent(".tiller/omp-hook.ts"), encoding: .utf8)
    #expect(hook.contains(jsonStringLiteral("/usr/local/bin/tillerctl")))
    #expect(hook.contains(paneId.uuidString))
    #expect(hook.contains(#"pi.on("turn_start""#))
    #expect(hook.contains(#"pi.on("turn_end""#))
    #expect(hook.contains(#"pi.on("session_shutdown""#))
}

@Test func ohMyPiCommandLoadsHook() {
    let adapter = OhMyPiAdapter()
    let cmd = adapter.command(worktreePath: "/tmp/wt", paneId: UUID(), tillerctlPath: "/usr/local/bin/tillerctl")
    #expect(cmd == "omp --hook '/tmp/wt/.tiller/omp-hook.ts'")
    #expect(adapter.hasNativeHooks == true)
}

@Test func ohMyPiHookForwardsSessionIdentity() throws {
    let dir = FileManager.default.temporaryDirectory
        .appendingPathComponent(UUID().uuidString)
    try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
    defer { try? FileManager.default.removeItem(at: dir) }

    try OhMyPiAdapter().prepare(worktreePath: dir.path, paneId: UUID(), tillerctlPath: "/x", skillMarkdown: try repositorySkill())
    let hook = try String(contentsOf: dir.appendingPathComponent(".tiller/omp-hook.ts"), encoding: .utf8)
    #expect(hook.contains(#"pi.on("session_start""#))
    #expect(hook.contains("--agent-session"))
}
