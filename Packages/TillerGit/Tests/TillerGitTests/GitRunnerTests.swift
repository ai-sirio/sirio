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

@Test func runStreamingEmitsLinesAndSucceeds() async throws {
    let src = try makeGitTestRepository().path
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

@Test func runCapturedAcceptsNoIndexDifferenceExitCode() async throws {
    let directory = FileManager.default.temporaryDirectory
        .appendingPathComponent("tiller-no-index-\(UUID().uuidString)", isDirectory: true)
    try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
    defer { try? FileManager.default.removeItem(at: directory) }
    try "new\n".write(to: directory.appendingPathComponent("new.txt"), atomically: true, encoding: .utf8)

    let result = try await GitRunner.runCaptured(
        ["diff", "--no-index", "--", "/dev/null", directory.appendingPathComponent("new.txt").path],
        in: directory.path,
        acceptedExitCodes: [0, 1]
    )

    #expect(result.exitCode == 1)
    #expect(result.stdoutString.contains("+new"))
}

@Test func runCapturedRejectsOutputBeyondLimit() async throws {
    let directory = FileManager.default.temporaryDirectory
        .appendingPathComponent("tiller-output-limit-\(UUID().uuidString)", isDirectory: true)
    try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
    defer { try? FileManager.default.removeItem(at: directory) }
    try String(repeating: "line\n", count: 1000)
        .write(to: directory.appendingPathComponent("large.txt"), atomically: true, encoding: .utf8)

    await #expect(throws: GitError.outputTooLarge(maxBytes: 128, maxLines: 20)) {
        _ = try await GitRunner.runCaptured(
            ["diff", "--no-index", "--", "/dev/null", directory.appendingPathComponent("large.txt").path],
            in: directory.path,
            acceptedExitCodes: [0, 1],
            limits: GitOutputLimits(maxBytes: 128, maxLines: 20)
        )
    }
}
@Test func runCapturedTreatsEmptyOutputAsZeroLines() async throws {
    let repository = try makeGitTestRepository()
    defer { try? FileManager.default.removeItem(at: repository) }

    let result = try await GitRunner.runCaptured(
        ["-c", "alias.raw=!printf ''", "raw"],
        in: repository.path,
        limits: GitOutputLimits(maxBytes: 32, maxLines: 0)
    )

    #expect(result.stdout.isEmpty)
}

@Test func runCapturedAcceptsLfTerminatedOutputAtLineLimit() async throws {
    let repository = try makeGitTestRepository()
    defer { try? FileManager.default.removeItem(at: repository) }

    let result = try await GitRunner.runCaptured(
        ["-c", "alias.raw=!printf 'one\\n'", "raw"],
        in: repository.path,
        limits: GitOutputLimits(maxBytes: 32, maxLines: 1)
    )

    #expect(result.stdout == Data("one\n".utf8))
}

@Test func runCapturedAcceptsUnterminatedOutputAtLineLimit() async throws {
    let repository = try makeGitTestRepository()
    defer { try? FileManager.default.removeItem(at: repository) }

    let result = try await GitRunner.runCaptured(
        ["-c", "alias.raw=!printf one", "raw"],
        in: repository.path,
        limits: GitOutputLimits(maxBytes: 32, maxLines: 1)
    )

    #expect(result.stdout == Data("one".utf8))
}

@Test func runCapturedRejectsUnterminatedOutputAtZeroLineLimit() async throws {
    let repository = try makeGitTestRepository()
    defer { try? FileManager.default.removeItem(at: repository) }

    await #expect(throws: GitError.outputTooLarge(maxBytes: 32, maxLines: 0)) {
        _ = try await GitRunner.runCaptured(
            ["-c", "alias.raw=!printf one", "raw"],
            in: repository.path,
            limits: GitOutputLimits(maxBytes: 32, maxLines: 0)
        )
    }
}

@Test func runCapturedDrainsHighStderrWhileCapturingStdout() async throws {
    let repository = try makeGitTestRepository()
    defer { try? FileManager.default.removeItem(at: repository) }
    try runGitForTest(
        ["config", "alias.noisy", "!printf '%131072s' x >&2; printf stdout"],
        in: repository
    )

    let result = try await GitRunner.runCaptured(["noisy"], in: repository.path)

    #expect(result.stdoutString.contains("stdout"))
    #expect(result.stderr.utf8.count >= 131072)
}
