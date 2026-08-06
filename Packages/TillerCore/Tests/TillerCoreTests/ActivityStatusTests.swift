import Foundation
import Testing

@testable import TillerCore

@Suite struct ActivityStatusTests {
    @Test func everyAgentStatusMapsToItsOwnCase() {
        #expect(ActivityStatus.from(.running) == .running)
        #expect(ActivityStatus.from(.needsInput) == .needsInput)
        #expect(ActivityStatus.from(.done) == .done)
        #expect(ActivityStatus.from(.error) == .error)
    }

    /// A terminal running a bare shell has no agent and therefore no
    /// AgentStatus at all. It must still produce a row, so the absent status
    /// becomes `.idle` instead of disqualifying the tab.
    @Test func absentAgentStatusBecomesIdle() {
        #expect(ActivityStatus.from(nil) == .idle)
    }

    @Test func onlyLiveStatesAskBeforeClosing() {
        #expect(ActivityStatus.running.requiresCloseConfirmation)
        #expect(ActivityStatus.needsInput.requiresCloseConfirmation)
        #expect(ActivityStatus.error.requiresCloseConfirmation)
        #expect(!ActivityStatus.done.requiresCloseConfirmation)
        #expect(!ActivityStatus.idle.requiresCloseConfirmation)
    }
}
