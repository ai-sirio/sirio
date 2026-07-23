import Foundation
import Testing
@testable import TillerACP

@Suite struct OpenCodeDriverTests {
    private actor FakeConnection: OpenCodeConnection {
        struct Request: Sendable {
            let method: String
            let path: String
            let body: Data?
        }

        private let sessionResponse: Data
        private let providersResponse: Data
        private var requests: [Request] = []
        private var continuation: AsyncThrowingStream<Data, Error>.Continuation?
        private var pendingEvents: [Data] = []

        init(sessionResponse: Data, providersResponse: Data) {
            self.sessionResponse = sessionResponse
            self.providersResponse = providersResponse
        }

        func request(method: String, path: String, body: Data?) async throws -> Data {
            requests.append(Request(method: method, path: path, body: body))
            switch (method, path) {
            case ("POST", "/session"):
                return sessionResponse
            case ("GET", "/config/providers"):
                return providersResponse
            default:
                return Data("true".utf8)
            }
        }

        nonisolated func events() -> AsyncThrowingStream<Data, Error> {
            AsyncThrowingStream { continuation in
                Task { await self.attach(continuation) }
            }
        }

        func close() {}

        private func attach(_ continuation: AsyncThrowingStream<Data, Error>.Continuation) {
            self.continuation = continuation
            for event in pendingEvents { continuation.yield(event) }
            pendingEvents.removeAll()
        }

        func emit(_ event: Data) {
            if let continuation { continuation.yield(event) }
            else { pendingEvents.append(event) }
        }

        func finishEvents() { continuation?.finish() }

        func requestSnapshot() -> [Request] { requests }

        func waitForRequest(method: String, path: String) async -> Request? {
            for _ in 0..<400 {
                if let request = requests.first(where: { $0.method == method && $0.path == path }) {
                    return request
                }
                try? await Task.sleep(for: .milliseconds(5))
            }
            return requests.first(where: { $0.method == method && $0.path == path })
        }
    }

    private func fixtureData(_ name: String, extension: String) throws -> Data {
        let url = Bundle.module.url(forResource: name, withExtension: `extension`,
                                    subdirectory: "Fixtures/opencode-init-turn")!
        return try Data(contentsOf: url)
    }

    private func makeConnection() throws -> FakeConnection {
        FakeConnection(
            sessionResponse: try fixtureData("session-create", extension: "json"),
            providersResponse: try fixtureData("providers", extension: "json"))
    }

    private func fixtureEvents() throws -> [Data] {
        var parser = SSEParser()
        return parser.feed(try fixtureData("events", extension: "sse"))
    }

    private actor EventCollector {
        private var values: [ACPSessionEvent] = []
        func append(_ value: ACPSessionEvent) { values.append(value) }
        func snapshot() -> [ACPSessionEvent] { values }
    }

    private func collect(_ driver: OpenCodeHTTPDriver,
                         into collector: EventCollector) -> Task<Void, Never> {
        Task {
            for await event in driver.events { await collector.append(event) }
        }
    }

    private func waitForEvents(_ collector: EventCollector,
                               count: Int) async -> [ACPSessionEvent] {
        for _ in 0..<400 {
            let values = await collector.snapshot()
            if values.count >= count { return values }
            try? await Task.sleep(for: .milliseconds(5))
        }
        return await collector.snapshot()
    }

    private func json(_ value: String) -> Data { Data(value.utf8) }

    private func updates(_ events: [ACPSessionEvent]) -> [SessionUpdate] {
        events.compactMap {
            guard case .update(let update) = $0 else { return nil }
            return update
        }
    }

