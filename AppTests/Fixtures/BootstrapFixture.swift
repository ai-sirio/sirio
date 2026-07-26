import Foundation
import TillerCore
import TillerPersistence

struct BootstrapFixture: Equatable, Sendable {
    let rootDirectory: URL
    let databasePath: String
    let projects: [Project]
    let worktrees: [UUID: [Worktree]]

    static func make() throws -> Self {
        let rootDirectory = FileManager.default.temporaryDirectory
            .appendingPathComponent("tiller-phase1-bootstrap-fixture", isDirectory: true)
        try? FileManager.default.removeItem(at: rootDirectory)
        try FileManager.default.createDirectory(at: rootDirectory, withIntermediateDirectories: true)

        let databasePath = rootDirectory.appendingPathComponent("tiller.sqlite").path
        let database = try AppDatabase(path: databasePath)
        let createdAt = Date(timeIntervalSince1970: 1_700_000_000)
        var projects: [Project] = []
        var worktrees: [UUID: [Worktree]] = [:]

        try database.write { db in
            for projectIndex in 0..<10 {
                let projectId = stableUUID(1_000 + projectIndex)
                let projectRoot = rootDirectory.appendingPathComponent(
                    "project-\(projectIndex)", isDirectory: true)
                try FileManager.default.createDirectory(
                    at: projectRoot, withIntermediateDirectories: true)
                let project = Project(
                    id: projectId,
                    name: "Project \(projectIndex)",
                    rootPath: projectRoot.path
                )
                projects.append(project)
                try ProjectRecord(
                    id: projectId.uuidString,
                    name: project.name,
                    rootPath: project.rootPath,
                    createdAt: createdAt
                ).insert(db)

                var projectWorktrees: [Worktree] = []
                for worktreeIndex in 0..<2 {
                    let worktreeId = stableUUID(2_000 + projectIndex * 2 + worktreeIndex)
                    let worktreeRoot = projectRoot.appendingPathComponent(
                        "worktree-\(worktreeIndex)", isDirectory: true)
                    try FileManager.default.createDirectory(
                        at: worktreeRoot, withIntermediateDirectories: true)
                    let worktree = Worktree(
                        id: worktreeId,
                        projectId: projectId,
                        branch: "feature/fixture-\(worktreeIndex)",
                        path: worktreeRoot.path,
                        isPrimary: worktreeIndex == 0
                    )
                    projectWorktrees.append(worktree)
                    try WorktreeRecord(
                        id: worktreeId.uuidString,
                        projectId: projectId.uuidString,
                        branch: worktree.branch,
                        path: worktree.path,
                        createdAt: createdAt,
                        isPrimary: worktree.isPrimary
                    ).insert(db)
                }
                worktrees[projectId] = projectWorktrees
            }
        }

        return Self(
            rootDirectory: rootDirectory,
            databasePath: databasePath,
            projects: projects,
            worktrees: worktrees
        )
    }

    func remove() {
        try? FileManager.default.removeItem(at: rootDirectory)
    }

    static func == (lhs: Self, rhs: Self) -> Bool {
        lhs.projects == rhs.projects && lhs.worktrees == rhs.worktrees
    }

    private static func stableUUID(_ value: Int) -> UUID {
        UUID(uuidString: String(format: "00000000-0000-4000-8000-%012d", value))!
    }
}
