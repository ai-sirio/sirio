# Universal Chat Port — Implementation Plan

> **For agentic workers:** implement task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. Every task ends with the repo compiling and its own verification command green; every **phase** ends with `Scripts/ci.sh` printing `CI OK`.

**Goal:** Make chat tabs a real, first-class content kind on the universal workspace engine (`WorkspaceCoordinator` + `ChatContentAdapter`), at functional parity with the legacy `terminalStack` chat rendering, so that Phase 14 of [2026-07-29-multi-pane-workspace.md](./2026-07-29-multi-pane-workspace.md) (legacy-engine deletion) can proceed without destroying the only working chat implementation.

**Why this plan exists:** the 2026-07-31 pre-Task-14.1 audit found that the universal engine only ever implemented terminal panes. `ChatContentAdapter.makeHost` returns a bare `NSViewController()`; `.newChat(agentID:)` mints a throwaway `ChatContentID(UUID().uuidString)` never connected to a session; `AppModel.chatController(for:)` has exactly one overload, typed `LegacyWorkspaceTab`. Markdown and code were ported in commit `74bdf84`. Chat is the last content kind left, and it is the largest.

**Architecture:** `App` stays the sole composition root. `ChatContentAdapter` gains two injected closures (session minting, controller construction) rather than a reference to `AppModel`, so it remains unit-testable with fakes and does not invert the package dependency direction. `TillerCore` gains nothing: chat identity is already fully expressible as `ChatContentID`.

**Tech Stack:** Swift 6 (strict concurrency), SwiftUI + AppKit (`NSHostingController`), swift-testing (`@Test`/`#expect`), GRDB/SQLite via `TillerPersistence`.

---

## Global Constraints

- Tests are written **before** the production change they cover, using swift-testing, never XCTest.
- Conventional Commits, lower-case imperative subject.
- The `WorkspaceEngineGate` still exists during this plan. Every production entry point touched here keeps its legacy branch working until Phase 14 deletes the gate. Do **not** delete legacy chat code in this plan.
- `App` is the only target that may import `Tiller`'s chat types; `ChatContentAdapter` must not gain an `AppModel` stored property.
- No behaviour regression for the legacy path: `AppModelFacadeTests` legacy assertions (`openChatTab` → `legacyTabs`) must keep passing untouched.

---

## Design decisions (pinned — do not re-litigate mid-implementation)

### D1 — `ChatContentID.rawValue` **is** the `ChatSessionStore` session id

`openChatTab` already creates the session row *before* the tab exists, precisely so the tab knows which conversation it owns before the first turn is persisted (`AppModel.swift:1913-1927`). That row carries `agentId`. Therefore:

- New chat: `ChatContentAdapter` calls the injected session factory → `chatStore.createSession(worktreeId:agentId:)` → uses `session.id` as `ChatContentID`.
- Resumed chat / restored chat: `agentId` is recovered with `chatStore.session(id: contentID.rawValue)?.agentId`, run through `AgentIdMigration.canonical` exactly as `openExistingChatSession` does today.

**Consequence:** `WorkspaceTab` needs **no new field**, `WorkspaceContentRef.chat` needs no second associated value, and no persistence migration is required. The universal model stays content-neutral — it never learns what an "agent" is.

**Failure mode this pins:** if `chatStore` is nil or `createSession` throws, preparation **fails** (`ContentAdapterError`), which under `WorkspaceCoordinator` means no tab, no group, no split, no focus change (FH-B-2). Silently minting a random `ChatContentID` — today's behaviour — is what created a chat tab that can never find its own conversation, and is banned.

### D2 — Dependencies are injected as closures, late-bound after `AppModel.init`

`AppModel.init` constructs the default adapters (`AppModel.swift:252-259`) *before* `self` is available, so constructor injection of `AppModel` is impossible without an implicitly-unwrapped cycle. `ChatContentAdapter` therefore exposes two settable closures, assigned at the end of `AppModel.init` with `[weak self]`:

```swift
// ChatContentAdapter
var makeSession: ((_ worktreeID: UUID, _ agentID: String) throws -> String)?
var makeController: ((WorkspaceTab, Worktree, _ startNewConversation: Bool) -> ChatController?)?
```

Rejected alternatives, and why:
- **Extending `AdapterBoundary`** — it is a generic lifecycle-hook struct (hydrate/checkpoint/close/dispose/reissue) shared by all three adapters. Adding chat-specific fields makes every adapter carry chat concerns.
- **Passing `chatStore`/`agentInstallStore`/`persistenceCoordinator`/`agentActivity` individually** — four dependencies to thread, and the adapter would then own the `onStatusChange` → `notifyTransition` wiring that belongs to the app's notification layer. The closure keeps one seam.

