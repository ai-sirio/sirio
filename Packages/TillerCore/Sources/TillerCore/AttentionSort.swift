import Foundation

/// Stable sort of an Identifiable list by agent-status urgency, so the rows
/// most needing the user come first. SwiftUI-free so TillerCore stays UI-agnostic;
/// generic so the same logic serves any `Identifiable` list via a status closure.
///
/// Order: error > needsInput > running > done > idle (nil status), last.
/// The urgency ranking is `AgentStatus.priorityOrder` — the single source of
/// truth shared with the sidebar badge color and the status rollup, so the sort,
/// the badge, and the rollup can never disagree on what "more urgent" means.
/// Same-rank items keep their input order (Swift's `sorted(by:)` is stable).
public enum AttentionSort {
    public static func sorted<T: Identifiable>(
        _ items: [T],
        statusOf: (T) -> AgentStatus?
    ) -> [T] {
        items.sorted { rank(statusOf($0)) < rank(statusOf($1)) }
    }

    /// Index of `status` in `AgentStatus.priorityOrder`; a nil status ranks
    /// after every known status (idle sinks to the bottom).
    private static func rank(_ status: AgentStatus?) -> Int {
        guard let status else { return AgentStatus.priorityOrder.count }
        // Defensive guardrail: if a future AgentStatus case is added without
        // updating priorityOrder, treat it as idle-equivalent (sorted last).
        // Unreachable with the current 4-case enum — every case is in priorityOrder.
        return AgentStatus.priorityOrder.firstIndex(of: status) ?? AgentStatus.priorityOrder.count
    }
}
