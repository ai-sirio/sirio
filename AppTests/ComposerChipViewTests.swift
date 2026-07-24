import AppKit
import SwiftUI
import Testing
import TillerACP

@testable import Tiller

@Suite("ComposerChipView")
@MainActor
struct ComposerChipViewTests {
    @Test func attachmentVendsAChipViewProvider() throws {
        let attachment = ComposerChipAttachment(chip: .skill(name: "brainstorm"))
        let textView = ChatTextEditor.makeTextView()
        let layoutManager = try #require(textView.textLayoutManager)

        let provider = attachment.viewProvider(
            for: textView, location: layoutManager.documentRange.location,
            textContainer: layoutManager.textContainer)

        #expect(provider is ComposerChipViewProvider)
    }

    @Test func providerLoadsANonEmptyHostingView() throws {
        let attachment = ComposerChipAttachment(chip: .file(path: "App/Chat/ChatTextEditor.swift"))
        let textView = ChatTextEditor.makeTextView()
        let layoutManager = try #require(textView.textLayoutManager)
        let provider = ComposerChipViewProvider(
            textAttachment: attachment, parentView: textView,
            textLayoutManager: layoutManager, location: layoutManager.documentRange.location)

        provider.loadView()

        #expect(provider.view is NSHostingView<ComposerChipView>)
        #expect(try #require(provider.view).fittingSize.width > 0)
        #expect(provider.tracksTextAttachmentViewBounds)
    }

    /// Guards the spec's open question: the composer keeps `isRichText = false`,
    /// and a chip inserted programmatically must still survive in the storage
    /// and keep its payload. If this fails, `isRichText` must be flipped to
    /// true and `pasteAsPlainText` / `importsGraphics = false` added — see the
    /// spec's "Open risk" section.
    @Test func chipSurvivesInsertionIntoAPlainTextView() {
        let textView = ChatTextEditor.makeTextView()
        #expect(!textView.isRichText)

        let chip = NSAttributedString(
            attachment: ComposerChipAttachment(chip: .skill(name: "review")))
        textView.textStorage?.append(chip)

        let stored = textView.textStorage?.attribute(
            .attachment, at: 0, effectiveRange: nil) as? ComposerChipAttachment
        #expect(stored?.chip == .skill(name: "review"))
        #expect(textView.textStorage?.length == 1)
    }
}
