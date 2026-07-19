import Foundation
import Testing
@testable import TillerACP

@Suite struct TranscriptReducerTests {
    @Test func streamsAgentMessageIntoOneItem() {
        var reducer = TranscriptReducer()
        reducer.userPrompted([.text("hi")])
        reducer.apply(.agentMessageChunk(.text("Hel")))
        reducer.apply(.agentMessageChunk(.text("lo")))
        #expect(reducer.items.count == 2)
        guard case .agentMessage(_, let text, let isComplete) = reducer.items[1] else {
            Issue.record("expected agentMessage"); return
        }
        #expect(text == "Hello")
        #expect(isComplete == false)
    }

    @Test func turnEndCompletesOpenMessage() {
        var reducer = TranscriptReducer()
        reducer.apply(.agentMessageChunk(.text("done")))
        reducer.turnEnded(.endTurn)
        guard case .agentMessage(_, _, let isComplete) = reducer.items[0] else {
            Issue.record("expected agentMessage"); return
        }
        #expect(isComplete == true)
    }

    /// Un thought che interrompe un messaggio agente deve chiuderlo: lasciarlo
    /// isComplete=false mostra RunningDots per sempre sotto la risposta.
    @Test func thoughtChunkCompletesOpenAgentMessage() {
        var reducer = TranscriptReducer()
        reducer.apply(.agentMessageChunk(.text("Risposta")))
        reducer.apply(.agentThoughtChunk(.text("pensiero")))
        guard case .agentMessage(_, "Risposta", let isComplete) = reducer.items[0] else {
            Issue.record("expected agentMessage first"); return
        }
        #expect(isComplete)
    }

    /// Il replay di session/load alterna user e agent chunk: nessun messaggio
    /// agente già streamato deve restare aperto.
    @Test func replayLeavesNoOpenAgentMessages() {
        var reducer = TranscriptReducer()
        reducer.apply(.userMessageChunk(.text("domanda 1")))
        reducer.apply(.agentMessageChunk(.text("risposta 1")))
        reducer.apply(.userMessageChunk(.text("domanda 2")))
        reducer.apply(.agentMessageChunk(.text("risposta 2")))
        reducer.turnEnded(.endTurn)
        let incomplete = reducer.items.filter {
            if case .agentMessage(_, _, false) = $0 { return true }
            return false
        }
        #expect(incomplete.isEmpty)
    }

    @Test func newUserPromptStartsNewAgentMessage() {
        var reducer = TranscriptReducer()
        reducer.apply(.agentMessageChunk(.text("first")))
        reducer.turnEnded(.endTurn)
        reducer.userPrompted([.text("again")])
        reducer.apply(.agentMessageChunk(.text("second")))
        // first agent message, turn divider, user message, second agent message
        #expect(reducer.items.count == 4)
        guard case .agentMessage(_, let text, _) = reducer.items[3] else {
            Issue.record("expected second agentMessage"); return
        }
        #expect(text == "second")
    }

    @Test func turnEndAppendsTimestampedDivider() {
        var reducer = TranscriptReducer()
        let stamp = Date(timeIntervalSince1970: 1_000)
        reducer.apply(.agentMessageChunk(.text("done")))
        reducer.turnEnded(.endTurn, at: stamp)
        guard case .turnDivider(_, let at) = reducer.items[1] else {
            Issue.record("expected turnDivider"); return
        }
        #expect(at == stamp)
    }

    @Test func turnEndOnEmptyTranscriptAddsNoDivider() {
        var reducer = TranscriptReducer()
        reducer.turnEnded(.cancelled)
        #expect(reducer.items.isEmpty)
    }

    @Test func thoughtsAccumulateSeparatelyFromMessages() {
        var reducer = TranscriptReducer()
        reducer.apply(.agentThoughtChunk(.text("hmm ")))
        reducer.apply(.agentThoughtChunk(.text("ok")))
        reducer.apply(.agentMessageChunk(.text("answer")))
        #expect(reducer.items.count == 2)
        guard case .thought(_, let text) = reducer.items[0] else {
            Issue.record("expected thought"); return
        }
        #expect(text == "hmm ok")
    }

    @Test func nonTextChunksAreIgnoredForStreaming() {
        var reducer = TranscriptReducer()
        reducer.apply(.agentMessageChunk(.image(mimeType: "image/png", data: "x")))
        // Non-text chunks in agent messages are rare; v1 drops them silently.
        #expect(reducer.items.isEmpty)
    }

    @Test func itemIdsAreUniqueAndStable() {
        var reducer = TranscriptReducer()
        reducer.userPrompted([.text("a")])
        reducer.apply(.agentMessageChunk(.text("b")))
        reducer.userPrompted([.text("c")])
        let ids = reducer.items.map(\.id)
        #expect(Set(ids).count == ids.count)
    }

    @Test func replayedUserMessageChunksBecomeUserMessages() {
        // session/load replays the conversation, including the user's turns.
        var reducer = TranscriptReducer()
        reducer.apply(.userMessageChunk(.text("fix ")))
        reducer.apply(.userMessageChunk(.text("bug")))
        reducer.apply(.agentMessageChunk(.text("ok")))
        #expect(reducer.items.count == 2)
        guard case .userMessage(_, let blocks) = reducer.items[0] else {
            Issue.record("expected userMessage"); return
        }
        #expect(blocks == [.text("fix bug")])
    }

    @Test func modeAndCommandsUpdateState() {
        var reducer = TranscriptReducer()
        reducer.apply(.currentModeUpdate("plan"))
        reducer.apply(.availableCommandsUpdate([AvailableCommand(name: "init", description: "d")]))
        #expect(reducer.currentModeId == "plan")
        #expect(reducer.availableCommands.map(\.name) == ["init"])
        #expect(reducer.items.isEmpty)
    }
}
