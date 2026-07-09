import Testing
import Foundation
@testable import TillerGit

private final class LineCollector: @unchecked Sendable {
    private let lock = NSLock()
    private(set) var lines: [String] = []
    func append(_ line: String) {
        lock.lock(); defer { lock.unlock() }
        lines.append(line)
    }
}

private func runGit(_ args: [String], in dir: String) throws {
    let p = Process()
    p.executableURL = URL(fileURLWithPath: "/usr/bin/git")
    p.arguments = args
    p.currentDirectoryURL = URL(fileURLWithPath: dir)
    try p.run(); p.waitUntilExit()
}

private func makeTempRepoWithCommit() throws -> String {
    let dir = NSTemporaryDirectory() + "tillergit-runner-src-" + UUID().uuidString
    try FileManager.default.createDirectory(atPath: dir, withIntermediateDirectories: true)
    try runGit(["init", "-b", "main"], in: dir)
    try runGit(["config", "user.email", "test@tiller.dev"], in: dir)
    try runGit(["config", "user.name", "Tiller Test"], in: dir)
    try "hello".write(toFile: dir + "/file.txt", atomically: true, encoding: .utf8)
    try runGit(["add", "."], in: dir)
    try runGit(["commit", "-m", "root"], in: dir)
    return dir
}

@Test func runStreamingEmitsLinesAndSucceeds() async throws {
    let src = try makeTempRepoWithCommit()
    defer { try? FileManager.default.removeItem(atPath: src) }
    let destParent = NSTemporaryDirectory() + "tillergit-runner-dest-" + UUID().uuidString
    try FileManager.default.createDirectory(atPath: destParent, withIntermediateDirectories: true)
    defer { try? FileManager.default.removeItem(atPath: destParent) }
    let dest = destParent + "/clone"

    let collector = LineCollector()
    try await GitRunner.runStreaming(
        ["clone", "--progress", "--no-local", src, dest], in: destParent
    ) { line in collector.append(line) }

    #expect(FileManager.default.fileExists(atPath: dest + "/.git"))
    #expect(!collector.lines.isEmpty)
}

@Test func runStreamingThrowsOnFailure() async throws {
    let destParent = NSTemporaryDirectory() + "tillergit-runner-fail-" + UUID().uuidString
    try FileManager.default.createDirectory(atPath: destParent, withIntermediateDirectories: true)
    defer { try? FileManager.default.removeItem(atPath: destParent) }

    await #expect(throws: GitError.self) {
        try await GitRunner.runStreaming(
            ["clone", "--progress", "/nonexistent/repo/path", destParent + "/dest"],
            in: destParent
        ) { _ in }
    }
}
