import AppKit
import Foundation
import Testing
import TillerControl
import TillerCore
import TillerTerminal
import TillerWorkspace
@testable import Tiller

@Suite(.serialized)
@MainActor
struct AppModelControlTests {
    @Test func terminalOpenURLRoutesHTTPToBrowserWithoutRegressingMarkdownAndCodeLinks() async throws {
        guard WorkspaceEngineGate.isEnabled else { return }
        let directory = FileManager.default.temporaryDirectory
            .appendingPathComponent("tiller-terminal-links-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: directory) }

        let markdownURL = directory.appendingPathComponent("README.md")
        let codeURL = directory.appendingPathComponent("Sources.swift")
        try "# Readme\n".write(to: markdownURL, atomically: true, encoding: .utf8)
        try "let value = 1\n".write(to: codeURL, atomically: true, encoding: .utf8)

        let worktree = makeWorktree(path: directory.path)
        let coordinator = WorkspaceCoordinator(
            persistence: ControlWorkspacePersistence(),
            registry: WorkspaceContentRegistry(),
            adapters: [
                .document: DocumentContentAdapter(),
                .browser: BrowserContentAdapter()
            ])
        let model = AppModel(
            paneRegistry: PaneRegistry(), registrationTimeoutMs: 100,
            workspaceCoordinator: coordinator)
        model.worktrees = [worktree.projectId: [worktree]]
        await coordinator.restore(worktree: worktree)

        model.handleTerminalOpenURL(markdownURL.path, in: worktree)
        let markdownTabs = try await waitForUniversalTabs(model, worktreeID: worktree.id, count: 1)
        model.handleTerminalOpenURL(codeURL.path, in: worktree)
        let documentTabs = try await waitForUniversalTabs(model, worktreeID: worktree.id, count: 2)

        model.handleTerminalOpenURL("http://127.0.0.1:4173/fixture", in: worktree)
        let allTabs = try await waitForUniversalTabs(model, worktreeID: worktree.id, count: 3)
        model.handleTerminalOpenURL("https://127.0.0.1:4173/second", in: worktree)
        try await Task.sleep(for: .milliseconds(50))
        let reusedTabs = model.workspaceCoordinator.layouts[worktree.id]?.allTabs ?? []

        #expect(markdownTabs.contains { tab in
            if case .document(_, .markdown) = tab.content { return true }
            return false
        })
        #expect(documentTabs.contains { tab in
            if case .document(_, .code) = tab.content { return true }
            return false
        })
        #expect(allTabs.filter {
            if case .browser = $0.content { return true }
            return false
        }.count == 1)
        #expect(reusedTabs.filter {
            if case .browser = $0.content { return true }
            return false
        }.count == 1)
    }

    @Test func humanShiftCommandUsesSystemBrowserAndAgentSchemesAreIgnored() async throws {
        var systemURLs: [URL] = []
        let model = AppModel(
            paneRegistry: PaneRegistry(), registrationTimeoutMs: 100,
            openSystemURL: { systemURLs.append($0) },
            currentEventModifiers: { [.command, .shift] })
        let worktree = makeWorktree(path: "/tmp/phase5-d20")
        model.worktrees = [worktree.projectId: [worktree]]

        model.handleTerminalOpenURL("https://example.test/escape", in: worktree)
        model.handleAgentOpenURL("mailto:agent@example.test", in: worktree)

        #expect(systemURLs.map(\.absoluteString) == ["https://example.test/escape"])
    }

    /// Title provenance: a manual rename is the user's word and must stop
    /// auto-naming from overwriting it; an auto title must not claim to be
    /// the user's.
    @Test func renameTabDisablesAutoNaming() async throws {
        let (model, worktree, tab) = try await makeDocumentTab(named: "rename-tab")

        #expect(tab.titleIsAutoNamed == true)
        model.renameTab(tab.id.rawValue, in: worktree.id, to: "My custom name")
        let updated = try await settledTab(model, worktree: worktree, id: tab.id) {
            $0.title == "My custom name"
        }

        #expect(updated.title == "My custom name")
        #expect(updated.titleIsAutoNamed == false)
    }

    @Test func browserControlVerbsOpenReadAndNavigateAFileSurface() async throws {
        guard WorkspaceEngineGate.isEnabled else { return }
        let worktree = makeWorktree(path: "/tmp/browser-control")
        let fixture = FileManager.default.temporaryDirectory
            .appendingPathComponent("tiller-browser-control-\(UUID().uuidString).html")
        try "<html><head><title>Control fixture</title></head><body>Control text</body></html>"
            .write(to: fixture, atomically: true, encoding: .utf8)
        defer { try? FileManager.default.removeItem(at: fixture) }

        let coordinator = WorkspaceCoordinator(
            persistence: ControlWorkspacePersistence(),
            registry: WorkspaceContentRegistry(),
            adapters: [.browser: BrowserContentAdapter()])
        let model = AppModel(
            paneRegistry: PaneRegistry(), registrationTimeoutMs: 100,
            workspaceCoordinator: coordinator)
        model.worktrees = [worktree.projectId: [worktree]]
        model.selectedWorktree = worktree
        await coordinator.restore(worktree: worktree)

        let opened = await model.handleControl(request(
            "browser.open", ["url": fixture.absoluteString]))
        #expect(opened.ok)
        let surface = try #require(opened.result?["surface"])
        #expect(UUID(uuidString: surface) != nil)

        let url = await model.handleControl(request(
            "browser.get", ["surface": surface, "what": "url"]))
        #expect(url.result?["value"] == fixture.absoluteString)

        let text = await model.handleControl(request(
            "browser.get", ["surface": "surface:1", "what": "text"]))
        #expect(text.result?["value"]?.contains("Control text") == true)

        let navigated = await model.handleControl(request(
            "browser.navigate", ["surface": surface, "action": "reload"]))
        #expect(navigated.result?["url"] == fixture.absoluteString)

        let invalidAction = await model.handleControl(request(
            "browser.navigate", ["surface": surface, "action": "forwards"]))
        #expect(invalidAction.error
            == "invalid_argument: action must be one of back, forward, reload")

        let invalidWhat = await model.handleControl(request(
            "browser.get", ["surface": surface, "what": "contents"]))
        #expect(invalidWhat.error
            == "invalid_argument: what must be one of url, text, html")

        let unknownSurface = await model.handleControl(request(
            "browser.navigate", ["surface": UUID().uuidString, "action": "reload"]))
        #expect(unknownSurface.error == "surface_not_found")
    }

    @Test func applyAutoTitleLeavesProvenanceUntouched() async throws {
        let (model, worktree, tab) = try await makeDocumentTab(named: "apply-auto-title")

        model.applyAutoTitle(tab.id.rawValue, in: worktree.id, title: "Fix login bug")
        let updated = try await settledTab(model, worktree: worktree, id: tab.id) {
            $0.title == "Fix login bug"
        }

        #expect(updated.title == "Fix login bug")
        #expect(updated.titleIsAutoNamed == true)
    }

    /// A document tab is the cheapest real universal tab: no PTY, no database.
    private func makeDocumentTab(named name: String) async throws
        -> (AppModel, Worktree, WorkspaceTab) {
        let coordinator = WorkspaceCoordinator(
            persistence: ControlWorkspacePersistence(),
            registry: WorkspaceContentRegistry(),
            adapters: [.document: DocumentContentAdapter()])
        let model = AppModel(
            paneRegistry: PaneRegistry(), registrationTimeoutMs: 100,
            workspaceCoordinator: coordinator)
        let directory = FileManager.default.temporaryDirectory
            .appendingPathComponent("tiller-\(name)-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        let url = directory.appendingPathComponent("Note.md")
        try "# Note\n".write(to: url, atomically: true, encoding: .utf8)
        let worktree = makeWorktree(path: directory.path)
        model.worktrees = [worktree.projectId: [worktree]]
        await coordinator.restore(worktree: worktree)
        model.openDocument(fileURL: url, in: worktree)
        var tabs: [WorkspaceTab] = []
        for _ in 0..<200 where tabs.isEmpty {
            tabs = coordinator.layouts[worktree.id]?.allTabs ?? []
            if tabs.isEmpty { try await Task.sleep(for: .milliseconds(10)) }
        }
        return (model, worktree, try #require(tabs.first))
    }

    private func settledTab(_ model: AppModel, worktree: Worktree, id: WorkspaceTabID,
                            until condition: (WorkspaceTab) -> Bool) async throws -> WorkspaceTab {
        for _ in 0..<200 {
            if let tab = model.workspaceCoordinator.layouts[worktree.id]?.tab(id),
               condition(tab) { return tab }
            try await Task.sleep(for: .milliseconds(10))
        }
        return try #require(model.workspaceCoordinator.layouts[worktree.id]?.tab(id))
    }

    private func waitForUniversalTabs(_ model: AppModel, worktreeID: UUID, count: Int) async throws
        -> [WorkspaceTab] {
        for _ in 0..<200 {
            let tabs = model.workspaceCoordinator.layouts[worktreeID]?.allTabs ?? []
            if tabs.count >= count { return tabs }
            try await Task.sleep(for: .milliseconds(10))
        }
        return try #require(model.workspaceCoordinator.layouts[worktreeID]?.allTabs)
    }

    @Test func panelCreateWithTheEngineEnabledResolvesThroughLiveControlPaneId() async {
        guard WorkspaceEngineGate.isEnabled else { return }
        let registry = PaneRegistry()
        let activation = ActivationCounter()
        let model = makeModel(
            registry: registry,
            timeoutMs: 1_000,
            activation: activation
        )
        let target = makeWorktree(path: "/tmp/target")
        let selected = makeWorktree(path: "/tmp/selected")
        model.worktrees = [target.projectId: [target], selected.projectId: [selected]]
        await model.workspaceCoordinator.restore(worktree: target)
        await model.workspaceCoordinator.restore(worktree: selected)
        // A real engine tab, not a legacy-store one: the active-tab accessor
        // reads the layout, which is what panel.create must leave untouched.
        model.newShellTab(in: selected)
        await poll {
            model.workspaceCoordinator.layouts[selected.id]?.allTabs.isEmpty == false
        }
        let selectedTabID = model.workspaceActiveTabID(for: selected.id)
        // Otherwise the two comparisons below could both be nil == nil.
        #expect(selectedTabID != nil)
        model.selectedWorktree = selected
        let priorOpenIds = model.openWorktreeIds
        let response = ResponseBox()

        let requestTask = Task {
            let value = await model.handleControl(request(
                "panel.create", ["worktree": target.path, "cmd": "printf ready"]
            ))
            await response.set(value)
            return value
        }
        await poll {
            model.workspaceCoordinator.layouts[target.id]?.allTabs.isEmpty == false
        }

        #expect(await response.value == nil)
        #expect(model.selectedWorktree?.id == selected.id)
        #expect(model.workspaceActiveTabID(for: selected.id) == selectedTabID)
        #expect(model.workspaceCoordinator.legacyTabs(for: target.id).isEmpty)
        guard let tab = model.workspaceCoordinator.layouts[target.id]?.allTabs.first,
              case .terminal(let contentID) = tab.content,
              let livePaneId = model.workspaceCoordinator.liveControlPaneId(
                  contentID: contentID, in: target.id) else {
            Issue.record("universal terminal did not expose a live control id")
            return
        }
        await registry.register(
            paneId: livePaneId,
            pty: PtyProcess { _ in },
            scrollback: ScrollbackBuffer()
        )

        let result = await requestTask.value
        #expect(result.ok)
        #expect(result.result == ["id": livePaneId.uuidString])
        #expect(model.selectedWorktree?.id == selected.id)
        #expect(model.workspaceActiveTabID(for: selected.id) == selectedTabID)
        #expect(Set(model.openWorktreeIds) == Set(priorOpenIds + [target.id]))
        #expect(activation.count == 0)
    }

    @Test func panelCreateRunsTheRequestedCommandThroughTheUniversalEngine() async {
        guard WorkspaceEngineGate.isEnabled else { return }
        let registry = PaneRegistry()
        let model = makeModel(registry: registry, timeoutMs: 1_000)
        let worktree = makeWorktree(path: "/tmp/universal-command")
        model.worktrees = [worktree.projectId: [worktree]]
        await model.workspaceCoordinator.restore(worktree: worktree)

        let task = Task {
            await model.handleControl(request(
                "panel.create",
                ["worktree": worktree.id.uuidString, "cmd": "printf universal-command"]
            ))
        }
        await poll {
            model.workspaceCoordinator.layouts[worktree.id]?.allTabs.isEmpty == false
        }
        guard let tab = model.workspaceCoordinator.layouts[worktree.id]?.allTabs.first,
              case .terminal(let contentID) = tab.content,
              let adapter = model.workspaceCoordinator.adapters[.terminal]
                  as? TerminalContentAdapter,
              let livePaneId = model.workspaceCoordinator.liveControlPaneId(
                  contentID: contentID, in: worktree.id) else {
            Issue.record("universal terminal was not prepared")
            return
        }
        #expect(adapter.command(for: tab.id) == "printf universal-command")
        await registry.register(
            paneId: livePaneId,
            pty: PtyProcess { _ in },
            scrollback: ScrollbackBuffer()
        )

        let result = await task.value
        #expect(result.ok)
        #expect(result.result == ["id": livePaneId.uuidString])
    }

    @Test func createTimeoutRollsBackTabCommandMountAndLateRegistration() async {
        guard !WorkspaceEngineGate.isEnabled else { return }
        let registry = PaneRegistry()
        let createdPaneId = UUID()
        let activation = ActivationCounter()
        let model = makeModel(
            registry: registry,
            paneId: createdPaneId,
            timeoutMs: 20,
            activation: activation
        )
        let target = makeWorktree(path: "/tmp/target-timeout")
        model.worktrees = [target.projectId: [target]]
        await model.workspaceCoordinator.restore(worktree: target)

        let result = await model.handleControl(request(
            "panel.create", ["worktree": target.id.uuidString, "cmd": "sleep 10"]
        ))

        #expect(result.ok == false)
        #expect(model.workspaceTabs(for: target.id).isEmpty)
        #expect(model.paneCommands[createdPaneId] == nil)
        #expect(model.openWorktreeIds.contains(target.id) == false)
        await registry.register(
            paneId: createdPaneId,
            pty: PtyProcess { _ in },
            scrollback: ScrollbackBuffer()
        )
        #expect(await registry.isRegistered(paneId: createdPaneId) == false)
        #expect(activation.count == 0)
    }

    @Test func splitTimeoutRollsBackOnlyNewLeafAndCommand() async {
        guard !WorkspaceEngineGate.isEnabled else { return }
        let registry = PaneRegistry()
        let createdPaneId = UUID()
        let model = makeModel(
            registry: registry,
            paneId: createdPaneId,
            timeoutMs: 20,
            activation: ActivationCounter()
        )
        let worktree = makeWorktree(path: "/tmp/split-timeout")
        let sourcePaneId = UUID()
        let tab = LegacyWorkspaceTab(id: UUID(), title: "Source", tree: .leaf(id: sourcePaneId))
        model.worktrees = [worktree.projectId: [worktree]]
        model.workspaceCoordinator.setLegacyTabs([tab], for: worktree.id)
        model.workspaceCoordinator.setLegacyActiveTabID(tab.id, for: worktree.id)
        model.openWorktreeIds = [worktree.id]

        let result = await model.handleControl(request(
            "panel.split",
            ["from": sourcePaneId.uuidString, "direction": "left", "cmd": "false"]
        ))

        #expect(result.ok == false)
        #expect(model.workspaceTabs(for: worktree.id).first?.terminalTree == .leaf(id: sourcePaneId))
        #expect(model.paneCommands[createdPaneId] == nil)
        await registry.register(
            paneId: createdPaneId,
            pty: PtyProcess { _ in },
            scrollback: ScrollbackBuffer()
        )
        #expect(await registry.isRegistered(paneId: createdPaneId) == false)
    }

    @Test func listUsesExplicitWorktreeSelector() async {
        guard !WorkspaceEngineGate.isEnabled else { return }
        let model = makeModel()
        let requested = makeWorktree(path: "/tmp/list-requested")
        let selected = makeWorktree(path: "/tmp/list-selected")
        let requestedPaneId = UUID(), selectedPaneId = UUID()
        let requestedTab = LegacyWorkspaceTab(
            id: UUID(), title: "Requested", tree: .leaf(id: requestedPaneId)
        )
        let selectedTab = LegacyWorkspaceTab(
            id: UUID(), title: "Selected", tree: .leaf(id: selectedPaneId)
        )
        model.worktrees = [requested.projectId: [requested], selected.projectId: [selected]]
        model.workspaceCoordinator.setLegacyTabs([requestedTab], for: requested.id)
        model.workspaceCoordinator.setLegacyTabs([selectedTab], for: selected.id)
        model.workspaceCoordinator.setLegacyActiveTabID(requestedTab.id, for: requested.id)
        model.workspaceCoordinator.setLegacyActiveTabID(selectedTab.id, for: selected.id)
        model.selectedWorktree = selected

        let result = await model.handleControl(request(
            "panel.list", ["worktree": requested.path]
        ))
        let rows = result.result?["panels"].flatMap(ControlRows.decode)

        #expect(result.ok)
        #expect(rows?.map { $0["id"] } == [requestedPaneId.uuidString])
        #expect(model.selectedWorktree?.id == selected.id)
    }

    @Test func panelListReflectsUniversalTabsWhenTheEngineIsEnabled() async {
        guard WorkspaceEngineGate.isEnabled else { return }
        let registry = PaneRegistry()
        let model = makeModel(registry: registry, timeoutMs: 1_000)
        let worktree = makeWorktree(path: "/tmp/universal-list")
        model.worktrees = [worktree.projectId: [worktree]]
        await model.workspaceCoordinator.restore(worktree: worktree)

        let createTask = Task {
            await model.handleControl(request(
                "panel.create", ["worktree": worktree.id.uuidString]
            ))
        }
        await poll {
            model.workspaceCoordinator.layouts[worktree.id]?.allTabs.isEmpty == false
        }
        guard let tab = model.workspaceCoordinator.layouts[worktree.id]?.allTabs.first,
              case .terminal(let contentID) = tab.content,
              let livePaneId = model.workspaceCoordinator.liveControlPaneId(
                  contentID: contentID, in: worktree.id) else {
            Issue.record("universal terminal did not expose its live control id")
            return
        }
        await registry.register(
            paneId: livePaneId, pty: PtyProcess { _ in }, scrollback: ScrollbackBuffer())
        let created = await createTask.value
        #expect(created.result?["id"] == livePaneId.uuidString)
        let listed = await model.handleControl(request(
            "panel.list", ["worktree": worktree.id.uuidString]
        ))
        let rows = listed.result?["panels"].flatMap(ControlRows.decode) ?? []

        #expect(listed.ok)
        #expect(rows.count == 1)
        #expect(rows.first?["id"] == livePaneId.uuidString)
        #expect(model.workspaceCoordinator.legacyTabs(for: worktree.id).isEmpty)
        await registry.cancelRegistration(paneId: livePaneId)
    }

    @Test func aMovedTerminalKeepsItsControlIdentityAndLivePty() async {
        guard WorkspaceEngineGate.isEnabled else { return }
        let registry = PaneRegistry()
        let model = makeModel(registry: registry, timeoutMs: 1_000)
        let worktree = makeWorktree(path: "/tmp/universal-move")
        model.worktrees = [worktree.projectId: [worktree]]
        await model.workspaceCoordinator.restore(worktree: worktree)

        let createTask = Task {
            await model.handleControl(request(
                "panel.create", ["worktree": worktree.id.uuidString]
            ))
        }
        await poll {
            model.workspaceCoordinator.layouts[worktree.id]?.allTabs.isEmpty == false
        }
        guard let tab = model.workspaceCoordinator.layouts[worktree.id]?.allTabs.first,
              case .terminal(let contentID) = tab.content,
              let controlID = model.workspaceCoordinator.liveControlPaneId(
                  contentID: contentID, in: worktree.id) else {
            Issue.record("universal terminal was not created")
            return
        }
        let pty = PtyProcess { _ in }
        try? pty.spawn(
            executable: "/bin/sh", arguments: ["-c", "sleep 30"],
            environment: ["PATH=/usr/bin:/bin"], initialCols: 80, initialRows: 24)
        await registry.register(
            paneId: controlID, pty: pty, scrollback: ScrollbackBuffer())
        let created = await createTask.value
        #expect(created.result?["id"] == controlID.uuidString)
        let pidBefore = await registry.shellPid(paneId: controlID)
        #expect(pidBefore != nil)

        let groupID = model.workspaceCoordinator.layouts[worktree.id]!.activeGroupID
        await model.workspaceCoordinator.handle(
            .requestMove(tab.id, to: .edgeSplit(anchor: groupID, placement: .right)),
            in: worktree)

        guard let movedTab = model.workspaceCoordinator.layouts[worktree.id]?.tab(tab.id),
              case .terminal(let movedContentID) = movedTab.content else {
            Issue.record("moved terminal tab disappeared")
            return
        }
        let movedControlID = model.workspaceCoordinator.liveControlPaneId(
            contentID: movedContentID, in: worktree.id)
        #expect(movedControlID == controlID)
        #expect(await registry.shellPid(paneId: controlID) == pidBefore)
        await registry.cancelRegistration(paneId: controlID)
    }

    @Test func staleOrNonTerminalIdsReturnTypedErrorsWithNoMutation() async {
        guard WorkspaceEngineGate.isEnabled else { return }
        let model = makeModel()
        let worktree = makeWorktree(path: "/tmp/universal-stale")
        model.worktrees = [worktree.projectId: [worktree]]
        await model.workspaceCoordinator.restore(worktree: worktree)
        let staleID = UUID().uuidString

        let read = await model.handleControl(request("panel.read", ["id": staleID]))
        let write = await model.handleControl(request(
            "panel.write", ["id": staleID, "input": "ignored"]
        ))
        let key = await model.handleControl(request(
            "panel.key", ["id": staleID, "key": "enter"]
        ))
        let wait = await model.handleControl(request(
            "panel.wait", ["id": staleID, "timeoutMs": "0"]
        ))
        let focus = await model.handleControl(request("panel.focus", ["id": staleID]))
        let close = await model.handleControl(request("panel.close", ["id": staleID]))

        #expect(read.error == "unknown panel")
        #expect(write.error == "unknown panel")
        #expect(key.error == "unknown panel")
        #expect(wait.error == "unknown panel or timeout")
        #expect(focus.error == "unknown panel")
        #expect(close.error == "unknown panel")
        #expect(model.workspaceCoordinator.layouts[worktree.id]?.allTabs.isEmpty == true)
    }

    @Test func disablingTheSocketDisablesNoLocalWorkspaceFeature() async {
        guard WorkspaceEngineGate.isEnabled else { return }
        let model = makeModel()
        let worktree = makeWorktree(path: "/tmp/socket-independent-workspace")
        model.worktrees = [worktree.projectId: [worktree]]
        await model.workspaceCoordinator.restore(worktree: worktree)
        model.setControlSocketEnabled(false)

        model.newShellTab(in: worktree)
        await poll {
            model.workspaceCoordinator.layouts[worktree.id]?.allTabs.isEmpty == false
        }

        #expect(model.workspaceCoordinator.layouts[worktree.id]?.allTabs.count == 1)
        model.setControlSocketEnabled(false)
    }

    @Test func closeRejectsUnknownPaneAndRemovesKnownPaneBeforeReturning() async {
        guard !WorkspaceEngineGate.isEnabled else { return }
        let registry = PaneRegistry()
        let model = makeModel(registry: registry)
        let worktree = makeWorktree(path: "/tmp/close")
        let paneId = UUID()
        let tab = LegacyWorkspaceTab(id: UUID(), title: "Close", tree: .leaf(id: paneId))
        model.worktrees = [worktree.projectId: [worktree]]
        model.workspaceCoordinator.setLegacyTabs([tab], for: worktree.id)
        model.workspaceCoordinator.setLegacyActiveTabID(tab.id, for: worktree.id)

        let unknown = await model.handleControl(request(
            "panel.close", ["id": UUID().uuidString]
        ))
        #expect(unknown.ok == false)

        await registry.register(
            paneId: paneId,
            pty: PtyProcess { _ in },
            scrollback: ScrollbackBuffer()
        )
        let known = await model.handleControl(request(
            "panel.close", ["id": paneId.uuidString]
        ))

        #expect(known.ok)
        #expect(model.workspaceTabContaining(paneId: paneId) == nil)
        #expect(await registry.isRegistered(paneId: paneId) == false)
        #expect(await registry.write(paneId: paneId, data: Data("stale".utf8)) == false)
    }

    @Test func capabilitiesAdvertiseCanonicalPanelsWithoutLegacyMethods() async {
        let model = makeModel()
        let result = await model.handleControl(request("system.capabilities"))
        let rows = result.result?["methods"].flatMap(ControlRows.decode) ?? []
        let methods = Set(rows.compactMap { $0["method"] })
        let canonical: Set<String> = [
            "panel.create", "panel.split", "panel.list", "panel.write", "panel.key",
            "panel.read", "panel.wait", "panel.focus", "panel.close",
        ]
        let legacy: Set<String> = [
            "surface.list", "pane.surfaces", "surface.focus", "surface.split",
            "surface.send_text", "surface.send_key", "surface.close",
        ]

        #expect(canonical.isSubset(of: methods))
        #expect(methods.isDisjoint(with: legacy))
    }

    @Test func activationCallbackIsInvokedOnlyByFocus() async {
        guard !WorkspaceEngineGate.isEnabled else { return }
        let activation = ActivationCounter()
        let model = makeModel(activation: activation)
        let owning = makeWorktree(path: "/tmp/focus-owning")
        let selected = makeWorktree(path: "/tmp/focus-selected")
        let paneId = UUID()
        let owningTab = LegacyWorkspaceTab(id: UUID(), title: "Owning", tree: .leaf(id: paneId))
        model.worktrees = [owning.projectId: [owning], selected.projectId: [selected]]
        model.workspaceCoordinator.setLegacyTabs([owningTab], for: owning.id)
        model.selectedWorktree = selected

        let controller = NSViewController()
        let focusView = AppModelFocusableView()
        controller.view = focusView
        model.workspacePaneCache(for: owning.id).controllers[paneId] = controller
        let host = NSView(frame: NSRect(x: 0, y: 0, width: 100, height: 100))
        host.addSubview(focusView)
        let window = NSWindow(
            contentRect: host.bounds,
            styleMask: .borderless,
            backing: .buffered,
            defer: false
        )
        window.contentView = host

        let listed = await model.handleControl(request(
            "panel.list", ["worktree": owning.id.uuidString]
        ))
        #expect(listed.ok)
        #expect(activation.count == 0)

        let focused = await model.handleControl(request(
            "panel.focus", ["id": paneId.uuidString]
        ))
        #expect(focused.ok)
        #expect(activation.count == 1)
        #expect(model.selectedWorktree?.id == owning.id)
        #expect(model.workspaceActiveTabID(for: owning.id) == owningTab.id)
        #expect(window.firstResponder === focusView)
    }

    @Test func focusFailsWhenPaneNeverAttaches() async {
        guard !WorkspaceEngineGate.isEnabled else { return }
        let activation = ActivationCounter()
        let model = makeModel(timeoutMs: 20, activation: activation)
        let worktree = makeWorktree(path: "/tmp/focus-unattached")
        let paneId = UUID()
        let tab = LegacyWorkspaceTab(id: UUID(), title: "Hidden", tree: .leaf(id: paneId))
        model.worktrees = [worktree.projectId: [worktree]]
        model.workspaceCoordinator.setLegacyTabs([tab], for: worktree.id)

        let result = await model.handleControl(request(
            "panel.focus", ["id": paneId.uuidString]
        ))

        #expect(result.ok == false)
        #expect(result.error == "panel could not be focused")
        #expect(model.selectedWorktree?.id == worktree.id)
        #expect(model.workspaceActiveTabID(for: worktree.id) == tab.id)
        #expect(activation.count == 1)
    }

    @Test func focusRejectsUnknownInvalidAndMissingPanelIds() async {
        let model = makeModel()
        let unknown = await model.handleControl(request(
            "panel.focus", ["id": UUID().uuidString]
        ))
        let invalid = await model.handleControl(request(
            "panel.focus", ["id": "not-a-uuid"]
        ))
        let missing = await model.handleControl(request("panel.focus"))

        #expect(unknown.error == "unknown panel")
        #expect(invalid.error == "unknown panel")
        #expect(missing.error == "unknown panel")
    }

    /// Asserts the cancellation SEMANTICS, not a stopwatch. The cancelled and
    /// timed-out paths used to return the same error, leaving elapsed time as
    /// the only discriminator — so this test was really a race against AppKit
    /// activation cost, and it lost at both a 250ms and a 500ms bound. Now the
    /// cancelled path has its own error, which is deterministic. The timing
    /// bound stays only as a generous guard against an outright hang: it must
    /// be well under the 3s timeout to prove the wait was short-circuited.
    @Test func cancelledFocusReturnsPromptly() async {
        guard !WorkspaceEngineGate.isEnabled else { return }
        let model = makeModel(timeoutMs: 3_000)
        let worktree = makeWorktree(path: "/tmp/focus-cancelled")
        let paneId = UUID()
        let tab = LegacyWorkspaceTab(id: UUID(), title: "Hidden", tree: .leaf(id: paneId))
        model.worktrees = [worktree.projectId: [worktree]]
        model.workspaceCoordinator.setLegacyTabs([tab], for: worktree.id)

        let task = Task {
            await model.handleControl(request("panel.focus", ["id": paneId.uuidString]))
        }
        await Task.yield()
        let clock = ContinuousClock()
        let start = clock.now
        task.cancel()
        let result = await task.value
        let elapsed = start.duration(to: clock.now)

        #expect(result.error == "panel focus cancelled")
        #expect(elapsed < .milliseconds(2_000))
    }

    @Test func createFailsAndRollsBackWhenPersistenceFails() async {
        guard !WorkspaceEngineGate.isEnabled else { return }
        let registry = PaneRegistry()
        let paneId = UUID()
        let model = makeModel(
            registry: registry,
            paneId: paneId,
            controlTabPersister: { _, _, _ in throw ControlPersistenceFailure.failed }
        )
        let worktree = makeWorktree(path: "/tmp/persist-failure")
        model.worktrees = [worktree.projectId: [worktree]]
        await model.workspaceCoordinator.restore(worktree: worktree)

        let task = Task { await model.handleControl(self.request(
            "panel.create", ["worktree": worktree.id.uuidString]
        )) }
        try? await Task.sleep(for: .milliseconds(20))
        await registry.register(
            paneId: paneId, pty: PtyProcess { _ in }, scrollback: ScrollbackBuffer()
        )
        let result = await task.value

        #expect(result.ok == false)
        #expect(model.workspaceTabContaining(paneId: paneId) == nil)
        #expect(model.openWorktreeIds.contains(worktree.id) == false)
        #expect(await registry.isRegistered(paneId: paneId) == false)
    }

    @Test func createResponseWaitsForOrderedPersistence() async {
        guard !WorkspaceEngineGate.isEnabled else { return }
        let registry = PaneRegistry()
        let paneId = UUID()
        let gate = PersistenceGate()
        let response = ResponseBox()
        let model = makeModel(
            registry: registry,
            paneId: paneId,
            timeoutMs: 1_000,
            controlTabPersister: { _, _, _ in await gate.wait() }
        )
        let worktree = makeWorktree(path: "/tmp/persist-order")
        model.worktrees = [worktree.projectId: [worktree]]
        await model.workspaceCoordinator.restore(worktree: worktree)

        let task = Task {
            let value = await model.handleControl(self.request(
                "panel.create", ["worktree": worktree.id.uuidString]
            ))
            await response.set(value)
            return value
        }
        try? await Task.sleep(for: .milliseconds(20))
        await registry.register(
            paneId: paneId, pty: PtyProcess { _ in }, scrollback: ScrollbackBuffer()
        )
        await poll { await gate.started }

        #expect(await gate.started)
        #expect(await response.value == nil)
        await gate.release()
        #expect(await task.value.ok)
    }

    @Test func concurrentCreateTimeoutNeverPersistsPendingPane() async {
        guard !WorkspaceEngineGate.isEnabled else { return }
        let registry = PaneRegistry()
        let successfulPaneId = UUID()
        let failedPaneId = UUID()
        var generatedPaneIds = [successfulPaneId, failedPaneId]
        let recorder = ControlPersistenceRecorder()
        let workspaceCoordinator = WorkspaceCoordinator(
            persistence: ControlWorkspacePersistence(),
            registry: WorkspaceContentRegistry(),
            adapters: [.terminal: TerminalContentAdapter()]
        )
        let model = AppModel(
            paneRegistry: registry,
            registrationTimeoutMs: 1_000,
            paneIdGenerator: { generatedPaneIds.removeFirst() },
            activateApplication: {},
            controlTabPersister: { _, tabs, _ in
                await recorder.record(tabs.flatMap(\.leafIds))
            },
            workspaceCoordinator: workspaceCoordinator
        )
        let worktree = makeWorktree(path: "/tmp/concurrent-persistence")
        model.worktrees = [worktree.projectId: [worktree]]
        await workspaceCoordinator.restore(worktree: worktree)

        let successfulTask = Task {
            await model.handleControl(request(
                "panel.create", ["worktree": worktree.id.uuidString]
            ))
        }
        try? await Task.sleep(for: .milliseconds(10))
        let failedTask = Task {
            await model.handleControl(request(
                "panel.create", ["worktree": worktree.id.uuidString]
            ))
        }
        try? await Task.sleep(for: .milliseconds(20))
        await registry.register(
            paneId: successfulPaneId,
            pty: PtyProcess { _ in },
            scrollback: ScrollbackBuffer()
        )

        let successful = await successfulTask.value
        let failed = await failedTask.value
        let snapshots = await recorder.snapshots

        #expect(successful.ok)
        #expect(failed.error == "panel did not register before timeout")
        #expect(snapshots.allSatisfy { !$0.contains(failedPaneId) })
    }

    private func makeModel(
        registry: PaneRegistry = PaneRegistry(),
        paneId: UUID = UUID(),
        timeoutMs: Int = 100,
        activation: ActivationCounter = ActivationCounter(),
        controlTabPersister: AppModel.ControlTabPersister? = nil
    ) -> AppModel {
        let workspaceCoordinator = WorkspaceCoordinator(
            persistence: ControlWorkspacePersistence(),
            registry: WorkspaceContentRegistry(),
            adapters: [.terminal: TerminalContentAdapter()]
        )
        return AppModel(
            paneRegistry: registry,
            registrationTimeoutMs: timeoutMs,
            paneIdGenerator: { paneId },
            activateApplication: { activation.count += 1 },
            controlTabPersister: controlTabPersister,
            workspaceCoordinator: workspaceCoordinator
        )
    }

    private func makeWorktree(path: String) -> Worktree {
        Worktree(
            id: UUID(),
            projectId: UUID(),
            branch: "main",
            path: path
        )
    }

    private func request(
        _ method: String,
        _ params: [String: String] = [:]
    ) -> ControlRequest {
        ControlRequest(id: UUID().uuidString, method: method, params: params)
    }
}

private actor ControlWorkspacePersistence: WorkspaceLayoutPersistence {
    func restore(worktreeID: UUID) async -> RestoredWorkspace {
        RestoredWorkspace(
            layout: .empty(groupID: PaneGroupID(worktreeID)),
            tabs: [:], revision: 0, diagnostics: [])
    }

    func commitStructural(worktreeID: UUID, revision: Int, snapshot: WorkspaceSnapshot,
                          tabs: [WorkspaceTab],
                          terminalContents: [TerminalContentRecordValue],
                          browserContents: [BrowserContentRecordValue]) async throws {}

    func checkpoint(worktreeID: UUID, revision: Int, snapshot: WorkspaceSnapshot) async {}
    func flush(worktreeID: UUID) async throws {}
    nonisolated func writeRecoverySidecar(
        worktreeID: UUID, revision: Int, snapshot: WorkspaceSnapshot) throws {}
    func purge(worktreeID: UUID) async throws {}
}

@MainActor
private final class ActivationCounter {
    var count = 0
}

private final class AppModelFocusableView: NSView {
    override var acceptsFirstResponder: Bool { true }
}

private actor ResponseBox {
    private(set) var value: ControlResponse?
    func set(_ value: ControlResponse) { self.value = value }
}

private enum ControlPersistenceFailure: Error { case failed }

private actor ControlPersistenceRecorder {
    private(set) var snapshots: [[UUID]] = []

    func record(_ paneIds: [UUID]) {
        snapshots.append(paneIds)
    }
}

/// Polls `condition` until it's true or `timeout` elapses, instead of
/// guessing a fixed sleep duration for async state to settle.
@MainActor
private func poll(
    timeout: Duration = .milliseconds(1_000),
    interval: Duration = .milliseconds(5),
    _ condition: () async -> Bool
) async {
    let deadline = ContinuousClock.now + timeout
    while ContinuousClock.now < deadline {
        if await condition() { return }
        try? await Task.sleep(for: interval)
    }
}

private actor PersistenceGate {
    private(set) var started = false
    private var continuation: CheckedContinuation<Void, Never>?

    func wait() async {
        started = true
        await withCheckedContinuation { continuation = $0 }
    }

    func release() {
        continuation?.resume()
        continuation = nil
    }
}
