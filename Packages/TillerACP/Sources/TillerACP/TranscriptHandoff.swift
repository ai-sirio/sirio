import Foundation

/// Serializes a transcript into the "previous conversation" preamble sent to
/// a newly selected agent. Pure — trivially testable.
public enum TranscriptHandoff {
    public static let maxBytes = 100_000

    public static func preamble(items: [TranscriptItem]) -> String? {
        let lines = items.compactMap(line(for:))
        guard !lines.isEmpty else { return nil }

        var included: [String] = []
        var total = 0
        var truncated = false
        for line in lines.reversed() {           // newest kept first
            let cost = line.utf8.count + 1
            if total + cost > maxBytes { truncated = true; break }
            included.append(line)
            total += cost
        }
        var body = Array(included.reversed()).joined(separator: "\n")
        if truncated { body = "(older messages truncated)\n" + body }
        return """
        Previous conversation with another agent (for context — continue from here):

        \(body)

        ---
        """
    }

    private static func line(for item: TranscriptItem) -> String? {
        switch item {
        case .userMessage(_, let blocks):
            let text = blocks.compactMap { block -> String? in
                if case .text(let value) = block { return value }
                return nil
            }.joined(separator: "\n")
            return text.isEmpty ? nil : "User: \(text)"
        case .agentMessage(_, let text, _):
            return text.isEmpty ? nil : "Agent: \(text)"
        case .toolCall(let call):
            return "- [tool] \(call.title) → \(call.status.rawValue)"
        case .thought, .plan, .turnDivider, .editSummary, .systemNotice:
            return nil
        }
    }
}
