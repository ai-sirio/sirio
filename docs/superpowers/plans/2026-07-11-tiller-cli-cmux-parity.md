# Tiller CLI (cmux-parity) + Manual Session Restore Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add cmux-style flat CLI commands to `tillerctl` (workspaces, panels, send, notifications, utility) backed by new socket methods in `AppModel`, plus a socket on/off toggle and a manual "restore previous launch" action.

**Architecture:** No new transport — extend the existing newline-delimited JSON unix-socket protocol (`{id, method, params}` → `{id, ok, result, error}`, both maps are `[String: String]`; multi-row results are embedded as a JSON-array *string* value). Pure logic (key encoding, listing builders, env/defaults resolution) lives in `TillerCore`/`TillerControl` with swift-testing tests; `App/AppModel+Control.swift` is thin dispatch glue. New flat kebab CLI subcommands are added beside the existing noun-verb ones; existing `panel`/`notify`/`session-ref`/`worktree` hooks-facing commands stay byte-compatible.

**Tech Stack:** Swift 6, SwiftPM packages, swift-testing (`@Test`/`#expect`), swift-argument-parser (already a dep of tillerctl), GRDB (existing, untouched), UserNotifications.

**Spec:** `docs/superpowers/specs/2026-07-11-tiller-cli-cmux-parity-design.md`

**Approved spec deviations (grounded in existing code):**
1. Env vars: the spec's `TILLER_WORKSPACE_ID`/`TILLER_SURFACE_ID` already exist as **`TILLER_WORKTREE_ID`** (set in `ContentView.swift:171`) and **`TILLER_PANE_ID`** (set in `PtyEnvironment.swift:19`). Reuse them — no new injection.
2. "Active pane" = **first leaf of the active tab of the selected worktree** — the exact heuristic `AppModel.splitCurrent` already uses (`AppModel.swift:562-567`).
3. `new-split` direction mapping: `left`/`right` → `.horizontal`, `up`/`down` → `.vertical`. The split tree has no insert-before position; left==right and up==down for now (`ponytail:` comment at the mapping site).
4. The spec's "handleControl dispatch tests for new methods (App target tests)" cannot exist: the App target has no test bundle (Global Constraints). Everything testable is extracted into `TillerCore`/`TillerControl` (Tasks 1, 3, 4, 5, 9); dispatch glue is covered by the manual smoke checklist in Task 11.

## Global Constraints

- Swift 6 strict concurrency; `AppModel` is `@MainActor @Observable`.
- Tests use swift-testing (`import Testing`, `@Test`, `#expect`) — never XCTest.
- Domain logic goes in packages, not `App/` (CLAUDE.md package boundaries). `App/` has **no test bundle** — anything worth testing must live in `TillerCore` or `TillerControl`.
- New files under `App/` require `xcodegen generate` afterward (project.yml `sources: [App]`).
- Commit messages: Conventional Commits, lower-case imperative subject.
- Verification gate for the whole repo: `Scripts/ci.sh` must print `CI OK`.
- Package-level iteration: `cd Packages/<Package> && swift test`.
- Existing socket methods (`panel.*`, `notify`, `session.ref`, `worktree.set`) and their CLI commands must keep working unchanged — agent hooks depend on them.

---

### Task 1: Socket enable resolution (TillerCore)

**Files:**
- Modify: `Packages/TillerCore/Sources/TillerCore/AppSettings.swift`
- Test: `Packages/TillerCore/Tests/TillerCoreTests/AppSettingsTests.swift`

**Interfaces:**
- Produces: `AppSettings.controlSocketEnabledKey: String` (UserDefaults key `"controlSocket.enabled"`), `AppSettings.controlSocketEnabled(defaultsValue: Bool?, env: [String: String]) -> Bool` — used by Task 2.

- [ ] **Step 1: Write the failing tests**

Append to `Packages/TillerCore/Tests/TillerCoreTests/AppSettingsTests.swift`:

```swift
@Test func controlSocketDefaultsToEnabled() {
    #expect(AppSettings.controlSocketEnabled(defaultsValue: nil, env: [:]) == true)
}

@Test func controlSocketRespectsStoredPreference() {
    #expect(AppSettings.controlSocketEnabled(defaultsValue: false, env: [:]) == false)
    #expect(AppSettings.controlSocketEnabled(defaultsValue: true, env: [:]) == true)
}

@Test func controlSocketEnvOverrideBeatsPreference() {
    #expect(AppSettings.controlSocketEnabled(
        defaultsValue: false, env: ["TILLER_SOCKET_ENABLE": "1"]) == true)
    #expect(AppSettings.controlSocketEnabled(
        defaultsValue: true, env: ["TILLER_SOCKET_ENABLE": "off"]) == false)
}

@Test func controlSocketAcceptsBooleanSpellings() {
    for truthy in ["1", "true", "TRUE", "on", "On"] {
        #expect(AppSettings.controlSocketEnabled(
            defaultsValue: false, env: ["TILLER_SOCKET_ENABLE": truthy]) == true)
    }
    for falsy in ["0", "false", "off", "OFF"] {
        #expect(AppSettings.controlSocketEnabled(
            defaultsValue: true, env: ["TILLER_SOCKET_ENABLE": falsy]) == false)
    }
}

@Test func controlSocketIgnoresGarbageEnvValue() {
    #expect(AppSettings.controlSocketEnabled(
        defaultsValue: false, env: ["TILLER_SOCKET_ENABLE": "maybe"]) == false)
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd Packages/TillerCore && swift test --filter AppSettingsTests`
Expected: FAIL — `controlSocketEnabled` not defined.

- [ ] **Step 3: Implement**

Append inside `public enum AppSettings` in `Packages/TillerCore/Sources/TillerCore/AppSettings.swift`:

```swift
    /// UserDefaults key for the control-socket toggle. Missing value means
    /// enabled (default true) — disabling it also disables agent hooks.
    public static let controlSocketEnabledKey = "controlSocket.enabled"

    /// Resolve whether the control socket should start.
    /// TILLER_SOCKET_ENABLE (1/0, true/false, on/off — case-insensitive)
    /// overrides the stored preference; anything else falls through.
    public static func controlSocketEnabled(defaultsValue: Bool?, env: [String: String]) -> Bool {
        if let raw = env["TILLER_SOCKET_ENABLE"] {
            switch raw.lowercased() {
            case "1", "true", "on": return true
            case "0", "false", "off": return false
            default: break
            }
        }
        return defaultsValue ?? true
    }
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd Packages/TillerCore && swift test --filter AppSettingsTests`
Expected: PASS (all, including pre-existing AppSettings tests).

- [ ] **Step 5: Commit**

```bash
git add Packages/TillerCore
git commit -m "feat: add control-socket enable resolution to AppSettings"
```

---

### Task 2: Wire socket toggle into AppModel + Settings UI

**Files:**
- Modify: `App/AppModel.swift` (bootstrap ~line 166, `startControlServer` ~line 179)
- Modify: `App/GeneralSettingsView.swift`

**Interfaces:**
- Consumes: `AppSettings.controlSocketEnabledKey`, `AppSettings.controlSocketEnabled(defaultsValue:env:)` (Task 1); `ControlServer.stop()` (exists, `ControlServer.swift:100`).
- Produces: `AppModel.setControlSocketEnabled(_ enabled: Bool)` — live start/stop, called from the Settings toggle.

- [ ] **Step 1: Gate server start in bootstrap**

In `App/AppModel.swift`, replace the unconditional `startControlServer()` call inside `bootstrap()` with:

```swift
            if AppSettings.controlSocketEnabled(
                defaultsValue: UserDefaults.standard.object(forKey: AppSettings.controlSocketEnabledKey) as? Bool,
                env: ProcessInfo.processInfo.environment
            ) {
                startControlServer()
            }
```

- [ ] **Step 2: Add live start/stop**

Add below `startControlServer()` in `App/AppModel.swift`:

```swift
    /// Live toggle from Settings. Stopping kills the listener — agent hooks
    /// (Layer A) stop reporting until re-enabled.
    func setControlSocketEnabled(_ enabled: Bool) {
        if enabled {
            startControlServer()
        } else {
            controlServer?.stop()
            controlServer = nil
        }
    }
```

- [ ] **Step 3: Add the Settings toggle**

`GeneralSettingsView` has no `model` today; pass it in. In `App/GeneralSettingsView.swift` add a property and toggle:

```swift
    var model: AppModel   // add near `var updater: UpdaterModel`
    @AppStorage(AppSettings.controlSocketEnabledKey) private var controlSocketEnabled = true
```

Inside the existing `Section("tillerctl")`, first element:

```swift
                Toggle(isOn: $controlSocketEnabled) {
                    Text("Enable control socket")
                    Text("Required by tillerctl and by agent lifecycle hooks — disabling it degrades agent status badges to title/process detection only.")
                }
                .onChange(of: controlSocketEnabled) { _, enabled in
                    model.setControlSocketEnabled(enabled)
                }
```

