import Foundation
import TillerCore
import TillerWorkspace

struct PreparedContent: @unchecked Sendable {
    let tab: WorkspaceTab
    let generationID: ResourceGenerationID
    let opaqueToken: AnyObject

    init(tab: WorkspaceTab, generationID: ResourceGenerationID, opaqueToken: AnyObject) {
        self.tab = tab
        self.generationID = generationID
        self.opaqueToken = opaqueToken
    }
}

enum ContentPhase: Equatable, Sendable {
    case dormant, preparing, active, inactive, closing
    case failed(reason: String)
}

enum TerminalDetail: Equatable, Sendable { case running, exited(code: Int32) }
enum ChatDetail: Equatable, Sendable { case idle, turning, disconnected, interrupted }
enum DocumentDetail: Equatable, Sendable { case available, dirty, conflicted, missing }

enum ContentRequest: Sendable, Equatable {
    case newTerminal(command: String?)
    case agentTerminal(agentID: String)
    case newChat(agentID: String)
    case resumeChat(ChatContentID)
    case openFile(URL, editor: DocumentEditorKind)
}

struct AdapterBoundary: Sendable {
    typealias Hydrate = @Sendable (WorkspaceTab, Worktree) async -> Void
    typealias Checkpoint = @Sendable (WorkspaceTab) async -> Void
    typealias Close = @Sendable (WorkspaceTab) async -> Void
    typealias Dispose = @Sendable (AnyObject) async -> Void
    typealias Reissue = @Sendable (String) async -> Void

    let hydrate: Hydrate
    let checkpoint: Checkpoint
    let close: Close
    let dispose: Dispose
    let reissue: Reissue

    init(hydrate: @escaping Hydrate = { _, _ in },
         checkpoint: @escaping Checkpoint = { _ in },
         close: @escaping Close = { _ in },
         dispose: @escaping Dispose = { _ in },
         reissue: @escaping Reissue = { _ in }) {
        self.hydrate = hydrate
        self.checkpoint = checkpoint
        self.close = close
        self.dispose = dispose
        self.reissue = reissue
    }
}

@MainActor
protocol WorkspaceContentAdapter: AnyObject {
    var kind: WorkspaceContentKind { get }
    func prepare(request: ContentRequest, worktree: Worktree) async throws -> PreparedContent
    func hydrate(tab: WorkspaceTab, worktree: Worktree) async
    func makeHost(tab: WorkspaceTab, worktree: Worktree) -> WorkspaceContentHost
    func checkpoint(tab: WorkspaceTab) async
    func retry(tab: WorkspaceTab, worktree: Worktree) async -> ResourceGenerationID
    func close(tab: WorkspaceTab) async
    func dispose(prepared: PreparedContent) async
}

@MainActor
class AdapterRuntimeToken: NSObject {
    let tabID: WorkspaceTabID
    let contentID: String
    var command: String?
    var generationID: ResourceGenerationID
    var phase: ContentPhase = .dormant
    var released = false
    var disposed = false
    var hydrated = false

    init(tabID: WorkspaceTabID, contentID: String, generationID: ResourceGenerationID,
         command: String? = nil) {
        self.tabID = tabID
        self.contentID = contentID
        self.command = command
        self.generationID = generationID
    }
}