### D3 — `chatControllers` stays keyed on `UUID`, using `tab.id.rawValue`

`WorkspaceTabID` wraps a `UUID`. `ChatController.tabId`, `agentActivity`, `notifyTransition`, `applyAutoTitle`, and `AgentsPanelModel` are all already `UUID`-keyed and already use `tab.id.rawValue` for the universal path (done in Task 14.0). No re-keying, no parallel dictionary.

### D4 — `restoreAgentSessions` is **out of scope**, and is a terminal bug, not a chat one

`restoreAgentSessions` (`AppModel.swift:2192`) replays `agentSessionRefs`, which are written per **terminal pane** running an agent CLI, to rewire a restored pane onto its agent's resume command. Chat tabs never appear in it. The real gap — that it is only reachable from `restoreWorktree`'s legacy `if let loaded {}` branch (`AppModel.swift:468`) and computes `paneIds` from `restoredTabs.flatMap(\.leafIds)` — belongs to the **terminal** port and is tracked as Phase 4 below only because nothing else will pick it up before Phase 14 deletes its call site.

### D5 — Mount semantics are preserved, not redesigned

Legacy `terminalStack` keeps an opened chat mounted forever and mounts a never-opened chat only when visible (`ContentView.swift:425-442`), because remounting rebuilt the whole transcript and read as a stall. The universal engine reaches the same outcome differently: `WorkspaceViewController` keeps one host per tab and `makeHost` is called once. So `makeHost` may construct the controller eagerly for its tab — but `hydrate` must remain the lazy start seam, so a restored-but-never-viewed chat does not launch its agent process at restore time (LC-3).

---

## Phase 1 — Adapter identity and dependency seam

*No UI yet. This phase makes a universal chat tab carry a real, resolvable conversation identity.*

### Task 1.1 — Failing tests for session-backed `ChatContentID`

- [ ] Add `AppTests/Workspace/ChatContentAdapterTests.swift` with a fake session factory (a closure recording its arguments and returning a fixed id) and a fake controller factory returning `nil`.

```swift
@Test func newChatMintsTheContentIDFromTheCreatedSession() async throws { }
@Test func newChatPassesTheWorktreeIDAndAgentIDToTheSessionFactory() async throws { }
@Test func resumeChatReusesTheGivenContentIDWithoutCreatingASession() async throws { }
@Test func newChatWithoutASessionFactoryThrowsAndCreatesNoTab() async throws { }
@Test func newChatWhoseSessionFactoryThrowsCreatesNoTab() async throws { }
```

The last two pin D1's failure mode: `prepare(request:worktree:)` must `throw`, and `prepareTab` must return `nil`, so `WorkspaceCoordinator` performs no mutation.

- [ ] Verify: `xcodebuild ... -only-testing:TillerTests/ChatContentAdapterTests` — tests fail (adapter still mints a random UUID).

### Task 1.2 — Implement the seam

- [ ] Add `makeSession` / `makeController` optional closures to `ChatContentAdapter` (D2).
- [ ] Rewrite `prepareTab(request:)` → `prepareTab(request:worktreeID:)` (mirroring the shape `DocumentContentAdapter` already uses), with:
  - `.newChat(agentID)`: `guard let makeSession, let sessionID = try? makeSession(worktreeID, agentID) else { return nil }`; `contentID = ChatContentID(sessionID)`.
  - `.resumeChat(id)`: unchanged, no session creation.
- [ ] Keep `prepareTab(request:)` as a deprecated shim only if a caller still needs it; otherwise delete it and fix call sites.
- [ ] Wire the closure in `AppModel.init` after `self` is fully initialised:

```swift
(self.workspaceCoordinator.adapters[.chat] as? ChatContentAdapter)?.makeSession = { [weak self] worktreeID, agentID in
    guard let store = self?.chatStore else { throw ChatContentAdapterError.noStore }
    return try store.createSession(worktreeId: worktreeID.uuidString, agentId: agentID).id
}
```

- [ ] Verify: the Task 1.1 tests pass.

### Task 1.3 — Title from the agent, not the raw id

Today `.newChat` sets `title = agentID` (the raw catalog id, e.g. `claude-acp`). Legacy `openChatTab` sets `"Chat"` and lets auto-rename replace it (D3 keeps auto-rename working unchanged).

