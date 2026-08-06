import AppKit
import Testing

@testable import Tiller

@Suite("ChatTextEditor drops")
@MainActor
struct ChatTextEditorDropTests {
    /// An editable NSTextView registers for file drags by default and would
    /// answer before the chat pane's own drop handler, inserting the file
    /// itself and bypassing the chip pipeline.
    @Test func textViewClaimsNoDraggedTypes() {
        let textView = ChatTextEditor.makeTextView()
        #expect(textView.registeredDraggedTypes.isEmpty)
    }
}
