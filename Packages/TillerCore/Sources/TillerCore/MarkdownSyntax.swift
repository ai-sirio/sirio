import Foundation

/// Trasformazioni testuali pure usate dalla toolbar dell'editor markdown.
/// Gli indici della selezione risultante sono ricalcolati via offset:
/// dopo replaceSubrange gli indici della stringa originale non sono
/// garantiti validi sulla nuova.
public enum MarkdownSyntax {
    public static func wrap(_ text: String, selection: Range<String.Index>,
                            prefix: String, suffix: String)
        -> (text: String, selection: Range<String.Index>) {
        let startOffset = text.distance(from: text.startIndex, to: selection.lowerBound)
        let selected = String(text[selection])
        var out = text
        out.replaceSubrange(selection, with: prefix + selected + suffix)
        let newStart = out.index(out.startIndex, offsetBy: startOffset + prefix.count)
        let newEnd = out.index(newStart, offsetBy: selected.count)
        return (out, newStart..<newEnd)
    }

    public static func prefixLines(_ text: String, selection: Range<String.Index>,
                                   linePrefix: String)
        -> (text: String, selection: Range<String.Index>) {
        let lineRange = text.lineRange(for: selection)
        let block = String(text[lineRange])
        let endsWithNewline = block.hasSuffix("\n")
        let core = endsWithNewline ? String(block.dropLast()) : block
        let prefixed = core
            .split(separator: "\n", omittingEmptySubsequences: false)
            .map { linePrefix + $0 }
            .joined(separator: "\n") + (endsWithNewline ? "\n" : "")
        let startOffset = text.distance(from: text.startIndex, to: lineRange.lowerBound)
        var out = text
        out.replaceSubrange(lineRange, with: prefixed)
        let newStart = out.index(out.startIndex, offsetBy: startOffset)
        let newEnd = out.index(newStart, offsetBy: prefixed.count)
        return (out, newStart..<newEnd)
    }
}
