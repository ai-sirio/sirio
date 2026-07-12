import Testing
import Foundation
@testable import TillerGit

private func makeTempDir() throws -> URL {
    let url = FileManager.default.temporaryDirectory
        .appendingPathComponent("tiller-detect-\(UUID().uuidString)")
    try FileManager.default.createDirectory(at: url, withIntermediateDirectories: true)
    return url
}

@Test func emptyFolderIsNotGit() throws {
    let dir = try makeTempDir()
    defer { try? FileManager.default.removeItem(at: dir) }
    #expect(GitRepoDetection.isGitRepository(path: dir.path) == false)
}

@Test func folderWithGitDirectoryIsGit() throws {
    let dir = try makeTempDir()
    defer { try? FileManager.default.removeItem(at: dir) }
    try FileManager.default.createDirectory(
        at: dir.appendingPathComponent(".git"), withIntermediateDirectories: true
    )
    #expect(GitRepoDetection.isGitRepository(path: dir.path))
}

@Test func folderWithGitFileIsGit() throws {
    // Linked worktrees and submodules have a `.git` *file* pointing at the real gitdir.
    let dir = try makeTempDir()
    defer { try? FileManager.default.removeItem(at: dir) }
    try "gitdir: /elsewhere/.git/worktrees/x"
        .write(to: dir.appendingPathComponent(".git"), atomically: true, encoding: .utf8)
    #expect(GitRepoDetection.isGitRepository(path: dir.path))
}

@Test func missingFolderIsNotGit() {
    #expect(GitRepoDetection.isGitRepository(path: "/nonexistent/tiller-test-path") == false)
}
