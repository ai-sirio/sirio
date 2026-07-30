import Foundation
import Observation
import TillerCore
import TillerWorkspace

enum ContentChoice: Sendable {
    case newTerminal
    case agentTerminal(agentID: String)
    case newChat(agentID: String)
    case resumeChat(ChatContentID)
    case openFile(URL, editor: DocumentEditorKind)
    case moveExistingTab(WorkspaceTabID)
}

@MainActor
@Observable
final class WorkspaceCoordinator: WorkspaceHostProvider {
    let persistence: WorkspaceLayoutPersistence
    let registry: WorkspaceContentRegistry
    let adapters: [WorkspaceContentKind: any WorkspaceContentAdapter]

    private(set) var layouts: [UUID: WorkspaceLayout] = [:]
    private(set) var revisions: [UUID: Int] = [:]
    private(set) var dirtyWorktreeIDs: Set<UUID> = []
    private(set) var lastRecoverableError: String?
    private(set) var lastSemanticDelta: WorkspaceLayoutDelta?
    private(set) var lastFocusIntent: FocusIntent = .none
    private(set) var pendingCleanupObligations: Set<UUID> = []

    private var worktrees: [UUID: Worktree] = [:]
    private var gates: [UUID: WorktreeCommitGate] = [:]

    init(persistence: WorkspaceLayoutPersistence, registry: WorkspaceContentRegistry,
         adapters: [WorkspaceContentKind: WorkspaceContentAdapter]) {
        self.persistence = persistence
        self.registry = registry
        self.adapters = adapters
    }

    func restore(worktree: Worktree) async {
        worktrees[worktree.id] = worktree
        if pendingCleanupObligations.contains(worktree.id) {
            do {
                try await persistence.purge(worktreeID: worktree.id)
                pendingCleanupObligations.remove(worktree.id)
            } catch {
                lastRecoverableError = String(describing: error)
            }
        }

        let restored = await persistence.restore(worktreeID: worktree.id)
        let layout = materialize(restored)
        layouts[worktree.id] = layout
        revisions[worktree.id] = restored.revision
        dirtyWorktreeIDs.remove(worktree.id)

        // Terminal RESOURCES hydrate eagerly here (LC-3: a background shell
        // keeps running once its worktree is mounted, even while its tab is
        // inactive) — but that is the PTY, not the view. HOSTS are never
        // created here for any content kind: `host(for:)` below creates one
        // lazily, the first time the render path actually needs to display a
        // tab. Restoring a worktree with dozens of tabs must not eagerly
        // instantiate a host — terminal, chat, or document — for every one
        // of them; that regresses exactly the restore/reconciliation
        // performance budgets Phase 13 measures (PF-5, PF-6).
        for tab in layout.allTabs where tab.content.kind == .terminal {
            guard let adapter = adapters[tab.content.kind] else { continue }
            await adapter.hydrate(tab: tab, worktree: worktree)
        }
    }

    /// Resolves a tab id to its content host, creating one on first use if
    /// none is cached yet. This is the mechanism issue #6 calls "hydrate
    /// lazily on first use" for chat/document, and it is equally how a
    /// restored terminal tab gets its view the first time it is rendered —
    /// `restore()` above only started its PTY, not its host.
    func host(for tabID: WorkspaceTabID) -> WorkspaceContentHost? {
        if let existing = registry.host(for: tabID) { return existing }
        for (worktreeID, layout) in layouts {
            guard let tab = layout.tab(tabID), let worktree = worktrees[worktreeID],
                  let adapter = adapters[tab.content.kind] else { continue }
            let host = adapter.makeHost(tab: tab, worktree: worktree)
            registry.adopt(host, tab: tab, generation: ResourceGenerationID())
            return host
        }
        return nil
    }

