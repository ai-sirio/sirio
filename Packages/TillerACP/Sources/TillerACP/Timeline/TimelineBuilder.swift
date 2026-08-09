import Foundation

/// UI-owned inputs to row derivation: expansion toggles, streaming flag, and
/// per-turn durations from the reducer.
public struct TimelineState: Sendable, Equatable {
    public var expandedWorkGroups: Set<String>
    public var unfoldedTurns: Set<String>
    public var isStreaming: Bool
    /// turnDivider id -> seconds; from `TranscriptReducer.turnDurations`.
    public var turnDurations: [String: TimeInterval]

    public init(expandedWorkGroups: Set<String> = [],
                unfoldedTurns: Set<String> = [],
                isStreaming: Bool = false,
                turnDurations: [String: TimeInterval] = [:]) {
        self.expandedWorkGroups = expandedWorkGroups
        self.unfoldedTurns = unfoldedTurns
        self.isStreaming = isStreaming
        self.turnDurations = turnDurations
    }
}

/// Pure derivation of timeline rows from the canonical transcript, mirroring
/// t3code's MessagesTimeline.logic.ts. No I/O, no view types.
public enum TimelineBuilder {
    /// Entries shown by a collapsed work group (t3code's MAX_VISIBLE).
    public static let maxVisibleWorkEntries = 1
    /// Most-recent turns that stay open by default (current + previous).
    public static let openTurnCount = 2

    public static func rows(items: [TranscriptItem],
                            state: TimelineState) -> [TimelineRow] {
        let turns = segment(items)
        var rows: [TimelineRow] = []
        for (index, turn) in turns.enumerated() {
            let turnId = turn.divider?.id ?? "open"
            let isFoldable = index < turns.count - openTurnCount
                && turn.divider != nil && !turn.items.isEmpty
            if isFoldable, !state.unfoldedTurns.contains(turnId),
               let divider = turn.divider {
                rows.append(.turnFold(turnId: turnId, label: turn.label,
                                      at: divider.at))
                continue
            }
            rows.append(contentsOf: turnRows(turn, turnId: turnId, state: state))
            if let divider = turn.divider {
                rows.append(.turnDivider(id: divider.id, at: divider.at))
            }
        }
        if state.isStreaming { rows.append(.working) }
        return rows
    }

    // MARK: - Turn segmentation

    private struct Turn {
        var items: [TranscriptItem] = []
        var divider: (id: String, at: Date)?

        /// First line of the first user message, for the fold row.
        var label: String {
            for item in items {
                guard case .userMessage(_, let blocks) = item else { continue }
                for block in blocks {
                    if case .text(let text) = block {
                        let line = text.split(separator: "\n").first
                            .map(String.init) ?? text
                        return String(line.prefix(60))
                    }
                }
            }
            return "Turn"
        }
    }

    private static func segment(_ items: [TranscriptItem]) -> [Turn] {
        var turns: [Turn] = []
        var current = Turn()
        for item in items {
            if case .turnDivider(let id, let at) = item {
                current.divider = (id, at)
                turns.append(current)
                current = Turn()
            } else {
                current.items.append(item)
            }
        }
        if !current.items.isEmpty { turns.append(current) }
        return turns
    }

    // MARK: - Rows within one open turn

    private static func turnRows(_ turn: Turn, turnId: String,
                                 state: TimelineState) -> [TimelineRow] {
        var rows: [TimelineRow] = []
        var pendingWork: [TimelineRow.WorkEntry] = []
        let lastAgentMessageId = turn.items.last {
            if case .agentMessage = $0 { return true }
            return false
        }?.id
        let planApproval = turn.items.compactMap { item -> PermissionState? in
            guard case .toolCall(let call) = item, call.kind == .switchMode,
                  call.permission?.isPending == true else { return nil }
            return call.permission
        }.first

        func flushWork() {
            guard let first = pendingWork.first else { return }
            let groupId = "wg-\(first.id)"
            rows.append(.work(groupId: groupId, entries: pendingWork,
                              isExpanded: state.expandedWorkGroups.contains(groupId)))
            pendingWork = []
        }

        for item in turn.items {
            switch item {
            case .toolCall(let call):
                pendingWork.append(TimelineRow.WorkEntry(item: call))
            case .plan(let id, let entries):
                flushWork()
                rows.append(.proposedPlan(id: id, entries: entries,
                                          approval: planApproval))
            case .agentMessage(_, _, let isComplete):
                flushWork()
                var meta: TimelineRow.MessageMeta?
                if item.id == lastAgentMessageId {
                    meta = TimelineRow.MessageMeta(
                        at: turn.divider?.at,
                        duration: turn.divider.flatMap { state.turnDurations[$0.id] },
                        showsCopyButton: isComplete
                            && (turn.divider != nil || !state.isStreaming))
                }
                rows.append(.message(item: item, meta: meta))
            case .turnDivider:
                break // never inside a segmented turn
            default:
                flushWork()
                rows.append(.message(item: item, meta: nil))
            }
        }
        flushWork()
        return rows
    }
}
