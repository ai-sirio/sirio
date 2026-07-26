import Foundation

/// A subagent spawn extracted from the transcript (a tool call whose input
/// carries `subagent_type` — the Claude Code Task tool schema).
public struct SubagentTaskInfo: Equatable, Sendable {
    public let toolCallId: String
    public let title: String
    public let status: ToolCallStatus
    public let subagentType: String

    public init(toolCallId: String, title: String, status: ToolCallStatus,
                subagentType: String) {
        self.toolCallId = toolCallId
        self.title = title
        self.status = status
        self.subagentType = subagentType
    }
}

public enum SubagentTasks {
    /// The subagent spawned by a tool call, or nil if it is a plain call.
    public static func info(for call: ToolCallItem) -> SubagentTaskInfo? {
        guard case .object(let input)? = call.rawInput,
              case .string(let subagentType)? = input["subagent_type"] else { return nil }
        let description: String? =
            if case .string(let text)? = input["description"] { text } else { nil }
        let title = description ?? (call.title.isEmpty ? nil : call.title) ?? "Subagent"
        return SubagentTaskInfo(toolCallId: call.toolCallId, title: title,
                                status: call.status, subagentType: subagentType)
    }

    /// Tool calls that spawned a subagent, in transcript order.
    public static func extract(from items: [TranscriptItem]) -> [SubagentTaskInfo] {
        items.compactMap { item in
            guard case .toolCall(let call) = item else { return nil }
            return info(for: call)
        }
    }
}
