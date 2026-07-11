import Testing
@testable import TillerControl

@Suite struct NotifyModeTests {
    @Test func agentStatusMode() throws {
        #expect(try NotifyMode.resolve(
            session: "P", status: "running", title: nil, body: nil) == .agentStatus)
    }

    @Test func userNotificationMode() throws {
        #expect(try NotifyMode.resolve(
            session: nil, status: nil, title: "T", body: "B") == .userNotification)
    }

    @Test func bothModesIsAmbiguous() {
        #expect(throws: NotifyModeError.ambiguousMode) {
            try NotifyMode.resolve(session: "P", status: "s", title: "T", body: "B")
        }
    }

    @Test func neitherModeIsError() {
        #expect(throws: NotifyModeError.missingMode) {
            try NotifyMode.resolve(session: nil, status: nil, title: nil, body: nil)
        }
    }

    @Test func sessionWithoutStatusIsError() {
        #expect(throws: NotifyModeError.missingStatus) {
            try NotifyMode.resolve(session: "P", status: nil, title: nil, body: nil)
        }
    }

    @Test func titleWithoutBodyIsError() {
        #expect(throws: NotifyModeError.missingBody) {
            try NotifyMode.resolve(session: nil, status: nil, title: "T", body: nil)
        }
    }
}
