import Foundation
import Testing
@testable import TillerGit

@Test func porcelainParserPreservesRenameAndSpecialPaths() throws {
    let raw = Data(" M changed file.txt\0R  new name.txt\0old name.txt\0?? line\nbreak.txt\0".utf8)
    let snapshot = try GitStatus.parse(raw)

    #expect(snapshot.entries.count == 3)
    #expect(snapshot.changes.map(\.path.value).contains("changed file.txt"))
    let renamed = try #require(snapshot.staged.first { $0.path.value == "new name.txt" })
    #expect(renamed.originalPath?.value == "old name.txt")
    #expect(snapshot.untracked.map(\.path.value).contains("line\nbreak.txt"))
}

@Test func porcelainParserClassifiesMixedStateInBothSections() throws {
    let snapshot = try GitStatus.parse(Data("MM file.txt\0UU conflict.txt\0".utf8))
    let mixed = try #require(snapshot.entries.first { $0.path.value == "file.txt" })
    #expect(snapshot.staged.contains(mixed))
    #expect(snapshot.changes.contains(mixed))
    #expect(snapshot.entries.first { $0.path.value == "conflict.txt" }?.isConflicted == true)
}

@Test func gitPathRejectsUnsafeValues() {
    #expect(throws: GitPathError.absolutePath("/tmp/x")) { _ = try GitPath("/tmp/x") }
    #expect(throws: GitPathError.parentTraversal("../x")) { _ = try GitPath("../x") }
    #expect(throws: GitPathError.nulByte) { _ = try GitPath("bad\0path") }
}

@Test func liveStatusIncludesStagedUnstagedAndUntracked() async throws {
    let repo = try makeGitTestRepository()
    defer { try? FileManager.default.removeItem(at: repo) }
    try "two\n".write(to: repo.appendingPathComponent("file.txt"), atomically: true, encoding: .utf8)
    try runGitForTest(["add", "--", "file.txt"], in: repo)
    try "three\n".write(to: repo.appendingPathComponent("file.txt"), atomically: true, encoding: .utf8)
    try "new\n".write(to: repo.appendingPathComponent("new.txt"), atomically: true, encoding: .utf8)

    let snapshot = try await GitStatus.load(in: repo.path)

    #expect(snapshot.staged.map(\.path.value) == ["file.txt"])
    #expect(snapshot.changes.map(\.path.value) == ["file.txt"])
    #expect(snapshot.untracked.map(\.path.value) == ["new.txt"])
    let gitDirectory = try await GitRepository.gitDirectory(in: repo.path)
    let hasHead = try await GitRepository.hasHead(in: repo.path)
    #expect(FileManager.default.fileExists(atPath: gitDirectory.path))
    #expect(hasHead)
}
