import Foundation
import Testing
import TillerACP
import TillerPersistence
import TillerCore
private struct BootstrapProjectSnapshot: Equatable {
    let id: UUID
    let name: String
    let relativePath: String
}

private struct BootstrapWorktreeSnapshot: Equatable {
    let id: UUID
    let projectId: UUID
    let branch: String
    let relativePath: String
    let isPrimary: Bool
}

private struct BootstrapSnapshot: Equatable {
    let projects: [BootstrapProjectSnapshot]
    let worktrees: [BootstrapWorktreeSnapshot]
}

private func bootstrapSnapshot(_ fixture: BootstrapFixture) -> BootstrapSnapshot {
    let rootPath = fixture.rootDirectory.standardizedFileURL.path
    let prefix = rootPath.hasSuffix("/") ? rootPath : rootPath + "/"
    func relativePath(_ path: String) -> String {
        let absolutePath = URL(fileURLWithPath: path).standardizedFileURL.path
        return absolutePath.hasPrefix(prefix)
            ? String(absolutePath.dropFirst(prefix.count))
            : absolutePath
    }

    return BootstrapSnapshot(
        projects: fixture.projects.map {
            BootstrapProjectSnapshot(
                id: $0.id,
                name: $0.name,
                relativePath: relativePath($0.rootPath)
            )
        },
        worktrees: fixture.worktrees.values
            .flatMap { $0 }
            .map {
                BootstrapWorktreeSnapshot(
                    id: $0.id,
                    projectId: $0.projectId,
                    branch: $0.branch,
                    relativePath: relativePath($0.path),
                    isPrimary: $0.isPrimary
                )
            }
            .sorted { $0.id.uuidString < $1.id.uuidString }
    )
}

@Test func chatStreamFixtureIsDeterministicAndByteComplete() {
    let first = ChatStreamFixture.make()
    let second = ChatStreamFixture.make()

    #expect(first == second)
    #expect(first.finalMessage.utf8.count >= 50 * 1024)
    #expect(first.fragments.count >= 1_000)
    #expect(first.fragments.joined() == first.finalMessage)
    #expect(Data(first.fragments.joined().utf8) == Data(first.finalMessage.utf8))
    #expect(first.transcript.count >= 200)
    #expect(Set(first.transcript.map(\.id)).count == first.transcript.count)
    #expect(first.finalMessage.contains("★ Insight ───"))
    #expect(first.finalMessage.contains("```swift"))
    #expect(first.finalMessage.components(separatedBy: "```swift").count > 3)
    #expect(first.finalMessage.contains("- "))
    #expect(first.finalMessage.contains("1. "))
    #expect(first.finalMessage.contains("["))
}


@Test func rightPanelFixtureContainsNestedDuplicates() {
    let first = RightPanelFixture.make()
    let second = RightPanelFixture.make()

    #expect(first == second)
    #expect(first.changedPaths.count >= 1_000)
    #expect(Set(first.changedPaths).count < first.changedPaths.count)
    #expect(first.changedPaths.contains { $0.split(separator: "/").count >= 4 })
}

@Test func bootstrapFixtureBuildsProjectsAndWorktreesOnDisk() throws {
    let first = try BootstrapFixture.make()
    defer { first.remove() }

    #expect(first.projects.count >= 10)
    #expect(first.worktrees.values.reduce(0) { $0 + $1.count } >= 20)
    #expect(FileManager.default.fileExists(atPath: first.rootDirectory.path))
    #expect(FileManager.default.fileExists(atPath: first.databasePath))
    #expect(first.projects.allSatisfy { FileManager.default.fileExists(atPath: $0.rootPath) })
    #expect(first.worktrees.values.flatMap { $0 }.allSatisfy {
        FileManager.default.fileExists(atPath: $0.path)
    })

    let second = try BootstrapFixture.make()
    defer { second.remove() }
    #expect(bootstrapSnapshot(first) == bootstrapSnapshot(second))
}

@Test func bootstrapFixturesCanCoexist() throws {
    let first = try BootstrapFixture.make()
    defer { first.remove() }
    let second = try BootstrapFixture.make()
    defer { second.remove() }

    #expect(first.rootDirectory != second.rootDirectory)
    #expect(FileManager.default.fileExists(atPath: first.rootDirectory.path))
    #expect(FileManager.default.fileExists(atPath: first.databasePath))
    #expect(FileManager.default.fileExists(atPath: second.rootDirectory.path))
    #expect(FileManager.default.fileExists(atPath: second.databasePath))
    #expect(first.projects.allSatisfy {
        FileManager.default.fileExists(atPath: $0.rootPath)
    })
    #expect(second.projects.allSatisfy {
        FileManager.default.fileExists(atPath: $0.rootPath)
    })
    #expect(first.worktrees.values.flatMap { $0 }.allSatisfy {
        FileManager.default.fileExists(atPath: $0.path)
    })
    #expect(second.worktrees.values.flatMap { $0 }.allSatisfy {
        FileManager.default.fileExists(atPath: $0.path)
    })
    #expect(bootstrapSnapshot(first) == bootstrapSnapshot(second))
}
