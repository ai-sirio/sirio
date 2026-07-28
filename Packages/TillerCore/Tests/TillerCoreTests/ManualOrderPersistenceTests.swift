import Testing
import Foundation
import TillerPersistence
@testable import TillerCore

@Suite struct ManualOrderPersistenceTests {
    @Test func projectsLoadInInsertionOrderUntilReordered() async throws {
        // Arrange
        let store = ProjectStore(database: try AppDatabase.inMemory())
        let first = try await store.addProject(name: "a", rootPath: "/tmp/a")
        let second = try await store.addProject(name: "b", rootPath: "/tmp/b")
        let third = try await store.addProject(name: "c", rootPath: "/tmp/c")
        #expect(try await store.loadAll().map(\.id) == [first.id, second.id, third.id])

        // Act
        try await store.saveProjectOrder([third.id, first.id, second.id])

        // Assert
        #expect(try await store.loadAll().map(\.id) == [third.id, first.id, second.id])
    }

    @Test func aNewProjectIsAppendedAfterAReorder() async throws {
        // Arrange
        let store = ProjectStore(database: try AppDatabase.inMemory())
        let first = try await store.addProject(name: "a", rootPath: "/tmp/a")
        let second = try await store.addProject(name: "b", rootPath: "/tmp/b")
        try await store.saveProjectOrder([second.id, first.id])

        // Act
        let third = try await store.addProject(name: "c", rootPath: "/tmp/c")

        // Assert: in coda, non in testa.
        #expect(try await store.loadAll().map(\.id) == [second.id, first.id, third.id])
    }

    @Test func worktreeOrderIsPersistedPerProject() async throws {
        // Arrange
        let store = ProjectStore(database: try AppDatabase.inMemory())
        let project = try await store.addProject(name: "demo", rootPath: "/tmp/demo")
        let main = try await store.addWorktree(projectId: project.id, branch: "main", path: "/tmp/demo")
        let feat = try await store.addWorktree(projectId: project.id, branch: "feat", path: "/tmp/demo-feat")

        // Act
        try await store.saveWorktreeOrder(projectId: project.id, order: [feat.id, main.id])

        // Assert
        #expect(try await store.worktrees(of: project.id).map(\.id) == [feat.id, main.id])
    }

    @Test func reorderingIgnoresIdsThatAreNotInTheList() async throws {
        // Arrange: un id estraneo non deve spostare né perdere righe.
        let store = ProjectStore(database: try AppDatabase.inMemory())
        let first = try await store.addProject(name: "a", rootPath: "/tmp/a")
        let second = try await store.addProject(name: "b", rootPath: "/tmp/b")

        // Act
        try await store.saveProjectOrder([UUID(), second.id, first.id])

        // Assert
        #expect(try await store.loadAll().map(\.id) == [second.id, first.id])
    }
}
