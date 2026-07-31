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

    /// Mints the conversation a fresh chat tab will own, returning its
    /// ChatSessionStore session id. Assigned by AppModel after init, because
    /// the adapters are built inside AppModel.init, before `self` exists.
    var makeSession: ((_ worktreeID: UUID, _ agentID: String) throws -> String)?

    /// Builds the tab's chat UI. Returns an NSViewController rather than a
    /// ChatController so this adapter stays ignorant of what a chat is — the
    /// app owns controller construction, status wiring, and teardown.
    /// `isFresh` is true only for a chat created by `.newChat` in this run:
    /// those start a conversation, resumed and restored ones stay detached.
    var makeContentViewController: (
        (_ tab: WorkspaceTab, _ worktree: Worktree, _ isFresh: Bool) -> NSViewController?)?

    /// Tears the app-side chat down (stop the agent, drop an empty session
    /// row). Called once per tab, on close.
    var releaseContent: ((WorkspaceTabID) -> Void)?

    /// Stored title of a resumed conversation, so reopening it from the
    /// history menu does not land on a tab labelled "Chat".
    var resolveTitle: ((ChatContentID) -> String?)?

    private var viewControllers: [WorkspaceTabID: NSViewController] = [:]
    private var freshChats: Set<WorkspaceTabID> = []

    init(boundary: AdapterBoundary = AdapterBoundary()) { self.boundary = boundary }

    func prepare(request: ContentRequest, worktree: Worktree) async throws -> PreparedContent {
        guard let tab = prepareTab(request: request, worktreeID: worktree.id) else {
            throw ContentAdapterError.unsupportedRequest
        }
        let token = runtimes[tab.id]!
        return PreparedContent(tab: tab, generationID: token.generationID, opaqueToken: token)
    }

    /// A chat tab's ChatContentID *is* its session id, so the tab can always
    /// resolve its conversation — and, through the session row, its agent —
    /// without the universal model having to know what an agent is. A chat
    /// that cannot get a session id must not become a tab: it could never
    /// find its own transcript.
    func prepareTab(request: ContentRequest, worktreeID: UUID) -> WorkspaceTab? {
        let contentID: ChatContentID
        switch request {
        case .newChat(let agentID):
            guard let makeSession,
                  let sessionID = try? makeSession(worktreeID, agentID) else { return nil }
            contentID = ChatContentID(sessionID)
        case .resumeChat(let id):
            contentID = id
        default:
            return nil
        }
        let isFresh: Bool = if case .newChat = request { true } else { false }
        // "Chat" until the first turn: auto-rename replaces it.
        let title = isFresh ? nil : resolveTitle?(contentID)
        let tab = WorkspaceTab(id: WorkspaceTabID(), title: title ?? "Chat",
                               titleIsAutoNamed: true, content: .chat(contentID))
        let token = AdapterRuntimeToken(
            tabID: tab.id, contentID: contentID.rawValue, generationID: ResourceGenerationID())
        token.phase = .preparing
        tabs[tab.id] = tab
        runtimes[tab.id] = token
        if isFresh { freshChats.insert(tab.id) }
        return tab
    }

    func hydrate(tab: WorkspaceTab, worktree: Worktree) async {
        guard let runtime = runtimes[tab.id], !runtime.released, !hydrated.contains(tab.id) else { return }
        hydrated.insert(tab.id)
        await boundary.hydrate(tab, worktree)
        runtime.phase = .active
    }

    func makeHost(tab: WorkspaceTab, worktree: Worktree) -> WorkspaceContentHost {
        let controller = viewControllers[tab.id]
            ?? makeContentViewController?(tab, worktree, freshChats.contains(tab.id))
            ?? NSViewController()
        viewControllers[tab.id] = controller
        return WorkspaceContentHostAdapter(tabID: tab.id, viewController: controller)
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
        releaseContent?(tab.id)
        runtimes.removeValue(forKey: tab.id)
        tabs.removeValue(forKey: tab.id)
        interrupted.removeValue(forKey: tab.id)
        viewControllers.removeValue(forKey: tab.id)
        freshChats.remove(tab.id)
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
