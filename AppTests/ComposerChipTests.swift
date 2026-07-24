import AppKit
import Testing
import TillerACP

@testable import Tiller

@Suite("ComposerChip")
@MainActor
struct ComposerChipTests {
    private let png = ImageAttachment(mimeType: "image/png", base64Data: "AAAA")

    private func storage(_ pieces: [Any]) -> NSTextStorage {
        let result = NSTextStorage()
        for piece in pieces {
            switch piece {
            case let text as String:
                result.append(NSAttributedString(string: text))
            case let chip as ComposerChip:
                result.append(NSAttributedString(attachment: ComposerChipAttachment(chip: chip)))
            default:
                Issue.record("unsupported storage piece")
            }
        }
        return result
    }

    @Test func emptyStorageProducesEmptyDraft() {
        #expect(ComposerDraft.parse(NSTextStorage())
                == ComposerDraft(text: "", mentionPaths: [], images: []))
    }

    @Test func plainTextPassesThroughUnchanged() {
        let draft = ComposerDraft.parse(storage(["hello world"]))
        #expect(draft == ComposerDraft(text: "hello world", mentionPaths: [], images: []))
    }

    @Test func skillChipSerializesAsSlashCommand() {
        let draft = ComposerDraft.parse(storage([ComposerChip.skill(name: "brainstorm"), "do it"]))
        #expect(draft.text == "/brainstorm do it")
        #expect(draft.mentionPaths.isEmpty)
        #expect(draft.images.isEmpty)
    }

    @Test func fileChipsBecomeMentionPathsAndLeaveNoText() {
        let draft = ComposerDraft.parse(storage([
            ComposerChip.file(path: "App/Chat/ChatTextEditor.swift"),
            ComposerChip.file(path: "README.md"),
            "review these",
        ]))
        #expect(draft.text == "review these")
        #expect(draft.mentionPaths == ["App/Chat/ChatTextEditor.swift", "README.md"])
    }

    @Test func duplicateFileChipsAreCollapsed() {
        let draft = ComposerDraft.parse(storage([
            ComposerChip.file(path: "README.md"),
            ComposerChip.file(path: "README.md"),
        ]))
        #expect(draft.mentionPaths == ["README.md"])
    }

    @Test func imageChipsBecomeImageAttachments() {
        let draft = ComposerDraft.parse(storage([ComposerChip.image(png), "what is this"]))
        #expect(draft.text == "what is this")
        #expect(draft.images == [png])
    }

    @Test func allThreeKindsParseTogetherInOrder() {
        let draft = ComposerDraft.parse(storage([
            ComposerChip.skill(name: "review"),
            "look at ",
            ComposerChip.file(path: "a.swift"),
            " and ",
            ComposerChip.image(png),
            " please",
        ]))
        #expect(draft.text == "/review look at  and  please")
        #expect(draft.mentionPaths == ["a.swift"])
        #expect(draft.images == [png])
    }

    @Test func iconNamesAreFixedPerKind() {
        #expect(ComposerChip.skill(name: "x").iconName == "cube")
        #expect(ComposerChip.file(path: "dir/a.swift").iconName == "doc")
        #expect(ComposerChip.image(png).iconName == "photo")
    }

    @Test func fileChipLabelIsTheLastPathComponent() {
        #expect(ComposerChip.file(path: "App/Chat/ChatTextEditor.swift").label
                == "ChatTextEditor.swift")
        #expect(ComposerChip.skill(name: "brainstorm").label == "brainstorm")
    }
}
