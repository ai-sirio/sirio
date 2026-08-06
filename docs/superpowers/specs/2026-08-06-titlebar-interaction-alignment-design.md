# Native Titlebar Interaction and Alignment Design

## Problem and root-cause evidence

Tiller currently combines a hidden titlebar with a custom SwiftUI strip drawn in the window canvas:

- `App/TillerApp.swift` applies `.windowStyle(.hiddenTitleBar)` to the main `WindowGroup`.
- `App/WindowChromeConfigurator.swift` inserts `.fullSizeContentView`, makes the titlebar transparent, and leaves the window chrome configured through an `NSViewRepresentable`.
- `App/ContentView.swift` builds `TitleStrip` inside `workspaceView`, above the split content, and explicitly ignores the top safe area so the 28pt strip sits in the reserved titlebar band.
- `App/TitleStrip.swift` is a SwiftUI `HStack` with a leading traffic-light inset, a flexible empty region, and the trailing controls.
- `App/AppTheme.swift` defines the current shared geometry: a 28pt strip, a 78pt traffic-light inset, and 13pt icon glyphs. It also defines `HoverIconButtonStyle`, whose hover and pressed states depend on SwiftUI controls receiving pointer events.

The recent toolbar replacement moved the four chrome controls into this canvas strip. The controls still have their existing actions, help text, accessibility labels, and callbacks in `ContentView`, but the titlebar band intercepts pointer events before those SwiftUI controls receive them. The observed result is consistent with the existing `ContentView` comment: controls are visible but do not show hover or pressed feedback, and their clicks do not reach the callbacks. The same ownership problem also means the empty part of the band is not reliably preserved as native titlebar interaction space.

The current visual arrangement has one leading sidebar control and three trailing controls: right panel, split, and permissions. Their common icon size is defined in `AppTheme`, but their effective hit regions are not hosted in a shared native titlebar control region. The left sidebar icon is also visually too high relative to the traffic lights.

## Goals

- Host the custom strip in a real AppKit titlebar region, preferably a titlebar accessory or equivalent hosted titlebar view.
- Preserve native titlebar behavior in the unused area, including window dragging and the system-configured titlebar double-click action.
- Make all four SwiftUI controls receive hit-testing so hover, pressed state, and existing callbacks work.
- Keep the existing callbacks, state, actions, shortcuts, help text, and accessibility labels.
- Give the four controls a common frame and common vertical centering rule.
- Optically align the four icons with the traffic lights, including lowering the left sidebar icon as part of the shared alignment rather than adding a one-off offset.
- Support both the legacy split and the new workspace split when verifying the behavior.
- Keep the change narrowly focused on titlebar hosting, interaction routing, and alignment.

## Non-goals

- Do not force double-click to zoom or otherwise replace the macOS titlebar double-click preference.
- Do not redesign the four actions, their state model, or their callback signatures.
- Do not change the semantics of sidebar visibility, right-panel visibility, splitting, or permissions navigation.
- Do not redesign the window controls, titlebar colors, general card geometry, or unrelated SwiftUI layout.
- Do not refactor unrelated application code or tests.
- Do not alter pre-existing working-tree changes.

## Approaches considered

### 1. Keep the strip in the canvas and forward events manually

This would leave `TitleStrip` in `ContentView` and attempt to forward mouse events from the titlebar container into SwiftUI. It is rejected because it keeps two competing event owners, makes hover tracking dependent on custom forwarding, and risks breaking native drag and double-click behavior again.

### 2. Restore the controls to the window toolbar

This would likely restore native hit-testing, but it would reintroduce the system toolbar layout that the recent change intentionally replaced. It would also make the desired optical relationship to the traffic lights and the custom four-icon arrangement dependent on toolbar placement rules rather than a controlled shared frame.

### 3. Add a custom AppKit mouse-interception or forwarding view

A transparent AppKit view could distinguish control rectangles from the empty titlebar area and route events accordingly. This is rejected as unnecessarily fragile: it duplicates hit-testing, must preserve drag and double-click edge cases, and would make SwiftUI state and AppKit event ownership harder to reason about.

### 4. Host the SwiftUI strip in a native titlebar accessory region — recommended

