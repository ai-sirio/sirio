import Foundation
import Observation
import TillerCore
import TillerTerminal
import TillerWorkspace

enum ContentChoice: Sendable {
    case newTerminal(command: String?)
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
    let legacyStore: LegacyWorkspaceStore

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
         adapters: [WorkspaceContentKind: WorkspaceContentAdapter],
         legacyStore: LegacyWorkspaceStore = LegacyWorkspaceStore()) {
        self.persistence = persistence
        self.registry = registry
        self.adapters = adapters
        self.legacyStore = legacyStore
    }

    func legacyTabs(for worktreeID: UUID) -> [LegacyWorkspaceTab] {
        legacyStore.tabs(for: worktreeID)
    }

    func legacyActiveTabID(for worktreeID: UUID) -> UUID? {
        legacyStore.activeTabID(for: worktreeID)
    }

    func setLegacyTabs(_ tabs: [LegacyWorkspaceTab], for worktreeID: UUID) {
        legacyStore.replaceTabs(tabs, for: worktreeID)
    }

    func appendLegacyTab(_ tab: LegacyWorkspaceTab, to worktreeID: UUID, activate: Bool) {
        legacyStore.appendTab(tab, to: worktreeID, activate: activate)
    }

    func setLegacyActiveTabID(_ tabID: UUID?, for worktreeID: UUID) {
        legacyStore.setActiveTabID(tabID, for: worktreeID)
    }

    func legacyPaneCache(for worktreeID: UUID) -> TerminalPaneCache {
        legacyStore.paneCache(for: worktreeID)
    }

    func legacyMutationRevision(for worktreeID: UUID) -> Int {
        legacyStore.mutationRevision(for: worktreeID)
    }

    func restore(worktree: Worktree) async {
        worktrees[worktree.id] = worktree
        let restoreSignpost = SignpostMetrics.beginInterval("workspaceRestore")
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
        SignpostMetrics.endInterval("workspaceRestore", restoreSignpost)

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

    func activeOrFirstGroup(for worktreeID: UUID) -> PaneGroupID? {
        guard let layout = layouts[worktreeID] else { return nil }
        if layout.group(layout.activeGroupID) != nil {
            return layout.activeGroupID
        }
        return layout.orderedGroupIDs.first
    }

    /// The group a new tab can be inserted into, restoring the worktree first
    /// if the coordinator has never seen it. Only bootstrap calls `restore`,
    /// so a worktree added during the session has no layout — and every
    /// creation route that stopped at `activeOrFirstGroup` returning nil
    /// looked, from the outside, like nothing happened at all.
    func ensureGroup(for worktree: Worktree) async -> PaneGroupID? {
        if let group = activeOrFirstGroup(for: worktree.id) { return group }
        await restore(worktree: worktree)
        return activeOrFirstGroup(for: worktree.id)
    }

    func terminalContentID(for tabID: WorkspaceTabID, in worktreeID: UUID)
        -> TerminalContentID? {
        guard let tab = layouts[worktreeID]?.tab(tabID),
              case .terminal(let contentID) = tab.content else { return nil }
        return contentID
    }

    /// Resolves a terminal tab's stable content id to the key its live PTY is
    /// currently registered under in PaneRegistry (the adapter's current
    /// ResourceGenerationID) — nil if the tab does not exist, is not a
    /// terminal, or has not hydrated a live process yet. Never cached: call
    /// this again after any move/split/relaunch.
    func liveControlPaneId(contentID: TerminalContentID, in worktreeID: UUID) -> UUID? {
        guard let layout = layouts[worktreeID],
              let tab = layout.allTabs.first(where: {
                  if case .terminal(let cid) = $0.content { return cid == contentID }
                  return false
              }),
              let terminalAdapter = adapters[.terminal] as? TerminalContentAdapter,
              let generation = terminalAdapter.generation(for: tab.id)
        else { return nil }
        return generation.rawValue
    }

    func handle(_ intent: WorkspaceIntent, in worktree: Worktree) async {
        worktrees[worktree.id] = worktree
        switch intent {
        case .requestSplit(let anchor, let placement):
            await requestSplit(anchor: anchor, placement: placement,
                               choice: .newTerminal(command: nil), in: worktree)
        case .requestNewTab(let groupID):
            await requestNewTab(
                into: groupID, choice: .newTerminal(command: nil), in: worktree)
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

        let preparedContent = await prepareContent(for: choice, in: worktree)
        if case .moveExistingTab = choice {
            // Moving an existing tab does not need an adapter preparation.
        } else if preparedContent == nil {
            return
        }

        await commitPreparedContent(
            preparedContent: preparedContent,
            in: worktree,
            expectedRevision: capturedRevision
        ) { currentLayout, prepared in
            guard currentLayout.group(anchor) != nil else { return nil }
            if let sourceTabID, currentLayout.tab(sourceTabID) == nil { return nil }

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
            let selectedExistingTabID = sourceTabID ?? existingDocumentTabID
            if let selectedExistingTabID {
                let command: WorkspaceLayoutCommand
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
                return PreparedCommand(
                    command: command,
                    attachPreparedContent: existingDocumentTabID == nil,
                    discardPreparedContent: existingDocumentTabID != nil)
            }
            guard let prepared else { return nil }
            return PreparedCommand(
                command: .splitGroup(
                    anchor: anchor, placement: placement, newGroup: newGroup,
                    newSplit: newSplit, content: .newTab(prepared.tab)),
                attachPreparedContent: true,
                discardPreparedContent: false)
        }
    }

    func requestNewTab(into groupID: PaneGroupID, choice: ContentChoice,
                       in worktree: Worktree) async {
        worktrees[worktree.id] = worktree
        let capturedRevision = revisions[worktree.id] ?? 0
        guard let layout = layouts[worktree.id], layout.group(groupID) != nil else {
            lastRecoverableError = "missing group"
            return
        }
        guard let preparedContent = await prepareContent(for: choice, in: worktree) else {
            return
        }

        await commitPreparedContent(
            preparedContent: preparedContent,
            in: worktree,
            expectedRevision: capturedRevision
        ) { currentLayout, prepared in
            guard currentLayout.group(groupID) != nil, let prepared else { return nil }
            return PreparedCommand(
                command: .insertTab(prepared.tab, into: groupID, index: nil, activate: true),
                attachPreparedContent: true,
                discardPreparedContent: false)
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

    func renameTab(_ id: WorkspaceTabID, title: String, isAutoNamed: Bool,
                   in worktree: Worktree) async {
        worktrees[worktree.id] = worktree
        let expectedRevision = revisions[worktree.id] ?? 0
        await commit(.renameTab(id, title: title, isAutoNamed: isAutoNamed),
                     in: worktree, expectedRevision: expectedRevision)
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

    private func prepareContent(for choice: ContentChoice, in worktree: Worktree)
        async -> PreparedContentBundle? {
        guard let request = contentRequest(for: choice) else { return nil }
        guard let adapter = adapters[request.kind] else {
            lastRecoverableError = "missing content adapter"
            return nil
        }
        do {
            let prepared = try await adapter.prepare(request: request, worktree: worktree)
            return PreparedContentBundle(prepared: prepared, adapter: adapter)
        } catch is CancellationError {
            return nil
        } catch {
            lastRecoverableError = String(describing: error)
            return nil
        }
    }

    private func commitPreparedContent(
        preparedContent: PreparedContentBundle?,
        in worktree: Worktree,
        expectedRevision: Int,
        makeCommand: (WorkspaceLayout, PreparedContent?) -> PreparedCommand?
    ) async {
        let gate = gate(for: worktree.id)
        await gate.acquire()
        defer { Task { await gate.release() } }

        guard revisions[worktree.id] == expectedRevision,
              let currentLayout = layouts[worktree.id] else {
            if let preparedContent {
                await preparedContent.adapter.dispose(prepared: preparedContent.prepared)
            }
            return
        }
        guard let preparedCommand = makeCommand(currentLayout, preparedContent?.prepared) else {
            if let preparedContent {
                await preparedContent.adapter.dispose(prepared: preparedContent.prepared)
            }
            return
        }

        var discardedPreparedContent = false
        if preparedCommand.discardPreparedContent, let preparedContent {
            await preparedContent.adapter.dispose(prepared: preparedContent.prepared)
            discardedPreparedContent = true
        }
        let applySignpost = SignpostMetrics.beginInterval("workspaceCommandApply")
        let applyResult = WorkspaceLayoutEngine.apply(
            preparedCommand.command, to: currentLayout)
        SignpostMetrics.endInterval("workspaceCommandApply", applySignpost)
        guard case .success(let transition) = applyResult else {
            lastRecoverableError = "workspace command rejected"
            if let preparedContent, !discardedPreparedContent {
                await preparedContent.adapter.dispose(prepared: preparedContent.prepared)
            }
            return
        }
        guard await durabilityGate(
            command: preparedCommand.command, transition: transition, worktree: worktree) else {
            if let preparedContent, !discardedPreparedContent {
                await preparedContent.adapter.dispose(prepared: preparedContent.prepared)
            }
            return
        }
        publish(transition, worktreeID: worktree.id)

        if preparedCommand.attachPreparedContent, let preparedContent {
            let host = preparedContent.adapter.makeHost(
                tab: preparedContent.prepared.tab, worktree: worktree)
            registry.adopt(
                host, tab: preparedContent.prepared.tab,
                generation: preparedContent.prepared.generationID)
        }
        fulfilFocus(transition.focusIntent)
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
        let applySignpost = SignpostMetrics.beginInterval("workspaceCommandApply")
        let applyResult = WorkspaceLayoutEngine.apply(command, to: layout)
        SignpostMetrics.endInterval("workspaceCommandApply", applySignpost)
        guard case .success(let transition) = applyResult else {
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
        let commitSignpost = SignpostMetrics.beginInterval("workspaceStructuralCommit")
        do {
            try await persistence.commitStructural(
                worktreeID: worktree.id, revision: revision,
                snapshot: WorkspaceSnapshot(layout: transition.layout),
                tabs: transition.layout.allTabs,
                terminalContents: terminalRecords(in: transition.layout, worktreeID: worktree.id))
            SignpostMetrics.endInterval("workspaceStructuralCommit", commitSignpost)
            return true
        } catch {
            SignpostMetrics.endInterval("workspaceStructuralCommit", commitSignpost)
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
        case .newTerminal(let command): .newTerminal(command: command)
        case .agentTerminal(let agentID): .agentTerminal(agentID: agentID)
        case .newChat(let agentID): .newChat(agentID: agentID)
        case .resumeChat(let id): .resumeChat(id)
        case .openFile(let url, let editor): .openFile(url, editor: editor)
        case .moveExistingTab: nil
        }
    }
}

private struct PreparedContentBundle {
    let prepared: PreparedContent
    let adapter: any WorkspaceContentAdapter
}

private struct PreparedCommand {
    let command: WorkspaceLayoutCommand
    let attachPreparedContent: Bool
    let discardPreparedContent: Bool
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
