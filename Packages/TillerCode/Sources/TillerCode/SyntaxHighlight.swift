import Foundation

public enum SyntaxHighlightRole: String, Sendable, Hashable {
    case keyword, comment, variable, property, function, method
    case number, string, type, parameter, attribute, value, tag
}

public struct SyntaxHighlightRange: Sendable, Hashable {
    public let range: NSRange
    public let role: SyntaxHighlightRole

    public init(range: NSRange, role: SyntaxHighlightRole) {
        self.range = range
        self.role = role
    }
}
