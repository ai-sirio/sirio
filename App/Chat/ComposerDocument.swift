import AppKit
import Observation

/// Owns the composer's draft as an `NSTextStorage`, which is the single source
/// of truth for both text and chips.
///
/// This replaces the `@Binding var text: String` the composer used before.
/// That binding is unusable once chips exist: `NSTextView.string` returns bare
/// `U+FFFC` characters for attachments, so the mirror comparison almost always
/// fails, and assigning `.string` wipes the whole attributed storage —
/// destroying every chip on any SwiftUI update.
///
/// SwiftUI reads the cheap derived values (`isEmpty`, `slashQuery`,
/// `mentionQuery`) that `refreshQueries` recomputes on each edit. The full walk
/// of the storage happens only in `takeDraft`, at send time.
@Observable
@MainActor
final class ComposerDocument {
    let storage = NSTextStorage()

    /// Mirrors the text view's `typingAttributes` so a chip insertion cannot
    /// leak attachment attributes into the text typed after it.
    var typingAttributes: [NSAttributedString.Key: Any] = [
        .font: NSFont.systemFont(ofSize: NSFont.systemFontSize),
        .foregroundColor: NSColor.textColor,
    ]

    private(set) var slashQuery: String?
    private(set) var mentionQuery: String?
    var isFocused = false

    var isEmpty: Bool { storage.length == 0 }

    /// The leading `/token`, as an NSString range, when the draft is a single
    /// unbroken slash token. Whitespace anywhere in it means the user has
    /// moved on to arguments and the popup should be closed.
    private var slashTokenRange: NSRange? {
        let string = storage.string as NSString
        guard string.hasPrefix("/") else { return nil }
        let whitespace = string.rangeOfCharacter(from: .whitespacesAndNewlines)
        guard whitespace.location == NSNotFound else { return nil }
        return NSRange(location: 0, length: string.length)
    }

    func refreshQueries() {
        slashQuery = slashTokenRange.map {
            String((storage.string as NSString).substring(with: $0).dropFirst())
        }
        mentionQuery = Self.mentionToken(in: storage.string)
    }

    /// The active `@`-token runs from the last "@" to the end of the text, with
    /// no whitespace inside. Deliberately simple, matching the previous
    /// behaviour in `ChatComposerView.updateMentionQuery`.
    static func mentionToken(in text: String) -> String? {
        guard let at = text.lastIndex(of: "@") else { return nil }
        let token = String(text[text.index(after: at)...])
        guard !token.contains(where: \.isWhitespace) else { return nil }
        return token
    }

    /// Swaps the in-progress `/token` for a skill chip plus a trailing space.
    /// Returns the caret range to apply after the swap, or nil when there is
    /// no token to replace.
    func replaceSlashToken(with chip: ComposerChip) -> NSRange? {
        guard let range = slashTokenRange else { return nil }
        let replacement = NSMutableAttributedString(
            attachment: ComposerChipAttachment(chip: chip))
        replacement.append(NSAttributedString(string: " ",
                                              attributes: typingAttributes))
        storage.replaceCharacters(in: range, with: replacement)
        resetTypingAttributes()
        slashQuery = nil
        return NSRange(location: range.location + replacement.length, length: 0)
    }

    func insert(_ chip: ComposerChip, replacing range: NSRange) {
        storage.replaceCharacters(
            in: range,
            with: NSAttributedString(attachment: ComposerChipAttachment(chip: chip)))
        resetTypingAttributes()
    }

    /// Drains the draft. The returned triple is exactly what
    /// `ChatController.send(text:mentionPaths:images:)` takes, so nothing below
    /// the UI knows chips exist.
    func takeDraft() -> ComposerDraft {
        let draft = ComposerDraft.parse(storage)
        storage.setAttributedString(NSAttributedString(string: ""))
        slashQuery = nil
        mentionQuery = nil
        return draft
    }

    private func resetTypingAttributes() {
        typingAttributes = [
            .font: NSFont.systemFont(ofSize: NSFont.systemFontSize),
            .foregroundColor: NSColor.textColor,
        ]
    }
}
