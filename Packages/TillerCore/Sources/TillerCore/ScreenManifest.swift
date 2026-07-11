import Foundation

/// Layer C: matches recent pane buffer content (ANSI-stripped tail text)
/// against per-agent rules, as a content-based counterpart to Layer B's
/// title-based `AgentTitleStatus`. Returns nil when no rule matches; nil
/// must never force a transition (same contract as `AgentTitleStatus.detect`).
///
/// Only `claude`'s rule is based on a confirmed CLI convention (its
/// numbered permission-prompt menu and "(esc to interrupt)" streaming
/// indicator). The other four adapters share a conservative generic
/// confirmation-prompt matcher — best-effort until verified against live
/// output (see spec: docs/superpowers/specs/2026-07-09-screen-manifest-detection-design.md).
public enum ScreenManifest {
    public static func detect(tailText: String, agentId: String) -> AgentStatus? {
        guard !tailText.isEmpty else { return nil }
        switch agentId {
        case "claude": return detectClaude(tailText)
        case "codex", "opencode", "pi", "omp": return detectGenericPrompt(tailText)
        default: return nil
        }
    }

    private static func detectClaude(_ text: String) -> AgentStatus? {
        if text.contains("Do you want to proceed?") { return .needsInput }
        if text.contains("esc to interrupt") { return .running }
        return nil
    }

    /// Best-effort: matches common CLI confirmation phrasing. A wrong match
    /// is worse than no match (nil is always safe), so this stays narrow.
    private static func detectGenericPrompt(_ text: String) -> AgentStatus? {
        let lower = text.lowercased()
        if lower.range(of: #"\(y/n\)|\[y/n\]|\by/n\b"#, options: .regularExpression) != nil {
            return .needsInput
        }
        let confirmPhrases = ["proceed?", "continue?", "allow?", "confirm?"]
        if confirmPhrases.contains(where: { lower.contains($0) }) { return .needsInput }
        return nil
    }
}
