import Testing
import Foundation
@testable import TillerACP

@Suite struct JSONRPCTests {
    @Test func decodesRequest() throws {
        let line = Data(#"{"jsonrpc":"2.0","id":3,"method":"fs/read_text_file","params":{"path":"/a"}}"#.utf8)
        let message = try JSONRPCMessage.decode(line)
        #expect(message == .request(id: .number(3), method: "fs/read_text_file",
                                    params: .object(["path": .string("/a")])))
    }

    @Test func decodesNotification() throws {
        let line = Data(#"{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"s1"}}"#.utf8)
        let message = try JSONRPCMessage.decode(line)
        #expect(message == .notification(method: "session/update",
                                         params: .object(["sessionId": .string("s1")])))
    }

    @Test func decodesResponseAndError() throws {
        let ok = try JSONRPCMessage.decode(Data(#"{"jsonrpc":"2.0","id":"a","result":{"x":1}}"#.utf8))
        #expect(ok == .response(id: .string("a"), result: .object(["x": .number(1)]), error: nil))

        let err = try JSONRPCMessage.decode(Data(#"{"jsonrpc":"2.0","id":4,"error":{"code":-32601,"message":"nope"}}"#.utf8))
        guard case .response(let id, let result, let error) = err else {
            Issue.record("expected response"); return
        }
        #expect(id == .number(4))
        #expect(result == nil)
        #expect(error?.code == -32601)
        #expect(error?.message == "nope")
    }

    @Test func encodedLineRoundTrips() throws {
        let original = JSONRPCMessage.request(
            id: .number(7), method: "initialize",
            params: .object(["protocolVersion": .number(1)]))
        let line = try original.encodedLine()
        #expect(line.last == UInt8(ascii: "\n"))
        #expect(try JSONRPCMessage.decode(line.dropLast()) == original)
    }

    @Test func rejectsGarbage() {
        #expect(throws: (any Error).self) {
            _ = try JSONRPCMessage.decode(Data("not json".utf8))
        }
    }
}
