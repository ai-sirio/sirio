import Foundation

/// Derives an `AgentStatus` from a terminal's OSC title text, using each
/// CLI's own title conventions (Layer B: a fallback/gap-filler used when a
/// lifecycle hook hasn't fired yet, or doesn't exist for that agent at all).
/// Conceptual port of Orca's `detectAgentStatusFromTitle` — not a literal
/// copy, and deliberately simpler (no staleness/retention system).
/// Returns nil when the title carries no recognizable status opinion; nil
/// must never force a transition.
public enum AgentTitleStatus {
    private static let brailleSpinnerRange: ClosedRange<UInt32> = 0x2800...0x28FF

    private static let workingKeywords = ["working", "thinking", "running"]
    private static let idleKeywords = ["ready", "idle", "done"]
    private static let waitingKeywords = ["permission", "action required", "waiting"]

    public static func detect(title: String, agentId: String) -> AgentStatus? {
        guard !title.isEmpty else { return nil }
        switch agentId {
        case "claude": return detectClaude(title)
        case "pi": return detectPi(title)
        default: return detectGeneric(title, agentId: agentId)
        }
    }

    private static func containsBrailleSpinner(_ title: String) -> Bool {
        title.unicodeScalars.contains { brailleSpinnerRange.contains($0.value) }
    }

    /// Claude Code sets its own title to "✳ <task>" when idle and either
    /// ". <task>" or a braille spinner frame when actively thinking.
    private static func detectClaude(_ title: String) -> AgentStatus? {
        if title == "✳" || title.hasPrefix("✳ ") { return .needsInput }
        if title.hasPrefix(". ") || containsBrailleSpinner(title) { return .running }
        return nil
    }

    /// Pi sets a braille spinner while working; the same title minus the
    /// spinner, still recognizable as Pi's, means idle.
    private static func detectPi(_ title: String) -> AgentStatus? {
        if containsBrailleSpinner(title) { return .running }
        guard title.localizedCaseInsensitiveContains("pi") else { return nil }
        return .needsInput
    }

    /// Codex/OpenCode/OMP have no bespoke glyph convention (or it's unverified) —
    /// match generically on the agent's own name token plus a status keyword.
    private static func detectGeneric(_ title: String, agentId: String) -> AgentStatus? {
        let lower = title.lowercased()
        guard lower.contains(agentId) else { return nil }
        if waitingKeywords.contains(where: { AgentNameBoundaryMatch.containsWord($0, in: lower) }) { return .needsInput }
        if idleKeywords.contains(where: { AgentNameBoundaryMatch.containsWord($0, in: lower) }) { return .needsInput }
        if workingKeywords.contains(where: { AgentNameBoundaryMatch.containsWord($0, in: lower) }) { return .running }
        return nil
    }
}
