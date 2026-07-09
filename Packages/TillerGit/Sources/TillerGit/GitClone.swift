import Foundation

/// Clones a repo with live progress reporting, parsed from git's own
/// `--progress` stderr output. Local clones (production URLs are always
/// remote — http(s)/ssh) already stream real transfer progress; no extra
/// flags are needed beyond what `GitRunner.runStreaming` already captures.
public enum GitClone {
    public static func clone(
        url: String, to path: String,
        onProgress: @escaping @Sendable (Double) -> Void
    ) async throws {
        let parentDir = (path as NSString).deletingLastPathComponent
        try await GitRunner.runStreaming(
            ["clone", "--progress", url, path], in: parentDir
        ) { line in
            if let value = parseProgress(fromLine: line) {
                onProgress(value)
            }
        }
    }

    static func parseProgress(fromLine line: String) -> Double? {
        guard line.hasPrefix("Receiving objects:") else { return nil }
        guard let percentRange = line.range(of: #"\d+(?=%)"#, options: .regularExpression) else {
            return nil
        }
        guard let percent = Double(line[percentRange]) else { return nil }
        return percent / 100
    }
}
