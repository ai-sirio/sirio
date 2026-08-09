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
        .font: AppFont.nsBody,
        .foregroundColor: NSColor.textColor,
    ]

    private(set) var slashQuery: String?
    private(set) var mentionQuery: String?
    var isFocused = false

    /// Stored, not computed from `storage.length`: `@Observable` only tracks
    /// stored properties, and the storage is a `let` whose contents mutate
    /// inside AppKit. Computed from it, this never invalidated a SwiftUI read,
    /// so the composer's placeholder stayed on screen underneath the text
    /// being typed. Every mutation path refreshes it through `syncIsEmpty`.
    private(set) var isEmpty = true

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
        syncIsEmpty()
    }

    /// Called from every path that mutates the storage. Assigning only on a
    /// real change keeps SwiftUI from being woken on each keystroke of an
    /// already non-empty draft.
    private func syncIsEmpty() {
        let empty = storage.length == 0
        if isEmpty != empty { isEmpty = empty }
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
        syncIsEmpty()
        return NSRange(location: range.location + replacement.length, length: 0)
    }

    func insert(_ chip: ComposerChip, replacing range: NSRange) {
        storage.replaceCharacters(
            in: range,
            with: NSAttributedString(attachment: ComposerChipAttachment(chip: chip)))
        resetTypingAttributes()
        syncIsEmpty()
    }

    /// Drains the draft. The returned triple is exactly what
    /// `ChatController.send(text:mentionPaths:images:)` takes, so nothing below
    /// the UI knows chips exist.
    func takeDraft() -> ComposerDraft {
        let draft = ComposerDraft.parse(storage)
        storage.setAttributedString(NSAttributedString(string: ""))
        slashQuery = nil
        mentionQuery = nil
        syncIsEmpty()
        return draft
    }

    /// Re-stamps the base font after the interface font size changes. The
    /// draft already on screen carries the old size in its attribute runs, so
    /// updating only `typingAttributes` would leave a draft in two sizes.
    /// Attachment runs are skipped — a chip sizes itself.
    func applyBaseFont() {
        resetTypingAttributes()
        guard storage.length > 0 else { return }
        let full = NSRange(location: 0, length: storage.length)
        storage.beginEditing()
        storage.enumerateAttribute(.attachment, in: full) { attachment, range, _ in
            guard attachment == nil else { return }
            storage.addAttribute(.font, value: AppFont.nsBody, range: range)
        }
        storage.endEditing()
    }

    private func resetTypingAttributes() {
        typingAttributes = [
            .font: AppFont.nsBody,
            .foregroundColor: NSColor.textColor,
        ]
    }
}
