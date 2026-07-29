import Foundation

public struct WorkspaceTabViewState: Equatable, Sendable, Codable {
    public var documentCaretOffset: Int?
    public var documentSelectionLength: Int?
    public var documentScrollAnchor: Double?
    public var documentFoldedRanges: [ClosedRange<Int>]
    public var editorMode: DocumentEditorKind?
    public var chatComposerDraft: String?
    public var chatAttachmentReferences: [String]
    public var chatTranscriptAnchor: Double?
    public var followsTail: Bool
    public var terminalViewportAnchor: Double?

    public static var empty: WorkspaceTabViewState {
        WorkspaceTabViewState(
            documentCaretOffset: nil,
            documentSelectionLength: nil,
            documentScrollAnchor: nil,
            documentFoldedRanges: [],
            editorMode: nil,
            chatComposerDraft: nil,
            chatAttachmentReferences: [],
            chatTranscriptAnchor: nil,
            followsTail: false,
            terminalViewportAnchor: nil
        )
    }
}

public struct WorkspaceTab: Identifiable, Equatable, Sendable, Codable {
    public let id: WorkspaceTabID
    public var title: String
    public var titleIsAutoNamed: Bool
    public let content: WorkspaceContentRef
    public var viewState: WorkspaceTabViewState

    public init(id: WorkspaceTabID, title: String, titleIsAutoNamed: Bool,
                content: WorkspaceContentRef, viewState: WorkspaceTabViewState = .empty) {
        self.id = id
        self.title = title
        self.titleIsAutoNamed = titleIsAutoNamed
        self.content = content
        self.viewState = viewState
    }
}
