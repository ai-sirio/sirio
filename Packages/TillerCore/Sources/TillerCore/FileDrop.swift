import Foundation

/// What a file dropped on Tiller turned out to be. The caller does the I/O
/// (reading the bytes of an `.image`) and the presentation (a chip, a shell
/// word, a message); this type carries the decision only.
public enum DroppedItem: Equatable, Sendable {
    case image(url: URL, mimeType: String)
    /// Relative to the worktree when the file lives inside it, absolute
    /// otherwise. `ChatPromptBuilder` accepts both.
    case file(path: String)
    case rejected(url: URL, reason: DropRejection)
}

public enum DropRejection: Equatable, Sendable {
    case imageTooLarge(byteCount: Int)
}

/// The single rule engine for dropped files. Chat, terminal, and the Files
/// sidebar all route through `classify`, so no destination can drift away
/// from the rules.
public enum FileDrop {
    /// An image is inlined into the prompt as base64, which costs about a
    /// third more than the file itself. Ten megabytes is the point past
    /// which a stray drop would stall a turn instead of enriching it.
    public static let maxImageBytes = 10 * 1_024 * 1_024

    /// Deliberately a table and not `UTType`: the design names exactly these
    /// five extensions, and a table is verifiable without asking the system
    /// anything.
    private static let imageMimeTypes: [String: String] = [
        "png": "image/png",
        "jpg": "image/jpeg",
        "jpeg": "image/jpeg",
        "gif": "image/gif",
        "webp": "image/webp",
    ]

    /// `byteCount` is a parameter rather than a `resourceValues` read so this
    /// stays pure: the size-cap tests never have to create a 10 MB file.
    public static func classify(_ inputs: [(url: URL, byteCount: Int)],
                                worktreePath: String) -> [DroppedItem] {
        inputs.map { input in
            // A folder has no image extension, so it falls through to `.file`
            // with no branch of its own — one branch fewer to diverge.
            guard let mimeType = imageMimeTypes[input.url.pathExtension.lowercased()] else {
                return .file(path: chipPath(for: input.url, worktreePath: worktreePath))
            }
            guard input.byteCount <= maxImageBytes else {
                return .rejected(url: input.url,
                                 reason: .imageTooLarge(byteCount: input.byteCount))
            }
            return .image(url: input.url, mimeType: mimeType)
        }
    }

    /// Absolute, shell-quoted, space-separated, no trailing newline: the user
    /// decides when to press return.
    public static func terminalInsertion(_ urls: [URL]) -> String {
        urls.map { shellQuote($0.path) }.joined(separator: " ")
    }

    /// The trailing "/" is what stops "/repo-backup/x" from counting as
    /// inside "/repo".
    static func chipPath(for url: URL, worktreePath: String) -> String {
        let root = worktreePath.hasSuffix("/") ? worktreePath : worktreePath + "/"
        let path = url.standardizedFileURL.path
        guard path.hasPrefix(root) else { return path }
        return String(path.dropFirst(root.count))
    }
}
