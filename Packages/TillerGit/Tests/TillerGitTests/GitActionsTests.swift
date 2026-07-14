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

@Test func discardUntrackedTreatsMetacharacterPathAsLiteral() async throws {
    let repo = try makeGitTestRepository()
    defer { try? FileManager.default.removeItem(at: repo) }
    try "remove\n".write(
        to: repo.appendingPathComponent("*"), atomically: true, encoding: .utf8)
    try "keep\n".write(
        to: repo.appendingPathComponent("keep.txt"), atomically: true, encoding: .utf8)
    let snapshot = try await GitStatus.load(in: repo.path)
    let entry = try #require(snapshot.untracked.first { $0.path.value == "*" })

    try await GitActions.discardUntracked([entry], in: repo.path)

    #expect(!FileManager.default.fileExists(atPath: repo.appendingPathComponent("*").path))
    #expect(FileManager.default.fileExists(atPath: repo.appendingPathComponent("keep.txt").path))
}

@Test func actionRejectsEntryFromStaleSnapshot() async throws {
    let repo = try makeGitTestRepository()
    defer { try? FileManager.default.removeItem(at: repo) }
    let file = repo.appendingPathComponent("stale.txt")
    try "stale\n".write(to: file, atomically: true, encoding: .utf8)
    let snapshot = try await GitStatus.load(in: repo.path)
    let entry = try #require(snapshot.untracked.first { $0.path.value == "stale.txt" })
    try runGitForTest(["add", "--", "stale.txt"], in: repo)

    await #expect(throws: GitActionError.invalidEntry(
        "Selected entries are not present in the current status.")) {
        try await GitActions.discardUntracked([entry], in: repo.path)
    }

    #expect(FileManager.default.fileExists(atPath: file.path))
    #expect(try await GitStatus.load(in: repo.path).staged.map(\.path.value) == ["stale.txt"])
}

@Test func actionsRejectConflictedEntriesBeforeMutation() async throws {
    let expectedError = GitActionError.invalidEntry(
        "Conflicted entries are not supported.")

    for action in 0..<4 {
        let repo = try makeGitTestRepository()
        defer { try? FileManager.default.removeItem(at: repo) }
        try runGitForTest(["checkout", "-b", "feature"], in: repo)
        try "feature\n".write(
            to: repo.appendingPathComponent("file.txt"), atomically: true, encoding: .utf8)
        try runGitForTest(["add", "--", "file.txt"], in: repo)
        try runGitForTest(["commit", "-m", "feature"], in: repo)
        try runGitForTest(["checkout", "main"], in: repo)
        try "main\n".write(
            to: repo.appendingPathComponent("file.txt"), atomically: true, encoding: .utf8)
        try runGitForTest(["add", "--", "file.txt"], in: repo)
        try runGitForTest(["commit", "-m", "main"], in: repo)
        _ = try? runGitForTest(["merge", "feature"], in: repo)

        let snapshot = try await GitStatus.load(in: repo.path)
        let conflict = try #require(snapshot.entries.first { $0.path.value == "file.txt" })
        #expect(conflict.isConflicted)

        await #expect(throws: expectedError) {
            switch action {
            case 0: try await GitActions.stage([conflict], in: repo.path)
            case 1: try await GitActions.unstage([conflict], in: repo.path)
            case 2: try await GitActions.discardChanges([conflict], in: repo.path)
            default: try await GitActions.discardUntracked([conflict], in: repo.path)
            }
        }

        #expect(try await GitStatus.load(in: repo.path).entries.first?.isConflicted == true)
    }
}
@Test func stageMultipleEntriesLeavesUnselectedEntryUnstaged() async throws {
    let repo = try makeGitTestRepository()
    defer { try? FileManager.default.removeItem(at: repo) }
    for (name, contents) in [("stage-a.txt", "a\n"), ("stage-b.txt", "b\n"),
                             ("keep.txt", "keep\n")] {
        try contents.write(
            to: repo.appendingPathComponent(name), atomically: true, encoding: .utf8)
    }
    let snapshot = try await GitStatus.load(in: repo.path)
    let selected = try #require(snapshot.untracked.filter {
        ["stage-a.txt", "stage-b.txt"].contains($0.path.value)
    }.count == 2
        ? snapshot.untracked.filter {
            ["stage-a.txt", "stage-b.txt"].contains($0.path.value)
        }
        : nil)

    try await GitActions.stage(selected, in: repo.path)

    let after = try await GitStatus.load(in: repo.path)
    #expect(Set(after.staged.map(\.path.value)) == ["stage-a.txt", "stage-b.txt"])
    #expect(after.untracked.map(\.path.value) == ["keep.txt"])
}
@Test func unstageMultipleEntriesIncludingRenameLeavesUnselectedStaged() async throws {
    let repo = try makeGitTestRepository()
    defer { try? FileManager.default.removeItem(at: repo) }
    try runGitForTest(["mv", "file.txt", "renamed.txt"], in: repo)
    for (name, contents) in [("ordinary.txt", "ordinary\n"), ("keep-staged.txt", "keep\n")] {
        try contents.write(
            to: repo.appendingPathComponent(name), atomically: true, encoding: .utf8)
    }
    try runGitForTest(["add", "--", "ordinary.txt", "keep-staged.txt"], in: repo)
    let snapshot = try await GitStatus.load(in: repo.path)
    let rename = try #require(snapshot.staged.first { $0.path.value == "renamed.txt" })
    let ordinary = try #require(snapshot.staged.first { $0.path.value == "ordinary.txt" })

    try await GitActions.unstage([rename, ordinary], in: repo.path)

    let after = try await GitStatus.load(in: repo.path)
    #expect(after.staged.map(\.path.value) == ["keep-staged.txt"])
    #expect(after.untracked.map(\.path.value).contains("renamed.txt"))
    #expect(after.untracked.map(\.path.value).contains("ordinary.txt"))
    #expect(try String(contentsOf: repo.appendingPathComponent("renamed.txt"),
                      encoding: .utf8) == "one\n")
}

