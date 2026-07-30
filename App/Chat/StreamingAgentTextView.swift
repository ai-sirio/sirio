import AppKit
import SwiftUI
import TillerCore
import TillerTerminal

struct StreamingAgentTextView: NSViewRepresentable {
    var text: String

    func makeCoordinator() -> Coordinator {
        Coordinator()
    }

    @MainActor
    static func makeTextView() -> NSTextView {
        let storage = NSTextStorage()
        let layoutManager = NSLayoutManager()
        storage.addLayoutManager(layoutManager)
        let container = NSTextContainer(
            size: CGSize(width: 0, height: CGFloat.greatestFiniteMagnitude))
        container.widthTracksTextView = false
        container.lineFragmentPadding = 0
        layoutManager.addTextContainer(container)

        let textView = NSTextView(frame: .zero, textContainer: container)
        textView.isEditable = false
        textView.isSelectable = true
        textView.drawsBackground = false
        textView.textContainerInset = NSSize.zero
        textView.isVerticallyResizable = true
        textView.isHorizontallyResizable = false
        textView.autoresizingMask = [.width]
        textView.font = MarkdownAttributedStringRenderer.bodyFont
        textView.textColor = MarkdownAttributedStringRenderer.bodyColor
        return textView
    }

    func makeNSView(context: Context) -> NSTextView {
        let textView = Self.makeTextView()
        textView.textStorage?.setAttributedString(
            NSAttributedString(string: text, attributes: MarkdownAttributedStringRenderer.bodyAttributes))
        context.coordinator.buffer = StreamingTextBuffer(current: text)
        return textView
    }

    @MainActor
    static func apply(_ edit: StreamingTextEdit, to storage: NSTextStorage) -> Int {
        switch edit {
        case .none:
            return 0
        case .append(let suffix):
            storage.append(NSAttributedString(
                string: suffix,
                attributes: MarkdownAttributedStringRenderer.bodyAttributes))
            return suffix.utf8.count
        case .reset(let fullText):
            storage.setAttributedString(NSAttributedString(
                string: fullText,
                attributes: MarkdownAttributedStringRenderer.bodyAttributes))
            return fullText.utf8.count
        }
    }


    func updateNSView(_ textView: NSTextView, context: Context) {
        guard let storage = textView.textStorage else { return }
        let edit = context.coordinator.buffer.update(to: text)
        if case .none = edit { return }
        var changedBytes = 0
        let sid = SignpostMetrics.makeSignpostID()
        let state = SignpostMetrics.beginInterval("streamCommit", id: sid)
        defer {
            SignpostMetrics.endInterval("streamCommit", state, message: "\(changedBytes)")
        }

        changedBytes = Self.apply(edit, to: storage)
        context.coordinator.measuredHeight = nil
    }

    func sizeThatFits(_ proposal: ProposedViewSize, nsView: NSTextView,
                      context: Context) -> CGSize? {
        guard let layoutManager = nsView.layoutManager,
              let container = nsView.textContainer else {
            return nil
        }
        let width = proposal.width ?? nsView.frame.width
        if context.coordinator.buffer.current == text,
           context.coordinator.measuredWidth == width,
           let cached = context.coordinator.measuredHeight {
            return CGSize(width: width, height: cached)
        }

        let sid = SignpostMetrics.makeSignpostID()
        let state = SignpostMetrics.beginInterval("streamLayout", id: sid)
        defer {
            SignpostMetrics.endInterval(
                "streamLayout", state, message: "width: \(Int(width))")
        }
        container.size = CGSize(width: max(0, width), height: .greatestFiniteMagnitude)
        layoutManager.ensureLayout(for: container)
        let height = layoutManager.usedRect(for: container).height
        context.coordinator.measuredWidth = width
        context.coordinator.measuredHeight = height
        return CGSize(width: width, height: height)
    }

    final class Coordinator {
        var buffer = StreamingTextBuffer()
        var measuredWidth: CGFloat?
        var measuredHeight: CGFloat?
    }
}
