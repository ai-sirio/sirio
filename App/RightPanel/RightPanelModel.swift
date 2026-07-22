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

    @ObservationIgnored private var generation = 0
    @ObservationIgnored private var monitor: FileSystemEventMonitor?
    @ObservationIgnored private var monitorTask: Task<Void, Never>?
    @ObservationIgnored private var debounceTask: Task<Void, Never>?
    @ObservationIgnored private var pendingPaths: Set<String> = []

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
        monitor?.stop()
        monitor = nil
        pendingPaths.removeAll()
        worktree = nil
        isGitRepository = false
        childrenByDirectory = [:]
        expandedDirectories = []
        directoryErrors = [:]
        status = .empty
        statusByPath = [:]
        directoryStatusByPath = [:]
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
        if childrenByDirectory[path] == nil { await loadDirectory(path, token: generation) }
    }

    func refresh() async {
        await refresh(changedPaths: [], token: generation, forceAllLoadedDirectories: true)
    }

    private func loadInitial(token: Int) async {
        guard let rootURL, let worktree else { return }
        filesLoading = true
        gitLoading = isGitRepository
        let fileTask = Task.detached(priority: .userInitiated) {
            try FileTreeLoader.children(at: "", rootURL: rootURL)
        }
        let statusTask = Task {
            isGitRepository ? try await GitStatus.load(in: worktree.path) : .empty
        }

        do {
            let nodes = try await fileTask.value
            guard token == generation else { return }
            childrenByDirectory[""] = nodes
            filesError = nil
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
        } catch {
            guard token == generation else { return }
            gitError = error.localizedDescription
        }
        gitLoading = false
    }

    private func loadDirectory(_ key: String, token: Int) async {
        guard let rootURL else { return }
        do {
            let nodes = try await Task.detached(priority: .userInitiated) {
                try FileTreeLoader.children(at: key, rootURL: rootURL)
            }.value
            guard token == generation else { return }
            childrenByDirectory[key] = nodes
            directoryErrors[key] = nil
        } catch {
            guard token == generation else { return }
            directoryErrors[key] = error.localizedDescription
        }
    }

    private func startMonitor(token: Int) async {
        guard let rootURL, let worktree else { return }
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

    private func scheduleRefresh(paths: [String], token: Int) {
        guard token == generation else { return }
        pendingPaths.formUnion(paths)
        debounceTask?.cancel()
        debounceTask = Task { [weak self] in
            try? await Task.sleep(for: .milliseconds(250))
            guard !Task.isCancelled, let self, token == self.generation else { return }
            let paths = Array(self.pendingPaths)
            self.pendingPaths.removeAll()
            await self.refresh(changedPaths: paths, token: token, forceAllLoadedDirectories: false)
        }
    }

    private func refresh(
        changedPaths: [String], token: Int, forceAllLoadedDirectories: Bool
    ) async {
        let sid = SignpostMetrics.makeSignpostID()
        let state = SignpostMetrics.beginInterval("panelRefresh", id: sid)
        defer { SignpostMetrics.endInterval("panelRefresh", state) }
        guard token == generation, let rootURL, let worktree else { return }
        let loadedKeys = childrenByDirectory.keys.filter { key in
            if forceAllLoadedDirectories || changedPaths.isEmpty { return true }
            let directory = key.isEmpty ? rootURL.path : rootURL.appendingPathComponent(key).path
            return changedPaths.contains { changed in
                changed == directory || changed.hasPrefix(directory + "/")
                    || directory.hasPrefix(changed + "/")
            }
        }
        for key in loadedKeys { await loadDirectory(key, token: token) }
        guard token == generation, isGitRepository else { return }
        do {
            let snapshot = try await GitStatus.load(in: worktree.path)
            guard token == generation else { return }
            apply(snapshot)
            gitError = nil
            if let selected = selectedEntry, diff != nil { await loadDiff(selected) }
        } catch {
            guard token == generation else { return }
            gitError = error.localizedDescription
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
        diffLoading = true
        defer { if token == generation { diffLoading = false } }
        do {
            let loaded = try await GitDiff.load(entry: entry, in: worktree.path)
            guard token == generation, selectedDiffPath == entry.path else { return }
            diff = loaded
            diffError = nil
        } catch {
            guard token == generation, selectedDiffPath == entry.path else { return }
            diff = nil
            diffError = error.localizedDescription
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
            await refresh(changedPaths: [], token: generation,
                          forceAllLoadedDirectories: true)
        } catch {
            gitError = error.localizedDescription
            await refresh(changedPaths: [], token: generation,
                          forceAllLoadedDirectories: true)
        }
    }
}
