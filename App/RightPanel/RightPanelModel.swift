import Foundation
import Observation
import SwiftUI
import TillerCore
import TillerGit
import TillerTerminal

struct FileExplorerRow: Identifiable, Equatable {
    let node: FileTreeNode
    let depth: Int
    var id: String { node.id }
}

struct AdaptiveDebounce: Sendable {
    static let baseDelay = Duration.milliseconds(250)
    static let maximumDelay = Duration.seconds(1)
    private static let burstWindow: TimeInterval = 0.5

    private let clock: @Sendable () -> Date
    private var lastEventAt: Date?
    private(set) var currentDelay = Self.baseDelay

    init(clock: @escaping @Sendable () -> Date = { Date() }) {
        self.clock = clock
    }

    mutating func recordEvent() -> Duration {
        let now = clock()
        if let lastEventAt,
           now.timeIntervalSince(lastEventAt) <= Self.burstWindow {
            currentDelay = min(currentDelay + .milliseconds(250), Self.maximumDelay)
        } else {
            currentDelay = Self.baseDelay
        }
        self.lastEventAt = now
        return currentDelay
    }

    mutating func reset() {
        lastEventAt = nil
        currentDelay = Self.baseDelay
    }
}

struct RightPanelLoaders: Sendable {
    typealias DirectoryLoader = @Sendable (String, URL) async throws -> [FileTreeNode]
    typealias StatusLoader = @Sendable (String) async throws -> GitStatusSnapshot
    typealias DiffLoader = @Sendable (GitStatusEntry, String) async throws -> GitFileDiff
    typealias StatsLoader =
        @Sendable ([GitStatusEntry], String) async throws -> [GitPath: GitDiffStat]

    let directory: DirectoryLoader
    let status: StatusLoader
    let diff: DiffLoader
    let stats: StatsLoader

    init(
        directory: @escaping DirectoryLoader,
        status: @escaping StatusLoader,
        diff: @escaping DiffLoader,
        stats: @escaping StatsLoader = { _, _ in [:] }
    ) {
        self.directory = directory
        self.status = status
        self.diff = diff
        self.stats = stats
    }

    static let live = Self(
        directory: { key, rootURL in
            try FileTreeLoader.children(at: key, rootURL: rootURL)
        },
        status: { path in
            try await GitStatus.load(in: path)
        },
        diff: { entry, path in
            try await GitDiff.load(entry: entry, in: path)
        },
        stats: { entries, path in
            try await GitDiff.stats(entries: entries, in: path)
        })
}

@MainActor @Observable
final class RightPanelModel {
    private(set) var worktree: Worktree?
    private(set) var isGitRepository = false
    private(set) var childrenByDirectory: [String: [FileTreeNode]] = [:]
    private(set) var expandedDirectories: Set<String> = []
    private(set) var directoryErrors: [String: String] = [:]
    private(set) var status = GitStatusSnapshot.empty
    private(set) var statusByPath: [String: GitStatusEntry] = [:]
    private(set) var directoryStatusByPath: [String: DirectoryGitStatus] = [:]
    private(set) var diffStats: [GitPath: GitDiffStat] = [:]
    private(set) var selectedDiffPath: GitPath?
    private(set) var diff: GitFileDiff?
    private(set) var filesLoading = false
    private(set) var gitLoading = false
    private(set) var diffLoading = false
    private(set) var mutationInProgress = false
    var filesError: String?
    var gitError: String?
    var diffError: String?
    var monitorError: String?

    private let loaders: RightPanelLoaders
    private let monitoringEnabled: Bool
    @ObservationIgnored private var refreshTask: Task<Void, Never>?
    @ObservationIgnored private var refreshInFlight = false
    @ObservationIgnored private var pendingForceAllLoadedDirectories = false
    @ObservationIgnored private var adaptiveDebounce = AdaptiveDebounce()

    init(
        loaders: RightPanelLoaders = .live,
        monitoringEnabled: Bool = true
    ) {
        self.loaders = loaders
        self.monitoringEnabled = monitoringEnabled
    }

