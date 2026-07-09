import Testing
import Foundation
import TillerCore
@testable import TillerTerminal

// MARK: - Fake PTY

final class FakePty: UsagePty, @unchecked Sendable {
    private(set) var writes: [Data] = []
    var spawnShouldFail = false
    /// Lines delivered to `onOutput` after a successful `spawn()`.
    var outputLines: [String] = []
    var onOutput: (@Sendable (Data) -> Void)?

    func spawn(executable: String, arguments: [String], environment: [String],
               initialCols: UInt16, initialRows: UInt16) throws {
        if spawnShouldFail { throw PtyError.forkFailed(1) }
        for line in outputLines {
            onOutput?(Data(line.utf8))
        }
    }

    func write(_ data: Data) { writes.append(data) }
    func terminate() {}
}

// MARK: - Fixtures

/// Carriage-return-delimited output that parses to session=12%, weekly=34%.
/// Copied from the known-good fixture in ProviderUsageParserTests.
private let sessionAndWeeklyFixture = "Current session\r12% used\r" +
    "Resets 4:00pm\rCurrent week (all models)\r34% used\r"

// MARK: - Tests

@Test func returnsUsageOnSuccess() async {
    let pty = FakePty()
    pty.outputLines = [sessionAndWeeklyFixture]

    let outcome = await ClaudeUsageFetcher.fetch(
        makePty: { onOutput in
            pty.onOutput = onOutput
            return pty
        },
        settle: .milliseconds(1),
        poll: .milliseconds(1),
        timeout: 2
    )

    guard case .success(let usage) = outcome else {
        Issue.record("Expected .success, got \(outcome)")
        return
    }
    #expect(usage.session?.usedPercent == 12)
    #expect(usage.weekly?.usedPercent == 34)
    #expect(usage.fableWeekly == nil)
}

@Test func returnsNotInstalledOnSpawnFailure() async {
    let pty = FakePty()
    pty.spawnShouldFail = true

    let outcome = await ClaudeUsageFetcher.fetch(
        makePty: { onOutput in
            pty.onOutput = onOutput
            return pty
        },
        settle: .milliseconds(1),
        poll: .milliseconds(1),
        timeout: 2
    )

    #expect(outcome == .unavailable(.notInstalled))
}

@Test func returnsNotInstalledOnCommandNotFound() async {
    let pty = FakePty()
    pty.outputLines = ["zsh: command not found: claude"]

    let outcome = await ClaudeUsageFetcher.fetch(
        makePty: { onOutput in
            pty.onOutput = onOutput
            return pty
        },
        settle: .milliseconds(1),
        poll: .milliseconds(1),
        timeout: 2
    )

    #expect(outcome == .unavailable(.notInstalled))
}

@Test func returnsLoggedOutOnNotLoggedIn() async {
    let pty = FakePty()
    pty.outputLines = ["Please run /login to authenticate"]

    let outcome = await ClaudeUsageFetcher.fetch(
        makePty: { onOutput in
            pty.onOutput = onOutput
            return pty
        },
        settle: .milliseconds(1),
        poll: .milliseconds(1),
        timeout: 2
    )

    #expect(outcome == .unavailable(.loggedOut))
}

@Test func chunkOrderPreserved() async {
    // Regression: splitting the usage panel into 20+ tiny fragments must not
    // corrupt the parsed output. Every chunk arrives on the buffer before the
    // first poll so order-preservation is a structural guarantee — still worth
    // asserting in case the buffer primitive ever changes.
    let pty = FakePty()
    let fixture = sessionAndWeeklyFixture
    let chunks = stride(from: 0, to: fixture.count, by: 3).map { i in
        let start = fixture.index(fixture.startIndex, offsetBy: i)
        let end = fixture.index(start, offsetBy: min(3, fixture.distance(from: start, to: fixture.endIndex)))
        return String(fixture[start..<end])
    }
    #expect(chunks.count >= 20, "chunk count was \(chunks.count) — expected ≥20")
    pty.outputLines = chunks

    let outcome = await ClaudeUsageFetcher.fetch(
        makePty: { onOutput in
            pty.onOutput = onOutput
            return pty
        },
        settle: .milliseconds(1),
        poll: .milliseconds(1),
        timeout: 2
    )

    guard case .success(let usage) = outcome else {
        Issue.record("Expected .success, got \(outcome)")
        return
    }
    #expect(usage.session?.usedPercent == 12)
    #expect(usage.weekly?.usedPercent == 34)
    #expect(usage.fableWeekly == nil)
}

@Test func sendsExtraEnterForPaletteConfirmation() async {
    let pty = FakePty()
    pty.outputLines = ["Usage limits\nShow plan usage limits"]

    let outcome = await ClaudeUsageFetcher.fetch(
        makePty: { onOutput in
            pty.onOutput = onOutput
            return pty
        },
        settle: .milliseconds(1),
        poll: .milliseconds(1),
        timeout: 0.2
    )

    // Should time out because the palette text does not contain parseable usage.
    #expect(outcome == .timedOut)
    #expect(pty.writes.count == 2)
    #expect(pty.writes[0] == Data("/usage\r".utf8))
    #expect(pty.writes[1] == Data("\r".utf8))
}

@Test func returnsTimedOutOnNoParse() async {
    let pty = FakePty()
    pty.outputLines = ["just some unrelated banner text"]

    let outcome = await ClaudeUsageFetcher.fetch(
        makePty: { onOutput in
            pty.onOutput = onOutput
            return pty
        },
        settle: .milliseconds(1),
        poll: .milliseconds(1),
        timeout: 0.2
    )

    #expect(outcome == .timedOut)
}