- [ ] Test: `@Test func newChatTabIsTitledChatSoAutoRenameCanReplaceIt() { }`
- [ ] Set `title = "Chat"`, `titleIsAutoNamed = true`.
- [ ] Verify: `Scripts/ci.sh` → `CI OK`. **Commit:** `feat: back universal chat tabs with real chat sessions`

---

## Phase 2 — Real chat host

### Task 2.1 — Failing test for controller construction

- [ ] Extend `ChatContentAdapterTests`:

```swift
@Test func makeHostAsksTheControllerFactoryOncePerTab() { }
@Test func makeHostReturnsAPlaceholderWhenTheControllerFactoryReturnsNil() { }
@Test func closeReleasesTheControllerSoAReopenBuildsAFreshOne() { }
```

The second test pins the "Agent not available" case that `ContentView.swift:438` handles today: a missing agent must degrade to a visible placeholder, never to a blank pane.

### Task 2.2 — Implement `makeHost`

- [ ] Mirror `DocumentContentAdapter.makeHost` (commit `74bdf84`): `guard case .chat(let contentID) = tab.content`, call `makeController(tab, worktree, startNewConversation)`, wrap `ChatPaneView(controller:worktree:appModel:)` in `NSHostingController`, cache per `tab.id`.
- [ ] On `nil` controller, return an `NSHostingController` wrapping `ContentUnavailableView("Agent not available", systemImage: "bubble.left")` — matching the legacy string exactly.
- [ ] `close(tab:)` / `dispose(prepared:)` additionally call the app's `teardownChatController(tabId:)` seam so the empty-session cleanup (`chatStore.deleteIfEmpty`) and `controller.stop()` still run.

### Task 2.3 — Wire `makeController` in `AppModel`

- [ ] Add a `WorkspaceTab`-typed sibling of `chatController(for:in:startNewConversation:startDetached:)`. It resolves `agentId` per D1:

```swift
func chatController(for tab: WorkspaceTab, in worktree: Worktree,
                    startNewConversation: Bool = false) -> ChatController? {
    guard case .chat(let contentID) = tab.content else { return nil }
    let tabId = tab.id.rawValue
    if let controller = chatControllers[tabId] { return controller }
    guard let record = try? chatStore?.session(id: contentID.rawValue) else { return nil }
    let agentId = AgentIdMigration.canonical(record.agentId)
    // ... identical ChatController(...) construction + onStatusChange wiring as the legacy overload
}
```

- [ ] **Deduplicate:** the two overloads must share one private `makeChatController(tabId:agentId:sessionId:in:startNewConversation:startDetached:)`. Two hand-copied `ChatController(...)` call sites will drift.
- [ ] Assign `makeController` in `AppModel.init` alongside `makeSession`.
- [ ] Verify: `Scripts/ci.sh` → `CI OK`. **Commit:** `feat: render real chat panes on the universal workspace engine`

---

## Phase 3 — Entry points and agent identity

*Everything above is reachable only if something creates a universal chat tab. Today nothing does.*

### Task 3.1 — `openChatTab` gate branch

- [ ] Test in `AppTests/Workspace/`, using the established fake-`WorkspaceLayoutPersistence` + poll pattern (see `CodeFileTabTests.swift:129`):

```swift
@Test func openChatTabCreatesAUniversalChatTabBackedByASession() async throws { }
@Test func openChatTabRegistersTheAgentIdForTheAgentsPanel() async throws { }
```

- [ ] Add the `WorkspaceEngineGate.isEnabled` branch to `openChatTab(agentId:in:)`, following the exact shape `openDocument` got in `74bdf84`: resolve `activeOrFirstGroup`, fire `Task { await workspaceCoordinator.requestNewTab(into: group, choice: .newChat(agentID: agentId), in: worktree) }`, return `nil`.
- [ ] **Check every caller first.** `openDocument`'s return value was already discarded by all production callers; `openChatTab` is `@discardableResult` but `AppModelFacadeTests` uses its return. Confirm production callers (menus, sidebar `+`, toolbar) before changing the contract — if any consumes the tab, that caller needs the universal branch too, not a `nil`.
- [ ] `agentActivity.agentSpawned(paneId:agentId:now:)` must fire for the universal tab as well, keyed on `tab.id.rawValue`. Since `requestNewTab` is async, this moves inside the `Task`, after the coordinator returns the created tab.

### Task 3.2 — `openExistingChatSession` gate branch