    @ObservationIgnored private var generation = 0
    @ObservationIgnored private var monitor: FileSystemEventMonitor?
    @ObservationIgnored private var monitorTask: Task<Void, Never>?
    @ObservationIgnored private var debounceTask: Task<Void, Never>?
    @ObservationIgnored private var pendingPaths: Set<String> = []
    @ObservationIgnored private var prefetchingPaths: Set<String> = []

    var rootURL: URL? {
        worktree.map { URL(fileURLWithPath: $0.path, isDirectory: true).standardizedFileURL }
    }

    var visibleRows: [FileExplorerRow] {
        var rows: [FileExplorerRow] = []
        func append(_ key: String, depth: Int) {
            for node in childrenByDirectory[key] ?? [] {
                rows.append(FileExplorerRow(node: node, depth: depth))
                if node.kind.isDirectory, expandedDirectories.contains(node.relativePath) {
                    append(node.relativePath, depth: depth + 1)
                }
            }
        }
        append("", depth: 0)
        return rows
    }

    func activate(worktree: Worktree?, isGitRepository: Bool) async {
        guard self.worktree?.id != worktree?.id || self.isGitRepository != isGitRepository else {
            return
        }
        deactivate()
        guard let worktree else { return }
        self.worktree = worktree
        self.isGitRepository = isGitRepository
        let token = generation
        await loadInitial(token: token)
        guard token == generation else { return }
        await startMonitor(token: token)
    }

    func deactivate() {
        generation += 1
        monitorTask?.cancel()
        monitorTask = nil
        debounceTask?.cancel()
        debounceTask = nil
        refreshTask?.cancel()
        refreshTask = nil
        refreshInFlight = false
        pendingForceAllLoadedDirectories = false
        adaptiveDebounce.reset()
        monitor?.stop()
        monitor = nil
        pendingPaths.removeAll()
        prefetchingPaths.removeAll()
        worktree = nil
        isGitRepository = false
        childrenByDirectory = [:]
        expandedDirectories = []
        directoryErrors = [:]
        status = .empty
        statusByPath = [:]
        directoryStatusByPath = [:]
        diffStats = [:]
        selectedDiffPath = nil
        diff = nil
        filesLoading = false
        gitLoading = false
        diffLoading = false
        mutationInProgress = false
        filesError = nil
        gitError = nil
        diffError = nil
        monitorError = nil
    }
}

extension RightPanelModel {
    func toggleDirectory(_ path: String) async {
        let wasExpanded = withAnimation(.easeOut(duration: 0.18)) {
            expandedDirectories.remove(path) != nil
        }
        if wasExpanded { return }
        withAnimation(.easeOut(duration: 0.18)) {
            expandedDirectories.insert(path)
        }
        if childrenByDirectory[path] == nil {
            await loadDirectory(path, token: generation)
        }
        guard let rootURL else { return }
        prefetchChildren(of: path, rootURL: rootURL, token: generation)
    }

    func refresh() async {
        let token = generation
        let task = startRefresh(
            changedPaths: [], token: token, forceAllLoadedDirectories: true)
        await task?.value
        guard token == generation else { return }
    }

    func refresh(
        changedPaths: [String], forceAllLoadedDirectories: Bool
    ) async {
        let token = generation
        let task = startRefresh(
            changedPaths: changedPaths,
            token: token,
            forceAllLoadedDirectories: forceAllLoadedDirectories)
        await task?.value
        guard token == generation else { return }
    }

