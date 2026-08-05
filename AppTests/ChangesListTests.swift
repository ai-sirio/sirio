import Foundation
import Testing
import TillerCore

@testable import Tiller
@testable import TillerGit

@Suite("ChangesList")
@MainActor
struct ChangesListTests {
    @Test func savedDiffModeMigratesToStatus() {
        #expect(RightPanelMode.effective(rawValue: "diff", isGitRepository: true) == .status)
        #expect(RightPanelMode.effective(rawValue: "diff", isGitRepository: false) == .files)
        #expect(RightPanelMode.allCases.count == 2)
    }

    @Test func onlyGitlessModesAreOfferedOutsideARepository() {
        #expect(RightPanelMode.available(isGitRepository: false) == [.files])
        #expect(RightPanelMode.available(isGitRepository: true) == [.files, .status])
    }

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

    @Test func successfulReloadReplacesLoadedDiffAtomically() async throws {
        let entry = try makeEntry("a.swift")
        let oldDiff = GitFileDiff(
            path: entry.path, lines: [], additions: 1, deletions: 0,
            isBinary: false, isSubmodule: false, oldText: nil, newText: nil)
        let newDiff = GitFileDiff(
            path: entry.path, lines: [], additions: 7, deletions: 2,
            isBinary: false, isSubmodule: false, oldText: nil, newText: nil)
        let scripted = ControlledDiffLoader(results: [.success(oldDiff), .success(newDiff)])
        let store = DiffLoadStore(loader: { entry, repoPath in
            try await scripted.load(entry, repoPath: repoPath)
        })

        await store.expand(entry, repoPath: "/tmp")
        #expect(store.state(for: entry.path) == .loaded(oldDiff))

        let refresh = Task { @MainActor in
            await store.reloadExpanded(entries: [entry], repoPath: "/tmp")
        }
        await scripted.waitUntilBlocked(2)

        #expect(store.state(for: entry.path) == .loaded(oldDiff))

        await scripted.release(2)
        await refresh.value

        #expect(store.state(for: entry.path) == .loaded(newDiff))
    }

    @Test func failedReloadPreservesPreviouslyLoadedDiff() async throws {
        let entry = try makeEntry("a.swift")
        let oldDiff = GitFileDiff(
            path: entry.path, lines: [], additions: 1, deletions: 0,
            isBinary: false, isSubmodule: false, oldText: nil, newText: nil)
        let scripted = ControlledDiffLoader(
            results: [.success(oldDiff), .failure(.boom)])
        let store = DiffLoadStore(loader: { entry, repoPath in
            try await scripted.load(entry, repoPath: repoPath)
        })

        await store.expand(entry, repoPath: "/tmp")
        #expect(store.state(for: entry.path) == .loaded(oldDiff))

        let refresh = Task { @MainActor in
            await store.reloadExpanded(entries: [entry], repoPath: "/tmp")
        }
        await scripted.waitUntilBlocked(2)

        #expect(store.state(for: entry.path) == .loaded(oldDiff))

        await scripted.release(2)
        await refresh.value

        #expect(store.state(for: entry.path) == .loaded(oldDiff))
    }

    @Test func refreshDropsExpansionForFilesThatLeftTheStatus() async throws {
        let gone = try makeEntry("gone.swift")
        let snapshots = SnapshotSequence(values: [
            GitStatusSnapshot(entries: [gone]),
            GitStatusSnapshot(entries: [])
        ])
        let model = RightPanelModel(
            loaders: makeLoaders(status: { _ in await snapshots.next() }),
            monitoringEnabled: false)
        await model.activate(worktree: makeWorktree(), isGitRepository: true)
        await model.diffStore.expand(gone, repoPath: "/tmp")
        #expect(model.diffStore.isExpanded(gone.path))

        await model.refresh()

        #expect(!model.diffStore.isExpanded(gone.path))
    }

    @Test func refreshReloadsOnlyExpandedDiffs() async throws {
        let open = try makeEntry("open.swift")
        let closed = try makeEntry("closed.swift")
        let loadedPaths = LoadedPaths()
        let snapshot = GitStatusSnapshot(entries: [open, closed])
        let model = RightPanelModel(
            loaders: makeLoaders(
                status: { _ in snapshot },
                diff: { entry, _ in
                    await loadedPaths.record(entry.path.value)
                    return GitFileDiff(
                        path: entry.path, lines: [], additions: 0, deletions: 0,
                        isBinary: false, isSubmodule: false, oldText: nil, newText: nil)
                }),
            monitoringEnabled: false)
        await model.activate(worktree: makeWorktree(), isGitRepository: true)
        await model.diffStore.expand(open, repoPath: "/tmp")
        await loadedPaths.clear()

        await model.refresh()

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

enum ChangesListTestFailure: Error, Sendable { case boom }

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

actor ControlledDiffLoader {
    private let results: [Result<GitFileDiff, ChangesListTestFailure>]
    private var callCount = 0
    private var blockedCalls: Set<Int> = []
    private var releasedCalls: Set<Int> = []
    private var blockedWaiters: [Int: [CheckedContinuation<Void, Never>]] = [:]
    private var releaseWaiters: [Int: [CheckedContinuation<Void, Never>]] = [:]

    init(results: [Result<GitFileDiff, ChangesListTestFailure>]) {
        self.results = results
    }

    func load(_ entry: GitStatusEntry, repoPath: String) async throws -> GitFileDiff {
        callCount += 1
        let call = callCount
        let result = results[call - 1]

        guard call == 2 else { return try result.get() }

        blockedCalls.insert(call)
        let waiters = blockedWaiters.removeValue(forKey: call) ?? []
        waiters.forEach { $0.resume() }

        if !releasedCalls.contains(call) {
            await withCheckedContinuation { continuation in
                releaseWaiters[call, default: []].append(continuation)
            }
        }

        blockedCalls.remove(call)
        return try result.get()
    }

    func waitUntilBlocked(_ call: Int) async {
        guard !blockedCalls.contains(call) else { return }
        await withCheckedContinuation { continuation in
            blockedWaiters[call, default: []].append(continuation)
        }
    }

    func release(_ call: Int) {
        releasedCalls.insert(call)
        let waiters = releaseWaiters.removeValue(forKey: call) ?? []
        waiters.forEach { $0.resume() }
    }
}

actor SnapshotSequence {
    private var values: [GitStatusSnapshot]
    private var index = 0

    init(values: [GitStatusSnapshot]) { self.values = values }

    func next() -> GitStatusSnapshot {
        defer { index = min(index + 1, values.count - 1) }
        return values[min(index, values.count - 1)]
    }
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
