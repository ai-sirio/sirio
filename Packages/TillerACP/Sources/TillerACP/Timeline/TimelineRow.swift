import Foundation

/// One renderable row of the chat timeline. Pure value derived from the
/// canonical transcript by TimelineBuilder; the view layer renders rows
/// without re-deriving grouping or folding.
public enum TimelineRow: Sendable, Equatable, Identifiable {
    /// Meta attached to assistant message rows (turn duration, copy action).
    public struct MessageMeta: Sendable, Equatable {
        public var at: Date?
        public var duration: TimeInterval?
        public var showsCopyButton: Bool

        public init(at: Date? = nil, duration: TimeInterval? = nil,
                    showsCopyButton: Bool = false) {
            self.at = at
            self.duration = duration
            self.showsCopyButton = showsCopyButton
        }
    }

    /// One tool call inside a work group, with its compact one-line label.
    public struct WorkEntry: Sendable, Equatable, Identifiable {
        public var item: ToolCallItem
        public var label: String

        public var id: String { item.id }

        public init(item: ToolCallItem) {
            self.item = item
            self.label = Self.compactLabel(item.title)
        }

        /// First line of the title, trailing "completed" stripped
        /// ("Read AppModel.swift completed" -> "Read AppModel.swift").
        static func compactLabel(_ title: String) -> String {
            var text = title.split(separator: "\n").first.map(String.init) ?? title
            text = text.trimmingCharacters(in: .whitespaces)
            if text.lowercased().hasSuffix("completed") {
                text = String(text.dropLast("completed".count))
                    .trimmingCharacters(in: .whitespaces)
            }
            return text
        }
    }

    /// Non-tool transcript item rendered as-is (user/agent message, thought,
    /// edit summary, system notice). `meta` is non-nil only for the turn's
    /// final assistant message.
    case message(item: TranscriptItem, meta: MessageMeta?)
    /// Consecutive tool calls of one turn, grouped. Collapsed shows only the
    /// last `TimelineBuilder.maxVisibleWorkEntries`; expanded shows all.
    case work(groupId: String, entries: [WorkEntry], isExpanded: Bool)
    /// A collapsed older turn (single expandable row, no divider follows).
    case turnFold(turnId: String, label: String, at: Date)
    case turnDivider(id: String, at: Date)
    case proposedPlan(id: String, entries: [PlanEntry], approval: PermissionState?)
    /// Trailing indicator while a turn is streaming.
    case working

    public var id: String {
        switch self {
        case .message(let item, _): "msg-\(item.id)"
        case .work(let groupId, _, _): "work-\(groupId)"
        case .turnFold(let turnId, _, _): "fold-\(turnId)"
        case .turnDivider(let id, _): "divider-\(id)"
        case .proposedPlan(let id, _, _): "plan-\(id)"
        case .working: "working"
        }
    }
}
