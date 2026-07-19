import AppKit
import Foundation

/// Converts a markdown string to an `NSAttributedString` styled to match
/// `TillerMarkdownTheme`, for rendering inside a single `NSTextView` so
/// drag-selection stays continuous across block boundaries. See
/// docs/superpowers/specs/2026-07-19-agent-markdown-selection-design.md.
enum MarkdownAttributedStringRenderer {

    static let bodySize: CGFloat = 13
    static let codeSize: CGFloat = 12

    static func render(_ markdown: String) -> NSAttributedString {
        let options = AttributedString.MarkdownParsingOptions(
            allowsExtendedAttributes: true,
            interpretedSyntax: .full,
            failurePolicy: .throwError)
        guard let parsed = try? AttributedString(markdown: markdown, options: options) else {
            return NSAttributedString(
                string: markdown,
                attributes: [.font: NSFont.systemFont(ofSize: bodySize)])
        }
        return render(parsed)
    }

    private static func render(_ parsed: AttributedString) -> NSAttributedString {
        let result = NSMutableAttributedString()
        for run in parsed.runs {
            let substring = String(parsed[run.range].characters)
            result.append(NSAttributedString(string: substring, attributes: attributes(for: run)))
        }
        return result
    }

    private static func attributes(for run: AttributedString.Runs.Run) -> [NSAttributedString.Key: Any] {
        var font = NSFont.systemFont(ofSize: bodySize)
        var color: NSColor = .labelColor
        let paragraphStyle = NSMutableParagraphStyle()
        var attrs: [NSAttributedString.Key: Any] = [:]

        if let inline = run.inlinePresentationIntent {
            if inline.contains(.code) {
                font = NSFont.monospacedSystemFont(ofSize: codeSize, weight: .regular)
            }
            if inline.contains(.stronglyEmphasized) {
                font = NSFontManager.shared.convert(font, toHaveTrait: .boldFontMask)
            }
            if inline.contains(.emphasized) {
                font = NSFontManager.shared.convert(font, toHaveTrait: .italicFontMask)
            }
            if inline.contains(.strikethrough) {
                attrs[.strikethroughStyle] = NSUnderlineStyle.single.rawValue
            }
        }

        if let link = run.link {
            attrs[.link] = link as NSURL
            color = .linkColor
        }

        attrs[.font] = font
        attrs[.foregroundColor] = color
        attrs[.paragraphStyle] = paragraphStyle
        return attrs
    }
}
