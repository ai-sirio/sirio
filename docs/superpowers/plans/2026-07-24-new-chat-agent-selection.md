# New Chat Agent Selection Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Agent becomes a creation-time choice for chats — New Chat shows an installed-agent submenu, in-chat switching is removed, and chat tab icons show the correct agent mark.

**Architecture:** `AppModel.openChatTab` takes the agent id explicitly (no `"claude-acp"` fallback), so `tab.chatAgentId` — which `WorkspaceTabIcon` already reads — is correct from creation and can never drift. A shared `NewChatMenuItems` submenu (installed ACP agents from `AcpAgentCenter`) replaces both flat "New Chat" buttons. `ChatAgentToolbarPicker` loses its `Menu` and becomes a static status indicator; `ChatController.switchAgent` is deleted as dead code.

**Tech Stack:** Swift 6, SwiftUI, swift-testing (`@Test`/`#expect`), xcodegen, `Scripts/ci.sh` gate.

**Spec:** `docs/superpowers/specs/2026-07-24-new-chat-agent-selection-design.md`

## Global Constraints

- App-facing UI strings are always English (standing user rule; also fix the stray `"Nuovo Terminale"` touched in Task 2).
- Never use `UserDefaults.standard` in AppTests — always a suite-named `UserDefaults` with `removePersistentDomain` (hosted tests share the app's defaults domain).
- `Scripts/ci.sh` must print `CI OK` before the work is considered done. The TillerTerminal PTY test `spawnCapturesOutput` is flaky — retry the gate up to 6 times before treating a failure there as real.
- AppTests are NOT run by `Scripts/ci.sh` (it runs package tests only); run them explicitly via `xcodebuild test`.
- Commit messages: Conventional Commits, lower-case imperative subject.

---

### Task 1: `openChatTab(agentId:in:)` — explicit agent at creation

**Files:**
- Create: `AppTests/OpenChatTabTests.swift`
- Modify: `App/AppModel.swift:1338-1367` (delete `defaultChatAgentId`, change `openChatTab` signature)
- Modify: `App/NewTabMenuItems.swift:35-37` (temporary literal call site; Task 2 replaces it)
- Modify: `App/SidebarView.swift:44-46` (temporary literal call site; Task 2 replaces it)
- Modify: `AppTests/AutoRenameWiringTests.swift:79`

**Interfaces:**
- Consumes: existing `WorkspaceTab`, `TabContent.chat(agentId:)`, `rememberChatAgent(_:)`, `chatController(for:in:startNewConversation:)`.
- Produces: `@discardableResult func openChatTab(agentId: String, in worktree: Worktree) -> WorkspaceTab?` on `AppModel`. The agent-less overload and `var defaultChatAgentId: String?` no longer exist. Task 2 and 3 rely on this exact signature.

- [ ] **Step 1: Write the failing test**

Create `AppTests/OpenChatTabTests.swift`. Copy the exact `import` block from the top of `AppTests/AutoRenameWiringTests.swift` (same target, same dependencies), then:

```swift
@MainActor
struct OpenChatTabTests {
    @Test func openChatTabUsesGivenAgentId() {
        let suiteName = "OpenChatTabTests-\(UUID().uuidString)"
        let defaults = UserDefaults(suiteName: suiteName)!
        defaults.removePersistentDomain(forName: suiteName)
        let model = AppModel(
            paneRegistry: PaneRegistry(), registrationTimeoutMs: 100, defaults: defaults)
        let worktree = Worktree(
            id: UUID(), projectId: UUID(), branch: "main", path: "/tmp/open-chat-tab")
        model.worktrees = [worktree.projectId: [worktree]]

        let tab = model.openChatTab(agentId: "codex-acp", in: worktree)

        #expect(tab?.chatAgentId == "codex-acp")
        defaults.removePersistentDomain(forName: suiteName)
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

```bash
xcodegen generate
xcodebuild test -project Tiller.xcodeproj -scheme Tiller \
  -derivedDataPath DerivedData CODE_SIGNING_ALLOWED=NO \
  -only-testing:TillerTests/OpenChatTabTests 2>&1 | tail -20
```

Expected: build FAILS — `extra argument 'agentId' in call` (the signature doesn't exist yet). A compile failure is the valid RED here.

- [ ] **Step 3: Change the AppModel signature**

In `App/AppModel.swift`, delete the `defaultChatAgentId` property entirely (lines ~1338-1343):

```swift
    var defaultChatAgentId: String? {
        ...
        return agentCenter.installedAgents.first?.id
    }
```

and replace `openChatTab(in:)` (lines ~1354-1367) with:

```swift
    @discardableResult
    func openChatTab(agentId: String, in worktree: Worktree) -> WorkspaceTab? {
        rememberChatAgent(agentId)
        let tab = WorkspaceTab(id: UUID(), title: "Chat",
                               content: .chat(agentId: agentId))
        selectedWorktree = worktree
        tabs[worktree.id, default: []].append(tab)
        activeTabId[worktree.id] = tab.id
        agentActivity.agentSpawned(paneId: tab.id, agentId: agentId, now: Date())
        persistTabs(for: worktree.id)
        _ = chatController(for: tab, in: worktree, startNewConversation: true)
        return tab
    }
```

Only the first two lines change (no `defaultChatAgentId ?? "claude-acp"`); the body is otherwise identical to the current one.

- [ ] **Step 4: Update the three call sites**

`App/NewTabMenuItems.swift:35-37` (temporary — Task 2 replaces this button with the submenu):

```swift
        Button("New Chat") {
            model.openChatTab(agentId: "claude-acp", in: worktree)
        }
```

`App/SidebarView.swift:44-46` (same temporary form):

```swift
                                            Button("New Chat") {
                                                model.openChatTab(agentId: "claude-acp", in: worktree)
                                            }
```

`AppTests/AutoRenameWiringTests.swift:79`:

```swift
        let tab = model.openChatTab(agentId: "claude-acp", in: worktree)!
```

- [ ] **Step 5: Run tests to verify they pass**

```bash
xcodebuild test -project Tiller.xcodeproj -scheme Tiller \
  -derivedDataPath DerivedData CODE_SIGNING_ALLOWED=NO \
  -only-testing:TillerTests/OpenChatTabTests \
  -only-testing:TillerTests/AutoRenameWiringTests 2>&1 | tail -20
```

Expected: `** TEST SUCCEEDED **`.

- [ ] **Step 6: Commit**

```bash
git add App/AppModel.swift App/NewTabMenuItems.swift App/SidebarView.swift \
  AppTests/OpenChatTabTests.swift AppTests/AutoRenameWiringTests.swift Tiller.xcodeproj
git commit -m "feat: chat tabs take an explicit agent id at creation"
```

---

### Task 2: Shared New Chat submenu at both entry points

**Files:**
- Modify: `App/NewTabMenuItems.swift` (add `NewChatMenuItems`, use it)
- Modify: `App/SidebarView.swift:35-46` (use `NewChatMenuItems`, fix Italian string)

**Interfaces:**
- Consumes: `openChatTab(agentId:in:)` (Task 1), `AppModel.agentCenter.installedAgents: [InstalledAgentSummary]` (`id`, `name`, `Identifiable`), `AgentMenuIconCache.image(for:)`, `AppModel.openAgentsSettings()` (all existing).
- Produces: `struct NewChatMenuItems: View` (init `(model: AppModel, worktree: Worktree)`) in `App/NewTabMenuItems.swift`, used by both entry points.

- [ ] **Step 1: Add the shared submenu view**

In `App/NewTabMenuItems.swift`, replace the trailing New Chat button

```swift
        Divider()
        Button("New Chat") {
            model.openChatTab(agentId: "claude-acp", in: worktree)
        }
```

with

```swift
        Divider()
        NewChatMenuItems(model: model, worktree: worktree)
```

and append at the end of the file:

```swift
/// Shared "New Chat" submenu listing installed ACP agents, used by the "+"
/// menu and the sidebar worktree context menu. Empty install list falls back
/// to a single item that opens the Agents settings.
struct NewChatMenuItems: View {
    @Bindable var model: AppModel
    let worktree: Worktree

    var body: some View {
        Menu("New Chat") {
            ForEach(model.agentCenter.installedAgents) { agent in
                Button {
                    model.openChatTab(agentId: agent.id, in: worktree)
                } label: {
                    if let icon = AgentMenuIconCache.image(for: agent.id) {
                        Label {
                            Text(agent.name)
                        } icon: {
                            Image(nsImage: icon)
                        }
                    } else {
                        Text(agent.name)
                    }
                }
            }
            if model.agentCenter.installedAgents.isEmpty {
                Button("Other agents…") { model.openAgentsSettings() }
            }
        }
    }
}
```

- [ ] **Step 2: Use it in the sidebar context menu**

In `App/SidebarView.swift`, inside the `WorktreeRow` context menu, replace

```swift
                                            Button("Nuovo Terminale") {
                                                model.newShellTab(in: worktree)
                                            }
```

with (stray Italian string — standing English-strings rule):

```swift
                                            Button("New Terminal") {
                                                model.newShellTab(in: worktree)
                                            }
```

and replace

```swift
                                            Button("New Chat") {
                                                model.openChatTab(agentId: "claude-acp", in: worktree)
                                            }
```

with

```swift
                                            NewChatMenuItems(model: model, worktree: worktree)
```

- [ ] **Step 3: Verify the app builds**

```bash
xcodebuild -project Tiller.xcodeproj -scheme Tiller -configuration Debug \
  -derivedDataPath DerivedData CODE_SIGNING_ALLOWED=NO build 2>&1 | tail -5
```

Expected: `** BUILD SUCCEEDED **`. (Thin SwiftUI menu — no unit test; behavior seam was tested in Task 1.)

- [ ] **Step 4: Commit**

```bash
git add App/NewTabMenuItems.swift App/SidebarView.swift
git commit -m "feat: new chat submenu with installed-agent choice"
```

---

### Task 3: Static toolbar indicator + delete `switchAgent`

**Files:**
- Modify: `App/Chat/ChatAgentToolbarPicker.swift` (strip the `Menu`)
- Modify: `App/Chat/ChatController.swift:280-310` (delete `switchAgent`)

**Interfaces:**
- Consumes: `agentStatusDotColor(for:)` (same file), `AgentIcon`, `AppModel.agentCenter.displayName(for:)` (all existing).
- Produces: `ChatAgentToolbarPicker` keeps its name and `(controller:appModel:)` init, so `App/ContentView.swift:107` needs no change. `ChatController.switchAgent(to:displayName:)` no longer exists (verified: its only caller is this picker; no tests exercise it).

- [ ] **Step 1: Make the picker a static indicator**

Replace the whole `ChatAgentToolbarPicker` struct in `App/Chat/ChatAgentToolbarPicker.swift` with:

```swift
/// Window-toolbar agent indicator for the active chat tab: agent icon with
/// fused status dot plus display name. Read-only — the agent is chosen at
/// chat creation and cannot change afterwards.
struct ChatAgentToolbarPicker: View {
    let controller: ChatController
    let appModel: AppModel

    var body: some View {
        HStack(spacing: 5) {
            AgentIcon(agentId: controller.agentId, size: 14)
                .overlay(alignment: .bottomTrailing) {
                    Circle()
                        .fill(agentStatusDotColor(for: controller.state))
                        .frame(width: 5, height: 5)
                        .offset(x: 1.5, y: 1.5)
                }
            Text(appModel.agentCenter.displayName(for: controller.agentId))
                .font(.caption)
        }
        .fixedSize()
    }
}
```

Keep `typealias ChatState` and `agentStatusDotColor(for:)` at the top of the file unchanged.

- [ ] **Step 2: Delete `switchAgent`**

In `App/Chat/ChatController.swift`, delete the whole method and its doc comment (lines ~280-310):

```swift
    /// Switches the conversation to another agent: same transcript, new
    /// process, new ACP session; the next prompt carries the handoff preamble.
    func switchAgent(to newAgentId: String, displayName: String) async {
        ...
        await start()
        persist()
    }
```

Do NOT remove `AgentIdMigration` or `pendingHandoff` — both have other users.

- [ ] **Step 3: Verify the app builds and chat tests pass**

```bash
xcodebuild -project Tiller.xcodeproj -scheme Tiller -configuration Debug \
  -derivedDataPath DerivedData CODE_SIGNING_ALLOWED=NO build 2>&1 | tail -5
xcodebuild test -project Tiller.xcodeproj -scheme Tiller \
  -derivedDataPath DerivedData CODE_SIGNING_ALLOWED=NO \
  -only-testing:TillerTests 2>&1 | tail -20
```

Expected: `** BUILD SUCCEEDED **`, then `** TEST SUCCEEDED **`. If a `TillerTests` failure mentions `AppModelControlTests`, it is a known pre-existing flaky area — re-run once before investigating.

- [ ] **Step 4: Commit**

```bash
git add App/Chat/ChatAgentToolbarPicker.swift App/Chat/ChatController.swift
git commit -m "feat: remove in-chat agent switching, toolbar shows static agent badge"
```

---

### Task 4: Full CI gate

**Files:** none (verification only).

- [ ] **Step 1: Run the gate**

```bash
Scripts/ci.sh
```

Expected: final line `CI OK`. Known flake: `TillerTerminal` test `spawnCapturesOutput` — if it is the only failure, re-run `Scripts/ci.sh` (up to 6 attempts) before treating it as a real regression.

- [ ] **Step 2: Manual smoke checklist (report to user, do not skip)**

1. "+" menu → New Chat shows submenu of installed agents with icons; picking one opens a chat with that agent.
2. Sidebar worktree context menu → same submenu; "New Terminal" now English.
3. New chat with a non-Claude agent → sidebar tab icon and tab-bar icon show that agent's mark, not Claude's.
4. Chat toolbar shows static icon+name+status dot; no chevron, no menu, no agent switching.