    func handle(_ intent: WorkspaceIntent, in worktree: Worktree) async {
        worktrees[worktree.id] = worktree
        switch intent {
        case .requestSplit(let anchor, let placement):
            await requestSplit(anchor: anchor, placement: placement,
                               choice: .newTerminal, in: worktree)
        case .requestClose(let tabID):
            await closeTab(tabID, in: worktree)
        case .requestMove(let tabID, to: let destination):
            let command: WorkspaceLayoutCommand
            switch destination {
            case .group(let groupID, let index):
                command = .moveTab(tabID, to: .group(groupID, index: index))
            case .edgeSplit(let anchor, let placement):
                command = .moveTab(
                    tabID, to: .newSplit(anchor: anchor, placement: placement,
                                         newGroup: PaneGroupID(), newSplit: SplitID()))
            }
            await commit(command, in: worktree, expectedRevision: revisions[worktree.id] ?? 0)
        case .activateTab(let tabID):
            await commit(.activateTab(tabID), in: worktree,
                         expectedRevision: revisions[worktree.id] ?? 0)
        case .activateGroup(let groupID):
            await commit(.activateGroup(groupID), in: worktree,
                         expectedRevision: revisions[worktree.id] ?? 0)
        case .setPreferredFraction(let splitID, let fraction):
            await commit(.setPreferredFraction(splitID, fraction), in: worktree,
                         expectedRevision: revisions[worktree.id] ?? 0)
        case .retryContent(let tabID):
            await retryContent(tabID, in: worktree)
        }
    }

    func requestSplit(anchor: PaneGroupID, placement: SplitPlacementSide,
                      choice: ContentChoice, in worktree: Worktree) async {
        worktrees[worktree.id] = worktree

        // Step 1: capture stable IDs and preconditions from the current revision.
        let capturedRevision = revisions[worktree.id] ?? 0
        guard let layout = layouts[worktree.id] else {
            lastRecoverableError = "missing layout"
            return
        }
        guard layout.group(anchor) != nil else {
            lastRecoverableError = "missing anchor"
            return
        }
        let sourceTabID: WorkspaceTabID?
        switch choice {
        case .moveExistingTab(let tabID):
            guard layout.tab(tabID) != nil else { return }
            sourceTabID = tabID
        default:
            sourceTabID = nil
        }

        // Step 2: prepare content outside the per-worktree commit gate.
        var prepared: PreparedContent?
        var adapter: (any WorkspaceContentAdapter)?
        if let request = contentRequest(for: choice) {
            guard let contentAdapter = adapters[request.kind] else {
                lastRecoverableError = "missing content adapter"
                return
            }
            adapter = contentAdapter
            do {
                prepared = try await contentAdapter.prepare(request: request, worktree: worktree)
            } catch is CancellationError {
                return
            } catch {
                lastRecoverableError = String(describing: error)
                return
            }
        }

        // Step 3: enter the worktree gate.
        let gate = gate(for: worktree.id)
        await gate.acquire()
        defer { Task { await gate.release() } }

        // Step 4: re-resolve identities and reject stale preconditions.
        guard revisions[worktree.id] == capturedRevision,
              let currentLayout = layouts[worktree.id], currentLayout.group(anchor) != nil else {
            if let prepared, let adapter { await adapter.dispose(prepared: prepared) }
            return
        }
        if let sourceTabID, currentLayout.tab(sourceTabID) == nil {
            if let prepared, let adapter { await adapter.dispose(prepared: prepared) }
            return
        }

        let newGroup = PaneGroupID()
        let newSplit = SplitID()
        let existingDocumentTabID: WorkspaceTabID? = {
            guard let prepared,
                  case .document(let documentID, _) = prepared.tab.content else { return nil }
            return currentLayout.allTabs.first {
                guard case .document(let existingID, _) = $0.content else { return false }
                return existingID == documentID
            }?.id
        }()
        if existingDocumentTabID != nil, let prepared, let adapter {
            await adapter.dispose(prepared: prepared)
        }
        let selectedExistingTabID = sourceTabID ?? existingDocumentTabID
        let command: WorkspaceLayoutCommand
        if let selectedExistingTabID {
            if currentLayout.groupContaining(tab: selectedExistingTabID) == anchor {
                command = .splitGroup(
                    anchor: anchor, placement: placement, newGroup: newGroup,
                    newSplit: newSplit, content: .existingTab(selectedExistingTabID))
            } else {
                command = .moveTab(
                    selectedExistingTabID,
                    to: .newSplit(anchor: anchor, placement: placement,
                                   newGroup: newGroup, newSplit: newSplit))
            }
        } else if let prepared {
            command = .splitGroup(
                anchor: anchor, placement: placement, newGroup: newGroup,
                newSplit: newSplit, content: .newTab(prepared.tab))
        } else {
            return
        }

        // Step 5: apply the pure Core command.
        guard case .success(let transition) = WorkspaceLayoutEngine.apply(command, to: currentLayout) else {
            lastRecoverableError = "workspace command rejected"
            if let prepared, let adapter { await adapter.dispose(prepared: prepared) }
            return
        }

        // Steps 6-8 are shared by the commit helper, kept explicit here so the
        // durability boundary remains before any resource publication.
        let committed = await durabilityGate(
            command: command, transition: transition, worktree: worktree)
        guard committed else {
            if let prepared, let adapter { await adapter.dispose(prepared: prepared) }
            return
        }
        publish(transition, worktreeID: worktree.id)

        if let prepared, let adapter, existingDocumentTabID == nil {
            // Step 8: attach/publish the prepared resource only after commit.
            let host = adapter.makeHost(tab: prepared.tab, worktree: worktree)
            registry.adopt(host, tab: prepared.tab, generation: prepared.generationID)
            fulfilFocus(transition.focusIntent)
        } else {
            fulfilFocus(transition.focusIntent)
        }
    }