Update the call site (find with `grep -n "GeneralSettingsView(" App/*.swift`) to pass `model:`.

- [ ] **Step 4: Verify build + behavior**

Run: `Scripts/ci.sh`
Expected: `CI OK`.

Manual smoke (optional but cheap): launch app, toggle off in Settings → `tillerctl ping` (after Task 6) or `nc -U` connect fails; toggle on → connects.

- [ ] **Step 5: Commit**

```bash
git add App/AppModel.swift App/GeneralSettingsView.swift App/ContentView.swift
git commit -m "feat: control-socket on/off toggle with TILLER_SOCKET_ENABLE override"
```

---

### Task 3: Symbolic key encoding (TillerCore)

**Files:**
- Create: `Packages/TillerCore/Sources/TillerCore/TerminalKey.swift`
- Test: `Packages/TillerCore/Tests/TillerCoreTests/TerminalKeyTests.swift`

**Interfaces:**
- Produces: `TerminalKey(rawValue: String)` enum with `.enter/.tab/.escape/.backspace/.delete/.up/.down/.left/.right` and `var bytes: Data` — consumed by `surface.send_key` dispatch (Task 8) and CLI validation (Task 8).

- [ ] **Step 1: Write the failing tests**

Create `Packages/TillerCore/Tests/TillerCoreTests/TerminalKeyTests.swift`:

```swift
import Testing
import Foundation
@testable import TillerCore

@Suite struct TerminalKeyTests {
    @Test func allNamesParse() {
        let names = ["enter", "tab", "escape", "backspace", "delete",
                     "up", "down", "left", "right"]
        for name in names {
            #expect(TerminalKey(rawValue: name) != nil, "\(name) should parse")
        }
        #expect(TerminalKey(rawValue: "banana") == nil)
    }

    @Test func encodesControlBytes() {
        #expect(TerminalKey.enter.bytes == Data([0x0D]))
        #expect(TerminalKey.tab.bytes == Data([0x09]))
        #expect(TerminalKey.escape.bytes == Data([0x1B]))
        #expect(TerminalKey.backspace.bytes == Data([0x7F]))
    }

    @Test func encodesEscapeSequences() {
        #expect(TerminalKey.up.bytes == Data("\u{1B}[A".utf8))
        #expect(TerminalKey.down.bytes == Data("\u{1B}[B".utf8))
        #expect(TerminalKey.right.bytes == Data("\u{1B}[C".utf8))
        #expect(TerminalKey.left.bytes == Data("\u{1B}[D".utf8))
        #expect(TerminalKey.delete.bytes == Data("\u{1B}[3~".utf8))
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd Packages/TillerCore && swift test --filter TerminalKeyTests`
Expected: FAIL — `TerminalKey` not defined.

- [ ] **Step 3: Implement**

Create `Packages/TillerCore/Sources/TillerCore/TerminalKey.swift`:

```swift
import Foundation

/// Symbolic key names accepted by `send-key`, resolved to the byte
/// sequences a PTY expects. Names travel over the socket; bytes are
/// resolved app-side at write time.
public enum TerminalKey: String, CaseIterable, Sendable {
    case enter, tab, escape, backspace, delete, up, down, left, right

    public var bytes: Data {
        switch self {
        case .enter: Data([0x0D])
        case .tab: Data([0x09])
        case .escape: Data([0x1B])
        case .backspace: Data([0x7F])
        case .delete: Data("\u{1B}[3~".utf8)
        case .up: Data("\u{1B}[A".utf8)
        case .down: Data("\u{1B}[B".utf8)
        case .right: Data("\u{1B}[C".utf8)
        case .left: Data("\u{1B}[D".utf8)
        }
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd Packages/TillerCore && swift test --filter TerminalKeyTests`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add Packages/TillerCore
git commit -m "feat: add TerminalKey symbolic key-to-bytes table"
```

---

### Task 4: Listing builders (TillerCore)

Pure functions that turn app state into `[[String: String]]` rows, so the row shape is unit-tested without the App target.

**Files:**
- Create: `Packages/TillerCore/Sources/TillerCore/ControlListing.swift`
- Test: `Packages/TillerCore/Tests/TillerCoreTests/ControlListingTests.swift`

**Interfaces:**
- Consumes: `Project`, `Worktree`, `WorkspaceTab` (existing TillerCore models; `WorkspaceTab.leafIds: [UUID]` exists).
- Produces (consumed by Tasks 7–8 dispatch and Task 5 JSON encoding):
  - `ControlListing.workspaceRows(projects: [Project], worktrees: [UUID: [Worktree]], selectedWorktreeId: UUID?) -> [[String: String]]` — keys `id`, `project`, `branch`, `path`, `selected` ("true"/"false"), ordered by project name then branch.
  - `ControlListing.paneRows(tabs: [WorkspaceTab], activeTabId: UUID?, agentIdForPane: (UUID) -> String?, titleForPane: (UUID) -> String?) -> [[String: String]]` — keys `id`, `tab`, `title`, `agent` (empty string when none), `active` ("true" for every leaf of the active tab).

- [ ] **Step 1: Write the failing tests**

Create `Packages/TillerCore/Tests/TillerCoreTests/ControlListingTests.swift`:

```swift
import Testing
import Foundation
@testable import TillerCore

@Suite struct ControlListingTests {
    private func makeProject(name: String) -> Project {
        Project(id: UUID(), name: name, rootPath: "/tmp/\(name)", createdAt: Date())
    }

    @Test func workspaceRowsFlattenAndMarkSelection() {
        let p = makeProject(name: "tiller")
        let a = Worktree(id: UUID(), projectId: p.id, branch: "main", path: "/tmp/tiller")
        let b = Worktree(id: UUID(), projectId: p.id, branch: "feature", path: "/tmp/tiller-feature")
        let rows = ControlListing.workspaceRows(
            projects: [p], worktrees: [p.id: [a, b]], selectedWorktreeId: b.id
        )
        #expect(rows.count == 2)
        #expect(rows[0]["branch"] == "feature")   // sorted by branch within project
        #expect(rows[0]["selected"] == "true")
        #expect(rows[1]["selected"] == "false")
        #expect(rows[1]["project"] == "tiller")
        #expect(rows[1]["id"] == a.id.uuidString)
        #expect(rows[1]["path"] == "/tmp/tiller")
    }

    @Test func workspaceRowsOrderByProjectName() {
        let p1 = makeProject(name: "zebra")
        let p2 = makeProject(name: "alpha")
        let w1 = Worktree(id: UUID(), projectId: p1.id, branch: "main", path: "/z")
        let w2 = Worktree(id: UUID(), projectId: p2.id, branch: "main", path: "/a")
        let rows = ControlListing.workspaceRows(
            projects: [p1, p2], worktrees: [p1.id: [w1], p2.id: [w2]],
            selectedWorktreeId: nil
        )
        #expect(rows.map { $0["project"]! } == ["alpha", "zebra"])
    }

    @Test func paneRowsListLeavesWithTabTitleAndActiveFlag() {
        let paneA = UUID(); let paneB = UUID(); let paneC = UUID()
        let tab1 = WorkspaceTab(id: UUID(), title: "Shell 1",
                                tree: .split(axis: .horizontal,
                                             first: .leaf(id: paneA),
                                             second: .leaf(id: paneB)))
        let tab2 = WorkspaceTab(id: UUID(), title: "claude", tree: .leaf(id: paneC))
        let rows = ControlListing.paneRows(
            tabs: [tab1, tab2], activeTabId: tab2.id,
            agentIdForPane: { $0 == paneC ? "claude" : nil },
            titleForPane: { $0 == paneA ? "zsh" : nil }
        )
        #expect(rows.count == 3)
        #expect(rows[0]["id"] == paneA.uuidString)
        #expect(rows[0]["tab"] == "Shell 1")
        #expect(rows[0]["title"] == "zsh")
        #expect(rows[0]["agent"] == "")
        #expect(rows[0]["active"] == "false")
        #expect(rows[2]["id"] == paneC.uuidString)
        #expect(rows[2]["agent"] == "claude")
        #expect(rows[2]["active"] == "true")
    }
}
```

Note: if `WorkspaceTab`'s tree initializer differs (check `Packages/TillerCore/Sources/TillerCore/WorkspaceTab.swift` and `SplitTree.swift` for the exact `.split` case signature — e.g. it may carry a `ratio`), adapt the test construction to the real initializer, keeping the assertions unchanged.

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd Packages/TillerCore && swift test --filter ControlListingTests`
Expected: FAIL — `ControlListing` not defined.

- [ ] **Step 3: Implement**

Create `Packages/TillerCore/Sources/TillerCore/ControlListing.swift`:

