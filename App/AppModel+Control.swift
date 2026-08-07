import Foundation
import AppKit
import TillerCore
import TillerControl
import TillerTerminal
import TillerBrowser

/// cmux-parity control methods. The core methods (panel.*, notify,
/// session.ref, worktree.set) stay in AppModel.handleControl; everything
/// added for CLI parity dispatches here.
extension AppModel {
    /// Complete system.capabilities payload for the control socket.
    static let cmuxMethods: [String] = [
        "panel.create", "panel.split", "panel.list",
        "panel.write", "panel.key", "panel.read",
        "panel.wait", "panel.focus", "panel.close",
        "notify", "session.ref", "worktree.set",
        "system.ping", "system.capabilities", "system.identify",
        "workspace.list", "workspace.create", "workspace.select",
        "workspace.current", "workspace.close",
        "notification.create", "notification.list", "notification.clear",
        "session.restore",
        "browser.open", "browser.navigate", "browser.get", "browser.screenshot",
        "browser.snapshot", "browser.act", "browser.wait", "browser.eval", "browser.console",
    ]

    /// Re-apply the launch snapshot: remount worktree hosts and add back
    /// tabs the user closed since launch. Existing state is left untouched
    /// (idempotent). Re-added agent panes get their resume command back;
    /// scrollback reattaches via loadScrollback on remount (records are
    /// never deleted on close). Returns re-added tabs + remounted worktrees.
    func restoreLaunchSnapshot() -> Int {
        guard let snapshot = launchSnapshot else { return 0 }
        var restored = 0
        // Worktrees first: a restored tab is invisible while its terminal
        // host is unmounted.
        for worktreeId in snapshot.openWorktreeIds
        where worktree(byId: worktreeId) != nil && !openWorktreeIds.contains(worktreeId) {
            openWorktreeIds.append(worktreeId)
            restored += 1
        }
        for (worktreeId, snapshotTabs) in snapshot.tabs {
            guard worktree(byId: worktreeId) != nil else { continue }
            let currentIds = Set(workspaceCoordinator.legacyTabs(for: worktreeId).map(\.id))
            var added = 0
            for tab in snapshotTabs where !currentIds.contains(tab.id) {
                workspaceCoordinator.appendLegacyTab(tab, to: worktreeId, activate: false)
                added += 1
                for paneId in tab.leafIds {
                    if let command = snapshot.paneCommands[paneId] {
                        paneCommands[paneId] = command
                        watchExit(paneId: paneId)
                    }
                }
            }
            if added > 0 { workspacePersistTabs(for: worktreeId) }
            restored += added
        }
        return restored
    }

    /// Resolve a worktree from a UUID string or absolute path — same dual
    /// selector the legacy worktree.set method accepts.
    func resolveWorktree(_ selector: String) -> Worktree? {
        if let uuid = UUID(uuidString: selector) {
            return worktrees.values.flatMap { $0 }.first { $0.id == uuid }
        }
        return worktrees.values.flatMap { $0 }.first { $0.path == selector }
    }

    /// Default branch name for workspace.create when --branch is omitted.
    static func generatedBranchName(now: Date = Date()) -> String {
        let fmt = DateFormatter()
        fmt.dateFormat = "yyyyMMdd-HHmmss"
        fmt.locale = Locale(identifier: "en_US_POSIX")
        return "wt-\(fmt.string(from: now))"
    }

