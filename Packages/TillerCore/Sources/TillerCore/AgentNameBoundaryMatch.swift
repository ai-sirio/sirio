import Foundation

/// Word-boundary-safe substring containment, shared by AgentTitleStatus's
/// keyword matching and AgentTitleIdentity's agent-name matching so both
/// title-matchers agree on what counts as a word boundary. Internal to
/// TillerCore — not part of the package's public API.
enum AgentNameBoundaryMatch {
    /// True if `word` appears in `lower` with a non-word character (or
    /// string edge) on both sides — so "ready" doesn't match inside
    /// "already", and "codex" doesn't match inside "opencodex" or
    /// "codex-notes" (hyphen counts as a word character here).
    static func containsWord(_ word: String, in lower: String) -> Bool {
        guard let range = lower.range(of: word) else { return false }
        let beforeOK = range.lowerBound == lower.startIndex || !isWordChar(lower[lower.index(before: range.lowerBound)])
        let afterOK = range.upperBound == lower.endIndex || !isWordChar(lower[range.upperBound])
        return beforeOK && afterOK
    }

    private static func isWordChar(_ c: Character) -> Bool {
        c.isLetter || c.isNumber || c == "-" || c == "_"
    }
}
