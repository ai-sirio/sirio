import Testing
import Foundation
@testable import TillerACP

@Suite("TimelineBuilder")
struct TimelineBuilderTests {
    func user(_ id: String, _ text: String) -> TranscriptItem {
        .userMessage(id: id, blocks: [.text(text)])
    }
    func agent(_ id: String, _ text: String, complete: Bool = true) -> TranscriptItem {
        .agentMessage(id: id, text: text, isComplete: complete)
    }
    func tool(_ id: String, title: String = "Read file",
              kind: ToolKind = .read,
              status: ToolCallStatus = .completed,
              permission: PermissionState? = nil) -> TranscriptItem {
        .toolCall(ToolCallItem(toolCallId: id, title: title, kind: kind,
                               status: status, permission: permission))
    }
    func divider(_ id: String, at: Date = Date(timeIntervalSince1970: 100)) -> TranscriptItem {
        .turnDivider(id: id, at: at)
    }

    @Test func mapsPlainMessagesToMessageRows() {
        let rows = TimelineBuilder.rows(
            items: [user("u1", "hi"), agent("a1", "hello")],
            state: TimelineState())
        #expect(rows.count == 2)
        guard case .message(let first, _) = rows[0] else { Issue.record("expected message"); return }
        #expect(first.id == "u1")
        guard case .message(let second, _) = rows[1] else { Issue.record("expected message"); return }
        #expect(second.id == "a1")
    }

    @Test func closedTurnKeepsItsDividerRow() {
        let rows = TimelineBuilder.rows(
            items: [user("u1", "hi"), agent("a1", "done"), divider("d1")],
            state: TimelineState())
        guard case .turnDivider(let id, _) = rows.last else {
            Issue.record("expected trailing divider"); return
        }
        #expect(id == "d1")
    }

    @Test func streamingAppendsWorkingRow() {
        let rows = TimelineBuilder.rows(
            items: [user("u1", "hi")],
            state: TimelineState(isStreaming: true))
        #expect(rows.last == .working)
    }

    @Test func notStreamingHasNoWorkingRow() {
        let rows = TimelineBuilder.rows(items: [user("u1", "hi")], state: TimelineState())
        #expect(!rows.contains(.working))
    }

    @Test func rowIdsAreStableAndUnique() {
        let rows = TimelineBuilder.rows(
            items: [user("u1", "hi"), agent("a1", "hello"), divider("d1")],
            state: TimelineState())
        #expect(Set(rows.map(\.id)).count == rows.count)
    }

    @Test func consecutiveToolCallsFormOneWorkGroup() {
        let rows = TimelineBuilder.rows(
            items: [user("u1", "go"), tool("t1"), tool("t2"), tool("t3")],
            state: TimelineState())
        let workRows = rows.compactMap { row -> [TimelineRow.WorkEntry]? in
            if case .work(_, let entries, _) = row { return entries }
            return nil
        }
        #expect(workRows.count == 1)
        #expect(workRows[0].count == 3)
        #expect(workRows[0].map(\.id) == ["t1", "t2", "t3"])
    }

    @Test func workGroupIdIsFirstToolCallId() {
        let rows = TimelineBuilder.rows(
            items: [tool("t1"), tool("t2")], state: TimelineState())
        guard case .work(let groupId, _, let isExpanded) = rows[0] else {
            Issue.record("expected work row"); return
        }
        #expect(groupId == "wg-t1")
        #expect(isExpanded == false)
    }

    @Test func expandedStateFollowsTimelineState() {
        let rows = TimelineBuilder.rows(
            items: [tool("t1"), tool("t2")],
            state: TimelineState(expandedWorkGroups: ["wg-t1"]))
        guard case .work(_, _, let isExpanded) = rows[0] else {
            Issue.record("expected work row"); return
        }
        #expect(isExpanded == true)
    }

    @Test func interleavedMessageSplitsWorkGroups() {
        let rows = TimelineBuilder.rows(
            items: [tool("t1"), agent("a1", "half"), tool("t2")],
            state: TimelineState())
        let groupIds = rows.compactMap { row -> String? in
            if case .work(let id, _, _) = row { return id }
            return nil
        }
        #expect(groupIds == ["wg-t1", "wg-t2"])
    }

