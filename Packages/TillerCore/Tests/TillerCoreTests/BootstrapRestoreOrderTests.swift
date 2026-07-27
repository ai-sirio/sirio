import Foundation
import Testing
@testable import TillerCore

@Suite
struct BootstrapRestoreOrderTests {
    private func worktree(_ n: Int, project: UUID = projectId) -> Worktree {
        Worktree(id: id(n), projectId: project, branch: "b\(n)", path: "/w\(n)")
    }
    private static let projectId = UUID()
    private var projectId: UUID { Self.projectId }
    private func id(_ n: Int) -> UUID {
        UUID(uuidString: String(format: "00000000-0000-4000-8000-%012d", n))!
    }

    @Test func selectedWorktreeIsRestoredFirst() {
        let all = [worktree(1), worktree(2), worktree(3)]

        let order = BootstrapRestoreOrder.partition(
            worktrees: all, openWorktreeIds: [id(1), id(3)], selectedWorktreeId: id(3))

        #expect(order.priority.map(\.id) == [id(3), id(1)])
        #expect(order.deferred.map(\.id) == [id(2)])
    }

    @Test func openWorktreesKeepStoredOrderWhenNothingIsSelected() {
        let all = [worktree(1), worktree(2), worktree(3)]

        let order = BootstrapRestoreOrder.partition(
            worktrees: all, openWorktreeIds: [id(3), id(1)], selectedWorktreeId: nil)

        #expect(order.priority.map(\.id) == [id(3), id(1)])
        #expect(order.deferred.map(\.id) == [id(2)])
    }

    @Test func everyWorktreeAppearsExactlyOnce() {
        let all = (1...5).map { worktree($0) }

        let order = BootstrapRestoreOrder.partition(
            worktrees: all, openWorktreeIds: [id(2), id(4)], selectedWorktreeId: id(4))

        let combined = order.priority.map(\.id) + order.deferred.map(\.id)
        #expect(Set(combined) == Set(all.map(\.id)))
        #expect(combined.count == all.count)
    }

    @Test func deferredWorktreesPreserveInputOrder() {
        let all = [worktree(5), worktree(1), worktree(4), worktree(2)]

        let order = BootstrapRestoreOrder.partition(
            worktrees: all, openWorktreeIds: [id(4)], selectedWorktreeId: id(4))

        #expect(order.deferred.map(\.id) == [id(5), id(1), id(2)])
    }

    @Test func storedIdsWithoutAMatchingWorktreeAreIgnored() {
        let all = [worktree(1), worktree(2)]

        let order = BootstrapRestoreOrder.partition(
            worktrees: all, openWorktreeIds: [id(99), id(2)], selectedWorktreeId: id(99))

        #expect(order.priority.map(\.id) == [id(2)])
        #expect(order.deferred.map(\.id) == [id(1)])
    }

    @Test func noOpenWorktreesDefersEverything() {
        let all = [worktree(1), worktree(2)]

        let order = BootstrapRestoreOrder.partition(
            worktrees: all, openWorktreeIds: [], selectedWorktreeId: nil)

        #expect(order.priority.isEmpty)
        #expect(order.deferred.map(\.id) == [id(1), id(2)])
    }

    @Test func aSelectionOutsideTheOpenSetIsNotPrioritized() {
        // Mounting is driven by openWorktreeIds; a stale selectedWorktreeId
        // pointing outside it must not pull a worktree onto the first-paint path.
        let all = [worktree(1), worktree(2)]

        let order = BootstrapRestoreOrder.partition(
            worktrees: all, openWorktreeIds: [id(1)], selectedWorktreeId: id(2))

        #expect(order.priority.map(\.id) == [id(1)])
        #expect(order.deferred.map(\.id) == [id(2)])
    }

    @Test func duplicateStoredIdsDoNotDuplicateWork() {
        let all = [worktree(1), worktree(2)]

        let order = BootstrapRestoreOrder.partition(
            worktrees: all, openWorktreeIds: [id(1), id(1)], selectedWorktreeId: id(1))

        #expect(order.priority.map(\.id) == [id(1)])
        #expect(order.deferred.map(\.id) == [id(2)])
    }
}
