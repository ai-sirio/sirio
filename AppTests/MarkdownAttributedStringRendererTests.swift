import AppKit
import Testing
@testable import Tiller

struct MarkdownAttributedStringRendererTests {
    @Test("body paragraphs use the T3 line and paragraph rhythm")
    func bodyRhythm() {
        let result = MarkdownAttributedStringRenderer.render("Hello world")
        let style = result.attribute(.paragraphStyle, at: 0, effectiveRange: nil) as? NSParagraphStyle
        #expect(style?.lineSpacing == 4.5)
        #expect(style?.paragraphSpacing == 9)
    }

    @Test("plain paragraph uses 13pt body font")
    func plainParagraph() {
        let result = MarkdownAttributedStringRenderer.render("Hello world")
        #expect(result.string == "Hello world")
        let font = result.attribute(.font, at: 0, effectiveRange: nil) as? NSFont
        #expect(font?.pointSize == 13)
    }

    @Test("plain paragraph text is muted relative to full labelColor")
    func plainParagraphIsMuted() {
        let result = MarkdownAttributedStringRenderer.render("Hello world")
        let color = result.attribute(.foregroundColor, at: 0, effectiveRange: nil) as? NSColor
        #expect(color?.alphaComponent == 0.82)
    }

    @Test("a heading keeps full-strength labelColor, unlike body prose")
    func headingIsNotMuted() {
        let result = MarkdownAttributedStringRenderer.render("# Title")
        let color = result.attribute(.foregroundColor, at: 0, effectiveRange: nil) as? NSColor
        #expect(color == .labelColor)
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
        #expect(result.string.contains("•\tone"))
        let range = (result.string as NSString).range(of: "one")
        let style = result.attribute(.paragraphStyle, at: range.location, effectiveRange: nil) as? NSParagraphStyle
        #expect(style?.headIndent == 16)
    }
    @Test("unordered list items get a visible bullet")
    func unorderedBullet() {
        let result = MarkdownAttributedStringRenderer.render("- one\n- two")
        #expect(result.string.contains("•\tone"))
        #expect(result.string.contains("•\ttwo"))
    }

    @Test("ordered list items get their ordinal")
    func orderedMarker() {
        let result = MarkdownAttributedStringRenderer.render("1. first\n2. second")
        #expect(result.string.contains("1.\tfirst"))
        #expect(result.string.contains("2.\tsecond"))
    }

    @Test("nested list items indent one level deeper")
    func nestedListIndent() {
        let result = MarkdownAttributedStringRenderer.render("- outer\n    - inner")
        let range = (result.string as NSString).range(of: "inner")
        let style = result.attribute(.paragraphStyle, at: range.location, effectiveRange: nil) as? NSParagraphStyle
        #expect(style?.headIndent == 32)
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
        #expect(result.string == "•\tone\n•\ttwo")
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

    @Test("inline code is marked as a chip with a background fill")
    func inlineCodeChip() {
        let result = MarkdownAttributedStringRenderer.render("`code`")
        #expect(result.attribute(CodeBlockStyle.inlineCodeAttribute, at: 0, effectiveRange: nil) != nil)
        let bg = result.attribute(.backgroundColor, at: 0, effectiveRange: nil) as? NSColor
        #expect(bg == CodeBlockStyle.chipFill)
    }

    @Test("inline code uses near-label foreground, not teal")
    func inlineCodeIsColored() {
        let result = MarkdownAttributedStringRenderer.render("`code`")
        let color = result.attribute(.foregroundColor, at: 0, effectiveRange: nil) as? NSColor
        #expect(color == NSColor.labelColor.withAlphaComponent(0.9))
    }

    @Test("code block text uses near-label monochrome by default")
    func codeBlockIsColored() {
        let result = MarkdownAttributedStringRenderer.render("```\nlet x = 1\n```")
        let range = (result.string as NSString).range(of: "let x = 1")
        let color = result.attribute(.foregroundColor, at: range.location, effectiveRange: nil) as? NSColor
        #expect(color == NSColor.labelColor.withAlphaComponent(0.85))
    }

    @Test("code block carries language metadata from the fence")
    func codeBlockLanguage() {
        let result = MarkdownAttributedStringRenderer.render("```swift\nlet a = 1\n```")
        let range = (result.string as NSString).range(of: "let a = 1")
        let info = result.attribute(CodeBlockStyle.codeBlockAttribute, at: range.location, effectiveRange: nil) as? CodeBlockInfo
        #expect(info?.language == "swift")
        #expect(info?.index == 0)
    }

    @Test("code block without a language falls back to text")
    func codeBlockLanguageFallback() {
        let result = MarkdownAttributedStringRenderer.render("```\nplain\n```")
        let range = (result.string as NSString).range(of: "plain")
        let info = result.attribute(CodeBlockStyle.codeBlockAttribute, at: range.location, effectiveRange: nil) as? CodeBlockInfo
        #expect(info?.language == "text")
    }

    @Test("separator newline before a code block does not join the card")
    func separatorNewlineExcluded() {
        let result = MarkdownAttributedStringRenderer.render("para\n\n```\ncode\n```")
        let codeLocation = (result.string as NSString).range(of: "code").location
        #expect(result.attribute(CodeBlockStyle.codeBlockAttribute, at: codeLocation - 1, effectiveRange: nil) == nil)
    }

    @Test("code block first line reserves header space, interior lines do not")
    func codeBlockHeaderSpace() {
        let result = MarkdownAttributedStringRenderer.render("```swift\nlet a = 1\nlet b = 2\n```")
        let first = (result.string as NSString).range(of: "let a = 1").location
        let second = (result.string as NSString).range(of: "let b = 2").location
        let firstStyle = result.attribute(.paragraphStyle, at: first, effectiveRange: nil) as? NSParagraphStyle
        let secondStyle = result.attribute(.paragraphStyle, at: second, effectiveRange: nil) as? NSParagraphStyle
        #expect(firstStyle?.paragraphSpacingBefore == CodeBlockStyle.headerHeight + 10)
        #expect(secondStyle?.paragraphSpacingBefore == 0)
        #expect(firstStyle?.headIndent == CodeBlockStyle.cardPadding)
        #expect(firstStyle?.firstLineHeadIndent == CodeBlockStyle.cardPadding)
        #expect(secondStyle?.headIndent == CodeBlockStyle.cardPadding)
        #expect(secondStyle?.firstLineHeadIndent == CodeBlockStyle.cardPadding)
        #expect(secondStyle?.paragraphSpacing == CodeBlockStyle.cardPadding + 8)
    }

}
