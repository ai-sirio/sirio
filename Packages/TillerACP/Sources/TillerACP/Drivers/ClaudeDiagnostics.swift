import Foundation

/// A non-conversational signal from the agent: hook lifecycle, request
/// status, thinking-token counters, background task progress, and
/// informational notifications. These reach the edge of TillerACP and are
/// not rendered in the transcript; the diagnostic panel that consumes them
/// is separate work.
public struct ClaudeDiagnostic: Sendable, Equatable {
    public enum Kind: String, Sendable, Equatable {
        case hook
        case status
        case thinkingTokens
        case taskProgress
        case informational
    }

    public let kind: Kind
    public let label: String
    public let detail: String?

    public init(kind: Kind, label: String, detail: String? = nil) {
        self.kind = kind
        self.label = label
        self.detail = detail
    }
}

public enum ClaudeDiagnostics {
    /// Returns nil for subtypes the transcript already renders inline, so a
    /// signal is never reported twice.
    public static func diagnostic(for event: ClaudeSystemEvent) -> ClaudeDiagnostic? {
        switch event.subtype {
        case "hook_started", "hook_progress", "hook_response":
            return ClaudeDiagnostic(
                kind: .hook,
                label: event.payload["hook_name"]?.stringValue ?? event.subtype,
                detail: event.subtype)
        case "status":
            return ClaudeDiagnostic(
                kind: .status,
                label: event.payload["status"]?.stringValue ?? "status")
        case "thinking_tokens":
            let total = event.payload["estimated_tokens"]?.intValue
            return ClaudeDiagnostic(
                kind: .thinkingTokens,
                label: "thinking",
                detail: total.map(String.init))
        case "task_progress":
            return ClaudeDiagnostic(
                kind: .taskProgress,
                label: event.payload["task_id"]?.stringValue ?? "task",
                detail: event.payload["summary"]?.stringValue)
        case "informational", "notification":
            return ClaudeDiagnostic(
                kind: .informational,
                label: event.subtype,
                detail: event.payload["message"]?.stringValue)
        default:
            return nil
        }
    }
}
