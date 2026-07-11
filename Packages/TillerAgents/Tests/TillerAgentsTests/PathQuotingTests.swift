import Testing
import Foundation
@testable import TillerAgents

// MARK: - ClaudeCodeAdapter

@Test func claudeCodeQuotingWithSpaces() throws {
    let dir = NSTemporaryDirectory() + "tiller-agents-\(UUID().uuidString.prefix(8))"
    try FileManager.default.createDirectory(atPath: dir, withIntermediateDirectories: true)
    defer { try? FileManager.default.removeItem(atPath: dir) }

    let tillerctlPath = "/Users/John Smith/bin/tillerctl"
    try ClaudeCodeAdapter().prepare(worktreePath: dir, paneId: UUID(), tillerctlPath: tillerctlPath)

    let data = try Data(contentsOf: URL(fileURLWithPath: dir + "/.claude/settings.local.json"))
    let json = try JSONSerialization.jsonObject(with: data) as? [String: Any]
    let hooks = json?["hooks"] as? [String: Any]
    let stop = ((hooks?["Stop"] as? [[String: Any]])?.first?["hooks"] as? [[String: Any]])?.first
    let cmd = stop?["command"] as? String
    #expect(cmd?.hasPrefix(shellQuote(tillerctlPath)) == true)
}

@Test func claudeCodeQuotingWithSingleQuote() throws {
    let dir = NSTemporaryDirectory() + "tiller-agents-\(UUID().uuidString.prefix(8))"
    try FileManager.default.createDirectory(atPath: dir, withIntermediateDirectories: true)
    defer { try? FileManager.default.removeItem(atPath: dir) }

    let tillerctlPath = "/Users/O'Brien/bin/tillerctl"
    try ClaudeCodeAdapter().prepare(worktreePath: dir, paneId: UUID(), tillerctlPath: tillerctlPath)

    let data = try Data(contentsOf: URL(fileURLWithPath: dir + "/.claude/settings.local.json"))
    let json = try JSONSerialization.jsonObject(with: data) as? [String: Any]
    let hooks = json?["hooks"] as? [String: Any]
    let stop = ((hooks?["Stop"] as? [[String: Any]])?.first?["hooks"] as? [[String: Any]])?.first
    let cmd = stop?["command"] as? String
    #expect(cmd?.hasPrefix(shellQuote(tillerctlPath)) == true)
}

@Test func claudeCodeQuotingWithDoubleQuote() throws {
    let dir = NSTemporaryDirectory() + "tiller-agents-\(UUID().uuidString.prefix(8))"
    try FileManager.default.createDirectory(atPath: dir, withIntermediateDirectories: true)
    defer { try? FileManager.default.removeItem(atPath: dir) }

    let tillerctlPath = #"/Users/John"Smith/bin/tillerctl"#
    try ClaudeCodeAdapter().prepare(worktreePath: dir, paneId: UUID(), tillerctlPath: tillerctlPath)

    let data = try Data(contentsOf: URL(fileURLWithPath: dir + "/.claude/settings.local.json"))
    let json = try JSONSerialization.jsonObject(with: data) as? [String: Any]
    let hooks = json?["hooks"] as? [String: Any]
    let stop = ((hooks?["Stop"] as? [[String: Any]])?.first?["hooks"] as? [[String: Any]])?.first
    let cmd = stop?["command"] as? String
    #expect(cmd?.hasPrefix(shellQuote(tillerctlPath)) == true)
}

// MARK: - CodexAdapter

@Test func codexQuotingWithSpaces() {
    let paneId = UUID()
    let tillerctlPath = "/Users/John Smith/bin/tillerctl"
    let cmd = CodexAdapter().command(worktreePath: "/w", paneId: paneId, tillerctlPath: tillerctlPath)

    // The JSON array inside notify=[...] must survive shell unquoting and JSON decoding.
    verifyCodexRoundTrip(cmd: cmd, paneId: paneId, tillerctlPath: tillerctlPath)
}

@Test func codexQuotingWithSingleQuote() {
    let paneId = UUID()
    let tillerctlPath = "/Users/O'Brien/bin/tillerctl"
    let cmd = CodexAdapter().command(worktreePath: "/w", paneId: paneId, tillerctlPath: tillerctlPath)

    #expect(!cmd.contains(tillerctlPath))
    verifyCodexRoundTrip(cmd: cmd, paneId: paneId, tillerctlPath: tillerctlPath)
}

@Test func codexQuotingWithDoubleQuote() {
    let paneId = UUID()
    let tillerctlPath = #"/Users/John"Smith/bin/tillerctl"#
    let cmd = CodexAdapter().command(worktreePath: "/w", paneId: paneId, tillerctlPath: tillerctlPath)

    #expect(!cmd.contains(tillerctlPath))
    verifyCodexRoundTrip(cmd: cmd, paneId: paneId, tillerctlPath: tillerctlPath)
}

