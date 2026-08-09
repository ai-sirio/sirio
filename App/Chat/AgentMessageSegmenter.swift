import Foundation

/// A slice of an agent's raw markdown reply, split apart so `★ Insight ───`
/// callouts can render as their own card instead of inline prose.
enum AgentMessageSegment: Equatable, Sendable {
    case prose(String)
    case insight(String)
    case diff(path: String, oldText: String?, newText: String)
}

/// Splits an agent reply's markdown into prose and insight segments.
///
/// Insight callouts are marked by a "★ Insight ───" header line and a
/// "───" footer line; the actual Explanatory output-style convention wraps
/// that in a single backtick span (not a fenced code block), which is why
/// nested inline-code backticks inside the body would otherwise break
/// `AttributedString(markdown:)` parsing into a mess of stray code spans
/// (see `MarkdownAttributedStringRenderer`). Some agent CLIs instead emit a
/// ``` fence, or no wrapper at all — the wrapper is tolerated but not
/// required; only the header/footer marker lines matter. Pulling the block
/// out here, before that renderer ever sees it, lets each half use the
/// rendering it actually needs: prose through `AgentMarkdownTextView`,
/// insights through `InsightCardView`.
enum AgentMessageSegmenter {
    private struct LocatedSegment {
        let range: Range<String.Index>
        let segment: AgentMessageSegment
    }

    private static let insightFence = try! NSRegularExpression(
        pattern: "(?:```\\n|`)?★ Insight [─]+\\n([\\s\\S]*?)\\n[─]+(?:\\n```|`)?",
        options: [])

    static func segments(from markdown: String) -> [AgentMessageSegment] {
        let full = markdown as NSString
        var located: [LocatedSegment] = []
        let matches = insightFence.matches(in: markdown, range: NSRange(location: 0, length: full.length))
        for match in matches {
            guard let range = Range(match.range, in: markdown) else { continue }
            located.append(LocatedSegment(
                range: range,
                segment: .insight(full.substring(with: match.range(at: 1)))))
        }
        located.append(contentsOf: unifiedDiffSegments(in: markdown))
        located.sort { $0.range.lowerBound < $1.range.lowerBound }

        var result: [AgentMessageSegment] = []
        var cursor = markdown.startIndex
        for entry in located {
            // A malformed or nested marker must not make us drop reply text.
            guard entry.range.lowerBound >= cursor else { continue }
            appendProse(from: markdown, range: cursor..<entry.range.lowerBound, to: &result)
            result.append(entry.segment)
            cursor = entry.range.upperBound
        }
        appendProse(from: markdown, range: cursor..<markdown.endIndex, to: &result)
        return result
    }

    private static func unifiedDiffSegments(in markdown: String) -> [LocatedSegment] {
        var result: [LocatedSegment] = []
        var lineStart = markdown.startIndex

        while lineStart < markdown.endIndex {
            let lineEnd = markdown[lineStart...].firstIndex(of: "\n") ?? markdown.endIndex
            let line = String(markdown[lineStart..<lineEnd])
            guard line.hasPrefix("diff --git ") else {
                lineStart = lineEnd < markdown.endIndex
                    ? markdown.index(after: lineEnd)
                    : markdown.endIndex
                continue
            }

            var blockEnd = lineEnd < markdown.endIndex
                ? markdown.index(after: lineEnd)
                : markdown.endIndex
            var nextLineStart = blockEnd
            while nextLineStart < markdown.endIndex {
                let nextLineEnd = markdown[nextLineStart...].firstIndex(of: "\n") ?? markdown.endIndex
                if markdown[nextLineStart..<nextLineEnd].hasPrefix("diff --git ") {
                    blockEnd = nextLineStart
                    break
                }
                blockEnd = nextLineEnd < markdown.endIndex
                    ? markdown.index(after: nextLineEnd)
                    : markdown.endIndex
                nextLineStart = nextLineEnd < markdown.endIndex
                    ? markdown.index(after: nextLineEnd)
                    : markdown.endIndex
            }

            let block = String(markdown[lineStart..<blockEnd])
            if let segment = parseUnifiedDiff(block) {
                result.append(LocatedSegment(
                    range: lineStart..<blockEnd,
                    segment: segment))
            }
            lineStart = blockEnd
        }

        return result
    }

    private static func parseUnifiedDiff(_ block: String) -> AgentMessageSegment? {
        guard let header = block.firstLine,
              let pathMarker = header.range(of: " b/") else { return nil }
        let path = String(header[pathMarker.upperBound...])
        guard !path.isEmpty else { return nil }

        var inHunk = false
        var oldLines: [String] = []
        var newLines: [String] = []
        for line in block.split(separator: "\n", omittingEmptySubsequences: false).map(String.init) {
            if line.hasPrefix("@@") {
                inHunk = true
                continue
            }
            guard inHunk else { continue }
            if line.hasPrefix("+++") || line.hasPrefix("---") || line.hasPrefix("\\") {
                continue
            }
            if line.hasPrefix("+") {
                newLines.append(String(line.dropFirst()))
            } else if line.hasPrefix("-") {
                oldLines.append(String(line.dropFirst()))
            } else if line.hasPrefix(" ") {
                let context = String(line.dropFirst())
                oldLines.append(context)
                newLines.append(context)
            }
        }

        guard inHunk, !oldLines.isEmpty || !newLines.isEmpty else { return nil }
        return .diff(
            path: path,
            oldText: oldLines.isEmpty ? nil : oldLines.joined(separator: "\n"),
            newText: newLines.joined(separator: "\n"))
    }

    private static func appendProse(
        from markdown: String,
        range: Range<String.Index>,
        to result: inout [AgentMessageSegment]
    ) {
        guard !range.isEmpty else { return }
        result.append(.prose(String(markdown[range])))
    }
}

private extension String {
    var firstLine: String? {
        guard !isEmpty else { return nil }
        let end = firstIndex(of: "\n") ?? endIndex
        return String(self[..<end])
    }
}