    func handleCmuxControl(_ request: ControlRequest) async -> ControlResponse {
        switch request.method {
        case "system.ping":
            return .success(id: request.id, result: ["pong": "true"])

        case "system.capabilities":
            return .success(id: request.id, result: [
                "methods": ControlRows.encode(Self.cmuxMethods.map { ["method": $0] }),
                "socketEnabled": "true",   // we answered, therefore it's on
            ])

        case "system.identify":
            return identify(request)

        case "workspace.list":
            return .success(id: request.id, result: [
                "workspaces": ControlRows.encode(ControlListing.workspaceRows(
                    projects: projects, worktrees: worktrees,
                    selectedWorktreeId: selectedWorktree?.id))
            ])

        case "workspace.current":
            guard let wt = selectedWorktree else {
                return .failure(id: request.id, error: "no workspace selected")
            }
            return .success(id: request.id, result: [
                "id": wt.id.uuidString, "branch": wt.branch, "path": wt.path,
                "project": projects.first { $0.id == wt.projectId }?.name ?? "",
            ])

        case "workspace.select":
            guard let selector = request.params["workspace"] else {
                return .failure(id: request.id, error: "missing workspace")
            }
            guard let wt = resolveWorktree(selector) else {
                return .failure(id: request.id, error: "unknown workspace \(selector)")
            }
            selectedWorktree = wt
            NSApp.activate(ignoringOtherApps: false)
            return .success(id: request.id, result: ["id": wt.id.uuidString])

        case "workspace.close":
            guard let selector = request.params["workspace"] else {
                return .failure(id: request.id, error: "missing workspace")
            }
            guard let wt = resolveWorktree(selector) else {
                return .failure(id: request.id, error: "unknown workspace \(selector)")
            }
            // Unmount the terminal host: PTYs terminate via onDisappear.
            // The worktree itself stays in the sidebar.
            openWorktreeIds.removeAll { $0 == wt.id }
            if selectedWorktree?.id == wt.id { selectedWorktree = nil }
            return .success(id: request.id)

        case "workspace.create":
            guard let projectSelector = request.params["project"] else {
                return .failure(id: request.id, error: "missing project")
            }
            guard let project = projects.first(where: {
                $0.id.uuidString == projectSelector || $0.name == projectSelector
            }) else {
                return .failure(id: request.id, error: "unknown project \(projectSelector)")
            }
            guard isGitProject(project) else {
                return .failure(id: request.id, error: "project is not a git repository")
            }
            let branch = request.params["branch"] ?? Self.generatedBranchName()
            let before = Set((worktrees[project.id] ?? []).map(\.id))
            await addWorktree(project: project, branch: branch)
            guard let created = (worktrees[project.id] ?? []).first(where: { !before.contains($0.id) }) else {
                return .failure(id: request.id, error: lastError ?? "workspace.create failed")
            }
            return .success(id: request.id, result: [
                "id": created.id.uuidString, "branch": created.branch, "path": created.path,
            ])

        case "notification.create":
            guard let title = request.params["title"], let body = request.params["body"] else {
                return .failure(id: request.id, error: "missing title/body")
            }
            postUserNotification(title: title, subtitle: request.params["subtitle"], body: body)
            return .success(id: request.id)

        case "notification.list":
            let rows = await deliveredNotificationRows()
            return .success(id: request.id, result: ["notifications": ControlRows.encode(rows)])

        case "notification.clear":
            clearDeliveredNotifications()
            return .success(id: request.id)

        case "session.restore":
            let restored = restoreLaunchSnapshot()
            return .success(id: request.id, result: ["restored": String(restored)])

        default:
            return .failure(id: request.id, error: "unknown method \(request.method)")
        }
    }

