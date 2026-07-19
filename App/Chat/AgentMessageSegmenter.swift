import Foundation

/// A slice of an agent's raw markdown reply, split apart so `★ Insight ───`
/// callouts can render as their own card instead of inline prose.
enum AgentMessageSegment: Equatable {
    case prose(String)
    case insight(String)
}

/// Splits an agent reply's markdown into prose and insight segments.
///
/// Insight callouts are written wrapped in a fenced code block (per the
/// output-style convention: "★ Insight ───" header line, body, "───"
/// footer line) — that fence is exactly why `AttributedString(markdown:)`
/// renders them as one opaque monospaced code span instead of styled prose
/// (see `MarkdownAttributedStringRenderer`). Pulling them out here, before
/// that renderer ever sees them, lets each half use the rendering it
/// actually needs: prose through `AgentMarkdownTextView`, insights through
/// `InsightCardView`.
enum AgentMessageSegmenter {
    private static let insightFence = try! NSRegularExpression(
        pattern: "```\\n★ Insight [─]+\\n([\\s\\S]*?)\\n[─]+\\n```",
        options: [])

    static func segments(from markdown: String) -> [AgentMessageSegment] {
        let full = markdown as NSString
        var result: [AgentMessageSegment] = []
        var cursor = 0
        let matches = insightFence.matches(in: markdown, range: NSRange(location: 0, length: full.length))
        for match in matches {
            let proseRange = NSRange(location: cursor, length: match.range.location - cursor)
            if proseRange.length > 0 {
                result.append(.prose(full.substring(with: proseRange)))
            }
            result.append(.insight(full.substring(with: match.range(at: 1))))
            cursor = match.range.location + match.range.length
        }
        let tailRange = NSRange(location: cursor, length: full.length - cursor)
        if tailRange.length > 0 {
            result.append(.prose(full.substring(with: tailRange)))
        }
        return result
    }
}
