import Foundation
import Testing
import TillerCore

@testable import Tiller
@testable import TillerGit

@Suite("RightPanelDirectoryStatus")
@MainActor
struct RightPanelDirectoryStatusTests {
    @Test func applyPopulatesDirectoryStatuses() throws {
        let model = RightPanelModel()
        let entry = try makeEntry("a/b/c.swift")
        model.apply(GitStatusSnapshot(entries: [entry]))
        #expect(model.directoryStatusByPath == ["a": .changed, "a/b": .changed])
    }

    @Test func applyEmptySnapshotClearsDirectoryStatuses() throws {
        let model = RightPanelModel()
        let entry = try makeEntry("a/b.swift")
        model.apply(GitStatusSnapshot(entries: [entry]))
        model.apply(.empty)
        #expect(model.directoryStatusByPath.isEmpty)
    }

    @Test func adaptiveDebounceStartsAtBaseDelay() {
        let clock = TestClock()
        var debounce = AdaptiveDebounce(clock: { clock.now })

        #expect(debounce.recordEvent() == .milliseconds(250))
    }

    @Test func adaptiveDebounceGrowsDuringSustainedBurstAndCapsAtOneSecond() {
        let clock = TestClock()
        var debounce = AdaptiveDebounce(clock: { clock.now })

        #expect(debounce.recordEvent() == .milliseconds(250))
        clock.advance(by: 0.1)
        #expect(debounce.recordEvent() == .milliseconds(500))
        clock.advance(by: 0.1)
        #expect(debounce.recordEvent() == .milliseconds(750))
        clock.advance(by: 0.1)
        #expect(debounce.recordEvent() == .seconds(1))
        clock.advance(by: 0.1)
        #expect(debounce.recordEvent() == .seconds(1))
    }

    @Test func adaptiveDebounceResetsAfterSettle() {
        let clock = TestClock()
        var debounce = AdaptiveDebounce(clock: { clock.now })

        _ = debounce.recordEvent()
        clock.advance(by: 0.1)
        _ = debounce.recordEvent()
        clock.advance(by: 1.1)

        #expect(debounce.recordEvent() == .milliseconds(250))
    }

    @Test func refreshCoalescesBurstIntoOneFollowUpAndDeduplicatesPaths() async throws {
        let probe = RightPanelProbe(
            snapshots: [try snapshot("src/file.swift"), try snapshot("src/file.swift")])
        let model = makeModel(probe: probe)
        await model.activate(worktree: makeWorktree(), isGitRepository: true)
        await model.toggleDirectory("src")
        await model.toggleDirectory("docs")
        await probe.resetRecordedCalls()

        model.scheduleRefresh(paths: [absolute("src/file.swift")])
        await probe.waitForStatusCall(2)
        for _ in 0..<10 {
            model.scheduleRefresh(paths: [
                absolute("src/file.swift"), absolute("docs/readme.md")])
        }
        await probe.releaseStatusCall(2)
        await probe.waitForStatusCall(3)
        await probe.releaseStatusCall(3)
        try await Task.sleep(for: .milliseconds(300))

        let directoryCalls = await probe.directoryCalls
        #expect(await probe.statusCallCount == 3)
        #expect(directoryCalls.filter { $0 == "docs" }.count == 1)
    }

    @Test func refreshReloadsOnlyAffectedLoadedDirectories() async throws {
        let probe = RightPanelProbe(
            snapshots: [try snapshot("src/file.swift"), try snapshot("src/file.swift")])
        let model = makeModel(probe: probe)
        await model.activate(worktree: makeWorktree(), isGitRepository: true)
        await model.toggleDirectory("src")
        await model.toggleDirectory("docs")
        await probe.resetRecordedCalls()

        await model.refresh(
            changedPaths: [absolute("src/file.swift")],
            forceAllLoadedDirectories: false)

        #expect(await probe.directoryCalls == ["", "src"])
    }

    @Test func affectedDirectoriesLoadConcurrentlyBeforeOneSnapshotApply() async throws {
        let probe = RightPanelProbe(
            snapshots: [try snapshot("src/file.swift"), try snapshot("src/file.swift")])
        let model = makeModel(probe: probe)
        await model.activate(worktree: makeWorktree(), isGitRepository: true)
        await model.toggleDirectory("src")
        await model.toggleDirectory("docs")
        await probe.resetRecordedCalls()
        await probe.blockDirectoryCalls(["", "src", "docs"])

        let refreshTask = Task { @MainActor in
            await model.refresh(changedPaths: [], forceAllLoadedDirectories: true)
        }
        for key in ["", "src", "docs"] {
            await probe.waitForDirectoryCall(key)
        }
        #expect(Set(await probe.directoryCalls) == ["", "src", "docs"])
        await probe.releaseDirectoryCalls()
        await refreshTask.value
    }

