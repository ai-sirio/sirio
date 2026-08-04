import Foundation
import Testing
import TillerCore

@testable import Tiller
@testable import TillerGit

@Suite("ChangesList")
@MainActor
struct ChangesListTests {
    @Test func refreshPopulatesDiffStats() async throws {
        let entry = try makeEntry("a.swift")
        let model = RightPanelModel(
            loaders: makeLoaders(
                status: { _ in GitStatusSnapshot(entries: [entry]) },
                stats: { _, _ in
                    [try GitPath("a.swift"):
                        GitDiffStat(additions: 5, deletions: 2, isBinary: false)]
                }),
            monitoringEnabled: false)

        await model.activate(worktree: makeWorktree(), isGitRepository: true)

        #expect(model.diffStats[try GitPath("a.swift")]?.additions == 5)
    }

    @Test func statsFailureLeavesListUsable() async throws {
        let entry = try makeEntry("a.swift")
        let model = RightPanelModel(
            loaders: makeLoaders(
                status: { _ in GitStatusSnapshot(entries: [entry]) },
                stats: { _, _ in throw ChangesListTestFailure.boom }),
            monitoringEnabled: false)

        await model.activate(worktree: makeWorktree(), isGitRepository: true)

        #expect(model.status.entries.count == 1)
        #expect(model.diffStats.isEmpty)
        #expect(model.gitError == nil)
    }
}

enum ChangesListTestFailure: Error { case boom }

@MainActor
func makeLoaders(
    status: @escaping RightPanelLoaders.StatusLoader,
    stats: @escaping RightPanelLoaders.StatsLoader = { _, _ in [:] },
    diff: @escaping RightPanelLoaders.DiffLoader = { entry, _ in
        GitFileDiff(path: entry.path, lines: [], additions: 0, deletions: 0,
                    isBinary: false, isSubmodule: false, oldText: nil, newText: nil)
    }
) -> RightPanelLoaders {
    RightPanelLoaders(
        directory: { _, _ in [] }, status: status, diff: diff, stats: stats)
}

func makeEntry(
    _ path: String, state: GitFileState = .modified
) throws -> GitStatusEntry {
    GitStatusEntry(
        path: try GitPath(path), originalPath: nil,
        indexState: nil, worktreeState: state)
}

func makeWorktree() -> Worktree {
    Worktree(
        id: UUID(), projectId: UUID(), branch: "main",
        path: FileManager.default.temporaryDirectory.path)
}