    func handleBrowserControl(_ request: ControlRequest) async -> ControlResponse {
        guard WorkspaceEngineGate.isEnabled else {
            return .failure(id: request.id, error: "browser requires the universal workspace")
        }
        if let verb = request.method.split(separator: ".").last,
           BrowserUnsupportedVerb(rawValue: String(verb)) != nil {
            return .failure(id: request.id, error: "not_supported")
        }
        guard let adapter = workspaceCoordinator.adapters[.browser] as? BrowserContentAdapter else {
            return .failure(id: request.id, error: "browser adapter unavailable")
        }

        guard let method = BrowserControlMethod(rawValue: request.method) else {
            return .failure(id: request.id, error: "unknown browser method")
        }
        switch method {
        case .open:
            guard let url = request.params["url"], !url.isEmpty else {
                return .failure(id: request.id, error: "missing url")
            }
            guard let worktree = browserWorktree(for: request) else {
                return .failure(id: request.id, error: "no workspace context")
            }
            guard let group = await workspaceCoordinator.ensureGroup(for: worktree) else {
                return .failure(id: request.id, error: "workspace layout unavailable")
            }
            let before = Set(workspaceCoordinator.layouts[worktree.id]?.allTabs.map(\.id) ?? [])
            await workspaceCoordinator.requestNewTab(
                into: group, choice: .newBrowser(url: nil), in: worktree)
            guard let tab = workspaceCoordinator.layouts[worktree.id]?.allTabs.first(
                where: { !before.contains($0.id) }),
                  case .browser(let contentID) = tab.content else {
                return .failure(id: request.id, error: "browser surface was not created")
            }
            _ = workspaceCoordinator.host(for: tab.id)
            do {
                let result = try await adapter.withAgentCommand(contentID: contentID) {
                    let page = try await adapter.open(contentID: contentID, url: url)
                    await workspaceCoordinator.updateBrowserPage(
                        tabID: tab.id, url: page.url.absoluteString,
                        title: page.title, in: worktree)
                    guard let surface = browserSurfaceIdentifier(
                        contentID: contentID, in: worktree.id, format: request.params["id-format"])
                    else {
                        throw BrowserError.invalidArgument(hint: "invalid id-format")
                    }
                    var result = [
                        "surface": surface,
                        "url": page.url.absoluteString,
                        "title": page.title,
                    ]
                    if request.params["id-format"] == "both" {
                        guard let surfaceRef = shortBrowserSurfaceIdentifier(
                            contentID: contentID, in: worktree.id) else {
                            throw BrowserError.surfaceNotFound
                        }
                        result["surfaceRef"] = surfaceRef
                    }
                    return result
                }
                return .success(id: request.id, result: result)
            } catch {
                await workspaceCoordinator.closeTab(tab.id, in: worktree)
                return browserErrorResponse(id: request.id, error: error)
            }

        case .navigate:
            guard let action = request.params["action"] else {
                return .failure(id: request.id, error: "missing action")
            }
            guard let navigation = BrowserCommand.Navigation(rawValue: action) else {
                return .failure(
                    id: request.id,
                    error: "invalid_argument: action must be one of back, forward, reload")
            }
            guard let target = browserTarget(
                selector: request.params["surface"], request: request) else {
                return .failure(id: request.id, error: "surface_not_found")
            }
            _ = workspaceCoordinator.host(for: target.tab.id)
            do {
                let page = try await adapter.withAgentCommand(contentID: target.contentID) {
                    let page = try await adapter.navigate(
                        contentID: target.contentID, action: navigation)
                    await workspaceCoordinator.updateBrowserPage(
                        tabID: target.tab.id, url: page.url.absoluteString,
                        title: page.title, in: target.worktree)
                    return page
                }
                return .success(id: request.id, result: [
                    "url": page.url.absoluteString, "title": page.title,
                ])
            } catch {
                return browserErrorResponse(id: request.id, error: error)
            }

        case .get:
            guard let what = request.params["what"] else {
                return .failure(id: request.id, error: "missing what")
            }
            guard let value = BrowserCommand.Get(rawValue: what) else {
                return .failure(
                    id: request.id,
                    error: "invalid_argument: what must be one of url, text, html")
            }
            guard let target = browserTarget(
                selector: request.params["surface"], request: request) else {
                return .failure(id: request.id, error: "surface_not_found")
            }
            _ = workspaceCoordinator.host(for: target.tab.id)
            do {
                let value = try await adapter.withAgentCommand(contentID: target.contentID) {
                    try await adapter.get(
                        contentID: target.contentID, value: value,
                        selector: request.params["selector"])
                }
                return .success(id: request.id, result: ["value": value])
            } catch {
                return browserErrorResponse(id: request.id, error: error)
            }

        case .screenshot:
            guard let target = browserTarget(
                selector: request.params["surface"], request: request) else {
                return .failure(id: request.id, error: "surface_not_found")
            }
            _ = workspaceCoordinator.host(for: target.tab.id)
            do {
                let path = try await adapter.withAgentCommand(contentID: target.contentID) {
                    try await adapter.screenshot(
                        contentID: target.contentID, path: request.params["path"])
                }
                return .success(id: request.id, result: ["path": path])
            } catch {
                return browserErrorResponse(id: request.id, error: error)
            }

        case .snapshot:
            guard let target = browserTarget(
                selector: request.params["surface"], request: request) else {
                return .failure(id: request.id, error: "surface_not_found")
            }
            _ = workspaceCoordinator.host(for: target.tab.id)
            do {
                let snapshot = try await adapter.withAgentCommand(contentID: target.contentID) {
                    try await adapter.snapshot(contentID: target.contentID)
                }
                return .success(id: request.id, result: snapshotResult(snapshot))
            } catch {
                return browserErrorResponse(id: request.id, error: error)
            }

        case .act:
            guard let target = browserTarget(
                selector: request.params["surface"], request: request) else {
                return .failure(id: request.id, error: "surface_not_found")
            }
            do {
                let action = try browserAct(from: request.params)
                let generation = try browserGeneration(from: request.params, action: action)
                let values = try await adapter.withAgentCommand(contentID: target.contentID) {
                    try await adapter.act(
                        contentID: target.contentID, action: action, generation: generation)
                    var values = ["ok": "true"]
                    if request.params["snapshotAfter"] == "true" {
                        let snapshot = try await adapter.snapshot(contentID: target.contentID)
                        values.merge(snapshotResult(snapshot)) { _, new in new }
                    }
                    return values
                }
                return .success(id: request.id, result: values)
            } catch {
                return browserErrorResponse(id: request.id, error: error)
            }

        case .wait:
            guard let target = browserTarget(
                selector: request.params["surface"], request: request) else {
                return .failure(id: request.id, error: "surface_not_found")
            }
            do {
                guard let timeoutRaw = request.params["timeoutMs"] else {
                    return .failure(id: request.id,
                                    error: "invalid_argument: timeoutMs is required")
                }
                guard let timeoutMs = Int(timeoutRaw) else {
                    return .failure(id: request.id,
                                    error: "invalid_argument: timeoutMs must be an integer")
                }
                guard timeoutMs > 0 else {
                    return .failure(id: request.id,
                                    error: "invalid_argument: timeoutMs must be a positive integer")
                }
                let condition = try browserWaitCondition(from: request.params)
                let elapsed = try await adapter.withAgentCommand(contentID: target.contentID) {
                    try await adapter.wait(
                        contentID: target.contentID, condition: condition, timeoutMs: timeoutMs)
                }
                return .success(id: request.id, result: [
                    "ok": "true", "elapsedMs": String(elapsed),
                ])
            } catch {
                return browserErrorResponse(id: request.id, error: error)
            }

        case .eval:
            guard let script = request.params["script"], !script.isEmpty else {
                return .failure(id: request.id, error: "missing script")
            }
            guard let target = browserTarget(
                selector: request.params["surface"], request: request) else {
                return .failure(id: request.id, error: "surface_not_found")
            }
            do {
                let value = try await adapter.withAgentCommand(contentID: target.contentID) {
                    try await adapter.eval(contentID: target.contentID, script: script)
                }
                return .success(id: request.id, result: ["value": value])
            } catch {
                return browserErrorResponse(id: request.id, error: error)
            }

        case .console:
            guard let target = browserTarget(
                selector: request.params["surface"], request: request) else {
                return .failure(id: request.id, error: "surface_not_found")
            }
            do {
                let since = try request.params["since"].map {
                    guard let value = Double($0) else {
                        throw BrowserError.invalidArgument(hint: "since must be a number")
                    }
                    return value
                }
                let entries = try await adapter.withAgentCommand(contentID: target.contentID) {
                    try await adapter.console(contentID: target.contentID, since: since)
                }
                return .success(id: request.id, result: [
                    "entries": ControlRows.encode(entries.map {
                        ["level": $0.level, "text": $0.text, "at": String($0.at)]
                    })
                ])
            } catch {
                return browserErrorResponse(id: request.id, error: error)
            }

        case .errors:
            return .failure(id: request.id, error: BrowserError.notSupported.errorDescription ?? "not_supported")

        }
    }