    private func loadInitial(token: Int) async {
        guard let rootURL, let worktree else { return }
        filesLoading = true
        gitLoading = isGitRepository
        let directoryLoader = loaders.directory
        let statusLoader = loaders.status
        let repositoryPath = worktree.path
        let shouldLoadStatus = isGitRepository
        let fileTask = Task.detached(priority: .userInitiated) {
            try await directoryLoader("", rootURL)
        }
        let statusTask = Task {
            shouldLoadStatus ? try await statusLoader(repositoryPath) : .empty
        }

        do {
            let nodes = try await fileTask.value
            guard token == generation else { return }
            childrenByDirectory[""] = nodes
            filesError = nil
            prefetchChildren(of: "", rootURL: rootURL, token: token)
        } catch {
            guard token == generation else { return }
            filesError = error.localizedDescription
        }
        filesLoading = false

        do {
            let snapshot = try await statusTask.value
            guard token == generation else { return }
            apply(snapshot)
            gitError = nil
            await loadStats(token: token)
        } catch {
            guard token == generation else { return }
            gitError = error.localizedDescription
        }
        gitLoading = false
    }

    private struct DirectoryLoadResult: Sendable {
        let key: String
        let nodes: [FileTreeNode]?
        let error: String?
    }

    private func loadDirectory(_ key: String, token: Int) async {
        guard let rootURL else { return }
        await loadDirectories([key], rootURL: rootURL, token: token)
    }

    private func prefetchChildren(of key: String, rootURL: URL, token: Int) {
        guard let nodes = childrenByDirectory[key] else { return }
        // One level of lookahead keeps clicks cache-hits without crawling the tree.
        let candidates = nodes.compactMap { node -> String? in
            guard node.kind.isDirectory,
                  childrenByDirectory[node.relativePath] == nil,
                  !prefetchingPaths.contains(node.relativePath) else {
                return nil
            }
            return node.relativePath
        }
        guard !candidates.isEmpty else { return }
        prefetchingPaths.formUnion(candidates)
        Task {
            await loadDirectories(candidates, rootURL: rootURL, token: token)
            prefetchingPaths.subtract(candidates)
        }
    }

    private func loadDirectories(
        _ keys: [String], rootURL: URL, token: Int
    ) async {
        let directoryLoader = loaders.directory
        let results = await withTaskGroup(of: DirectoryLoadResult.self) { group in
            for key in keys {
                group.addTask {
                    await Task.detached(priority: .userInitiated) {
                        do {
                            return DirectoryLoadResult(
                                key: key,
                                nodes: try await directoryLoader(key, rootURL),
                                error: nil)
                        } catch {
                            return DirectoryLoadResult(
                                key: key,
                                nodes: nil,
                                error: error.localizedDescription)
                        }
                    }.value
                }
            }
            var results: [DirectoryLoadResult] = []
            while let result = await group.next() {
                results.append(result)
            }
            return results
        }
        guard token == generation else { return }

        var nextChildren = childrenByDirectory
        var nextErrors = directoryErrors
        for result in results {
            if let nodes = result.nodes {
                nextChildren[result.key] = nodes
                nextErrors[result.key] = nil
            } else {
                nextErrors[result.key] = result.error
            }
        }
        childrenByDirectory = nextChildren
        directoryErrors = nextErrors
    }

    private func startMonitor(token: Int) async {
        guard monitoringEnabled, let rootURL, let worktree else { return }
        monitorError = nil
        var roots = [rootURL]
        if isGitRepository {
            do {
                roots.append(try await GitRepository.gitDirectory(in: worktree.path))
            } catch {
                monitorError = "Git metadata monitoring unavailable: \(error.localizedDescription)"
            }
        }
        guard token == generation else { return }
        guard let monitor = FileSystemEventMonitor(roots: Array(Set(roots))) else {
            monitorError = "Automatic filesystem refresh is unavailable."
            return
        }
        self.monitor = monitor
        monitorTask = Task { [weak self] in
            for await urls in monitor.events {
                guard !Task.isCancelled else { return }
                await self?.scheduleRefresh(paths: urls.map(\.path), token: token)
            }
        }
    }

    func scheduleRefresh(paths: [String]) {
        scheduleRefresh(paths: paths, token: generation)
    }

    private func scheduleRefresh(paths: [String], token: Int) {
        guard token == generation else { return }
        pendingPaths.formUnion(paths)
        let delay = adaptiveDebounce.recordEvent()
        debounceTask?.cancel()
        guard !refreshInFlight else { return }
        debounceTask = Task { [weak self] in
            do {
                try await Task.sleep(for: delay)
            } catch {
                return
            }
            guard !Task.isCancelled, let self, token == self.generation else { return }
            self.debounceTask = nil
            self.startPendingRefresh(token: token)
        }
    }

