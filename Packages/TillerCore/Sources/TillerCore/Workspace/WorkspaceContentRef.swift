import Foundation

public enum DocumentEditorKind: String, Codable, Sendable {
    case markdown
    case code
}

public enum WorkspaceContentKind: String, Codable, Sendable {
    case terminal
    case chat
    case document
}

public enum WorkspaceContentRef: Hashable, Sendable, Codable {
    case terminal(TerminalContentID)
    case chat(ChatContentID)
    case document(DocumentID, editor: DocumentEditorKind)

    public var kind: WorkspaceContentKind {
        switch self {
        case .terminal:
            .terminal
        case .chat:
            .chat
        case .document:
            .document
        }
    }

    /// Stable string used by the unique (worktreeId, kind, contentId) constraint.
    public var contentIdentifierString: String {
        switch self {
        case .terminal(let rawValue):
            rawValue.rawValue.uuidString
        case .chat(let rawValue):
            rawValue.rawValue
        case .document(let documentID, editor: _):
            documentID.canonicalPath
        }
    }
}