```swift
import Foundation

/// Pure row builders for the control socket's list responses. Rows are
/// `[String: String]` so they serialize into the protocol's string-map
/// results without a type bridge.
public enum ControlListing {
    public static func workspaceRows(
        projects: [Project],
        worktrees: [UUID: [Worktree]],
        selectedWorktreeId: UUID?
    ) -> [[String: String]] {
        projects
            .sorted { $0.name.localizedCaseInsensitiveCompare($1.name) == .orderedAscending }
            .flatMap { project in
                (worktrees[project.id] ?? [])
                    .sorted { $0.branch.localizedCaseInsensitiveCompare($1.branch) == .orderedAscending }
                    .map { wt in
                        [
                            "id": wt.id.uuidString,
                            "project": project.name,
                            "branch": wt.branch,
                            "path": wt.path,
                            "selected": wt.id == selectedWorktreeId ? "true" : "false",
                        ]
                    }
            }
    }

    public static func paneRows(
        tabs: [WorkspaceTab],
        activeTabId: UUID?,
        agentIdForPane: (UUID) -> String?,
        titleForPane: (UUID) -> String?
    ) -> [[String: String]] {
        tabs.flatMap { tab in
            tab.leafIds.map { paneId in
                [
                    "id": paneId.uuidString,
                    "tab": tab.title,
                    "title": titleForPane(paneId) ?? "",
                    "agent": agentIdForPane(paneId) ?? "",
                    "active": tab.id == activeTabId ? "true" : "false",
                ]
            }
        }
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd Packages/TillerCore && swift test --filter ControlListingTests`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add Packages/TillerCore
git commit -m "feat: add ControlListing row builders for workspace/pane listings"
```

---

### Task 5: Request builders + rows JSON encoding (TillerControl)

**Files:**
- Modify: `Packages/TillerControl/Sources/TillerControl/TillerctlRequestBuilder.swift`
- Create: `Packages/TillerControl/Sources/TillerControl/ControlRows.swift`
- Test: `Packages/TillerControl/Tests/TillerControlTests/TillerctlRequestTests.swift` (append)
- Test: `Packages/TillerControl/Tests/TillerControlTests/ControlRowsTests.swift`

**Interfaces:**
- Produces (consumed by CLI Tasks 6–10 and app dispatch Tasks 6–10):
  - Builders, all returning `ControlRequest` with a fresh UUID id:
    `workspaceList()`, `workspaceCreate(project: String, branch: String?)`, `workspaceSelect(workspace: String)`, `workspaceCurrent()`, `workspaceClose(workspace: String)`, `surfaceList()`, `paneSurfaces()`, `surfaceFocus(surface: String)`, `surfaceSplit(direction: String)`, `surfaceSendText(text: String, surface: String?)`, `surfaceSendKey(key: String, surface: String?)`, `notificationCreate(title: String, subtitle: String?, body: String)`, `notificationList()`, `notificationClear()`, `systemPing()`, `systemCapabilities()`, `systemIdentify(worktree: String?, pane: String?)`, `sessionRestore()`.
  - `ControlRows.encode(_ rows: [[String: String]]) -> String` (sorted-keys JSON array string) and `ControlRows.decode(_ json: String) -> [[String: String]]?`.

- [ ] **Step 1: Write the failing tests**

Append to `TillerctlRequestTests.swift` (inside the existing suite/struct, matching its style):

```swift
@Test func workspaceBuilders() {
    #expect(TillerctlRequestBuilder.workspaceList().method == "workspace.list")
    let create = TillerctlRequestBuilder.workspaceCreate(project: "P1", branch: nil)
    #expect(create.method == "workspace.create")
    #expect(create.params == ["project": "P1"])   // nil branch omitted
    let createBr = TillerctlRequestBuilder.workspaceCreate(project: "P1", branch: "fix")
    #expect(createBr.params == ["project": "P1", "branch": "fix"])
    #expect(TillerctlRequestBuilder.workspaceSelect(workspace: "W")
        .params == ["workspace": "W"])
    #expect(TillerctlRequestBuilder.workspaceCurrent().method == "workspace.current")
    #expect(TillerctlRequestBuilder.workspaceClose(workspace: "W")
        .method == "workspace.close")
}

@Test func surfaceBuilders() {
    #expect(TillerctlRequestBuilder.surfaceList().method == "surface.list")
    #expect(TillerctlRequestBuilder.paneSurfaces().method == "pane.surfaces")
    #expect(TillerctlRequestBuilder.surfaceFocus(surface: "S")
        .params == ["surface": "S"])
    #expect(TillerctlRequestBuilder.surfaceSplit(direction: "right")
        .params == ["direction": "right"])
    let send = TillerctlRequestBuilder.surfaceSendText(text: "ls\n", surface: nil)
    #expect(send.method == "surface.send_text")
    #expect(send.params == ["text": "ls\n"])      // nil surface omitted
    let sendTo = TillerctlRequestBuilder.surfaceSendText(text: "x", surface: "S")
    #expect(sendTo.params == ["text": "x", "surface": "S"])
    let key = TillerctlRequestBuilder.surfaceSendKey(key: "enter", surface: nil)
    #expect(key.method == "surface.send_key")
    #expect(key.params == ["key": "enter"])
}

@Test func notificationAndSystemBuilders() {
    let n = TillerctlRequestBuilder.notificationCreate(
        title: "T", subtitle: nil, body: "B")
    #expect(n.method == "notification.create")
    #expect(n.params == ["title": "T", "body": "B"])   // nil subtitle omitted
    #expect(TillerctlRequestBuilder.notificationList().method == "notification.list")
    #expect(TillerctlRequestBuilder.notificationClear().method == "notification.clear")
    #expect(TillerctlRequestBuilder.systemPing().method == "system.ping")
    #expect(TillerctlRequestBuilder.systemCapabilities().method == "system.capabilities")
    let id = TillerctlRequestBuilder.systemIdentify(worktree: "W", pane: nil)
    #expect(id.method == "system.identify")
    #expect(id.params == ["worktree": "W"])
    #expect(TillerctlRequestBuilder.sessionRestore().method == "session.restore")
}
```

Create `Packages/TillerControl/Tests/TillerControlTests/ControlRowsTests.swift`:

```swift
import Testing
@testable import TillerControl

@Suite struct ControlRowsTests {
    @Test func roundTripsRows() {
        let rows = [["b": "2", "a": "1"], ["a": "3", "b": "4"]]
        let json = ControlRows.encode(rows)
        #expect(ControlRows.decode(json) == rows)
    }

    @Test func encodesDeterministically() {
        // sorted keys → stable output for tests and diffing
        #expect(ControlRows.encode([["b": "2", "a": "1"]]) == #"[{"a":"1","b":"2"}]"#)
    }

    @Test func emptyAndGarbage() {
        #expect(ControlRows.encode([]) == "[]")
        #expect(ControlRows.decode("not json") == nil)
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd Packages/TillerControl && swift test`
Expected: FAIL — new builders and `ControlRows` not defined.

- [ ] **Step 3: Implement**

Create `Packages/TillerControl/Sources/TillerControl/ControlRows.swift`:

```swift
import Foundation

/// The control protocol's result payload is a flat `[String: String]`.
/// List responses embed their rows as one JSON-array string value under a
/// single key — this is the shared encoder/decoder for that convention.
public enum ControlRows {
    public static func encode(_ rows: [[String: String]]) -> String {
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.sortedKeys]
        guard let data = try? encoder.encode(rows) else { return "[]" }
        return String(decoding: data, as: UTF8.self)
    }

    public static func decode(_ json: String) -> [[String: String]]? {
        try? JSONDecoder().decode([[String: String]].self, from: Data(json.utf8))
    }
}
```

Append to `TillerctlRequestBuilder.swift` (inside the enum):

```swift
    // MARK: - cmux-parity methods

    private static func request(_ method: String, _ params: [String: String?] = [:]) -> ControlRequest {
        ControlRequest(id: UUID().uuidString, method: method,
                       params: params.compactMapValues { $0 })
    }

    public static func workspaceList() -> ControlRequest { request("workspace.list") }
    public static func workspaceCreate(project: String, branch: String?) -> ControlRequest {
        request("workspace.create", ["project": project, "branch": branch])
    }
    public static func workspaceSelect(workspace: String) -> ControlRequest {
        request("workspace.select", ["workspace": workspace])
    }
    public static func workspaceCurrent() -> ControlRequest { request("workspace.current") }
    public static func workspaceClose(workspace: String) -> ControlRequest {
        request("workspace.close", ["workspace": workspace])
    }