@Test func discardChangesMultipleEntriesLeavesUnselectedChange() async throws {
    let repo = try makeGitTestRepository()
    defer { try? FileManager.default.removeItem(at: repo) }
    for (name, contents) in [("discard-a.txt", "a\n"), ("discard-b.txt", "b\n"),
                             ("keep.txt", "keep\n")] {
        try contents.write(
            to: repo.appendingPathComponent(name), atomically: true, encoding: .utf8)
    }
    try runGitForTest(["add", "--", "discard-a.txt", "discard-b.txt", "keep.txt"], in: repo)
    try runGitForTest(["commit", "-m", "tracked fixtures"], in: repo)
    for (name, contents) in [("discard-a.txt", "changed-a\n"),
                             ("discard-b.txt", "changed-b\n"), ("keep.txt", "changed-keep\n")] {
        try contents.write(
            to: repo.appendingPathComponent(name), atomically: true, encoding: .utf8)
    }
    let snapshot = try await GitStatus.load(in: repo.path)
    let selected = try #require(snapshot.changes.filter {
        ["discard-a.txt", "discard-b.txt"].contains($0.path.value)
    }.count == 2
        ? snapshot.changes.filter {
            ["discard-a.txt", "discard-b.txt"].contains($0.path.value)
        }
        : nil)

    try await GitActions.discardChanges(selected, in: repo.path)

    #expect(try String(contentsOf: repo.appendingPathComponent("discard-a.txt"),
                      encoding: .utf8) == "a\n")
    #expect(try String(contentsOf: repo.appendingPathComponent("discard-b.txt"),
                      encoding: .utf8) == "b\n")
    #expect(try String(contentsOf: repo.appendingPathComponent("keep.txt"),
                      encoding: .utf8) == "changed-keep\n")
    #expect((try await GitStatus.load(in: repo.path)).changes.map(\.path.value) == ["keep.txt"])
}

@Test func discardUntrackedMultipleEntriesLeavesUnselectedEntry() async throws {
    let repo = try makeGitTestRepository()
    defer { try? FileManager.default.removeItem(at: repo) }
    for (name, contents) in [("remove-a.txt", "a\n"), ("remove-b.txt", "b\n"),
                             ("keep.txt", "keep\n")] {
        try contents.write(
            to: repo.appendingPathComponent(name), atomically: true, encoding: .utf8)
    }
    let snapshot = try await GitStatus.load(in: repo.path)
    let selected = try #require(snapshot.untracked.filter {
        ["remove-a.txt", "remove-b.txt"].contains($0.path.value)
    }.count == 2
        ? snapshot.untracked.filter {
            ["remove-a.txt", "remove-b.txt"].contains($0.path.value)
        }
        : nil)

    try await GitActions.discardUntracked(selected, in: repo.path)

    #expect(!FileManager.default.fileExists(atPath: repo.appendingPathComponent("remove-a.txt").path))
    #expect(!FileManager.default.fileExists(atPath: repo.appendingPathComponent("remove-b.txt").path))
    #expect(FileManager.default.fileExists(atPath: repo.appendingPathComponent("keep.txt").path))
}
@Test func stageTreatsMetacharacterPathAsLiteral() async throws {
    let repo = try makeGitTestRepository()
    defer { try? FileManager.default.removeItem(at: repo) }
    try "literal\n".write(
        to: repo.appendingPathComponent("*"), atomically: true, encoding: .utf8)
    try "keep\n".write(
        to: repo.appendingPathComponent("keep.txt"), atomically: true, encoding: .utf8)
    let snapshot = try await GitStatus.load(in: repo.path)
    let entry = try #require(snapshot.untracked.first { $0.path.value == "*" })

    try await GitActions.stage([entry], in: repo.path)

    let after = try await GitStatus.load(in: repo.path)
    #expect(after.staged.map(\.path.value) == ["*"])
    #expect(after.untracked.map(\.path.value) == ["keep.txt"])
}