Create the titlebar region at the AppKit window boundary and host the existing SwiftUI strip there. The accessory owns only the control-bearing portion; the remaining titlebar area remains native. SwiftUI retains the existing actions and state, while AppKit retains titlebar drag and double-click behavior. This directly separates interaction ownership instead of compensating for the current overlap.

## Proposed architecture and AppKit/SwiftUI boundary

`WindowChromeConfigurator` remains the boundary that discovers and configures the `NSWindow`. Its titlebar configuration should also establish the native titlebar accessory or equivalent hosted region for the custom strip. The implementation should use the existing window lifecycle and avoid creating a second independent window-chrome coordinator.

The AppKit side owns:

- locating the main `NSWindow` after the representable is attached;
- installing, updating, and removing the titlebar accessory as the SwiftUI content requires;
- defining the accessory’s placement and native titlebar participation;
- ensuring the accessory is limited to the control region rather than covering the whole titlebar;
- preserving the native empty titlebar area for window movement and system behavior.

The SwiftUI side owns:

- rendering the four controls and their labels/icons;
- retaining the existing state bindings and callbacks from `ContentView`;
- retaining `HoverIconButtonStyle`, help text, and accessibility labels;
- applying the shared control frame and alignment contract;
- rendering no full-width transparent hit-testing surface behind the accessory.

`ContentView` should continue to provide the current control actions and the split-specific choice between `universalSplitMenu` and the legacy horizontal split button. It should stop treating the full-width canvas strip as the titlebar interaction surface. `TitleStrip` should become the reusable SwiftUI content hosted by the native titlebar region, with its layout expressed in terms of the shared control frame rather than a full-width canvas row.

`AppTheme` remains the source of shared titlebar geometry and icon sizing. Any new geometry token should describe the common control frame or accessory spacing, not a workaround for one icon. The existing 28pt titlebar height should remain the starting contract unless native accessory metrics require a documented, common adjustment.

## Behavior and event routing

1. The window continues to use the hidden-titlebar/full-size-content configuration needed by Tiller’s canvas treatment.
2. AppKit installs the SwiftUI-hosting accessory in the native titlebar region.
3. Pointer events inside a control’s frame are delivered to the hosted SwiftUI control. `HoverIconButtonStyle` therefore receives hover and pressed updates, and the existing action closures execute on click.
4. Pointer events in the empty titlebar area are not covered by the accessory. AppKit handles native window dragging there.
5. A double-click in the empty titlebar area remains handled by macOS and follows the user’s current system preference. The implementation must not install a custom double-click action that always zooms.
6. The accessory must not swallow events belonging to the traffic lights. The traffic lights remain native window controls; the accessory’s layout must be positioned beside them according to the shared alignment contract.
7. The event behavior must be identical for the legacy terminal split and the new workspace split. The split implementation may change which split button content is rendered, but it must not change titlebar event ownership.
8. Existing keyboard shortcuts and menu commands remain the fallback and parallel access paths for the same actions; no new command routing is introduced by this design.

## Layout and optical alignment

The four controls use one common button frame, one common vertical alignment, and one common spacing rule. The implementation must not add a special offset for the left sidebar icon or any other individual icon.

The layout contract is:

- preserve the four-control order: left sidebar control on the leading side, then right panel, split, and permissions in the existing trailing group;
- use a shared frame sized for the largest of the four hit targets, with the icon centered inside that frame;
- center the shared frames vertically within the native titlebar accessory region;
- use the same vertical centerline as the traffic-light group, subject to optical verification rather than glyph-bound assumptions;
- keep the 13pt icon size as the existing baseline from `AppTheme` unless verification shows that a common size change is required;
- retain the existing traffic-light relationship without using the current full-width `trafficLightInset` as a substitute for native titlebar placement;
- align by the common frame and centerline, not by per-icon `offset` values;
- ensure the left sidebar icon is lowered by the shared frame/center rule, not by a sidebar-only correction.

The empty titlebar region must remain visually continuous with the existing chrome while remaining native for hit-testing. The accessory should not add an opaque background or an invisible full-width control layer that changes titlebar event ownership.

## Accessibility