    func closeTab(_ id: WorkspaceTabID, in worktree: Worktree) async {
        worktrees[worktree.id] = worktree
        guard let tab = layouts[worktree.id]?.tab(id) else { return }
        let expectedRevision = revisions[worktree.id] ?? 0
        let command = WorkspaceLayoutCommand.closeTab(id)
        guard await commit(command, in: worktree, expectedRevision: expectedRevision) else { return }
        if let adapter = adapters[tab.content.kind] {
            await adapter.close(tab: tab)
        }
        await registry.release(tabID: id)
    }

    func checkpointOnQuit() async {
        for (worktreeID, layout) in layouts {
            let worktree = worktrees[worktreeID] ?? Worktree(
                id: worktreeID, projectId: UUID(), branch: "", path: "")
            for tab in layout.allTabs {
                if let adapter = adapters[tab.content.kind] {
                    await adapter.checkpoint(tab: tab)
                }
            }
            let revision = revisions[worktreeID] ?? 0
            await persistence.checkpoint(
                worktreeID: worktree.id, revision: revision, snapshot: WorkspaceSnapshot(layout: layout))
            do {
                try await persistence.flush(worktreeID: worktree.id)
            } catch {
                dirtyWorktreeIDs.insert(worktreeID)
                lastRecoverableError = String(describing: error)
            }
        }
    }

    func purge(worktree: Worktree) async {
        worktrees[worktree.id] = worktree
        do {
            try await persistence.purge(worktreeID: worktree.id)
            pendingCleanupObligations.remove(worktree.id)
        } catch {
            pendingCleanupObligations.insert(worktree.id)
            lastRecoverableError = String(describing: error)
        }
    }

    private func retryContent(_ id: WorkspaceTabID, in worktree: Worktree) async {
        guard let tab = layouts[worktree.id]?.tab(id), let adapter = adapters[tab.content.kind] else { return }
        let generation = await adapter.retry(tab: tab, worktree: worktree)
        _ = registry.replaceGeneration(tabID: id, generation: generation)
    }

    @discardableResult
    private func commit(_ command: WorkspaceLayoutCommand, in worktree: Worktree,
                        expectedRevision: Int) async -> Bool {
        // Steps 1 and 2 are immediate for commands without a content candidate.
        let gate = gate(for: worktree.id)
        // Step 3: enter the worktree gate.
        await gate.acquire()
        defer { Task { await gate.release() } }
        // Step 4: re-resolve the captured revision and IDs.
        guard revisions[worktree.id] == expectedRevision,
              let layout = layouts[worktree.id] else { return false }
        // Step 5: apply Core.
        guard case .success(let transition) = WorkspaceLayoutEngine.apply(command, to: layout) else {
            lastRecoverableError = "workspace command rejected"
            return false
        }
        // Step 6: durability gate when structural.
        guard await durabilityGate(command: command, transition: transition, worktree: worktree) else {
            return false
        }
        // Step 7: publish layout and semantic delta.
        publish(transition, worktreeID: worktree.id)
        // Step 8: attach existing resources and fulfil focus after attachment.
        fulfilFocus(transition.focusIntent)
        return true
    }

