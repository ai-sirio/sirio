import Foundation

public enum TillerSkillProvisioner {
    public static let marker = "<!-- Machine-managed by Tiller. Do not edit this installed copy. -->"
    /// Stable prefix shared by every marker sentence Tiller has ever written.
    /// Recognizing existing files by this prefix (rather than the exact
    /// current `marker` string) lets the wording evolve across releases
    /// without locking out worktrees provisioned by an older build.
    private static let managedPrefix = "<!-- Machine-managed by Tiller"

    public enum Error: Swift.Error, Equatable, LocalizedError {
        case missingMarker
        case unsupportedAgent(String)
        case unmanagedFile(String)

        public var errorDescription: String? {
            switch self {
            case .missingMarker:
                "Tiller skill content is missing its managed-file marker"
            case let .unsupportedAgent(id):
                "Unsupported Tiller agent: \(id)"
            case let .unmanagedFile(path):
                "Refusing to overwrite unmanaged skill at \(path)"
            }
        }
    }

    public static func destination(agentID: String, worktreePath: String) throws -> URL {
        let relativePath = switch agentID {
        case "claude": ".claude/skills/tiller/SKILL.md"
        case "codex", "opencode", "pi", "omp": ".agents/skills/tiller/SKILL.md"
        default: throw Error.unsupportedAgent(agentID)
        }
        return URL(fileURLWithPath: worktreePath, isDirectory: true)
            .appendingPathComponent(relativePath)
    }

    public static func install(
        markdown: String,
        agentID: String,
        worktreePath: String
    ) throws {
        guard markdown.contains(marker) else { throw Error.missingMarker }
        let destination = try destination(agentID: agentID, worktreePath: worktreePath)
        let fileManager = FileManager.default
        if fileManager.fileExists(atPath: destination.path) {
            let existing = try String(contentsOf: destination, encoding: .utf8)
            guard existing.contains(managedPrefix) else {
                throw Error.unmanagedFile(destination.path)
            }
        }
        try fileManager.createDirectory(
            at: destination.deletingLastPathComponent(),
            withIntermediateDirectories: true
        )
        try markdown.write(to: destination, atomically: true, encoding: .utf8)
    }
}
