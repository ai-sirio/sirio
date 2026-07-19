import AppKit
import SwiftUI

/// Renders an agent chat message's markdown inside a single `NSTextView` so
/// drag-selection stays continuous across block boundaries (paragraph, code
/// block, list, heading) — see
/// docs/superpowers/specs/2026-07-19-agent-markdown-selection-design.md.
/// Unlike `ChatTextEditor`, this view is read-only and uncapped in height:
/// it reports its full content height and relies on `TranscriptView`'s
/// enclosing `ScrollView`, matching how `Markdown(text)` behaved before it.
struct AgentMarkdownTextView: NSViewRepresentable {
    var markdown: String

    func makeCoordinator() -> Coordinator { Coordinator() }

    func makeNSView(context: Context) -> NSTextView {
        let textView = NSTextView()
        textView.isEditable = false
        textView.isSelectable = true
        textView.drawsBackground = false
        textView.textContainerInset = .zero
        textView.textContainer?.lineFragmentPadding = 0
        textView.textContainer?.widthTracksTextView = true
        textView.isVerticallyResizable = true
        textView.isHorizontallyResizable = false
        textView.autoresizingMask = [.width]
        textView.textStorage?.setAttributedString(MarkdownAttributedStringRenderer.render(markdown))
        context.coordinator.lastRenderedSource = markdown
        return textView
    }

    func updateNSView(_ textView: NSTextView, context: Context) {
        guard context.coordinator.lastRenderedSource != markdown else { return }
        textView.textStorage?.setAttributedString(MarkdownAttributedStringRenderer.render(markdown))
        context.coordinator.lastRenderedSource = markdown
    }

    /// Same technique as `ChatTextEditor.sizeThatFits`: SwiftUI consults this
    /// (not Auto Layout / `intrinsicContentSize`) to size an
    /// `NSViewRepresentable`. No height cap here — the outer `ScrollView` in
    /// `TranscriptView` owns scrolling, this view just reports how tall its
    /// text actually is.
    func sizeThatFits(_ proposal: ProposedViewSize, nsView: NSTextView,
                      context: Context) -> CGSize? {
        guard let layoutManager = nsView.layoutManager, let container = nsView.textContainer else {
            return nil
        }
        layoutManager.ensureLayout(for: container)
        let height = layoutManager.usedRect(for: container).height
        return CGSize(width: proposal.width ?? nsView.frame.width, height: height)
    }

    final class Coordinator {
        var lastRenderedSource: String?
    }
}
