import AppKit
import Foundation

/// Converts a markdown string to an `NSAttributedString` styled to match
/// `TillerMarkdownTheme`, for rendering inside a single `NSTextView` so
/// drag-selection stays continuous across block boundaries. See
/// docs/superpowers/specs/2026-07-19-agent-markdown-selection-design.md.
///
/// `★ Insight ───` callouts are stripped out before text ever reaches this
/// renderer — see `AgentMessageSegmenter` / `InsightCardView` — so this file
/// only ever sees plain prose markdown.
enum MarkdownAttributedStringRenderer {

    static let bodySize: CGFloat = 13
    static let codeSize: CGFloat = 12
    /// h1/h2/h3 match `TillerMarkdownTheme`; h4-h6 fall back to h3 sizing —
    /// the theme itself only styles up to h3, so there's no richer source of
    /// truth to copy for deeper levels.
    private static let headingSizes: [Int: CGFloat] = [1: 15, 2: 14, 3: 13]
    private static let headingMargins: [Int: (top: CGFloat, bottom: CGFloat)] =
        [1: (12, 4), 2: (10, 4), 3: (8, 2)]
    static let codeColor = NSColor.systemTeal.withAlphaComponent(0.75)

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

    /// `AttributedString(markdown:)` does not insert a literal newline
    /// between blocks — block structure is expressed only through each
    /// run's `presentationIntent`, and the caller is expected to detect
    /// block-boundary changes itself. Runs sharing the same *immediate*
    /// (innermost) `presentationIntent` component identity belong to the
    /// same block — e.g. every inline span inside one paragraph shares that
    /// paragraph's identity — so a boundary is only a block-level change,
    /// never a mid-paragraph inline-style change.
    private static func render(_ parsed: AttributedString) -> NSAttributedString {
        let result = NSMutableAttributedString()
        var previousBlockIdentity: Int?
        for run in parsed.runs {
            var substring = String(parsed[run.range].characters)
            let blockIdentity = run.presentationIntent?.components.first?.identity
            let isNewBlock = blockIdentity != previousBlockIdentity
            if previousBlockIdentity != nil, isNewBlock {
                substring = "\n" + substring
            }
            if isNewBlock, let marker = listMarker(for: run.presentationIntent) {
                substring = previousBlockIdentity == nil ? marker + substring
                    : substring.replacingOccurrences(of: "\n", with: "\n" + marker,
                                                     range: substring.range(of: "\n"))
            }
            if let blockIdentity { previousBlockIdentity = blockIdentity }
            result.append(NSAttributedString(string: substring, attributes: attributes(for: run)))
        }
        return result
    }

    /// A list item's marker ("•\t" or "3.\t"), or nil for non-list blocks.
    /// Markdown markers are stripped by `AttributedString(markdown:)`; only
    /// the presentation intent knows an item's ordinal and whether its list is
    /// ordered.
    private static func listMarker(for intent: PresentationIntent?) -> String? {
        guard let components = intent?.components else { return nil }
        var ordinal: Int?
        var isOrdered = false
        for component in components {
            if case .listItem(let n) = component.kind { ordinal = ordinal ?? n }
            if case .orderedList = component.kind { isOrdered = true }
        }
        guard let ordinal else { return nil }
        return isOrdered ? "\(ordinal).\t" : "•\t"
    }

    private static func attributes(for run: AttributedString.Runs.Run) -> [NSAttributedString.Key: Any] {
        var font = NSFont.systemFont(ofSize: bodySize)
        // Body prose is muted relative to headings, so headings keep
        // reading as the visual anchor of a reply.
        var color: NSColor = .labelColor.withAlphaComponent(0.82)
        let paragraphStyle = NSMutableParagraphStyle()
        paragraphStyle.lineSpacing = 4.5
        var attrs: [NSAttributedString.Key: Any] = [:]
        if let intent = run.presentationIntent {
            for component in intent.components {
                switch component.kind {
                case .header(let level):
                    let size = headingSizes[level] ?? headingSizes[3]!
                    font = NSFontManager.shared.convert(
                        NSFont.systemFont(ofSize: size), toHaveTrait: .boldFontMask)
                    color = .labelColor
                    let margin = headingMargins[level] ?? headingMargins[3]!
                    paragraphStyle.paragraphSpacingBefore = margin.top
                    paragraphStyle.paragraphSpacing = margin.bottom
                case .paragraph:
                    paragraphStyle.paragraphSpacing = 9
                case .codeBlock:
                    font = NSFont.monospacedSystemFont(ofSize: codeSize, weight: .regular)
                    color = codeColor
                    paragraphStyle.paragraphSpacing = 6
                case .blockQuote:
                    color = .secondaryLabelColor
                    paragraphStyle.headIndent = 12
                    paragraphStyle.firstLineHeadIndent = 12
                    paragraphStyle.paragraphSpacing = 6
                case .listItem:
                    // A list item's run also carries a `.paragraph` component
                    // (processed earlier in this loop), which would otherwise
                    // leave every item spaced out like a standalone paragraph
                    // — reset it so list items stay tight against each other.
                    paragraphStyle.paragraphSpacing = 0
                    let depth = intent.components.filter {
                        if case .listItem = $0.kind { return true } else { return false }
                    }.count
                    let indent = CGFloat(depth) * 16
                    paragraphStyle.firstLineHeadIndent = indent - 16
                    paragraphStyle.headIndent = indent
                    paragraphStyle.tabStops = [NSTextTab(textAlignment: .left, location: indent)]
                    break
                default:
                    break
                }
            }
        }

        if let inline = run.inlinePresentationIntent {
            if inline.contains(.code) {
                font = NSFont.monospacedSystemFont(ofSize: codeSize, weight: .regular)
                color = .labelColor.withAlphaComponent(0.9)
                attrs[CodeBlockStyle.inlineCodeAttribute] = true
                attrs[.backgroundColor] = CodeBlockStyle.chipFill
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
