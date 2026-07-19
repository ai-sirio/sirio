import Testing
import Foundation
@testable import TillerACP

@Suite struct ACPClientTests {
    struct Empty: Codable, Equatable {}

    @Test func correlatesRequestAndResponse() async throws {
        let mock = MockTransport()
        let client = ACPClient(transport: mock)
        try await client.start()

        let result = Task {
            try await client.request(
                "session/new", params: NewSessionParams(cwd: "/w"), as: NewSessionResult.self)
        }

        let sent = try await mock.waitForSent(count: 1)
        guard case .request(let id, let method, let params) = try await mock.sentMessage(0) else {
            Issue.record("expected request"); return
        }
        #expect(method == "session/new")
        #expect(params?["cwd"]?.stringValue == "/w")
        #expect(sent.count == 1)

        let idJSON = String(decoding: try JSONEncoder().encode(id), as: UTF8.self)
        await mock.emit(#"{"jsonrpc":"2.0","id":\#(idJSON),"result":{"sessionId":"s9"}}"#)
        #expect(try await result.value.sessionId == "s9")
    }

    @Test func surfacesAgentErrors() async throws {
        let mock = MockTransport()
        let client = ACPClient(transport: mock)
        try await client.start()

        let result = Task {
            try await client.request(
                "session/new", params: NewSessionParams(cwd: "/w"), as: NewSessionResult.self)
        }
        _ = try await mock.waitForSent(count: 1)
        guard case .request(let id, _, _) = try await mock.sentMessage(0) else {
            Issue.record("expected request"); return
        }
        let idJSON = String(decoding: try JSONEncoder().encode(id), as: UTF8.self)
        await mock.emit(#"{"jsonrpc":"2.0","id":\#(idJSON),"error":{"code":-32000,"message":"auth required"}}"#)

        await #expect(throws: ACPClientError.self) { _ = try await result.value }
    }

    @Test func deliversIncomingNotificationsAndRequests() async throws {
        let mock = MockTransport()
        let client = ACPClient(transport: mock)
        try await client.start()

        await mock.emit(#"{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"s1","update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"hi"}}}}"#)
        await mock.emit(#"{"jsonrpc":"2.0","id":7,"method":"fs/read_text_file","params":{"sessionId":"s1","path":"/w/a"}}"#)

        var iterator = client.incoming.makeAsyncIterator()
        guard case .notification(let method, _)? = await iterator.next() else {
            Issue.record("expected notification"); return
        }
        #expect(method == "session/update")
        guard case .request(let id, let requestMethod, _)? = await iterator.next() else {
            Issue.record("expected request"); return
        }
        #expect(id == .number(7))
        #expect(requestMethod == "fs/read_text_file")
    }

    @Test func respondEncodesResultForGivenId() async throws {
        let mock = MockTransport()
        let client = ACPClient(transport: mock)
        try await client.start()
        try await client.respond(to: .number(7), result: ReadTextFileResult(content: "x"))
        _ = try await mock.waitForSent(count: 1)
        guard case .response(let id, let result, let error) = try await mock.sentMessage(0) else {
            Issue.record("expected response"); return
        }
        #expect(id == .number(7))
        #expect(result?["content"]?.stringValue == "x")
        #expect(error == nil)
    }

    @Test func transportCloseFailsPendingRequests() async throws {
        let mock = MockTransport()
        let client = ACPClient(transport: mock)
        try await client.start()
        let result = Task {
            try await client.request("session/prompt", params: Empty(), as: Empty.self)
        }
        _ = try await mock.waitForSent(count: 1)
        await mock.close()
        await #expect(throws: ACPClientError.self) { _ = try await result.value }
    }
}
