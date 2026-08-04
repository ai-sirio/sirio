import Foundation
import TillerCore

/// Projects the universal engine's layout into the row model the sidebar tree
/// (and everything else still typed on `LegacyWorkspaceTab`) reads.
///
/// The engine keeps its tabs in `WorkspaceLayout` and never writes the legacy
/// store, so without this projection the sidebar renders an empty tree for
/// every worktree once the engine gate is on.
enum SidebarTabProjection {
    /// - Parameters:
    ///   - livePaneID: resolves a terminal's stable content id to the pane id
    ///     its PTY is currently registered under — what the pane context menu
    ///     and `paneTitles` are keyed by. Nil before the surface has spawned.
    ///   - chatAgentID: the agent a chat session belongs to, for the row icon.
    static func rows(
        layout: WorkspaceLayout,
        livePaneID: (TerminalContentID) -> UUID? = { _ in nil },
        chatAgentID: (ChatContentID) -> String? = { _ in nil }
    ) -> [LegacyWorkspaceTab] {
        layout.allTabs.map { tab in
            LegacyWorkspaceTab(
                id: tab.id.rawValue,
                title: tab.title,
                content: content(
                    of: tab.content, livePaneID: livePaneID, chatAgentID: chatAgentID),
                titleIsAutoNamed: tab.titleIsAutoNamed)
        }
    }

    private static func content(
        of ref: WorkspaceContentRef,
        livePaneID: (TerminalContentID) -> UUID?,
        chatAgentID: (ChatContentID) -> String?
    ) -> LegacyTabContent {
        switch ref {
        case .terminal(let contentID):
            // One engine tab is one terminal: splits are groups in the engine,
            // not a tree inside the tab, so the projection is always a single
            // leaf. The content id stands in until the PTY registers, so the
            // row still reads as a terminal on the very first frame.
            .terminal(.leaf(id: livePaneID(contentID) ?? contentID.rawValue))
        case .chat(let contentID):
            .chat(agentId: chatAgentID(contentID) ?? "", sessionId: contentID.rawValue)
        case .document(let documentID, let editor):
            switch editor {
            case .code: .code(fileURL: URL(fileURLWithPath: documentID.canonicalPath))
            case .markdown: .markdown(fileURL: URL(fileURLWithPath: documentID.canonicalPath))
            }
        }
    }
}
