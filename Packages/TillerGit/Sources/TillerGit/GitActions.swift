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
        let paths = try currentPathArguments(entries)
        _ = try await GitRunner.run(["add", "-A", "--"] + paths, in: repoPath)
    }

    public static func unstage(_ entries: [GitStatusEntry], in repoPath: String) async throws {
        guard entries.allSatisfy(\.isStaged) else {
            throw GitActionError.invalidEntry("Unstage requires staged entries.")
        }
        let paths = try pathArguments(entries)
        if try await GitRepository.hasHead(in: repoPath) {
            _ = try await GitRunner.run(["reset", "HEAD", "--"] + paths, in: repoPath)
        } else {
            _ = try await GitRunner.run(
                ["rm", "--cached", "--force", "--"] + paths, in: repoPath)
        }
    }

    public static func discardChanges(
        _ entries: [GitStatusEntry], in repoPath: String
    ) async throws {
        guard entries.allSatisfy({ $0.hasWorktreeChanges && !$0.isUntracked }) else {
            throw GitActionError.invalidEntry("Discard Changes requires tracked worktree changes.")
        }
        _ = try await GitRunner.run(
            ["restore", "--worktree", "--"] + currentPathArguments(entries), in: repoPath)
    }

    public static func discardUntracked(
        _ entries: [GitStatusEntry], in repoPath: String
    ) async throws {
        guard entries.allSatisfy(\.isUntracked) else {
            throw GitActionError.invalidEntry("Discard Untracked requires untracked entries.")
        }
        _ = try await GitRunner.run(
            ["clean", "-f", "-d", "--"] + currentPathArguments(entries), in: repoPath)
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
