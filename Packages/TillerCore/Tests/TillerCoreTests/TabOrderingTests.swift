import Testing
import Foundation
@testable import TillerCore

private func tabs(_ n: Int) -> [WorkspaceTab] {
    (0..<n).map { WorkspaceTab(id: UUID(), title: "T\($0)", tree: .leaf(id: UUID())) }
}

@Suite struct TabOrderingMovingTests {
    @Test func movesBeforeTarget() {
        let list = tabs(4)
        let moved = TabOrdering.moving(list, id: list[3].id, before: list[1].id)
        #expect(moved.map(\.id) == [list[0].id, list[3].id, list[1].id, list[2].id])
    }

    @Test func nilTargetMovesToEnd() {
        let list = tabs(3)
        let moved = TabOrdering.moving(list, id: list[0].id, before: nil)
        #expect(moved.map(\.id) == [list[1].id, list[2].id, list[0].id])
    }

    @Test func moveBeforeItselfIsNoOp() {
        let list = tabs(3)
        #expect(TabOrdering.moving(list, id: list[1].id, before: list[1].id).map(\.id) == list.map(\.id))
    }

    @Test func unknownIdIsNoOp() {
        let list = tabs(2)
        #expect(TabOrdering.moving(list, id: UUID(), before: list[0].id).map(\.id) == list.map(\.id))
    }

    @Test func unknownTargetIsNoOp() {
        let list = tabs(2)
        #expect(TabOrdering.moving(list, id: list[0].id, before: UUID()).map(\.id) == list.map(\.id))
    }

    @Test func moveForwardBeforeLaterTarget() {
        // Sposta la prima tab prima della terza: il target index va calcolato
        // DOPO la rimozione, non prima.
        let list = tabs(3)
        let moved = TabOrdering.moving(list, id: list[0].id, before: list[2].id)
        #expect(moved.map(\.id) == [list[1].id, list[0].id, list[2].id])
    }
}

@Suite struct TabOrderingCycleTests {
    @Test func forwardWraps() {
        #expect(TabOrdering.cycledIndex(current: 2, forward: true, count: 3) == 0)
        #expect(TabOrdering.cycledIndex(current: 0, forward: true, count: 3) == 1)
    }

    @Test func backwardWraps() {
        #expect(TabOrdering.cycledIndex(current: 0, forward: false, count: 3) == 2)
        #expect(TabOrdering.cycledIndex(current: 2, forward: false, count: 3) == 1)
    }

    @Test func nilCurrentStartsAtEdge() {
        #expect(TabOrdering.cycledIndex(current: nil, forward: true, count: 3) == 0)
        #expect(TabOrdering.cycledIndex(current: nil, forward: false, count: 3) == 2)
    }

    @Test func emptyListReturnsNil() {
        #expect(TabOrdering.cycledIndex(current: nil, forward: true, count: 0) == nil)
    }
}

@Suite struct TabOrderingSelectionTests {
    @Test func numberMapsToIndex() {
        #expect(TabOrdering.selectionIndex(number: 1, count: 5) == 0)
        #expect(TabOrdering.selectionIndex(number: 5, count: 5) == 4)
    }

    @Test func nineIsAlwaysLast() {
        #expect(TabOrdering.selectionIndex(number: 9, count: 3) == 2)
        #expect(TabOrdering.selectionIndex(number: 9, count: 12) == 11)
    }

    @Test func outOfRangeIsNil() {
        #expect(TabOrdering.selectionIndex(number: 5, count: 3) == nil)
        #expect(TabOrdering.selectionIndex(number: 0, count: 3) == nil)
        #expect(TabOrdering.selectionIndex(number: 1, count: 0) == nil)
        #expect(TabOrdering.selectionIndex(number: 9, count: 0) == nil)
    }
}
