import AppKit
import Foundation
import Testing
import TillerACP
import TillerCore

@testable import Tiller

@Suite("ComposerDropApplier")
@MainActor
struct ComposerDropApplierTests {
    private func temporaryPNG() throws -> URL {
        let url = URL(fileURLWithPath: NSTemporaryDirectory())
            .appendingPathComponent("\(UUID().uuidString).png")
        try Data([0x89, 0x50, 0x4E, 0x47]).write(to: url)
        return url
    }

    @Test func imagesBecomeImageChipsCarryingBase64Bytes() throws {
        let url = try temporaryPNG()
        defer { try? FileManager.default.removeItem(at: url) }
        let document = ComposerDocument()

        let messages = ComposerDropApplier.apply(
            [.image(url: url, mimeType: "image/png")], to: document)

        #expect(messages.isEmpty)
        let draft = ComposerDraft.parse(document.storage)
        #expect(draft.images == [ImageAttachment(mimeType: "image/png",
                                                 base64Data: "iVBORw==")])
    }

    @Test func filesBecomeMentionPathsInDropOrder() {
        let document = ComposerDocument()

        let messages = ComposerDropApplier.apply(
            [.file(path: "Sources/App.swift"), .file(path: "/tmp/notes.txt")],
            to: document)

        #expect(messages.isEmpty)
        #expect(ComposerDraft.parse(document.storage).mentionPaths
                == ["Sources/App.swift", "/tmp/notes.txt"])
    }

    @Test func oversizedImagesProduceAMessageAndNoChip() {
        let document = ComposerDocument()

        let messages = ComposerDropApplier.apply(
            [.rejected(url: URL(fileURLWithPath: "/tmp/huge.png"),
                       reason: .imageTooLarge(byteCount: FileDrop.maxImageBytes + 1))],
            to: document)

        #expect(messages == ["huge.png is too large (max 10 MB)"])
        #expect(document.isEmpty)
    }

    @Test func aMixedDropAttachesWhatItCanAndReportsTheRest() throws {
        let url = try temporaryPNG()
        defer { try? FileManager.default.removeItem(at: url) }
        let document = ComposerDocument()

        let messages = ComposerDropApplier.apply([
            .image(url: url, mimeType: "image/png"),
            .file(path: "notes.txt"),
            .rejected(url: URL(fileURLWithPath: "/tmp/huge.png"),
                      reason: .imageTooLarge(byteCount: FileDrop.maxImageBytes + 1)),
        ], to: document)

        let draft = ComposerDraft.parse(document.storage)
        #expect(draft.images.count == 1)
        #expect(draft.mentionPaths == ["notes.txt"])
        #expect(messages.count == 1)
    }
}
