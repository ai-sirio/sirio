# Tiller Glass Sidebar and Native Titlebar Revision Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Restore Tiller’s continuous titlebar/main-pane hierarchy while adding a real dark macOS glass sidebar, retaining Split and Permissions shortcuts in the native titlebar, and restoring SF Pro for the app chrome.

**Architecture:** Keep `AppModel`, `NavigationSplitView`, tab/terminal composition, and `UsageBarView` unchanged. Add one AppKit-backed visual-effect background scoped to the sidebar, remove the opaque sidebar root fill, and move the existing shortcut actions from `TopBarView` into `ContentView`’s native toolbar. Remove the now-unnecessary `TopBarView` and `AppFont` files and restore direct `Font.system` call sites.

**Tech Stack:** Swift 6, SwiftUI, AppKit `NSVisualEffectView`, macOS 15.0, XcodeGen, `xcodebuild`, existing package test suites.

## Global Constraints

- Spec: `docs/superpowers/specs/2026-07-10-opencode-ui-redesign-design.md`.
- The application target is macOS 15.0; `project.yml` already includes every source under `App`, so the new source file needs no project-file change.
- The sidebar uses exactly one `NSVisualEffectView` with `.sidebar` material, `.withinWindow` blending, `.followsWindowActiveState`, and a forced `.darkAqua` appearance, wrapped by a navy `AppTheme.background.opacity(0.86)` overlay.
- The sidebar background must extend from the titlebar through the workspace footer while remaining bound to the resizable `NavigationSplitView` sidebar column. It must never be a fixed-width or rounded boxed panel.
- Main pane and terminal remain opaque `AppTheme.background`; the existing sidebar width limits stay `min: 200`, `ideal: 240`, `max: 400`.
- `model.splitCurrent(.horizontal)` and `model.settingsCategory = .permissions; model.openSettings()` remain the only action implementations. No new `AppModel` state, persistence, route, dependency, or networking work is allowed.
- Remove `TopBarView` and `AppFont` completely; do not leave aliases, compatibility wrappers, or obsolete comments.
- Restore SF Pro with direct `Font.system` calls while preserving size and weight values. Keep the Markdown editor’s existing `TextEditor` source-content font `.system(.body, design: .monospaced)` unchanged; it is not chrome typography.
- `Tiller` has no App test target. Do not fabricate unit-test coverage for SwiftUI wiring. Each task verifies `./Scripts/ci.sh` plus the stated manual UI contract.
- Commit only task-owned source files. Do not stage `.serena/`, the plan file, or unrelated user changes.

---

### Task 1: Introduce one native glass surface for the sidebar

**Files:**
- Create: `App/SidebarMaterialContainer.swift`
- Modify: `App/ContentView.swift:62-65`
- Modify: `App/SidebarView.swift:78`
- Modify: `App/AppTheme.swift:5-8`

**Interfaces:**
- Produces: `SidebarMaterialContainer: View`, which composes one private `NSViewRepresentable` material view and its navy tint as the background of a SwiftUI sidebar.
- Consumes: existing `SidebarView(model:)` and `AppTheme.background`.
- Does not expose application data or actions.

- [ ] **Step 1: Add the single visual-effect bridge**

Create `App/SidebarMaterialContainer.swift` with exactly this implementation:

```swift
import SwiftUI
import AppKit

/// One native dark-glass surface, corrected to Tiller's navy palette.
struct SidebarMaterialContainer: View {
    var body: some View {
        SidebarMaterialView()
            .overlay(AppTheme.background.opacity(0.86))
    }
}

private struct SidebarMaterialView: NSViewRepresentable {
    func makeNSView(context: Context) -> NSVisualEffectView {
        let view = NSVisualEffectView()
        view.material = .sidebar
        view.blendingMode = .withinWindow
        view.state = .followsWindowActiveState
        view.appearance = NSAppearance(named: .darkAqua)
        return view
    }

    func updateNSView(_: NSVisualEffectView, context: Context) {}
}
```

- [ ] **Step 2: Place the material behind the existing sidebar once**

In `App/ContentView.swift`, change the workspace sidebar construction to:

```swift
NavigationSplitView {
    SidebarView(model: model)
        .background(SidebarMaterialContainer())
        .navigationSplitViewColumnWidth(min: 200, ideal: 240, max: 400)
} detail: {
```

Do not modify the detail closure, tab strip, terminal stack, drop destination, or usage footer.

- [ ] **Step 3: Make the sidebar root transparent**

Delete this root modifier from `SidebarView.body` in `App/SidebarView.swift`:

```swift
.background(AppTheme.background)
```

Keep the filter, row, selection, hover, and footer divider fills unchanged.

- [ ] **Step 4: Correct the `AppTheme` ownership comment**

Replace the comment immediately above `enum AppTheme` with:

```swift
/// Shared color tokens for Tiller's dark chrome. `background` is the opaque
/// main-pane and terminal surface, and it tints the native sidebar material
/// through `SidebarMaterialContainer`. The remaining tokens style sidebar
/// rows, labels, filter controls, hover, and selection. Agent accent colors
/// live in `AgentIcon`.
```

