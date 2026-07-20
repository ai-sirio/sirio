import Testing

@testable import Tiller

@Suite("SlashCommandSelection")
struct SlashCommandSelectionTests {
    @Test func downMovesForwardAndClampsAtEnd() {
        #expect(SlashCommandSelection.effect(for: .down, index: 0, count: 3) == .moved(1))
        #expect(SlashCommandSelection.effect(for: .down, index: 2, count: 3) == .moved(2))
    }

    @Test func upMovesBackwardAndClampsAtZero() {
        #expect(SlashCommandSelection.effect(for: .up, index: 2, count: 3) == .moved(1))
        #expect(SlashCommandSelection.effect(for: .up, index: 0, count: 3) == .moved(0))
    }

    @Test func staleIndexIsClampedBeforeUse() {
        // List shrank from 5 to 2 while index was 4.
        #expect(SlashCommandSelection.effect(for: .tab, index: 4, count: 2) == .accepted(1))
        #expect(SlashCommandSelection.effect(for: .down, index: 4, count: 2) == .moved(1))
    }

    @Test func tabAndEnterAccept() {
        #expect(SlashCommandSelection.effect(for: .tab, index: 1, count: 3) == .accepted(1))
        #expect(SlashCommandSelection.effect(for: .enter, index: 1, count: 3) == .accepted(1))
    }

    @Test func escapeDismisses() {
        #expect(SlashCommandSelection.effect(for: .escape, index: 0, count: 3) == .dismissed)
    }

    @Test func emptyListYieldsNoEffect() {
        #expect(SlashCommandSelection.effect(for: .down, index: 0, count: 0) == nil)
    }
}
