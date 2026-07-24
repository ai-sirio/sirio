import AppKit
import TillerACP

/// A composer token rendered as an inline chip. Each chip occupies exactly
/// one `U+FFFC` attachment character in the composer's text storage, which is
/// what makes it atomic for free: AppKit already treats that character as
/// indivisible for caret movement, selection, and backspace.
enum ComposerChip: Equatable {
    case skill(name: String)
    case file(path: String)
    case image(ImageAttachment)

    /// Fixed per kind — `AvailableCommand` carries no icon metadata, and the
    /// file/image symbols match what the old chip row used.
    var iconName: String {
        switch self {
        case .skill: "cube"
        case .file: "doc"
        case .image: "photo"
        }
    }

    var label: String {
        switch self {
        case .skill(let name): name
        case .file(let path): (path as NSString).lastPathComponent
        case .image: "Image"
        }
    }
}

/// Carries a `ComposerChip` through the text storage. A class because
/// `NSTextAttachment` is one.
final class ComposerChipAttachment: NSTextAttachment {
    let chip: ComposerChip

    init(chip: ComposerChip) {
        self.chip = chip
        super.init(data: nil, ofType: nil)
    }

    /// Composer chips are never archived — the draft lives only in memory.
    /// Failing the initializer rather than trapping avoids adding a crash
    /// site for a path that is not exercised.
    required init?(coder: NSCoder) { nil }

    /// Overridden per-attachment rather than registered with
    /// `NSTextAttachment.registerViewProviderClass(_:forFileType:)`, because
    /// that registry is process-global and a composer chip is a detail of one
    /// view, not a document type.
    override func viewProvider(
        for parentView: NSView?, location: NSTextLocation,
        textContainer: NSTextContainer?
    ) -> NSTextAttachmentViewProvider? {
        ComposerChipViewProvider(
            textAttachment: self, parentView: parentView,
            textLayoutManager: textContainer?.textLayoutManager, location: location)
    }
}

/// What the composer hands to `ChatController.send`. Deliberately the exact
/// shape that call already takes, so chips change nothing below the UI.
struct ComposerDraft: Equatable {
    var text: String
    var mentionPaths: [String]
    var images: [ImageAttachment]

    /// Walks the storage once. Non-chip runs contribute their characters to
    /// `text`; chip runs contribute to the matching list instead, except the
    /// skill chip, which re-serializes as the `/name ` prefix the agent
    /// expects on the wire.
    static func parse(_ storage: NSAttributedString) -> ComposerDraft {
        var draft = ComposerDraft(text: "", mentionPaths: [], images: [])
        let string = storage.string as NSString
        storage.enumerateAttribute(
            .attachment, in: NSRange(location: 0, length: storage.length)
        ) { value, range, _ in
            guard let chip = (value as? ComposerChipAttachment)?.chip else {
                draft.text += string.substring(with: range)
                return
            }
            switch chip {
            case .skill(let name):
                draft.text += "/\(name) "
            case .file(let path):
                if !draft.mentionPaths.contains(path) { draft.mentionPaths.append(path) }
            case .image(let attachment):
                draft.images.append(attachment)
            }
        }
        return draft
    }
}
