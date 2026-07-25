import Foundation
import Testing
@testable import TillerACP

@Suite struct ToolCallTreeTests {
    func call(_ id: String, parent: String? = nil,
              kind: ToolKind = .read) -> TranscriptItem {
        var item = ToolCallItem(toolCallId: id, title: id, kind: kind,
                                status: .completed)
        item.parentToolCallId = parent
        return .toolCall(item)
    }

    @Test func childrenAreLiftedOutOfRoots() {
        let items = [call("task"), call("c1", parent: "task"), call("c2", parent: "task")]
        let grouped = ToolCallTree.group(items: items)
        #expect(grouped.roots.count == 1)
        #expect(grouped.roots.first?.id == "task")
        #expect(grouped.children(of: "task").map(\.toolCallId) == ["c1", "c2"])
    }

    @Test func orphanChildStaysARoot() {
        let grouped = ToolCallTree.group(items: [call("c1", parent: "missing")])
        #expect(grouped.roots.count == 1)
        #expect(grouped.roots.first?.id == "c1")
        #expect(grouped.children(of: "missing").isEmpty)
    }

    @Test func childArrivingBeforeParentIsStillNested() {
        let items = [call("c1", parent: "task"), call("task")]
        let grouped = ToolCallTree.group(items: items)
        #expect(grouped.roots.map(\.id) == ["task"])
        #expect(grouped.children(of: "task").map(\.toolCallId) == ["c1"])
    }

    @Test func noItemIsEverLost() {
        let items = [
            .userMessage(id: "u1", blocks: []),
            call("task"), call("c1", parent: "task"), call("c2", parent: "nope"),
            .agentMessage(id: "a1", text: "hi", isComplete: true),
        ] as [TranscriptItem]
        let grouped = ToolCallTree.group(items: items)
        let nested = grouped.children.values.reduce(0) { $0 + $1.count }
        #expect(grouped.roots.count + nested == items.count)
    }

    @Test func nonToolItemsKeepTheirOrder() {
        let items: [TranscriptItem] = [
            .userMessage(id: "u1", blocks: []),
            call("task"),
            .agentMessage(id: "a1", text: "hi", isComplete: true),
        ]
        #expect(ToolCallTree.group(items: items).roots.map(\.id) == ["u1", "task", "a1"])
    }

    @Test func pendingPlanApprovalIsFoundInTheSamePass() {
        var switchMode = ToolCallItem(toolCallId: "sm", title: "exit plan",
                                      kind: .switchMode, status: .pending)
        switchMode.permission = PermissionState(
            requestId: .string("r1"),
            options: [PermissionOption(optionId: "allow_once", name: "Approve",
                                       kind: .allowOnce)])
        let grouped = ToolCallTree.group(items: [.toolCall(switchMode)])
        #expect(grouped.pendingPlanApproval?.requestId == .string("r1"))
    }

    @Test func resolvedPlanApprovalIsNotReported() {
        var switchMode = ToolCallItem(toolCallId: "sm", title: "exit plan",
                                      kind: .switchMode, status: .completed)
        switchMode.permission = PermissionState(
            requestId: .string("r1"), options: [],
            resolution: .selected(optionId: "allow_once"))
        #expect(ToolCallTree.group(items: [.toolCall(switchMode)]).pendingPlanApproval == nil)
    }
}
