import Foundation
import Testing
import TillerGit
@testable import Tiller

@Suite
struct DiffTabAvailabilityTests {
    @Test func aCleanFileUsesTheCleanEmptyState() {
        #expect(DiffTabAvailability.reason(for: nil as GitFileDiff?) == .clean)
    }

    @Test func aBinaryDiffUsesTheBinaryEmptyState() throws {
        let path = try GitPath("Assets/icon.png")
        let diff = try GitDiff.parse(
            "diff --git a/Assets/icon.png b/Assets/icon.png\nBinary files a/Assets/icon.png and b/Assets/icon.png differ",
            path: path)

        #expect(DiffTabAvailability.reason(for: diff) == .binary)
    }

    @Test func aSubmoduleDiffUsesTheSubmoduleEmptyState() throws {
        let path = try GitPath("Vendor/tool")
        let diff = try GitDiff.parse(
            "diff --git a/Vendor/tool b/Vendor/tool\n@@ -1 +1 @@\n-Subproject commit old\n+Subproject commit new",
            path: path)

        #expect(DiffTabAvailability.reason(for: diff) == .submodule)
    }

    @Test func anOutputLimitErrorUsesTheLimitEmptyState() {
        #expect(
            DiffTabAvailability.reason(
                for: GitError.outputTooLarge(maxBytes: 10, maxLines: 20)) == .outputLimit)
    }
}
