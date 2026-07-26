import AppKit
import Testing
@testable import Tiller

struct StreamingTextBufferTests {
    @Test("appends only the suffix when the current text is a prefix")
    func appendsPrefixSuffix() {
        var buffer = StreamingTextBuffer(current: "hello")

        #expect(buffer.update(to: "hello world") == .append(" world"))
    }

    @Test("resets when the new text diverges or shrinks")
    func resetsWhenTextIsNotAnExtension() {
        var buffer = StreamingTextBuffer(current: "hello world")

        #expect(buffer.update(to: "hello there") == .reset("hello there"))
        #expect(buffer.update(to: "hello") == .reset("hello"))
    }

    @Test("returns none for identical text")
    func identicalTextIsIgnored() {
        var buffer = StreamingTextBuffer(current: "same")

        #expect(buffer.update(to: "same") == .none)
    }

    @Test("appends the full message when the current text is empty")
    func emptyCurrentTextAppendsFullMessage() {
        var buffer = StreamingTextBuffer()

        #expect(buffer.update(to: "first message") == .append("first message"))
    }

    @Test("preserves UTF-8 suffixes that split a grapheme cluster")
    func multiByteAppendBoundary() {
        var buffer = StreamingTextBuffer(current: "👩")
        let newText = "👩‍💻 ships 🚀"

        guard case .append(let suffix) = buffer.update(to: newText) else {
            Issue.record("Expected an append edit")
            return
        }
        #expect(Array(suffix.utf8) == Array(newText.utf8.dropFirst("👩".utf8.count)))
        #expect(Array(buffer.current.utf8) == Array(newText.utf8))
    }

    @Test("replaying the chat fixture preserves the final message byte for byte")
    func replayFixture() {
        let fixture = ChatStreamFixture.make()
        var buffer = StreamingTextBuffer()
        var displayed = ""

        for fragment in fixture.fragments {
            let next = buffer.current + fragment
            switch buffer.update(to: next) {
            case .none:
                break
            case .append(let suffix):
                displayed += suffix
            case .reset(let text):
                displayed = text
            }
        }

        #expect(Array(displayed.utf8) == Array(fixture.finalMessage.utf8))
        #expect(fixture.fragments.count >= 1_000)
    }

    @MainActor
    @Test("streaming text view remains read-only selectable body text")
    func streamingTextViewPreservesSelectionAndAccessibilityBasics() {
        let textView = StreamingAgentTextView.makeTextView()

        #expect(textView.isEditable == false)
        #expect(textView.isSelectable == true)
        #expect(textView.font == MarkdownAttributedStringRenderer.bodyFont)
        #expect(textView.textColor == MarkdownAttributedStringRenderer.bodyColor)
    }

    @MainActor
    @Test("incomplete stream routing performs zero full markdown renders")
    func incompleteStreamSkipsMarkdownRendering() {
        let fixture = ChatStreamFixture.make()
        var buffer = StreamingTextBuffer()
        let storage = NSTextStorage()
        MarkdownAttributedStringRenderer.resetRenderCount()

        for fragment in fixture.fragments {
            let edit = buffer.update(to: buffer.current + fragment)
            _ = StreamingAgentTextView.apply(edit, to: storage)
            #expect(AgentMessagePresentation.mode(isComplete: false) == .streaming)
        }

        #expect(Array(storage.string.utf8) == Array(fixture.finalMessage.utf8))
        #expect(MarkdownAttributedStringRenderer.renderCount == 0)
    }

    @MainActor
    @Test("completion renders exactly once per prose segment")
    func completionRendersEachProseSegmentOnce() {
        let fixture = ChatStreamFixture.make()
        let prose = AgentMessageSegmenter.segments(from: fixture.finalMessage).compactMap { segment in
            if case .prose(let text) = segment { return text }
            return nil
        }
        MarkdownAttributedStringRenderer.resetRenderCount()

        #expect(AgentMessagePresentation.mode(isComplete: true) == .rich)
        for text in prose {
            _ = MarkdownAttributedStringRenderer.render(text)
        }

        #expect(MarkdownAttributedStringRenderer.renderCount == prose.count)
    }
}
