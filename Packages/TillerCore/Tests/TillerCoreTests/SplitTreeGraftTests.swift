// Packages/TillerCore/Tests/TillerCoreTests/SplitTreeGraftTests.swift
import Testing
import Foundation
@testable import TillerCore

/// Semantica di SplitTree usata da adoptPane (spostamento pane tra tab):
/// splitting(newLeaf:) accetta un id già esistente altrove (graft) e
/// removing(leaf:) su albero mono-leaf torna nil (tab svuotato).
struct SplitTreeGraftTests {
    @Test func splittingGraftsExistingLeafIdIntoTree() {
        // Arrange
        let anchor = UUID()
        let moved = UUID()
        let tree = SplitTree.leaf(id: anchor)

        // Act
        let grafted = tree.splitting(leaf: anchor, axis: .horizontal, newLeaf: moved)

        // Assert
        #expect(grafted == .split(axis: .horizontal, first: .leaf(id: anchor), second: .leaf(id: moved)))
        #expect(grafted.leafIds == [anchor, moved])
    }

    @Test func removingOnlyLeafReturnsNil() {
        let only = UUID()
        #expect(SplitTree.leaf(id: only).removing(leaf: only) == nil)
    }

    @Test func moveSequencePreservesRemainingTreeAndLeafOrder() {
        // Arrange: sorgente con due pane, destinazione mono-pane.
        let a = UUID()
        let b = UUID()
        let dest = UUID()
        let source = SplitTree.leaf(id: a).splitting(leaf: a, axis: .vertical, newLeaf: b)

        // Act: rimuovo b dal sorgente e lo innesto nella destinazione.
        let sourceAfter = source.removing(leaf: b)
        let destination = SplitTree.leaf(id: dest)
            .splitting(leaf: dest, axis: .horizontal, newLeaf: b)

        // Assert
        #expect(sourceAfter == .leaf(id: a))
        #expect(destination.leafIds == [dest, b])
    }
}