    @Test func compactLabelStripsTrailingCompleted() {
        #expect(TimelineRow.WorkEntry.compactLabel("Read AppModel.swift completed")
                == "Read AppModel.swift")
        #expect(TimelineRow.WorkEntry.compactLabel("Read AppModel.swift")
                == "Read AppModel.swift")
        #expect(TimelineRow.WorkEntry.compactLabel("multi\nline title") == "multi")
    }

    @Test func olderTurnsFoldKeepingLastTwoOpen() {
        // three closed turns + one open turn = 4 turns; first two fold
        let items: [TranscriptItem] = [
            user("u1", "first question"), agent("a1", "r1"), divider("d1"),
            user("u2", "second question"), agent("a2", "r2"), divider("d2"),
            user("u3", "third question"), agent("a3", "r3"), divider("d3"),
            user("u4", "fourth question"),
        ]
        let rows = TimelineBuilder.rows(items: items, state: TimelineState())
        let foldIds = rows.compactMap { row -> String? in
            if case .turnFold(let turnId, _, _) = row { return turnId }
            return nil
        }
        #expect(foldIds == ["d1", "d2"])
        // folded turns emit no divider row
        let dividerIds = rows.compactMap { row -> String? in
            if case .turnDivider(let id, _) = row { return id }
            return nil
        }
        #expect(dividerIds == ["d3"])
        // open turns keep their message rows
        #expect(rows.contains { $0.id == "msg-u3" })
        #expect(rows.contains { $0.id == "msg-u4" })
        #expect(!rows.contains { $0.id == "msg-u1" })
    }

    @Test func unfoldedTurnRendersItsRows() {
        let items: [TranscriptItem] = [
            user("u1", "first"), agent("a1", "r1"), divider("d1"),
            user("u2", "second"), divider("d2"),
            user("u3", "third"), divider("d3"),
        ]
        let rows = TimelineBuilder.rows(
            items: items, state: TimelineState(unfoldedTurns: ["d1"]))
        #expect(rows.contains { $0.id == "msg-u1" })
        #expect(!rows.contains { row in
            if case .turnFold(let id, _, _) = row { return id == "d1" }
            return false
        })
    }

    @Test func foldLabelComesFromFirstUserMessage() {
        let long = String(repeating: "x", count: 100)
        let items: [TranscriptItem] = [
            user("u1", long), divider("d1"),
            user("u2", "b"), divider("d2"),
            user("u3", "c"), divider("d3"),
        ]
        let rows = TimelineBuilder.rows(items: items, state: TimelineState())
        guard case .turnFold(_, let label, _) = rows[0] else {
            Issue.record("expected fold"); return
        }
        #expect(label.count == 60)
    }

    @Test func finalAssistantMessageCarriesDurationAndCopy() {
        let items: [TranscriptItem] = [
            user("u1", "go"), agent("a1", "partial"), tool("t1"),
            agent("a2", "final"), divider("d1"),
        ]
        let rows = TimelineBuilder.rows(
            items: items,
            state: TimelineState(turnDurations: ["d1": 42]))
        func meta(_ rowId: String) -> TimelineRow.MessageMeta?? {
            for row in rows {
                if case .message(let item, let meta) = row, "msg-\(item.id)" == rowId {
                    return meta
                }
            }
            return nil
        }
        #expect(meta("msg-a1") == .some(nil))
        let final = meta("msg-a2")
        #expect(final??.duration == 42)
        #expect(final??.showsCopyButton == true)
    }

    @Test func streamingOpenTurnHidesCopyButton() {
        let items: [TranscriptItem] = [
            user("u1", "go"), agent("a1", "typing", complete: false),
        ]
        let rows = TimelineBuilder.rows(
            items: items, state: TimelineState(isStreaming: true))
        guard case .message(_, let meta) = rows[1] else {
            Issue.record("expected message"); return
        }
        #expect(meta?.showsCopyButton == false)
        #expect(meta?.duration == nil)
    }

    @Test func proposedPlanCarriesPendingSwitchModeApproval() {
        let approval = PermissionState(
            requestId: .string("exit"),
            options: [
                PermissionOption(optionId: "yes", name: "Approve", kind: .allowOnce),
                PermissionOption(optionId: "no", name: "Reject", kind: .rejectOnce),
            ])
        let items: [TranscriptItem] = [
            user("u1", "plan it"),
            .plan(id: "pl1", entries: [PlanEntry(content: "step 1", priority: "medium",
                                                 status: "pending")]),
            tool("exit-tool", title: "Exit plan mode", kind: .switchMode,
                 status: .pending, permission: approval),
        ]
        let rows = TimelineBuilder.rows(items: items, state: TimelineState())
        guard let planRow = rows.first(where: { row in
            if case .proposedPlan = row { return true }
            return false
        }), case .proposedPlan(_, let entries, let rowApproval) = planRow else {
            Issue.record("expected proposedPlan row"); return
        }
        #expect(entries.count == 1)
        #expect(rowApproval?.requestId == .string("exit"))
    }
}
