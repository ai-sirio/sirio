import Testing
@testable import TillerACP

@Suite struct TranscriptReducerToolTests {
    private let options = [
        PermissionOption(optionId: "y", name: "Allow", kind: .allowOnce),
        PermissionOption(optionId: "n", name: "Reject", kind: .rejectOnce),
    ]

    @Test func toolCallLifecycleUpdatesInPlace() {
        var reducer = TranscriptReducer()
        reducer.apply(.toolCall(ToolCall(toolCallId: "tc1", title: "Read A.swift",
                                         kind: .read, status: .pending)))
        reducer.apply(.toolCallUpdate(ToolCallUpdate(toolCallId: "tc1", status: .completed,
            content: [.content(.text("let x = 1"))])))
        #expect(reducer.items.count == 1)
        guard case .toolCall(let item) = reducer.items[0] else {
            Issue.record("expected toolCall"); return
        }
        #expect(item.status == .completed)
        #expect(item.content == [.content(.text("let x = 1"))])
    }

    @Test func toolCallClosesOpenAgentMessage() {
        var reducer = TranscriptReducer()
        reducer.apply(.agentMessageChunk(.text("Let me look…")))
        reducer.apply(.toolCall(ToolCall(toolCallId: "tc1", title: "Read",
                                         kind: .read, status: .pending)))
        reducer.apply(.agentMessageChunk(.text("Found it.")))
        #expect(reducer.items.count == 3)
        guard case .agentMessage(_, let first, let complete) = reducer.items[0] else {
            Issue.record("expected agentMessage"); return
        }
        #expect(first == "Let me look…")
        #expect(complete == true)
    }

    @Test func permissionAttachesToExistingToolCall() {
        var reducer = TranscriptReducer()
        reducer.apply(.toolCall(ToolCall(toolCallId: "tc1", title: "Edit F.swift",
                                         kind: .edit, status: .pending)))
        reducer.permissionRequested(requestId: .number(9),
                                    toolCall: ToolCallUpdate(toolCallId: "tc1"),
                                    options: options)
        guard case .toolCall(let item) = reducer.items[0] else {
            Issue.record("expected toolCall"); return
        }
        #expect(item.permission?.isPending == true)
        #expect(item.permission?.options == options)
    }

    @Test func permissionCreatesCardWhenToolCallUnseen() {
        var reducer = TranscriptReducer()
        reducer.permissionRequested(requestId: .number(2),
            toolCall: ToolCallUpdate(toolCallId: "tcX", title: "Edit Y.swift", kind: .edit),
            options: options)
        #expect(reducer.items.count == 1)
        guard case .toolCall(let item) = reducer.items[0] else {
            Issue.record("expected toolCall"); return
        }
        #expect(item.title == "Edit Y.swift")
        #expect(item.permission?.requestId == .number(2))
    }

    @Test func permissionResolvedRecordsSelection() {
        var reducer = TranscriptReducer()
        reducer.permissionRequested(requestId: .number(2),
            toolCall: ToolCallUpdate(toolCallId: "tc1", title: "Edit", kind: .edit),
            options: options)
        reducer.permissionResolved(requestId: .number(2),
                                   resolution: .selected(optionId: "y"))
        guard case .toolCall(let item) = reducer.items[0] else {
            Issue.record("expected toolCall"); return
        }
        #expect(item.permission?.resolution == .selected(optionId: "y"))
        #expect(item.permission?.isPending == false)
    }

    @Test func cancelledTurnCancelsPendingPermissions() {
        var reducer = TranscriptReducer()
        reducer.permissionRequested(requestId: .number(5),
            toolCall: ToolCallUpdate(toolCallId: "tc1", title: "Edit", kind: .edit),
            options: options)
        reducer.turnEnded(.cancelled)
        guard case .toolCall(let item) = reducer.items[0] else {
            Issue.record("expected toolCall"); return
        }
        #expect(item.permission?.resolution == .cancelled)
    }
    /// I chunk _meta.terminal_output si accumulano sul ToolCallItem.
    @Test func terminalOutputChunksAccumulate() {
        var reducer = TranscriptReducer()
        reducer.apply(.toolCall(ToolCall(
            toolCallId: "t1", title: "Bash", kind: .execute, status: .inProgress,
            content: [.terminal(terminalId: "t1")])))
        reducer.apply(.toolCallUpdate(ToolCallUpdate(
            toolCallId: "t1",
            terminalMeta: TerminalMeta(terminalOutput: .init(terminalId: "t1",
                                                              data: "riga 1\n")))))
        reducer.apply(.toolCallUpdate(ToolCallUpdate(
            toolCallId: "t1",
            terminalMeta: TerminalMeta(terminalOutput: .init(terminalId: "t1",
                                                              data: "riga 2\n")))))
        guard case .toolCall(let item) = reducer.items[0] else {
            Issue.record("expected toolCall"); return
        }
        #expect(item.terminalOutput == "riga 1\nriga 2\n")
    }

    /// terminal_exit registra l'exit status; un update successivo che rimpiazza
    /// il content (upsert dal tool_call finale) non deve cancellare l'accumulo.
    @Test func terminalExitAndContentReplacePreserveState() {
        var reducer = TranscriptReducer()
        reducer.apply(.toolCall(ToolCall(
            toolCallId: "t1", title: "Bash", kind: .execute, status: .inProgress,
            content: [.terminal(terminalId: "t1")])))
        reducer.apply(.toolCallUpdate(ToolCallUpdate(
            toolCallId: "t1",
            terminalMeta: TerminalMeta(terminalOutput: .init(terminalId: "t1",
                                                              data: "ok\n")))))
        reducer.apply(.toolCall(ToolCall(
            toolCallId: "t1", title: "Bash", kind: .execute, status: .completed,
            content: [.terminal(terminalId: "t1")])))
        reducer.apply(.toolCallUpdate(ToolCallUpdate(
            toolCallId: "t1", status: .completed,
            terminalMeta: TerminalMeta(terminalExit: .init(terminalId: "t1",
                                                            exitCode: 0)))))
        guard case .toolCall(let item) = reducer.items[0] else {
            Issue.record("expected toolCall"); return
        }
        #expect(item.terminalOutput == "ok\n")
        #expect(item.terminalExit == TerminalExitStatus(exitCode: 0, signal: nil))
        #expect(item.status == .completed)
    }

}
