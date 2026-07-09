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
    try adapter.prepare(worktreePath: dir.path, paneId: paneId, tillerctlPath: "/usr/local/bin/tillerctl")

    // Only .opencode/ appears in the worktree; nothing else, nothing global.
    #expect(try FileManager.default.contentsOfDirectory(atPath: dir.path) == [".opencode"])
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

@Test func piPrepareWritesNothingAndCommandIsBare() throws {
    let dir = FileManager.default.temporaryDirectory
        .appendingPathComponent(UUID().uuidString)
    try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
    defer { try? FileManager.default.removeItem(at: dir) }

    let adapter = PiAdapter()
    try adapter.prepare(worktreePath: dir.path, paneId: UUID(), tillerctlPath: "/usr/local/bin/tillerctl")

    #expect(try FileManager.default.contentsOfDirectory(atPath: dir.path).isEmpty)
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
    try OhMyPiAdapter().prepare(worktreePath: dir.path, paneId: paneId, tillerctlPath: "/usr/local/bin/tillerctl")

    // Only .tiller/ appears in the worktree; nothing else, nothing global.
    #expect(try FileManager.default.contentsOfDirectory(atPath: dir.path) == [".tiller"])
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

    try OhMyPiAdapter().prepare(worktreePath: dir.path, paneId: UUID(), tillerctlPath: "/x")
    let hook = try String(contentsOf: dir.appendingPathComponent(".tiller/omp-hook.ts"), encoding: .utf8)
    #expect(hook.contains(#"pi.on("session_start""#))
    #expect(hook.contains("--agent-session"))
}
