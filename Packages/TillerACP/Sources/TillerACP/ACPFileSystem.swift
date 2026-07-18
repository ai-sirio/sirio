import Foundation

/// Serves the agent's `fs/*` requests. Implementations decide policy;
/// `WorktreeFileSystem` confines all access to one worktree.
public protocol ACPFileSystem: Sendable {
    func readTextFile(path: String, line: Int?, limit: Int?) throws -> String
    func writeTextFile(path: String, content: String) throws
}

/// Disk-backed implementation that refuses any path outside its root —
/// the client-side guardrail from the spec's edge cases.
public struct WorktreeFileSystem: ACPFileSystem {
    public struct PathOutsideWorktree: Error {
        public let path: String
    }

    private let root: String

    public init(root: String) {
        self.root = URL(fileURLWithPath: root).standardizedFileURL
            .resolvingSymlinksInPath().path
    }

    private func validated(_ path: String) throws -> URL {
        let url = URL(fileURLWithPath: path).standardizedFileURL
        // Resolve symlinks on the deepest existing ancestor so a symlink
        // escape inside the path can't slip past the prefix check.
        let resolved = url.resolvingSymlinksInPath()
        guard resolved.path == root || resolved.path.hasPrefix(root + "/") else {
            throw PathOutsideWorktree(path: path)
        }
        return resolved
    }

    public func readTextFile(path: String, line: Int?, limit: Int?) throws -> String {
        let url = try validated(path)
        let content = try String(contentsOf: url, encoding: .utf8)
        guard line != nil || limit != nil else { return content }
        var lines = content.components(separatedBy: "\n")
        if let line, line >= 1 { lines = Array(lines.dropFirst(line - 1)) }
        if let limit { lines = Array(lines.prefix(limit)) }
        return lines.joined(separator: "\n")
    }

    public func writeTextFile(path: String, content: String) throws {
        let url = try validated(path)
        try FileManager.default.createDirectory(
            at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
        try content.write(to: url, atomically: true, encoding: .utf8)
    }
}
