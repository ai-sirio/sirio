import Foundation
import Testing
import TillerAgents
import TillerCore
import TillerTerminal
import TillerWorkspace

@testable import Tiller

/// The sidebar and the "+" menus create tabs through AppModel, but the
/// universal engine keeps them in `WorkspaceCoordinator.layouts` while the
/// sidebar reads the legacy store. These tests pin the seam: every creation
/// route must land in the layout, and the sidebar's own accessor must report
/// what the layout holds.
@Suite("SidebarEngineParityTests", .serialized)
@MainActor
struct SidebarEngineParityTests {
    /// A worktree created after launch is never passed to
    /// `WorkspaceCoordinator.restore`, so it has no layout and no group —
    /// which used to make every "New Terminal" silently do nothing.
    @Test func newShellTabOpensATabForAWorktreeThatWasNeverRestored() async throws {
        let fixture = makeFixture(suffix: "unrestored")
        defer { fixture.cleanUp() }

        fixture.model.newShellTab(in: fixture.worktree)
        await settle(fixture) { engineTabs(fixture).count == 1 }

        #expect(engineTabs(fixture).count == 1)
    }

    @Test func newShellTabSelectsTheWorktreeItOpensTheTabIn() async throws {
        let fixture = makeFixture(suffix: "selects")
        defer { fixture.cleanUp() }

        fixture.model.newShellTab(in: fixture.worktree)
        await settle(fixture) { engineTabs(fixture).count == 1 }

        #expect(fixture.model.selectedWorktree?.id == fixture.worktree.id)
    }

    /// The sidebar tree reads `workspaceTabs(for:)`; with the engine on it must
    /// project the layout, not the legacy store the engine never writes.
    @Test func sidebarTabsProjectTheEngineLayout() async throws {
        let fixture = makeFixture(suffix: "projection")
        defer { fixture.cleanUp() }

        fixture.model.newShellTab(in: fixture.worktree)
        await settle(fixture) { engineTabs(fixture).count == 1 }
        let engineTab = try #require(engineTabs(fixture).first)

        let rows = fixture.model.workspaceTabs(for: fixture.worktree.id)
        #expect(rows.count == 1)
        #expect(rows.first?.id == engineTab.id.rawValue)
        #expect(rows.first?.title == engineTab.title)
        // A terminal row needs a leaf so the sidebar draws it as a terminal.
        #expect(rows.first?.terminalTree != nil)
        #expect(fixture.model.activeTab(for: fixture.worktree.id)?.id == engineTab.id.rawValue)
    }

    /// Agent panels are spawned from the same context menu. They used to stay
    /// on the legacy path, so the tab existed but the engine renderer — which
    /// draws from the layout — never showed it.
    @Test func spawnAgentOpensATerminalTabInTheEngineLayout() async throws {
        let fixture = makeFixture(suffix: "agent")
        defer { fixture.cleanUp() }
        let adapter = try #require(AgentCatalog.all.first)

        await fixture.model.spawnAgent(adapter, in: fixture.worktree)
        await settle(fixture) { engineTabs(fixture).count == 1 }

        let tab = try #require(engineTabs(fixture).first)
        #expect(tab.title == adapter.displayName)
        #expect(tab.content.kind == .terminal)
    }

    // MARK: - Fixture

    private struct Fixture {
        let model: AppModel
        let worktree: Worktree
        let suiteName: String
        let directory: URL

        func cleanUp() {
            UserDefaults().removePersistentDomain(forName: suiteName)
            try? FileManager.default.removeItem(at: directory)
        }
    }

    /// Deliberately does *not* call `restore`: the bug under test is what
    /// happens to a worktree the coordinator has never seen.
    private func makeFixture(suffix: String) -> Fixture {
        let suiteName = "dev.tiller.Tiller.SidebarEngineParityTests.\(suffix)"
        let defaults = UserDefaults(suiteName: suiteName)!
        defaults.removePersistentDomain(forName: suiteName)
        let directory = FileManager.default.temporaryDirectory
            .appendingPathComponent("tiller-sidebar-parity-\(UUID().uuidString)")
        try? FileManager.default.createDirectory(
            at: directory, withIntermediateDirectories: true)
        let model = AppModel(
            paneRegistry: PaneRegistry(), registrationTimeoutMs: 100, defaults: defaults,
            workspaceCoordinator: WorkspaceCoordinator(
                persistence: FakeWorkspacePersistence(),
                registry: WorkspaceContentRegistry(),
                adapters: [.terminal: TerminalContentAdapter()]))
        let worktree = Worktree(
            id: UUID(), projectId: UUID(), branch: "main", path: directory.path)
        model.worktrees = [worktree.projectId: [worktree]]
        return Fixture(
            model: model, worktree: worktree, suiteName: suiteName, directory: directory)
    }

    private func engineTabs(_ fixture: Fixture) -> [WorkspaceTab] {
        fixture.model.workspaceCoordinator.layouts[fixture.worktree.id]?.allTabs ?? []
    }

    private func settle(_ fixture: Fixture, until condition: () -> Bool) async {
        for _ in 0..<200 where !condition() {
            try? await Task.sleep(for: .milliseconds(10))
        }
    }
}
