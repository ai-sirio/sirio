import Foundation

/// Extracts an agent-native session reference from hook payloads.
/// Pure functions — no I/O — so tillerctl and tests share the same logic.
public enum AgentSessionExtractor {
    /// Keys agents use for their session identity, in priority order.
    private static let keys = [
        "session_id", "session-id", "sessionId",
        "thread_id", "thread-id",
        "rollout_path", "rollout-path",
    ]

    /// Parse a single JSON object (e.g. Claude Code hook stdin) and return
    /// the first known session key with a non-empty string value.
    public static func sessionRef(fromJSON data: Data) -> String? {
        guard let obj = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else {
            return nil
        }
        return ref(in: obj)
    }

    /// Scan positional arguments (e.g. the Codex `notify` JSON payload) for
    /// the first parseable JSON object containing a known session key.
    public static func sessionRef(fromPayloadArguments args: [String]) -> String? {
        for arg in args {
            if let found = sessionRef(fromJSON: Data(arg.utf8)) { return found }
        }
        return nil
    }

    private static func ref(in obj: [String: Any]) -> String? {
        for key in keys {
            guard let value = obj[key] as? String, !value.isEmpty else { continue }
            // Rollout paths carry the id in the filename.
            if key.hasPrefix("rollout") { return rolloutId(fromPath: value) }
            return value
        }
        return nil
    }

    /// "…/rollout-2026-07-08T12-00-00-<uuid>.jsonl" → "<uuid>" (lowercase).
    static func rolloutId(fromPath path: String) -> String? {
        let name = (path as NSString).lastPathComponent
        guard name.hasPrefix("rollout-"), name.hasSuffix(".jsonl") else { return nil }
        let stem = String(name.dropLast(".jsonl".count))
        let parts = stem.split(separator: "-")
        guard parts.count >= 5 else { return nil }
        let candidate = parts.suffix(5).joined(separator: "-")
        guard UUID(uuidString: candidate) != nil else { return nil }
        return candidate.lowercased()
    }
}
