import Foundation

/// Filesystem-backed autocomplete for @-mentions. Synchronous and capped:
/// callers invoke it from a background task on each keystroke.
public enum FileMentionIndex {
    private static let ignoredDirectories: Set<String> = [
        ".git", "node_modules", ".build", "DerivedData",
    ]
    private static let scanCap = 5000

    public static func candidates(worktreePath: String, query: String,
                                  limit: Int) -> [String] {
        let root = URL(fileURLWithPath: worktreePath).resolvingSymlinksInPath()
        guard let enumerator = FileManager.default.enumerator(
            at: root, includingPropertiesForKeys: [.isRegularFileKey],
            options: [.skipsHiddenFiles]) else { return [] }

        var relativePaths: [String] = []
        var scanned = 0
        for case let url as URL in enumerator {
            scanned += 1
            if scanned > scanCap { break }
            let name = url.lastPathComponent
            if ignoredDirectories.contains(name) {
                enumerator.skipDescendants()
                continue
            }
            guard (try? url.resourceValues(forKeys: [.isRegularFileKey]))?
                .isRegularFile == true else { continue }
            guard let rootRange = url.path.range(of: root.path, options: .backwards),
                  url.path[rootRange.upperBound...].first == "/" else { continue }
            let relative = String(url.path[rootRange.upperBound...].dropFirst())
            relativePaths.append(relative)
        }

        let lowered = query.lowercased()
        let matches = lowered.isEmpty
            ? relativePaths
            : relativePaths.filter { $0.lowercased().contains(lowered) }
        let ranked = matches.sorted { a, b in
            let aPrefix = (a as NSString).lastPathComponent.lowercased()
                .hasPrefix(lowered)
            let bPrefix = (b as NSString).lastPathComponent.lowercased()
                .hasPrefix(lowered)
            if aPrefix != bPrefix { return aPrefix }
            return a < b
        }
        return Array(ranked.prefix(limit))
    }
}
