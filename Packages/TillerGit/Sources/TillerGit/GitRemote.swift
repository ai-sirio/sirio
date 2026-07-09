import Foundation

public enum GitRemote {
    /// Parses the "origin" remote URL for a github.com owner. Supports
    /// `git@github.com:owner/repo.git` and `https://github.com/owner/repo.git`.
    /// Returns nil if there's no origin remote or it isn't github.com.
    public static func githubOwner(repoPath: String) async -> String? {
        guard let output = try? await GitRunner.run(["remote", "get-url", "origin"], in: repoPath) else {
            return nil
        }
        let url = output.trimmingCharacters(in: .whitespacesAndNewlines)
        if url.hasPrefix("git@github.com:") {
            let rest = url.dropFirst("git@github.com:".count)
            return rest.split(separator: "/").first.map(String.init)
        }
        if let components = URLComponents(string: url), components.host == "github.com" {
            return components.path.split(separator: "/").first.map(String.init)
        }
        return nil
    }

    /// Derives a project folder name from a clone URL — the last path
    /// segment, minus a trailing `.git` or `/`. Handles both HTTPS
    /// (`https://host/owner/repo.git`) and scp-like SSH (`git@host:owner/repo.git`).
    public static func projectName(fromCloneURL url: String) -> String {
        var trimmed = url.trimmingCharacters(in: .whitespacesAndNewlines)
        if trimmed.hasSuffix("/") { trimmed.removeLast() }
        if trimmed.hasSuffix(".git") { trimmed.removeLast(4) }
        let lastSegment = trimmed.split(separator: "/").last ?? Substring(trimmed)
        return String(lastSegment)
    }
}
