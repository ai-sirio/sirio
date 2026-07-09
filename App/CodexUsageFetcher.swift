import Foundation
import TillerCore

/// Speaks the Codex app-server JSON-RPC protocol over plain stdio pipes
/// (not a PTY — JSON-RPC needs clean binary stdio). Ported from Orca's
/// `codex-fetcher.ts` RPC path; the PTY-fallback path Orca also has is
/// intentionally not ported (see spec's Sub-phase 1 rationale).
enum CodexUsageFetcher {
    private static let timeout: TimeInterval = 10
    private static let poll: Duration = .milliseconds(100)

    private actor LineBuffer {
        private var data = Data()
        private var pendingLines: [Data] = []

        func append(_ chunk: Data) {
            data.append(chunk)
            while let newlineIndex = data.firstIndex(of: 0x0A) {
                pendingLines.append(Data(data[data.startIndex..<newlineIndex]))
                data.removeSubrange(data.startIndex...newlineIndex)
            }
        }

        func drainLines() -> [Data] {
            defer { pendingLines.removeAll() }
            return pendingLines
        }
    }

    static func fetch() async -> UsageFetchOutcome {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/env")
        process.arguments = ["codex", "-s", "read-only", "-a", "untrusted", "app-server"]

        let stdinPipe = Pipe()
        let stdoutPipe = Pipe()
        process.standardInput = stdinPipe
        process.standardOutput = stdoutPipe
        // Why: an unread stderr pipe fills its kernel buffer and blocks the
        // child process mid-write; routing to /dev/null avoids that hang
        // entirely since Codex's stderr isn't needed for this RPC exchange.
        process.standardError = FileHandle.nullDevice

        let buffer = LineBuffer()
        stdoutPipe.fileHandleForReading.readabilityHandler = { handle in
            let chunk = handle.availableData
            guard !chunk.isEmpty else { return }
            Task { await buffer.append(chunk) }
        }

        do {
            try process.run()
        } catch {
            return .unavailable(.notInstalled)
        }
        defer {
            stdoutPipe.fileHandleForReading.readabilityHandler = nil
            if process.isRunning { process.terminate() }
        }

        func writeLine(_ json: [String: Any]) {
            guard process.isRunning else { return }
            guard let data = try? JSONSerialization.data(withJSONObject: json) else { return }
            var line = data
            line.append(UInt8(ascii: "\n"))
            stdinPipe.fileHandleForWriting.write(line)
        }

        writeLine([
            "jsonrpc": "2.0", "id": 1, "method": "initialize",
            "params": ["clientInfo": ["name": "tiller", "version": "1.0.0"]]
        ])

        var rateLimitsRequested = false
        let deadline = Date().addingTimeInterval(timeout)

        while Date() < deadline {
            for line in await buffer.drainLines() {
                guard
                    let object = try? JSONSerialization.jsonObject(with: line) as? [String: Any],
                    let id = object["id"] as? Int
                else { continue }

                if id == 1, !rateLimitsRequested {
                    writeLine(["jsonrpc": "2.0", "method": "initialized", "params": [String: Any]()])
                    writeLine(["jsonrpc": "2.0", "id": 2, "method": "account/rateLimits/read", "params": [String: Any]()])
                    rateLimitsRequested = true
                    continue
                }

                if id == 2, rateLimitsRequested {
                    if let errorObject = object["error"] as? [String: Any],
                       let message = errorObject["message"] as? String {
                        if message.range(
                            of: "not logged in|unauthorized|401",
                            options: [.regularExpression, .caseInsensitive]
                        ) != nil {
                            return .unavailable(.loggedOut)
                        }
                        return .unavailable(.error)
                    }
                    guard
                        let resultObject = object["result"],
                        let resultData = try? JSONSerialization.data(withJSONObject: resultObject),
                        let usage = CodexRateLimitParser.parse(resultData: resultData)
                    else {
                        return .unavailable(.error)
                    }
                    return .success(usage)
                }
            }
            try? await Task.sleep(for: poll)
        }
        return .timedOut
    }
}
