import Foundation
import Testing
@testable import TillerGit

@Test func numstatParsesPlainRecords() throws {
    let output = "26\t0\tsrc/main/index.ts\0" + "7\t24\tsrc/App.svelte\0"
    let stats = GitDiff.parseNumstat(output)

    #expect(stats[try GitPath("src/main/index.ts")]
        == GitDiffStat(additions: 26, deletions: 0, isBinary: false))
    #expect(stats[try GitPath("src/App.svelte")]
        == GitDiffStat(additions: 7, deletions: 24, isBinary: false))
}

@Test func numstatParsesPathWithSpaces() throws {
    let stats = GitDiff.parseNumstat("3\t1\tdocs/my notes.md\0")

    #expect(stats[try GitPath("docs/my notes.md")]
        == GitDiffStat(additions: 3, deletions: 1, isBinary: false))
}

@Test func numstatAttributesRenameToDestinationPath() throws {
    // Rename record: the path field is empty, then <from>\0<to>\0 follow.
    let output = "1\t1\t\0old/name.swift\0new/name.swift\0"
    let stats = GitDiff.parseNumstat(output)

    #expect(stats[try GitPath("new/name.swift")]
        == GitDiffStat(additions: 1, deletions: 1, isBinary: false))
    #expect(stats[try GitPath("old/name.swift")] == nil)
}

@Test func numstatMarksBinaryFiles() throws {
    let stats = GitDiff.parseNumstat("-\t-\tassets/logo.png\0")

    #expect(stats[try GitPath("assets/logo.png")]
        == GitDiffStat(additions: 0, deletions: 0, isBinary: true))
}

@Test func numstatIgnoresEmptyAndMalformedOutput() {
    #expect(GitDiff.parseNumstat("").isEmpty)
    #expect(GitDiff.parseNumstat("garbage\0").isEmpty)
}

@Test func statsCoversTrackedAndUntrackedFiles() async throws {
    let repo = try makeGitTestRepository()
    defer { try? FileManager.default.removeItem(at: repo) }

    try "one\ntwo\n".write(
        to: repo.appendingPathComponent("file.txt"), atomically: true, encoding: .utf8)
    try "a\nb\nc\n".write(
        to: repo.appendingPathComponent("new.txt"), atomically: true, encoding: .utf8)

    let tracked = GitStatusEntry(
        path: try GitPath("file.txt"), originalPath: nil,
        indexState: nil, worktreeState: .modified)
    let untracked = GitStatusEntry(
        path: try GitPath("new.txt"), originalPath: nil,
        indexState: nil, worktreeState: .untracked)

    let stats = try await GitDiff.stats(entries: [tracked, untracked], in: repo.path)

    #expect(stats[try GitPath("file.txt")]?.additions == 1)
    #expect(stats[try GitPath("file.txt")]?.deletions == 0)
    #expect(stats[try GitPath("new.txt")]
        == GitDiffStat(additions: 3, deletions: 0, isBinary: false))
}

@Test func statsOnRepositoryWithoutHeadCountsUntrackedOnly() async throws {
    let repo = try makeGitTestRepository(withCommit: false)
    defer { try? FileManager.default.removeItem(at: repo) }
    try "x\ny\n".write(
        to: repo.appendingPathComponent("fresh.txt"), atomically: true, encoding: .utf8)

    let entry = GitStatusEntry(
        path: try GitPath("fresh.txt"), originalPath: nil,
        indexState: nil, worktreeState: .untracked)
    let stats = try await GitDiff.stats(entries: [entry], in: repo.path)

    #expect(stats[try GitPath("fresh.txt")]?.additions == 2)
}
