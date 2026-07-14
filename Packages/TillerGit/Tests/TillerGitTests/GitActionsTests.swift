import Foundation
import Testing
@testable import TillerGit

@Test func discardChangesPreservesStagedContent() async throws {
    let repo = try makeGitTestRepository()
    defer { try? FileManager.default.removeItem(at: repo) }
    let file = repo.appendingPathComponent("file.txt")
    try "two\n".write(to: file, atomically: true, encoding: .utf8)
    try runGitForTest(["add", "--", "file.txt"], in: repo)
    try "three\n".write(to: file, atomically: true, encoding: .utf8)
    let beforeDiscard = try await GitStatus.load(in: repo.path)
    let entry = try #require(beforeDiscard.changes.first)

    try await GitActions.discardChanges([entry], in: repo.path)

    #expect(try String(contentsOf: file, encoding: .utf8) == "two\n")
    let afterDiscard = try await GitStatus.load(in: repo.path)
    #expect(afterDiscard.staged.map(\.path.value) == ["file.txt"])
}

@Test func stageAndUnstageLeaveWorkingFileIntact() async throws {
    let repo = try makeGitTestRepository()
    defer { try? FileManager.default.removeItem(at: repo) }
    let file = repo.appendingPathComponent("file.txt")
    try "two\n".write(to: file, atomically: true, encoding: .utf8)
    let beforeStage = try await GitStatus.load(in: repo.path)
    var entry = try #require(beforeStage.changes.first)
    try await GitActions.stage([entry], in: repo.path)
    let afterStage = try await GitStatus.load(in: repo.path)
    entry = try #require(afterStage.staged.first)

    try await GitActions.unstage([entry], in: repo.path)

    #expect(try String(contentsOf: file, encoding: .utf8) == "two\n")
    let afterUnstage = try await GitStatus.load(in: repo.path)
    #expect(afterUnstage.changes.map(\.path.value) == ["file.txt"])
}

@Test func unbornUnstageKeepsFileAndReturnsItToUntracked() async throws {
    let repo = try makeGitTestRepository(withCommit: false)
    defer { try? FileManager.default.removeItem(at: repo) }
    let file = repo.appendingPathComponent("new.txt")
    try "new\n".write(to: file, atomically: true, encoding: .utf8)
    try runGitForTest(["add", "--", "new.txt"], in: repo)
    let beforeUnstage = try await GitStatus.load(in: repo.path)
    let entry = try #require(beforeUnstage.staged.first)

    try await GitActions.unstage([entry], in: repo.path)

    #expect(FileManager.default.fileExists(atPath: file.path))
    let afterUnstage = try await GitStatus.load(in: repo.path)
    #expect(afterUnstage.untracked.map(\.path.value) == ["new.txt"])
}

@Test func discardUntrackedRemovesOnlySnapshotPaths() async throws {
    let repo = try makeGitTestRepository()
    defer { try? FileManager.default.removeItem(at: repo) }
    try "remove\n".write(to: repo.appendingPathComponent("remove.txt"), atomically: true, encoding: .utf8)
    try "keep\n".write(to: repo.appendingPathComponent("keep.txt"), atomically: true, encoding: .utf8)
    let snapshot = try await GitStatus.load(in: repo.path)
    let remove = try #require(snapshot.untracked.first { $0.path.value == "remove.txt" })

    try await GitActions.discardUntracked([remove], in: repo.path)

    #expect(!FileManager.default.fileExists(atPath: repo.appendingPathComponent("remove.txt").path))
    #expect(FileManager.default.fileExists(atPath: repo.appendingPathComponent("keep.txt").path))
}

@Test func unstageRenameUsesCurrentAndOriginalPaths() async throws {
    let repo = try makeGitTestRepository()
    defer { try? FileManager.default.removeItem(at: repo) }
    try runGitForTest(["mv", "file.txt", "renamed.txt"], in: repo)
    try runGitForTest(["add", "-A"], in: repo)
    let beforeUnstage = try await GitStatus.load(in: repo.path)
    let rename = try #require(beforeUnstage.staged.first {
        $0.path.value == "renamed.txt"
    })
    #expect(rename.originalPath?.value == "file.txt")

    try await GitActions.unstage([rename], in: repo.path)

    let afterUnstage = try await GitStatus.load(in: repo.path)
    #expect(afterUnstage.staged.isEmpty)
    #expect(afterUnstage.changes.map(\.path.value).contains("file.txt"))
    #expect(afterUnstage.untracked.map(\.path.value).contains("renamed.txt"))
}
