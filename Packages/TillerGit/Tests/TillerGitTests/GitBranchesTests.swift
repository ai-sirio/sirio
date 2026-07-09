import Testing
import Foundation
@testable import TillerGit

private func makeTempRepoWithBranches() throws -> String {
    let dir = NSTemporaryDirectory() + "tillergit-branches-test-" + UUID().uuidString
    try FileManager.default.createDirectory(atPath: dir, withIntermediateDirectories: true)
    for args in [
        ["init", "-b", "main"],
        ["config", "user.email", "test@tiller.dev"],
        ["config", "user.name", "Tiller Test"],
        ["commit", "--allow-empty", "-m", "root"],
        ["branch", "feature-x"],
        ["branch", "feature-y"]
    ] {
        let p = Process()
        p.executableURL = URL(fileURLWithPath: "/usr/bin/git")
        p.arguments = args
        p.currentDirectoryURL = URL(fileURLWithPath: dir)
        try p.run(); p.waitUntilExit()
    }
    return dir
}

@Test func listReturnsAllLocalBranches() async throws {
    let repo = try makeTempRepoWithBranches()
    defer { try? FileManager.default.removeItem(atPath: repo) }
    let branches = try await GitBranches.list(repoPath: repo)
    #expect(Set(branches) == Set(["main", "feature-x", "feature-y"]))
}
