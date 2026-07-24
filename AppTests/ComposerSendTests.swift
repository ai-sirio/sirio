import AppKit
import Testing
import TillerACP

@testable import Tiller

/// End-to-end over the draft path: chips go in, the triple that
/// `ChatController.send` accepts comes out, and the composer is left empty.
@Suite("ComposerSend")
@MainActor
struct ComposerSendTests {
    private let png = ImageAttachment(mimeType: "image/png", base64Data: "AAAA")

    @Test func acceptingASlashCommandLeavesAChipAndSendsTheCommand() {
        let document = ComposerDocument()
        document.storage.append(NSAttributedString(string: "/brain"))
        document.refreshQueries()

        _ = document.replaceSlashToken(with: .skill(name: "brainstorm"))
        document.storage.append(NSAttributedString(string: "an idea"))

        let draft = document.takeDraft()
        #expect(draft.text == "/brainstorm  an idea")
        #expect(document.isEmpty)
    }

    @Test func acceptingAMentionRemovesTheAtTokenAndAddsAFileChip() {
        let document = ComposerDocument()
        document.storage.append(NSAttributedString(string: "look at @Chat"))
        document.refreshQueries()
        #expect(document.mentionQuery == "Chat")

        // The composer replaces the "@token" range with the chip.
        let at = (document.storage.string as NSString).range(of: "@Chat")
        document.insert(.file(path: "App/Chat/ChatTextEditor.swift"), replacing: at)

        let draft = document.takeDraft()
        #expect(draft.text == "look at ")
        #expect(draft.mentionPaths == ["App/Chat/ChatTextEditor.swift"])
    }

    @Test func attachingAnImageAddsAChipAndSendsTheAttachment() {
        let document = ComposerDocument()
        document.insert(.image(png), replacing: NSRange(location: 0, length: 0))

        let draft = document.takeDraft()
        #expect(draft.images == [png])
        #expect(draft.text.isEmpty)
    }

    @Test func draftWithOnlyAChipIsStillSendable() {
        let document = ComposerDocument()
        document.insert(.image(png), replacing: NSRange(location: 0, length: 0))
        #expect(!document.isEmpty)
    }
}
