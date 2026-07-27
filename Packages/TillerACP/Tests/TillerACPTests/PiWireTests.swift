import Foundation
import Testing
@testable import TillerACP

@Suite struct PiWireTests {
    private func value(_ line: Data) throws -> JSONValue {
        let payload = line.last == UInt8(ascii: "\n") ? line.dropLast() : line[...]
        return try JSONDecoder().decode(JSONValue.self, from: payload)
    }

    @Test func commandUsesPiEnvelopeAndOneLF() throws {
        let line = try PiWire.command(
            id: "req-1", type: "set_model",
            fields: ["provider": .string("anthropic"),
                     "modelId": .string("claude-sonnet-4")])
        let decoded = try value(line)

        #expect(line.last == UInt8(ascii: "\n"))
        #expect(decoded["id"]?.stringValue == "req-1")
        #expect(decoded["type"]?.stringValue == "set_model")
        #expect(decoded["provider"]?.stringValue == "anthropic")
        #expect(decoded["modelId"]?.stringValue == "claude-sonnet-4")
        #expect(decoded["jsonrpc"] == nil)
        #expect(decoded["method"] == nil)
    }

    @Test func decodesCorrelatedSuccessAndFailureResponses() throws {
        let success = try PiWire.decode(Data(
            #"{"id":"req-1","type":"response","command":"get_state","success":true,"data":{"sessionId":"abc"}}"#.utf8))
        let failure = try PiWire.decode(Data(
            #"{"id":"req-2","type":"response","command":"set_model","success":false,"error":"Model not found"}"#.utf8))

        #expect(success == .response(.init(
            id: "req-1", command: "get_state", success: true,
            data: .object(["sessionId": .string("abc")]), error: nil)))
        #expect(failure == .response(.init(
            id: "req-2", command: "set_model", success: false,
            data: nil, error: "Model not found")))
    }

    @Test func decodesEventsAndAcceptsTrailingCR() throws {
        let message = try PiWire.decode(Data(
            "{\"type\":\"agent_start\"}\r".utf8))
        guard case .event(let type, let fields) = message else {
            Issue.record("expected event")
            return
        }
        #expect(type == "agent_start")
        #expect(fields["type"]?.stringValue == "agent_start")
    }

    @Test func extensionUIResponsesUseProtocolSpecificFields() throws {
        let selected = try value(PiWire.extensionUIValue(id: "ui-1", value: "Allow"))
        let confirmed = try value(PiWire.extensionUIConfirmation(id: "ui-2", confirmed: false))
        let cancelled = try value(PiWire.extensionUICancel(id: "ui-3"))

        #expect(selected["type"]?.stringValue == "extension_ui_response")
        #expect(selected["id"]?.stringValue == "ui-1")
        #expect(selected["value"]?.stringValue == "Allow")
        #expect(confirmed["confirmed"]?.boolValue == false)
        #expect(cancelled["cancelled"]?.boolValue == true)
    }
}
