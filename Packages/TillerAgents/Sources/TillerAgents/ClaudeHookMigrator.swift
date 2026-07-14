import Foundation

/// One-time repair for `.claude/settings.local.json` hook commands whose
/// embedded tillerctl path went stale (e.g. a cleaned DerivedData build).
/// Rewrites only the leading quoted binary path of tillerctl commands,
/// preserving the paneId and every other argument — and every other key
/// in the file.
public enum ClaudeHookMigrator {
    /// Returns rewritten settings JSON when at least one hook command's
    /// tillerctl path changed; nil when nothing to rewrite or unparseable.
    public static func rewrittenSettings(_ data: Data, tillerctlPath: String) -> Data? {
        guard var root = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any],
              var hooks = root["hooks"] as? [String: Any] else { return nil }
        var changed = false
        for (event, value) in hooks {
            guard var entries = value as? [[String: Any]] else { continue }
            for i in entries.indices {
                guard var inner = entries[i]["hooks"] as? [[String: Any]] else { continue }
                for j in inner.indices {
                    guard let command = inner[j]["command"] as? String,
                          let rewritten = rewriteCommand(command, tillerctlPath: tillerctlPath)
                    else { continue }
                    inner[j]["command"] = rewritten
                    changed = true
                }
                entries[i]["hooks"] = inner
            }
            hooks[event] = entries
        }
        guard changed else { return nil }
        root["hooks"] = hooks
        return try? JSONSerialization.data(
            withJSONObject: root,
            options: [.prettyPrinted, .sortedKeys, .withoutEscapingSlashes]
        )
    }

    /// Applies `rewrittenSettings` to the file at `path` in place.
    /// Returns true when the file was rewritten. Missing/unchanged: no-op.
    @discardableResult
    public static func migrateFile(atPath path: String, tillerctlPath: String) -> Bool {
        guard let data = try? Data(contentsOf: URL(fileURLWithPath: path)),
              let rewritten = rewrittenSettings(data, tillerctlPath: tillerctlPath),
              (try? rewritten.write(to: URL(fileURLWithPath: path), options: .atomic)) != nil
        else { return false }
        return true
    }

    /// "'/any/path/tillerctl' <args>" → "'<tillerctlPath>' <args>".
    /// Nil when the command is not a quoted-tillerctl invocation or the
    /// path is already current.
    static func rewriteCommand(_ command: String, tillerctlPath: String) -> String? {
        guard command.hasPrefix("'") else { return nil }
        let afterQuote = command.index(after: command.startIndex)
        guard let closing = command[afterQuote...].firstIndex(of: "'") else { return nil }
        let path = String(command[afterQuote..<closing])
        guard path.hasSuffix("/tillerctl"), path != tillerctlPath else { return nil }
        return shellQuote(tillerctlPath) + command[command.index(after: closing)...]
    }
}