    private enum BrowserControlMethod {
        case open, navigate, get, screenshot, snapshot, act, wait, eval, console, errors

        init?(rawValue: String) {
            if rawValue == "browser.open" { self = .open }
            else if rawValue == "browser.navigate" { self = .navigate }
            else if rawValue == "browser.get" { self = .get }
            else if rawValue == "browser.screenshot" { self = .screenshot }
            else if rawValue == "browser.snapshot" { self = .snapshot }
            else if rawValue == "browser.act" { self = .act }
            else if rawValue == "browser.wait" { self = .wait }
            else if rawValue == "browser.eval" { self = .eval }
            else if rawValue == "browser.console" { self = .console }
            else if rawValue == "browser.errors" { self = .errors }
            else if rawValue.hasPrefix("browser.") {
                let verb = String(rawValue.dropFirst("browser.".count))
                if let unsupported = BrowserUnsupportedVerb(rawValue: verb) {
                    _ = unsupported
                    return nil
                }
                return nil
            }
            else { return nil }
        }
    }

    private struct BrowserTarget {
        let worktree: Worktree
        let tab: WorkspaceTab
        let contentID: BrowserContentID
    }

    private func browserWorktree(for request: ControlRequest) -> Worktree? {
        if let selector = request.params["workspace"] {
            return resolveWorktree(selector)
        }
        if let paneID = request.params["pane"].flatMap(UUID.init(uuidString:)),
           let target = universalControlTarget(paneId: paneID) {
            return target.worktree
        }
        return selectedWorktree
    }

