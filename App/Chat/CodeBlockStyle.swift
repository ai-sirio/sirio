import AppKit

/// Shared metrics, attribute keys and colors for T3-style code rendering.
/// Custom attributes travel with the attributed string: the renderer writes
/// them, `CodeBlockLayoutManager` draws from them, and `MarkdownTextView`
/// positions header overlays from the same ranges.
enum CodeBlockStyle {
    /// Marks inline `code` runs. Value: `true`.
    static let inlineCodeAttribute = NSAttributedString.Key("tiller.inlineCode")
    /// Marks fenced code-block ranges. Value: `CodeBlockInfo`.
    static let codeBlockAttribute = NSAttributedString.Key("tiller.codeBlock")

    static let headerHeight: CGFloat = 28
    static let cardPadding: CGFloat = 12
    static let cardCornerRadius: CGFloat = 8
    static let chipCornerRadius: CGFloat = 4

    static let cardFill = NSColor(name: nil) { appearance in
        appearance.bestMatch(from: [.darkAqua, .aqua]) == .darkAqua
            ? NSColor.black.withAlphaComponent(0.25)
            : NSColor.black.withAlphaComponent(0.04)
    }
    static let cardBorder = NSColor.separatorColor
    static let chipFill = NSColor(name: nil) { appearance in
        appearance.bestMatch(from: [.darkAqua, .aqua]) == .darkAqua
            ? NSColor.white.withAlphaComponent(0.09)
            : NSColor.black.withAlphaComponent(0.06)
    }
}

/// Metadata attached to a code block's range via `codeBlockAttribute`.
/// `index` keeps adjacent same-language blocks from merging into one
/// attribute range and identifies the block's header overlay.
struct CodeBlockInfo: Equatable {
    let language: String
    let index: Int
}