- [ ] Test: `@Test func openingAnExistingSessionFocusesTheTabAlreadyShowingIt() async throws { }` and `@Test func openingAnExistingSessionOpensADetachedTabWhenNotAlreadyOpen() async throws { }`
- [ ] Dedup by scanning `layouts[worktree.id]?.allTabs` for `.chat(contentID)` where `contentID.rawValue == sessionId` — the universal analogue of today's `chatSessionId` scan — then `.activateTab`, else `requestNewTab(choice: .resumeChat(ChatContentID(sessionId)))`.
- [ ] Same for `deleteChatSession(sessionId:in:)`, which uses the same scan to close the open tab.

### Task 3.3 — Agent identity on restore

- [ ] Test: `@Test func restoringAWorktreeRegistersAgentIdsForItsChatTabs() async throws { }`
- [ ] In `restoreWorktree`, after `workspaceCoordinator.restore(worktree:)`, iterate the restored layout's `.chat` tabs and call `agentActivity.registerAgentId(paneId: tab.id.rawValue, agentId:)` with the agent id resolved per D1. This is the universal counterpart of `AppModel.swift:464-467`.
- [ ] Verify: `Scripts/ci.sh` → `CI OK`. **Commit:** `feat: route chat entry points through the universal workspace engine`

---

## Phase 4 — Terminal agent-session restore (D4 — carried, not chat)

### Task 4.1 — Universal `restoreAgentSessions`

- [ ] Test: `@Test func agentSessionRefsForPanesMissingFromTheUniversalLayoutArePruned() async throws { }`
- [ ] Compute `paneIds` from the universal layout instead of `restoredTabs.flatMap(\.leafIds)`: iterate `layouts[worktree.id]?.allTabs`, keep `.terminal(contentID)`, resolve via `workspaceCoordinator.liveControlPaneId(contentID:in:)`.
- [ ] **Ordering hazard:** `restoreAgentSessions` must run *after* `workspaceCoordinator.restore(worktree:)`, not before as the legacy call site does — the layout does not exist yet at line 468. This inverts the current order inside `restoreWorktree`, and the pruning branch (`guard paneIds.contains(ref.paneId)`) will delete every ref if it runs against an empty layout. This is the single highest-risk step of the plan: getting it wrong silently discards users' agent session refs on first launch.
- [ ] Verify: `Scripts/ci.sh` → `CI OK`. **Commit:** `fix: restore agent sessions from the universal workspace layout`

---

## Phase 5 — Parity verification

### Task 5.1 — Automated parity sweep

- [ ] One test per legacy chat behaviour that has no universal test yet, added to a single `UniversalChatParityTests` suite:

```swift
@Test func autoRenameRetitlesAUniversalChatTab() async throws { }
@Test func closingAUniversalChatTabDeletesAnEmptySessionRow() async throws { }
@Test func closingAUniversalChatTabStopsItsController() async throws { }
@Test func aRestoredChatTabDoesNotStartItsAgentUntilItIsViewed() async throws { }
```

The last one guards LC-3 / D5 and is the one most likely to regress silently: it fails only as "every agent launches at app start", which no other test observes.

### Task 5.2 — Manual acceptance (real hardware, gate on)

- [ ] MC-1 New chat from sidebar `+` → tab appears, agent connects, first turn round-trips.
- [ ] MC-2 Auto-rename replaces "Chat" after the first turn.
- [ ] MC-3 Quit and relaunch → the chat tab restores with its transcript, and the agent does **not** start until the tab is viewed.
- [ ] MC-4 Chat history menu → open an old conversation → transcript renders, tab is detached (no new agent turn fired).
- [ ] MC-5 Same conversation opened twice → focuses the existing tab, does not duplicate.
- [ ] MC-6 Close a chat that never had a turn → its row disappears from the history menu.
- [ ] MC-7 Split a chat tab right → both halves render, neither loses its transcript.
- [ ] MC-8 Drag a chat tab into another pane group → transcript, scroll position, and composer draft survive (LC-1).
- [ ] MC-9 Agents panel shows the chat and its subagent tasks.
- [ ] MC-10 An agent that fails to launch shows "Agent not available", not a blank pane.

---

## Exit criterion

Phase 14 of the multi-pane plan (legacy-engine deletion) unblocks when Phases 1–3 and 5 are green and MC-1…MC-10 pass. Phase 4 is independently required by Phase 14 because the deletion removes its only current call site.

## Deliberately out of scope

- Deleting any legacy chat code, `LegacyWorkspaceTab.chat`, or `WorkspaceEngineGate` — that is Phase 14's job, and doing it here means two risky changes land in one bisect step.
- Redesigning `ChatController` lifecycle, transports, or persistence. This plan is a port: same controller, same store, new host.
- `ProjectStore.saveTabs`/`loadTabs` removal — Phase 14.