    public static func surfaceList() -> ControlRequest { request("surface.list") }
    public static func paneSurfaces() -> ControlRequest { request("pane.surfaces") }
    public static func surfaceFocus(surface: String) -> ControlRequest {
        request("surface.focus", ["surface": surface])
    }
    public static func surfaceSplit(direction: String) -> ControlRequest {
        request("surface.split", ["direction": direction])
    }
    public static func surfaceSendText(text: String, surface: String?) -> ControlRequest {
        request("surface.send_text", ["text": text, "surface": surface])
    }
    public static func surfaceSendKey(key: String, surface: String?) -> ControlRequest {
        request("surface.send_key", ["key": key, "surface": surface])
    }

    public static func notificationCreate(title: String, subtitle: String?, body: String) -> ControlRequest {
        request("notification.create", ["title": title, "subtitle": subtitle, "body": body])
    }
    public static func notificationList() -> ControlRequest { request("notification.list") }
    public static func notificationClear() -> ControlRequest { request("notification.clear") }

    public static func systemPing() -> ControlRequest { request("system.ping") }
    public static func systemCapabilities() -> ControlRequest { request("system.capabilities") }
    public static func systemIdentify(worktree: String?, pane: String?) -> ControlRequest {
        request("system.identify", ["worktree": worktree, "pane": pane])
    }
    public static func sessionRestore() -> ControlRequest { request("session.restore") }
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd Packages/TillerControl && swift test`
Expected: PASS (including all pre-existing TillerControl tests).

- [ ] **Step 5: Commit**

```bash
git add Packages/TillerControl
git commit -m "feat: add cmux-parity request builders and ControlRows encoding"
```

---

### Task 6: Dispatch scaffold + system group (ping, capabilities, identify) end-to-end

**Files:**
- Create: `App/AppModel+Control.swift`
- Modify: `App/AppModel.swift` (`handleControl` default case, line ~290)
- Create: `Packages/TillerControl/Sources/tillerctl/CmuxCommands.swift`
- Modify: `Packages/TillerControl/Sources/tillerctl/Tillerctl.swift` (register subcommands)
- Modify: `project.yml` — none needed, but run `xcodegen generate` (new App file)

**Interfaces:**
- Consumes: builders + `ControlRows` (Task 5).
- Produces:
  - `AppModel.handleCmuxControl(_ request: ControlRequest) async -> ControlResponse` — the dispatch home for every new method; later tasks add cases to its switch.
  - `AppModel.cmuxMethods: [String]` — static list backing `system.capabilities`; later tasks append their methods.
  - CLI plumbing consumed by later tasks: `struct JSONFlag: ParsableArguments` (`--json`), `func roundTripOrDie(_ request: ControlRequest, socket: String) throws -> ControlResponse`, `func printRows(_ response: ControlResponse, key: String, columns: [String], asJSON: Bool)`.

- [ ] **Step 1: Forward unknown methods from the existing switch**

In `App/AppModel.swift`, `handleControl`, replace:

```swift
        default:
            return .failure(id: request.id, error: "unknown method \(request.method)")
```

with:

```swift
        default:
            return await handleCmuxControl(request)
```

- [ ] **Step 2: Create the dispatch extension with the system group**

Create `App/AppModel+Control.swift`:

```swift
import Foundation
import AppKit
import TillerCore
import TillerControl

/// cmux-parity control methods. The legacy methods (panel.*, notify,
/// session.ref, worktree.set) stay in AppModel.handleControl; everything
/// added for CLI parity dispatches here.
extension AppModel {
    /// Methods answered by handleCmuxControl — the system.capabilities
    /// payload. Legacy methods are listed too: capabilities describes the
    /// whole socket, not just this file.
    static let cmuxMethods: [String] = [
        "panel.create", "panel.write", "panel.read", "panel.wait",
        "notify", "session.ref", "worktree.set",
        "system.ping", "system.capabilities", "system.identify",
    ]

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

        default:
            return .failure(id: request.id, error: "unknown method \(request.method)")
        }
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

    /// "Active pane" of a worktree — first leaf of its active tab, the same
    /// heuristic splitCurrent() already uses.
    func activePaneId(in worktree: Worktree) -> UUID? {
        activeTab(for: worktree.id)?.leafIds.first
    }
}
```

- [ ] **Step 3: Regenerate the Xcode project**

Run: `xcodegen generate`
Expected: exits 0, `Tiller.xcodeproj` regenerated (new App file included).

- [ ] **Step 4: Create the CLI plumbing + system commands**

Create `Packages/TillerControl/Sources/tillerctl/CmuxCommands.swift`:

```swift
import ArgumentParser
import Foundation
import TillerControl

// MARK: - Shared CLI plumbing for the flat cmux-parity commands

struct JSONFlag: ParsableArguments {
    @Flag(name: .customLong("json"), help: "Output raw JSON.") var json = false
}

func roundTripOrDie(_ request: ControlRequest, socket: String) throws -> ControlResponse {
    let response: ControlResponse
    do {
        response = try ControlClient.roundTrip(socketPath: socket, request: request)
    } catch {
        // Connection-level failure (socket file missing / connection refused):
        // the spec's canonical message for a disabled socket or dead app.
        throw ValidationError(
            "Tiller control socket is disabled or Tiller is not running (\(error))")
    }
    guard response.ok else {
        throw ValidationError(response.error ?? "\(request.method) failed")
    }
    return response
}

/// Print a rows result: `--json` prints the embedded JSON array verbatim;
/// otherwise one line per row, tab-separated in `columns` order.
func printRows(_ response: ControlResponse, key: String, columns: [String], asJSON: Bool) {
    let raw = response.result?[key] ?? "[]"
    if asJSON { print(raw); return }
    for row in ControlRows.decode(raw) ?? [] {
        print(columns.map { row[$0] ?? "" }.joined(separator: "\t"))
    }
}

/// Print a single-object result: `--json` re-encodes result as JSON.
func printResult(_ response: ControlResponse, columns: [String], asJSON: Bool) {
    let result = response.result ?? [:]
    if asJSON {
        print(ControlRows.encode([result]))
        return
    }
    print(columns.compactMap { result[$0] }.joined(separator: "\t"))
}

// MARK: - Utility commands

struct Ping: ParsableCommand {
    static let configuration = CommandConfiguration(
        commandName: "ping", abstract: "Check that Tiller is running and responding.")
    @OptionGroup var socketOptions: SocketOptions
    func run() throws {
        _ = try roundTripOrDie(TillerctlRequestBuilder.systemPing(),
                               socket: socketOptions.socket)
        print("pong")
    }
}

struct Capabilities: ParsableCommand {
    static let configuration = CommandConfiguration(
        commandName: "capabilities", abstract: "List available socket methods.")
    @OptionGroup var socketOptions: SocketOptions
    @OptionGroup var jsonFlag: JSONFlag
    func run() throws {
        let response = try roundTripOrDie(TillerctlRequestBuilder.systemCapabilities(),
                                          socket: socketOptions.socket)
        printRows(response, key: "methods", columns: ["method"], asJSON: jsonFlag.json)
    }
}

struct Identify: ParsableCommand {
    static let configuration = CommandConfiguration(
        commandName: "identify", abstract: "Show the current workspace/surface context.")
    @OptionGroup var socketOptions: SocketOptions
    @OptionGroup var jsonFlag: JSONFlag
    func run() throws {
        let env = ProcessInfo.processInfo.environment
        let response = try roundTripOrDie(
            TillerctlRequestBuilder.systemIdentify(
                worktree: env["TILLER_WORKTREE_ID"], pane: env["TILLER_PANE_ID"]),
            socket: socketOptions.socket)
        printResult(response,
                    columns: ["project", "branch", "path", "workspaceId", "surfaceId"],
                    asJSON: jsonFlag.json)
    }
}
```

In `Tillerctl.swift`, extend the subcommand list:

```swift
        subcommands: [
            Panel.self, Notify.self, SessionRef.self, Worktree.self,
            // cmux-parity flat commands
            Ping.self, Capabilities.self, Identify.self,
        ]
