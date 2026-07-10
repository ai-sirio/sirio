# Sidebar Tree Navigation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the tab bar with 3-level tree navigation in the sidebar (Project → Worktree → Tab), give the titlebar the terminal's background color, and add a sidebar toggle.

**Architecture:** View-layer only ("minimal rewire"). AppModel data (`projects`, `worktrees[projectId]`, `tabs[worktreeId]`) already expresses the tree; selecting a tree node calls the existing `selectedWorktree` + `activateTab` APIs. No schema migration, no AppModel restructuring.

**Tech Stack:** Swift 6, SwiftUI, macOS 15.0 deployment target (macOS 26 glass via `#available` in GlassStyle.swift), XcodeGen (`project.yml`), verify via `./Scripts/ci.sh`.

## Global Constraints

- Spec: `docs/superpowers/specs/2026-07-10-sidebar-tree-navigation-design.md`
- Persistence schema v7 untouched — no GRDB migration.
- AppModel: no structural changes; only existing APIs consumed.
- UsageBarView untouched.
- All `AppTheme` color tokens unchanged.
- UI copy in Italian, matching existing strings ("Nuovo Terminale", "Chiudi", "Rinomina").
- Every task ends with `./Scripts/ci.sh` finishing with `CI OK` before commit.
- 219 TillerCore tests must stay green (ci.sh runs them).

---

### Task 1: TabRow tree nodes in the sidebar

**Files:**
- Modify: `App/SidebarView.swift`

**Interfaces:**
- Consumes (all existing on `AppModel`): `tabs: [UUID: [WorkspaceTab]]`, `activeTab(for: UUID) -> WorkspaceTab?`, `activateTab(_: UUID, in: UUID)`, `closeTab(_: UUID, in: Worktree)`, `renameTab(_: UUID, in: UUID, to: String)`, `newShellTab(in: Worktree)`, `selectedWorktree: Worktree?`, `markdownDocuments: [UUID: MarkdownDocument]`, `agentActivity.paneAgents: [UUID: String]`. From `WorkspaceTab`: `title`, `markdownFileURL`, `leafIds`.
- Produces: private `TabRow` view in `SidebarView.swift`; tree rendering under each `WorktreeRow`. Task 2 relies on the tree being fully operable so the tab bar can be deleted.

- [ ] **Step 1: Render tab nodes under each worktree**

In `App/SidebarView.swift`, inside the `ForEach` over sorted worktrees (currently `WorktreeRow(...).contextMenu { ... }`), append the tab nodes right after the row. Replace the existing block:

```swift
ForEach(AttentionSort.sorted(model.worktrees[project.id] ?? [], statusOf: model.statusForWorktree)) { worktree in
    WorktreeRow(model: model, worktree: worktree)
        .contextMenu {
            Button("Nuovo Terminale") {
                model.newShellTab(in: worktree)
            }
            ForEach(AgentCatalog.all, id: \.id) { adapter in
                Button("New \(adapter.displayName) Panel") {
                    Task { await model.spawnAgent(adapter, in: worktree) }
                }
            }
            Divider()
            Button(worktree.isPrimary ? "Unset Primary" : "Set Primary") {
                Task { await model.setPrimary(worktree) }
            }
            Button("Remove Worktree", role: .destructive) {
                Task { await model.removeWorktree(worktree) }
            }
        }

    ForEach(model.tabs[worktree.id] ?? []) { tab in
        TabRow(model: model, worktree: worktree, tab: tab)
    }
}
```

(The only context-menu change is the new "Nuovo Terminale" button at the top; the agent/primary/remove entries are the existing ones.)

Also extend the existing animation modifiers so tab insertion/removal animates — after the two existing `.animation(...)` lines add:

```swift
.animation(.easeInOut(duration: 0.18), value: model.tabs.mapValues { $0.map(\.id) })
```

- [ ] **Step 2: Add the TabRow component**

Add at the bottom of `App/SidebarView.swift` (after `NewWorktreeButton`), following the flat row style of `WorktreeRow`:

