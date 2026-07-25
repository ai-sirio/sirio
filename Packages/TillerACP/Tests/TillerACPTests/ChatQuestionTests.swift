import Foundation
import Testing
@testable import TillerACP

@Suite struct ChatQuestionTests {
    @Test func buildsFromAPermissionRequest() {
        var call = ToolCallItem(toolCallId: "t1", title: "Write config.toml",
                                kind: .edit, status: .pending)
        call.permission = PermissionState(
            requestId: .string("r1"),
            options: [PermissionOption(optionId: "allow_once", name: "Allow once",
                                       kind: .allowOnce),
                      PermissionOption(optionId: "reject_once", name: "Reject",
                                       kind: .rejectOnce)])
        let question = ChatQuestion.from(call)
        #expect(question?.header == "Write config.toml")
        #expect(question?.options.map(\.id) == ["allow_once", "reject_once"])
        #expect(question?.options.last?.isRejection == true)
        #expect(question?.chosenOptionId == nil)
    }

    @Test func buildsFromAnAskUserQuestionInput() {
        var call = ToolCallItem(toolCallId: "t1", title: "AskUserQuestion",
                                kind: .other, status: .pending)
        call.rawInput = .object([
            "questions": .array([.object([
                "header": .string("Storage"),
                "question": .string("Which backend?"),
                "options": .array([
                    .object(["label": .string("SQLite"),
                             "description": .string("durable")]),
                    .object(["label": .string("In-memory")]),
                ]),
            ])]),
        ])
        call.permission = PermissionState(requestId: .string("r1"), options: [])
        let question = ChatQuestion.from(call)
        #expect(question?.header == "Storage")
        #expect(question?.prompt == "Which backend?")
        #expect(question?.options.map(\.label) == ["SQLite", "In-memory"])
        #expect(question?.options.first?.detail == "durable")
        #expect(question?.options.allSatisfy { !$0.isRejection } == true)
    }

    @Test func reportsTheChosenOption() {
        var call = ToolCallItem(toolCallId: "t1", title: "Write", kind: .edit,
                                status: .completed)
        call.permission = PermissionState(
            requestId: .string("r1"),
            options: [PermissionOption(optionId: "allow_once", name: "Allow once",
                                       kind: .allowOnce)],
            resolution: .selected(optionId: "allow_once"))
        #expect(ChatQuestion.from(call)?.chosenOptionId == "allow_once")
    }

    @Test func aCancelledRequestIsResolvedButUnanswered() {
        var call = ToolCallItem(toolCallId: "t1", title: "Write", kind: .edit,
                                status: .failed)
        call.permission = PermissionState(
            requestId: .string("r1"),
            options: [PermissionOption(optionId: "allow_once", name: "Allow once",
                                       kind: .allowOnce)],
            resolution: .cancelled)
        let question = ChatQuestion.from(call)
        #expect(question?.isExpired == true)
        #expect(question?.isAnswered == false)
        #expect(question?.isResolved == true)
    }

    @Test func isNilWithoutAPermission() {
        let call = ToolCallItem(toolCallId: "t1", title: "Read", kind: .read,
                                status: .completed)
        #expect(ChatQuestion.from(call) == nil)
    }
}