Keep every `AppTheme` value unchanged.

- [ ] **Step 5: Verify the focused build contract**

Run:

```bash
./Scripts/ci.sh
```

Expected: the command ends with `CI OK`.

- [ ] **Step 6: Run the sidebar smoke check**

Launch the freshly built application:

```bash
open DerivedData/Build/Products/Debug/Tiller.app
```

Verify manually: the sidebar is a contained navy-tinted glass surface, not the system material's gray-green; its blur remains perceptible without exposing readable terminal content. The main pane stays opaque, the vertical split boundary remains subtle, and filter text, project text, hover, selection, footer settings/help, and sidebar resize remain readable and operable.

- [ ] **Step 7: Commit the task-owned files**

Use the required commit workflow to commit only:

```text
App/SidebarMaterialContainer.swift
App/ContentView.swift
App/SidebarView.swift
App/AppTheme.swift
```

Use the Italian Conventional Commit subject:

```text
feat: sidebar in vetro nativa
```

---

### Task 2: Restore a continuous full-height sidebar and native titlebar controls

**Files:**
- Modify: `App/ContentView.swift:26-35,59-85`
- Modify: `App/WindowChromeConfigurator.swift:4-19`
- Modify: `docs/superpowers/specs/2026-07-10-opencode-ui-redesign-design.md`
- Delete: `App/TopBarView.swift`

**Interfaces:**
- Consumes: existing `AppModel.splitCurrent(_:)`, `AppModel.settingsCategory`, `AppModel.openSettings()`, and the `SidebarMaterialContainer` created in Task 1.
- Produces: a workspace-only native toolbar and a full-height, resizable sidebar surface.
- Does not change action resolution, navigation, permissions state semantics, or split-view width rules.

- [ ] **Step 1: Add workspace-only toolbar controls and hide its background**

Insert these modifiers after `.configuresWindowChrome()` in `ContentView.body`:

```swift
.toolbar {
    if model.route == .workspace {
        ToolbarItemGroup(placement: .primaryAction) {
            Button {
                model.splitCurrent(.horizontal)
            } label: {
                Image(systemName: "square.split.1x2")
            }
            .help("Split terminale")
            .accessibilityLabel("Split terminale")

            Button {
                model.settingsCategory = .permissions
                model.openSettings()
            } label: {
                Image(systemName: "lock.shield")
            }
            .help("Permessi")
            .accessibilityLabel("Permessi")
        }
    }
}
.toolbarBackgroundVisibility(.hidden, for: .windowToolbar)
```

Do not apply `.buttonStyle(.plain)` or a custom foreground color: toolbar controls must retain native macOS treatment.

- [ ] **Step 2: Let the resizable sidebar material cover the vertical safe area**

Change the workspace opening from:

```swift
VStack(spacing: 0) {
    TopBarView(model: model)
    NavigationSplitView {
        SidebarView(model: model)
            .background(SidebarMaterialContainer())
```

to:

```swift
VStack(spacing: 0) {
    NavigationSplitView {
        SidebarView(model: model)
            .background(
                SidebarMaterialContainer()
                    .ignoresSafeArea(.container, edges: .vertical)
            )
```

Keep `.navigationSplitViewColumnWidth(min: 200, ideal: 240, max: 400)` and all detail/usage-footer code unchanged. This relies on the sidebar’s live native layout width; do not add a geometry reader, preference key, fixed-width overlay, rounded clipping, or a background on the whole `NavigationSplitView`.

- [ ] **Step 3: Enable full-size content underneath the transparent titlebar**

Replace the body of `WindowChromeConfigurator.makeNSView(context:)` with:

```swift
func makeNSView(context: Context) -> NSView {
    let view = NSView()
    DispatchQueue.main.async {
        guard let window = view.window else { return }
        window.styleMask.insert(.fullSizeContentView)
        window.titlebarAppearsTransparent = true
        window.backgroundColor = NSColor(AppTheme.background)
    }
    return view
}
```

Replace the preceding comment with:

```swift
/// Makes the titlebar transparent and lets split-view backgrounds extend under
/// it. The window itself stays on `AppTheme.background`; the sidebar supplies
/// its own full-height navy-tinted material through its native column.
```

- [ ] **Step 4: Delete the obsolete top-bar source**

Remove `App/TopBarView.swift` entirely.

- [ ] **Step 5: Record the full-height requirement in the design spec**

Keep the user-approved design language that requires a sidebar extending from
the titlebar through the workspace footer, attached to the resizable sidebar
column rather than rendered as a rounded box. Include the existing navy-tint
decision; do not revise unrelated design sections.

- [ ] **Step 6: Verify the focused build contract**

Run:

```bash
./Scripts/ci.sh
```

Expected: the command ends with `CI OK`.

- [ ] **Step 7: Run the titlebar and full-height sidebar smoke check**

Launch the current debug build:

```bash
open DerivedData/Build/Products/Debug/Tiller.app
```

