import Foundation

/// Line counts for a tool call's diff payload. The card shows these so a
/// truncated preview still tells the truth about the change's size.
public enum DiffStats {
    public static func counts(oldText: String?, newText: String)
        -> (added: Int, removed: Int) {
        (added: lineCount(newText), removed: lineCount(oldText))
    }

    /// Blank lines in the middle count; a trailing newline does not invent a
    /// final empty line.
    private static func lineCount(_ text: String?) -> Int {
        guard let text, !text.isEmpty else { return 0 }
        var lines = text.split(separator: "\n", omittingEmptySubsequences: false)
        if lines.last?.isEmpty == true { lines.removeLast() }
        return lines.count
    }
}
