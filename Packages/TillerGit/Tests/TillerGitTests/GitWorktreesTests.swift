import Testing
import Foundation
@testable import TillerGit

/// Creates a throwaway git repo with one commit; deleted on teardown.
private func makeTempRepo() throws -> String {
    let dir = NSTemporaryDirectory() + "tillergit-test-" + UUID().uuidString
    try FileManager.default.createDirectory(atPath: dir, withIntermediateDirectories: true)
    for args in [
        ["init", "-b", "main"],
        ["config", "user.email", "test@tiller.dev"],
        ["config", "user.name", "Tiller Test"],
        ["commit", "--allow-empty", "-m", "root"]
    ] {
        let p = Process()
        p.executableURL = URL(fileURLWithPath: "/usr/bin/git")
        p.arguments = args
        p.currentDirectoryURL = URL(fileURLWithPath: dir)
        try p.run(); p.waitUntilExit()
    }
    return dir
}

@Test func listShowsMainWorktree() async throws {
    let repo = try makeTempRepo()
    defer { try? FileManager.default.removeItem(atPath: repo) }
    let worktrees = try await GitWorktrees.list(repoPath: repo)
    #expect(worktrees.count == 1)
    #expect(worktrees[0].isMain)
}

@Test func addAndRemoveWorktree() async throws {
    let repo = try makeTempRepo()
    defer { try? FileManager.default.removeItem(atPath: repo) }
    let wtPath = repo + "-wt-feature"
    defer { try? FileManager.default.removeItem(atPath: wtPath) }

    try await GitWorktrees.add(repoPath: repo, branch: "feature-x", at: wtPath)
    var worktrees = try await GitWorktrees.list(repoPath: repo)
    #expect(worktrees.count == 2)
    #expect(worktrees.contains { $0.branch == "feature-x" })

    try await GitWorktrees.remove(repoPath: repo, path: wtPath)
    worktrees = try await GitWorktrees.list(repoPath: repo)
    #expect(worktrees.count == 1)
}

@Test func runnerThrowsOnFailure() async {
    await #expect(throws: GitError.self) {
        _ = try await GitRunner.run(["definitely-not-a-command"], in: NSTemporaryDirectory())
    }
}

@Test func addWorktreeFromExplicitBase() async throws {
    let repo = try makeTempRepo()
    defer { try? FileManager.default.removeItem(atPath: repo) }

    // Tag the current commit, then advance main — proves the new branch
    // comes from the tag, not from whatever HEAD happens to be later.
    for args in [["tag", "base-point"], ["commit", "--allow-empty", "-m", "second"]] {
        let p = Process()
        p.executableURL = URL(fileURLWithPath: "/usr/bin/git")
        p.arguments = args
        p.currentDirectoryURL = URL(fileURLWithPath: repo)
        try p.run(); p.waitUntilExit()
    }

    let wtPath = repo + "-wt-based"
    defer { try? FileManager.default.removeItem(atPath: wtPath) }
    try await GitWorktrees.add(repoPath: repo, branch: "based-branch", at: wtPath, base: "base-point")

    let branchHead = try await GitRunner.run(["log", "-1", "--format=%H", "based-branch"], in: repo)
    let tagHead = try await GitRunner.run(["log", "-1", "--format=%H", "base-point"], in: repo)
    #expect(branchHead == tagHead)
}
