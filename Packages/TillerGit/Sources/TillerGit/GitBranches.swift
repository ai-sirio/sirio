import Foundation

public enum GitBranches {
    /// Local branch names (remote branches are out of scope).
    public static func list(repoPath: String) async throws -> [String] {
        let output = try await GitRunner.run(
            ["branch", "--list", "--format=%(refname:short)"], in: repoPath
        )
        return output
            .split(separator: "\n", omittingEmptySubsequences: true)
            .map(String.init)
    }
}
