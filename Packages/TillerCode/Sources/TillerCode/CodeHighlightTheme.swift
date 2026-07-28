import AppKit

public struct CodeHighlightColor: Hashable, Sendable {
    public let red: Double
    public let green: Double
    public let blue: Double
    public let alpha: Double

    public init(red: Double, green: Double, blue: Double, alpha: Double = 1) {
        self.red = red
        self.green = green
        self.blue = blue
        self.alpha = alpha
    }

    @MainActor public var nsColor: NSColor {
        NSColor(srgbRed: red, green: green, blue: blue, alpha: alpha)
    }
}

public struct CodeHighlightStyle: Hashable, Sendable {
    public let color: CodeHighlightColor
    public let bold: Bool
    public let italic: Bool

    public init(color: CodeHighlightColor, bold: Bool = false, italic: Bool = false) {
        self.color = color
        self.bold = bold
        self.italic = italic
    }
}

public struct CodeHighlightTheme: Sendable {
    public let text: CodeHighlightStyle
    public let roles: [SyntaxHighlightRole: CodeHighlightStyle]

    public init(
        text: CodeHighlightStyle,
        roles: [SyntaxHighlightRole: CodeHighlightStyle]
    ) {
        self.text = text
        self.roles = roles
    }

    public func style(for role: SyntaxHighlightRole) -> CodeHighlightStyle {
        roles[role] ?? text
    }
}
