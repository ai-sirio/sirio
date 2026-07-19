import Testing
import Foundation
@testable import TillerACP

@Suite struct ProcessTransportTests {
    @Test func catEchoesLinesBack() async throws {
        let transport = ProcessTransport(
            executable: "/bin/cat", arguments: [],
            cwd: FileManager.default.temporaryDirectory.path)
        try await transport.start()
        try await transport.send(line: Data("{\"a\":1}\n".utf8))
        try await transport.send(line: Data("{\"b\":2}\n".utf8))

        var received: [String] = []
        for try await line in transport.lines() {
            received.append(String(decoding: line, as: UTF8.self))
            if received.count == 2 { break }
        }
        #expect(received == ["{\"a\":1}", "{\"b\":2}"])
        await transport.terminate()
    }

    @Test func splitsPartialAndBatchedWrites() async throws {
        // printf writes two lines in one burst and one line without buffering:
        // the transport must reassemble on \n regardless of chunk boundaries.
        let transport = ProcessTransport(
            executable: "/bin/sh",
            arguments: ["-c", #"printf 'one\ntwo\n'; printf 'three\n'"#],
            cwd: FileManager.default.temporaryDirectory.path)
        try await transport.start()

        var received: [String] = []
        for try await line in transport.lines() {
            received.append(String(decoding: line, as: UTF8.self))
        }
        #expect(received == ["one", "two", "three"])
    }

    @Test func streamFinishesOnProcessExit() async throws {
        let transport = ProcessTransport(
            executable: "/usr/bin/true", arguments: [],
            cwd: FileManager.default.temporaryDirectory.path)
        try await transport.start()
        var count = 0
        for try await _ in transport.lines() { count += 1 }
        #expect(count == 0)  // reaching here proves the stream terminated
    }

    @Test func stderrIsForwardedNotMixedIntoLines() async throws {
        let collector = StderrCollector()
        let transport = ProcessTransport(
            executable: "/bin/sh",
            arguments: ["-c", "echo err >&2; echo '{}' "],
            cwd: FileManager.default.temporaryDirectory.path,
            onStderrLine: { line in Task { await collector.append(line) } })
        try await transport.start()
        var received: [String] = []
        for try await line in transport.lines() {
            received.append(String(decoding: line, as: UTF8.self))
        }
        #expect(received == ["{}"]) 
        // stderr delivery is async; poll briefly.
        for _ in 0..<100 where await collector.lines.isEmpty {
            try await Task.sleep(for: .milliseconds(10))
        }
        #expect(await collector.lines == ["err"])
    }
}

private actor StderrCollector {
    var lines: [String] = []
    func append(_ line: String) { lines.append(line) }
}
