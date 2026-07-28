import AppKit
import CodeEditSourceEditor
import Testing
import TillerCode
@testable import Tiller

@MainActor
struct AttributedCodeRendererTests {
    @Test func appliesRoleColorAndPreservesPlainTextColor() throws {
        let theme = EditorTheme.tiller(isDark: true)
        let font = NSFont.monospacedSystemFont(ofSize: 11, weight: .regular)
        let rendered = NSAttributedString(AttributedCodeRenderer.renderLine(
            "let value",
            ranges: [SyntaxHighlightRange(
                range: NSRange(location: 0, length: 3), role: .keyword)],
            theme: theme,
            font: font))
        let keyword = try #require(rendered.attribute(
            .foregroundColor, at: 0, effectiveRange: nil) as? NSColor)
        let plain = try #require(rendered.attribute(
            .foregroundColor, at: 4, effectiveRange: nil) as? NSColor)
        #expect(keyword == theme.keywords.color)
        #expect(plain == theme.text.color)
    }

    @Test func ignoresOutOfBoundsRangesAndKeepsTextLength() {
        let rendered = AttributedCodeRenderer.renderLine(
            "let value = 1",
            ranges: [
                SyntaxHighlightRange(
                    range: NSRange(location: 0, length: 3), role: .keyword),
                SyntaxHighlightRange(
                    range: NSRange(location: 99, length: 2), role: .string)
            ],
            theme: .tiller(isDark: true),
            font: .monospacedSystemFont(ofSize: 11, weight: .regular))
        #expect(NSAttributedString(rendered).length == 13)
    }
}
