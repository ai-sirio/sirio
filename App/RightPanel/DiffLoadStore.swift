import Foundation
import Observation
import SwiftUI
import TillerGit

enum DiffLoadState: Equatable {
    case idle
    case loading
    case loaded(GitFileDiff)
    case failed(String)
}

/// Per-file expansion and diff loading for the changes list.
///
/// Kept out of `RightPanelModel` on purpose: the model is `@Observable`, so a
/// diff map living there would invalidate every row in the list each time one
/// file finished loading.
@MainActor @Observable
final class DiffLoadStore {
    typealias Loader = @Sendable (GitStatusEntry, String) async throws -> GitFileDiff

    private(set) var expanded: Set<GitPath> = []
    private(set) var states: [GitPath: DiffLoadState] = [:]

    @ObservationIgnored private var inFlight: Set<GitPath> = []
    @ObservationIgnored private let loader: Loader

    init(loader: @escaping Loader) {
        self.loader = loader
    }

    func isExpanded(_ path: GitPath) -> Bool { expanded.contains(path) }

    func state(for path: GitPath) -> DiffLoadState { states[path] ?? .idle }

    func toggle(_ entry: GitStatusEntry, repoPath: String) async {
        if expanded.contains(entry.path) {
            collapse(entry.path)
        } else {
            await expand(entry, repoPath: repoPath)
        }
    }

    func expand(_ entry: GitStatusEntry, repoPath: String) async {
        withAnimation(.easeOut(duration: 0.18)) {
            _ = expanded.insert(entry.path)
        }
        guard state(for: entry.path) == .idle else { return }
        await load(entry, repoPath: repoPath)
    }

    /// Collapsing keeps whatever was loaded: re-expanding is then instant, and
    /// a stale diff is impossible because every refresh reloads what is open.
    func collapse(_ path: GitPath) {
        withAnimation(.easeOut(duration: 0.18)) {
            expanded.remove(path)
        }
    }

    func retry(_ entry: GitStatusEntry, repoPath: String) async {
        states[entry.path] = .idle
        await load(entry, repoPath: repoPath)
    }

    func prune(to paths: Set<GitPath>) {
        expanded = expanded.intersection(paths)
        states = states.filter { paths.contains($0.key) }
    }

    func reloadExpanded(entries: [GitStatusEntry], repoPath: String) async {
        for entry in entries where expanded.contains(entry.path) {
            await load(entry, repoPath: repoPath, preservingLoaded: true)
        }
    }

    func reset() {
        expanded = []
        states = [:]
        inFlight = []
    }

    private func load(
        _ entry: GitStatusEntry,
        repoPath: String,
        preservingLoaded: Bool = false
    ) async {
        guard inFlight.insert(entry.path).inserted else { return }
        defer { inFlight.remove(entry.path) }

        let expectedState: DiffLoadState
        if preservingLoaded, case let .loaded(previousDiff) = states[entry.path] {
            expectedState = .loaded(previousDiff)
        } else {
            states[entry.path] = .loading
            expectedState = .loading
        }

        do {
            let diff = try await loader(entry, repoPath)
            guard states[entry.path] == expectedState else { return }
            states[entry.path] = .loaded(diff)
        } catch {
            guard states[entry.path] == expectedState else { return }
            if case .loaded = expectedState {
                states[entry.path] = expectedState
            } else {
                states[entry.path] = .failed(error.localizedDescription)
            }
        }
    }
}
