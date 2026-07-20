import AppKit
import SwiftUI
import Testing

@testable import Tiller

@Suite("ChatTextEditorSlash")
@MainActor
struct ChatTextEditorSlashTests {
    private func makeEditor(
        onSlashKey: ((SlashKey) -> Bool)? = nil,
        onSubmit: @escaping () -> Void = {}
    ) -> ChatTextEditor {
        ChatTextEditor(
            text: .constant(""), isEditable: true, minHeight: 36, maxHeight: 160,
            onSubmit: onSubmit, onSlashKey: onSlashKey)
    }

    @Test func mapsSelectorsToSlashKeys() {
        typealias C = ChatTextEditor.Coordinator
        #expect(C.slashKey(for: #selector(NSResponder.moveUp(_:))) == .up)
        #expect(C.slashKey(for: #selector(NSResponder.moveDown(_:))) == .down)
        #expect(C.slashKey(for: #selector(NSResponder.insertTab(_:))) == .tab)
        #expect(C.slashKey(for: #selector(NSResponder.insertNewline(_:))) == .enter)
        #expect(C.slashKey(for: #selector(NSResponder.cancelOperation(_:))) == .escape)
        #expect(C.slashKey(for: #selector(NSResponder.moveLeft(_:))) == nil)
    }

    @Test func consumedKeyIsSwallowed() {
        var received: [SlashKey] = []
        let editor = makeEditor(onSlashKey: { key in
            received.append(key)
            return true
        })
        let coordinator = ChatTextEditor.Coordinator(editor)
        let textView = NSTextView()
        let handled = coordinator.textView(
            textView, doCommandBy: #selector(NSResponder.moveDown(_:)))
        #expect(handled)
        #expect(received == [.down])
    }

    @Test func unconsumedEnterStillSubmits() {
        var submitted = false
        let editor = makeEditor(
            onSlashKey: { _ in false }, onSubmit: { submitted = true })
        let coordinator = ChatTextEditor.Coordinator(editor)
        let handled = coordinator.textView(
            NSTextView(), doCommandBy: #selector(NSResponder.insertNewline(_:)))
        #expect(handled)
        #expect(submitted)
    }

    @Test func unconsumedArrowFallsThroughToDefault() {
        let editor = makeEditor(onSlashKey: { _ in false })
        let coordinator = ChatTextEditor.Coordinator(editor)
        let handled = coordinator.textView(
            NSTextView(), doCommandBy: #selector(NSResponder.moveUp(_:)))
        #expect(!handled)
    }

    @Test func nilHandlerKeepsCurrentBehavior() {
        var submitted = false
        let editor = makeEditor(onSlashKey: nil, onSubmit: { submitted = true })
        let coordinator = ChatTextEditor.Coordinator(editor)
        #expect(!coordinator.textView(
            NSTextView(), doCommandBy: #selector(NSResponder.moveUp(_:))))
        #expect(coordinator.textView(
            NSTextView(), doCommandBy: #selector(NSResponder.insertNewline(_:))))
        #expect(submitted)
    }
}
