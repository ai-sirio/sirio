import AppKit
import Foundation
import Testing
import TillerCore
import TillerWorkspace
@testable import Tiller

@Suite(.serialized)
@MainActor
struct WorkspaceContentAdapterTests {
    @Test func aDetachedHostIsNotTornDown() async {
        let registry = WorkspaceContentRegistry()
        let tab = terminalTab()
        let host = RecordingHost(tabID: tab.id)
        registry.adopt(host, tab: tab, generation: ResourceGenerationID())

        _ = registry.host(for: tab.id)

        #expect(registry.host(for: tab.id) === host)
        #expect(host.releaseCount == 0)
    }

    @Test func terminalHydratesWithTheMountedWorktreeEvenWhenItsTabIsInactive() async {
        let recorder = AdapterBoundaryRecorder()
        let adapter = TerminalContentAdapter(boundary: recorder.boundary)
        let tab = terminalTab()
        let worktree = fixtureWorktree()

        await adapter.hydrate(tab: tab, worktree: worktree)

        #expect(await recorder.hydratedWorktreeIDs == [worktree.id])
        #expect(await adapter.phase(for: tab.id) == .active)
    }

    @Test func chatAndDocumentHydrateOnlyOnFirstActivation() async throws {
        let chatRecorder = AdapterBoundaryRecorder()
        let chat = ChatContentAdapter(boundary: chatRecorder.boundary)
        chat.makeSession = { _, _ in "session-hydration" }
        let chatTab = try #require(
            chat.prepareTab(request: .newChat(agentID: "codex"), worktreeID: UUID()))
        let documentRecorder = AdapterBoundaryRecorder()
        let document = DocumentContentAdapter(boundary: documentRecorder.boundary)
        let documentTab = try #require(document.prepareTab(
            request: .openFile(URL(fileURLWithPath: "/tmp/note.md"), editor: .markdown)))
        let worktree = fixtureWorktree()

        #expect(await chatRecorder.hydratedWorktreeIDs.isEmpty)
        #expect(await documentRecorder.hydratedWorktreeIDs.isEmpty)

        await chat.hydrate(tab: chatTab, worktree: worktree)
        await chat.hydrate(tab: chatTab, worktree: worktree)
        await document.hydrate(tab: documentTab, worktree: worktree)
        await document.hydrate(tab: documentTab, worktree: worktree)

        #expect(await chatRecorder.hydrationCount == 1)
        #expect(await documentRecorder.hydrationCount == 1)
    }

    @Test func staleGenerationCallbacksAreIgnored() async {
        let registry = WorkspaceContentRegistry()
        let tab = terminalTab()
        let first = ResourceGenerationID()
        let second = ResourceGenerationID()
        registry.adopt(RecordingHost(tabID: tab.id), tab: tab, generation: first)
        registry.adopt(RecordingHost(tabID: tab.id), tab: tab, generation: second)

        #expect(!registry.publish(tabID: tab.id, generation: first, phase: .failed(reason: "late")))
        #expect(registry.phase(for: tab.id) == .dormant)
        #expect(registry.publish(tabID: tab.id, generation: second, phase: .active))
        #expect(registry.phase(for: tab.id) == .active)
    }

    @Test func closeReleasesEachRuntimeExactlyOnce() async {
        let recorder = AdapterBoundaryRecorder()
        let adapter = TerminalContentAdapter(boundary: recorder.boundary)
        let tab = terminalTab()
        let worktree = fixtureWorktree()
        let prepared = try? await adapter.prepare(
            request: .newTerminal(command: nil), worktree: worktree)

        await adapter.close(tab: tab)
        await adapter.close(tab: tab)

        #expect(await recorder.closeCount == 1)
        _ = prepared
    }

    @Test func retryMintsANewGenerationAndClearsTheFailedPhase() async {
        let adapter = TerminalContentAdapter(boundary: AdapterBoundaryRecorder().boundary)
        let tab = terminalTab()
        let worktree = fixtureWorktree()
        await adapter.markFailed(tabID: tab.id, reason: "launch failed")
        let first = await adapter.generation(for: tab.id)

        let second = await adapter.retry(tab: tab, worktree: worktree)

        #expect(second != first)
        #expect(await adapter.phase(for: tab.id) == .preparing)
    }

    @Test func interruptedChatTurnsAreRetainedAndNeverReissued() async throws {
        let recorder = AdapterBoundaryRecorder()
        let adapter = ChatContentAdapter(boundary: recorder.boundary)
        let tab = try #require(adapter.prepareTab(
            request: .resumeChat(ChatContentID("session-1")), worktreeID: UUID()))
        let worktree = fixtureWorktree()
        await adapter.recordInterruptedTurn(tabID: tab.id, text: "keep this")

        await adapter.hydrate(tab: tab, worktree: worktree)

        #expect(await adapter.interruptedTurns(for: tab.id) == ["keep this"])
        #expect(await recorder.reissuedSessionIDs.isEmpty)
    }

    @Test func missingDocumentsKeepTheirBufferAndRequireAnExplicitChoice() async throws {
        let adapter = DocumentContentAdapter(boundary: AdapterBoundaryRecorder().boundary)
        let worktree = fixtureWorktree()
        let prepared = try await adapter.prepare(
            request: .openFile(URL(fileURLWithPath: "/tmp/missing.md"), editor: .markdown),
            worktree: worktree)
        let tab = prepared.tab
        guard case .document(let documentID, editor: _) = tab.content else {
            Issue.record("the prepared content must be a document")
            return
        }
        #expect(documentID.worktreeID == worktree.id)
        await adapter.setBuffer("unsaved buffer", for: tab.id)
        await adapter.markMissing(tabID: tab.id)

        await adapter.hydrate(tab: tab, worktree: fixtureWorktree())

        #expect(await adapter.documentDetail(for: tab.id) == .missing)
        #expect(await adapter.buffer(for: tab.id) == "unsaved buffer")
        #expect(await adapter.requiresExplicitChoice(tabID: tab.id))
    }

    @Test func disposalOfAPreparedCandidateIsIdempotent() async {
        let recorder = AdapterBoundaryRecorder()
        let adapter = TerminalContentAdapter(boundary: recorder.boundary)
        let prepared = try? await adapter.prepare(
            request: .newTerminal(command: nil), worktree: fixtureWorktree())
        guard let prepared else {
            Issue.record("the fake terminal boundary must prepare a candidate")
            return
        }

        await adapter.dispose(prepared: prepared)
        await adapter.dispose(prepared: prepared)

        #expect(await recorder.disposeCount == 1)
    }

    private func terminalTab() -> WorkspaceTab {
        let id = WorkspaceTabID(UUID(uuidString: "00000000-0000-4000-8000-000000000001")!)
        return WorkspaceTab(
            id: id, title: "Terminal", titleIsAutoNamed: true,
            content: .terminal(TerminalContentID(UUID(uuidString: "00000000-0000-4000-8000-000000000002")!)))
    }

    private func fixtureWorktree() -> Worktree {
        Worktree(
            id: UUID(uuidString: "00000000-0000-4000-8000-000000000099")!,
            projectId: UUID(uuidString: "00000000-0000-4000-8000-000000000098")!,
            branch: "main", path: "/tmp/tiller-worktree")
    }
}

