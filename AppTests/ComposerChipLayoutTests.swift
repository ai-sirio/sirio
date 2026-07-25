import AppKit
import SwiftUI
import Testing

@testable import Tiller

/// Guards the chip's reserved space in the text flow.
///
/// The bug this suite exists for: `NSTextAttachment` reserves a default
/// 32x32 cell when it reports no bounds of its own, so every chip — whatever
/// its label — got the same narrow slot while its hosting view drew itself
/// 60-150pt wide. The view then spilled out of the slot and was clipped at
/// the composer's edge. `tracksTextAttachmentViewBounds` alone did not
/// prevent this.
@Suite("ComposerChipLayout")
@MainActor
struct ComposerChipLayoutTests {
    private let container = NSTextContainer(size: NSSize(width: 400, height: 40))
    private let proposedLine = NSRect(x: 0, y: 0, width: 400, height: 40)

    private func bounds(for chip: ComposerChip) -> NSRect {
        ComposerChipAttachment(chip: chip).attachmentBounds(
            for: container, proposedLineFragment: proposedLine,
            glyphPosition: .zero, characterIndex: 0)
    }

    private func fittingWidth(for chip: ComposerChip) -> CGFloat {
        NSHostingView(rootView: ComposerChipView(chip: chip)).fittingSize.width
    }

    @Test func reservedWidthMatchesTheRenderedChip() {
        let chip = ComposerChip.skill(name: "ace-log-search")
        #expect(bounds(for: chip).width == fittingWidth(for: chip))
    }

    /// The regression in one assertion: a long label and a short one must not
    /// reserve the same width. They both got 32pt before the fix.
    @Test func longerLabelsReserveMoreWidth() {
        let short = bounds(for: .skill(name: "review")).width
        let long = bounds(for: .skill(name: "splunk-unipol-log-search")).width
        #expect(long > short)
    }

    /// A chip must never reserve more than the line it sits on, or the text
    /// system has no width left and the chip is pushed off the edge.
    @Test func reservedWidthNeverExceedsTheLineFragment() {
        let narrow = NSTextContainer(size: NSSize(width: 80, height: 40))
        let width = ComposerChipAttachment(chip: .skill(name: "splunk-unipol-log-search"))
            .attachmentBounds(for: narrow,
                              proposedLineFragment: NSRect(x: 0, y: 0, width: 80, height: 40),
                              glyphPosition: .zero, characterIndex: 0).width
        #expect(width <= 80)
    }

    @Test func reservedHeightMatchesTheRenderedChip() {
        let chip = ComposerChip.file(path: "App/Chat/ChatTextEditor.swift")
        let height = NSHostingView(rootView: ComposerChipView(chip: chip)).fittingSize.height
        #expect(bounds(for: chip).height == height)
    }

    /// The text layout must actually consume the reported bounds — asserting
    /// on `attachmentBounds` alone would pass even if the layout ignored it.
    @Test func laidOutFragmentIsAsWideAsTheChip() throws {
        let document = ComposerDocument()
        let textView = ChatTextEditor.makeTextView(document: document)
        textView.frame = NSRect(x: 0, y: 0, width: 400, height: 40)
        textView.textContainer?.size = NSSize(width: 400, height: CGFloat.greatestFiniteMagnitude)
        document.insert(.skill(name: "splunk-unipol-log-search"),
                        replacing: NSRange(location: 0, length: 0))

        let layoutManager = try #require(textView.textLayoutManager)
        layoutManager.ensureLayout(for: layoutManager.documentRange)

        var fragmentWidth: CGFloat = 0
        layoutManager.enumerateTextLayoutFragments(
            from: layoutManager.documentRange.location, options: [.ensuresLayout]
        ) { fragment in
            fragmentWidth = fragment.layoutFragmentFrame.width
            return false
        }

        #expect(fragmentWidth >= fittingWidth(for: .skill(name: "splunk-unipol-log-search")))
    }
}
