import SwiftUI
import Testing

@testable import Tiller

struct AgentStatusDotColorTests {
    @Test func readyIsGreen() {
        #expect(agentStatusDotColor(for: .ready) == .green)
    }

    @Test func promptingIsOrange() {
        #expect(agentStatusDotColor(for: .prompting) == .orange)
    }

    @Test func inactiveStatesAreGray() {
        #expect(agentStatusDotColor(for: .idle) == .gray)
        #expect(agentStatusDotColor(for: .connecting) == .gray)
        #expect(agentStatusDotColor(for: .needsAuth) == .gray)
        #expect(agentStatusDotColor(for: .disconnected(message: nil)) == .gray)
    }
}
