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
    @Test("h1 heading is 15pt semibold")
    func heading1() {
        let result = MarkdownAttributedStringRenderer.render("# Title")
        #expect(result.string == "Title")
        let font = result.attribute(.font, at: 0, effectiveRange: nil) as? NSFont
        #expect(font?.pointSize == 15)
        #expect(font?.fontDescriptor.symbolicTraits.contains(.bold) == true)
    }

    @Test("h2 heading is 14pt semibold")
    func heading2() {
        let result = MarkdownAttributedStringRenderer.render("## Section")
        let font = result.attribute(.font, at: 0, effectiveRange: nil) as? NSFont
        #expect(font?.pointSize == 14)
    }

    @Test("h3 heading is 13pt semibold")
    func heading3() {
        let result = MarkdownAttributedStringRenderer.render("### Subsection")
        let font = result.attribute(.font, at: 0, effectiveRange: nil) as? NSFont
        #expect(font?.pointSize == 13)
        #expect(font?.fontDescriptor.symbolicTraits.contains(.bold) == true)
    }

    @Test("h4 falls back to h3 sizing")
    func heading4FallsBackToH3() {
        let result = MarkdownAttributedStringRenderer.render("#### Deep")
        let font = result.attribute(.font, at: 0, effectiveRange: nil) as? NSFont
        #expect(font?.pointSize == 13)
        #expect(font?.fontDescriptor.symbolicTraits.contains(.bold) == true)
    }

    @Test("fenced code block uses 12pt monospaced font")
    func codeBlock() {
        let result = MarkdownAttributedStringRenderer.render("```\nlet x = 1\n```")
        #expect(result.string.contains("let x = 1"))
        let range = (result.string as NSString).range(of: "let x = 1")
        let font = result.attribute(.font, at: range.location, effectiveRange: nil) as? NSFont
        #expect(font?.pointSize == 12)
        #expect(font?.fontDescriptor.symbolicTraits.contains(.monoSpace) == true)
    }

    @Test("blockquote is indented and secondary-colored")
    func blockquote() {
        let result = MarkdownAttributedStringRenderer.render("> quoted")
        #expect(result.string.contains("quoted"))
        let range = (result.string as NSString).range(of: "quoted")
        let style = result.attribute(.paragraphStyle, at: range.location, effectiveRange: nil) as? NSParagraphStyle
        #expect(style?.headIndent == 12)
        let color = result.attribute(.foregroundColor, at: range.location, effectiveRange: nil) as? NSColor
        #expect(color == .secondaryLabelColor)
    }

    @Test("unordered list item is indented")
    func unorderedListItem() {
        let result = MarkdownAttributedStringRenderer.render("- one\n- two")
        #expect(result.string.contains("one"))
        let range = (result.string as NSString).range(of: "one")
        let style = result.attribute(.paragraphStyle, at: range.location, effectiveRange: nil) as? NSParagraphStyle
        #expect(style?.headIndent == 16)
    }

    @Test("unclosed code fence falls back to plain selectable text")
    func unclosedFenceFallsBackToPlainText() {
        let input = "before `unterminated"
        let result = MarkdownAttributedStringRenderer.render(input)
        #expect(result.string == input)
        let font = result.attribute(.font, at: 0, effectiveRange: nil) as? NSFont
        #expect(font?.pointSize == MarkdownAttributedStringRenderer.bodySize)
    }

    @Test("consecutive paragraphs are separated by a line break, not run together")
    func consecutiveParagraphsAreSeparated() {
        let result = MarkdownAttributedStringRenderer.render("Para one\n\nPara two")
        #expect(result.string == "Para one\nPara two")
    }

    @Test("a heading is separated from the paragraph before it")
    func headingSeparatedFromPrecedingParagraph() {
        let result = MarkdownAttributedStringRenderer.render("Intro\n\n# Heading")
        #expect(result.string == "Intro\nHeading")
    }

    @Test("list items are separated by a line break, not run together")
    func listItemsAreSeparated() {
        let result = MarkdownAttributedStringRenderer.render("- one\n- two")
        #expect(result.string == "one\ntwo")
    }

    @Test("inline spans within one paragraph are NOT split by the block separator")
    func inlineSpansWithinOneParagraphStayJoined() {
        let result = MarkdownAttributedStringRenderer.render("This is **bold** text.")
        #expect(result.string == "This is bold text.")
    }

    @Test("list items stay tight (no extra paragraph spacing between them)")
    func listItemsHaveNoExtraSpacing() {
        let result = MarkdownAttributedStringRenderer.render("- one\n- two")
        let range = (result.string as NSString).range(of: "one")
        let style = result.attribute(.paragraphStyle, at: range.location, effectiveRange: nil) as? NSParagraphStyle
        #expect(style?.paragraphSpacing == 0)
    }

    @Test("inline code is colored, not just monospaced")
    func inlineCodeIsColored() {
        let result = MarkdownAttributedStringRenderer.render("`code`")
        let color = result.attribute(.foregroundColor, at: 0, effectiveRange: nil) as? NSColor
        #expect(color == .systemTeal)
    }

    @Test("fenced code block is colored, not just monospaced")
    func codeBlockIsColored() {
        let result = MarkdownAttributedStringRenderer.render("```\nlet x = 1\n```")
        let range = (result.string as NSString).range(of: "let x = 1")
        let color = result.attribute(.foregroundColor, at: range.location, effectiveRange: nil) as? NSColor
        #expect(color == .systemTeal)
    }

    @Test("an insight callout block is colored distinctly from normal prose")
    func insightCalloutIsColored() {
        let result = MarkdownAttributedStringRenderer.render("★ Insight ───\nSome point.\n───")
        let color = result.attribute(.foregroundColor, at: 0, effectiveRange: nil) as? NSColor
        #expect(color == .systemPurple)
    }

    @Test("prose before an insight callout keeps the normal label color")
    func proseBeforeInsightStaysNormal() {
        let result = MarkdownAttributedStringRenderer.render("Intro text.\n\n★ Insight ───\nPoint.\n───")
        let color = result.attribute(.foregroundColor, at: 0, effectiveRange: nil) as? NSColor
        #expect(color == .labelColor)
    }
}
