import AppKit
import SwiftUI
import Testing

@testable import Tiller

@Suite("ComposerTextKit2")
@MainActor
struct ComposerTextKit2Tests {
    private func makeCoordinator() -> ChatTextEditor.Coordinator {
        ChatTextEditor.Coordinator(
            ChatTextEditor(document: ComposerDocument(), isEditable: true,
                           minHeight: 36, maxHeight: 160, onSubmit: {}, onSlashKey: nil))
    }

    /// A TextKit 1 fallback is silent: no error, no warning, no crash — the
    /// only observable symptom is `textLayoutManager` going nil, and later,
    /// chips never rendering. This test is the guard for that.
    @Test func measuringHeightKeepsTextKit2() {
        let textView = ChatTextEditor.makeTextView()
        let scrollView = AutoSizingScrollView()
        scrollView.documentView = textView
        textView.string = "hello\nworld"

        makeCoordinator().recalculateHeight(textView: textView, scrollView: scrollView)

        #expect(textView.textLayoutManager != nil)
    }

    @Test func heightGrowsWithMoreLines() {
        let coordinator = makeCoordinator()
        let textView = ChatTextEditor.makeTextView()
        let scrollView = AutoSizingScrollView()
        scrollView.documentView = textView
        textView.frame = NSRect(x: 0, y: 0, width: 300, height: 36)

        textView.string = "one line"
        coordinator.recalculateHeight(textView: textView, scrollView: scrollView)
        let single = scrollView.computedHeight

        textView.string = "one line\ntwo lines\nthree lines\nfour lines"
        coordinator.recalculateHeight(textView: textView, scrollView: scrollView)

        #expect(scrollView.computedHeight > single)
    }

    @Test func intrinsicHeightClampsToMaxHeight() {
        let scrollView = AutoSizingScrollView()
        scrollView.minHeight = 36
        scrollView.maxHeight = 160
        scrollView.computedHeight = 9_000

        #expect(scrollView.intrinsicContentSize.height == 160)
    }

    @Test func editingBeginAndEndDriveDocumentFocus() {
        let document = ComposerDocument()
        let coordinator = ChatTextEditor.Coordinator(
            ChatTextEditor(document: document, isEditable: true, minHeight: 36,
                           maxHeight: 160, onSubmit: {}, onSlashKey: nil))
        let textView = ChatTextEditor.makeTextView()

        coordinator.textDidBeginEditing(
            Notification(name: NSText.didBeginEditingNotification, object: textView))
        #expect(document.isFocused)

        coordinator.textDidEndEditing(
            Notification(name: NSText.didEndEditingNotification, object: textView))
        #expect(!document.isFocused)
    }

    @Test func textViewSharesTheDocumentStorage() {
        let document = ComposerDocument()
        document.storage.append(NSAttributedString(string: "seeded"))
        let editor = ChatTextEditor(document: document, isEditable: true, minHeight: 36,
                                    maxHeight: 160, onSubmit: {}, onSlashKey: nil)

        let textView = ChatTextEditor.makeTextView(document: document)
        _ = editor

        #expect(textView.string == "seeded")
        #expect(textView.textLayoutManager != nil)
    }
}
