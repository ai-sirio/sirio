import Testing
import Foundation
@testable import TillerACP

/// Scripted fake agent: replies to initialize/session methods like a real
/// adapter so ACPSession's whole lifecycle runs against the mock transport.
private actor FakeAgent {
    let mock = MockTransport()
    var loadSession = false
    var lastPromptStopReason = "end_turn"

    init(loadSession: Bool = false) {
        self.loadSession = loadSession
    }

    /// Watches sent lines and answers protocol requests in the background.
    func run() {
        Task {
            var answered = 0
            while true {
                let sent = await mock.sent
                if sent.count > answered {
                    for index in answered..<sent.count {
                        try? await answer(try mock.sentMessage(index))
                    }
                    answered = sent.count
                }
                try? await Task.sleep(for: .milliseconds(5))
            }
        }
    }

    private func answer(_ message: JSONRPCMessage) async throws {
        guard case .request(let id, let method, _) = message else { return }
        let idJSON = String(decoding: try JSONEncoder().encode(id), as: UTF8.self)
        switch method {
        case "initialize":
            await mock.emit("""
            {"jsonrpc":"2.0","id":\(idJSON),"result":{"protocolVersion":1,
             "agentCapabilities":{"loadSession":\(loadSession)}}}
            """.replacingOccurrences(of: "\n", with: ""))
        case "session/new":
            await mock.emit(#"{"jsonrpc":"2.0","id":\#(idJSON),"result":{"sessionId":"sess-new"}}"#)
        case "session/load":
            await mock.emit(#"{"jsonrpc":"2.0","id":\#(idJSON),"result":{}}"#)
        case "session/prompt":
            await mock.emit(#"{"jsonrpc":"2.0","id":\#(idJSON),"result":{"stopReason":"\#(lastPromptStopReason)"}}"#)
        default:
            break
        }
    }
}

@Suite struct ACPSessionTests {
    private func makeSession(agent: FakeAgent,
                             fileSystem: any ACPFileSystem = NullFileSystem())
    async throws -> ACPSession {
        let client = ACPClient(transport: agent.mock)
        let session = ACPSession(client: client, fileSystem: fileSystem)
        try await session.start()
        await agent.run()
        return session
    }

    @Test func connectCreatesNewSession() async throws {
        let agent = FakeAgent()
        let session = try await makeSession(agent: agent)
        let handle = try await session.connect(cwd: "/w", resumeSessionId: nil)
        #expect(handle.sessionId == "sess-new")
        #expect(handle.didResume == false)
        #expect(handle.agentCapabilities.loadSession == false)
    }

    @Test func connectResumesWhenSupported() async throws {
        let agent = FakeAgent(loadSession: true)
        let session = try await makeSession(agent: agent)
        let handle = try await session.connect(cwd: "/w", resumeSessionId: "old-1")
        #expect(handle.sessionId == "old-1")
        #expect(handle.didResume == true)
    }

    @Test func connectIgnoresResumeIdWhenUnsupported() async throws {
        let agent = FakeAgent(loadSession: false)
        let session = try await makeSession(agent: agent)
        let handle = try await session.connect(cwd: "/w", resumeSessionId: "old-1")
        #expect(handle.sessionId == "sess-new")
        #expect(handle.didResume == false)
    }

    @Test func promptReturnsStopReasonAndEmitsUpdates() async throws {
        let agent = FakeAgent()
        let session = try await makeSession(agent: agent)
        _ = try await session.connect(cwd: "/w", resumeSessionId: nil)

        await agent.mock.emit(#"{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"sess-new","update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"hi"}}}}"#)
        let reason = try await session.prompt([.text("hello")])
        #expect(reason == .endTurn)

        var iterator = session.events.makeAsyncIterator()
        guard case .update(let update)? = await iterator.next() else {
            Issue.record("expected update event"); return
        }
        #expect(update == .agentMessageChunk(.text("hi")))
    }

    @Test func permissionRequestRoundTrips() async throws {
        let agent = FakeAgent()
        let session = try await makeSession(agent: agent)
        _ = try await session.connect(cwd: "/w", resumeSessionId: nil)

        await agent.mock.emit("""
        {"jsonrpc":"2.0","id":99,"method":"session/request_permission","params":
         {"sessionId":"sess-new","toolCall":{"toolCallId":"tc1"},
          "options":[{"optionId":"y","name":"Allow","kind":"allow_once"}]}}
        """.replacingOccurrences(of: "\n", with: ""))

        var iterator = session.events.makeAsyncIterator()
        guard case .permissionRequested(let requestId, let toolCall, let options)? =
                await iterator.next() else {
            Issue.record("expected permissionRequested"); return
        }
        #expect(requestId == .number(99))
        #expect(toolCall.toolCallId == "tc1")
        #expect(options.first?.optionId == "y")

        await session.answerPermission(requestId: requestId,
                                       outcome: .selected(optionId: "y"))
        let sent = try await agent.mock.waitForSent(count: 3)  // init, new, answer
        guard case .response(let id, let result, _) = try await agent.mock.sentMessage(sent.count - 1) else {
            Issue.record("expected response"); return
        }
        #expect(id == .number(99))
        #expect(result?["outcome"]?["optionId"]?.stringValue == "y")
    }

    @Test func servesFsReadAndRejectsOutsidePaths() async throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("acp-sess-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        try "content-x".write(to: root.appendingPathComponent("f.txt"),
                              atomically: true, encoding: .utf8)

        let agent = FakeAgent()
        let session = try await makeSession(
            agent: agent, fileSystem: WorktreeFileSystem(root: root.path))
        _ = try await session.connect(cwd: root.path, resumeSessionId: nil)
        let baseline = await agent.mock.sent.count

        await agent.mock.emit(#"{"jsonrpc":"2.0","id":50,"method":"fs/read_text_file","params":{"sessionId":"sess-new","path":"\#(root.path)/f.txt"}}"#)
        var sent = try await agent.mock.waitForSent(count: baseline + 1)
        guard case .response(_, let result, _) = try await agent.mock.sentMessage(sent.count - 1) else {
            Issue.record("expected response"); return
        }
        #expect(result?["content"]?.stringValue == "content-x")

        await agent.mock.emit(#"{"jsonrpc":"2.0","id":51,"method":"fs/read_text_file","params":{"sessionId":"sess-new","path":"/etc/hosts"}}"#)
        sent = try await agent.mock.waitForSent(count: baseline + 2)
        guard case .response(let id, _, let error) = try await agent.mock.sentMessage(sent.count - 1) else {
            Issue.record("expected error response"); return
        }
        #expect(id == .number(51))
        #expect(error != nil)
    }
}

/// File system that rejects everything — for tests that never touch fs.
private struct NullFileSystem: ACPFileSystem {
    struct Unsupported: Error {}
    func readTextFile(path: String, line: Int?, limit: Int?) throws -> String { throw Unsupported() }
    func writeTextFile(path: String, content: String) throws { throw Unsupported() }
}
