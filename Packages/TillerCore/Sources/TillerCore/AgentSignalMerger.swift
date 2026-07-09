import Foundation

/// Combines Layer A (explicit `tillerctl notify` hook pushes) and Layer B
/// (terminal-title-derived status) into one decision per pane: a
/// title-derived signal is dropped if a hook signal for the same pane
/// landed very recently, so a lagging title glyph can't stomp a fresher
/// explicit hook event. Deliberately simple — last-write-wins otherwise, no
/// staleness/retention system.
public enum AgentSignalMerger {
    public static let debounceInterval: TimeInterval = 1.5

    public static func shouldApplyTitleSignal(
        lastHookUpdateAt: Date?,
        now: Date,
        debounceInterval: TimeInterval = debounceInterval
    ) -> Bool {
        guard let lastHookUpdateAt else { return true }
        return now.timeIntervalSince(lastHookUpdateAt) >= debounceInterval
    }
}