```

- [ ] **Step 5: Build and verify**

Run: `cd Packages/TillerControl && swift build && swift test`
Expected: builds, all tests PASS.

Run: `Scripts/ci.sh`
Expected: `CI OK`.

Manual e2e (requires running Tiller.app): `swift run tillerctl ping` → `pong`; `swift run tillerctl capabilities` → method list; `swift run tillerctl identify` → context line (or clear error when nothing selected).

- [ ] **Step 6: Commit**

```bash
git add App/AppModel.swift App/AppModel+Control.swift Packages/TillerControl
git commit -m "feat: control dispatch scaffold with ping/capabilities/identify"
```

(`Tiller.xcodeproj` is gitignored — nothing to stage after `xcodegen generate`.)

---

### Task 7: Workspace command group

**Files:**
- Modify: `App/AppModel+Control.swift`
- Modify: `Packages/TillerControl/Sources/tillerctl/CmuxCommands.swift`
- Modify: `Packages/TillerControl/Sources/tillerctl/Tillerctl.swift`

**Interfaces:**
- Consumes: `ControlListing.workspaceRows` (Task 4), builders (Task 5), existing `AppModel.addWorktree(project:branch:)`, `store.worktree(byPath:)` pattern from `worktree.set` (`AppModel.swift:274-279`), `resyncSelection`.
- Produces: socket methods `workspace.list`, `workspace.create`, `workspace.select`, `workspace.current`, `workspace.close`; helper `AppModel.resolveWorktree(_ selector: String) async -> Worktree?` reused by Task 8.

- [ ] **Step 1: Add worktree resolution + workspace cases**

In `App/AppModel+Control.swift` add to the extension:

```swift
    /// Resolve a worktree from a UUID string or absolute path — same dual
    /// selector the legacy worktree.set method accepts.
    func resolveWorktree(_ selector: String) -> Worktree? {
        if let uuid = UUID(uuidString: selector) {
            return worktrees.values.flatMap { $0 }.first { $0.id == uuid }
        }
        return worktrees.values.flatMap { $0 }.first { $0.path == selector }
    }
```

Add cases to the `switch` in `handleCmuxControl` (before `default`):

```swift
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
            let branch = request.params["branch"] ?? Self.generatedBranchName()
            let before = Set((worktrees[project.id] ?? []).map(\.id))
            await addWorktree(project: project, branch: branch)
            guard let created = (worktrees[project.id] ?? []).first(where: { !before.contains($0.id) }) else {
                return .failure(id: request.id, error: lastError ?? "workspace.create failed")
            }
            return .success(id: request.id, result: [
                "id": created.id.uuidString, "branch": created.branch, "path": created.path,
            ])
```

And the branch-name helper in the same extension:

```swift
    /// Default branch name for workspace.create when --branch is omitted.
    static func generatedBranchName(now: Date = Date()) -> String {
        let fmt = DateFormatter()
        fmt.dateFormat = "yyyyMMdd-HHmmss"
        fmt.locale = Locale(identifier: "en_US_POSIX")
        return "wt-\(fmt.string(from: now))"
    }
```

Append the five method names to `cmuxMethods`:

```swift
        "workspace.list", "workspace.create", "workspace.select",
        "workspace.current", "workspace.close",
```

- [ ] **Step 2: Add the CLI commands**

Append to `CmuxCommands.swift`:

```swift
// MARK: - Workspace commands

struct ListWorkspaces: ParsableCommand {
    static let configuration = CommandConfiguration(
        commandName: "list-workspaces", abstract: "List all worktrees.")
    @OptionGroup var socketOptions: SocketOptions
    @OptionGroup var jsonFlag: JSONFlag
    func run() throws {
        let response = try roundTripOrDie(TillerctlRequestBuilder.workspaceList(),
                                          socket: socketOptions.socket)
        printRows(response, key: "workspaces",
                  columns: ["id", "project", "branch", "path", "selected"],
                  asJSON: jsonFlag.json)
    }
}

struct NewWorkspace: ParsableCommand {
    static let configuration = CommandConfiguration(
        commandName: "new-workspace", abstract: "Create a worktree in a project.")
    @OptionGroup var socketOptions: SocketOptions
    @Option(help: "Project UUID or name.") var project: String
    @Option(help: "Branch name (default: generated).") var branch: String?
    func run() throws {
        let response = try roundTripOrDie(
            TillerctlRequestBuilder.workspaceCreate(project: project, branch: branch),
            socket: socketOptions.socket)
        print(response.result?["id"] ?? "")
    }
}

struct SelectWorkspace: ParsableCommand {
    static let configuration = CommandConfiguration(
        commandName: "select-workspace", abstract: "Select a worktree in the sidebar.")
    @OptionGroup var socketOptions: SocketOptions
    @Option(help: "Worktree UUID or absolute path.") var workspace: String
    func run() throws {
        _ = try roundTripOrDie(
            TillerctlRequestBuilder.workspaceSelect(workspace: workspace),
            socket: socketOptions.socket)
    }
}

struct CurrentWorkspace: ParsableCommand {
    static let configuration = CommandConfiguration(
        commandName: "current-workspace", abstract: "Show the selected worktree.")
    @OptionGroup var socketOptions: SocketOptions
    @OptionGroup var jsonFlag: JSONFlag
    func run() throws {
        let response = try roundTripOrDie(TillerctlRequestBuilder.workspaceCurrent(),
                                          socket: socketOptions.socket)
        printResult(response, columns: ["project", "branch", "path", "id"],
                    asJSON: jsonFlag.json)
    }
}

struct CloseWorkspace: ParsableCommand {
    static let configuration = CommandConfiguration(
        commandName: "close-workspace",
        abstract: "Unmount a worktree's terminals (worktree stays in sidebar).")
    @OptionGroup var socketOptions: SocketOptions
    @Option(help: "Worktree UUID or absolute path.") var workspace: String
    func run() throws {
        _ = try roundTripOrDie(
            TillerctlRequestBuilder.workspaceClose(workspace: workspace),
            socket: socketOptions.socket)
    }
}
```

Register in `Tillerctl.swift` subcommands (append to the cmux-parity group):

```swift
            ListWorkspaces.self, NewWorkspace.self, SelectWorkspace.self,
            CurrentWorkspace.self, CloseWorkspace.self,
```

- [ ] **Step 3: Verify**

Run: `cd Packages/TillerControl && swift build && swift test`
Expected: builds, tests PASS.
Run: `Scripts/ci.sh`
Expected: `CI OK`.

Manual e2e with the app running: `swift run tillerctl list-workspaces` shows the sidebar's worktrees; `select-workspace --workspace <path>` switches the sidebar; `current-workspace` reflects it; `close-workspace` unmounts (terminal disappears); `new-workspace --project <name> --branch test-cli` creates and lists a new worktree.

- [ ] **Step 4: Commit**

```bash
git add App/AppModel+Control.swift Packages/TillerControl
git commit -m "feat: workspace command group (list/new/select/current/close)"
```

---

### Task 8: Surface command group (list, focus, split, send, send-key)

**Files:**
- Modify: `App/AppModel+Control.swift`
- Modify: `Packages/TillerControl/Sources/tillerctl/CmuxCommands.swift`
- Modify: `Packages/TillerControl/Sources/tillerctl/Tillerctl.swift`

**Interfaces:**
- Consumes: `ControlListing.paneRows` (Task 4), `TerminalKey` (Task 3), builders (Task 5), `AppModel.activePaneId(in:)` (Task 6), existing `AppModel.split(paneId:axis:)`, `tabContaining(paneId:)`, `activateTab(_:in:)`, `agentActivity.paneAgents[paneId]` / `agentActivity.agentId(paneId:)`, `paneTitles[paneId]`, `PaneRegistry.shared.write(paneId:data:)`.
- Produces: socket methods `surface.list`, `pane.surfaces`, `surface.focus`, `surface.split`, `surface.send_text`, `surface.send_key`; helper `AppModel.resolveTargetPane(_ explicit: String?) -> UUID?`.

- [ ] **Step 1: Add pane resolution + surface cases**

In `App/AppModel+Control.swift`:

```swift
    /// Target pane for send/send-key: explicit surface param wins (the CLI
    /// already substituted TILLER_PANE_ID when run inside a pane), else the
    /// active pane of the selected worktree.
    func resolveTargetPane(_ explicit: String?) -> UUID? {
        if let explicit {
            return UUID(uuidString: explicit)
        }
        guard let worktree = selectedWorktree else { return nil }
        return activePaneId(in: worktree)
    }
