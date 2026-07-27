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

    @Test func disambiguatesRepeatedStructuredOptionLabels() {
        var call = ToolCallItem(toolCallId: "t1", title: "AskUserQuestion",
                                kind: .other, status: .pending)
        call.rawInput = .object([
            "questions": .array([.object([
                "question": .string("Which mode?"),
                "options": .array([
                    .object(["label": .string("Automatic")]),
                    .object(["label": .string("Automatic")]),
                ]),
            ])]),
        ])
        call.permission = PermissionState(requestId: .string("r1"), options: [])

        let ids = ChatQuestion.from(call)?.options.map(\.id) ?? []
        #expect(ids == ["Automatic", "Automatic-1"])
        #expect(Set(ids).count == ids.count)
    }

    @Test func structuredQuestionTakesPrecedenceOverPermissionOptions() {
        var call = ToolCallItem(toolCallId: "t1", title: "Permission fallback",
                                kind: .other, status: .pending)
        call.rawInput = .object([
            "questions": .array([.object([
                "header": .string("Structured header"),
                "question": .string("Which mode?"),
                "options": .array([
                    .object(["label": .string("Automatic")]),
                ]),
            ])]),
        ])
        call.permission = PermissionState(
            requestId: .string("r1"),
            options: [PermissionOption(optionId: "permission-id", name: "Permission option",
                                       kind: .allowOnce)])

        let question = ChatQuestion.from(call)
        #expect(question?.header == "Structured header")
        #expect(question?.prompt == "Which mode?")
        #expect(question?.options.map(\.id) == ["Automatic"])
    }

    @Test func dropsMalformedStructuredOptionsButKeepsValidSiblings() {
        var call = ToolCallItem(toolCallId: "t1", title: "AskUserQuestion",
                                kind: .other, status: .pending)
        call.rawInput = .object([
            "questions": .array([.object([
                "header": .string("Storage"),
                "question": .string("Which backend?"),
                "options": .array([
                    .object(["label": .string("SQLite")]),
                    .object(["description": .string("missing label")]),
                ]),
            ])]),
        ])
        call.permission = PermissionState(
            requestId: .string("r1"),
            options: [PermissionOption(optionId: "permission-id", name: "Permission fallback",
                                       kind: .allowOnce)])

        let question = ChatQuestion.from(call)
        #expect(question?.options.map(\.label) == ["SQLite"])
        #expect(question?.options.map(\.id) == ["SQLite"])
    }

    @Test func fallsBackToPermissionOptionsWhenAllStructuredOptionsAreMalformed() {
        var call = ToolCallItem(toolCallId: "t1", title: "Permission fallback",
                                kind: .other, status: .pending)
        call.rawInput = .object([
            "questions": .array([.object([
                "header": .string("Storage"),
                "question": .string("Which backend?"),
                "options": .array([
                    .object(["description": .string("missing label")]),
                    .object(["description": .string("still missing label")]),
                ]),
            ])]),
        ])
        call.permission = PermissionState(
            requestId: .string("r1"),
            options: [PermissionOption(optionId: "permission-id", name: "Permission fallback",
                                       kind: .allowOnce)])

        let question = ChatQuestion.from(call)
        #expect(question?.header == "Permission fallback")
        #expect(question?.prompt == "")
        #expect(question?.options.map(\.id) == ["permission-id"])
        #expect(question?.options.map(\.label) == ["Permission fallback"])
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

    @Test func structuredTextQuestionAllowsNoOptionsAndCarriesPlaceholder() throws {
        var call = ToolCallItem(toolCallId: "pi-ui-input", title: "Enter branch",
                                kind: .other, status: .pending)
        call.rawInput = .object([
            "questions": .array([.object([
                "header": .string("Enter branch"),
                "question": .string("Branch name"),
                "options": .array([])
            ])]),
            "_tillerTextInput": .object([
                "placeholder": .string("feature/native-pi"),
                "prefill": .string("feature/")
            ])
        ])
        call.permission = PermissionState(requestId: .string("ui-1"), options: [])

        let question = try #require(ChatQuestion.from(call))
        #expect(question.options.isEmpty)
        #expect(question.textInput == .init(
            placeholder: "feature/native-pi", prefill: "feature/"))
    }

    @Test func structuredOptionCanBeMarkedAsRejection() throws {
        var call = ToolCallItem(toolCallId: "pi-ui-select", title: "Choose",
                                kind: .other, status: .pending)
        call.rawInput = .object(["questions": .array([.object([
            "question": .string("Choose"),
            "options": .array([
                .object(["id": .string("proceed"), "label": .string("Proceed")]),
                .object(["id": .string("__cancel__"), "label": .string("Cancel"),
                         "isRejection": .bool(true)])
            ])
        ])])])
        call.permission = PermissionState(requestId: .string("ui-2"), options: [])

        let question = try #require(ChatQuestion.from(call))
        #expect(question.options.last?.id == "__cancel__")
        #expect(question.options.last?.isRejection == true)
    }

    @Test func isNilWithoutAPermission() {
        let call = ToolCallItem(toolCallId: "t1", title: "Read", kind: .read,
                                status: .completed)
        #expect(ChatQuestion.from(call) == nil)
    }
}
