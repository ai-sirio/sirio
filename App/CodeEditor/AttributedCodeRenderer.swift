import AppKit
import CodeEditSourceEditor
import TillerCode

@MainActor
enum AttributedCodeRenderer {
    static func renderLine(
        _ text: String,
        ranges: [SyntaxHighlightRange],
        theme: EditorTheme,
        font: NSFont
    ) -> AttributedString {
        let output = NSMutableAttributedString(
            string: text,
            attributes: [.font: font, .foregroundColor: theme.text.color])
        let bounds = NSRange(location: 0, length: output.length)
        for run in ranges {
            let clipped = NSIntersectionRange(bounds, run.range)
            guard clipped.length > 0 else { continue }
            let style = theme.attribute(for: run.role)
            var traits: NSFontTraitMask = []
            if style.bold { traits.insert(.boldFontMask) }
            if style.italic { traits.insert(.italicFontMask) }
            let styledFont = traits.isEmpty
                ? font
                : NSFontManager.shared.convert(font, toHaveTrait: traits)
            output.addAttributes(
                [.foregroundColor: style.color, .font: styledFont],
                range: clipped)
        }
        return AttributedString(output)
    }
}
