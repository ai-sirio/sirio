import SwiftUI
import Testing
@testable import Tiller

@MainActor
@Suite("AgentAccentColorProvider")
struct AgentAccentColorProviderTests {
    @Test func resolvesTheStoredHexOverTheDefault() {
        let store = UserDefaults(suiteName: "AgentAccentColorProviderTests.stored.\(UUID().uuidString)")!
        store.set("#112233", forKey: AppSettings.agentColorKey(for: "claude"))

        var captured: Color?
        _ = AgentAccentColorProvider(agentId: "claude", store: store) { color -> Color in
            captured = color
            return color
        }.body

        #expect(captured == Color(hex: "#112233"))
    }

    @Test func fallsBackToTheAgentDefaultWhenNothingIsStored() {
        let store = UserDefaults(suiteName: "AgentAccentColorProviderTests.empty.\(UUID().uuidString)")!

        var captured: Color?
        _ = AgentAccentColorProvider(agentId: "codex", store: store) { color -> Color in
            captured = color
            return color
        }.body

        #expect(captured == Color(hex: AgentAccentColor.defaultHex(for: "codex")))
    }
}
