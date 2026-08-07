import Foundation
import CryptoKit
import TillerCore

struct TerminalContentRecordValue: Sendable, Equatable {
    let id: TerminalContentID
    let worktreeID: UUID
    let launchKind: LaunchKind
    let commandJSON: String?

    enum LaunchKind: Equatable, Sendable {
        case shell
        case agent(id: String)
    }
}

struct BrowserContentRecordValue: Sendable, Equatable {
    let id: BrowserContentID
    let worktreeID: UUID
    let url: String
    let title: String?
}

struct RestoredWorkspace: Sendable {
    let layout: WorkspaceLayout
    let tabs: [WorkspaceTabID: WorkspaceTab]
    let browserContents: [BrowserContentID: BrowserContentRecordValue]
    let revision: Int
    let diagnostics: [RestoreDiagnostic]

    init(layout: WorkspaceLayout, tabs: [WorkspaceTabID: WorkspaceTab],
         browserContents: [BrowserContentID: BrowserContentRecordValue] = [:],
         revision: Int, diagnostics: [RestoreDiagnostic]) {
        self.layout = layout
        self.tabs = tabs
        self.browserContents = browserContents
        self.revision = revision
        self.diagnostics = diagnostics
    }
}

enum RestoreDiagnostic: Hashable, Sendable {
    case quarantinedSnapshot(reason: String)
    case removedMissingTab(WorkspaceTabID)
    case repairedSelection(PaneGroupID)
    case unavailableContent(WorkspaceTabID)
    case importedRecoverySidecar(revision: Int)
    case futureSchemaVersion(Int)
}

protocol WorkspaceLayoutPersistence: Sendable {
    func restore(worktreeID: UUID) async -> RestoredWorkspace
    func commitStructural(worktreeID: UUID, revision: Int,
                          snapshot: WorkspaceSnapshot,
                          tabs: [WorkspaceTab],
                          terminalContents: [TerminalContentRecordValue],
                          browserContents: [BrowserContentRecordValue]) async throws
    func checkpoint(worktreeID: UUID, revision: Int, snapshot: WorkspaceSnapshot) async
    func flush(worktreeID: UUID) async throws
    func writeRecoverySidecar(worktreeID: UUID, revision: Int, snapshot: WorkspaceSnapshot) throws
    func purge(worktreeID: UUID) async throws
}

enum WorkspacePersistenceError: Error, Equatable, Sendable {
    case invalidSnapshot
    case missingTab(WorkspaceTabID)
    case missingTerminalContent(TerminalContentID)
    case contentBelongsToAnotherWorktree(TerminalContentID)
    case missingBrowserContent(BrowserContentID)
    case browserContentBelongsToAnotherWorktree(BrowserContentID)
    case invalidRecord
}

enum SHA256Hex {
    static func digest(_ data: Data) -> String {
        return SHA256.hash(data: data).map { String(format: "%02x", $0) }.joined()
    }
}
