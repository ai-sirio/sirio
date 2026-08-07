import Foundation
import TillerCore
import TillerWorkspace

enum PaneTabIconKind: Equatable {
    case terminal(agentID: String?)
    case chat(agentID: String?)
    case document(DocumentEditorKind)
    case browser(faviconURL: URL?)

    var agentID: String? {
        switch self {
        case .terminal(let id), .chat(let id): id
        case .document, .browser: nil
        }
    }
}

struct PaneTabPresentation: Equatable {
    let icon: PaneTabIconKind
    let isDirty: Bool
    let agentStatus: AgentStatus?

    var kindLabel: String {
        switch icon {
        case .terminal: "Terminal"
        case .chat: "Chat"
        case .document(.markdown): "Markdown document"
        case .document(.code): "Code document"
        case .browser: "Browser"
        }
    }

    func accessibilityLabel(for entry: TabMenuEntry) -> String {
        return [
            kindLabel,
            entry.title,
            entry.isActive ? "selected" : nil,
            isDirty ? "modified" : nil,
            agentStatus?.humanLabel
        ].compactMap { $0 }.joined(separator: ", ")
    }
}

@MainActor
struct PaneTabPresentationResolver {
    let isDirty: (WorkspaceTabID) -> Bool
    let liveTerminalPane: (TerminalContentID) -> UUID?
    let browserFaviconURL: (BrowserContentID) -> URL?
    let status: ([UUID]) -> AgentStatus?
    let agentID: ([UUID]) -> String?

    func resolve(_ entry: TabMenuEntry) -> PaneTabPresentation {
        switch entry.content {
        case .terminal(let contentID):
            let paneIDs = liveTerminalPane(contentID).map { [$0] } ?? []
            let agent = agentID(paneIDs)
            return PaneTabPresentation(
                icon: .terminal(agentID: agent),
                isDirty: false,
                agentStatus: status(paneIDs))
        case .chat:
            let paneIDs = [entry.tabID.rawValue]
            let agent = agentID(paneIDs)
            return PaneTabPresentation(
                icon: .chat(agentID: agent),
                isDirty: false,
                agentStatus: status(paneIDs))
        case .document(_, let editor):
            return PaneTabPresentation(
                icon: .document(editor),
                isDirty: isDirty(entry.tabID),
                agentStatus: nil)
        case .browser(let contentID):
            return PaneTabPresentation(
                icon: .browser(faviconURL: browserFaviconURL(contentID)),
                isDirty: false,
                agentStatus: nil)
        case nil:
            return PaneTabPresentation(
                icon: .terminal(agentID: nil),
                isDirty: false,
                agentStatus: nil)
        }
    }
}
