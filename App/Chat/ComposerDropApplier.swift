import AppKit
import Foundation
import TillerACP
import TillerCore

/// Turns classified drop items into composer chips.
///
/// Split out of the `.onDrop` closure so it can be tested without SwiftUI:
/// the closure keeps only the parts that must touch the view.
enum ComposerDropApplier {
    /// Appends one chip per attachable item at the end of the draft and
    /// returns the user-facing messages for the items that were not
    /// attached. Nothing is inserted for a rejected item.
    @MainActor
    @discardableResult
    static func apply(_ items: [DroppedItem], to document: ComposerDocument) -> [String] {
        var messages: [String] = []
        for item in items {
            switch item {
            case .image(let url, let mimeType):
                guard let data = try? Data(contentsOf: url) else {
                    messages.append("Couldn't read \(url.lastPathComponent)")
                    continue
                }
                let attachment = ImageAttachment(mimeType: mimeType,
                                                 base64Data: data.base64EncodedString())
                insert(.image(attachment), into: document)
            case .file(let path):
                insert(.file(path: path), into: document)
            case .rejected(let url, .imageTooLarge):
                messages.append("\(url.lastPathComponent) is too large (max 10 MB)")
            }
        }
        return messages
    }

    /// Chips always land at the end of the draft: a drop has no caret of its
    /// own, and the end is where `attachImage` already puts them.
    @MainActor
    private static func insert(_ chip: ComposerChip, into document: ComposerDocument) {
        document.insert(chip,
                        replacing: NSRange(location: document.storage.length, length: 0))
    }
}
