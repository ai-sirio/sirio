import Foundation

/// Adapter for Anthropic's Claude Code CLI.
///
/// `prepare` writes a project-level `settings.local.json` under
/// `<worktreePath>/.claude/` with hooks that notify Tiller on lifecycle events.
/// Merge semantics replace ONLY the five hook event arrays (`Stop`,
/// `Notification`, `SessionStart`, `UserPromptSubmit`, `SessionEnd`) while
/// preserving every other key in the file.
///
/// `command` returns `"claude"` — the pane already changes cwd to the worktree.
public struct ClaudeCodeAdapter: AgentAdapter, Sendable {
    public let id = "claude"
    public let displayName = "Claude Code"

    public var hasNativeHooks: Bool { true }
    public init() {}

    public func prepare(
        worktreePath: String,
        paneId: UUID,
        tillerctlPath: String
    ) throws {

        let claudeDir = (worktreePath as NSString).appendingPathComponent(".claude")
        try FileManager.default.createDirectory(atPath: claudeDir, withIntermediateDirectories: true)

        let settingsPath = (claudeDir as NSString).appendingPathComponent("settings.local.json")

        // Build the five hook arrays with the concrete tillerctl path and paneId.
        let needsInputCmd = "\(shellQuote(tillerctlPath)) notify --session \(paneId.uuidString) --status needs-input --stdin-json"
        let runningCmd    = "\(shellQuote(tillerctlPath)) notify --session \(paneId.uuidString) --status running --stdin-json"
        let doneCmd       = "\(shellQuote(tillerctlPath)) notify --session \(paneId.uuidString) --status done --stdin-json"

        let hooks: [String: Any] = [
            "Stop": [
                ["matcher": "", "hooks": [
                    ["type": "command", "command": needsInputCmd]
                ]]
            ],
            "Notification": [
                ["matcher": "", "hooks": [
                    ["type": "command", "command": needsInputCmd]
                ]]
            ],
            // A session start (fresh launch or /compact-resume) lands Claude on an
            // idle prompt awaiting the user's first input — it is waiting, not
            // running. UserPromptSubmit flips it to running when the user submits.
            "SessionStart": [
                ["matcher": "", "hooks": [
                    ["type": "command", "command": needsInputCmd]
                ]]
            ],
            "UserPromptSubmit": [
                ["matcher": "", "hooks": [
                    ["type": "command", "command": runningCmd]
                ]]
            ],
            "SessionEnd": [
                ["matcher": "", "hooks": [
                    ["type": "command", "command": doneCmd]
                ]]
            ],
        ]

        // Merge with existing file if present.
        var root: [String: Any]
        if FileManager.default.fileExists(atPath: settingsPath),
           let data = try? Data(contentsOf: URL(fileURLWithPath: settingsPath)),
           let obj = try? JSONSerialization.jsonObject(with: data) as? [String: Any] {
            root = obj
        } else {
            root = [:]
        }

        // Replace the five keys under "hooks"; preserve all other hooks keys.
        if var existingHooks = root["hooks"] as? [String: Any] {
            for (key, value) in hooks {
                existingHooks[key] = value
            }
            root["hooks"] = existingHooks
        } else {
            root["hooks"] = hooks
        }

        let output = try JSONSerialization.data(withJSONObject: root, options: [.prettyPrinted, .sortedKeys])
        try output.write(to: URL(fileURLWithPath: settingsPath), options: .atomic)
    }

    public func command(worktreePath: String, paneId: UUID, tillerctlPath: String) -> String {
        "claude"
    }

    public func resumeCommand(worktreePath: String, paneId: UUID,
                              tillerctlPath: String, sessionRef: String) -> String? {
        "claude --resume \(shellQuote(sessionRef))"
    }
}
