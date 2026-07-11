import Foundation

/// Determines which Tiller-catalog agent (if any) a terminal title belongs
/// to, with no prior registration required — the identity-detection half
/// of Layer B, used to recognize an agent the user launched by hand
/// instead of through Tiller's own spawn button. Conceptual port of Orca's
/// `getAgentLabel`, scoped to Tiller's 5-agent catalog.
///
/// Deliberately more conservative than AgentTitleStatus.detect's per-agent
/// status logic, which assumes the identity is already known and is safe
/// to be permissive (e.g. Pi's bare "contains pi" idle check). Assigning
/// IDENTITY from an arbitrary, unregistered title must avoid mistaking a
/// plain shell prompt showing a branch/cwd name like "pi-notes" for a
/// running Pi agent.
public enum AgentTitleIdentity {
    private static let brailleSpinnerRange: ClosedRange<UInt32> = 0x2800...0x28FF
    private static let claudeIdle = "\u{2733}" // ✳

    /// Returns the catalog agent id ("claude", "codex", "opencode", "pi",
    /// "omp") this title belongs to, or nil if the title carries no
    /// recognizable agent identity.
    public static func identify(title: String) -> String? {
        guard !title.isEmpty else { return nil }

        // 1. Claude's own glyph prefixes are unambiguous — check first.
        if title == claudeIdle || title.hasPrefix("\(claudeIdle) ") || title.hasPrefix(". ") {
            return "claude"
        }

        // 2. Pi and its omp fork brand titles with the π glyph; the separator
        // distinguishes them: pi = "π - <cwd>", omp = "π: <cwd>" (captured
        // live from both CLIs). Checked before the braille fallback so a
        // working "⠋ π: …" title isn't misread as Claude's spinner.
        if title.contains("π:") { return "omp" }
        if title.contains("π") { return "pi" }

        // 3. Codex/OpenCode/omp announce their own name as literal text.
        // Word-boundary match (not substring) so a bare cwd/branch title
        // like "opencode-experiment" or "~/codex-notes" doesn't identify.
        let lower = title.lowercased()
        for id in ["codex", "opencode", "omp"] {
            if AgentNameBoundaryMatch.containsWord(id, in: lower) {
                return id
            }
        }

        // 4. Pi's braille spinner alone is ambiguous with Claude's — require
        // the literal word "pi" alongside it too.
        let hasBrailleSpinner = title.unicodeScalars.contains { brailleSpinnerRange.contains($0.value) }
        if hasBrailleSpinner && AgentNameBoundaryMatch.containsWord("pi", in: lower) {
            return "pi"
        }

        // 5. Any other braille spinner is Claude's working convention.
        if hasBrailleSpinner {
            return "claude"
        }

        return nil
    }
}