    @Test func staleGenerationNeverAppliesDirectoryOrStatusResults() async throws {
        let probe = RightPanelProbe(
            snapshots: [try snapshot("src/file.swift"), try snapshot("new/file.swift")],
            blockingStatusCall: 2)
        let model = makeModel(probe: probe)
        await model.activate(worktree: makeWorktree(), isGitRepository: true)
        await probe.resetRecordedCalls()

        let refreshTask = Task { @MainActor in
            await model.refresh(
                changedPaths: [absolute("src/file.swift")],
                forceAllLoadedDirectories: false)
        }
        await probe.waitForStatusCall(2)
        model.deactivate()
        await probe.releaseStatusCall(2)
        await refreshTask.value

        #expect(model.status == .empty)
        #expect(model.childrenByDirectory.isEmpty)
    }

    @Test func deactivatedPanelIgnoresRefreshEvents() async throws {
        let probe = RightPanelProbe(snapshots: [try snapshot("src/file.swift")])
        let model = makeModel(probe: probe)
        model.deactivate()
        model.scheduleRefresh(paths: [absolute("src/file.swift")])
        try await Task.sleep(for: .milliseconds(300))

        #expect(await probe.statusCallCount == 0)
        #expect(await probe.directoryCalls.isEmpty)
    }

    @Test func switchingWorktreeInvalidatesOldRefreshLifecycle() async throws {
        let probe = RightPanelProbe(
            snapshots: [
                try snapshot("old/file.swift"),
                try snapshot("stale/file.swift"),
                try snapshot("new/file.swift"),
            ],
            blockingStatusCall: 2)
        let model = makeModel(probe: probe)
        let oldWorktree = makeWorktree()
        await model.activate(worktree: oldWorktree, isGitRepository: true)

        let refreshTask = Task { @MainActor in
            await model.refresh(
                changedPaths: [absolute("old/file.swift")],
                forceAllLoadedDirectories: false)
        }
        await probe.waitForStatusCall(2)
        let newWorktree = makeWorktree()
        await model.activate(worktree: newWorktree, isGitRepository: true)
        await probe.releaseStatusCall(2)
        await refreshTask.value

        #expect(model.worktree?.id == newWorktree.id)
        #expect(model.status.entries.first?.path.value == "new/file.swift")
    }

    @Test func unrelatedChangesDoNotReloadSelectedDiff() async throws {
        let selected = try makeEntry("src/file.swift")
        let probe = RightPanelProbe(
            snapshots: [GitStatusSnapshot(entries: [selected]), GitStatusSnapshot(entries: [selected])])
        let model = makeModel(probe: probe)
        await model.activate(worktree: makeWorktree(), isGitRepository: true)
        await model.selectDiff(selected)
        await probe.resetRecordedCalls()

        await model.refresh(
            changedPaths: [absolute("docs/readme.md")],
            forceAllLoadedDirectories: false)

        #expect(await probe.diffCallCount == 0)
    }

    @Test func selectedFileChangeReloadsItsDiff() async throws {
        let selected = try makeEntry("src/file.swift")
        let probe = RightPanelProbe(
            snapshots: [GitStatusSnapshot(entries: [selected]), GitStatusSnapshot(entries: [selected])])
        let model = makeModel(probe: probe)
        await model.activate(worktree: makeWorktree(), isGitRepository: true)
        await model.selectDiff(selected)
        await probe.resetRecordedCalls()

        await model.refresh(
            changedPaths: [absolute("src/file.swift")],
            forceAllLoadedDirectories: false)

        #expect(await probe.diffCallCount == 1)
    }

    @Test func selectedStatusChangeReloadsItsDiffEvenForUnrelatedPath() async throws {
        let initial = try makeEntry("src/file.swift", state: .modified)
        let updated = try makeEntry("src/file.swift", state: .added)
        let probe = RightPanelProbe(
            snapshots: [GitStatusSnapshot(entries: [initial]), GitStatusSnapshot(entries: [updated])])
        let model = makeModel(probe: probe)
        await model.activate(worktree: makeWorktree(), isGitRepository: true)
        await model.selectDiff(initial)
        await probe.resetRecordedCalls()

        await model.refresh(
            changedPaths: [absolute("docs/readme.md")],
            forceAllLoadedDirectories: false)

        #expect(await probe.diffCallCount == 1)
    }

    @Test func changingSelectedFileDuringRefreshReloadsNewDiff() async throws {
        let first = try makeEntry("src/first.swift")
        let second = try makeEntry("src/second.swift")
        let probe = RightPanelProbe(
            snapshots: [GitStatusSnapshot(entries: [first, second]), GitStatusSnapshot(entries: [first, second])],
            blockingStatusCall: 2)
        let model = makeModel(probe: probe)
        await model.activate(worktree: makeWorktree(), isGitRepository: true)
        await model.selectDiff(first)
        await probe.resetRecordedCalls()

        let refreshTask = Task { @MainActor in
            await model.refresh(
                changedPaths: [absolute("docs/readme.md")],
                forceAllLoadedDirectories: false)
        }
        await probe.waitForStatusCall(2)
        await model.selectDiff(second)
        await probe.releaseStatusCall(2)
        await refreshTask.value

        #expect(await probe.diffCallCount == 2)
        #expect(model.selectedDiffPath == second.path)
    }

