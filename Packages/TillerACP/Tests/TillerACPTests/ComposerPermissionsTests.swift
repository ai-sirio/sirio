import Testing
@testable import TillerACP

@Suite("ComposerPermissions")
struct ComposerPermissionsTests {
    func pending(_ id: String, kind: ToolKind = .execute) -> TranscriptItem {
        .toolCall(ToolCallItem(
            toolCallId: id, title: "Run \(id)", kind: kind, status: .pending,
            permission: PermissionState(
                requestId: .string(id),
                options: [PermissionOption(optionId: "allow", name: "Allow",
                                           kind: .allowOnce)])))
    }

    @Test func extractsPendingInTranscriptOrder() {
        let resolved = TranscriptItem.toolCall(ToolCallItem(
            toolCallId: "done", title: "Done", kind: .execute, status: .completed,
            permission: PermissionState(requestId: .string("done"), options: [],
                                        resolution: .cancelled)))
        let items: [TranscriptItem] = [
            pending("p1"), resolved,
            .agentMessage(id: "a1", text: "x", isComplete: true),
            pending("p2"),
        ]
        let result = ComposerPermissions.extract(from: items)
        #expect(result.map(\.toolCallId) == ["p1", "p2"])
        #expect(result[0].requestId == .string("p1"))
        #expect(result[0].options.count == 1)
    }

    @Test func excludesSwitchModePermissions() {
        let items: [TranscriptItem] = [pending("exec"), pending("plan-exit", kind: .switchMode)]
        let result = ComposerPermissions.extract(from: items)
        #expect(result.map(\.toolCallId) == ["exec"])
    }

    @Test func emptyWhenNothingPending() {
        #expect(ComposerPermissions.extract(from: [
            .agentMessage(id: "a1", text: "x", isComplete: true),
        ]).isEmpty)
    }
}
