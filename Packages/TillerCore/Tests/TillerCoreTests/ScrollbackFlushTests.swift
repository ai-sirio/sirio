import Testing
import Foundation
@testable import TillerCore

@Test func flushTargetsSingleLeafYieldsOnePair() {
    let wt = UUID(); let pane = UUID()
    let tabs = [wt: [WorkspaceTab(id: UUID(), title: "Terminale 1", tree: .leaf(id: pane))]]

    let targets = scrollbackFlushTargets(tabs: tabs)

    #expect(targets.count == 1)
    #expect(targets[0].worktreeId == wt)
    #expect(targets[0].paneId == pane)
}

@Test func flushTargetsSplitYieldsPairPerLeaf() {
    let wt = UUID(); let a = UUID(); let b = UUID()
    let tree = SplitTree.leaf(id: a).splitting(leaf: a, axis: .horizontal, newLeaf: b)
    let tabs = [wt: [WorkspaceTab(id: UUID(), title: "Terminale 1", tree: tree)]]

    let targets = scrollbackFlushTargets(tabs: tabs)

    #expect(targets.count == 2)
    #expect(Set(targets.map(\.paneId)) == Set([a, b]))
    #expect(targets.allSatisfy { $0.worktreeId == wt })
}

@Test func flushTargetsMultipleWorktreesKeepCorrectWorktreeId() {
    let wt1 = UUID(); let wt2 = UUID(); let p1 = UUID(); let p2 = UUID()
    let tabs = [
        wt1: [WorkspaceTab(id: UUID(), title: "Terminale 1", tree: .leaf(id: p1))],
        wt2: [WorkspaceTab(id: UUID(), title: "Terminale 1", tree: .leaf(id: p2))],
    ]

    let targets = scrollbackFlushTargets(tabs: tabs)

    #expect(targets.count == 2)
    #expect(targets.contains { $0.worktreeId == wt1 && $0.paneId == p1 })
    #expect(targets.contains { $0.worktreeId == wt2 && $0.paneId == p2 })
}

@Test func flushTargetsEmptyTabsYieldsNothing() {
    #expect(scrollbackFlushTargets(tabs: [:]).isEmpty)
}

@Test func flushTargetsMultipleTabsPerWorktreeCoverAll() {
    let wt = UUID(); let p1 = UUID(); let p2 = UUID()
    let tabs = [wt: [
        WorkspaceTab(id: UUID(), title: "Terminale 1", tree: .leaf(id: p1)),
        WorkspaceTab(id: UUID(), title: "Terminale 2", tree: .leaf(id: p2)),
    ]]

    let targets = scrollbackFlushTargets(tabs: tabs)

    #expect(Set(targets.map(\.paneId)) == Set([p1, p2]))
}
