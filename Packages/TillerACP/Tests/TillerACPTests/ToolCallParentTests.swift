import Foundation
import Testing
@testable import TillerACP

@Suite struct ToolCallParentTests {
    @Test func legacyToolCallDecodesWithoutParent() throws {
        let json = Data(#"{"toolCallId":"t1","title":"Read","kind":"read","status":"completed"}"#.utf8)
        let call = try JSONDecoder().decode(ToolCall.self, from: json)
        #expect(call.parentToolCallId == nil)
    }

    @Test func parentSurvivesEncodeDecode() throws {
        let call = ToolCall(toolCallId: "child", title: "Read", kind: .read,
                            status: .completed, parentToolCallId: "task-1")
        let data = try JSONEncoder().encode(call)
        let decoded = try JSONDecoder().decode(ToolCall.self, from: data)
        #expect(decoded.parentToolCallId == "task-1")
    }

    @Test func itemInheritsParentFromCall() {
        let call = ToolCall(toolCallId: "child", title: "Read", kind: .read,
                            status: .completed, parentToolCallId: "task-1")
        let item = ToolCallItem(call)
        #expect(item.parentToolCallId == "task-1")
    }

    @Test func mergeAdoptsParentWhenUpdateCarriesIt() {
        var item = ToolCallItem(toolCallId: "child", title: "Read", kind: .read,
                                status: .pending)
        item.merge(ToolCallUpdate(toolCallId: "child", status: .completed,
                                  parentToolCallId: "task-1"))
        #expect(item.parentToolCallId == "task-1")
        #expect(item.status == .completed)
    }

    @Test func mergeKeepsExistingParentWhenUpdateOmitsIt() {
        var item = ToolCallItem(toolCallId: "child", title: "Read", kind: .read,
                                status: .pending)
        item.parentToolCallId = "task-1"
        item.merge(ToolCallUpdate(toolCallId: "child", status: .completed))
        #expect(item.parentToolCallId == "task-1")
    }
}