    private func startPendingRefresh(token: Int) {
        guard token == generation, !refreshInFlight else { return }
        guard !pendingPaths.isEmpty || pendingForceAllLoadedDirectories else { return }
        let paths = Array(pendingPaths)
        let forceAllLoadedDirectories = pendingForceAllLoadedDirectories
        pendingPaths.removeAll()
        pendingForceAllLoadedDirectories = false
        _ = startRefresh(
            changedPaths: paths,
            token: token,
            forceAllLoadedDirectories: forceAllLoadedDirectories)
    }

    private func startRefresh(
        changedPaths: [String], token: Int, forceAllLoadedDirectories: Bool
    ) -> Task<Void, Never>? {
        guard token == generation else { return nil }
        guard !refreshInFlight else {
            pendingPaths.formUnion(changedPaths)
            pendingForceAllLoadedDirectories =
                pendingForceAllLoadedDirectories || forceAllLoadedDirectories
            return nil
        }
        refreshInFlight = true
        let task = Task { [weak self] in
            guard let self else { return }
            await self.performRefresh(
                changedPaths: changedPaths,
                token: token,
                forceAllLoadedDirectories: forceAllLoadedDirectories)
            await self.finishRefresh(token: token)
        }
        refreshTask = task
        return task
    }

    private func finishRefresh(token: Int) {
        guard token == generation else { return }
        refreshInFlight = false
        refreshTask = nil
        if !pendingPaths.isEmpty || pendingForceAllLoadedDirectories {
            scheduleFollowUpRefresh(token: token)
        } else {
            adaptiveDebounce.reset()
        }
    }

    private func scheduleFollowUpRefresh(token: Int) {
        debounceTask?.cancel()
        let delay = adaptiveDebounce.currentDelay
        debounceTask = Task { [weak self] in
            do {
                try await Task.sleep(for: delay)
            } catch {
                return
            }
            guard !Task.isCancelled, let self, token == self.generation else { return }
            self.debounceTask = nil
            self.startPendingRefresh(token: token)
        }
    }

    private func performRefresh(
        changedPaths: [String], token: Int, forceAllLoadedDirectories: Bool
    ) async {
        let sid = SignpostMetrics.makeSignpostID()
        let state = SignpostMetrics.beginInterval("panelRefresh", id: sid)
        defer {
            SignpostMetrics.endInterval(
                "panelRefresh", state, message: "paths: \(changedPaths.count)")
        }
        guard token == generation, let rootURL, let worktree else { return }
        let selectedPathBefore = selectedDiffPath
        let selectedEntryBefore = selectedEntry
        let loadedKeys = childrenByDirectory.keys.filter { key in
            if forceAllLoadedDirectories || changedPaths.isEmpty { return true }
            let directory = key.isEmpty ? rootURL.path : rootURL.appendingPathComponent(key).path
            return changedPaths.contains { changed in
                changed == directory || changed.hasPrefix(directory + "/")
                    || directory.hasPrefix(changed + "/")
            }
        }.sorted()
        await loadDirectories(loadedKeys, rootURL: rootURL, token: token)
        guard token == generation else { return }
        guard isGitRepository else { return }
        do {
            let snapshot = try await loaders.status(worktree.path)
            guard token == generation else { return }
            apply(snapshot)
            gitError = nil
            await loadStats(token: token)
            guard token == generation else { return }
            let selectedChanged = selectedPathBefore != selectedDiffPath
                || selectedEntryBefore != selectedEntry
            let selectedAffected = selectedEntry.map {
                changedPathsAffecting($0.path, changedPaths: changedPaths, rootURL: rootURL)
            } ?? false
            if selectedChanged || selectedAffected, let selectedEntry {
                await loadDiff(selectedEntry)
                guard token == generation else { return }
            }
        } catch {
            guard token == generation else { return }
            gitError = error.localizedDescription
        }
    }

