# Chat Full-Height Layout (OnPeer-style) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Remove the chat pane's header row so the transcript fills the full detail column; move the agent picker (with a status dot) into the window toolbar and the follow/new-conversation actions into the composer.

**Architecture:** Three view-layer changes in the `App/` target: a new `ChatAgentToolbarPicker` view shown in ContentView's toolbar only when the active tab is a chat, deletion of `ChatPaneView.header`, and two icon buttons added to `ChatComposerView.controlBar`. The only new logic is a pure `ChatState -> Color` mapping, which gets a unit test.

**Tech Stack:** SwiftUI (macOS 15+, Swift 6), swift-testing (`@Test`/`#expect`), xcodegen, `Scripts/ci.sh` as the gate.

**Spec:** `docs/superpowers/specs/2026-07-23-chat-fullheight-toolbar-design.md`

## Global Constraints

- UI strings are always English, even though conversation with the user may be Italian.
- Tests use swift-testing (`import Testing`, `@Test`, `#expect`), never XCTest.
- After creating any new file, run `xcodegen generate` before building (never hand-edit `Tiller.xcodeproj`).
- Never use `UserDefaults.standard` in AppTests (hosted tests share the app's defaults and wipe user preferences).
- `Scripts/ci.sh` must print `CI OK` before the work is considered done. Note: the TillerTerminal test `spawnCapturesOutput` is known-flaky and may need up to 5-6 retries of `ci.sh`.
- Commit messages: Conventional Commits, lower-case imperative subject.
- No changes to TabBarView, UsageBarView, SidebarView, RightPanelView, or any right-panel default (already hidden by default).

## Reference: existing API used by this plan (do not modify)

- `App/Chat/ChatController.swift:11` — `enum ChatState: Equatable { case idle, connecting, ready, prompting, needsAuth; case disconnected(message: String?) }`
- `ChatController` (`@MainActor @Observable`): `state: ChatState`, `agentId: String`, `isFollowing: Bool`, `func newConversation() async`, `func switchAgent(to:displayName:) async`
- `AppModel`: `selectedWorktree: Worktree?`, `activeTab(for: UUID) -> WorkspaceTab?`, `chatController(for: WorkspaceTab, in: Worktree) -> ChatController?`, `rememberChatAgent(_ id: String)`, `openAgentsSettings()`, `agentCenter.installedAgents`, `agentCenter.displayName(for: String) -> String`
- `TabContent` (`Packages/TillerCore/Sources/TillerCore/WorkspaceTab.swift:5`): `case terminal(SplitTree)`, `case markdown(fileURL: URL)`, `case chat(agentId: String)`
- `AgentIcon(agentId: String, size: CGFloat)` view; `AgentMenuIconCache.image(for: String) -> NSImage?`

---

### Task 1: Status dot color function

**Files:**
- Create: `App/Chat/ChatAgentToolbarPicker.swift`
- Test: `AppTests/AgentStatusDotColorTests.swift`

**Interfaces:**
- Consumes: `ChatState` (existing, see Reference).
- Produces: `func agentStatusDotColor(for state: ChatState) -> Color` (module-internal, used by Task 2's view).

- [ ] **Step 1: Write the failing test**

Create `AppTests/AgentStatusDotColorTests.swift`:

```swift
import SwiftUI
import Testing

@testable import Tiller

struct AgentStatusDotColorTests {
    @Test func readyIsGreen() {
        #expect(agentStatusDotColor(for: .ready) == .green)
    }

    @Test func promptingIsOrange() {
        #expect(agentStatusDotColor(for: .prompting) == .orange)
    }

    @Test func inactiveStatesAreGray() {
        #expect(agentStatusDotColor(for: .idle) == .gray)
        #expect(agentStatusDotColor(for: .connecting) == .gray)
        #expect(agentStatusDotColor(for: .needsAuth) == .gray)
        #expect(agentStatusDotColor(for: .disconnected(message: nil)) == .gray)
    }
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run:
```bash
xcodegen generate
xcodebuild -project Tiller.xcodeproj -scheme Tiller -configuration Debug \
  -derivedDataPath DerivedData CODE_SIGNING_ALLOWED=NO test \
  -only-testing:TillerTests/AgentStatusDotColorTests 2>&1 | tail -20
```
Expected: build FAILS with `cannot find 'agentStatusDotColor' in scope`.

- [ ] **Step 3: Write the minimal implementation**

Create `App/Chat/ChatAgentToolbarPicker.swift`:

```swift
import SwiftUI

/// Maps the chat controller state to the status dot shown on the toolbar
/// agent picker (replaces the old ChatPaneView header state chip).
func agentStatusDotColor(for state: ChatState) -> Color {
    switch state {
    case .ready: .green
    case .prompting: .orange
    case .idle, .connecting, .needsAuth, .disconnected: .gray
    }
}
```

- [ ] **Step 4: Run the test to verify it passes**

Run the same command as Step 2 (xcodegen already ran; re-run it anyway — it is idempotent).
Expected: `** TEST SUCCEEDED **` (3 tests pass).

- [ ] **Step 5: Commit**

```bash
git add App/Chat/ChatAgentToolbarPicker.swift AppTests/AgentStatusDotColorTests.swift
git commit -m "feat: add agent status dot color mapping for toolbar picker"
```

---

### Task 2: Toolbar agent picker wired into ContentView

**Files:**
- Modify: `App/Chat/ChatAgentToolbarPicker.swift` (add the view below the function from Task 1)
- Modify: `App/ContentView.swift:35-41` (new computed property) and `App/ContentView.swift:98` (toolbar group)

**Interfaces:**
- Consumes: `agentStatusDotColor(for:)` (Task 1), `ChatController`, `AppModel` (see Reference).
- Produces: `struct ChatAgentToolbarPicker: View` with `init(controller: ChatController, appModel: AppModel)`; ContentView private computed property `activeChatController: ChatController?`.

- [ ] **Step 1: Add the view**

Append to `App/Chat/ChatAgentToolbarPicker.swift`:

```swift
/// Window-toolbar replacement for the removed ChatPaneView header: agent
/// menu labeled with the agent icon (status dot fused onto it), name, and
/// chevron. Shown only while the active tab is a chat.
struct ChatAgentToolbarPicker: View {
    let controller: ChatController
    let appModel: AppModel

    var body: some View {
        Menu {
            ForEach(appModel.agentCenter.installedAgents) { agent in
                Button {
                    Task {
                        await controller.switchAgent(to: agent.id,
                                                     displayName: agent.name)
                        appModel.rememberChatAgent(agent.id)
                    }
                } label: {
                    if let image = AgentMenuIconCache.image(for: agent.id) {
                        Label { Text(agent.name) } icon: { Image(nsImage: image) }
                    } else {
                        Text(agent.name)
                    }
                }
                .disabled(agent.id == controller.agentId)
            }
            Divider()
            Button("Other agents…") { appModel.openAgentsSettings() }
        } label: {
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
                Image(systemName: "chevron.down")
                    .font(.system(size: 8, weight: .semibold))
                    .foregroundStyle(.secondary)
            }
        }
        .menuStyle(.borderlessButton)
        .fixedSize()
        .help("Switch agent for this conversation")
    }
}
```

- [ ] **Step 2: Wire it into ContentView**

In `App/ContentView.swift`, add this computed property next to `rightPanelContext` (after line 41):

```swift
private var activeChatController: ChatController? {
    guard let worktree = model.selectedWorktree,
          let tab = model.activeTab(for: worktree.id),
          case .chat = tab.content else { return nil }
    return model.chatController(for: tab, in: worktree)
}
```

In the same file, inside `ToolbarItemGroup(placement: .primaryAction)` (line 98), insert as the FIRST entry, before the right-panel button:

```swift
if let controller = activeChatController {
    ChatAgentToolbarPicker(controller: controller, appModel: model)
}
```

- [ ] **Step 3: Build to verify it compiles**

Run:
```bash
xcodebuild -project Tiller.xcodeproj -scheme Tiller -configuration Debug \
  -derivedDataPath DerivedData CODE_SIGNING_ALLOWED=NO build 2>&1 | tail -5
```
Expected: `** BUILD SUCCEEDED **`.

- [ ] **Step 4: Commit**

```bash
git add App/Chat/ChatAgentToolbarPicker.swift App/ContentView.swift
git commit -m "feat: show chat agent picker in window toolbar for active chat tab"
```

---

### Task 3: Remove the ChatPaneView header

**Files:**
- Modify: `App/Chat/ChatPaneView.swift`

**Interfaces:**
- Consumes: nothing new.
- Produces: nothing — pure removal. The toolbar picker (Task 2) and composer buttons (Task 4) replace the header's controls, so this task must land AFTER Task 2.

- [ ] **Step 1: Delete the header**

In `App/Chat/ChatPaneView.swift`:

1. In `body`, delete the two lines `header` and `Divider()` (lines 15-16), so the `VStack(spacing: 0)` starts directly with the `switch controller.state` banners.
2. Delete the whole `private var header: some View { ... }` property (lines 70-125).
3. Delete the whole `@ViewBuilder private var stateChip: some View { ... }` property (lines 127-143).
4. Update the doc comment on line 6 from
   `/// A whole chat tab: header (agent identity + state + new conversation),`
   `/// transcript, composer. State banners cover auth/disconnect/npx failures.`
   to
   `/// A whole chat tab: transcript + composer. Agent identity/state live in`
   `/// the window toolbar; state banners cover auth/disconnect/npx failures.`

The resulting `body` starts:

```swift
    var body: some View {
        VStack(spacing: 0) {
            switch controller.state {
            case .needsAuth:
```

Everything else in the file (banners, `banner(_:detail:actionTitle:action:)`, `.task` modifier) stays untouched.

- [ ] **Step 2: Build to verify it compiles**

Run:
```bash
xcodebuild -project Tiller.xcodeproj -scheme Tiller -configuration Debug \
  -derivedDataPath DerivedData CODE_SIGNING_ALLOWED=NO build 2>&1 | tail -5
```
Expected: `** BUILD SUCCEEDED **`. If it fails with unused-symbol or reference errors mentioning `header` or `stateChip`, a caller was missed — search the App target for those names and remove the stale reference.

- [ ] **Step 3: Commit**

```bash
git add App/Chat/ChatPaneView.swift
git commit -m "feat: remove chat pane header so transcript fills full height"
```

---

### Task 4: Follow + New conversation in the composer

**Files:**
- Modify: `App/Chat/ChatComposerView.swift:90-111` (`controlBar`)

**Interfaces:**
- Consumes: `controller.isFollowing`, `controller.newConversation()` (existing).
- Produces: nothing consumed by later tasks.

- [ ] **Step 1: Add the two buttons**

In `App/Chat/ChatComposerView.swift`, inside `controlBar`, insert between `contextUsageIndicator` and the paperclip `Button`:

```swift
            Button {
                controller.isFollowing.toggle()
            } label: {
                Image(systemName: controller.isFollowing ? "eye.fill" : "eye")
                    .foregroundStyle(controller.isFollowing
                                     ? Color.accentColor : Color.secondary)
            }
            .buttonStyle(.plain)
            .help("Opens the files the agent is editing in the right panel")
            Button {
                Task { await controller.newConversation() }
            } label: {
                Image(systemName: "plus.bubble")
                    .foregroundStyle(.secondary)
            }
            .buttonStyle(.plain)
            .help("New conversation")
```

The resulting `controlBar` order is: `modePill`, `agentPill`, `Spacer()`, `contextUsageIndicator`, follow button, new-conversation button, paperclip, stop/send. Note: these two buttons are NOT gated on `canInteract` — new-conversation must stay usable while disconnected, matching the removed header's behavior.

Also update the doc comment on line 6-7 from
`/// Message input styled as a floating rounded card: text on top, control row`
`/// below (mode pill with status dot, agent pill, attach, circular send).`
to
`/// Message input styled as a floating rounded card: text on top, control row`
`/// below (mode pill with status dot, agent pill, follow, new conversation,`
`/// attach, circular send).`

- [ ] **Step 2: Build to verify it compiles**

Run:
```bash
xcodebuild -project Tiller.xcodeproj -scheme Tiller -configuration Debug \
  -derivedDataPath DerivedData CODE_SIGNING_ALLOWED=NO build 2>&1 | tail -5
```
Expected: `** BUILD SUCCEEDED **`.

- [ ] **Step 3: Commit**

```bash
git add App/Chat/ChatComposerView.swift
git commit -m "feat: move follow and new-conversation actions into chat composer"
```

---

### Task 5: Gate and smoke checklist

**Files:**
- None created; verification only.

**Interfaces:**
- Consumes: everything above.
- Produces: green CI, ready-to-merge branch.

- [ ] **Step 1: Run the full gate**

Run:
```bash
Scripts/ci.sh
```
Expected: `CI OK`. If TillerTerminal's `spawnCapturesOutput` flakes, re-run up to 5-6 times; any OTHER failure is real and must be fixed before proceeding.

- [ ] **Step 2: Run the hosted App tests**

Run:
```bash
xcodebuild -project Tiller.xcodeproj -scheme Tiller -configuration Debug \
  -derivedDataPath DerivedData CODE_SIGNING_ALLOWED=NO test 2>&1 | tail -10
```
Expected: `** TEST SUCCEEDED **`.

- [ ] **Step 3: Manual smoke checklist (report results, human verifies visually)**

Launch the app (⌘R from Xcode or the built product) and check:

1. Open a chat tab → no header row above the transcript; transcript starts directly under the tab bar; agent picker with status dot appears top-right in the titlebar.
2. Switch agent from the toolbar picker → conversation switches agent; picker label updates.
3. Status dot: gray while connecting, green when ready, orange while a prompt is running.
4. Follow (eye) and new-conversation (plus.bubble) buttons work from the composer row.
5. Switch to a terminal tab → toolbar picker disappears; toolbar identical to before this change.
6. Right panel still hidden by default on a fresh state (`defaults delete com.tiller.app rightPanel.visible` to reset if needed — check the actual bundle id with `mdls -name kMDItemCFBundleIdentifier` on the built app first).

- [ ] **Step 4: Final commit (only if smoke fixes were needed)**

```bash
git add -A
git commit -m "fix: chat full-height smoke fixes"
```