```

Cases in `handleCmuxControl` (before `default`):

```swift
        case "surface.list", "pane.surfaces":
            guard let worktree = selectedWorktree else {
                return .failure(id: request.id, error: "no workspace selected")
            }
            let allTabs = tabs[worktree.id] ?? []
            let scope = request.method == "pane.surfaces"
                ? allTabs.filter { $0.id == activeTabId[worktree.id] }
                : allTabs
            let rows = ControlListing.paneRows(
                tabs: scope, activeTabId: activeTabId[worktree.id],
                agentIdForPane: { self.agentActivity.paneAgents[$0] },
                titleForPane: { self.paneTitles[$0] }
            )
            return .success(id: request.id, result: ["surfaces": ControlRows.encode(rows)])

        case "surface.focus":
            guard let paneId = request.params["surface"].flatMap(UUID.init(uuidString:)),
                  let tuple = tabContaining(paneId: paneId) else {
                return .failure(id: request.id, error: "unknown surface")
            }
            selectedWorktree = tuple.worktree
            activateTab(tuple.tab.id, in: tuple.worktree.id)
            NSApp.activate(ignoringOtherApps: true)
            NSApp.windows.first?.makeKeyAndOrderFront(nil)
            return .success(id: request.id)

        case "surface.split":
            let direction = request.params["direction"] ?? ""
            // ponytail: the split tree has no insert-before slot, so
            // left==right and up==down; revisit if position ever matters.
            let axis: SplitAxis
            switch direction {
            case "left", "right": axis = .horizontal
            case "up", "down": axis = .vertical
            default:
                return .failure(id: request.id, error: "invalid direction \(direction) (left|right|up|down)")
            }
            guard let worktree = selectedWorktree,
                  let target = activePaneId(in: worktree) else {
                return .failure(id: request.id, error: "no active pane to split")
            }
            split(paneId: target, axis: axis)
            return .success(id: request.id)

        case "surface.send_text":
            guard let text = request.params["text"] else {
                return .failure(id: request.id, error: "missing text")
            }
            guard let paneId = resolveTargetPane(request.params["surface"]) else {
                return .failure(id: request.id, error: "no target surface")
            }
            let wrote = await PaneRegistry.shared.write(paneId: paneId, data: Data(text.utf8))
            return wrote ? .success(id: request.id)
                         : .failure(id: request.id, error: "unknown surface")

        case "surface.send_key":
            guard let key = request.params["key"].flatMap(TerminalKey.init(rawValue:)) else {
                let names = TerminalKey.allCases.map(\.rawValue).joined(separator: "|")
                return .failure(id: request.id, error: "invalid key (\(names))")
            }
            guard let paneId = resolveTargetPane(request.params["surface"]) else {
                return .failure(id: request.id, error: "no target surface")
            }
            let wrote = await PaneRegistry.shared.write(paneId: paneId, data: key.bytes)
            return wrote ? .success(id: request.id)
                         : .failure(id: request.id, error: "unknown surface")
```

Append to `cmuxMethods`:

```swift
        "surface.list", "pane.surfaces", "surface.focus", "surface.split",
        "surface.send_text", "surface.send_key",
```

Note: `agentActivity.paneAgents` — verify the property name with `grep -n "paneAgents\|func agentId" Packages/TillerCore/Sources/TillerCore/AgentActivityModel.swift`; `AppModel.swift:1021` uses `agentActivity.paneAgents[paneId]`, so that spelling is correct.

- [ ] **Step 2: Add the CLI commands**

Append to `CmuxCommands.swift`:

```swift
// MARK: - Surface commands

/// Explicit --surface wins; otherwise TILLER_PANE_ID (set inside every
/// Tiller pane) is forwarded so a pane targets itself; otherwise the app
/// resolves its active pane.
func defaultSurface(_ explicit: String?) -> String? {
    explicit ?? ProcessInfo.processInfo.environment["TILLER_PANE_ID"]
}

struct NewSplit: ParsableCommand {
    static let configuration = CommandConfiguration(
        commandName: "new-split", abstract: "Split the active pane.")
    @OptionGroup var socketOptions: SocketOptions
    @Argument(help: "left | right | up | down") var direction: String
    func run() throws {
        _ = try roundTripOrDie(
            TillerctlRequestBuilder.surfaceSplit(direction: direction),
            socket: socketOptions.socket)
    }
}

struct ListPanels: ParsableCommand {
    static let configuration = CommandConfiguration(
        commandName: "list-panels", abstract: "List panes of the current worktree.")
    @OptionGroup var socketOptions: SocketOptions
    @OptionGroup var jsonFlag: JSONFlag
    func run() throws {
        let response = try roundTripOrDie(TillerctlRequestBuilder.surfaceList(),
                                          socket: socketOptions.socket)
        printRows(response, key: "surfaces",
                  columns: ["id", "tab", "title", "agent", "active"],
                  asJSON: jsonFlag.json)
    }
}

struct ListPaneSurfaces: ParsableCommand {
    static let configuration = CommandConfiguration(
        commandName: "list-pane-surfaces", abstract: "List panes of the active tab.")
    @OptionGroup var socketOptions: SocketOptions
    @OptionGroup var jsonFlag: JSONFlag
    func run() throws {
        let response = try roundTripOrDie(TillerctlRequestBuilder.paneSurfaces(),
                                          socket: socketOptions.socket)
        printRows(response, key: "surfaces",
                  columns: ["id", "tab", "title", "agent", "active"],
                  asJSON: jsonFlag.json)
    }
}

struct FocusPanel: ParsableCommand {
    static let configuration = CommandConfiguration(
        commandName: "focus-panel", abstract: "Focus a pane (switches worktree/tab).")
    @OptionGroup var socketOptions: SocketOptions
    @Option(help: "Pane UUID.") var panel: String
    func run() throws {
        _ = try roundTripOrDie(
            TillerctlRequestBuilder.surfaceFocus(surface: panel),
            socket: socketOptions.socket)
    }
}

struct Send: ParsableCommand {
    static let configuration = CommandConfiguration(
        commandName: "send", abstract: "Send text to a pane (default: active pane).")
    @OptionGroup var socketOptions: SocketOptions
    @Option(help: "Target pane UUID (default: this pane, else active pane).")
    var surface: String?
    @Argument(help: "Text to send.") var text: String
    func run() throws {
        _ = try roundTripOrDie(
            TillerctlRequestBuilder.surfaceSendText(text: text,
                                                    surface: defaultSurface(surface)),
            socket: socketOptions.socket)
    }
}

struct SendKey: ParsableCommand {
    static let configuration = CommandConfiguration(
        commandName: "send-key", abstract: "Send a key press to a pane.")
    @OptionGroup var socketOptions: SocketOptions
    @Option(help: "Target pane UUID (default: this pane, else active pane).")
    var surface: String?
    @Argument(help: "enter | tab | escape | backspace | delete | up | down | left | right")
    var key: String
    func run() throws {
        _ = try roundTripOrDie(
            TillerctlRequestBuilder.surfaceSendKey(key: key,
                                                   surface: defaultSurface(surface)),
            socket: socketOptions.socket)
    }
}
```

Register in `Tillerctl.swift`:

```swift
            NewSplit.self, ListPanels.self, ListPaneSurfaces.self, FocusPanel.self,
            Send.self, SendKey.self,
```

- [ ] **Step 3: Verify**

Run: `cd Packages/TillerControl && swift build && swift test` → PASS.
Run: `Scripts/ci.sh` → `CI OK`.

Manual e2e with the app running and a worktree selected:
`swift run tillerctl list-panels` lists panes; `new-split right` splits; `send 'echo hi'` + `send-key enter` executes in the active pane; `focus-panel --panel <id>` brings the window forward on that pane's tab.

- [ ] **Step 4: Commit**

```bash
git add App/AppModel+Control.swift Packages/TillerControl
git commit -m "feat: surface command group (split/list/focus/send/send-key)"
```

---

### Task 9: Notification command group + notify dispatch

**Files:**
- Modify: `App/AgentNotifier.swift`
- Modify: `App/AppModel+Control.swift`
- Create: `Packages/TillerControl/Sources/TillerControl/NotifyMode.swift`
- Modify: `Packages/TillerControl/Sources/tillerctl/Tillerctl.swift` (existing `Notify` struct)
- Modify: `Packages/TillerControl/Sources/tillerctl/CmuxCommands.swift`
- Test: `Packages/TillerControl/Tests/TillerControlTests/NotifyModeTests.swift` (flag-dispatch matrix; the `UNUserNotificationCenter` side is system I/O and stays untested)

**Interfaces:**
- Consumes: `AgentNotifier` (AppModel holds it as `private let notifier` — change to expose the three new methods via AppModel wrappers, keeping `notifier` private).
- Produces:
  - `AgentNotifier.postUser(title: String, subtitle: String?, body: String)`
  - `AgentNotifier.deliveredNotifications() async -> [[String: String]]` (keys `title`, `subtitle`, `body`, `date` ISO8601)
  - `AgentNotifier.clearDelivered()`
  - `NotifyMode.resolve(session:status:title:body:) throws -> NotifyMode` + `NotifyModeError.usageMessage` (pure flag dispatch, lives in the TillerControl library so it's testable).
  - Socket methods `notification.create`, `notification.list`, `notification.clear`.
  - CLI: `notify --title/--subtitle/--body` mode on the existing `Notify` command; `list-notifications`, `clear-notifications`.

- [ ] **Step 1: Extend AgentNotifier**

Append to the class in `App/AgentNotifier.swift`:

```swift
    /// User-visible notification requested over the control socket
    /// (tillerctl notify --title …). Unlike agent-status notifications,
    /// each gets a unique identifier — they don't replace each other.
    func postUser(title: String, subtitle: String?, body: String) {
        let center = UNUserNotificationCenter.current()
        center.getNotificationSettings { settings in
            guard settings.authorizationStatus == .authorized ||
                  settings.authorizationStatus == .provisional else { return }
            let content = UNMutableNotificationContent()
            content.title = title
            if let subtitle { content.subtitle = subtitle }
            content.body = body
            content.sound = .default
            let request = UNNotificationRequest(
                identifier: "tiller.user.\(UUID().uuidString)",
                content: content, trigger: nil
            )
            center.add(request) { _ in }
        }
    }

    /// Delivered Tiller notifications still in Notification Center.
    func deliveredNotifications() async -> [[String: String]] {
        let delivered = await UNUserNotificationCenter.current().deliveredNotifications()
        let iso = ISO8601DateFormatter()
        return delivered.map { n in
            [
                "title": n.request.content.title,
                "subtitle": n.request.content.subtitle,
                "body": n.request.content.body,
                "date": iso.string(from: n.date),
            ]
        }
    }

    func clearDelivered() {
        UNUserNotificationCenter.current().removeAllDeliveredNotifications()
    }
