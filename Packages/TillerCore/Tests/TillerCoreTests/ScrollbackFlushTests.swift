import Testing
import Foundation
@testable import TillerCore

@Test func flushTargetsSinglePaneYieldsOnePair() {
    let wt = UUID(); let pane = UUID()
    let targets = scrollbackFlushTargets(paneIds: [wt: [pane]])

    #expect(targets.count == 1)
    #expect(targets[0].worktreeId == wt)
    #expect(targets[0].paneId == pane)
}

@Test func flushTargetsMultiplePanesYieldPairPerPane() {
    let wt = UUID(); let a = UUID(); let b = UUID()
    let targets = scrollbackFlushTargets(paneIds: [wt: [a, b]])

    #expect(targets.count == 2)
    #expect(Set(targets.map(\.paneId)) == Set([a, b]))
    #expect(targets.allSatisfy { $0.worktreeId == wt })
}

@Test func flushTargetsMultipleWorktreesKeepCorrectWorktreeId() {
    let wt1 = UUID(); let wt2 = UUID(); let p1 = UUID(); let p2 = UUID()
    let targets = scrollbackFlushTargets(paneIds: [wt1: [p1], wt2: [p2]])

    #expect(targets.count == 2)
    #expect(targets.contains { $0.worktreeId == wt1 && $0.paneId == p1 })
    #expect(targets.contains { $0.worktreeId == wt2 && $0.paneId == p2 })
}

@Test func flushTargetsEmptyYieldsNothing() {
    #expect(scrollbackFlushTargets(paneIds: [:]).isEmpty)
}

@Test func flushTargetsEmptyPaneListYieldsNothing() {
    let wt = UUID()
    #expect(scrollbackFlushTargets(paneIds: [wt: []]).isEmpty)
}