@Test func unstageTreatsMetacharacterPathAsLiteral() async throws {
    let repo = try makeGitTestRepository()
    defer { try? FileManager.default.removeItem(at: repo) }
    try "literal\n".write(
        to: repo.appendingPathComponent("*"), atomically: true, encoding: .utf8)
    try "keep\n".write(
        to: repo.appendingPathComponent("keep.txt"), atomically: true, encoding: .utf8)
    try runGitForTest(["add", "--", "*", "keep.txt"], in: repo)
    let snapshot = try await GitStatus.load(in: repo.path)
    let entry = try #require(snapshot.staged.first { $0.path.value == "*" })

    try await GitActions.unstage([entry], in: repo.path)

    let after = try await GitStatus.load(in: repo.path)
    #expect(after.untracked.map(\.path.value) == ["*"])
    #expect(after.staged.map(\.path.value) == ["keep.txt"])
}

@Test func discardChangesTreatsMetacharacterPathAsLiteral() async throws {
    let repo = try makeGitTestRepository()
    defer { try? FileManager.default.removeItem(at: repo) }
    for (name, contents) in [("*", "literal\n"), ("keep.txt", "keep\n")] {
        try contents.write(
            to: repo.appendingPathComponent(name), atomically: true, encoding: .utf8)
    }
    try runGitForTest(["add", "--", "*", "keep.txt"], in: repo)
    try runGitForTest(["commit", "-m", "literal fixtures"], in: repo)
    try "changed-literal\n".write(
        to: repo.appendingPathComponent("*"), atomically: true, encoding: .utf8)
    try "changed-keep\n".write(
        to: repo.appendingPathComponent("keep.txt"), atomically: true, encoding: .utf8)
    let snapshot = try await GitStatus.load(in: repo.path)
    let entry = try #require(snapshot.changes.first { $0.path.value == "*" })

    try await GitActions.discardChanges([entry], in: repo.path)

    #expect(try String(contentsOf: repo.appendingPathComponent("*"), encoding: .utf8)
            == "literal\n")
    #expect(try String(contentsOf: repo.appendingPathComponent("keep.txt"), encoding: .utf8)
            == "changed-keep\n")
}
@Test func stageRejectsEntryFromStaleSnapshot() async throws {
    let repo = try makeGitTestRepository()
    defer { try? FileManager.default.removeItem(at: repo) }
    let file = repo.appendingPathComponent("stale-stage.txt")
    try "stale\n".write(to: file, atomically: true, encoding: .utf8)
    let snapshot = try await GitStatus.load(in: repo.path)
    let entry = try #require(snapshot.untracked.first { $0.path.value == "stale-stage.txt" })
    try runGitForTest(["add", "--", "stale-stage.txt"], in: repo)

    await #expect(throws: GitActionError.invalidEntry(
        "Selected entries are not present in the current status.")) {
        try await GitActions.stage([entry], in: repo.path)
    }

    #expect(try await GitStatus.load(in: repo.path).staged.map(\.path.value)
            == ["stale-stage.txt"])
}

@Test func unstageRejectsEntryFromStaleSnapshot() async throws {
    let repo = try makeGitTestRepository()
    defer { try? FileManager.default.removeItem(at: repo) }
    let file = repo.appendingPathComponent("stale-unstage.txt")
    try "stale\n".write(to: file, atomically: true, encoding: .utf8)
    try runGitForTest(["add", "--", "stale-unstage.txt"], in: repo)
    let snapshot = try await GitStatus.load(in: repo.path)
    let entry = try #require(snapshot.staged.first { $0.path.value == "stale-unstage.txt" })
    try runGitForTest(["reset", "HEAD", "--", "stale-unstage.txt"], in: repo)

    await #expect(throws: GitActionError.invalidEntry(
        "Selected entries are not present in the current status.")) {
        try await GitActions.unstage([entry], in: repo.path)
    }

    #expect(try await GitStatus.load(in: repo.path).untracked.map(\.path.value)
            == ["stale-unstage.txt"])
}

@Test func discardChangesRejectsEntryFromStaleSnapshot() async throws {
    let repo = try makeGitTestRepository()
    defer { try? FileManager.default.removeItem(at: repo) }
    let file = repo.appendingPathComponent("stale-discard.txt")
    try "original\n".write(to: file, atomically: true, encoding: .utf8)
    try runGitForTest(["add", "--", "stale-discard.txt"], in: repo)
    try runGitForTest(["commit", "-m", "stale fixture"], in: repo)
    try "changed\n".write(to: file, atomically: true, encoding: .utf8)
    let snapshot = try await GitStatus.load(in: repo.path)
    let entry = try #require(snapshot.changes.first { $0.path.value == "stale-discard.txt" })
    try runGitForTest(["restore", "--worktree", "--", "stale-discard.txt"], in: repo)

    await #expect(throws: GitActionError.invalidEntry(
        "Selected entries are not present in the current status.")) {
        try await GitActions.discardChanges([entry], in: repo.path)
    }

    #expect(try String(contentsOf: file, encoding: .utf8) == "original\n")
    #expect((try await GitStatus.load(in: repo.path)).isClean)
}
