import Testing

@testable import TillerGit

@Suite("DirectoryStatusAggregator")
struct DirectoryStatusAggregatorTests {
    private func entry(
        _ path: String, index: GitFileState? = nil,
        worktree: GitFileState? = nil, original: String? = nil
    ) throws -> GitStatusEntry {
        GitStatusEntry(
            path: try GitPath(path),
            originalPath: try original.map { try GitPath($0) },
            indexState: index, worktreeState: worktree)
    }

    private func statuses(_ entries: [GitStatusEntry]) -> [String: DirectoryGitStatus] {
        DirectoryStatusAggregator.directoryStatuses(
            from: Dictionary(entries.map { ($0.path.value, $0) },
                             uniquingKeysWith: { first, _ in first }))
    }

    @Test func nestedModifiedFileMarksAllAncestorsChanged() throws {
        let result = statuses([try entry("a/b/c.swift", worktree: .modified)])
        #expect(result == ["a": .changed, "a/b": .changed])
    }

    @Test func rootLevelFileContributesNoDirectories() throws {
        let result = statuses([try entry("README.md", worktree: .modified)])
        #expect(result.isEmpty)
    }

    @Test func untrackedDoesNotDowngradeChanged() throws {
        let result = statuses([
            try entry("a/x.swift", worktree: .modified),
            try entry("a/y.swift", worktree: .untracked),
        ])
        #expect(result == ["a": .changed])
    }

    @Test func conflictedWinsOverChangedAndUntracked() throws {
        let result = statuses([
            try entry("a/x.swift", worktree: .modified),
            try entry("a/y.swift", index: .unmerged, worktree: .unmerged),
        ])
        #expect(result == ["a": .conflicted])
    }

    @Test func untrackedOnlyDirectoryIsUntracked() throws {
        let result = statuses([try entry("new/dir/file.txt", worktree: .untracked)])
        #expect(result == ["new": .untracked, "new/dir": .untracked])
    }

    @Test func renameContributesBothPathsAncestors() throws {
        let result = statuses([
            try entry("dst/moved.swift", index: .renamed, original: "src/moved.swift")
        ])
        #expect(result == ["dst": .changed, "src": .changed])
    }

    @Test func stagedAddedFileIsChanged() throws {
        let result = statuses([try entry("a/new.swift", index: .added)])
        #expect(result == ["a": .changed])
    }

    @Test func emptyInputYieldsEmptyOutput() {
        #expect(DirectoryStatusAggregator.directoryStatuses(from: [:]).isEmpty)
    }
}