    private func durabilityGate(command: WorkspaceLayoutCommand,
                                transition: WorkspaceLayoutTransition,
                                worktree: Worktree) async -> Bool {
        guard WorkspaceLayoutDelta.isStructuralCommand(command) else { return true }
        let revision = (revisions[worktree.id] ?? 0) + 1
        do {
            try await persistence.commitStructural(
                worktreeID: worktree.id, revision: revision,
                snapshot: WorkspaceSnapshot(layout: transition.layout),
                tabs: transition.layout.allTabs,
                terminalContents: terminalRecords(in: transition.layout, worktreeID: worktree.id))
            return true
        } catch {
            lastRecoverableError = String(describing: error)
            return false
        }
    }

    private func publish(_ transition: WorkspaceLayoutTransition, worktreeID: UUID) {
        layouts[worktreeID] = transition.layout
        revisions[worktreeID, default: 0] += 1
        dirtyWorktreeIDs.remove(worktreeID)
        lastSemanticDelta = transition.delta
        lastFocusIntent = .none
    }

    private func fulfilFocus(_ intent: FocusIntent) {
        guard case .focusTab(let tabID) = intent else {
            lastFocusIntent = intent
            return
        }
        guard registry.host(for: tabID)?.fulfill(intent) == true else { return }
        lastFocusIntent = intent
    }

    private func terminalRecords(in layout: WorkspaceLayout, worktreeID: UUID)
        -> [TerminalContentRecordValue] {
        layout.allTabs.compactMap { tab in
            guard case .terminal(let id) = tab.content else { return nil }
            return TerminalContentRecordValue(
                id: id, worktreeID: worktreeID, launchKind: .shell, commandJSON: nil)
        }
    }

    private func gate(for worktreeID: UUID) -> WorktreeCommitGate {
        if let gate = gates[worktreeID] { return gate }
        let gate = WorktreeCommitGate()
        gates[worktreeID] = gate
        return gate
    }

    private func materialize(_ restored: RestoredWorkspace) -> WorkspaceLayout {
        let groups = restored.layout.groups.mapValues { group in
            PaneGroup(
                id: group.id,
                tabs: group.tabs.map { restored.tabs[$0.id] ?? $0 },
                activeTabID: group.activeTabID)
        }
        if case .success(let layout) = WorkspaceLayout.make(
            root: restored.layout.root, groups: groups,
            activeGroupID: restored.layout.activeGroupID) {
            return layout
        }
        return restored.layout
    }

    private func contentRequest(for choice: ContentChoice) -> ContentRequest? {
        switch choice {
        case .newTerminal: .newTerminal
        case .agentTerminal(let agentID): .agentTerminal(agentID: agentID)
        case .newChat(let agentID): .newChat(agentID: agentID)
        case .resumeChat(let id): .resumeChat(id)
        case .openFile(let url, let editor): .openFile(url, editor: editor)
        case .moveExistingTab: nil
        }
    }
}

private actor WorktreeCommitGate {
    private var held = false
    private var waiters: [CheckedContinuation<Void, Never>] = []

    func acquire() async {
        if !held {
            held = true
            return
        }
        await withCheckedContinuation { waiters.append($0) }
    }

    func release() {
        if let next = waiters.first {
            waiters.removeFirst()
            next.resume()
        } else {
            held = false
        }
    }
}

private extension ContentRequest {
    var kind: WorkspaceContentKind {
        switch self {
        case .newTerminal, .agentTerminal: .terminal
        case .newChat, .resumeChat: .chat
        case .openFile: .document
        }
    }
}