```

- [ ] **Step 2: Add socket cases**

`notifier` is `private let` in AppModel; the extension in `AppModel+Control.swift` is a different file, so add thin wrappers in `App/AppModel.swift` next to `notifyTransition` (which already touches `notifier`):

```swift
    // Control-socket bridges to the private notifier.
    func postUserNotification(title: String, subtitle: String?, body: String) {
        Task { await notifier.ensureAuthorization() }
        notifier.postUser(title: title, subtitle: subtitle, body: body)
    }
    func deliveredNotificationRows() async -> [[String: String]] {
        await notifier.deliveredNotifications()
    }
    func clearDeliveredNotifications() { notifier.clearDelivered() }
```

Cases in `handleCmuxControl`:

```swift
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
```

Append to `cmuxMethods`: `"notification.create", "notification.list", "notification.clear",`

- [ ] **Step 3: Write the failing NotifyMode tests**

Create `Packages/TillerControl/Tests/TillerControlTests/NotifyModeTests.swift`:

```swift
import Testing
@testable import TillerControl

@Suite struct NotifyModeTests {
    @Test func agentStatusMode() throws {
        #expect(try NotifyMode.resolve(
            session: "P", status: "running", title: nil, body: nil) == .agentStatus)
    }

    @Test func userNotificationMode() throws {
        #expect(try NotifyMode.resolve(
            session: nil, status: nil, title: "T", body: "B") == .userNotification)
    }

    @Test func bothModesIsAmbiguous() {
        #expect(throws: NotifyModeError.ambiguousMode) {
            try NotifyMode.resolve(session: "P", status: "s", title: "T", body: "B")
        }
    }

    @Test func neitherModeIsError() {
        #expect(throws: NotifyModeError.missingMode) {
            try NotifyMode.resolve(session: nil, status: nil, title: nil, body: nil)
        }
    }

    @Test func sessionWithoutStatusIsError() {
        #expect(throws: NotifyModeError.missingStatus) {
            try NotifyMode.resolve(session: "P", status: nil, title: nil, body: nil)
        }
    }

    @Test func titleWithoutBodyIsError() {
        #expect(throws: NotifyModeError.missingBody) {
            try NotifyMode.resolve(session: nil, status: nil, title: "T", body: nil)
        }
    }
}
```

- [ ] **Step 4: Run tests to verify they fail**

Run: `cd Packages/TillerControl && swift test --filter NotifyModeTests`
Expected: FAIL — `NotifyMode` not defined.

- [ ] **Step 5: Implement NotifyMode**

Create `Packages/TillerControl/Sources/TillerControl/NotifyMode.swift`:

```swift
/// Flag dispatch for the dual-mode `notify` CLI command: agent-status
/// (--session/--status, used by agent hooks) vs user notification
/// (--title/--body, cmux parity). Pure so the usage-error matrix is
/// testable without ArgumentParser.
public enum NotifyMode: Equatable, Sendable {
    case agentStatus
    case userNotification

    public static func resolve(
        session: String?, status: String?, title: String?, body: String?
    ) throws -> NotifyMode {
        switch (session != nil, title != nil) {
        case (true, true): throw NotifyModeError.ambiguousMode
        case (false, false): throw NotifyModeError.missingMode
        case (true, false):
            guard status != nil else { throw NotifyModeError.missingStatus }
            return .agentStatus
        case (false, true):
            guard body != nil else { throw NotifyModeError.missingBody }
            return .userNotification
        }
    }
}

public enum NotifyModeError: Error, Equatable, Sendable {
    case ambiguousMode, missingMode, missingStatus, missingBody

    public var usageMessage: String {
        switch self {
        case .ambiguousMode, .missingMode:
            "use either --session/--status (agent status) or --title/--body (user notification)"
        case .missingStatus: "--status is required with --session"
        case .missingBody: "--body is required with --title"
        }
    }
}
```

- [ ] **Step 6: Run tests to verify they pass**

Run: `cd Packages/TillerControl && swift test --filter NotifyModeTests`
Expected: PASS.

- [ ] **Step 7: Teach the existing Notify CLI command both modes**

In `Tillerctl.swift`, replace the `Notify` struct's options and `run()` so `--session/--status` become optional and `--title/--subtitle/--body` are added:

```swift
struct Notify: ParsableCommand {
    static let configuration = CommandConfiguration(
        abstract: "Agent-status update (--session/--status) or user notification (--title/--body)."
    )
    @OptionGroup var socketOptions: SocketOptions
    // Mode 1: agent status (used by agent hooks — do not change semantics).
    @Option var session: String?
    @Option var status: String?   // running | needs-input | done | error
    @Option(name: .customLong("agent-session"),
            help: "Agent-native session reference for restore.")
    var agentSession: String?
    @Flag(name: .customLong("stdin-json"),
          help: "Read a hook JSON payload from stdin and extract the agent session id.")
    var stdinJSON = false
    // Mode 2: user-visible notification (cmux parity).
    @Option var title: String?
    @Option var subtitle: String?
    @Option var body: String?
    // Agent hooks may append extra positional payload (e.g. Codex notify JSON) —
    // scanned for a session reference, otherwise ignored.
    @Argument(parsing: .allUnrecognized) var extra: [String] = []

    func validate() throws {
        do {
            _ = try NotifyMode.resolve(session: session, status: status,
                                       title: title, body: body)
        } catch let error as NotifyModeError {
            throw ValidationError(error.usageMessage)
        }
    }

    func run() throws {
        if let title {
            _ = try roundTripOrDie(
                TillerctlRequestBuilder.notificationCreate(
                    title: title, subtitle: subtitle, body: body ?? ""),
                socket: socketOptions.socket)
            return
        }
        // Agent-status mode: unchanged behavior.
        var ref = agentSession
        if ref == nil, stdinJSON {
            let data = FileHandle.standardInput.readDataToEndOfFile()
            ref = AgentSessionExtractor.sessionRef(fromJSON: data)
        }
        if ref == nil {
            ref = AgentSessionExtractor.sessionRef(fromPayloadArguments: extra)
        }
        let response = try ControlClient.roundTrip(
            socketPath: socketOptions.socket,
            request: TillerctlRequestBuilder.notify(
                session: session!, status: status!, agentSession: ref)
        )
        guard response.ok else { throw ValidationError(response.error ?? "notify failed") }
    }
}
```

- [ ] **Step 8: Add list/clear CLI commands**

Append to `CmuxCommands.swift`:

```swift
// MARK: - Notification commands

struct ListNotifications: ParsableCommand {
    static let configuration = CommandConfiguration(
        commandName: "list-notifications", abstract: "List delivered notifications.")
    @OptionGroup var socketOptions: SocketOptions
    @OptionGroup var jsonFlag: JSONFlag
    func run() throws {
        let response = try roundTripOrDie(TillerctlRequestBuilder.notificationList(),
                                          socket: socketOptions.socket)
        printRows(response, key: "notifications",
                  columns: ["date", "title", "subtitle", "body"], asJSON: jsonFlag.json)
    }
}

