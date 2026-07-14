import Foundation

public enum GitActionError: Error, Equatable {
    case emptySelection
    case invalidEntry(String)
}

extension GitActionError: LocalizedError {
    public var errorDescription: String? {
        switch self {
        case .emptySelection: "No Git paths were selected."
        case .invalidEntry(let message): message
        }
    }
}

public enum GitActions {
    public static func stage(_ entries: [GitStatusEntry], in repoPath: String) async throws {
        try await validate(entries, in: repoPath, section: .stage)
        _ = try await GitRunner.run(
            ["--literal-pathspecs", "add", "-A", "--"] + currentPathArguments(entries),
            in: repoPath)
    }

    public static func unstage(_ entries: [GitStatusEntry], in repoPath: String) async throws {
        guard entries.allSatisfy(\.isStaged) else {
            throw GitActionError.invalidEntry("Unstage requires staged entries.")
        }
        try await validate(entries, in: repoPath, section: .unstage)
        let paths = try pathArguments(entries)
        if try await GitRepository.hasHead(in: repoPath) {
            _ = try await GitRunner.run(
                ["--literal-pathspecs", "reset", "HEAD", "--"] + paths, in: repoPath)
        } else {
            _ = try await GitRunner.run(
                ["--literal-pathspecs", "rm", "--cached", "--force", "--"] + paths,
                in: repoPath)
        }
    }

    public static func discardChanges(
        _ entries: [GitStatusEntry], in repoPath: String
    ) async throws {
        guard entries.allSatisfy({ $0.hasWorktreeChanges && !$0.isUntracked }) else {
            throw GitActionError.invalidEntry("Discard Changes requires tracked worktree changes.")
        }
        try await validate(entries, in: repoPath, section: .discardChanges)
        _ = try await GitRunner.run(
            ["--literal-pathspecs", "restore", "--worktree", "--"]
                + currentPathArguments(entries),
            in: repoPath)
    }

    public static func discardUntracked(
        _ entries: [GitStatusEntry], in repoPath: String
    ) async throws {
        try await validate(entries, in: repoPath, section: .discardUntracked)
        _ = try await GitRunner.run(
            ["--literal-pathspecs", "clean", "-f", "-d", "--"]
                + currentPathArguments(entries),
            in: repoPath)
    }

    private enum ValidSection {
        case stage
        case unstage
        case discardChanges
        case discardUntracked
    }

    private static func validate(
        _ entries: [GitStatusEntry], in repoPath: String, section: ValidSection
    ) async throws {
        guard !entries.isEmpty else { throw GitActionError.emptySelection }
        guard entries.allSatisfy({ !$0.isConflicted }) else {
            throw GitActionError.invalidEntry("Conflicted entries are not supported.")
        }

        let current = try await GitStatus.load(in: repoPath)
        let validEntries: [GitStatusEntry]
        switch section {
        case .stage:
            validEntries = current.changes + current.untracked
        case .unstage:
            validEntries = current.staged
        case .discardChanges:
            validEntries = current.changes
        case .discardUntracked:
            validEntries = current.untracked
        }
        guard entries.allSatisfy({ validEntries.contains($0) }) else {
            throw GitActionError.invalidEntry(
                "Selected entries are not present in the current status.")
        }
    }

    private static func currentPathArguments(_ entries: [GitStatusEntry]) throws -> [String] {
        guard !entries.isEmpty else { throw GitActionError.emptySelection }
        var seen: Set<GitPath> = []
        var output: [String] = []
        for path in entries.map(\.path) where seen.insert(path).inserted {
            output.append(path.value)
        }
        return output
    }

    private static func pathArguments(_ entries: [GitStatusEntry]) throws -> [String] {
        guard !entries.isEmpty else { throw GitActionError.emptySelection }
        var seen: Set<GitPath> = []
        var output: [String] = []
        for path in entries.flatMap(\.mutationPaths) where seen.insert(path).inserted {
            output.append(path.value)
        }
        return output
    }
}
