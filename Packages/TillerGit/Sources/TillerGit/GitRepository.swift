import Foundation

public enum GitRepository {
    public static func gitDirectory(in repoPath: String) async throws -> URL {
        let raw = try await GitRunner.run(
            ["rev-parse", "--path-format=absolute", "--git-dir"], in: repoPath)
        return URL(fileURLWithPath: raw.trimmingCharacters(in: .whitespacesAndNewlines),
                   isDirectory: true).standardizedFileURL
    }

    public static func hasHead(in repoPath: String) async throws -> Bool {
        let result = try await GitRunner.runCaptured(
            ["rev-parse", "--verify", "--quiet", "HEAD"],
            in: repoPath,
            acceptedExitCodes: [0, 1])
        return result.exitCode == 0
    }
}
