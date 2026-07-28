import Testing
import Foundation
@testable import TillerCore

private struct Row: Identifiable, Equatable {
    let id: UUID
    let status: AgentStatus?
}

@Suite struct UrgentFirstSortTests {
    @Test func manualOrderIsPreservedWhenNothingIsUrgent() {
        // Arrange: running e done non sono urgenti — non devono risalire.
        let list = [
            Row(id: UUID(), status: nil),
            Row(id: UUID(), status: .running),
            Row(id: UUID(), status: .done)
        ]

        // Act
        let sorted = AttentionSort.urgentFirst(list, statusOf: \.status)

        // Assert
        #expect(sorted == list)
    }

    @Test func errorAndNeedsInputFloatToTheTop() {
        // Arrange
        let idle = Row(id: UUID(), status: nil)
        let running = Row(id: UUID(), status: .running)
        let needsInput = Row(id: UUID(), status: .needsInput)
        let failed = Row(id: UUID(), status: .error)
        let list = [idle, running, needsInput, failed]

        // Act
        let sorted = AttentionSort.urgentFirst(list, statusOf: \.status)

        // Assert: error prima di needsInput, il resto intatto sotto.
        #expect(sorted == [failed, needsInput, idle, running])
    }

    @Test func urgentRowsKeepTheirRelativeManualOrderWithinTheSameStatus() {
        // Arrange: due needsInput, l'ordine manuale fra pari deve reggere.
        let first = Row(id: UUID(), status: .needsInput)
        let second = Row(id: UUID(), status: .needsInput)
        let idle = Row(id: UUID(), status: nil)
        let list = [idle, first, second]

        // Act
        let sorted = AttentionSort.urgentFirst(list, statusOf: \.status)

        // Assert
        #expect(sorted == [first, second, idle])
    }
}
