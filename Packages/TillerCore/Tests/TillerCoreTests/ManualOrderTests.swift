import Testing
import Foundation
@testable import TillerCore

private struct Row: Identifiable, Equatable {
    let id: UUID
}

@Suite struct ManualOrderTests {
    private func rows(_ count: Int) -> [Row] {
        (0..<count).map { _ in Row(id: UUID()) }
    }

    @Test func movesRowBeforeTarget() {
        // Arrange
        let list = rows(3)

        // Act: la terza riga va prima della prima.
        let moved = ManualOrder.moving(list, id: list[2].id, before: list[0].id)

        // Assert
        #expect(moved.map(\.id) == [list[2].id, list[0].id, list[1].id])
    }

    @Test func movingForwardAccountsForTheShiftedTargetIndex() {
        // Arrange
        let list = rows(3)

        // Act: la prima riga va prima della terza; dopo la rimozione
        // l'indice del target scala di uno.
        let moved = ManualOrder.moving(list, id: list[0].id, before: list[2].id)

        // Assert
        #expect(moved.map(\.id) == [list[1].id, list[0].id, list[2].id])
    }

    @Test func nilTargetAppendsToTheEnd() {
        // Arrange
        let list = rows(3)

        // Act
        let moved = ManualOrder.moving(list, id: list[0].id, before: nil)

        // Assert
        #expect(moved.map(\.id) == [list[1].id, list[2].id, list[0].id])
    }

    @Test func unknownIdOrTargetLeavesTheListUnchanged() {
        // Arrange
        let list = rows(3)
        let stranger = UUID()

        // Act
        let unknownSource = ManualOrder.moving(list, id: stranger, before: list[0].id)
        let unknownTarget = ManualOrder.moving(list, id: list[0].id, before: stranger)

        // Assert
        #expect(unknownSource == list)
        #expect(unknownTarget == list)
    }

    @Test func droppingARowOnItselfLeavesTheListUnchanged() {
        // Arrange
        let list = rows(3)

        // Act
        let moved = ManualOrder.moving(list, id: list[1].id, before: list[1].id)

        // Assert
        #expect(moved == list)
    }
}