    private func changedPathsAffecting(
        _ path: GitPath, changedPaths: [String], rootURL: URL
    ) -> Bool {
        let selectedPath = rootURL.appendingPathComponent(path.value).path
        return changedPaths.contains { changed in
            let changedPath = changed.hasPrefix("/")
                ? URL(fileURLWithPath: changed).standardizedFileURL.path
                : rootURL.appendingPathComponent(changed).standardizedFileURL.path
            return selectedPath == changedPath
                || selectedPath.hasPrefix(changedPath + "/")
        }
    }
}

extension RightPanelModel {
    var allChangedEntries: [GitStatusEntry] { status.entries }

    var selectedEntry: GitStatusEntry? {
        guard let selectedDiffPath else { return nil }
        return status.entries.first { $0.path == selectedDiffPath }
    }

    func selectDiff(_ entry: GitStatusEntry) async {
        selectedDiffPath = entry.path
        await loadDiff(entry)
    }

    func ensureDiffLoaded() async {
        if let selectedEntry {
            if diff?.path != selectedEntry.path { await loadDiff(selectedEntry) }
        } else if let first = status.entries.first {
            await selectDiff(first)
        }
    }

    func stage(_ entries: [GitStatusEntry]) async {
        await mutate { try await GitActions.stage(entries, in: $0) }
    }

    func unstage(_ entries: [GitStatusEntry]) async {
        await mutate { try await GitActions.unstage(entries, in: $0) }
    }

    func discardChanges(_ entries: [GitStatusEntry]) async {
        await mutate { try await GitActions.discardChanges(entries, in: $0) }
    }

    func discardUntracked(_ entries: [GitStatusEntry]) async {
        await mutate { try await GitActions.discardUntracked(entries, in: $0) }
    }

    func apply(_ snapshot: GitStatusSnapshot) {
        status = snapshot
        statusByPath = Dictionary(
            snapshot.entries.map { ($0.path.value, $0) },
            uniquingKeysWith: { first, _ in first })
        directoryStatusByPath = DirectoryStatusAggregator.directoryStatuses(from: statusByPath)
        if let selectedDiffPath,
           snapshot.entries.contains(where: { $0.path == selectedDiffPath }) {
            return
        }
        selectedDiffPath = snapshot.entries.first?.path
        diff = nil
        diffError = nil
    }

    private func loadDiff(_ entry: GitStatusEntry) async {
        guard let worktree else { return }
        let token = generation
        let diffLoader = loaders.diff
        diffLoading = true
        defer { if token == generation { diffLoading = false } }
        do {
            let loaded = try await diffLoader(entry, worktree.path)
            guard token == generation, selectedDiffPath == entry.path else { return }
            diff = loaded
            diffError = nil
        } catch {
            guard token == generation, selectedDiffPath == entry.path else { return }
            diff = nil
            diffError = error.localizedDescription
        }
    }

    /// Counts are a nicety: a failure hides the numbers and leaves the list
    /// working, so it must never surface as `gitError`.
    private func loadStats(token: Int) async {
        guard let worktree else { return }
        let entries = status.entries
        guard !entries.isEmpty else {
            diffStats = [:]
            return
        }
        do {
            let loaded = try await loaders.stats(entries, worktree.path)
            guard token == generation else { return }
            diffStats = loaded
        } catch {
            guard token == generation else { return }
            diffStats = [:]
        }
    }

    private func mutate(
        _ operation: @escaping @Sendable (String) async throws -> Void
    ) async {
        guard !mutationInProgress, let worktree else { return }
        mutationInProgress = true
        defer { mutationInProgress = false }
        do {
            try await operation(worktree.path)
            gitError = nil
            await refresh(changedPaths: [], forceAllLoadedDirectories: true)
        } catch {
            gitError = error.localizedDescription
            await refresh(changedPaths: [], forceAllLoadedDirectories: true)
        }
    }
}
