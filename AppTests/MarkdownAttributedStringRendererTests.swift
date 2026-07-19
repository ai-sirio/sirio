import AppKit
import Testing
@testable import Tiller

struct MarkdownAttributedStringRendererTests {
    @Test("plain paragraph uses 13pt body font")
    func plainParagraph() {
        let result = MarkdownAttributedStringRenderer.render("Hello world")
        #expect(result.string == "Hello world")
        let font = result.attribute(.font, at: 0, effectiveRange: nil) as? NSFont
        #expect(font?.pointSize == 13)
    }

    @Test("bold text gets a bold font")
    func boldText() {
        let result = MarkdownAttributedStringRenderer.render("**bold**")
        #expect(result.string == "bold")
        let font = result.attribute(.font, at: 0, effectiveRange: nil) as? NSFont
        #expect(font?.fontDescriptor.symbolicTraits.contains(.bold) == true)
    }

    @Test("italic text gets an italic font")
    func italicText() {
        let result = MarkdownAttributedStringRenderer.render("*italic*")
        #expect(result.string == "italic")
        let font = result.attribute(.font, at: 0, effectiveRange: nil) as? NSFont
        #expect(font?.fontDescriptor.symbolicTraits.contains(.italic) == true)
    }

    @Test("inline code uses 12pt monospaced font")
    func inlineCode() {
        let result = MarkdownAttributedStringRenderer.render("`code`")
        #expect(result.string == "code")
        let font = result.attribute(.font, at: 0, effectiveRange: nil) as? NSFont
        #expect(font?.pointSize == 12)
        #expect(font?.fontDescriptor.symbolicTraits.contains(.monoSpace) == true)
    }

    @Test("strikethrough text gets the strikethrough attribute")
    func strikethroughText() {
        let result = MarkdownAttributedStringRenderer.render("~~gone~~")
        #expect(result.string == "gone")
        let style = result.attribute(.strikethroughStyle, at: 0, effectiveRange: nil) as? Int
        #expect(style == NSUnderlineStyle.single.rawValue)
    }

    @Test("link text carries the .link attribute with the right URL")
    func linkText() {
        let result = MarkdownAttributedStringRenderer.render("[Tiller](https://example.com)")
        #expect(result.string == "Tiller")
        let link = result.attribute(.link, at: 0, effectiveRange: nil) as? NSURL
        #expect(link?.absoluteString == "https://example.com")
    }
}