@MainActor
private final class RecordingHost: WorkspaceContentHost {
    let tabID: WorkspaceTabID
    let viewController = NSViewController()
    private(set) var releaseCount = 0

    init(tabID: WorkspaceTabID) { self.tabID = tabID }

    func setVisible(_ isVisible: Bool) {}
    func fulfill(_ intent: FocusIntent) -> Bool { true }
    func releaseRuntime() { releaseCount += 1 }
}

private actor AdapterBoundaryRecorder {
    var hydratedWorktreeIDs: [UUID] = []
    var hydrationCount = 0
    var closeCount = 0
    var disposeCount = 0
    var reissuedSessionIDs: [String] = []

    nonisolated var boundary: AdapterBoundary {
        AdapterBoundary(
            hydrate: { [weak self] _, worktree in
                await self?.recordHydration(worktreeID: worktree.id)
            },
            close: { [weak self] _ in await self?.recordClose() },
            dispose: { [weak self] _ in await self?.recordDispose() },
            reissue: { [weak self] sessionID in await self?.recordReissue(sessionID) })
    }

    func recordHydration(worktreeID: UUID) {
        hydratedWorktreeIDs.append(worktreeID)
        hydrationCount += 1
    }

    func recordClose() { closeCount += 1 }
    func recordDispose() { disposeCount += 1 }
    func recordReissue(_ sessionID: String) { reissuedSessionIDs.append(sessionID) }
}