/// Strip `codex -c ` and the outer shell quotes, then decode `notify=[...]`.
private func verifyCodexRoundTrip(cmd: String, paneId: UUID, tillerctlPath: String) {
    // Expect: codex -c 'notify=[...]'
    let prefix = "codex -c "
    #expect(cmd.hasPrefix(prefix))
    let afterPrefix = String(cmd.dropFirst(prefix.count))

    // Shell-unquote: the value is single-quoted, so strip the outer '...'
    // and unescape any embedded '\'' sequences.
    #expect(afterPrefix.hasPrefix("'") && afterPrefix.hasSuffix("'"))
    let inner = String(afterPrefix.dropFirst().dropLast())
        .replacingOccurrences(of: "'\\''", with: "'")

    // Extract the JSON array: notify=[...]
    let notifyPrefix = "notify="
    #expect(inner.hasPrefix(notifyPrefix))
    let jsonFragment = String(inner.dropFirst(notifyPrefix.count))

    // Decode the JSON array.
    guard let jsonData = jsonFragment.data(using: .utf8),
          let decoded = try? JSONDecoder().decode([String].self, from: jsonData) else {
        Issue.record("Failed to decode JSON: \(jsonFragment)")
        return
    }

    // The decoded array should match the original args.
    let expectedArgs = [tillerctlPath, "notify", "--session", paneId.uuidString, "--status", "needs-input"]
    #expect(decoded == expectedArgs)
}

// MARK: - OhMyPiAdapter

@Test func ohMyPiQuotingWithSpaces() throws {
    let dir = FileManager.default.temporaryDirectory
        .appendingPathComponent(UUID().uuidString)
    try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
    defer { try? FileManager.default.removeItem(at: dir) }

    let tillerctlPath = "/Users/John Smith/bin/tillerctl"
    try OhMyPiAdapter().prepare(worktreePath: dir.path, paneId: UUID(), tillerctlPath: tillerctlPath)

    let hook = try String(contentsOf: dir.appendingPathComponent(".tiller/omp-hook.ts"), encoding: .utf8)
    #expect(hook.contains(jsonStringLiteral(tillerctlPath)))
}

@Test func ohMyPiQuotingWithSingleQuote() throws {
    let dir = FileManager.default.temporaryDirectory
        .appendingPathComponent(UUID().uuidString)
    try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
    defer { try? FileManager.default.removeItem(at: dir) }

    let tillerctlPath = "/Users/O'Brien/bin/tillerctl"
    try OhMyPiAdapter().prepare(worktreePath: dir.path, paneId: UUID(), tillerctlPath: tillerctlPath)

    let hook = try String(contentsOf: dir.appendingPathComponent(".tiller/omp-hook.ts"), encoding: .utf8)
    #expect(hook.contains(jsonStringLiteral(tillerctlPath)))
}

@Test func ohMyPiQuotingWithDoubleQuote() throws {
    let dir = FileManager.default.temporaryDirectory
        .appendingPathComponent(UUID().uuidString)
    try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
    defer { try? FileManager.default.removeItem(at: dir) }

    let tillerctlPath = #"/Users/John"Smith/bin/tillerctl"#
    try OhMyPiAdapter().prepare(worktreePath: dir.path, paneId: UUID(), tillerctlPath: tillerctlPath)

    let hook = try String(contentsOf: dir.appendingPathComponent(".tiller/omp-hook.ts"), encoding: .utf8)
    #expect(hook.contains(jsonStringLiteral(tillerctlPath)))
}

@Test func ohMyPiCommandQuotingWorktreePath() {
    let adapter = OhMyPiAdapter()

    // Space in worktreePath
    let spaceCmd = adapter.command(worktreePath: "/Users/John Smith/Project", paneId: UUID(), tillerctlPath: "/x")
    #expect(spaceCmd.hasPrefix("omp --hook "))
    let spaceSuffix = String(spaceCmd.dropFirst("omp --hook ".count))
    #expect(spaceSuffix == shellQuote("/Users/John Smith/Project/.tiller/omp-hook.ts"))

    // Single quote in worktreePath
    let quoteCmd = adapter.command(worktreePath: "/Users/O'Brien/Project", paneId: UUID(), tillerctlPath: "/x")
    #expect(quoteCmd.hasPrefix("omp --hook "))
    let quoteSuffix = String(quoteCmd.dropFirst("omp --hook ".count))
    #expect(quoteSuffix == shellQuote("/Users/O'Brien/Project/.tiller/omp-hook.ts"))

    // Double quote in worktreePath
    let dquoteCmd = adapter.command(worktreePath: #"/Users/John"Smith/Project"#, paneId: UUID(), tillerctlPath: "/x")
    #expect(dquoteCmd.hasPrefix("omp --hook "))
    let dquoteSuffix = String(dquoteCmd.dropFirst("omp --hook ".count))
    #expect(dquoteSuffix == shellQuote(#"/Users/John"Smith/Project/.tiller/omp-hook.ts"#))
}
