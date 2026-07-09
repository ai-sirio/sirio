import Testing
@testable import TillerCore

/// Lightweight Identifiable fixture: id proves stability, status drives rank.
private struct Item: Identifiable {
    let id: Int
    let status: AgentStatus?
}

private func sorted(_ items: [Item]) -> [Int] {
    AttentionSort.sorted(items, statusOf: { $0.status }).map(\.id)
}

@Test func errorFloatsAboveEverything() {
    let items = [Item(id: 1, status: .done), Item(id: 2, status: .error)]
    #expect(sorted(items) == [2, 1])
}

@Test func fullOrderingAcrossAllRanks() {
    // Deliberately shuffled input; expect priorityOrder then idle.
    let items = [
        Item(id: 1, status: nil),
        Item(id: 2, status: .done),
        Item(id: 3, status: .running),
        Item(id: 4, status: .needsInput),
        Item(id: 5, status: .error),
    ]
    #expect(sorted(items) == [5, 4, 3, 2, 1])
}

@Test func idleSinksToBottom() {
    let items = [Item(id: 1, status: nil), Item(id: 2, status: .running)]
    #expect(sorted(items) == [2, 1])
}

@Test func sameStatusPreservesInputOrder() {
    let items = [
        Item(id: 1, status: .needsInput),
        Item(id: 2, status: .needsInput),
        Item(id: 3, status: .needsInput),
    ]
    #expect(sorted(items) == [1, 2, 3])
}

@Test func emptyListReturnsEmpty() {
    #expect(sorted([]) == [])
}

@Test func allIdlePreservesInputOrder() {
    let items = [Item(id: 3, status: nil), Item(id: 1, status: nil), Item(id: 2, status: nil)]
    #expect(sorted(items) == [3, 1, 2])
}

@Test func singleItemReturnsItself() {
    #expect(sorted([Item(id: 7, status: .running)]) == [7])
}