    private func makeModel(probe: RightPanelProbe) -> RightPanelModel {
        RightPanelModel(
            loaders: .init(
                directory: { key, _ in await probe.loadDirectory(key) },
                status: { _ in await probe.loadStatus() },
                diff: { entry, _ in try await probe.loadDiff(entry) },
                stats: { _, _ in [:] }),
            monitoringEnabled: false)
    }

    private func makeWorktree() -> Worktree {
        Worktree(
            id: UUID(), projectId: UUID(), branch: "main",
            path: "/tmp/right-panel-phase6", isPrimary: true)
    }

    private func snapshot(_ path: String) throws -> GitStatusSnapshot {
        GitStatusSnapshot(entries: [try makeEntry(path)])
    }

    private func makeEntry(
        _ path: String, state: GitFileState = .modified
    ) throws -> GitStatusEntry {
        GitStatusEntry(
            path: try GitPath(path), originalPath: nil,
            indexState: nil, worktreeState: state)
    }

    private func absolute(_ path: String) -> String {
        "/tmp/right-panel-phase6/\(path)"
    }
}

private final class TestClock: @unchecked Sendable {
    var now = Date(timeIntervalSince1970: 0)

    func advance(by seconds: TimeInterval) {
        now.addTimeInterval(seconds)
    }
}

private actor RightPanelProbe {
    private let snapshots: [GitStatusSnapshot]
    private let blockingStatusCall: Int?
    private var blockedContinuations: [Int: CheckedContinuation<Void, Never>] = [:]
    private var statusWaiters: [Int: CheckedContinuation<Void, Never>] = [:]
    private var blockedDirectoryKeys: Set<String> = []
    private var blockedDirectoryContinuations: [String: CheckedContinuation<Void, Never>] = [:]
    private var directoryWaiters: [String: CheckedContinuation<Void, Never>] = [:]
    private(set) var statusCallCount = 0
    private(set) var directoryCalls: [String] = []
    private(set) var diffCallCount = 0

    init(snapshots: [GitStatusSnapshot], blockingStatusCall: Int? = nil) {
        self.snapshots = snapshots
        self.blockingStatusCall = blockingStatusCall
    }

    func loadDirectory(_ key: String) async -> [FileTreeNode] {
        directoryCalls.append(key)
        directoryWaiters[key]?.resume()
        directoryWaiters[key] = nil
        if blockedDirectoryKeys.contains(key) {
            await withCheckedContinuation { continuation in
                blockedDirectoryContinuations[key] = continuation
            }
        }
        return [
            FileTreeNode(relativePath: "\(key.isEmpty ? "src" : key + "/file.swift")",
                         name: key.isEmpty ? "src" : "file.swift",
                         kind: key.isEmpty ? .directory : .file),
        ]
    }

    func loadStatus() async -> GitStatusSnapshot {
        statusCallCount += 1
        let call = statusCallCount
        statusWaiters[call]?.resume()
        statusWaiters[call] = nil
        if call == blockingStatusCall {
            await withCheckedContinuation { continuation in
                blockedContinuations[call] = continuation
            }
        }
        return snapshots[min(call - 1, snapshots.count - 1)]
    }

    func loadDiff(_ entry: GitStatusEntry) throws -> GitFileDiff {
        diffCallCount += 1
        return try GitDiff.parse("", path: entry.path)
    }

    func waitForDirectoryCall(_ key: String) async {
        guard !directoryCalls.contains(key) else { return }
        await withCheckedContinuation { continuation in
            directoryWaiters[key] = continuation
        }
    }

    func blockDirectoryCalls(_ keys: Set<String>) {
        blockedDirectoryKeys = keys
    }

    func releaseDirectoryCalls() {
        for key in blockedDirectoryKeys {
            blockedDirectoryContinuations[key]?.resume()
            blockedDirectoryContinuations[key] = nil
        }
        blockedDirectoryKeys.removeAll()
    }

    func waitForStatusCall(_ call: Int) async {
        guard statusCallCount < call else { return }
        await withCheckedContinuation { continuation in
            statusWaiters[call] = continuation
        }
    }

    func releaseStatusCall(_ call: Int) {
        blockedContinuations[call]?.resume()
        blockedContinuations[call] = nil
    }

    func resetRecordedCalls() {
        directoryCalls = []
        diffCallCount = 0
    }
}
