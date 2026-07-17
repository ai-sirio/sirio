# Empty Worktree Lifecycle — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (- [ ]) syntax for tracking.

## Goal

Allow a worktree to have zero tabs. When the last tab is closed, the worktree becomes empty (no PTY, no scrollback, no pane cache) but persists in the model, survives relaunch, and shows an empty state in the terminal area. The user creates a new tab explicitly via the "New Terminal" button or ⌘T.

## Architecture

The core change relaxes the invariant "every worktree always has ≥ 1 tab" to "every worktree may have 0 or more tabs." This touches `AppModel.closeTab`, `selectedWorktree.didSet`, `bootstrap`, and `ContentView.terminalStack`. All other callers already handle empty tab lists safely.

## Tech Stack

- Swift 6, macOS 15+
- `swift-testing` (`@Test` / `#expect`)
- `Scripts/ci.sh` as verification gate
- No new dependencies

## Global Constraints

- Never auto-create a tab. The user must explicitly request a new terminal.
- Empty worktrees must persist across quit/relaunch.
- Empty worktrees must have no PTY process, no scrollback buffer, no pane cache, no PaneRegistry entry.
- All existing tests must pass unchanged.
- `Scripts/ci.sh` must print `CI OK`.

---

## Interfaces

### Consumed

```swift
// AppModel (App/AppModel.swift)
func closeTab(_ tabId: UUID, in worktree: Worktree)  // line 639
func ensureTabs(for worktree: Worktree)                // line 598
func bootstrap() async                                 // line 183
var openWorktreeIds: [UUID]                            // line 62
var tabs: [UUID: [WorkspaceTab]]                       // line 152
var activeTabId: [UUID: UUID]                           // line 166
func activeTab(for worktreeId: UUID) -> WorkspaceTab?   // line 610
func newShellTab(in worktree: Worktree)                // line 626
func newShellTabInSelected()                           // line 631
func persistTabs(for worktreeId: UUID)                 // line 783
func liveLeafIds(for worktreeId: UUID) -> Set<UUID>    // line 732
func worktree(byId id: UUID) -> Worktree?              // line 68

// ContentView (App/ContentView.swift)
var terminalStack: some View                           // line 225

// ProjectStore (TillerCore)
func saveTabs(worktreeId: UUID, tabs: [WorkspaceTab], activeTabId: UUID?) throws  // line 198
func loadTabs(of worktreeId: UUID) throws -> (tabs: [WorkspaceTab], activeTabId: UUID?)  // line 231
```

### Produced

```swift
// AppModel changes:
// - closeTab: remove replacement-tab creation (lines 649-654)
// - selectedWorktree.didSet: remove ensureTabs call (line 32)
// - bootstrap: remove empty-tab guard (line 212); include empty worktrees in openWorktreeIds (line 225)
// - ensureTabs: keep as public API but no longer called from didSet

// ContentView changes:
// - terminalStack: add EmptyWorktreeView for selected worktree with no tabs

// New view:
// - EmptyWorktreeView: minimal view with worktree name, branch, "New Terminal" button
```

---

## Tasks

### Task 1: Add pure-model tests for empty worktree lifecycle

**Files:**
- `Packages/TillerCore/Tests/TillerCoreTests/ProjectStoreTests.swift` (append)
- `Packages/TillerCore/Tests/TillerCoreTests/ScrollbackFlushTests.swift` (append)
- `Packages/TillerCore/Tests/TillerCoreTests/WorktreeMountPolicyTests.swift` (append)

**Step 1.1 — Test: save and load empty tab list**

Add to `ProjectStoreTests.swift`:

```swift
@Test func saveAndLoadEmptyTabList() throws {
    let db = try AppDatabase.inMemory()
    let store = ProjectStore(database: db)
    let project = try store.addProject(name: "p", rootPath: "/tmp/p")
    let worktree = try store.addWorktree(projectId: project.id, branch: "main", path: "/tmp/p")
    try store.saveTabs(worktreeId: worktree.id, tabs: [], activeTabId: nil)
    let loaded = try store.loadTabs(of: worktree.id)
    #expect(loaded.tabs.isEmpty)
    #expect(loaded.activeTabId == nil)
}
```

**Step 1.2 — Test: flush targets for empty tab list yields nothing**

Add to `ScrollbackFlushTests.swift`:

```swift
@Test func flushTargetsEmptyTabListYieldsNothing() {
    let wt = UUID()
    let tabs = [wt: [WorkspaceTab]()]
    let targets = scrollbackFlushTargets(tabs: tabs)
    #expect(targets.isEmpty)
}
```

**Step 1.3 — Test: mount policy evicts first idle worktree when over cap**

Add to `WorktreeMountPolicyTests.swift`:

