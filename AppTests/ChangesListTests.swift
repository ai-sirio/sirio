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

    @Test func expandingIdlePathLoadsItsDiff() async throws {
        let entry = try makeEntry("a.swift")
        let store = DiffLoadStore(loader: { entry, _ in
            GitFileDiff(path: entry.path, lines: [], additions: 1, deletions: 0,
                        isBinary: false, isSubmodule: false, oldText: nil, newText: nil)
        })

        #expect(store.state(for: entry.path) == .idle)
        await store.expand(entry, repoPath: "/tmp")

        #expect(store.isExpanded(entry.path))
        guard case .loaded(let diff) = store.state(for: entry.path) else {
            Issue.record("expected a loaded diff")
            return
        }
        #expect(diff.additions == 1)
    }

    @Test func concurrentExpansionsOfSamePathLoadOnce() async throws {
        let entry = try makeEntry("a.swift")
        let counter = LoadCounter()
        let store = DiffLoadStore(loader: { entry, _ in
            await counter.increment()
            try? await Task.sleep(for: .milliseconds(20))
            return GitFileDiff(path: entry.path, lines: [], additions: 0, deletions: 0,
                               isBinary: false, isSubmodule: false, oldText: nil, newText: nil)
        })

        async let first: Void = store.expand(entry, repoPath: "/tmp")
        async let second: Void = store.expand(entry, repoPath: "/tmp")
        _ = await (first, second)

        #expect(await counter.value == 1)
    }

    @Test func collapsingKeepsTheLoadedDiff() async throws {
        let entry = try makeEntry("a.swift")
        let store = DiffLoadStore(loader: { entry, _ in
            GitFileDiff(path: entry.path, lines: [], additions: 3, deletions: 0,
                        isBinary: false, isSubmodule: false, oldText: nil, newText: nil)
        })

        await store.expand(entry, repoPath: "/tmp")
        store.collapse(entry.path)

        #expect(!store.isExpanded(entry.path))
        guard case .loaded = store.state(for: entry.path) else {
            Issue.record("collapsing must not discard the loaded diff")
            return
        }
    }

    @Test func failedLoadIsReportedAndRetryable() async throws {
        let entry = try makeEntry("a.swift")
        let attempts = LoadCounter()
        let store = DiffLoadStore(loader: { entry, _ in
            let count = await attempts.increment()
            if count == 1 { throw ChangesListTestFailure.boom }
            return GitFileDiff(path: entry.path, lines: [], additions: 9, deletions: 0,
                               isBinary: false, isSubmodule: false, oldText: nil, newText: nil)
        })

        await store.expand(entry, repoPath: "/tmp")
        guard case .failed = store.state(for: entry.path) else {
            Issue.record("expected a failed state")
            return
        }

        await store.retry(entry, repoPath: "/tmp")
        guard case .loaded(let diff) = store.state(for: entry.path) else {
            Issue.record("expected retry to succeed")
            return
        }
        #expect(diff.additions == 9)
    }

    @Test func pruneDropsPathsThatLeftTheStatus() async throws {
        let kept = try makeEntry("kept.swift")
        let gone = try makeEntry("gone.swift")
        let store = DiffLoadStore(loader: { entry, _ in
            GitFileDiff(path: entry.path, lines: [], additions: 0, deletions: 0,
                        isBinary: false, isSubmodule: false, oldText: nil, newText: nil)
        })
        await store.expand(kept, repoPath: "/tmp")
        await store.expand(gone, repoPath: "/tmp")

        store.prune(to: [kept.path])

        #expect(store.isExpanded(kept.path))
        #expect(!store.isExpanded(gone.path))
        #expect(store.state(for: gone.path) == .idle)
    }

    @Test func reloadExpandedRefreshesOnlyExpandedPaths() async throws {
        let expanded = try makeEntry("open.swift")
        let collapsed = try makeEntry("closed.swift")
        let loadedPaths = LoadedPaths()
        let store = DiffLoadStore(loader: { entry, _ in
            await loadedPaths.record(entry.path.value)
            return GitFileDiff(path: entry.path, lines: [], additions: 0, deletions: 0,
                               isBinary: false, isSubmodule: false, oldText: nil, newText: nil)
        })
        await store.expand(expanded, repoPath: "/tmp")
        await loadedPaths.clear()

        await store.reloadExpanded(entries: [expanded, collapsed], repoPath: "/tmp")

        #expect(await loadedPaths.values == ["open.swift"])
    }

    @Test func diffBodySkipsMetadataLines() throws {
        let patch = """
        diff --git a/file.txt b/file.txt
        --- a/file.txt
        +++ b/file.txt
        @@ -1,2 +1,3 @@
         one
        -two
        +second
        """
        let diff = try GitDiff.parse(patch, path: try GitPath("file.txt"))

        #expect(FileDiffBody.renderableLines(of: diff).allSatisfy { $0.kind != .metadata })
        #expect(FileDiffBody.renderableLines(of: diff).contains { $0.kind == .hunk })
    }

    @Test func countLabelShowsSignedAdditionsAndDeletions() {
        #expect(ChangedFileCounts.label(
            for: GitDiffStat(additions: 24, deletions: 7, isBinary: false)) == "−7 +24")
        #expect(ChangedFileCounts.label(
            for: GitDiffStat(additions: 26, deletions: 0, isBinary: false)) == "+26")
        #expect(ChangedFileCounts.label(
            for: GitDiffStat(additions: 0, deletions: 3, isBinary: false)) == "−3")
    }

    @Test func countLabelReportsBinaryAndMissingStats() {
        #expect(ChangedFileCounts.label(
            for: GitDiffStat(additions: 0, deletions: 0, isBinary: true)) == "bin")
        #expect(ChangedFileCounts.label(for: nil) == nil)
        #expect(ChangedFileCounts.label(
            for: GitDiffStat(additions: 0, deletions: 0, isBinary: false)) == nil)
    }
}

enum ChangesListTestFailure: Error { case boom }

actor LoadCounter {
    private(set) var value = 0

    @discardableResult
    func increment() -> Int {
        value += 1
        return value
    }
}

actor LoadedPaths {
    private(set) var values: [String] = []

    func record(_ path: String) { values.append(path) }
    func clear() { values.removeAll() }
}

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
