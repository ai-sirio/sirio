import AppKit

/// TextKit 1 layout manager that paints the T3-style decorations the
/// renderer only *marks*: rounded cards behind code-block ranges (including
/// the header zone reserved via `paragraphSpacingBefore`) and rounded chips
/// behind inline code. Reading the custom attributes at draw time keeps the
/// attributed string the single source of truth.
final class CodeBlockLayoutManager: NSLayoutManager {

    override func drawBackground(forGlyphRange glyphsToShow: NSRange, at origin: NSPoint) {
        drawCodeBlockCards(at: origin)
        super.drawBackground(forGlyphRange: glyphsToShow, at: origin)
    }

    /// Inline-code chips: `.backgroundColor` set by the renderer routes
    /// here; rounded corners instead of the square default.
    override func fillBackgroundRectArray(_ rectArray: UnsafePointer<NSRect>,
                                          count: Int,
                                          forCharacterRange charRange: NSRange,
                                          color: NSColor) {
        let isChip = textStorage?.attribute(CodeBlockStyle.inlineCodeAttribute,
                                            at: charRange.location,
                                            effectiveRange: nil) != nil
        guard isChip else {
            super.fillBackgroundRectArray(rectArray, count: count,
                                          forCharacterRange: charRange, color: color)
            return
        }
        color.setFill()
        for i in 0..<count {
            let rect = rectArray[i].insetBy(dx: -2, dy: -0.5)
            NSBezierPath(roundedRect: rect,
                         xRadius: CodeBlockStyle.chipCornerRadius,
                         yRadius: CodeBlockStyle.chipCornerRadius).fill()
        }
    }

    private func drawCodeBlockCards(at origin: NSPoint) {
        guard let storage = textStorage, let container = textContainers.first else { return }
        storage.enumerateAttribute(CodeBlockStyle.codeBlockAttribute,
                                   in: NSRange(location: 0, length: storage.length)) { value, range, _ in
            guard value is CodeBlockInfo else { return }
            let glyphRange = self.glyphRange(forCharacterRange: range, actualCharacterRange: nil)
            var rect = self.boundingRect(forGlyphRange: glyphRange, in: container)
            rect.origin.x = 0
            rect.size.width = container.size.width
            rect.origin.y -= CodeBlockStyle.headerHeight + 6
            rect.size.height += CodeBlockStyle.headerHeight + 6 + 8
            rect = rect.offsetBy(dx: origin.x, dy: origin.y)
            let path = NSBezierPath(roundedRect: rect,
                                    xRadius: CodeBlockStyle.cardCornerRadius,
                                    yRadius: CodeBlockStyle.cardCornerRadius)
            CodeBlockStyle.cardFill.setFill()
            path.fill()
            path.lineWidth = 1
            CodeBlockStyle.cardBorder.setStroke()
            path.stroke()
        }
    }
}