    private func browserTarget(selector: String?, request: ControlRequest) -> BrowserTarget? {
        guard let selector else { return nil }
        let worktree = browserWorktree(for: request)
        if let uuid = UUID(uuidString: selector) {
            for candidate in worktrees.values.flatMap({ $0 }) {
                guard let tab = workspaceCoordinator.layouts[candidate.id]?.allTabs.first(
                    where: {
                        guard case .browser(let contentID) = $0.content else { return false }
                        return contentID.rawValue == uuid
                    }),
                    case .browser(let contentID) = tab.content else { continue }
                return BrowserTarget(worktree: candidate, tab: tab, contentID: contentID)
            }
            return nil
        }
        guard selector.hasPrefix("surface:"),
              let number = Int(selector.dropFirst("surface:".count)), number > 0,
              let worktree,
              let tabs = workspaceCoordinator.layouts[worktree.id]?.allTabs.filter({
                  if case .browser = $0.content { return true }
                  return false
              }),
              tabs.indices.contains(number - 1),
              case .browser(let contentID) = tabs[number - 1].content else { return nil }
        return BrowserTarget(worktree: worktree, tab: tabs[number - 1], contentID: contentID)
    }

    private func shortBrowserSurfaceIdentifier(contentID: BrowserContentID, in worktreeID: UUID)
        -> String? {
        let tabs = workspaceCoordinator.layouts[worktreeID]?.allTabs.filter {
            if case .browser = $0.content { return true }
            return false
        } ?? []
        guard let index = tabs.firstIndex(where: {
            if case .browser(let id) = $0.content { return id == contentID }
            return false
        }) else { return nil }
        return "surface:\(index + 1)"
    }

    private func browserSurfaceIdentifier(
        contentID: BrowserContentID, in worktreeID: UUID, format: String?
    ) -> String? {
        if format == "short" {
            return shortBrowserSurfaceIdentifier(contentID: contentID, in: worktreeID)
        }
        if format == nil || format == "uuids" || format == "both" {
            return contentID.rawValue.uuidString
        }
        return nil
    }

    private func browserErrorResponse(id: String, error: Error) -> ControlResponse {
        if let error = error as? BrowserError {
            return .failure(id: id, error: error.errorDescription ?? error.code.rawValue)
        }
        return .failure(id: id, error: String(describing: error))
    }

    private func snapshotResult(_ snapshot: BrowserSnapshot) -> [String: String] {
        let nodes = snapshot.nodes.map { node in
            [
                "ref": node.ref,
                "role": node.role,
                "name": node.name,
                "value": node.value ?? "",
                "box": "{\"x\":\(node.box.x),\"y\":\(node.box.y),\"width\":\(node.box.width),\"height\":\(node.box.height)}",
            ]
        }
        return ["generation": String(snapshot.generation), "nodes": ControlRows.encode(nodes)]
    }