- Preserve the existing accessibility labels: Sidebar, Right panel, Split terminal or Split Right With…, and Permissions.
- Preserve the existing help text, including state-dependent sidebar and right-panel descriptions.
- Keep each icon as a real SwiftUI `Button` or equivalent control so keyboard and assistive-technology activation continue to invoke the existing actions.
- Ensure the accessory does not introduce duplicate accessibility elements for the native traffic lights or expose the empty titlebar area as a fake control.
- Verify that VoiceOver focus can reach all four controls in a logical order and that the controls remain operable without pointer interaction.
- Do not rely on hover or pressed appearance as the only indication of control identity or state.

## Test plan

### Deterministic action and geometry tests

Add or update focused tests for the titlebar interaction contract without changing unrelated test coverage:

- verify that the four control actions remain connected to their existing state changes or callbacks;
- verify that both the legacy split button and the new workspace split menu use the same titlebar hosting and shared control geometry;
- verify the common control frame, common vertical center, and shared spacing/alignment values;
- verify that no control-specific vertical offset is part of the layout contract;
- verify that the titlebar accessory does not claim the empty drag region as a control surface, where the AppKit test seam permits deterministic inspection.

### Manual interaction verification

Run the app in both split configurations:

- legacy terminal split;
- new workspace split.

For each configuration, verify manually:

1. Move the pointer over each of the four icons and confirm hover feedback appears.
2. Press and release each icon and confirm pressed feedback appears and the existing action fires.
3. Confirm sidebar, right-panel, split, and permissions behavior remains unchanged.
4. Drag the window from multiple points in the empty titlebar area.
5. Double-click the empty titlebar area with a macOS preference that does not zoom, then with a preference that does; confirm Tiller follows the preference rather than forcing one behavior.
6. Confirm the traffic lights remain native and usable.
7. Compare the icon centerline with the traffic lights and confirm the left sidebar icon is no longer too high.
8. Repeat the checks in light and dark appearances if the titlebar hosting changes visual contrast or hit-region visibility.

Run the repository’s normal CI gate after implementation, but do not broaden this design into unrelated CI or test refactoring.

## Acceptance criteria

- The custom strip is hosted in a real native AppKit titlebar region rather than as a full-width canvas overlay.
- All four controls show hover and pressed feedback and deliver clicks to their existing callbacks.
- The empty titlebar area supports native window dragging.
- Empty-area double-click follows the macOS system preference and is not hard-coded to zoom.
- The traffic lights remain native and are not covered by the accessory.
- The four controls share a common frame, common vertical centering, and common alignment rule.
- The left sidebar icon is optically centered with the traffic lights without a special per-icon offset.
- Existing state, actions, help text, accessibility labels, keyboard shortcuts, and split-specific behavior are preserved.
- Focused action/geometry tests pass for both legacy and new split paths, and the manual hover/click/drag/double-click checks pass for both paths.
- The implementation changes only the titlebar hosting/alignment surface and its focused tests; unrelated working-tree changes remain untouched.

## Risks and mitigations

| Risk | Mitigation |
| --- | --- |
| The accessory still covers more of the titlebar than intended. | Keep its frame limited to the control region and manually test dragging at several empty-area locations. |
| SwiftUI-hosted controls lose hover or pressed updates inside the accessory. | Verify event delivery directly through manual interaction and retain real SwiftUI buttons with `HoverIconButtonStyle`. |
| Native titlebar double-click behavior is replaced by custom handling. | Do not add a double-click action; leave the empty region native and test both macOS preference outcomes. |
| The accessory overlaps or visually misaligns with traffic lights. | Use one shared frame and centerline, inspect the native controls directly, and reject per-icon offsets. |
| Legacy and workspace split paths diverge. | Keep `ContentView`’s existing split-specific content selection while testing both paths against the same accessory and geometry contract. |
| Window lifecycle creates duplicate or stale accessories. | Make installation/update/removal idempotent within `WindowChromeConfigurator` and verify after route/window updates. |
| The fix changes unrelated canvas or card geometry. | Remove only the full-width strip hosting responsibility, preserve existing canvas tokens, and review the final diff for scope before commit. |
