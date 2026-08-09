import AppKit
import SwiftUI
import TillerCore
import TillerTerminal

/// NSTextView subclass owning the code-block header overlays. Headers are
/// rebuilt whenever the attributed text changes and repositioned on every
/// layout pass from the same attribute ranges the layout manager draws from.
final class MarkdownTextView: NSTextView {
    private var headerViews: [NSHostingView<CodeBlockHeaderView>] = []
    var onAppearanceChanged: (() -> Void)?

    func rebuildCodeBlockHeaders() {
        headerViews.forEach { $0.removeFromSuperview() }
        headerViews.removeAll()
        guard let storage = textStorage else { return }
        storage.enumerateAttribute(CodeBlockStyle.codeBlockAttribute,
                                   in: NSRange(location: 0, length: storage.length)) { value, range, _ in
            guard let info = value as? CodeBlockInfo else { return }
            let code = (storage.string as NSString).substring(with: range)
                .trimmingCharacters(in: .newlines)
            let host = NSHostingView(rootView: CodeBlockHeaderView(language: info.language, code: code))
            addSubview(host)
            headerViews.append(host)
        }
        needsLayout = true
    }

    override func layout() {
        super.layout()
        layoutCodeBlockHeaders()
    }

    override func viewDidChangeEffectiveAppearance() {
        super.viewDidChangeEffectiveAppearance()
        onAppearanceChanged?()
    }

    private func layoutCodeBlockHeaders() {
        guard let layoutManager, let textContainer, let storage = textStorage else { return }
        var index = 0
        storage.enumerateAttribute(CodeBlockStyle.codeBlockAttribute,
                                   in: NSRange(location: 0, length: storage.length)) { value, range, _ in
            guard value is CodeBlockInfo, index < headerViews.count else { return }
            let glyphRange = layoutManager.glyphRange(forCharacterRange: range, actualCharacterRange: nil)
            let rect = layoutManager.boundingRect(forGlyphRange: glyphRange, in: textContainer)
            headerViews[index].frame = NSRect(
                x: 0,
                y: rect.minY - CodeBlockStyle.headerHeight - 4,
                width: textContainer.size.width,
                height: CodeBlockStyle.headerHeight)
            index += 1
        }
    }
}


/// Renders an agent chat message's markdown inside a single `NSTextView` so
/// drag-selection stays continuous across block boundaries (paragraph, code
/// block, list, heading) — see
/// docs/superpowers/specs/2026-07-19-agent-markdown-selection-design.md.
/// Unlike `ChatTextEditor`, this view is read-only and uncapped in height:
/// it reports its full content height and relies on `TranscriptView`'s
/// enclosing `ScrollView`, matching how `Markdown(text)` behaved before it.
struct AgentMarkdownTextView: NSViewRepresentable {
    var markdown: String
    var onOpenURL: ((URL) -> Void)? = nil
    /// Same role as `onAppearanceChanged` below, for the other thing AppKit
    /// cannot observe: read in the parent's body so a size change reaches
    /// `updateNSView` at all.
    var fontSize: CGFloat = AppFont.base

    func makeCoordinator() -> Coordinator { Coordinator(parent: self) }

    @MainActor
    static func makeTextView() -> MarkdownTextView {
        let storage = NSTextStorage()
        let layoutManager = CodeBlockLayoutManager()
        storage.addLayoutManager(layoutManager)
        let container = NSTextContainer(size: CGSize(width: 0, height: CGFloat.greatestFiniteMagnitude))
        container.widthTracksTextView = false
        container.lineFragmentPadding = 0
        layoutManager.addTextContainer(container)
        let textView = MarkdownTextView(frame: .zero, textContainer: container)
        textView.isEditable = false
        textView.isSelectable = true
        textView.drawsBackground = false
        textView.textContainerInset = NSSize.zero
        textView.isVerticallyResizable = true
        textView.isHorizontallyResizable = false
        textView.autoresizingMask = [.width]
        return textView
    }

