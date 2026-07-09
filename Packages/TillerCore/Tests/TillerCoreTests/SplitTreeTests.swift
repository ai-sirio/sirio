import Testing
import Foundation
@testable import TillerCore

@Test func splittingALeafProducesTwoLeaves() {
    let a = UUID(), b = UUID()
    let tree = SplitTree.leaf(id: a).splitting(leaf: a, axis: .horizontal, newLeaf: b)
    guard case let .split(axis, first, second) = tree else {
        Issue.record("expected .split"); return
    }
    #expect(axis == .horizontal)
    #expect(first == .leaf(id: a))
    #expect(second == .leaf(id: b))
}

@Test func splittingANestedLeafOnlyTouchesThatBranch() {
    let a = UUID(), b = UUID(), c = UUID()
    let tree = SplitTree.leaf(id: a)
        .splitting(leaf: a, axis: .horizontal, newLeaf: b)
        .splitting(leaf: b, axis: .vertical, newLeaf: c)
    guard case let .split(_, first, second) = tree,
          case let .split(innerAxis, innerFirst, innerSecond) = second else {
        Issue.record("expected nested .split"); return
    }
    #expect(first == .leaf(id: a))
    #expect(innerAxis == .vertical)
    #expect(innerFirst == .leaf(id: b))
    #expect(innerSecond == .leaf(id: c))
}

@Test func removingALeafCollapsesItsParent() {
    let a = UUID(), b = UUID()
    let tree = SplitTree.leaf(id: a).splitting(leaf: a, axis: .horizontal, newLeaf: b)
    #expect(tree.removing(leaf: b) == .leaf(id: a))
}

@Test func removingTheLastLeafReturnsNil() {
    let a = UUID()
    #expect(SplitTree.leaf(id: a).removing(leaf: a) == nil)
}

@Test func splitTreeCodableRoundTripsLeafAndNestedTree() throws {
    let a = UUID(), b = UUID(), c = UUID()
    let leaf = SplitTree.leaf(id: a)
    let nested = leaf
        .splitting(leaf: a, axis: .horizontal, newLeaf: b)
        .splitting(leaf: b, axis: .vertical, newLeaf: c)
    for tree in [leaf, nested] {
        let data = try JSONEncoder().encode(tree)
        let decoded = try JSONDecoder().decode(SplitTree.self, from: data)
        #expect(decoded == tree)
    }
}
