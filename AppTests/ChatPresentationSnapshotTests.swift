import Foundation
import Testing
@testable import Tiller
@testable import TillerACP

@Suite
struct ChatPresentationSnapshotTests {
    @Test func fixtureSnapshotMatchesCurrentDerivedProperties() {
        let fixture = ChatStreamFixture.make()
        let built = ChatPresentationSnapshot.build(items: fixture.transcript)
        let snapshot = built.snapshot

        #expect(snapshot.items == fixture.transcript)
        #expect(snapshot.grouped == ToolCallTree.group(items: fixture.transcript))
        #expect(snapshot.composerPermissions == ComposerPermissions.extract(from: fixture.transcript))
        #expect(snapshot.activeSubagentTasks == SubagentTasks.extract(from: fixture.transcript))
        #expect(snapshot.currentActivity == currentActivity(in: fixture.transcript))
        #expect(snapshot.hasPendingPermission == hasPendingPermission(in: fixture.transcript))

        for item in fixture.transcript {
            guard case .agentMessage(let id, let text, _) = item else { continue }
            #expect(snapshot.agentMessageSegments[id] == AgentMessageSegmenter.segments(from: text))
        }
    }

    @Test func unchangedAgentMessagesReuseSegmentsWhileActiveMessageUpdates() {
        let first = [
            .agentMessage(id: "old", text: "stable prose", isComplete: false),
            .agentMessage(id: "active", text: "first", isComplete: false)
        ] as [TranscriptItem]
        let second = [
            .agentMessage(id: "old", text: "stable prose", isComplete: false),
            .agentMessage(id: "active", text: "first second", isComplete: false)
        ] as [TranscriptItem]
        var invocations = 0
        let segmenter: (String) -> [AgentMessageSegment] = { text in
            invocations += 1
            return AgentMessageSegmenter.segments(from: text)
        }

        let firstBuild = ChatPresentationSnapshot.build(items: first, segmenter: segmenter)
        let secondBuild = ChatPresentationSnapshot.build(
            items: second, previousCache: firstBuild.cache, segmenter: segmenter)

        #expect(invocations == 3)
        #expect(secondBuild.snapshot.agentMessageSegments["old"] == firstBuild.snapshot.agentMessageSegments["old"])
        #expect(secondBuild.snapshot.agentMessageSegments["active"] == AgentMessageSegmenter.segments(from: "first second"))
    }

    @Test func identicalSnapshotDoesNotPublish() {
        let items: [TranscriptItem] = [
            .agentMessage(id: "a1", text: "same", isComplete: true)
        ]
        let firstBuild = ChatPresentationSnapshot.build(items: items)
        let secondBuild = ChatPresentationSnapshot.build(
            items: items, previousCache: firstBuild.cache)
        var published = 0
        var current = firstBuild.snapshot

        ChatPresentationSnapshot.assignIfChanged(
            &current, secondBuild.snapshot) { published += 1 }

        #expect(published == 0)
        #expect(current == firstBuild.snapshot)
    }

    @Test func insightBlocksAndToolChildrenStayAttached() {
        var child = ToolCallItem(toolCallId: "child", title: "child", kind: .read,
                                 status: .completed)
        child.parentToolCallId = "parent"
        let items: [TranscriptItem] = [
            .toolCall(ToolCallItem(toolCallId: "parent", title: "parent", kind: .execute,
                                   status: .inProgress)),
            .toolCall(child),
            .agentMessage(id: "agent", text: "before\n★ Insight ───\ninside\n───\nafter", isComplete: true)
        ]

        let snapshot = ChatPresentationSnapshot.build(items: items).snapshot

        #expect(snapshot.grouped.roots.map(\.id) == ["parent", "agent"])
        #expect(snapshot.grouped.children(of: "parent").map(\.toolCallId) == ["child"])
        #expect(snapshot.agentMessageSegments["agent"] == [
            .prose("before\n"), .insight("inside"), .prose("\nafter")
        ])
    }

    @Test func pendingPermissionsAndSubagentsMatchCurrentDerivedProperties() {
        let permission = PermissionState(
            requestId: .string("request"),
            options: [PermissionOption(optionId: "allow", name: "Allow", kind: .allowOnce)])
        var subagent = ToolCallItem(toolCallId: "subagent", title: "Task",
                                    kind: .execute, status: .inProgress,
                                    permission: permission)
        subagent.rawInput = .object([
            "subagent_type": .string("Explore"),
            "description": .string("Inspect the repository")
        ])
        let items: [TranscriptItem] = [.toolCall(subagent)]

        let snapshot = ChatPresentationSnapshot.build(items: items).snapshot

        #expect(snapshot.composerPermissions == ComposerPermissions.extract(from: items))
        #expect(snapshot.activeSubagentTasks == SubagentTasks.extract(from: items))
        #expect(snapshot.hasPendingPermission == hasPendingPermission(in: items))
    }

    private func currentActivity(in items: [TranscriptItem]) -> String? {
        for item in items.reversed() {
            guard case .toolCall(let call) = item else { continue }
            if call.status == .pending || call.status == .inProgress { return call.title }
        }
        return nil
    }

    private func hasPendingPermission(in items: [TranscriptItem]) -> Bool {
        items.contains {
            if case .toolCall(let item) = $0 { return item.permission?.isPending == true }
            return false
        }
    }
}