Verify manually: no horizontal bar appears below the titlebar; the navy-tinted
sidebar starts behind the traffic lights, reaches the workspace footer, and
has no rounded/card-like perimeter; its width tracks sidebar resize; main pane
and terminal stay opaque; Split and Permessi are right-aligned native titlebar
controls only in the workspace; Add Project remains in the sidebar toolbar;
Split creates a horizontal terminal split with an active terminal; and
Permessi opens Settings with the permissions category selected.

- [ ] **Step 8: Commit the task-owned files**

Use the required commit workflow to commit only:

```text
App/ContentView.swift
App/WindowChromeConfigurator.swift
App/TopBarView.swift
docs/superpowers/specs/2026-07-10-opencode-ui-redesign-design.md
```

Use the Italian Conventional Commit subject:

```text
feat: sidebar continua e scorciatoie native
```

---

### Task 3: Restore SF Pro chrome and remove the monospace helper

**Files:**
- Delete: `App/AppFont.swift`
- Modify: `App/AddProjectSheet.swift`
- Modify: `App/AgentIcon.swift`
- Modify: `App/MarkdownEditor/MarkdownEditorTabView.swift`
- Modify: `App/MarkdownEditor/MarkdownToolbar.swift`
- Modify: `App/ProjectSettingsSheet.swift`
- Modify: `App/SidebarView.swift`
- Modify: `App/TabBarView.swift`
- Modify: `App/UsageBarView.swift`

**Interfaces:**
- Removes: `AppFont.system(size:weight:)`.
- Produces: direct native SF Pro `Font.system` expressions with unchanged size and weight arguments.
- Preserves: the Markdown `TextEditor` source-content font at `App/MarkdownEditor/MarkdownEditorTabView.swift:36`.

- [ ] **Step 1: Restore direct system-font call sites**

In every file listed above, replace the exact expression prefix:

```swift
.font(AppFont.system(
```

with:

```swift
.font(.system(
```

This replacement keeps every existing `size:` and `weight:` argument exactly as it is. It applies to the UI chrome call sites in the listed files only.

- [ ] **Step 2: Restore the two pre-existing chrome metadata labels to SF Pro**

In `App/AddProjectSheet.swift`, change:

```swift
.font(.system(size: 11, design: .monospaced))
```

to:

```swift
.font(.system(size: 11))
```

In `App/SidebarView.swift`, make the identical replacement for the worktree relative-age label.

Do not change this Markdown source-editor declaration:

```swift
.font(.system(.body, design: .monospaced))
```

It styles editable Markdown content, not app chrome.

- [ ] **Step 3: Delete the now-empty abstraction**

Remove `App/AppFont.swift` entirely after no call site refers to `AppFont`.

- [ ] **Step 4: Verify typography cleanup and build**

Confirm the source has no `AppFont` reference and no remaining chrome-level
`design: .monospaced` declaration; the only deliberate monospaced declaration
is the Markdown `TextEditor` in `MarkdownEditorTabView.swift`. Then run:

```bash
./Scripts/ci.sh
```

Expected: the command ends with `CI OK`.

- [ ] **Step 5: Run the complete visual acceptance check**

Launch the debug application:

```bash
open DerivedData/Build/Products/Debug/Tiller.app
```

Verify manually in one session:

1. App title, toolbar, filter, project/worktree rows, tabs, usage footer, and sheets render with proportional SF Pro.
2. The Markdown editor continues to render source text monospaced.
3. The dark glass sidebar remains readable while the main pane and terminal stay opaque.
4. Window resize and inactive-window states do not cause blur bleed, clipped controls, or illegible selected/hovered rows.
5. Empty state, Markdown drop, context menus, footer branch/path, Split, and Permessi still behave as before.

- [ ] **Step 6: Commit the task-owned files**

Use the required commit workflow to commit only:

```text
App/AppFont.swift
App/AddProjectSheet.swift
App/AgentIcon.swift
App/MarkdownEditor/MarkdownEditorTabView.swift
App/MarkdownEditor/MarkdownToolbar.swift
App/ProjectSettingsSheet.swift
App/SidebarView.swift
App/TabBarView.swift
App/UsageBarView.swift
```

Use the Italian Conventional Commit subject:

```text
refactor: ripristina SF Pro nella chrome
```

## Final Acceptance Criteria

- There is no `TopBarView`, `AppFont`, separate workspace top bar, or obsolete comment claiming the sidebar shares the opaque main background.
- Split and Permessi retain their exact existing `AppModel` entry points and are discoverable in the native titlebar only while in the workspace.
- The sidebar uses one native dark visual-effect surface with a `0.86` navy tint, starts behind the traffic lights, reaches the workspace footer, tracks native sidebar resize, and has no rounded/card-like perimeter or gray-green system material.
- SF Pro is used for the app chrome; the Markdown source editor remains the sole intentional monospaced content surface.
- The usage footer preserves its selected-worktree branch/path context.
- `./Scripts/ci.sh` ends with `CI OK`, and the manual visual contract passes.
