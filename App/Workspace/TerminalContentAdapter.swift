import AppKit
import Foundation
import TillerCore
import TillerWorkspace
import TillerTerminal
import TillerControl

@MainActor
final class TerminalContentAdapter: WorkspaceContentAdapter {
    let kind: WorkspaceContentKind = .terminal
    private let boundary: AdapterBoundary
    private var tabs: [WorkspaceTabID: WorkspaceTab] = [:]
    private var runtimes: [WorkspaceTabID: AdapterRuntimeToken] = [:]
    private var closedTabIDs: Set<WorkspaceTabID> = []

    init(boundary: AdapterBoundary = AdapterBoundary()) { self.boundary = boundary }

    func prepare(request: ContentRequest, worktree: Worktree) async throws -> PreparedContent {
        let command: String?
        switch request {
        case .newTerminal(let requestedCommand): command = requestedCommand
        case .agentTerminal: command = nil
        default: command = nil
        }
        guard isTerminalRequest(request) else {
            throw ContentAdapterError.unsupportedRequest
        }
        let tab = makeTab(request: request)
        let token = AdapterRuntimeToken(
            tabID: tab.id, contentID: tab.content.contentIdentifierString,
            generationID: ResourceGenerationID(), command: command)
        token.phase = .preparing
        tabs[tab.id] = tab
        runtimes[tab.id] = token
        return PreparedContent(tab: tab, generationID: token.generationID, opaqueToken: token)
    }

    func hydrate(tab: WorkspaceTab, worktree: Worktree) async {
        guard !closedTabIDs.contains(tab.id) else { return }
        let runtime = ensureRuntime(for: tab)
        guard !runtime.released else { return }
        await boundary.hydrate(tab, worktree)
        guard !runtime.released else { return }
        runtime.hydrated = true
        runtime.phase = .active
    }

    func makeHost(tab: WorkspaceTab, worktree: Worktree) -> WorkspaceContentHost {
        guard case .terminal(let contentID) = tab.content else {
            return WorkspaceContentHostAdapter(tabID: tab.id, viewController: NSViewController())
        }
        let surface = TerminalSurfaceHost(
            contentID: contentID,
            configuration: TerminalSurfaceConfiguration(
                workingDirectory: worktree.path,
                command: runtimes[tab.id]?.command,
                extraEnvironment: [
                    "TILLER_ENV": "1",
                    "TILLER_SOCKET": ControlSocket.defaultPath(),
                    "TILLER_WORKTREE_ID": worktree.id.uuidString
                ]))
        // The PTY registers under TerminalSurfaceHost's generation. Keep the
        // adapter's live lookup aligned with that runtime key; the content id
        // remains stable across relaunches.
        runtimes[tab.id]?.generationID = surface.generationID
        return WorkspaceContentHostAdapter(
            tabID: tab.id,
            viewController: surface.viewController,
            focus: { _ in surface.focusTerminal() },
            release: { await surface.teardown() })
    }

    func checkpoint(tab: WorkspaceTab) async { await boundary.checkpoint(tab) }

    func retry(tab: WorkspaceTab, worktree: Worktree) async -> ResourceGenerationID {
        let token = runtimes[tab.id] ?? AdapterRuntimeToken(
            tabID: tab.id, contentID: tab.content.contentIdentifierString,
            generationID: ResourceGenerationID())
        let replacement = ResourceGenerationID()
        token.generationID = replacement
        token.phase = .preparing
        token.released = false
        token.hydrated = false
        closedTabIDs.remove(tab.id)
        runtimes[tab.id] = token
        tabs[tab.id] = tab
        return replacement
    }

    func close(tab: WorkspaceTab) async {
        guard closedTabIDs.insert(tab.id).inserted else { return }
        let runtime = ensureRuntime(for: tab)
        guard !runtime.released else { return }
        runtime.phase = .closing
        runtime.released = true
        await boundary.close(tab)
        runtimes.removeValue(forKey: tab.id)
        tabs.removeValue(forKey: tab.id)
    }

    func dispose(prepared: PreparedContent) async {
        guard let token = prepared.opaqueToken as? AdapterRuntimeToken, !token.disposed else { return }
        token.disposed = true
        token.released = true
        await boundary.dispose(prepared.opaqueToken)
        runtimes.removeValue(forKey: token.tabID)
        tabs.removeValue(forKey: token.tabID)
    }

    func phase(for tabID: WorkspaceTabID) -> ContentPhase? { runtimes[tabID]?.phase }
    func generation(for tabID: WorkspaceTabID) -> ResourceGenerationID? { runtimes[tabID]?.generationID }
    func command(for tabID: WorkspaceTabID) -> String? { runtimes[tabID]?.command }

    func markFailed(tabID: WorkspaceTabID, reason: String) {
        runtimes[tabID]?.phase = .failed(reason: reason)
    }

    private func ensureRuntime(for tab: WorkspaceTab) -> AdapterRuntimeToken {
        if let runtime = runtimes[tab.id] { return runtime }
        let runtime = AdapterRuntimeToken(
            tabID: tab.id, contentID: tab.content.contentIdentifierString,
            generationID: ResourceGenerationID())
        tabs[tab.id] = tab
        runtimes[tab.id] = runtime
        return runtime
    }

    private func makeTab(request: ContentRequest) -> WorkspaceTab {
        let tabID = WorkspaceTabID()
        let contentID = TerminalContentID(tabID.rawValue)
        let title: String
        switch request {
        case .newTerminal: title = "Terminal"
        case .agentTerminal(let agentID): title = agentID
        default: title = "Terminal"
        }
        return WorkspaceTab(id: tabID, title: title, titleIsAutoNamed: true,
                            content: .terminal(contentID))
    }

    private func isTerminalRequest(_ request: ContentRequest) -> Bool {
        switch request {
        case .newTerminal, .agentTerminal: return true
        default: return false
        }
    }
}
