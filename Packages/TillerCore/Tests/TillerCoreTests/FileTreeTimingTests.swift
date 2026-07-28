import Foundation
import Testing
@testable import TillerCore

private struct TimingSample {
    let cold: [Double]
    let warm: [Double]
    let lookahead: [Double]
}

private func makeTimingFixture(entryCount: Int) throws -> URL {
    let root = FileManager.default.temporaryDirectory
        .appendingPathComponent("tiller-file-tree-timing-\(UUID().uuidString)", isDirectory: true)
    try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)

    let directoryCount = Int(Double(entryCount) * 0.3)
    for index in 0..<directoryCount {
        try FileManager.default.createDirectory(
            at: root.appendingPathComponent("directory-\(index)", isDirectory: true),
            withIntermediateDirectories: false)
    }
    for index in 0..<(entryCount - directoryCount) {
        try Data("fixture-\(index)".utf8).write(
            to: root.appendingPathComponent("file-\(index).txt"))
    }
    return root
}

private func makeJavaScriptFixture() throws -> URL {
    let root = try makeTimingFixture(entryCount: 15)
    let modules = root.appendingPathComponent("node_modules", isDirectory: true)
    try FileManager.default.createDirectory(at: modules, withIntermediateDirectories: false)
    let rootFiles = try FileManager.default.contentsOfDirectory(atPath: root.path)
    for name in rootFiles where name.hasPrefix("directory-") {
        try FileManager.default.removeItem(at: root.appendingPathComponent(name))
    }
    for index in 11..<14 {
        try Data("root-file-\(index)".utf8).write(
            to: root.appendingPathComponent("file-\(index).txt"))
    }
    for index in 0..<3000 {
        try Data("module-\(index)".utf8).write(
            to: modules.appendingPathComponent("package-\(index).js"))
    }
    return root
}

private func elapsedMilliseconds(_ operation: () throws -> Void) rethrows -> Double {
    let clock = ContinuousClock()
    let start = clock.now
    try operation()
    let duration = start.duration(to: clock.now)
    let components = duration.components
    return (Double(components.seconds) * 1_000) +
        (Double(components.attoseconds) / 1_000_000_000_000_000)
}

private func summarize(_ samples: [Double]) -> String {
    let sorted = samples.sorted()
    let median = sorted[sorted.count / 2]
    return String(
        format: "min %.3f ms | median %.3f ms | max %.3f ms",
        sorted[0], median, sorted[sorted.count - 1])
}

private func measureFixture(entryCount: Int) throws -> TimingSample {
    var cold: [Double] = []
    var warm: [Double] = []
    var lookahead: [Double] = []

    for _ in 0..<5 {
        let root = try makeTimingFixture(entryCount: entryCount)
        defer { try? FileManager.default.removeItem(at: root) }

        var rootNodes: [FileTreeNode] = []
        cold.append(try elapsedMilliseconds {
            rootNodes = try FileTreeLoader.children(at: "", rootURL: root)
        })
        #expect(rootNodes.count == entryCount)

        for _ in 0..<5 {
            var nodes: [FileTreeNode] = []
            warm.append(try elapsedMilliseconds {
                nodes = try FileTreeLoader.children(at: "", rootURL: root)
            })
            #expect(nodes.count == entryCount)
        }

        let directories = rootNodes.filter(\.kind.isDirectory)
        lookahead.append(try elapsedMilliseconds {
            _ = try FileTreeLoader.children(at: "", rootURL: root)
            for directory in directories {
                let children = try FileTreeLoader.children(at: directory.relativePath, rootURL: root)
                #expect(children.isEmpty)
            }
        })
    }

    return TimingSample(cold: cold, warm: warm, lookahead: lookahead)
}

@Suite(.serialized)
struct FileTreeTimingTests {
    @Test("FileTree enumeration timing")
    func fileTreeEnumerationTiming() throws {
        print("\nFileTree timing results (5 fresh fixtures; warm has 5 calls per fixture)")
        for entryCount in [50, 200, 1000] {
            let sample = try measureFixture(entryCount: entryCount)
            print("N=\(entryCount) cold:    \(summarize(sample.cold))")
            print("N=\(entryCount) warm:    \(summarize(sample.warm))")
            print("N=\(entryCount) lookahead: \(summarize(sample.lookahead))")
        }
    }

    @Test("FileTree realistic JavaScript project timing")
    func fileTreeRealisticJavaScriptProjectTiming() throws {
        let root = try makeJavaScriptFixture()
        defer { try? FileManager.default.removeItem(at: root) }
        let modulesPath = "node_modules"

        var rootNodes: [FileTreeNode] = []
        let rootMilliseconds = try elapsedMilliseconds {
            rootNodes = try FileTreeLoader.children(at: "", rootURL: root)
        }
        #expect(rootNodes.count == 15)

        var moduleNodes: [FileTreeNode] = []
        let modulesMilliseconds = try elapsedMilliseconds {
            moduleNodes = try FileTreeLoader.children(at: modulesPath, rootURL: root)
        }
        #expect(moduleNodes.count == 3000)

        let lookaheadMilliseconds = try elapsedMilliseconds {
            _ = try FileTreeLoader.children(at: "", rootURL: root)
            for directory in rootNodes where directory.kind.isDirectory {
                _ = try FileTreeLoader.children(at: directory.relativePath, rootURL: root)
            }
        }
        print("\nRealistic JavaScript fixture: root=15 entries, node_modules=3000 files")
        print(String(format: "root expansion: %.3f ms", rootMilliseconds))
        print(String(format: "node_modules cold expansion: %.3f ms", modulesMilliseconds))
        print(String(format: "full root one-level lookahead: %.3f ms", lookaheadMilliseconds))
    }
}