struct ClearNotifications: ParsableCommand {
    static let configuration = CommandConfiguration(
        commandName: "clear-notifications", abstract: "Clear delivered notifications.")
    @OptionGroup var socketOptions: SocketOptions
    func run() throws {
        _ = try roundTripOrDie(TillerctlRequestBuilder.notificationClear(),
                               socket: socketOptions.socket)
    }
}
```

Register in `Tillerctl.swift`: `ListNotifications.self, ClearNotifications.self,`

- [ ] **Step 9: Verify — including hook regression**

Run: `cd Packages/TillerControl && swift build && swift test`
Expected: PASS — especially the pre-existing `TillerctlRequestTests` notify tests (hook mode must be untouched on the wire).
Run: `Scripts/ci.sh` → `CI OK`.

Manual e2e: `swift run tillerctl notify --title "Build" --body "done"` shows a macOS notification; `list-notifications` lists it; `clear-notifications` empties Notification Center; spawn an agent from the menu and confirm its status badge still works (hook-mode notify regression check).

- [ ] **Step 10: Commit**

```bash
git add App/AgentNotifier.swift App/AppModel.swift App/AppModel+Control.swift Packages/TillerControl
git commit -m "feat: user notification commands (notify --title, list, clear)"
```

---

### Task 10: Manual session restore (launch snapshot + restore-session + menu)

**Files:**
- Modify: `App/AppModel.swift` (bootstrap: capture snapshot)
- Modify: `App/AppModel+Control.swift` (session.restore case + restore logic)
- Modify: `App/TillerApp.swift` (History menu, ⌘⇧O)
- Modify: `Packages/TillerControl/Sources/tillerctl/CmuxCommands.swift` + `Tillerctl.swift`

**Interfaces:**
- Consumes: `tabs`, `openWorktreeIds`, `paneCommands` (populated by `restoreAgentSessions` during bootstrap), `persistTabs(for:)` (visibility widened in Step 1), `agentActivity.agentSpawned`, `watchExit(paneId:)`.
- Produces: `AppModel.launchSnapshot: LaunchSnapshot?`, `AppModel.restoreLaunchSnapshot() -> Int` (returns number of re-added tabs + remounted worktrees), socket method `session.restore`, CLI `restore-session`.

- [ ] **Step 1: Widen persistTabs visibility**

`persistTabs(for:)` is `private` in `App/AppModel.swift` (~line 663); Swift `private` is file-scoped, so the `AppModel+Control.swift` extension can't call it. Drop the modifier:

```swift
    func persistTabs(for worktreeId: UUID) {
```

(Same module, so internal is enough — no `public` needed.)

- [ ] **Step 2: Capture the snapshot at end of bootstrap**

In `App/AppModel.swift` add near the other stored properties:

```swift
    /// State as loaded at launch — the target of the manual
    /// "Restore Previous Launch" action. In-memory only.
    struct LaunchSnapshot {
        let tabs: [UUID: [WorkspaceTab]]
        let paneCommands: [UUID: String]
        let openWorktreeIds: [UUID]
    }
    private(set) var launchSnapshot: LaunchSnapshot?
```

In `bootstrap()`, after the project/worktree loop and before `startControlServer` gating, add:

```swift
            launchSnapshot = LaunchSnapshot(
                tabs: tabs, paneCommands: paneCommands,
                openWorktreeIds: openWorktreeIds
            )
```

- [ ] **Step 3: Implement restore + socket case**

In `App/AppModel+Control.swift`:

```swift
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
            let currentIds = Set((tabs[worktreeId] ?? []).map(\.id))
            var added = 0
            for tab in snapshotTabs where !currentIds.contains(tab.id) {
                tabs[worktreeId, default: []].append(tab)
                added += 1
                for paneId in tab.leafIds {
                    if let command = snapshot.paneCommands[paneId] {
                        paneCommands[paneId] = command
                        watchExit(paneId: paneId)
                    }
                }
            }
            if added > 0 { persistTabs(for: worktreeId) }
            restored += added
        }
        return restored
    }
```

Case in `handleCmuxControl`:

```swift
        case "session.restore":
            let restored = restoreLaunchSnapshot()
            return .success(id: request.id, result: ["restored": String(restored)])
```

Append `"session.restore",` to `cmuxMethods`.

- [ ] **Step 4: Menu item**

In `App/TillerApp.swift`, inside `.commands { … }` add:

```swift
            CommandMenu("History") {
                Button("Restore Previous Launch") {
                    _ = model.restoreLaunchSnapshot()
                }
                .keyboardShortcut("o", modifiers: [.command, .shift])
            }
```

- [ ] **Step 5: CLI command**

Append to `CmuxCommands.swift`:

```swift
// MARK: - Session restore

struct RestoreSession: ParsableCommand {
    static let configuration = CommandConfiguration(
        commandName: "restore-session",
        abstract: "Re-apply the layout Tiller loaded at launch.")
    @OptionGroup var socketOptions: SocketOptions
    func run() throws {
        let response = try roundTripOrDie(TillerctlRequestBuilder.sessionRestore(),
                                          socket: socketOptions.socket)
        print("restored \(response.result?["restored"] ?? "0") item(s)")
    }
}
```

Register in `Tillerctl.swift`: `RestoreSession.self,`

- [ ] **Step 6: Verify**

Run: `Scripts/ci.sh` → `CI OK`.

Manual e2e: launch app with saved tabs → close one tab (⌘W) → `swift run tillerctl restore-session` → prints `restored 1 item(s)` and the tab reappears with its scrollback; run again → `restored 0 item(s)`. Close a whole worktree from the sidebar → `restore-session` remounts it. Same via History → Restore Previous Launch (⌘⇧O).

- [ ] **Step 7: Commit**

```bash
git add App/AppModel.swift App/AppModel+Control.swift App/TillerApp.swift Packages/TillerControl
git commit -m "feat: manual restore of launch snapshot (restore-session, cmd-shift-O)"
```

---

### Task 11: Final verification + docs

**Files:**
- Modify: `CLAUDE.md` (control socket section — one paragraph)

- [ ] **Step 1: Full gate**

Run: `Scripts/ci.sh`
Expected: `CI OK`. (Known flake: `TillerTerminal spawnCapturesOutput` — one retry allowed.)

- [ ] **Step 2: Manual smoke checklist (app running)**

```bash
BIN=Packages/TillerControl/.build/debug/tillerctl
swift build --package-path Packages/TillerControl --product tillerctl
$BIN ping                       # pong
$BIN capabilities               # includes all new methods
$BIN list-workspaces            # sidebar contents
$BIN select-workspace --workspace "$(pwd)"
$BIN current-workspace
$BIN new-split right
$BIN list-panels
$BIN send 'echo ciao' && $BIN send-key enter    # runs in the active pane
$BIN notify --title "Tiller" --body "CLI works"
$BIN list-notifications
$BIN clear-notifications
$BIN identify                   # from outside a pane: selected context
$BIN restore-session            # restored 0 item(s) when nothing closed
```

Regression: spawn Claude from the agent menu → badge turns running (hook `notify --session` mode intact); toggle "Enable control socket" off in Settings → `$BIN ping` fails with a socket error; toggle on → `pong`.

- [ ] **Step 3: Update CLAUDE.md**

In the "Control socket (`TillerControl`)" section, extend the method list sentence to mention the cmux-parity groups, e.g. append: `Beyond the original methods, cmux-parity groups exist: workspace.* (worktrees), surface.*/pane.surfaces (panes), notification.*, system.* (ping/capabilities/identify), session.restore — all dispatched in App/AppModel+Control.swift; the flat kebab tillerctl commands (list-workspaces, send, …) mirror cmux's CLI names. The socket can be disabled in Settings (controlSocket.enabled / TILLER_SOCKET_ENABLE), which also disables Layer-A hooks.`

- [ ] **Step 4: Commit**

```bash
git add CLAUDE.md
git commit -m "docs: document cmux-parity control methods and socket toggle"
```

---

## Self-review notes

- Spec coverage: workspace group (T7), split/panels/focus (T8), send/send-key + active-pane resolution (T8), notifications + notify flag-dispatch tests (T9), utility (T6), restore manuale — tabs **and** closed worktrees (T10), socket toggle (T1-2), env vars (already exist — deviation 1), `--json` (printRows/printResult in T6), socket-disabled CLI message (roundTripOrDie, T6). Sidebar metadata, custom resume, ancestry check: out of scope per spec.
- The spec's `TILLER_WORKSPACE_ID`/`TILLER_SURFACE_ID` naming is intentionally replaced by the pre-existing `TILLER_WORKTREE_ID`/`TILLER_PANE_ID` (deviation 1) — update the spec doc if this bothers anyone downstream.
- Spec's "App target dispatch tests" are replaced by package-level tests + T11 manual smoke (deviation 4 — App has no test bundle).
- `handleCmuxControl` result rows convention: single key holding a JSON-array string (`workspaces`, `surfaces`, `notifications`, `methods`) — decoder is `ControlRows.decode`, used by the CLI only.
- Symbols verified against `App/AppModel.swift`: `worktree(byId:)` (:39), `openWorktreeIds: [UUID]` (:37), `lastError` (:125), `addWorktree(project:branch:) async` (:421), `activeTab(for:)` (:490), `activateTab(_:in:)` (:548), `tabContaining(paneId:) -> (worktree, tab, index)?` (:569), `split(paneId:axis:)` (:579), `persistTabs(for:)` (:663, private → widened in T10 Step 1), `paneCommands` (:732), `paneTitles` (:736), `watchExit(paneId:)` (:946), `notifier` (:120, private — bridged via wrappers in T9). `AgentActivityModel.paneAgents` confirmed public var.
