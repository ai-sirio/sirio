import Foundation
import Testing
@testable import TillerControl

@Suite struct TillerctlShimTests {
    private func makeTempDir() throws -> URL {
        let dir = URL(fileURLWithPath: NSTemporaryDirectory())
            .appendingPathComponent("shim-tests-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        return dir
    }

    @Test func installCreatesSymlinkToTarget() throws {
        let dir = try makeTempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let target = dir.appendingPathComponent("tillerctl-real").path
        FileManager.default.createFile(atPath: target, contents: Data("x".utf8))
        let shim = dir.appendingPathComponent("bin/tillerctl").path

        try TillerctlShim.install(target: target, shimPath: shim)

        #expect(try FileManager.default.destinationOfSymbolicLink(atPath: shim) == target)
    }

    @Test func reinstallRepointsExistingShim() throws {
        let dir = try makeTempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let old = dir.appendingPathComponent("old-tillerctl").path
        let new = dir.appendingPathComponent("new-tillerctl").path
        FileManager.default.createFile(atPath: old, contents: Data("a".utf8))
        FileManager.default.createFile(atPath: new, contents: Data("b".utf8))
        let shim = dir.appendingPathComponent("bin/tillerctl").path

        try TillerctlShim.install(target: old, shimPath: shim)
        try TillerctlShim.install(target: new, shimPath: shim)

        #expect(try FileManager.default.destinationOfSymbolicLink(atPath: shim) == new)
    }

    @Test func installReplacesRegularFileAtShimPath() throws {
        let dir = try makeTempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let target = dir.appendingPathComponent("tillerctl-real").path
        FileManager.default.createFile(atPath: target, contents: Data("x".utf8))
        let shim = dir.appendingPathComponent("bin/tillerctl").path
        try FileManager.default.createDirectory(
            atPath: (shim as NSString).deletingLastPathComponent,
            withIntermediateDirectories: true
        )
        FileManager.default.createFile(atPath: shim, contents: Data("stale".utf8))

        try TillerctlShim.install(target: target, shimPath: shim)

        #expect(try FileManager.default.destinationOfSymbolicLink(atPath: shim) == target)
    }

    @Test func defaultShimPathIsUnderTillerBin() {
        #expect(TillerctlShim.defaultShimPath().hasSuffix("/Tiller/bin/tillerctl"))
    }
}