    func makeNSView(context: Context) -> NSTextView {
        let textView = Self.makeTextView()
        let coordinator = context.coordinator
        textView.delegate = coordinator
        textView.onAppearanceChanged = { [weak textView] in
            guard let textView, let source = coordinator.lastRenderedSource else { return }
            textView.textStorage?.setAttributedString(MarkdownAttributedStringRenderer.render(source))
            (textView as? MarkdownTextView)?.rebuildCodeBlockHeaders()
            coordinator.measuredHeight = nil
        }
        textView.textStorage?.setAttributedString(MarkdownAttributedStringRenderer.render(markdown))
        textView.rebuildCodeBlockHeaders()
        context.coordinator.lastRenderedSource = markdown
        context.coordinator.lastRenderedFontSize = fontSize
        return textView
    }

    func updateNSView(_ textView: NSTextView, context: Context) {
        context.coordinator.parent = self
        guard context.coordinator.lastRenderedSource != markdown
                || context.coordinator.lastRenderedFontSize != fontSize else { return }
        textView.textStorage?.setAttributedString(MarkdownAttributedStringRenderer.render(markdown))
        (textView as? MarkdownTextView)?.rebuildCodeBlockHeaders()
        context.coordinator.lastRenderedSource = markdown
        context.coordinator.lastRenderedFontSize = fontSize
        // Invalidate cached height so the next `sizeThatFits` re-runs layout.
        context.coordinator.measuredHeight = nil
    }

    /// SwiftUI consults this (not Auto Layout / `intrinsicContentSize`) to size
    /// an `NSViewRepresentable`. No height cap here — the outer `ScrollView` in
    /// `TranscriptView` owns scrolling, this view just reports how tall its
    /// text actually is.
    ///
    /// `LazyVStack` inside a `ScrollView` measures every item to compute the
    /// total scrollable content height, so `sizeThatFits` is called many times
    /// per layout pass (once per item, then again on every invalidation).
    /// Without the cache below, each call ran `ensureLayout` — AppKit text
    /// layout — which turned any non-trivial transcript into a multi-second
    /// main-thread hang. The cache skips that work when neither the markdown
    /// nor the proposed width changed. `widthTracksTextView` is disabled so
    /// the container size we set here is authoritative, preventing a feedback
    /// loop where the layout manager re-lays out on frame changes.
    func sizeThatFits(_ proposal: ProposedViewSize, nsView: NSTextView,
                      context: Context) -> CGSize? {
        guard let layoutManager = nsView.layoutManager, let container = nsView.textContainer else {
            return nil
        }
        let width = proposal.width ?? nsView.frame.width
        if context.coordinator.lastRenderedSource == markdown,
           context.coordinator.measuredWidth == width,
           let cached = context.coordinator.measuredHeight {
            return CGSize(width: width, height: cached)
        }
        let sid = SignpostMetrics.makeSignpostID()
        let state = SignpostMetrics.beginInterval("streamLayout", id: sid)
        defer {
            SignpostMetrics.endInterval(
                "streamLayout", state,
                message: "width: \(SignpostMetrics.lengthLabel(width))")
        }
        container.size = CGSize(width: max(0, width), height: .greatestFiniteMagnitude)
        layoutManager.ensureLayout(for: container)
        let height = layoutManager.usedRect(for: container).height
        context.coordinator.measuredWidth = width
        context.coordinator.measuredHeight = height
        return CGSize(width: width, height: height)
    }

    final class Coordinator: NSObject, NSTextViewDelegate {
        var parent: AgentMarkdownTextView
        var lastRenderedSource: String?
        var lastRenderedFontSize: CGFloat?
        var measuredWidth: CGFloat?
        var measuredHeight: CGFloat?

        init(parent: AgentMarkdownTextView) {
            self.parent = parent
        }

        func textView(_ textView: NSTextView, clickedOnLink link: Any, at _: Int) -> Bool {
            let resolvedURL: URL?
            if let url = link as? URL {
                resolvedURL = url
            } else if let nsURL = link as? NSURL {
                resolvedURL = nsURL as URL
            } else {
                resolvedURL = nil
            }
            guard let resolvedURL else { return false }
            parent.onOpenURL?(resolvedURL)
            return true
        }
    }
}
