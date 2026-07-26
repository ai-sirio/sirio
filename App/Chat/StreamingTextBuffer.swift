import Foundation

enum StreamingTextEdit: Equatable, Sendable {
    case none
    case append(String)
    case reset(String)
}

struct StreamingTextBuffer: Equatable, Sendable {
    private(set) var current: String

    init(current: String = "") {
        self.current = current
    }

    mutating func update(to newText: String) -> StreamingTextEdit {
        let edit = Self.edit(current: current, newText: newText)
        current = newText
        return edit
    }

    private static func edit(current: String, newText: String) -> StreamingTextEdit {
        guard current != newText else { return .none }
        guard newText.utf8.starts(with: current.utf8) else { return .reset(newText) }

        let suffixStart = newText.utf8.index(
            newText.utf8.startIndex,
            offsetBy: current.utf8.count)
        let suffix = String(decoding: newText.utf8[suffixStart...], as: UTF8.self)
        return .append(suffix)
    }
}
