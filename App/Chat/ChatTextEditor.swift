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
    let document: ComposerDocument
    var isEditable: Bool
    var minHeight: CGFloat
    var maxHeight: CGFloat
    var onSubmit: () -> Void
    /// Returns true to consume the key (popup navigation); false restores
    /// the default behavior. nil behaves like always-false.
    var onSlashKey: ((SlashKey) -> Bool)? = nil
    /// Read here, in the parent's body, so Observation sees the dependency:
    /// `updateNSView` alone is never re-run by a change AppKit cannot observe.
    var fontSize: CGFloat = AppFont.base

    func makeCoordinator() -> Coordinator { Coordinator(self) }

    /// Builds the configured text view. Separate from `makeNSView` so tests
    /// can get a real text view without an `NSViewRepresentableContext`,
    /// which cannot be constructed outside SwiftUI. Mirrors the seam
    /// `AgentMarkdownTextView.makeTextView()` already uses.
    ///
    /// Created with the plain `NSTextView()` initializer, which is TextKit 2
    /// on macOS 15. `init(frame:textContainer:)` would opt into TextKit 1.
    ///
    /// When a document is supplied, its storage is adopted as the view's own,
    /// so the document and the view are literally the same text — no mirror,
    /// no sync, and nothing that can wipe an attachment.
    @MainActor
    static func makeTextView(document: ComposerDocument? = nil) -> NSTextView {
        let textView = NSTextView()
        textView.appearance = NSAppearance(named: AppTheme.ComposerAppearance.appKitAppearance)
        textView.textColor = AppTheme.ComposerAppearance.primaryTextColor
        textView.insertionPointColor = AppTheme.ComposerAppearance.primaryTextColor
        if let document {
            textView.textContentStorage?.textStorage = document.storage
        }
        textView.font = AppFont.nsBody
        textView.isRichText = false
        // Drops are handled by ChatPaneView, which routes them through
        // FileDrop. An editable NSTextView otherwise claims the file types
        // first — it is lower in the responder chain than the SwiftUI drop
        // area — and inserts the file on its own terms.
        textView.unregisterDraggedTypes()
        textView.drawsBackground = false
        textView.textContainerInset = NSSize(width: 0, height: 4)
        textView.textContainer?.lineFragmentPadding = 0
        textView.textContainer?.widthTracksTextView = true
        textView.isVerticallyResizable = true
        textView.isHorizontallyResizable = false
        textView.autoresizingMask = [.width]
        return textView
    }

    func makeNSView(context: Context) -> AutoSizingScrollView {
        let textView = Self.makeTextView(document: document)
        textView.delegate = context.coordinator
        textView.isEditable = isEditable
        textView.typingAttributes = document.typingAttributes

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

    /// No text mirroring here on purpose: the document's storage *is* the text
    /// view's storage, so there is nothing to copy across, and the previous
    /// `textView.string = text` assignment would have erased every chip.
    func updateNSView(_ scrollView: AutoSizingScrollView, context: Context) {
        context.coordinator.parent = self
        guard let textView = scrollView.documentView as? NSTextView else { return }
        textView.isEditable = isEditable
        if textView.font?.pointSize != fontSize {
            document.applyBaseFont()
            textView.font = AppFont.nsBody
        }
        textView.typingAttributes = document.typingAttributes
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
            parent.document.refreshQueries()
            parent.document.typingAttributes = textView.typingAttributes
            if let scrollView = textView.enclosingScrollView as? AutoSizingScrollView {
                recalculateHeight(textView: textView, scrollView: scrollView)
            }
        }

        /// For an editable text view these two are exactly the moments it gains
        /// and loses first-responder status, so the composer's focus ring can
        /// be driven from the delegate that already exists — no
        /// `window.firstResponder` observer to install or tear down.
        func textDidBeginEditing(_ notification: Notification) {
            parent.document.isFocused = true
        }

        func textDidEndEditing(_ notification: Notification) {
            parent.document.isFocused = false
        }

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

        /// Measured through TextKit 2. Reading `textView.layoutManager` here
        /// would silently downgrade the view to TextKit 1 compatibility mode,
        /// in which `NSTextAttachmentViewProvider` is never invoked and the
        /// composer's chips stop rendering with no error of any kind.
        /// `usageBoundsForTextContainer` is the TextKit 2 analogue of
        /// `NSLayoutManager.usedRect(for:)`.
        func recalculateHeight(textView: NSTextView, scrollView: AutoSizingScrollView) {
            guard let layoutManager = textView.textLayoutManager else { return }
            layoutManager.ensureLayout(for: layoutManager.documentRange)
            let used = layoutManager.usageBoundsForTextContainer.height
                + textView.textContainerInset.height * 2
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