```swift
/// Nodo tab del tree in sidebar: icona (agente / terminale / markdown),
/// titolo, dirty dot per markdown, × in hover, rename inline su doppio click.
/// Indentato sotto la WorktreeRow del proprio worktree.
private struct TabRow: View {
    @Bindable var model: AppModel
    let worktree: Worktree
    let tab: WorkspaceTab
    @State private var hovering = false
    @State private var renaming = false
    @State private var draftTitle = ""
    @FocusState private var renameFieldFocused: Bool

    private var isSelected: Bool {
        model.selectedWorktree?.id == worktree.id
            && model.activeTab(for: worktree.id)?.id == tab.id
    }

    var body: some View {
        HStack(spacing: 7) {
            icon
                .frame(width: 14)

            if renaming {
                TextField("", text: $draftTitle)
                    .textFieldStyle(.plain)
                    .font(.system(size: 12))
                    .focused($renameFieldFocused)
                    .onSubmit {
                        model.renameTab(tab.id, in: worktree.id, to: draftTitle)
                        renaming = false
                    }
                    .onExitCommand { renaming = false }
            } else {
                Text(tab.title)
                    .font(.system(size: 12))
                    .foregroundStyle(isSelected ? AppTheme.titleSelected : AppTheme.subtitle)
                    .lineLimit(1)
                    .truncationMode(.tail)
                if model.markdownDocuments[tab.id]?.isDirty == true {
                    Circle().fill(.secondary).frame(width: 5, height: 5)
                }
            }

            Spacer(minLength: 4)

            if hovering && !renaming {
                Button {
                    model.closeTab(tab.id, in: worktree)
                } label: {
                    Image(systemName: "xmark")
                        .font(.system(size: 9, weight: .bold))
                        .foregroundStyle(AppTheme.meta)
                }
                .buttonStyle(.plain)
                .help("Chiudi tab (⌘W)")
            }
        }
        .padding(.vertical, 4)
        .padding(.horizontal, 9)
        .contentShape(Rectangle())
        .background(rowBackground)
        .padding(.leading, 28)
        .padding(.vertical, 1)
        .focusEffectDisabled()
        .onHover { hovering = $0 }
        .onTapGesture(count: 2) {
            draftTitle = tab.title
            renaming = true
            renameFieldFocused = true
        }
        .onTapGesture {
            model.selectedWorktree = worktree
            model.activateTab(tab.id, in: worktree.id)
        }
        .contextMenu {
            Button("Rinomina") {
                draftTitle = tab.title
                renaming = true
                renameFieldFocused = true
            }
            Button("Chiudi") {
                model.closeTab(tab.id, in: worktree)
            }
        }
    }

    @ViewBuilder private var icon: some View {
        if tab.markdownFileURL != nil {
            Image(systemName: "doc.text")
                .font(.system(size: 10))
                .foregroundStyle(AppTheme.meta)
        } else if let agentId = tab.leafIds.compactMap({ model.agentActivity.paneAgents[$0] }).first {
            AgentIcon(agentId: agentId, size: 12)
        } else {
            Image(systemName: "terminal")
                .font(.system(size: 10))
                .foregroundStyle(AppTheme.meta)
        }
    }

    @ViewBuilder private var rowBackground: some View {
        if isSelected {
            RoundedRectangle(cornerRadius: 7)
                .fill(AppTheme.selectionFill)
                .overlay(
                    RoundedRectangle(cornerRadius: 7)
                        .stroke(AppTheme.selectionRing, lineWidth: 1)
                )
        } else if hovering {
            RoundedRectangle(cornerRadius: 7).fill(AppTheme.rowHover)
        } else {
            Color.clear
        }
    }
}
```

`AgentIcon` lives in `App/AgentIcon.swift` and is already used from `TabBarView` with the same `AgentIcon(agentId:size:)` signature.

- [ ] **Step 3: Add hover "+" to WorktreeRow**

In `WorktreeRow` (same file), after `Spacer(minLength: 4)` and before the `runningAgentIds` block, insert:

```swift
if hovering {
    Button {
        model.newShellTab(in: worktree)
    } label: {
        Image(systemName: "plus")
            .font(.system(size: 11))
            .foregroundStyle(AppTheme.meta)
    }
    .buttonStyle(.plain)
    .help("Nuovo terminale (⌘T)")
}
```

- [ ] **Step 4: Verify build and tests**

Run:

```bash
./Scripts/ci.sh
```

Expected: ends with `CI OK`.

- [ ] **Step 5: Smoke check the tree**

```bash
open DerivedData/Build/Products/Debug/Tiller.app
```

Verify manually: tab nodes appear indented under every worktree of an expanded project (not just the selected one); clicking a node switches worktree + tab in the main area; ×/rename/context menu work; hovering a worktree shows "+" and it creates "Terminale N"; markdown tabs show `doc.text` icon and dirty dot when edited. The old tab bar still exists and stays in sync — that's expected until Task 2.

- [ ] **Step 6: Commit**

```bash
git add App/SidebarView.swift
git commit -m "feat: tab nodes as sidebar tree under worktrees"
```

---

### Task 2: Remove the tab bar

