import Foundation

/// A subagent spawn extracted from the transcript (a tool call whose input
/// carries `subagent_type` — the Claude Code Task tool schema).
public struct SubagentTaskInfo: Equatable, Sendable {
    public let toolCallId: String
    public let title: String
    public let status: ToolCallStatus

    public init(toolCallId: String, title: String, status: ToolCallStatus) {
        self.toolCallId = toolCallId
        self.title = title
        self.status = status
    }
}

public enum SubagentTasks {
    /// Tool calls that spawned a subagent, in transcript order.
    public static func extract(from items: [TranscriptItem]) -> [SubagentTaskInfo] {
        items.compactMap { item in
            guard case .toolCall(let call) = item,
                  case .object(let input)? = call.rawInput,
                  input["subagent_type"] != nil else { return nil }
            let description: String? =
                if case .string(let text)? = input["description"] { text } else { nil }
            let title = description ?? (call.title.isEmpty ? nil : call.title) ?? "Subagent"
            return SubagentTaskInfo(toolCallId: call.toolCallId,
                                    title: title, status: call.status)
        }
    }
}
