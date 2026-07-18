import Testing
import Foundation
@testable import TillerACP

@Suite struct ACPTypesTests {
    @Test func decodesInitializeResultWithDefaults() throws {
        // Minimal agent answer: absent capabilities default to false/empty.
        let json = Data(#"{"protocolVersion":1,"agentCapabilities":{"loadSession":true}}"#.utf8)
        let result = try JSONDecoder().decode(InitializeResult.self, from: json)
        #expect(result.protocolVersion == 1)
        #expect(result.agentCapabilities.loadSession == true)
        #expect(result.agentCapabilities.promptCapabilities.image == false)
        #expect(result.agentCapabilities.promptCapabilities.embeddedContext == false)
        #expect(result.authMethods.isEmpty)
    }

    @Test func encodesInitializeParams() throws {
        let params = InitializeParams(
            protocolVersion: 1,
            clientCapabilities: ClientCapabilities(
                fs: FileSystemCapability(readTextFile: true, writeTextFile: true),
                terminal: false))
        let value = try JSONValue.encoding(params)
        #expect(value["protocolVersion"]?.intValue == 1)
        #expect(value["clientCapabilities"]?["fs"]?["readTextFile"]?.boolValue == true)
        #expect(value["clientCapabilities"]?["terminal"]?.boolValue == false)
    }

    @Test func decodesNewSessionResultWithModes() throws {
        let json = Data("""
        {"sessionId":"sess-1","modes":{"currentModeId":"default",
         "availableModes":[{"id":"default","name":"Default"},{"id":"plan","name":"Plan"}]}}
        """.utf8)
        let result = try JSONDecoder().decode(NewSessionResult.self, from: json)
        #expect(result.sessionId == "sess-1")
        #expect(result.modes?.currentModeId == "default")
        #expect(result.modes?.availableModes.count == 2)
    }

    @Test func decodesStopReason() throws {
        let result = try JSONDecoder().decode(
            PromptResult.self, from: Data(#"{"stopReason":"end_turn"}"#.utf8))
        #expect(result.stopReason == .endTurn)
    }

    @Test func contentBlockVariantsRoundTrip() throws {
        let blocks: [ContentBlock] = [
            .text("hello"),
            .image(mimeType: "image/png", data: "aGk="),
            .resourceLink(uri: "file:///w/App/A.swift", name: "A.swift"),
            .resource(uri: "file:///w/App/A.swift", text: "let x = 1"),
        ]
        let data = try JSONEncoder().encode(blocks)
        let back = try JSONDecoder().decode([ContentBlock].self, from: data)
        #expect(back == blocks)
    }

    @Test func contentBlockUnknownTypeIsTolerated() throws {
        let json = Data(#"[{"type":"audio","data":"...","mimeType":"audio/wav"}]"#.utf8)
        let blocks = try JSONDecoder().decode([ContentBlock].self, from: json)
        #expect(blocks == [.unknown(type: "audio")])
    }
}