    private func browserAct(from params: [String: String]) throws -> BrowserAct {
        let verb = try requiredParam("verb", in: params)
        let ref = params["ref"]
        let selector = params["selector"]
        switch verb {
        case "click": return .click(ref: ref, selector: selector)
        case "fill": return .fill(ref: ref, selector: selector, value: try requiredParam("value", in: params))
        case "type": return .type(ref: ref, selector: selector, value: try requiredParam("value", in: params))
        case "press": return .press(ref: ref, selector: selector, key: try requiredParam("key", in: params))
        case "scroll":
            let x = try numberParam("deltaX", in: params, default: 0)
            let y = try numberParam("deltaY", in: params, default: 0)
            return .scroll(ref: ref, selector: selector, deltaX: x, deltaY: y)
        default:
            throw BrowserError.invalidArgument(
                hint: "verb must be one of click, fill, type, press, scroll")
        }
    }

    private func browserGeneration(from params: [String: String], action: BrowserAct) throws -> Int? {
        let hasRef: Bool
        switch action {
        case .click(let ref, _), .fill(let ref, _, _), .type(let ref, _, _),
             .press(let ref, _, _), .scroll(let ref, _, _, _): hasRef = ref != nil
        }
        guard hasRef else { return nil }
        guard let raw = params["generation"] else {
            throw BrowserError.invalidArgument(hint: "missing generation")
        }
        guard let generation = Int(raw) else {
            throw BrowserError.invalidArgument(hint: "generation must be an integer")
        }
        return generation
    }

    private func browserWaitCondition(from params: [String: String]) throws -> BrowserWaitCondition {
        let candidates: [(String, (String) -> BrowserWaitCondition)] = [
            ("selector", BrowserWaitCondition.selector),
            ("text", BrowserWaitCondition.text),
            ("urlContains", BrowserWaitCondition.urlContains),
            ("loadState", BrowserWaitCondition.loadState),
            ("function", BrowserWaitCondition.function),
        ]
        let present = candidates.compactMap { key, make in params[key].map(make) }
        guard present.count == 1 else {
            throw BrowserError.invalidArgument(
                hint: "pass exactly one of selector, text, urlContains, loadState, function")
        }
        return present[0]
    }

    private func requiredParam(_ name: String, in params: [String: String]) throws -> String {
        guard let value = params[name], !value.isEmpty else {
            throw BrowserError.invalidArgument(hint: "missing \(name)")
        }
        return value
    }

    private func numberParam(_ name: String, in params: [String: String], default value: Double)
        throws -> Double {
        guard let raw = params[name] else { return value }
        guard let parsed = Double(raw) else {
            throw BrowserError.invalidArgument(hint: "\(name) must be a number")
        }
        return parsed
    }

    /// Caller context: env-provided worktree/pane ids win (a process inside
    /// a Tiller pane identifies itself); otherwise fall back to the UI's
    /// selected worktree and its active pane.
    private func identify(_ request: ControlRequest) -> ControlResponse {
        let worktree = request.params["worktree"]
            .flatMap(UUID.init(uuidString:))
            .flatMap { id in worktrees.values.flatMap { $0 }.first { $0.id == id } }
            ?? selectedWorktree
        guard let worktree else {
            return .failure(id: request.id, error: "no worktree context (not inside a Tiller pane and nothing selected)")
        }
        let paneId = request.params["pane"].flatMap(UUID.init(uuidString:))
            ?? activePaneId(in: worktree)
        var result: [String: String] = [
            "workspaceId": worktree.id.uuidString,
            "branch": worktree.branch,
            "path": worktree.path,
            "project": projects.first { $0.id == worktree.projectId }?.name ?? "",
        ]
        if let paneId { result["surfaceId"] = paneId.uuidString }
        return .success(id: request.id, result: result)
    }

    private func activePaneId(in worktree: Worktree) -> UUID? {
        activeTab(for: worktree.id)?.leafIds.first
    }

    /// "Active pane" of a worktree — first leaf of its active tab, the same
    /// heuristic splitCurrent() already uses.
}
