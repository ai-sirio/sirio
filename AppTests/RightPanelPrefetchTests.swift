import Foundation
import Testing
import TillerCore

@testable import Tiller

@Suite("RightPanelPrefetch")
@MainActor
struct RightPanelPrefetchTests {
    @Test func activationPrefetchesOneLevelAndExpansionContinuesLookahead() async {
        let probe = PrefetchProbe()
        let model = makeModel(probe: probe)

        await model.activate(worktree: makeWorktree(), isGitRepository: false)
        await probe.waitForCalls(["", "src", "docs"])
        await waitForCachedDirectories(model, ["src", "docs"])

        #expect(Set(await probe.calls) == ["", "src", "docs"])
        #expect(!model.childrenByDirectory.keys.contains("src/nested"))
        #expect(!model.childrenByDirectory.keys.contains("docs/nested"))

        await model.toggleDirectory("src")
        await probe.waitForCalls(["src/nested"])
        await waitForCachedDirectories(model, ["src/nested"])

        #expect(await probe.calls.filter { $0 == "src" }.count == 1)
        #expect(await probe.calls.filter { $0 == "src/nested" }.count == 1)

        await model.toggleDirectory("src")
        await model.toggleDirectory("src")
        await probe.waitForCalls(["src/nested"])

        #expect(await probe.calls.filter { $0 == "src" }.count == 1)
        #expect(await probe.calls.filter { $0 == "src/nested" }.count == 1)
        #expect(!model.childrenByDirectory.keys.contains("src/nested/grandchild"))
    }

    private func makeModel(probe: PrefetchProbe) -> RightPanelModel {
        RightPanelModel(
            loaders: .init(
                directory: { key, _ in await probe.loadDirectory(key) },
                status: { _ in .empty },
                diff: { _, _ in fatalError("unused") }),
            monitoringEnabled: false)
    }

    private func makeWorktree() -> Worktree {
        Worktree(
            id: UUID(), projectId: UUID(), branch: "main",
            path: "/tmp/right-panel-prefetch", isPrimary: true)
    }

    private func waitForCachedDirectories(
        _ model: RightPanelModel, _ keys: [String]
    ) async {
        while !Set(keys).isSubset(of: model.childrenByDirectory.keys) {
            try? await Task.sleep(for: .milliseconds(10))
        }
    }
}

private actor PrefetchProbe {
    private(set) var calls: [String] = []

    func loadDirectory(_ key: String) -> [FileTreeNode] {
        calls.append(key)
        switch key {
        case "":
            return [
                node("src", kind: .directory),
                node("docs", kind: .directory),
                node("README.md", kind: .file),
            ]
        case "src":
            return [
                node("src/nested", kind: .directory),
                node("src/main.swift", kind: .file),
            ]
        case "docs":
            return [node("docs/nested", kind: .directory)]
        case "src/nested":
            return [node("src/nested/grandchild", kind: .directory)]
        default:
            return []
        }
    }

    func waitForCalls(_ expected: [String]) async {
        while !Set(expected).isSubset(of: calls) {
            try? await Task.sleep(for: .milliseconds(10))
        }
    }

    private func node(_ path: String, kind: FileTreeNodeKind) -> FileTreeNode {
        FileTreeNode(relativePath: path, name: path.split(separator: "/").last.map(String.init) ?? path, kind: kind)
    }
}
