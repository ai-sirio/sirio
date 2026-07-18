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

    @Test func decodesNewSessionResultWithModels() throws {
        // claude-code-acp shape: standard ACP model selector.
        let json = Data("""
        {"sessionId":"sess-1","models":{"currentModelId":"default",
         "availableModels":[
           {"modelId":"default","name":"Default (recommended)","description":"Opus"},
           {"modelId":"sonnet","name":"Sonnet"}]}}
        """.utf8)
        let result = try JSONDecoder().decode(NewSessionResult.self, from: json)
        #expect(result.resolvedModels?.currentModelId == "default")
        #expect(result.resolvedModels?.availableModels.map(\.name)
                == ["Default (recommended)", "Sonnet"])
    }

    @Test func derivesModelsFromOpenCodeConfigOptions() throws {
        // OpenCode 1.18 shape: no "models", a configOptions "model" select.
        let json = Data("""
        {"sessionId":"ses_1","configOptions":[
          {"id":"model","name":"Model","type":"select","currentValue":"openai/gpt-5.4",
           "options":[{"value":"openai/gpt-5.4","name":"OpenAI/GPT-5.4"},
                      {"value":"openai/gpt-5.6-luna","name":"OpenAI/GPT-5.6 Luna"}]},
          {"id":"effort","name":"Effort","type":"select","currentValue":"medium",
           "options":[{"value":"medium","name":"Medium"}]}]}
        """.utf8)
        let result = try JSONDecoder().decode(NewSessionResult.self, from: json)
        #expect(result.resolvedModels?.currentModelId == "openai/gpt-5.4")
        #expect(result.resolvedModels?.availableModels.map(\.modelId)
                == ["openai/gpt-5.4", "openai/gpt-5.6-luna"])
        #expect(result.resolvedModels?.availableModels.map(\.name)
                == ["OpenAI/GPT-5.4", "OpenAI/GPT-5.6 Luna"])
    }

    @Test func resolvedModelsIsNilWithoutModelData() throws {
        let json = Data(#"{"sessionId":"sess-1"}"#.utf8)
        let result = try JSONDecoder().decode(NewSessionResult.self, from: json)
        #expect(result.resolvedModels == nil)
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
