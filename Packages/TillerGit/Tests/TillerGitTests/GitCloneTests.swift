import Testing
import Foundation
@testable import TillerGit

@Test func parseProgressExtractsPercent() {
    let line = "Receiving objects:  45% (450/1000), 2.1 MiB | 1.2 MiB/s"
    #expect(GitClone.parseProgress(fromLine: line) == 0.45)
}

@Test func parseProgressHandlesDoneLine() {
    let line = "Receiving objects: 100% (3/3), done."
    #expect(GitClone.parseProgress(fromLine: line) == 1.0)
}

@Test func parseProgressNilForUnrelatedLine() {
    #expect(GitClone.parseProgress(fromLine: "Cloning into 'repo'...") == nil)
}

@Test func parseProgressNilForResolvingDeltasLine() {
    #expect(GitClone.parseProgress(fromLine: "Resolving deltas: 100% (1/1), done.") == nil)
}

private func runGit(_ args: [String], in dir: String) throws {
    let p = Process()
    p.executableURL = URL(fileURLWithPath: "/usr/bin/git")
    p.arguments = args
    p.currentDirectoryURL = URL(fileURLWithPath: dir)
    try p.run(); p.waitUntilExit()
}

private func makeTempRepoWithCommit() throws -> String {
    let dir = NSTemporaryDirectory() + "tillergit-clone-src-" + UUID().uuidString
    try FileManager.default.createDirectory(atPath: dir, withIntermediateDirectories: true)
    try runGit(["init", "-b", "main"], in: dir)
    try runGit(["config", "user.email", "test@tiller.dev"], in: dir)
    try runGit(["config", "user.name", "Tiller Test"], in: dir)
    try "hello".write(toFile: dir + "/file.txt", atomically: true, encoding: .utf8)
    try runGit(["add", "."], in: dir)
    try runGit(["commit", "-m", "root"], in: dir)
    return dir
}

@Test func cloneCopiesRepoAndReportsProgress() async throws {
    let src = try makeTempRepoWithCommit()
    defer { try? FileManager.default.removeItem(atPath: src) }
    let destParent = NSTemporaryDirectory() + "tillergit-clone-dest-" + UUID().uuidString
    try FileManager.default.createDirectory(atPath: destParent, withIntermediateDirectories: true)
    defer { try? FileManager.default.removeItem(atPath: destParent) }
    let dest = destParent + "/clone"

    let collector = ProgressCollector()
    try await GitClone.clone(url: src, to: dest) { value in
        collector.append(value)
    }

    #expect(FileManager.default.fileExists(atPath: dest + "/file.txt"))
}

private final class ProgressCollector: @unchecked Sendable {
    private let lock = NSLock()
    private var values: [Double] = []
    func append(_ value: Double) {
        lock.withLock { values.append(value) }
    }
}
