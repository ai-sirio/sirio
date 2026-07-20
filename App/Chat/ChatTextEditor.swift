import SwiftUI
import AppKit

/// Multiline chat input backed by a real `NSTextView` inside an `NSScrollView`.
///
/// Replaces SwiftUI's `TextField(text:axis:.vertical)`, which cannot scroll
/// once content exceeds its `lineLimit` (content silently clips with no way
/// to reach the hidden text) and has flaky I-beam cursor / selection behavior
/// on macOS. A real `NSTextView` gets correct cursor rects, native
/// click-drag/double-click selection and copy, and a scroller for free once
/// the growing height hits `maxHeight`.
/// Keys the composer may steal from the text view while the slash-command
/// popup is open.
enum SlashKey: Equatable {
    case up, down, tab, enter, escape
}

struct ChatTextEditor: NSViewRepresentable {
    @Binding var text: String
    var isEditable: Bool
    var minHeight: CGFloat
    var maxHeight: CGFloat
    var onSubmit: () -> Void
    /// Returns true to consume the key (popup navigation); false restores
    /// the default behavior. nil behaves like always-false.
    var onSlashKey: ((SlashKey) -> Bool)? = nil

    func makeCoordinator() -> Coordinator { Coordinator(self) }

    func makeNSView(context: Context) -> AutoSizingScrollView {
        let textView = NSTextView()
        textView.delegate = context.coordinator
        textView.string = text
        textView.font = .systemFont(ofSize: NSFont.systemFontSize)
        textView.isRichText = false
        textView.isEditable = isEditable
        textView.drawsBackground = false
        textView.textContainerInset = NSSize(width: 0, height: 4)
        textView.textContainer?.lineFragmentPadding = 0
        textView.textContainer?.widthTracksTextView = true
        textView.isVerticallyResizable = true
        textView.isHorizontallyResizable = false
        textView.autoresizingMask = [.width]

        let scrollView = AutoSizingScrollView()
        scrollView.minHeight = minHeight
        scrollView.maxHeight = maxHeight
        scrollView.documentView = textView
        scrollView.hasVerticalScroller = true
        scrollView.autohidesScrollers = true
        scrollView.drawsBackground = false
        scrollView.borderType = .noBorder
        return scrollView
    }

    func updateNSView(_ scrollView: AutoSizingScrollView, context: Context) {
        context.coordinator.parent = self
        guard let textView = scrollView.documentView as? NSTextView else { return }
        if textView.string != text {
            textView.string = text
            context.coordinator.applySlashHighlight(to: textView)
        }
        textView.isEditable = isEditable
        scrollView.minHeight = minHeight
        scrollView.maxHeight = maxHeight
        context.coordinator.recalculateHeight(textView: textView, scrollView: scrollView)
    }

    /// `NSScrollView` keeps `translatesAutoresizingMaskIntoConstraints = true`,
    /// so Auto Layout (and with it `intrinsicContentSize`) never actually
    /// governs its size here; SwiftUI would otherwise hand it the full
    /// proposed space from the parent `ZStack`. This is what SwiftUI actually
    /// consults for sizing an `NSViewRepresentable`.
    func sizeThatFits(_ proposal: ProposedViewSize, nsView: AutoSizingScrollView,
                      context: Context) -> CGSize? {
        CGSize(width: proposal.width ?? nsView.frame.width,
               height: nsView.intrinsicContentSize.height)
    }

    final class Coordinator: NSObject, NSTextViewDelegate {
        var parent: ChatTextEditor

        init(_ parent: ChatTextEditor) {
            self.parent = parent
        }

        func textDidChange(_ notification: Notification) {
            guard let textView = notification.object as? NSTextView else { return }
            parent.text = textView.string
            applySlashHighlight(to: textView)
            if let scrollView = textView.enclosingScrollView as? AutoSizingScrollView {
                recalculateHeight(textView: textView, scrollView: scrollView)
            }
        }

        /// Only the leading "/token" prefix is tinted (a muted accent), and
        /// stays tinted even once the user keeps typing arguments after a
        /// space — only the draft-command condition (starts with "/") is
        /// checked, not "no whitespace anywhere". Text that doesn't start
        /// with "/" gets no highlight. `typingAttributes` stay pinned to the
        /// default color; the token range gets recolored on every edit.
        func applySlashHighlight(to textView: NSTextView) {
            guard let storage = textView.textStorage, storage.length > 0 else { return }
            let string = textView.string as NSString
            var tokenLength = 0
            if string.hasPrefix("/") {
                let whitespace = string.rangeOfCharacter(from: .whitespacesAndNewlines)
                tokenLength = whitespace.location == NSNotFound ? string.length : whitespace.location
            }
            if tokenLength > 0 {
                storage.addAttribute(
                    .foregroundColor, value: Self.mutedAccentColor,
                    range: NSRange(location: 0, length: tokenLength))
            }
            if tokenLength < storage.length {
                storage.addAttribute(
                    .foregroundColor, value: NSColor.textColor,
                    range: NSRange(location: tokenLength, length: storage.length - tokenLength))
            }
            textView.typingAttributes[.foregroundColor] = NSColor.textColor
        }

        static let mutedAccentColor = NSColor.controlAccentColor.withAlphaComponent(0.7)

        /// Slash-popup keys get first refusal via `onSlashKey`. Then plain
        /// Return sends (swallowed here); Shift+Return inserts a real
        /// newline via the default AppKit handling (`return false`).
        func textView(_ textView: NSTextView, doCommandBy commandSelector: Selector) -> Bool {
            if let key = Self.slashKey(for: commandSelector),
               parent.onSlashKey?(key) == true {
                return true
            }
            guard commandSelector == #selector(NSResponder.insertNewline(_:)) else { return false }
            let shiftHeld = NSApp.currentEvent?.modifierFlags.contains(.shift) ?? false
            if shiftHeld { return false }
            parent.onSubmit()
            return true
        }

        static func slashKey(for selector: Selector) -> SlashKey? {
            switch selector {
            case #selector(NSResponder.moveUp(_:)): .up
            case #selector(NSResponder.moveDown(_:)): .down
            case #selector(NSResponder.insertTab(_:)): .tab
            case #selector(NSResponder.insertNewline(_:)): .enter
            case #selector(NSResponder.cancelOperation(_:)): .escape
            default: nil
            }
        }

        func recalculateHeight(textView: NSTextView, scrollView: AutoSizingScrollView) {
            guard let layoutManager = textView.layoutManager, let container = textView.textContainer else { return }
            layoutManager.ensureLayout(for: container)
            let used = layoutManager.usedRect(for: container).height + textView.textContainerInset.height * 2
            if scrollView.computedHeight != used {
                scrollView.computedHeight = used
                scrollView.invalidateIntrinsicContentSize()
            }
        }
    }
}

/// Reports an intrinsic height clamped to `[minHeight, maxHeight]` so SwiftUI
/// grows the composer as the user types, then hands off to the scroller once
/// the content exceeds `maxHeight`.
final class AutoSizingScrollView: NSScrollView {
    var minHeight: CGFloat = 34
    var maxHeight: CGFloat = 160
    var computedHeight: CGFloat = 34

    override var intrinsicContentSize: NSSize {
        NSSize(width: NSView.noIntrinsicMetric, height: min(max(computedHeight, minHeight), maxHeight))
    }
}