**Files:**
- Modify: `App/ContentView.swift` (workspaceView, ~line 97-100)
- Delete: `App/TabBarView.swift`
- Modify: `App/GlassStyle.swift` (remove orphaned `tillerTabBackground`)

**Interfaces:**
- Consumes: Task 1's tree (now the only tab navigation).
- Produces: right column = `terminalStack` + `UsageBarView` only. Task 3 restyles this column's top.

- [ ] **Step 1: Remove TabBarView from ContentView**

In `App/ContentView.swift`, `workspaceView`, delete these lines from the right-column `VStack`:

```swift
if let selected = model.selectedWorktree {
    TabBarView(model: model, worktree: selected)
}
```

- [ ] **Step 2: Delete TabBarView.swift**

```bash
git rm App/TabBarView.swift
```

- [ ] **Step 3: Remove the orphaned tab background helper**

`tillerTabBackground(isActive:)` in `App/GlassStyle.swift` was only used by `TabBarView` (verify: `grep -rn "tillerTabBackground" App/` must return only GlassStyle.swift). Delete the whole `tillerTabBackground` function (the `/// Sfondo della tab...` doc comment through its closing brace). Keep `tillerGlass`, `tillerGlassButtonStyle`, and `TillerGlassContainer` — still used elsewhere.

Check `TillerGlassContainer` usage too:

```bash
grep -rn "TillerGlassContainer" App/
```

If the only remaining use was TabBarView, delete `TillerGlassContainer` as well; if other files use it, keep it.

- [ ] **Step 4: Verify build and tests**

```bash
./Scripts/ci.sh
```

Expected: `CI OK`. A compile error about `TabBarView` means a stale reference — search `grep -rn "TabBarView" App/` and remove leftovers.

- [ ] **Step 5: Smoke check**

```bash
open DerivedData/Build/Products/Debug/Tiller.app
```

Verify: no tab strip above the terminal; tree navigation still switches terminals; ⌘T creates a tab (visible in tree), ⌘W closes the active one; dropping a `.md` file on the terminal area adds a markdown node to the tree and opens the editor.

- [ ] **Step 6: Commit**

```bash
git add -A App/
git commit -m "feat: remove tab bar, tree is the only tab navigation"
```

---

### Task 3: Titlebar in terminal color with centered title

**Files:**
- Modify: `App/ContentView.swift`

**Interfaces:**
- Consumes: `model.selectedWorktree` (`branch`, `projectId`), `model.projects` (`displayName`, `name`).
- Produces: titlebar zone above the right column rendered in `AppTheme.background` with a centered "branch ⋅ project" title. Task 4 adds the leading toggle button to the same toolbar.

- [ ] **Step 1: Extend the terminal background under the titlebar**

In `workspaceView`, change the right-column background from:

```swift
.background(AppTheme.background)
```

to:

```swift
.background(AppTheme.background.ignoresSafeArea(edges: .top))
```

Content keeps respecting the safe area (terminal not under the titlebar); only the color extends up. The sidebar keeps its full-height glass (`SidebarMaterialContainer` already `.ignoresSafeArea()`).

- [ ] **Step 2: Add the centered title**

In `ContentView`, inside the existing `.toolbar { }` block (which already has the `if model.route == .workspace` guard), add a principal item before the `ToolbarItemGroup(placement: .primaryAction)`:

```swift
ToolbarItem(placement: .principal) {
    if let worktree = model.selectedWorktree {
        HStack(spacing: 5) {
            Text(worktree.branch)
                .font(.system(size: 12, weight: .semibold))
                .foregroundStyle(AppTheme.title)
            if let project = model.projects.first(where: { $0.id == worktree.projectId }) {
                Text("⋅ \((project.displayName?.isEmpty == false ? project.displayName : nil) ?? project.name)")
                    .font(.system(size: 12))
                    .foregroundStyle(AppTheme.meta)
            }
        }
    }
}
```

No selected worktree → no title (empty-state view already explains the situation).

- [ ] **Step 3: Verify build and tests**

```bash
./Scripts/ci.sh
```

Expected: `CI OK`.

- [ ] **Step 4: Smoke check**

```bash
open DerivedData/Build/Products/Debug/Tiller.app
```

Verify: the strip above the terminal is the terminal's own color (no material seam); "branch ⋅ project" sits centered in the titlebar; split + permissions buttons remain trailing; sidebar glass still reaches the top of the window on its side.

- [ ] **Step 5: Commit**

```bash
git add App/ContentView.swift
git commit -m "feat: terminal-colored titlebar with centered worktree title"
```

---

### Task 4: Sidebar toggle

**Files:**
- Modify: `App/ContentView.swift`
- Modify: `App/TillerApp.swift`