```swift
@Test func evictsFirstIdleWorktreeWhenOverCap() {
    let a = UUID()
    let b = UUID()
    let evicted = WorktreeMountPolicy.idsToEvict(
        openWorktreeIds: [a, b],
        selectedWorktreeId: nil,
        cap: 1,
        status: { _ in nil },
        hasUnsavedWork: { _ in false }
    )
    #expect(evicted == [a])
}
```

**Verification:**
```bash
cd Packages/TillerCore && swift test --filter saveAndLoadEmptyTabList
cd Packages/TillerCore && swift test --filter flushTargetsEmptyTabListYieldsNothing
cd Packages/TillerCore && swift test --filter evictsFirstIdleWorktreeWhenOverCap
```
Expected: all three pass.

---

### Task 2: Remove replacement-tab creation from `closeTab`

**File:** `App/AppModel.swift`, lines 649-654

**Current:**
```swift
if list.isEmpty {
    list = [WorkspaceTab(
        id: UUID(),
        title: WorkspaceTab.nextShellTitle(existing: []),
        tree: .leaf(id: UUID())
    )]
}
```

**Change:** Remove the entire `if list.isEmpty { ... }` block. When `list.isEmpty`, the worktree becomes empty. The subsequent lines (656-660) already handle empty lists correctly:

```swift
tabs[worktree.id] = list          // can be []
if !list.contains(where: { $0.id == activeTabId[worktree.id] }) {
    activeTabId[worktree.id] = list.last?.id   // nil when empty
}
persistTabs(for: worktree.id)
```

**Edit:**
```
oldString:         if list.isEmpty {
            list = [WorkspaceTab(
                id: UUID(),
                title: WorkspaceTab.nextShellTitle(existing: []),
                tree: .leaf(id: UUID())
            )]
        }
newString:         // Allow empty tab list — the worktree can have zero tabs.
        // The user creates a new tab via ⌘T or the sidebar "+" menu.
```

**Verification:**
```bash
Scripts/ci.sh
```
Expected: `CI OK`.

---

### Task 3: Remove `ensureTabs` call from `selectedWorktree.didSet`

> **Cross-plan note:** This task and `docs/superpowers/plans/2026-07-17-performance-measurement-plan.md` Task 8 both edit `AppModel.selectedWorktree.didSet`. Phase 1 changes (this task: remove `ensureTabs`) must be applied **before** Phase 2 wraps the body in signpost intervals (Task 8). Apply in order: first this task, then Task 8.

**File:** `App/AppModel.swift`, line 32

**Current:**
```swift
if !openWorktreeIds.contains(worktree.id) { openWorktreeIds.append(worktree.id) }
ensureTabs(for: worktree)
```

**Change:** Remove the `ensureTabs(for: worktree)` call. The worktree is added to `openWorktreeIds` (so its host mounts) but no tab is auto-created.

**Edit:**
```
oldString:         if !openWorktreeIds.contains(worktree.id) { openWorktreeIds.append(worktree.id) }
            ensureTabs(for: worktree)
newString:         if !openWorktreeIds.contains(worktree.id) { openWorktreeIds.append(worktree.id) }
            // No longer auto-creates a tab. The worktree mounts with zero tabs.
            // The user opens a tab explicitly via ⌘T or the sidebar "+" menu.
```

**Verification:**
```bash
Scripts/ci.sh
```
Expected: `CI OK`.

---

### Task 4: Update `bootstrap` to accept empty tab lists

**File:** `App/AppModel.swift`, lines 212 and 225

**Change 4.1 — Remove empty-tab guard (line 212):**

**Current:**
```swift
guard !restoredTabs.isEmpty else { continue }
tabs[worktree.id] = restoredTabs
activeTabId[worktree.id] = loaded.activeTabId.flatMap { active in
    restoredTabs.contains { $0.id == active } ? active : nil
} ?? restoredTabs.first?.id
```

**Edit:**
```
oldString:                     guard !restoredTabs.isEmpty else { continue }
                    tabs[worktree.id] = restoredTabs
                    activeTabId[worktree.id] = loaded.activeTabId.flatMap { active in
                        restoredTabs.contains { $0.id == active } ? active : nil
                    } ?? restoredTabs.first?.id
newString:                     tabs[worktree.id] = restoredTabs
                    activeTabId[worktree.id] = loaded.activeTabId.flatMap { active in
                        restoredTabs.contains { $0.id == active } ? active : nil
                    } ?? restoredTabs.first?.id
```

**Change 4.2 — Include empty worktrees in `openWorktreeIds` (line 225):**

**Current:**
```swift
openWorktreeIds = storedOpenIds.filter { !(tabs[$0] ?? []).isEmpty }
```

