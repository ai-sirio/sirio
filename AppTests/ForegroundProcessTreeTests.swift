import Foundation
import Testing
import TillerCore
@testable import Tiller

@Suite
struct ForegroundProcessTreeTests {
    // Fake process table: parent pid -> children, pid -> name.
    static let children: [pid_t: [pid_t]] = [
        100: [200],          // shell -> claude
        200: [250, 260],     // claude -> rg, claude(sub)
        260: [270],          // claude(sub) -> node
    ]
    static let names: [pid_t: String] = [
        200: "claude", 250: "rg", 260: "claude", 270: "node",
    ]

    func build(maxDepth: Int = 5, maxCount: Int = 50) -> [ProcessNode] {
        ForegroundProcessAgent.buildTree(
            roots: Self.children[100] ?? [],
            childrenOf: { Self.children[$0] ?? [] },
            nameOf: { Self.names[$0] },
            maxDepth: maxDepth, maxCount: maxCount)
    }

    @Test func buildsNestedTreeWithNames() {
        let tree = build()
        #expect(tree.count == 1)
        #expect(tree[0].pid == 200)
        #expect(tree[0].name == "claude")
        #expect(tree[0].children.map(\.pid) == [250, 260])
        #expect(tree[0].children[1].children.map(\.name) == ["node"])
    }

    @Test func depthCapTruncatesGrandchildren() {
        let tree = build(maxDepth: 1)
        #expect(tree.count == 1)
        #expect(tree[0].children.isEmpty)
    }

    @Test func countCapStopsWalk() {
        let tree = build(maxCount: 2)
        var total = 0
        func count(_ nodes: [ProcessNode]) {
            for n in nodes { total += 1; count(n.children) }
        }
        count(tree)
        #expect(total <= 2)
    }

    @Test func unnamedProcessesAreSkipped() {
        let tree = ForegroundProcessAgent.buildTree(
            roots: [999],
            childrenOf: { _ in [] },
            nameOf: { _ in nil },
            maxDepth: 5, maxCount: 50)
        #expect(tree.isEmpty)
    }
}
