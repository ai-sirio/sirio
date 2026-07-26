import Foundation
import Testing
import TillerACP
import TillerPersistence
import TillerCore

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
    #expect(first.projects == second.projects)
    #expect(first.worktrees == second.worktrees)
}
