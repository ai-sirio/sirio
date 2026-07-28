# Native macOS Pane, Drag, and Accessibility Research

**Date:** 2026-07-28

**Status:** research complete; product and architecture decisions remain open

**Ticket:** [Research native macOS pane and drag accessibility conventions](https://github.com/tillerai/tiller/issues/5)

**Map:** [Design the draggable multi-pane workspace](https://github.com/tillerai/tiller/issues/1)

## Question

Which current macOS conventions and APIs should constrain pane splitting, tab drag-and-drop, resizing, focus, keyboard operation, VoiceOver, reduced motion, and visible drop feedback in Tiller?

## Executive findings

1. **The existing AppKit split renderer is a useful foundation, not a complete universal layout engine.** `TerminalSplitHost` already renders a recursive terminal tree with `NSSplitViewController`, preserves terminal controllers across structural rebuilds, and restores first responder. The planned workspace is broader: today only terminal tabs own a `SplitTree`; chat and editor tabs are single-content `WorkspaceTab` values. A heterogeneous pane-group model, local tab stacks, active selection per group, and divider proportions still require explicit domain and persistence decisions.

2. **SwiftUI can own tab dragging and visual drop feedback while AppKit continues to own divider resizing.** Tiller's current in-process `NSItemProvider`/custom-UTType drag is reusable. The closure-based `dropDestination` receives the final drop location, but a `DropDelegate` provides the continuous entered/updated/exited lifecycle needed to preview edge zones before the drop. In `DropDelegate`, `validateDrop` returns `Bool`; `dropUpdated` returns `DropProposal?`.

3. **Keyboard focus and accessibility focus are separate systems.** `FocusState` models ordinary SwiftUI focus; `AccessibilityFocusState` models focus from assistive technologies such as VoiceOver. Terminal focus currently crosses AppKit through `NSWindow.makeFirstResponder`; chat and editor surfaces need their own focus adapters. The design must not assume one property wrapper unifies all three paths automatically.

4. **Pointer dragging cannot be the only way to rearrange panes.** Split, move, close, and focus operations need menu/keyboard commands and accessibility actions with the same underlying mutations. A custom accessibility rotor may improve navigation, but it is an optional enhancement rather than a substitute for commands and actions.

5. **GRDB remains a reasonable storage mechanism, but the current encoded shape is insufficient.** Current persistence stores one row per global tab, one active tab per worktree, and a terminal-only `treeJSON`. It does not store pane groups, local tab order, active tab per group, focused group, or divider proportions. Migration and failure fallback therefore remain real design work.

## Evidence and constraints

### 1. Current Tiller model and renderer

- `TabContent` has four mutually exclusive cases: terminal, Markdown, code, and chat. Only `.terminal` carries a `SplitTree` (`Packages/TillerCore/Sources/TillerCore/WorkspaceTab.swift`).
- `SplitTree` is a recursive binary tree of `UUID` leaves and axis-only split nodes. It stores neither heterogeneous leaf content nor divider proportions (`Packages/TillerCore/Sources/TillerCore/SplitTree.swift`).
- `ContentView.terminalStack` mounts every worktree/tab host in a `ZStack` and makes only the globally active tab visible. Chat and editor content are rendered directly; only terminal content is delegated to `TerminalSplitHost` (`App/ContentView.swift`).
- `TerminalSplitHost` is an `NSViewControllerRepresentable`. It maps split nodes to `NSSplitViewController`, wraps terminal leaves in `NSHostingController`, caches leaf controllers so PTYs survive tree rebuilds, and restores a prior AppKit first responder after reparenting (`Packages/TillerTerminal/Sources/TillerTerminal/SplitViewRenderer.swift`).
- Each terminal split item currently has `minimumThickness = 160`. `EqualSplitViewController` initializes a newly created split at 50/50 with `NSSplitView.setPosition(_:ofDividerAt:)`, but the resulting divider proportion is not represented in `SplitTree`.

**Constraint:** a universal layout may reuse the recursive topology and AppKit renderer techniques, but it cannot simply keep `WorkspaceTab`, `SplitTree`, and persistence unchanged. That decision belongs to [Define pane-group structure and invariants](https://github.com/tillerai/tiller/issues/2), [Choose persistence and migration semantics for pane layouts](https://github.com/tillerai/tiller/issues/7), and [Choose the module boundary for the universal layout engine](https://github.com/tillerai/tiller/issues/8).

### 2. Native split and resize behavior

Apple describes `NSSplitViewController` as the controller for adjacent child views and `NSSplitView` as the view that arranges children and manages dividers. `NSSplitViewItem` supplies per-child sizing and collapse policy.

Primary SDK facts verified against the installed macOS 26.5 SDK:

- `NSSplitViewItem.canCollapse` defaults to `false` for an ordinary split item. Specialized sidebar and inspector constructors can choose different defaults.
- `minimumThickness` and `maximumThickness` default to `NSSplitViewItemUnspecifiedDimension`. Tiller explicitly overrides the minimum to 160 for terminal leaves.
- `holdingPriority` defaults to `NSLayoutPriority.defaultLow`, whose value is 250. Apple's AppKit header states that the item with the lowest priority is first to take additional width as the split view grows or shrinks.
- `NSSplitView.setPosition(_:ofDividerAt:)` is the direct API for programmatic divider placement.

Sources: [NSSplitViewController](https://developer.apple.com/documentation/appkit/nssplitviewcontroller), [NSSplitView](https://developer.apple.com/documentation/appkit/nssplitview), [NSSplitViewItem](https://developer.apple.com/documentation/appkit/nssplitviewitem), [holdingPriority](https://developer.apple.com/documentation/appkit/nssplitviewitem/holdingpriority), plus `AppKit.framework/Headers/NSSplitViewItem.h`, `NSSplitView.h`, and `NSLayoutConstraint.h` in the installed Apple SDK.

**Implications:**

- Keep native divider dragging rather than implementing resize gestures manually.
- Decide collapse behavior explicitly; relying on assumed defaults is unsafe when different `NSSplitViewItem` constructors are introduced.
- Treat active-pane holding priority as a prototype question. Raising it changes which pane absorbs window resize; it is not a universal native convention.
- Persist divider proportions if restoration is expected to reproduce the user's layout. The current axis-only tree cannot do that.

### 3. Drag source and destination APIs

Tiller's current `ReorderableRow` uses `.onDrag`, an `NSItemProvider`, and the exported `it.tiller.row-drag` UTType with `.ownProcess` visibility. `AppModel.draggingRow` holds the in-process identity and `onDrop` commits the previewed order (`App/RowReorder.swift`, `App/TabBarView.swift`, `App/AppModel.swift`). This is an existing implementation choice, not a platform requirement.

For the planned edge-drop interaction, the macOS 15-compatible SwiftUI APIs provide two useful levels:

- `dropDestination(for:action:isTargeted:)` receives `([T], CGPoint) -> Bool`. It can choose an action from the **final** drop location and exposes a Boolean targeted callback, but it does not expose the continuously changing pointer location through that callback.
- `.onDrop(of:delegate:)` delegates the complete lifecycle. The installed SwiftUI interface defines:
  - `validateDrop(info:) -> Bool`
  - `dropEntered(info:)`
  - `dropUpdated(info:) -> DropProposal?`
  - `dropExited(info:)`
  - `performDrop(info:) -> Bool`

`DropInfo.location` can classify center/left/right/top/bottom zones. `dropUpdated` is the correct place to update the proposed `DropOperation` and ephemeral preview state; `performDrop` performs the model mutation. The drop-zone overlay should remain transient view state so pointer movement does not rebuild the split-controller hierarchy.

Sources: [SwiftUI drag and drop](https://developer.apple.com/documentation/swiftui/drag-and-drop), [DropDelegate](https://developer.apple.com/documentation/swiftui/dropdelegate), [DropInfo](https://developer.apple.com/documentation/swiftui/dropinfo), [DropProposal](https://developer.apple.com/documentation/swiftui/dropproposal), [DropOperation](https://developer.apple.com/documentation/swiftui/dropoperation), [dropDestination](https://developer.apple.com/documentation/swiftui/view/dropdestination(for:action:istargeted:)), and the installed `SwiftUI.swiftinterface`.

**Implications:**

- Preserve a private in-process payload for tab moves; do not confuse it with Finder file URLs.
- Use separate validation paths for internal tab moves and external file drops.
- Draw the proposed resulting region in-app; `DropProposal` communicates operation semantics but does not describe Tiller's future split geometry.
- Do not mutate persistent layout during hover. Commit exactly once in `performDrop`, then persist.
- Prototype both `DropDelegate` and the newer closure-based APIs before locking the implementation boundary; the deployment target is macOS 15, so newer macOS 26-only drag-container APIs cannot be required.

Apple's [Drag and Drop HIG](https://developer.apple.com/design/human-interface-guidelines/drag-and-drop) is the primary design reference for feedback and predictable outcomes. This research does not claim a platform-prescribed edge percentage: the zone geometry remains a product decision.

### 4. Focus and keyboard operation

Current Tiller commands cover tab creation/closure, file open/save, global tab cycling, and tab-number selection (`App/TillerApp.swift`). Split and directional pane-focus commands are not present. Terminal focus is handled separately: `AppModel.focusPane` activates the containing tab and retries `TerminalPaneCache.focus`, while `TerminalSplitHost.restoreFocusIfNeeded` repairs first responder after a structural rebuild (`App/AppModel.swift`, `Packages/TillerTerminal/Sources/TillerTerminal/TerminalPaneCache.swift`, `SplitViewRenderer.swift`).

Apple's APIs distinguish:

- `FocusState` — ordinary SwiftUI focus state, available on macOS 12 and later.
- `AccessibilityFocusState` — focus of an active accessibility technology, also available on macOS 12 and later.
- `NSWindow.makeFirstResponder` — AppKit first-responder control.

Sources: [SwiftUI Focus](https://developer.apple.com/documentation/swiftui/focus), [FocusState](https://developer.apple.com/documentation/swiftui/focusstate), [AccessibilityFocusState](https://developer.apple.com/documentation/swiftui/accessibilityfocusstate), [NSWindow.makeFirstResponder](https://developer.apple.com/documentation/appkit/nswindow/makefirstresponder(_:)), and the installed SwiftUI interface.

**Implications:**

- Define one semantic command layer — split, move tab, focus neighboring group, close tab/group — then adapt it to menus, keyboard shortcuts, pointer drops, and accessibility actions.
- Give each workspace item type an explicit focus adapter. Terminal focus may remain AppKit-backed; chat and editors may use SwiftUI focus or their wrapped AppKit text views.
- Restore focus by stable item/group identity after a move. Do not retain only an `NSView` pointer across arbitrary host replacement.
- Choose shortcuts only after checking existing app and system menu conflicts. This research deliberately does not prescribe ⌘D, ⌘⇧D, or directional combinations.

Apple's [Keyboard HIG](https://developer.apple.com/design/human-interface-guidelines/keyboards) remains the primary source for menu discoverability and keyboard alternatives.

### 5. VoiceOver and non-pointer parity

The current central workspace has accessibility labels on some toolbar controls and hides inactive surfaces, but there is no pane-group accessibility model or equivalent non-pointer operation for tab dragging (`App/ContentView.swift`, `App/TabBarView.swift`).

Relevant SwiftUI facilities available to a macOS 15 deployment include:

- named `accessibilityAction` modifiers for split, move, close, and focus alternatives;
- the `accessibilityActions` builder for grouping actions;
- `accessibilityRotor` for optional navigation among stable pane-group entries;
- `AccessibilityFocusState` for assistive-technology focus;
- `accessibilityDragPoint` and `accessibilityDropPoint` if the final interaction benefits from explicit accessible drag endpoints.

Sources: [Accessibility fundamentals](https://developer.apple.com/documentation/swiftui/accessibility-fundamentals), [Accessible controls](https://developer.apple.com/documentation/swiftui/accessible-controls), [Accessible navigation](https://developer.apple.com/documentation/swiftui/accessible-navigation), [AccessibilityFocusState](https://developer.apple.com/documentation/swiftui/accessibilityfocusstate), and the bundled Apple documentation snapshot dated 2026-05-30.

**Requirements for the eventual specification:**

- Every pane group and local tab bar needs a stable, meaningful label and current-state value.
- Every drag-only layout mutation needs an action/menu/keyboard equivalent.
- Focus must move to a deterministic surviving item after close, move, or group collapse.
- Drop feedback cannot rely on color alone; selected zone geometry needs sufficient contrast and an additional shape/border cue.
- VoiceOver behavior must be verified in the running app. Static modifier presence is not acceptance evidence.

A custom “Pane groups” rotor is plausible, but it should be validated with VoiceOver before becoming mandatory. The accessibility of terminal **content** is not established by this research; pane-level controls can be accessible even if the embedded terminal renderer has limitations.

### 6. Reduced motion and visual feedback

SwiftUI exposes `accessibilityReduceMotion`; AppKit exposes `NSWorkspace.accessibilityDisplayShouldReduceMotion`. Tiller currently has several unconditional short animations in `ContentView` and `TabBarView`, but this ticket is scoped to the new pane interaction.

Sources: [Accessible appearance](https://developer.apple.com/documentation/swiftui/accessible-appearance), [accessibilityReduceMotion](https://developer.apple.com/documentation/swiftui/environmentvalues/accessibilityreducemotion), and [NSWorkspace accessibilityDisplayShouldReduceMotion](https://developer.apple.com/documentation/appkit/nsworkspace/accessibilitydisplayshouldreducemotion).

**Implications:**

- The normal drop preview may animate opacity or geometry only if the result remains immediately understandable.
- With Reduce Motion enabled, use an immediate static outline/fill for the proposed zone and avoid sliding or morphing between regions.
- Divider dragging itself should remain direct and track the pointer; Reduce Motion applies to decorative or transitional animation, not to eliminating direct manipulation.

### 7. Persistence and restoration

`ProjectStore.saveTabs` deletes and rewrites the worktree's `terminalTab` rows. Each row stores one `WorkspaceTab`; `treeJSON` contains a `SplitTree` only for terminal tabs. Markdown, code, and chat rows store type-specific columns. `loadTabs` reconstructs the same global list and returns one `activeTabId` (`Packages/TillerCore/Sources/TillerCore/ProjectStore.swift`).

This can preserve today's terminal topology, but not the destination's complete state:

- pane-group tree and stable group IDs;
- local tab order in each group;
- active tab per group;
- focused/active group;
- divider proportions;
- decode version and migration/fallback state.

GRDB can remain the durable store, but whether the new layout is a versioned JSON aggregate, normalized records, or an extension of existing rows is unresolved. A migration also must define how each current global tab and each terminal-only split becomes one or more pane groups. Corrupt or partially restorable layouts need a deterministic fallback that preserves recoverable workspace items.

This is intentionally deferred to [Choose persistence and migration semantics for pane layouts](https://github.com/tillerai/tiller/issues/7).

### 8. SwiftUI/AppKit boundary

A low-risk candidate boundary, to validate rather than assume, is:

- **TillerCore:** platform-neutral pane-group topology, IDs, invariants, and pure mutations.
- **App:** local tab bars, drag source/destination state, drop-zone overlays, commands, accessibility semantics, and workspace-item composition.
- **TillerTerminal:** terminal resource/controller cache and terminal-specific focus/lifecycle adapter.
- **AppKit renderer owned by an appropriate UI module:** `NSSplitViewController` hierarchy, native dividers, min/max sizing, and divider-ratio reporting.

`NSHostingController` and `NSViewControllerRepresentable` are the supported bridges between SwiftUI and AppKit. Their availability does not decide which package should own a universal renderer.

Sources: [AppKit integration](https://developer.apple.com/documentation/swiftui/appkit-integration), [NSHostingController](https://developer.apple.com/documentation/swiftui/nshostingcontroller), [NSViewControllerRepresentable](https://developer.apple.com/documentation/swiftui/nsviewcontrollerrepresentable), plus the current package dependency rules in `AGENTS.md`.

The final boundary remains the subject of [Choose the module boundary for the universal layout engine](https://github.com/tillerai/tiller/issues/8).

## Recommended validation prototypes

1. **Drop lifecycle and feedback:** prove that edge zones update continuously through `dropUpdated`, clear through `dropExited`, commit only in `performDrop`, and do not rebuild AppKit layout during hover.
2. **Terminal identity preservation:** move a running terminal tab between groups and create a split without restarting the PTY, losing scrollback, or misrouting keyboard input.
3. **Non-terminal focus:** move and activate chat, Markdown, and code tabs; verify the intended input becomes first responder and keyboard commands target the focused group.
4. **Resize and restoration:** create a three-level mixed-axis layout, choose non-equal divider positions, relaunch, and verify topology, proportions, local active tabs, and focus fallback.
5. **Invalid-state fallback:** remove or corrupt one referenced workspace item and verify the remaining groups restore without discarding unrelated tabs.
6. **Drag-type separation:** verify internal tab moves, Finder file drops, cancelled drags, and rejected cross-worktree drags cannot be confused.
7. **Accessibility audit:** operate split/move/focus/close without a pointer; inspect the hierarchy and actions with Accessibility Inspector; complete a VoiceOver pass.
8. **Reduced motion:** verify static, high-contrast zone feedback and no layout-transition motion when Reduce Motion is enabled.
9. **Deep-layout performance:** measure continuous drag and divider resize over a realistically nested layout; ensure hover state does not recreate content hosts.

## Questions still requiring human decisions

1. What invariants govern empty groups, last-tab removal, maximum depth, minimum size, and group collapse?
2. What center/edge geometry and hysteresis make drop targets predictable across large and small panes?
3. Does a drop move only an existing tab, or can modifier keys request a duplicate view where the content type supports it?
4. What does the Split menu create for terminal, chat, Markdown, code, file selection, and existing tabs?
5. Which stable identities own terminal processes, chat sessions/controllers, documents, views, tabs, and groups?
6. Which menu names and keyboard shortcuts avoid Tiller's existing command conflicts?
7. Which accessibility actions are primary, and does a pane-group rotor improve real VoiceOver navigation?
8. Should ordinary pane groups ever collapse, or should an empty/last-tab rule remove the group from the topology?
9. Should the focused pane resist window resize through a different holding priority?
10. What versioned persistence representation and migration fallback preserve current tabs and terminal splits?
11. What compact-window behavior is required when all pane minimum sizes cannot be satisfied?
12. What terminal-content accessibility is actually exposed by the bundled GhosttyTerminal/libghostty version?

## Primary sources

### Apple documentation and SDK

- [NSSplitViewController](https://developer.apple.com/documentation/appkit/nssplitviewcontroller)
- [NSSplitView](https://developer.apple.com/documentation/appkit/nssplitview)
- [NSSplitViewItem](https://developer.apple.com/documentation/appkit/nssplitviewitem)
- [NSSplitViewItem.holdingPriority](https://developer.apple.com/documentation/appkit/nssplitviewitem/holdingpriority)
- [SwiftUI drag and drop](https://developer.apple.com/documentation/swiftui/drag-and-drop)
- [DropDelegate](https://developer.apple.com/documentation/swiftui/dropdelegate)
- [DropInfo](https://developer.apple.com/documentation/swiftui/dropinfo)
- [DropProposal](https://developer.apple.com/documentation/swiftui/dropproposal)
- [DropOperation](https://developer.apple.com/documentation/swiftui/dropoperation)
- [dropDestination](https://developer.apple.com/documentation/swiftui/view/dropdestination(for:action:istargeted:))
- [FocusState](https://developer.apple.com/documentation/swiftui/focusstate)
- [AccessibilityFocusState](https://developer.apple.com/documentation/swiftui/accessibilityfocusstate)
- [Accessible controls](https://developer.apple.com/documentation/swiftui/accessible-controls)
- [Accessible navigation](https://developer.apple.com/documentation/swiftui/accessible-navigation)
- [Accessible appearance](https://developer.apple.com/documentation/swiftui/accessible-appearance)
- [AppKit integration](https://developer.apple.com/documentation/swiftui/appkit-integration)
- [Apple HIG: Drag and Drop](https://developer.apple.com/design/human-interface-guidelines/drag-and-drop)
- [Apple HIG: Accessibility](https://developer.apple.com/design/human-interface-guidelines/accessibility)
- [Apple HIG: Keyboards](https://developer.apple.com/design/human-interface-guidelines/keyboards)
- Installed Xcode 26.5 SDK declarations: `AppKit.framework/Headers/NSSplitViewItem.h`, `NSSplitView.h`, `NSLayoutConstraint.h`, and `SwiftUI.swiftinterface`.
- Bundled Apple documentation snapshot, dated 2026-05-30, under the `macos-development-swiftui-appkit` skill references.

### Tiller source

- `Packages/TillerCore/Sources/TillerCore/SplitTree.swift`
- `Packages/TillerCore/Sources/TillerCore/WorkspaceTab.swift`
- `Packages/TillerCore/Sources/TillerCore/ProjectStore.swift`
- `Packages/TillerTerminal/Sources/TillerTerminal/SplitViewRenderer.swift`
- `Packages/TillerTerminal/Sources/TillerTerminal/TerminalPaneCache.swift`
- `Packages/TillerTerminal/Sources/TillerTerminal/PtyTerminalPane.swift`
- `App/RowReorder.swift`
- `App/TabBarView.swift`
- `App/ContentView.swift`
- `App/AppModel.swift`
- `App/TillerApp.swift`
- `project.yml`
- `AGENTS.md`

## Gaps

- Apple documentation pages are JavaScript-rendered; URL identity was live-checked, while exact protocol signatures and defaults were verified against the installed Apple SDK and the bundled Apple documentation snapshot.
- Apple sample projects were not fetched. The prototypes above must validate integration behavior in Tiller rather than treating API availability as proof.
- GhosttyTerminal/libghostty terminal-content accessibility was not inspected deeply enough to state its VoiceOver ceiling.
- No Accessibility Inspector or VoiceOver session was run; those are required validation steps, not conclusions that research alone can provide.
- Exact keyboard shortcuts, drop-zone geometry, collapse policy, holding-priority policy, and persistence representation remain product/architecture decisions by design.
