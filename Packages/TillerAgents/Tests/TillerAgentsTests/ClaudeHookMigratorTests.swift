import Foundation
import Testing
@testable import TillerAgents

@Suite struct ClaudeHookMigratorTests {
    private let newPath = "/Users/u/Library/Application Support/Tiller/bin/tillerctl"

    private func settings(command: String) -> Data {
        let root: [String: Any] = [
            "permissions": ["allow": ["Bash"]],
            "hooks": [
                "Stop": [
                    ["matcher": "", "hooks": [["type": "command", "command": command]]]
                ]
            ],
        ]
        return try! JSONSerialization.data(withJSONObject: root)
    }

    @Test func rewritesStaleTillerctlPathPreservingArguments() throws {
        let stale = "'/old/DerivedData/Tiller.app/Contents/MacOS/tillerctl' notify --session ABC --status needs-input --stdin-json"
        let data = settings(command: stale)

        let rewritten = try #require(ClaudeHookMigrator.rewrittenSettings(data, tillerctlPath: newPath))

        let text = String(decoding: rewritten, as: UTF8.self)
        #expect(text.contains("notify --session ABC --status needs-input --stdin-json"))
        #expect(text.contains("Tiller/bin/tillerctl"))
        #expect(!text.contains("DerivedData"))
    }

    @Test func preservesUnrelatedKeys() throws {
        let stale = "'/old/tillerctl' notify --session ABC --status done --stdin-json"
        let rewritten = try #require(ClaudeHookMigrator.rewrittenSettings(
            settings(command: stale), tillerctlPath: newPath))

        let root = try #require(try JSONSerialization.jsonObject(with: rewritten) as? [String: Any])
        let permissions = try #require(root["permissions"] as? [String: Any])
        #expect(permissions["allow"] as? [String] == ["Bash"])
    }

    @Test func returnsNilWhenPathAlreadyCurrent() {
        let current = "\(shellQuote(newPath)) notify --session ABC --status done --stdin-json"
        #expect(ClaudeHookMigrator.rewrittenSettings(
            settings(command: current), tillerctlPath: newPath) == nil)
    }

    @Test func leavesNonTillerctlCommandsUntouched() {
        let other = "/usr/bin/say done"
        #expect(ClaudeHookMigrator.rewrittenSettings(
            settings(command: other), tillerctlPath: newPath) == nil)
    }

    @Test func returnsNilForMalformedJSON() {
        #expect(ClaudeHookMigrator.rewrittenSettings(
            Data("not json".utf8), tillerctlPath: newPath) == nil)
    }

    @Test func migrateFileRewritesOnDisk() throws {
        let dir = URL(fileURLWithPath: NSTemporaryDirectory())
            .appendingPathComponent("migrator-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: dir) }
        let file = dir.appendingPathComponent("settings.local.json")
        let stale = "'/old/tillerctl' notify --session ABC --status done --stdin-json"
        try settings(command: stale).write(to: file)

        let changed = ClaudeHookMigrator.migrateFile(atPath: file.path, tillerctlPath: newPath)

        #expect(changed)
        let text = try String(contentsOf: file, encoding: .utf8)
        #expect(!text.contains("/old/tillerctl"))
        #expect(text.contains("Tiller/bin/tillerctl"))
    }

    @Test func migrateFileIsNoOpForMissingFile() {
        #expect(!ClaudeHookMigrator.migrateFile(
            atPath: "/nonexistent/settings.local.json", tillerctlPath: newPath))
    }
}
