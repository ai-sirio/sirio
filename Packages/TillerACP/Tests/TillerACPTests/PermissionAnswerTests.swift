import Foundation
import Testing
@testable import TillerACP

@Suite struct PermissionAnswerTests {
    @Test func answeredEncodesAsSelectedForTheACPWire() throws {
        let outcome = PermissionOutcome.answered(
            optionId: "SQLite", updatedInput: .object(["choice": .string("SQLite")]))
        let data = try JSONEncoder().encode(outcome)
        let decoded = try JSONDecoder().decode(PermissionOutcome.self, from: data)
        #expect(decoded == .selected(optionId: "SQLite"))
    }

    /// Uses the target's existing `MockTransport`, whose `sent` records every
    /// line the driver wrote.
    func sentLines(answering outcome: PermissionOutcome) async -> [String] {
        let transport = MockTransport()
        let driver = ClaudeStreamJSONDriver(transport: transport,
                                            permissionMode: .ask,
                                            model: nil, resumeSessionId: nil)
        await driver.answerPermission(requestId: .string("r1"), outcome: outcome)
        return await transport.sent.map { String(decoding: $0, as: UTF8.self) }
    }

    @Test func controlResponseCarriesUpdatedInput() async {
        let sent = await sentLines(answering: .answered(
            optionId: "SQLite", updatedInput: .object(["choice": .string("SQLite")])))
        #expect(sent.contains { $0.contains("\"behavior\":\"allow\"") })
        #expect(sent.contains { $0.contains("updatedInput") })
        #expect(sent.contains { $0.contains("SQLite") })
    }

    @Test func plainSelectionStillSendsNoUpdatedInput() async {
        let sent = await sentLines(answering: .selected(optionId: "allow_once"))
        #expect(sent.contains { $0.contains("\"behavior\":\"allow\"") })
        #expect(sent.allSatisfy { !$0.contains("updatedInput") })
    }
}