**Interfaces:**
- Consumes: Task 3's toolbar.
- Produces: `@AppStorage("sidebar.visible")` boolean read by both files; ⌃⌘S menu command "Nascondi/Mostra Sidebar".

- [ ] **Step 1: Conditional sidebar in ContentView**

In `App/ContentView.swift` add the storage property next to the other `@AppStorage` lines:

```swift
@AppStorage("sidebar.visible") private var sidebarVisible = true
```

In `workspaceView`, wrap the sidebar column:

```swift
HSplitView {
    if sidebarVisible {
        SidebarView(model: model)
            .frame(minWidth: 200, idealWidth: 240, maxWidth: 400, maxHeight: .infinity)
            .background(
                SidebarMaterialContainer()
                    .ignoresSafeArea()
            )
            .onGeometryChange(for: CGFloat.self) { $0.size.width } action: {
                sidebarWidth = $0
            }
    }
    // ... right column unchanged
}
```

Gate the divider-covering overlay on visibility (a hidden sidebar has no seam to cover) — change the existing `.overlay(alignment: .leading) { ... }` content to:

```swift
.overlay(alignment: .leading) {
    if sidebarVisible {
        SidebarMaterialContainer()
            .frame(width: 2)
            .offset(x: sidebarWidth)
            .ignoresSafeArea()
            .allowsHitTesting(false)
    }
}
```

And animate the collapse by adding, after that overlay:

```swift
.animation(.easeInOut(duration: 0.2), value: sidebarVisible)
```

- [ ] **Step 2: Leading toolbar button**

In `ContentView`'s `.toolbar` block, inside the workspace guard, add before the principal item:

```swift
ToolbarItem(placement: .navigation) {
    Button {
        sidebarVisible.toggle()
    } label: {
        Image(systemName: "sidebar.left")
    }
    .help(sidebarVisible ? "Nascondi Sidebar (⌃⌘S)" : "Mostra Sidebar (⌃⌘S)")
    .accessibilityLabel("Sidebar")
}
```

- [ ] **Step 3: Menu command in TillerApp**

In `App/TillerApp.swift` add the same storage property to the `TillerApp` struct:

```swift
@AppStorage("sidebar.visible") private var sidebarVisible = true
```

and inside `.commands { }` add a new group:

```swift
CommandGroup(after: .sidebar) {
    Button(sidebarVisible ? "Nascondi Sidebar" : "Mostra Sidebar") {
        sidebarVisible.toggle()
    }
    .keyboardShortcut("s", modifiers: [.control, .command])
}
```

- [ ] **Step 4: Verify build and tests**

```bash
./Scripts/ci.sh
```

Expected: `CI OK`.

- [ ] **Step 5: Smoke check**

```bash
open DerivedData/Build/Products/Debug/Tiller.app
```

Verify: toolbar button (leading, near traffic lights) and ⌃⌘S both toggle the sidebar with animation; terminal column expands full-width when hidden; no 2px seam artifact remains when hidden; quit and relaunch — hidden state persists; menu View shows "Nascondi/Mostra Sidebar" with correct dynamic label.

- [ ] **Step 6: Commit**

```bash
git add App/ContentView.swift App/TillerApp.swift
git commit -m "feat: sidebar toggle with persistence and ⌃⌘S shortcut"
```

---

### Task 5: Full regression pass

**Files:**
- None (verification only).

**Interfaces:**
- Consumes: everything above.
- Produces: green build, green tests, verified manual checklist.

- [ ] **Step 1: CI**

```bash
./Scripts/ci.sh
```

Expected: `CI OK` (includes the 219 TillerCore tests — all must pass, unchanged).

- [ ] **Step 2: Manual checklist**

```bash
open DerivedData/Build/Products/Debug/Tiller.app
```

- Tree navigates between terminals of different worktrees and different projects.
- × / hover "+" / double-click rename / context menus on tab nodes work.
- Markdown: node opens editor, dirty dot appears on edit, ⌘S saves, drop `.md` on terminal area creates node.
- ⌘T / ⌘W operate on the selected worktree's tabs.
- Terminal split (toolbar button) still works inside the active tab.
- Sidebar toggle: button + ⌃⌘S, persistence across relaunch.
- Titlebar: terminal color, centered "branch ⋅ project".
- Usage bar unchanged at the bottom.
- Scrollback survives switching tabs via tree (panes stay alive — they were already kept alive across tab switches by the opacity/hitTesting mechanism in `terminalStack`, which is untouched).

- [ ] **Step 3: Commit any checklist fixes**

If fixes were needed, commit them individually with `fix:` messages; otherwise nothing to commit.
