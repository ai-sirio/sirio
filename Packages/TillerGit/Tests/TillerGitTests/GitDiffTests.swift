import Foundation
import Testing
@testable import TillerGit

@Test func unifiedParserTracksLineNumbersAndCounts() throws {
    let patch = """
    diff --git a/file.txt b/file.txt
    --- a/file.txt
    +++ b/file.txt
    @@ -1,2 +1,3 @@
     one
    -two
    +second
    +three
    """
    let diff = try GitDiff.parse(patch, path: GitPath("file.txt"))
    let changes = diff.lines.filter { $0.kind == .addition || $0.kind == .deletion }

    #expect(diff.additions == 2)
    #expect(diff.deletions == 1)
    #expect(changes[0].oldLineNumber == 2)
    #expect(changes[0].newLineNumber == nil)
    #expect(changes[1].oldLineNumber == nil)
    #expect(changes[1].newLineNumber == 2)
}

@Test func parserRecognizesBinaryAndSubmoduleSummary() throws {
    let binary = try GitDiff.parse(
        "Binary files a/image.png and b/image.png differ\n", path: GitPath("image.png"))
    let submodule = try GitDiff.parse(
        "@@ -1 +1 @@\n-Subproject commit aaa\n+Subproject commit bbb\n", path: GitPath("Vendor"))
    #expect(binary.isBinary)
    #expect(submodule.isSubmodule)
}

@Test func liveDiffCombinesStagedAndUnstagedChanges() async throws {
    let repo = try makeGitTestRepository()
    defer { try? FileManager.default.removeItem(at: repo) }
    try "two\n".write(to: repo.appendingPathComponent("file.txt"), atomically: true, encoding: .utf8)
    try runGitForTest(["add", "--", "file.txt"], in: repo)
    try "three\n".write(to: repo.appendingPathComponent("file.txt"), atomically: true, encoding: .utf8)
    let snapshot = try await GitStatus.load(in: repo.path)
    let entry = try #require(snapshot.entries.first)

    let diff = try await GitDiff.load(entry: entry, in: repo.path)

    #expect(diff.lines.contains { $0.kind == .deletion && $0.text == "one" })
    #expect(diff.lines.contains { $0.kind == .addition && $0.text == "three" })
    #expect(diff.oldText == "one\n")
    #expect(diff.newText == "three\n")
}

@Test func untrackedAndUnbornFilesDiffFromDevNull() async throws {
    let repo = try makeGitTestRepository(withCommit: false)
    defer { try? FileManager.default.removeItem(at: repo) }
    let fileContents = "new\n"
    try fileContents.write(to: repo.appendingPathComponent("new.txt"), atomically: true, encoding: .utf8)
    let snapshot = try await GitStatus.load(in: repo.path)
    let entry = try #require(snapshot.untracked.first)

    let diff = try await GitDiff.load(entry: entry, in: repo.path)

    #expect(diff.deletions == 0)
    #expect(diff.lines.contains { $0.kind == .addition && $0.text == "new" })
    #expect(diff.oldText == nil)
    #expect(diff.newText == fileContents)
}

@Test func deletedFileCarriesHeadSnapshotOnly() async throws {
    let repo = try makeGitTestRepository()
    defer { try? FileManager.default.removeItem(at: repo) }
    try FileManager.default.removeItem(at: repo.appendingPathComponent("file.txt"))
    let snapshot = try await GitStatus.load(in: repo.path)
    let entry = try #require(snapshot.entries.first { $0.path.value == "file.txt" })

    let diff = try await GitDiff.load(entry: entry, in: repo.path)

    #expect(diff.oldText == "one\n")
    #expect(diff.newText == nil)
}

@Test func parserTracksZeroLengthHunkAndBlankContentBoundaries() throws {
    let patch = """
    diff --git a/empty.txt b/empty.txt
    --- /dev/null
    +++ b/empty.txt
    @@ -0,0 +1,2 @@
    +first
    +
    """
    let diff = try GitDiff.parse(patch, path: GitPath("empty.txt"))
    let additions = diff.lines.filter { $0.kind == .addition }

    #expect(additions.map(\.newLineNumber) == [1, 2])
    #expect(additions.map(\.text) == ["first", ""])
    #expect(diff.additions == 2)
}

@Test func parserDoesNotTreatHeadersAsChanges() throws {
    let patch = """
    diff --git a/file.txt b/file.txt
    --- a/file.txt
    +++ b/file.txt
    @@ -1 +1 @@
    -old
    +new
    \\ No newline at end of file
    """
    let diff = try GitDiff.parse(patch, path: GitPath("file.txt"))

    #expect(diff.lines.filter { $0.kind == .addition }.count == 1)
    #expect(diff.lines.filter { $0.kind == .deletion }.count == 1)
    #expect(diff.lines.last?.kind == .metadata)
}

@Test func parserPreservesChangeLinesThatStartLikeHeaders() throws {
    let patch = """
    --- a/file.txt
    +++ b/file.txt
    @@ -1,2 +1,2 @@
    ----old
    ++++new
    """
    let diff = try GitDiff.parse(patch, path: GitPath("file.txt"))
    let changes = diff.lines.filter { $0.kind == .addition || $0.kind == .deletion }

    #expect(changes.map(\.kind) == [.deletion, .addition])
    #expect(changes.map(\.text) == ["---old", "+++new"])
}

@Test func oversizedDiffFailsWithoutReturningPartialPatch() async throws {
    let repo = try makeGitTestRepository()
    defer { try? FileManager.default.removeItem(at: repo) }
    let content = String(repeating: "line\n", count: 21_000)
    try content.write(to: repo.appendingPathComponent("file.txt"), atomically: true, encoding: .utf8)

    let snapshot = try await GitStatus.load(in: repo.path)
    let entry = try #require(snapshot.entries.first)

    await #expect(throws: GitError.outputTooLarge(
        maxBytes: GitDiff.outputLimits.maxBytes,
        maxLines: GitDiff.outputLimits.maxLines)) {
        _ = try await GitDiff.load(entry: entry, in: repo.path)
    }
}

@Test func byteOversizedDiffFailsWithoutReturningPartialPatch() async throws {
    let repo = try makeGitTestRepository()
    defer { try? FileManager.default.removeItem(at: repo) }
    let content = String(repeating: "x", count: GitDiff.outputLimits.maxBytes + 1)
    try content.write(to: repo.appendingPathComponent("file.txt"), atomically: true, encoding: .utf8)

    let snapshot = try await GitStatus.load(in: repo.path)
    let entry = try #require(snapshot.entries.first)

    await #expect(throws: GitError.outputTooLarge(
        maxBytes: GitDiff.outputLimits.maxBytes,
        maxLines: GitDiff.outputLimits.maxLines)) {
        _ = try await GitDiff.load(entry: entry, in: repo.path)
    }
}