    @Test func fixtureTurnMapsToOrderedCanonicalUpdatesAndEndTurn() async throws {
        let connection = try makeConnection()
        let driver = OpenCodeHTTPDriver(connection: connection, permissionMode: .ask,
                                         resumeSessionId: nil)
        let collector = EventCollector()
        let eventTask = collect(driver, into: collector)
        try await driver.start()
        let handle = try await driver.connect(cwd: "/tmp/worktree", resumeSessionId: nil,
                                              mcpServers: [])
        #expect(handle.didResume == false)
        #expect(handle.sessionId.hasPrefix("ses_"))
        #expect(handle.models?.availableModels.isEmpty == false)
        let sessionID = handle.sessionId

        let promptTask = Task { try await driver.prompt([.text("done")]) }
        _ = await connection.waitForRequest(method: "POST",
                                             path: "/session/\(handle.sessionId)/message")
        for event in try fixtureEvents() { await connection.emit(event) }
        await connection.emit(json(#"{"type":"message.updated","properties":{"sessionID":"__SESSION__","info":{"id":"msg_assistant","role":"assistant"}}}"#.replacingOccurrences(of: "__SESSION__", with: sessionID)))
        await connection.emit(json(#"{"type":"message.part.updated","properties":{"sessionID":"__SESSION__","part":{"type":"reasoning","id":"part_reasoning","messageID":"msg_assistant","sessionID":"__SESSION__"},"delta":"thinking"}}"#.replacingOccurrences(of: "__SESSION__", with: sessionID)))
        await connection.emit(json(#"{"type":"message.part.updated","properties":{"sessionID":"__SESSION__","part":{"type":"text","id":"part_text","messageID":"msg_assistant","sessionID":"__SESSION__"},"delta":"done"}}"#.replacingOccurrences(of: "__SESSION__", with: sessionID)))
        await connection.emit(json(#"{"type":"message.part.updated","properties":{"sessionID":"__SESSION__","part":{"type":"tool","id":"part_tool","callID":"call-1","messageID":"msg_assistant","sessionID":"__SESSION__","tool":"bash","state":{"status":"running","input":{"command":"echo done"}}}}}"#.replacingOccurrences(of: "__SESSION__", with: sessionID)))
        await connection.emit(json(#"{"type":"message.part.updated","properties":{"sessionID":"__SESSION__","part":{"type":"tool","id":"part_tool","callID":"call-1","messageID":"msg_assistant","sessionID":"__SESSION__","tool":"bash","state":{"status":"completed","input":{"command":"echo done"},"output":"done\n"}}}}"#.replacingOccurrences(of: "__SESSION__", with: sessionID)))
        await connection.emit(json(#"{"type":"message.part.updated","properties":{"sessionID":"__SESSION__","part":{"type":"step-finish","id":"part_finish","messageID":"msg_assistant","sessionID":"__SESSION__","tokens":{"total":10,"input":8,"output":2}}}}"#.replacingOccurrences(of: "__SESSION__", with: sessionID)))
        await connection.emit(json(#"{"type":"session.status","properties":{"sessionID":"__SESSION__","status":{"type":"idle"}}}"#.replacingOccurrences(of: "__SESSION__", with: sessionID)))

        #expect(try await promptTask.value == .endTurn)
        let actual = updates(await waitForEvents(collector, count: 5))
        #expect(actual.count >= 5)
        #expect(actual[0] == .agentThoughtChunk(.text("thinking")))
        #expect(actual[1] == .agentMessageChunk(.text("done")))
        guard case .toolCall(let call) = actual[2] else {
            Issue.record("expected toolCall")
            return
        }
        #expect(call.toolCallId == "call-1")
        #expect(call.kind == .execute)
        guard case .toolCallUpdate(let update) = actual[3] else {
            Issue.record("expected toolCallUpdate")
            return
        }
        #expect(update.toolCallId == "call-1")
        #expect(update.status == .completed)
        #expect(actual[4] == .usageUpdate(ContextUsage(used: 10, size: 262144)))

        eventTask.cancel()
        await driver.stop()
    }

    @Test func permissionRoundTripUsesOpenCodeReplyPathAndBody() async throws {
        let connection = try makeConnection()
        let driver = OpenCodeHTTPDriver(connection: connection, permissionMode: .ask,
                                         resumeSessionId: nil)
        let collector = EventCollector()
        let eventTask = collect(driver, into: collector)
        try await driver.start()
        let handle = try await driver.connect(cwd: "/tmp/worktree", resumeSessionId: nil, mcpServers: [])

        await connection.emit(json(#"{"type":"permission.asked","properties":{"id":"perm-1","sessionID":"__SESSION__","permission":"bash","patterns":["echo done"],"metadata":{},"always":[]}}"#.replacingOccurrences(of: "__SESSION__", with: handle.sessionId)))
        let events = await waitForEvents(collector, count: 1)
        guard case .permissionRequested(let requestId, let toolCall, let options)? = events.first else {
            Issue.record("expected permissionRequested")
            return
        }
        #expect(requestId == .string("perm-1"))
        #expect(toolCall.toolCallId == "perm-1")
        #expect(toolCall.kind == .execute)
        #expect(options.map(\.kind) == [.allowOnce, .allowAlways, .rejectOnce])

        await driver.answerPermission(requestId: requestId,
                                      outcome: .selected(optionId: "allow_once"))
        let request = await connection.waitForRequest(method: "POST",
                                                        path: "/session/\(handle.sessionId)/permissions/perm-1")
        #expect(request != nil)
        let body = try #require(request?.body)
        let value = try JSONDecoder().decode(JSONValue.self, from: body)
        #expect(value["response"]?.stringValue == "once")

        eventTask.cancel()
        await driver.stop()
    }

    @Test func resumeUsesGivenSessionWithoutCreatingOne() async throws {
        let connection = try makeConnection()
        let driver = OpenCodeHTTPDriver(connection: connection, permissionMode: .ask,
                                         resumeSessionId: "ses_resume")
        try await driver.start()
        let handle = try await driver.connect(cwd: "/tmp/worktree", resumeSessionId: "ses_resume",
                                              mcpServers: [])
        #expect(handle.sessionId == "ses_resume")
        #expect(handle.didResume == true)
        #expect(!(await connection.requestSnapshot()).contains {
            $0.method == "POST" && $0.path == "/session"
        })
        await driver.stop()
    }

    @Test func cancelPostsAbortForCurrentSession() async throws {
        let connection = try makeConnection()
        let driver = OpenCodeHTTPDriver(connection: connection, permissionMode: .ask,
                                         resumeSessionId: "ses_abort")
        try await driver.start()
        _ = try await driver.connect(cwd: "/tmp/worktree", resumeSessionId: "ses_abort",
                                     mcpServers: [])
        await driver.cancel()
        #expect(await connection.waitForRequest(method: "POST",
                                                 path: "/session/ses_abort/abort") != nil)
        await driver.stop()
    }
}
