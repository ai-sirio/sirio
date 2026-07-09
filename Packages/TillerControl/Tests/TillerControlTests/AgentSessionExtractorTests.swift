import Testing
import Foundation
@testable import TillerControl

@Test func extractsClaudeSessionIdFromStdinJSON() {
    let json = #"{"session_id":"abc-123","transcript_path":"/tmp/t.jsonl","hook_event_name":"SessionStart"}"#
    #expect(AgentSessionExtractor.sessionRef(fromJSON: Data(json.utf8)) == "abc-123")
}

@Test func extractsCodexThreadIdFromPayloadArguments() {
    let payload = #"{"type":"agent-turn-complete","thread-id":"0199a213-81ef-7db2-ac26-a35a05e8a409","last-assistant-message":"done"}"#
    #expect(AgentSessionExtractor.sessionRef(fromPayloadArguments: ["turn-ended", payload])
        == "0199a213-81ef-7db2-ac26-a35a05e8a409")
}

@Test func extractsRolloutIdFromRolloutPath() {
    let payload = #"{"rollout-path":"/Users/x/.codex/sessions/2026/07/08/rollout-2026-07-08T12-00-00-11111111-2222-3333-4444-555555555555.jsonl"}"#
    #expect(AgentSessionExtractor.sessionRef(fromPayloadArguments: [payload])
        == "11111111-2222-3333-4444-555555555555")
}

@Test func malformedOrIrrelevantPayloadYieldsNil() {
    #expect(AgentSessionExtractor.sessionRef(fromPayloadArguments: ["not-json", #"{"foo":1}"#]) == nil)
    #expect(AgentSessionExtractor.sessionRef(fromJSON: Data()) == nil)
}

@Test func emptySessionValueYieldsNil() {
    #expect(AgentSessionExtractor.sessionRef(fromJSON: Data(#"{"session_id":""}"#.utf8)) == nil)
}

@Test func unparseableRolloutPathYieldsNil() {
    let payload = #"{"rollout-path":"/x/not-a-rollout-file.jsonl"}"#
    #expect(AgentSessionExtractor.sessionRef(fromPayloadArguments: [payload]) == nil)
}

@Test func sessionIdWinsOverThreadId() {
    let json = #"{"thread_id":"low-priority","session_id":"high-priority"}"#
    #expect(AgentSessionExtractor.sessionRef(fromJSON: Data(json.utf8)) == "high-priority")
}

@Test func rolloutIdLowercasesUppercaseUUID() {
    let payload = #"{"rollout-path":"/x/rollout-2026-07-08T12-00-00-11111111-AAAA-BBBB-CCCC-DDDDDDDDDDDD.jsonl"}"#
    #expect(AgentSessionExtractor.sessionRef(fromPayloadArguments: [payload])
        == "11111111-aaaa-bbbb-cccc-dddddddddddd")
}

@Test func underscoreRolloutPathVariantIsSupported() {
    let payload = #"{"rollout_path":"/x/rollout-2026-07-08T12-00-00-11111111-2222-3333-4444-555555555555.jsonl"}"#
    #expect(AgentSessionExtractor.sessionRef(fromPayloadArguments: [payload])
        == "11111111-2222-3333-4444-555555555555")
}
