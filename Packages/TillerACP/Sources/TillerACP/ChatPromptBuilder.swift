import Foundation

/// An image the user attached to the composer, ready for an ACP image block.
public struct ImageAttachment: Sendable, Equatable {
    public var mimeType: String
    public var base64Data: String
    public init(mimeType: String, base64Data: String) {
        self.mimeType = mimeType
        self.base64Data = base64Data
    }
}

/// Assembles the `session/prompt` content blocks from composer state.
public enum ChatPromptBuilder {
    public static func build(text: String, mentionPaths: [String],
                             images: [ImageAttachment],
                             worktreePath: String) -> [ContentBlock] {
        var blocks: [ContentBlock] = []
        let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
        if !trimmed.isEmpty { blocks.append(.text(trimmed)) }
        for path in mentionPaths {
            // A dropped file can live anywhere on disk, so an absolute path is
            // used as-is; appending it to the worktree would build a URI that
            // resolves to nothing and fail silently at the agent.
            let url = path.hasPrefix("/")
                ? URL(fileURLWithPath: path)
                : URL(fileURLWithPath: worktreePath).appendingPathComponent(path)
            blocks.append(.resourceLink(uri: url.absoluteString,
                                        name: url.lastPathComponent))
        }
        for image in images {
            blocks.append(.image(mimeType: image.mimeType, data: image.base64Data))
        }
        return blocks
    }
}
