import Foundation
import TillerCore
@testable import Tiller

final class FakeWorkspacePersistence: WorkspaceLayoutPersistence, @unchecked Sendable {
    var restored: RestoredWorkspace?
    var structuralCommits: [(UUID, Int, WorkspaceSnapshot)] = []
    var checkpoints: [(UUID, Int, WorkspaceSnapshot)] = []
    var structuralError: Error?

    func restore(worktreeID: UUID) async -> RestoredWorkspace {
        restored ?? RestoredWorkspace(
            layout: .empty(), tabs: [:], revision: 0, diagnostics: [])
    }

    func commitStructural(worktreeID: UUID, revision: Int, snapshot: WorkspaceSnapshot,
                          tabs: [WorkspaceTab], terminalContents: [TerminalContentRecordValue]) async throws {
        if let structuralError { throw structuralError }
        structuralCommits.append((worktreeID, revision, snapshot))
    }

    func checkpoint(worktreeID: UUID, revision: Int, snapshot: WorkspaceSnapshot) async {
        checkpoints.append((worktreeID, revision, snapshot))
    }

    func flush(worktreeID: UUID) async throws {}
    func writeRecoverySidecar(worktreeID: UUID, revision: Int, snapshot: WorkspaceSnapshot) throws {}
    func purge(worktreeID: UUID) async throws {}
}
