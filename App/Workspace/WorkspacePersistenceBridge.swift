import Foundation
import TillerCore

final class WorkspacePersistenceBridge: WorkspaceLayoutPersistence, @unchecked Sendable {
    private let lock = NSLock()
    private var delegate: (any WorkspaceLayoutPersistence)?

    func install(_ delegate: any WorkspaceLayoutPersistence) {
        lock.lock()
        defer { lock.unlock() }
        self.delegate = delegate
    }

    func restore(worktreeID: UUID) async -> RestoredWorkspace {
        let delegate = currentDelegate()
        return await delegate?.restore(worktreeID: worktreeID)
            ?? RestoredWorkspace(layout: .empty(), tabs: [:], revision: 0, diagnostics: [])
    }

    func commitStructural(worktreeID: UUID, revision: Int, snapshot: WorkspaceSnapshot,
                          tabs: [WorkspaceTab],
                          terminalContents: [TerminalContentRecordValue]) async throws {
        guard let delegate = currentDelegate() else {
            throw WorkspacePersistenceError.invalidRecord
        }
        try await delegate.commitStructural(
            worktreeID: worktreeID, revision: revision, snapshot: snapshot,
            tabs: tabs, terminalContents: terminalContents)
    }

    func checkpoint(worktreeID: UUID, revision: Int, snapshot: WorkspaceSnapshot) async {
        await currentDelegate()?.checkpoint(
            worktreeID: worktreeID, revision: revision, snapshot: snapshot)
    }

    func flush(worktreeID: UUID) async throws {
        guard let delegate = currentDelegate() else {
            throw WorkspacePersistenceError.invalidRecord
        }
        try await delegate.flush(worktreeID: worktreeID)
    }

    func writeRecoverySidecar(worktreeID: UUID, revision: Int,
                              snapshot: WorkspaceSnapshot) throws {
        guard let delegate = currentDelegate() else {
            throw WorkspacePersistenceError.invalidRecord
        }
        try delegate.writeRecoverySidecar(
            worktreeID: worktreeID, revision: revision, snapshot: snapshot)
    }

    func purge(worktreeID: UUID) async throws {
        guard let delegate = currentDelegate() else {
            throw WorkspacePersistenceError.invalidRecord
        }
        try await delegate.purge(worktreeID: worktreeID)
    }

    private func currentDelegate() -> (any WorkspaceLayoutPersistence)? {
        lock.lock()
        defer { lock.unlock() }
        return delegate
    }
}
