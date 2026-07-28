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

    /// Sort for lists that also carry a user-defined manual order: only the
    /// statuses that are blocked on the user (`error`, `needsInput`) float to
    /// the top, everything else stays exactly where the user dragged it.
    ///
    /// A full `sorted(_:statusOf:)` would fight the manual order — rows would
    /// reshuffle on their own whenever an agent changed state, making a drag
    /// look like it had been ignored.
    public static func urgentFirst<T: Identifiable>(
        _ items: [T],
        statusOf: (T) -> AgentStatus?
    ) -> [T] {
        // Stable: `enumerated` keeps the manual order as the tie-break, both
        // among urgent rows of equal rank and among all the non-urgent ones.
        items.enumerated()
            .sorted { lhs, rhs in
                let left = urgencyRank(statusOf(lhs.element))
                let right = urgencyRank(statusOf(rhs.element))
                return left == right ? lhs.offset < rhs.offset : left < right
            }
            .map(\.element)
    }

    /// Rank used by `urgentFirst`: urgent statuses keep their relative
    /// `priorityOrder`, everything else collapses into one non-urgent bucket.
    private static func urgencyRank(_ status: AgentStatus?) -> Int {
        guard let status, urgentStatuses.contains(status) else { return urgentStatuses.count }
        return AgentStatus.priorityOrder.firstIndex(of: status) ?? urgentStatuses.count
    }

    /// The statuses that are waiting on the user rather than on the machine.
    private static let urgentStatuses: Set<AgentStatus> = [.error, .needsInput]

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
