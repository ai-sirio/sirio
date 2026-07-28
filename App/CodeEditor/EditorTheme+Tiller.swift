import AppKit
import CodeEditSourceEditor
import TillerCode

extension EditorTheme {
    static func tiller(isDark: Bool) -> EditorTheme {
        let text = NSColor(srgbRed: isDark ? 0.85 : 0.15,
                           green: isDark ? 0.86 : 0.16,
                           blue: isDark ? 0.89 : 0.20, alpha: 1)
        let keyword = NSColor(srgbRed: isDark ? 0.78 : 0.43,
                              green: isDark ? 0.58 : 0.24,
                              blue: isDark ? 0.96 : 0.66, alpha: 1)
        let type = NSColor(srgbRed: isDark ? 0.39 : 0.05,
                           green: isDark ? 0.78 : 0.48,
                           blue: isDark ? 0.84 : 0.53, alpha: 1)
        let string = NSColor(srgbRed: isDark ? 0.63 : 0.12,
                             green: isDark ? 0.82 : 0.50,
                             blue: isDark ? 0.58 : 0.20, alpha: 1)
        let number = NSColor(srgbRed: isDark ? 0.91 : 0.68,
                             green: isDark ? 0.69 : 0.35,
                             blue: isDark ? 0.36 : 0.04, alpha: 1)
        let comment = NSColor(srgbRed: isDark ? 0.52 : 0.38,
                              green: isDark ? 0.56 : 0.42,
                              blue: isDark ? 0.62 : 0.45, alpha: 1)
        return EditorTheme(
            text: .init(color: text),
            insertionPoint: isDark ? .white : .black,
            invisibles: .init(color: comment.withAlphaComponent(0.65)),
            background: NSColor(
                srgbRed: isDark ? 0.071 : 0.965,
                green: isDark ? 0.071 : 0.965,
                blue: isDark ? 0.086 : 0.975,
                alpha: 1),
            lineHighlight: NSColor(
                srgbRed: isDark ? 0.11 : 0.91,
                green: isDark ? 0.11 : 0.92,
                blue: isDark ? 0.14 : 0.95,
                alpha: 1),
            selection: NSColor.selectedTextBackgroundColor,
            keywords: .init(color: keyword, bold: true),
            commands: .init(color: keyword),
            types: .init(color: type),
            attributes: .init(color: keyword),
            variables: .init(color: text),
            values: .init(color: text),
            numbers: .init(color: number),
            strings: .init(color: string),
            characters: .init(color: string),
            comments: .init(color: comment, italic: true))
    }

    func attribute(for role: SyntaxHighlightRole) -> Attribute {
        switch role {
        case .keyword, .tag: keywords
        case .comment: comments
        case .type: types
        case .attribute: attributes
        case .number: numbers
        case .string: strings
        case .variable, .property, .function, .method, .parameter: variables
        case .value: values
        }
    }
}

extension CodeHighlightTheme {
    static func tiller(isDark: Bool) -> CodeHighlightTheme {
        let text = CodeHighlightColor(
            red: isDark ? 0.85 : 0.15,
            green: isDark ? 0.86 : 0.16,
            blue: isDark ? 0.89 : 0.20)
        let keyword = CodeHighlightColor(
            red: isDark ? 0.78 : 0.43,
            green: isDark ? 0.58 : 0.24,
            blue: isDark ? 0.96 : 0.66)
        let type = CodeHighlightColor(
            red: isDark ? 0.39 : 0.05,
            green: isDark ? 0.78 : 0.48,
            blue: isDark ? 0.84 : 0.53)
        let string = CodeHighlightColor(
            red: isDark ? 0.63 : 0.12,
            green: isDark ? 0.82 : 0.50,
            blue: isDark ? 0.58 : 0.20)
        let number = CodeHighlightColor(
            red: isDark ? 0.91 : 0.68,
            green: isDark ? 0.69 : 0.35,
            blue: isDark ? 0.36 : 0.04)
        let comment = CodeHighlightColor(
            red: isDark ? 0.52 : 0.38,
            green: isDark ? 0.56 : 0.42,
            blue: isDark ? 0.62 : 0.45)
        return CodeHighlightTheme(
            text: .init(color: text),
            roles: [
                .keyword: .init(color: keyword, bold: true),
                .tag: .init(color: keyword),
                .comment: .init(color: comment, italic: true),
                .type: .init(color: type),
                .attribute: .init(color: keyword),
                .number: .init(color: number),
                .string: .init(color: string),
                .variable: .init(color: text),
                .property: .init(color: text),
                .function: .init(color: text),
                .method: .init(color: text),
                .parameter: .init(color: text),
                .value: .init(color: text)
            ])
    }
}
