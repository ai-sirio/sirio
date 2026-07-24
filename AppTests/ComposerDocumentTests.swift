import AppKit
import Testing
import TillerACP

@testable import Tiller

@Suite("ComposerDocument")
@MainActor
struct ComposerDocumentTests {
    private let png = ImageAttachment(mimeType: "image/png", base64Data: "AAAA")

    private func document(_ text: String = "") -> ComposerDocument {
        let document = ComposerDocument()
        if !text.isEmpty {
            document.storage.append(NSAttributedString(string: text))
        }
        return document
    }

    @Test func freshDocumentIsEmpty() {
        #expect(document().isEmpty)
    }

    @Test func documentWithAChipIsNotEmpty() {
        let subject = document()
        subject.insert(.image(png), replacing: NSRange(location: 0, length: 0))
        #expect(!subject.isEmpty)
    }

    @Test func slashQueryIsTheLeadingTokenWithoutTheSlash() {
        let subject = document("/brain")
        subject.refreshQueries()
        #expect(subject.slashQuery == "brain")
    }

    @Test func slashQueryIsNilOnceTheTokenContainsWhitespace() {
        let subject = document("/brain storm")
        subject.refreshQueries()
        #expect(subject.slashQuery == nil)
    }

    @Test func slashQueryIsNilWhenTextDoesNotStartWithSlash() {
        let subject = document("hello")
        subject.refreshQueries()
        #expect(subject.slashQuery == nil)
    }

    @Test func mentionQueryIsTheTokenAfterTheLastAt() {
        let subject = document("look at @Chat")
        subject.refreshQueries()
        #expect(subject.mentionQuery == "Chat")
    }

    @Test func mentionQueryIsNilOnceTheTokenContainsWhitespace() {
        let subject = document("look at @Chat editor")
        subject.refreshQueries()
        #expect(subject.mentionQuery == nil)
    }

    @Test func replaceSlashTokenSwapsTheTokenForAChipAndASpace() {
        let subject = document("/brain")
        subject.refreshQueries()

        let caret = subject.replaceSlashToken(with: .skill(name: "brainstorm"))

        #expect(caret == NSRange(location: 2, length: 0))
        #expect(subject.storage.length == 2)
        let stored = subject.storage.attribute(
            .attachment, at: 0, effectiveRange: nil) as? ComposerChipAttachment
        #expect(stored?.chip == .skill(name: "brainstorm"))
        #expect(subject.slashQuery == nil)
    }

    @Test func replaceSlashTokenReturnsNilWithoutAToken() {
        let subject = document("hello")
        subject.refreshQueries()
        #expect(subject.replaceSlashToken(with: .skill(name: "x")) == nil)
    }

    @Test func insertPlacesTheChipAtTheGivenRange() {
        let subject = document("ab")
        subject.insert(.file(path: "README.md"), replacing: NSRange(location: 1, length: 0))

        #expect(subject.storage.length == 3)
        let stored = subject.storage.attribute(
            .attachment, at: 1, effectiveRange: nil) as? ComposerChipAttachment
        #expect(stored?.chip == .file(path: "README.md"))
    }

    @Test func insertRestoresDefaultTypingAttributes() {
        let subject = document()
        subject.insert(.skill(name: "x"), replacing: NSRange(location: 0, length: 0))
        #expect(subject.typingAttributes[.foregroundColor] as? NSColor == .textColor)
    }

    @Test func takeDraftReturnsTheParsedDraftAndEmptiesTheStorage() {
        let subject = document("hello")
        subject.insert(.file(path: "README.md"), replacing: NSRange(location: 5, length: 0))

        let draft = subject.takeDraft()

        #expect(draft.text == "hello")
        #expect(draft.mentionPaths == ["README.md"])
        #expect(subject.isEmpty)
        #expect(subject.storage.length == 0)
    }
}
