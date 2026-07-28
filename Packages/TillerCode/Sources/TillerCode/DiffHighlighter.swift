import Foundation

public struct LineHighlightMap: Sendable {
    private let rangesByLine: [[SyntaxHighlightRange]]

    public init?(text: String, language: TillerCodeLanguage) {
        guard let highlights = TreeSitterHighlighter.ranges(
            in: text, language: language) else { return nil }
        let source = text as NSString
        var contentRanges: [NSRange] = []
        var fullRanges: [NSRange] = []
        var cursor = 0
        while cursor < source.length {
            var start = 0
            var end = 0
            var contentsEnd = 0
            source.getLineStart(
                &start, end: &end, contentsEnd: &contentsEnd,
                for: NSRange(location: cursor, length: 0))
            contentRanges.append(NSRange(
                location: start, length: contentsEnd - start))
            fullRanges.append(NSRange(location: start, length: end - start))
            cursor = max(end, cursor + 1)
        }

        var rows = Array(repeating: [SyntaxHighlightRange](),
                         count: contentRanges.count)
        for highlight in highlights where highlight.range.length > 0 {
            guard let first = Self.lineIndex(
                    containing: highlight.range.location, in: fullRanges),
                  let last = Self.lineIndex(
                    containing: NSMaxRange(highlight.range) - 1, in: fullRanges)
            else { continue }
            for index in first...last {
                let line = contentRanges[index]
                let intersection = NSIntersectionRange(line, highlight.range)
                guard intersection.length > 0 else { continue }
                rows[index].append(SyntaxHighlightRange(
                    range: NSRange(
                        location: intersection.location - line.location,
                        length: intersection.length),
                    role: highlight.role))
            }
        }
        rangesByLine = rows
    }

    public func ranges(forLine lineNumber: Int) -> [SyntaxHighlightRange] {
        guard lineNumber > 0, lineNumber <= rangesByLine.count else { return [] }
        return rangesByLine[lineNumber - 1]
    }

    private static func lineIndex(
        containing location: Int,
        in ranges: [NSRange]
    ) -> Int? {
        var low = 0
        var high = ranges.count - 1
        while low <= high {
            let middle = (low + high) / 2
            let range = ranges[middle]
            if location < range.location { high = middle - 1 }
            else if location >= NSMaxRange(range) { low = middle + 1 }
            else { return middle }
        }
        return nil
    }
}

public struct DiffHighlights: Sendable {
    public static let byteLimit = 500_000
    public let old: LineHighlightMap?
    public let new: LineHighlightMap?

    public init(oldText: String?, newText: String?, language: TillerCodeLanguage) {
        old = Self.map(text: oldText, language: language)
        new = Self.map(text: newText, language: language)
    }

    private static func map(
        text: String?,
        language: TillerCodeLanguage
    ) -> LineHighlightMap? {
        guard let text, text.utf8.count <= byteLimit else { return nil }
        return LineHighlightMap(text: text, language: language)
    }
}

public actor DiffHighlightCache {
    public static let shared = DiffHighlightCache()

    private struct Key: Hashable {
        let path: String
        let oldHash: Int?
        let newHash: Int?
    }
    private var entries: [Key: DiffHighlights] = [:]
    private static let maxEntries = 32

    public func highlights(
        path: URL,
        oldText: String?,
        newText: String?
    ) -> DiffHighlights {
        let key = Key(
            path: path.standardizedFileURL.path,
            oldHash: oldText?.hashValue,
            newHash: newText?.hashValue)
        if let cached = entries[key] { return cached }
        let sample = newText ?? oldText ?? ""
        let language = CodeLanguageResolver.language(for: path, contents: sample)
        let value = DiffHighlights(
            oldText: oldText, newText: newText, language: language)
        if entries.count >= Self.maxEntries { entries.removeAll(keepingCapacity: true) }
        entries[key] = value
        return value
    }
}
