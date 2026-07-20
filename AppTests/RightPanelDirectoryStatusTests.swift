import Testing
import TillerGit

@testable import Tiller

@Suite("RightPanelDirectoryStatus")
@MainActor
struct RightPanelDirectoryStatusTests {
    @Test func applyPopulatesDirectoryStatuses() throws {
        let model = RightPanelModel()
        let entry = GitStatusEntry(
            path: try GitPath("a/b/c.swift"), originalPath: nil,
            indexState: nil, worktreeState: .modified)
        model.apply(GitStatusSnapshot(entries: [entry]))
        #expect(model.directoryStatusByPath == ["a": .changed, "a/b": .changed])
    }

    @Test func applyEmptySnapshotClearsDirectoryStatuses() throws {
        let model = RightPanelModel()
        let entry = GitStatusEntry(
            path: try GitPath("a/b.swift"), originalPath: nil,
            indexState: nil, worktreeState: .modified)
        model.apply(GitStatusSnapshot(entries: [entry]))
        model.apply(.empty)
        #expect(model.directoryStatusByPath.isEmpty)
    }
}
