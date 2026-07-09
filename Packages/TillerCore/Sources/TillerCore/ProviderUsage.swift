import Foundation

// MARK: - Value types

public struct UsageWindow: Equatable, Sendable {
    public let label: String
    public let usedPercent: Int
    public let resetsAt: Date?

    public init(label: String, usedPercent: Int, resetsAt: Date? = nil) {
        self.label = label
        self.usedPercent = min(100, max(0, usedPercent))
        self.resetsAt = resetsAt
    }
}

public struct ProviderUsage: Equatable, Sendable {
    public let session: UsageWindow?
    public let weekly: UsageWindow?
    public let monthly: UsageWindow?
    public let fableWeekly: UsageWindow?

    public init(
        session: UsageWindow?, weekly: UsageWindow?,
        monthly: UsageWindow? = nil, fableWeekly: UsageWindow? = nil
    ) {
        self.session = session
        self.weekly = weekly
        self.monthly = monthly
        self.fableWeekly = fableWeekly
    }

    public var hasAny: Bool { session != nil || weekly != nil || monthly != nil || fableWeekly != nil }
}

// MARK: - Parser (pure)

/// Parses the `claude` `/usage` TUI text into usage windows. Returns nil when
/// no window could be read (e.g. the TUI has not rendered yet). Ported from
/// Orca's `claude-pty.ts` parser; label regexes kept bug-compatible with a
/// moving CLI TUI.
public func parseClaudeUsage(_ raw: String) -> ProviderUsage? {
    let lines = stripANSI(raw)
        .split(whereSeparator: { $0 == "\n" || $0 == "\r" })
        .map(String.init)

    let session = firstPercent(in: lines, isLabel: isSessionLabel)
        .map { UsageWindow(label: "5h", usedPercent: $0) }
    let weekly = firstPercent(in: lines, isLabel: { isWeeklyLabel($0) && !containsFable($0) })
        .map { UsageWindow(label: "wk", usedPercent: $0) }
    let fable = firstPercent(in: lines, isLabel: isFableLabel)
        .map { UsageWindow(label: "Fable", usedPercent: $0) }

    let usage = ProviderUsage(session: session, weekly: weekly, fableWeekly: fable)
    return usage.hasAny ? usage : nil
}

// MARK: - Label matchers

private let weeklyPattern =
    #"(?:current\s*week|weekly\s*(?:limits?|usage|rate\s*limits?)|7\s*[- ]?\s*day)"#

private func matches(_ line: String, _ pattern: String) -> Bool {
    line.range(of: pattern, options: [.regularExpression, .caseInsensitive]) != nil
}

private func containsFable(_ line: String) -> Bool {
    line.range(of: #"\bfable\b"#, options: [.regularExpression, .caseInsensitive]) != nil
}

private func isSessionLabel(_ line: String) -> Bool {
    matches(line, #"current\s*session|session\s*(?:limit|usage)"#)
}

private func isWeeklyLabel(_ line: String) -> Bool { matches(line, weeklyPattern) }

private func isFableLabel(_ line: String) -> Bool {
    // A standalone "Fable" heading, or a weekly line scoped to Fable.
    line.trimmingCharacters(in: .whitespaces).lowercased() == "fable"
        || (isWeeklyLabel(line) && containsFable(line))
}

private func isSectionLabel(_ line: String) -> Bool {
    isSessionLabel(line) || isWeeklyLabel(line) || isFableLabel(line)
}

// MARK: - Percent extraction

/// Finds the label line, then returns the first percent token on that line or
/// the next few lines, stopping at the next (different) section heading.
private func firstPercent(in lines: [String], isLabel: (String) -> Bool) -> Int? {
    for i in lines.indices where isLabel(lines[i]) {
        for j in i..<min(i + 4, lines.count) {
            if j > i, isSectionLabel(lines[j]), !isLabel(lines[j]) { break }
            if let p = percentToken(lines[j]) { return p }
        }
    }
    return nil
}

/// `"12% used"` → 12, `"84% left"` → 16 (remaining inverted), bare `"62%"` → 62.
private func percentToken(_ line: String) -> Int? {
    guard let r = line.range(of: #"\d{1,3}\s*%"#, options: .regularExpression) else { return nil }
    let digits = line[r].prefix(while: { $0.isNumber })
    guard let n = Int(digits) else { return nil }
    let used = line.lowercased().contains("left") ? (100 - n) : n
    return min(100, max(0, used))
}

// MARK: - Fetch outcome & view state

public enum UsageReason: Equatable, Sendable {
    case notInstalled, loggedOut, timedOut, error
}

public enum UsageFetchOutcome: Equatable, Sendable {
    case success(ProviderUsage)
    case unavailable(UsageReason)
    case timedOut
}

public enum ProviderUsageState: Equatable, Sendable {
    case loading
    case loaded(ProviderUsage)
    case stale(ProviderUsage)
    case unavailable(UsageReason)
}

/// Maps a fetch outcome onto the next view state. A timeout keeps the last
/// good value (dimmed as `.stale`) rather than blanking the bar.
public enum UsageStateReducer {
    public static func reduce(
        outcome: UsageFetchOutcome, previous: ProviderUsageState
    ) -> ProviderUsageState {
        switch outcome {
        case .success(let usage):
            return .loaded(usage)
        case .unavailable(let reason):
            return .unavailable(reason)
        case .timedOut:
            if let prev = lastUsage(previous) { return .stale(prev) }
            return .unavailable(.timedOut)
        }
    }

    private static func lastUsage(_ state: ProviderUsageState) -> ProviderUsage? {
        switch state {
        case .loaded(let u), .stale(let u): return u
        case .loading, .unavailable: return nil
        }
    }
}
