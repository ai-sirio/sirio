import Testing
import Foundation
@testable import TillerCore

@Test func evictsNothingWhenUnderCap() {
    let ids = [UUID(), UUID()]
    let evicted = WorktreeMountPolicy.idsToEvict(
        openWorktreeIds: ids,
        selectedWorktreeId: nil,
        cap: 5,
        status: { _ in nil },
        hasUnsavedWork: { _ in false }
    )
    #expect(evicted.isEmpty)
}

@Test func evictsNothingWhenCapDisabled() {
    let ids = [UUID(), UUID(), UUID()]
    let evicted = WorktreeMountPolicy.idsToEvict(
        openWorktreeIds: ids,
        selectedWorktreeId: nil,
        cap: 0,
        status: { _ in nil },
        hasUnsavedWork: { _ in false }
    )
    #expect(evicted.isEmpty)
}

@Test func evictsOldestIdleWorktreeFirstWhenOverCap() {
    let a = UUID()
    let b = UUID()
    let c = UUID()
    let evicted = WorktreeMountPolicy.idsToEvict(
        openWorktreeIds: [a, b, c],
        selectedWorktreeId: nil,
        cap: 2,
        status: { _ in nil },
        hasUnsavedWork: { _ in false }
    )
    #expect(evicted == [a])
}

@Test func neverEvictsSelectedWorktree() {
    let a = UUID()
    let b = UUID()
    let evicted = WorktreeMountPolicy.idsToEvict(
        openWorktreeIds: [a, b],
        selectedWorktreeId: a,
        cap: 1,
        status: { _ in nil },
        hasUnsavedWork: { _ in false }
    )
    #expect(evicted == [b])
}

@Test func neverEvictsWorktreeWithRunningAgent() {
    let a = UUID()
    let b = UUID()
    let evicted = WorktreeMountPolicy.idsToEvict(
        openWorktreeIds: [a, b],
        selectedWorktreeId: nil,
        cap: 1,
        status: { $0 == a ? .running : nil },
        hasUnsavedWork: { _ in false }
    )
    #expect(evicted == [b])
}

@Test func neverEvictsWorktreeWaitingOnInput() {
    let a = UUID()
    let b = UUID()
    let evicted = WorktreeMountPolicy.idsToEvict(
        openWorktreeIds: [a, b],
        selectedWorktreeId: nil,
        cap: 1,
        status: { $0 == a ? .needsInput : nil },
        hasUnsavedWork: { _ in false }
    )
    #expect(evicted == [b])
}

@Test func evictsWorktreeWithFinishedOrErroredAgent() {
    let a = UUID()
    let b = UUID()
    let evicted = WorktreeMountPolicy.idsToEvict(
        openWorktreeIds: [a, b],
        selectedWorktreeId: nil,
        cap: 1,
        status: { $0 == a ? .done : .error },
        hasUnsavedWork: { _ in false }
    )
    #expect(evicted == [a])
}

@Test func neverEvictsWorktreeWithUnsavedWork() {
    let a = UUID()
    let b = UUID()
    let evicted = WorktreeMountPolicy.idsToEvict(
        openWorktreeIds: [a, b],
        selectedWorktreeId: nil,
        cap: 1,
        status: { _ in nil },
        hasUnsavedWork: { $0 == a }
    )
    #expect(evicted == [b])
}

@Test func stopsEvictingOnceCapIsSatisfied() {
    let a = UUID()
    let b = UUID()
    let c = UUID()
    let evicted = WorktreeMountPolicy.idsToEvict(
        openWorktreeIds: [a, b, c],
        selectedWorktreeId: nil,
        cap: 2,
        status: { _ in nil },
        hasUnsavedWork: { _ in false }
    )
    #expect(evicted == [a])
}
