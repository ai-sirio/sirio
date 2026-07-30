import AppKit
import Foundation
import TillerCore
import TillerWorkspace

@MainActor
final class ChatContentAdapter: WorkspaceContentAdapter {
    let kind: WorkspaceContentKind = .chat
    private let boundary: AdapterBoundary
    private var tabs: [WorkspaceTabID: WorkspaceTab] = [:]
    private var runtimes: [WorkspaceTabID: AdapterRuntimeToken] = [:]
    private var interrupted: [WorkspaceTabID: [String]] = [:]
    private var hydrated: Set<WorkspaceTabID> = []

    init(boundary: AdapterBoundary = AdapterBoundary()) { self.boundary = boundary }

    func prepare(request: ContentRequest, worktree: Worktree) async throws -> PreparedContent {
        guard let tab = prepareTab(request: request) else {
            throw ContentAdapterError.unsupportedRequest
        }
        let token = runtimes[tab.id]!
        return PreparedContent(tab: tab, generationID: token.generationID, opaqueToken: token)
    }

    func prepareTab(request: ContentRequest) -> WorkspaceTab? {
        let contentID: ChatContentID
        let title: String
        switch request {
        case .newChat(let agentID):
            contentID = ChatContentID(UUID().uuidString)
            title = agentID
        case .resumeChat(let id):
            contentID = id
            title = "Chat"
        default:
            return nil
        }
        let tab = WorkspaceTab(id: WorkspaceTabID(), title: title, titleIsAutoNamed: true,
                              content: .chat(contentID))
        let token = AdapterRuntimeToken(
            tabID: tab.id, contentID: contentID.rawValue, generationID: ResourceGenerationID())
        token.phase = .preparing
        tabs[tab.id] = tab
        runtimes[tab.id] = token
        return tab
    }

    func hydrate(tab: WorkspaceTab, worktree: Worktree) async {
        guard let runtime = runtimes[tab.id], !runtime.released, !hydrated.contains(tab.id) else { return }
        hydrated.insert(tab.id)
        await boundary.hydrate(tab, worktree)
        runtime.phase = .active
    }

    func makeHost(tab: WorkspaceTab, worktree: Worktree) -> WorkspaceContentHost {
        WorkspaceContentHostAdapter(tabID: tab.id, viewController: NSViewController())
    }

    func checkpoint(tab: WorkspaceTab) async { await boundary.checkpoint(tab) }

    func retry(tab: WorkspaceTab, worktree: Worktree) async -> ResourceGenerationID {
        let token = runtimes[tab.id] ?? AdapterRuntimeToken(
            tabID: tab.id, contentID: tab.content.contentIdentifierString,
            generationID: ResourceGenerationID())
        token.generationID = ResourceGenerationID()
        token.phase = .preparing
        token.released = false
        hydrated.remove(tab.id)
        runtimes[tab.id] = token
        tabs[tab.id] = tab
        return token.generationID
    }

    func close(tab: WorkspaceTab) async {
        guard let runtime = runtimes[tab.id], !runtime.released else { return }
        runtime.phase = .closing
        runtime.released = true
        await boundary.close(tab)
        runtimes.removeValue(forKey: tab.id)
        tabs.removeValue(forKey: tab.id)
        interrupted.removeValue(forKey: tab.id)
        hydrated.remove(tab.id)
    }

    func dispose(prepared: PreparedContent) async {
        guard let token = prepared.opaqueToken as? AdapterRuntimeToken, !token.disposed else { return }
        token.disposed = true
        token.released = true
        await boundary.dispose(prepared.opaqueToken)
        runtimes.removeValue(forKey: token.tabID)
        tabs.removeValue(forKey: token.tabID)
    }

    func recordInterruptedTurn(tabID: WorkspaceTabID, text: String) {
        interrupted[tabID, default: []].append(text)
        runtimes[tabID]?.phase = .active
    }

    func interruptedTurns(for tabID: WorkspaceTabID) -> [String] { interrupted[tabID] ?? [] }
    func phase(for tabID: WorkspaceTabID) -> ContentPhase? { runtimes[tabID]?.phase }
    func generation(for tabID: WorkspaceTabID) -> ResourceGenerationID? { runtimes[tabID]?.generationID }
}