**Edit:**
```
oldString:             openWorktreeIds = storedOpenIds.filter { !(tabs[$0] ?? []).isEmpty }
newString:             openWorktreeIds = storedOpenIds.filter { worktree(byId: $0) != nil }
```

**Verification:**
```bash
Scripts/ci.sh
```
Expected: `CI OK`.

---

### Task 5: Add empty state UI for worktree with no tabs

**File:** `App/ContentView.swift`, `terminalStack` property (lines 225-233)

**Current:**
```swift
if model.openWorktreeIds.isEmpty {
    ContentUnavailableView(
        "No worktree selected",
        systemImage: "terminal",
        description: Text("Add a project, then select a worktree.")
    )
}
```

**Change:** Add a second `ContentUnavailableView` for the case where a worktree is selected but has no tabs. This must be placed before the `ForEach(model.openWorktreeIds, ...)` block.

**Edit:**
```
oldString:             if model.openWorktreeIds.isEmpty {
                ContentUnavailableView(
                    "No worktree selected",
                    systemImage: "terminal",
                    description: Text("Add a project, then select a worktree.")
                )
            }
newString:             if model.openWorktreeIds.isEmpty {
                ContentUnavailableView(
                    "No worktree selected",
                    systemImage: "terminal",
                    description: Text("Add a project, then select a worktree.")
                )
            } else if let worktree = model.selectedWorktree,
                      let tabs = model.tabs[worktree.id], tabs.isEmpty {
                EmptyWorktreeView(worktree: worktree, onNewTerminal: {
                    model.newShellTab(in: worktree)
                })
            }
```

**New file:** `App/EmptyWorktreeView.swift`

```swift
import SwiftUI
import TillerCore

struct EmptyWorktreeView: View {
    let worktree: Worktree
    let onNewTerminal: () -> Void

    var body: some View {
        ContentUnavailableView {
            Label("No tabs", systemImage: "rectangle.split.1x2")
        } description: {
            VStack(spacing: 4) {
                Text(worktree.branch)
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
                Text("Press ⌘T to open a new terminal tab.")
                    .font(.caption)
                    .foregroundStyle(.tertiary)
            }
        } actions: {
            Button("New Terminal", action: onNewTerminal)
                .buttonStyle(.borderedProminent)
                .keyboardShortcut("t", modifiers: .command)
        }
    }
}
```

**Verification:**
```bash
Scripts/ci.sh
```
Expected: `CI OK`.

---

### Task 6: Manual smoke tests

**Step 6.1 — Close last tab:**
1. Open a worktree with one tab.
2. Close the tab (⌘W or close button).
3. Verify: the worktree is still selected in the sidebar, the terminal area shows `EmptyWorktreeView` with "No tabs" and a "New Terminal" button.
4. Verify: `ps aux | grep tiller` shows no shell process for the empty worktree.

**Step 6.2 — Create first tab:**
1. With the empty worktree selected, click "New Terminal" or press ⌘T.
2. Verify: a shell tab appears with a running shell.

**Step 6.3 — Persistence:**
1. Close all tabs in a worktree.
2. Quit Tiller.
3. Relaunch Tiller.
4. Verify: the worktree is present in the sidebar with no tabs. Selecting it shows `EmptyWorktreeView`.

**Step 6.4 — Control socket:**
1. With an empty worktree selected, run `tillerctl panel.create --worktree <id>`.
2. Verify: a new panel tab appears.

**Step 6.5 — Sidebar context menu:**
1. Right-click an empty worktree in the sidebar.
2. Select "Nuovo Terminale".
3. Verify: a new shell tab appears.

---

## Acceptance Criteria

| AC | Verification |
|----|-------------|
| AC9 | Empty worktree persists and restores: close all tabs, quit, relaunch — worktree present with empty state |
| AC10 | "New Terminal" button and keyboard shortcut create first tab |
| AC11 | Empty mounted worktree has no PTY process: `ps aux | grep tiller` — no shell process for empty worktree |
| AC12 | `Scripts/ci.sh` prints `CI OK` |

## Commit Messages

```bash
# After Task 1:
git add Packages/TillerCore/Tests/TillerCoreTests/ProjectStoreTests.swift
git add Packages/TillerCore/Tests/TillerCoreTests/ScrollbackFlushTests.swift
git add Packages/TillerCore/Tests/TillerCoreTests/WorktreeMountPolicyTests.swift
git commit -m "test: add empty-worktree lifecycle tests for persistence, flush, and mount policy"

# After Tasks 2-5:
git add App/AppModel.swift App/ContentView.swift App/EmptyWorktreeView.swift
git commit -m "feat: allow empty worktrees with zero tabs and empty-state UI"

# After Task 6 (manual verification):
git commit --allow-empty -m "chore: verify empty-worktree lifecycle manually"
```
