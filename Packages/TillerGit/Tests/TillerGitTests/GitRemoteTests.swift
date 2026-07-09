import Testing
import Foundation
@testable import TillerGit

private func makeTempRepo() throws -> String {
    let dir = NSTemporaryDirectory() + "tillergit-remote-test-" + UUID().uuidString
    try FileManager.default.createDirectory(atPath: dir, withIntermediateDirectories: true)
    let p = Process()
    p.executableURL = URL(fileURLWithPath: "/usr/bin/git")
    p.arguments = ["init", "-b", "main"]
    p.currentDirectoryURL = URL(fileURLWithPath: dir)
    try p.run(); p.waitUntilExit()
    return dir
}

private func addOriginRemote(_ url: String, in dir: String) throws {
    let p = Process()
    p.executableURL = URL(fileURLWithPath: "/usr/bin/git")
    p.arguments = ["remote", "add", "origin", url]
    p.currentDirectoryURL = URL(fileURLWithPath: dir)
    try p.run(); p.waitUntilExit()
}

@Test func githubOwnerFromHTTPSRemote() async throws {
    let repo = try makeTempRepo()
    defer { try? FileManager.default.removeItem(atPath: repo) }
    try addOriginRemote("https://github.com/acme/widgets.git", in: repo)
    let owner = await GitRemote.githubOwner(repoPath: repo)
    #expect(owner == "acme")
}

@Test func githubOwnerFromSSHRemote() async throws {
    let repo = try makeTempRepo()
    defer { try? FileManager.default.removeItem(atPath: repo) }
    try addOriginRemote("git@github.com:acme/widgets.git", in: repo)
    let owner = await GitRemote.githubOwner(repoPath: repo)
    #expect(owner == "acme")
}

@Test func githubOwnerNilForNonGitHubRemote() async throws {
    let repo = try makeTempRepo()
    defer { try? FileManager.default.removeItem(atPath: repo) }
    try addOriginRemote("https://gitlab.com/acme/widgets.git", in: repo)
    let owner = await GitRemote.githubOwner(repoPath: repo)
    #expect(owner == nil)
}

@Test func githubOwnerNilWithoutRemote() async throws {
    let repo = try makeTempRepo()
    defer { try? FileManager.default.removeItem(atPath: repo) }
    let owner = await GitRemote.githubOwner(repoPath: repo)
    #expect(owner == nil)
}

@Test func projectNameFromHTTPSURLWithGitSuffix() {
    #expect(GitRemote.projectName(fromCloneURL: "https://github.com/acme/widgets.git") == "widgets")
}

@Test func projectNameFromHTTPSURLWithoutGitSuffix() {
    #expect(GitRemote.projectName(fromCloneURL: "https://github.com/acme/widgets") == "widgets")
}

@Test func projectNameFromSSHURLWithGitSuffix() {
    #expect(GitRemote.projectName(fromCloneURL: "git@github.com:acme/widgets.git") == "widgets")
}

@Test func projectNameFromSSHURLWithoutGitSuffix() {
    #expect(GitRemote.projectName(fromCloneURL: "git@github.com:acme/widgets") == "widgets")
}

@Test func projectNameTrimsTrailingSlash() {
    #expect(GitRemote.projectName(fromCloneURL: "https://github.com/acme/widgets/") == "widgets")
}
