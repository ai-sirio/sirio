import SwiftUI
import Testing
@testable import Tiller

@Suite("AgentAccentColor")
struct AgentAccentColorTests {
    @Test func defaultHexMatchesEachKnownAgent() {
        #expect(AgentAccentColor.defaultHex(for: "claude") == "D97757")
        #expect(AgentAccentColor.defaultHex(for: "codex") == "0A84FF")
        #expect(AgentAccentColor.defaultHex(for: "omp") == "9B4DFF")
        #expect(AgentAccentColor.defaultHex(for: "opencode") == "FF9500")
        #expect(AgentAccentColor.defaultHex(for: "pi") == "34C759")
    }

    @Test func defaultHexFallsBackToNeutralGrayForUnknownAgent() {
        #expect(AgentAccentColor.defaultHex(for: "some-future-agent") == "8E8E93")
    }

    @Test func resolvedHexPrefersAValidStoredValue() {
        #expect(AgentAccentColor.resolvedHex(for: "claude", storedValue: "#112233") == "#112233")
    }

    @Test func resolvedHexFallsBackToDefaultWhenNothingIsStored() {
        #expect(AgentAccentColor.resolvedHex(for: "claude", storedValue: nil) == "D97757")
    }

    @Test func resolvedHexFallsBackToDefaultWhenStoredValueIsMalformed() {
        #expect(AgentAccentColor.resolvedHex(for: "codex", storedValue: "not-a-color") == "0A84FF")
        #expect(AgentAccentColor.resolvedHex(for: "codex", storedValue: "") == "0A84FF")
    }

    @Test func colorForAgentDecodesTheResolvedHex() {
        #expect(AgentAccentColor.color(for: "pi", storedValue: nil) == Color(hex: "34C759"))
        #expect(AgentAccentColor.color(for: "pi", storedValue: "#000000") == Color(hex: "#000000"))
    }
}
