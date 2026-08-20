# IntelliJ-Inspired Translucent Shell Design

**Date:** 2026-08-20  
**Status:** Approved for implementation planning  
**Reference:** user-supplied `Screenshot 2026-08-20 alle 19.39.17.png`

## Objective

Redesign Tiller's application shell around an IntelliJ-inspired frame: a translucent outer chassis containing three compact, rounded work surfaces for the project sidebar, central workspace, and right panel. The central pane remains the visual priority, while the existing application behavior, content hierarchy, and technical colour semantics remain intact.

The redesign applies to both dark and light appearances. The dark palette is sampled from the supplied IntelliJ screenshot. The light palette is the approved cool-glass translation of the same hierarchy.

## Approved Direction

The approved direction combines these choices:

- three independent panels inside one continuous outer frame;
- central pane emphasized by flexible width, not by a permanent outline;
- compact geometry with a 4 px gap and 7 px panel radius;
- titlebar and status bar integrated into the translucent frame;
- native transparency and blur where the backend supports them reliably;
- deterministic opaque fallback everywhere else;
- both dark and light appearances;
- all native Tiller surfaces adopt the new palette;
- terminal ANSI colours and syntax highlighting remain unchanged;
- a 1 px coral focus ring appears only for keyboard-visible focus;
- Settings renders as one rounded panel inside the same frame;
- hiding either sidebar expands the centre while preserving its outer inset and rounded treatment.

## Scope

### In scope

- application-level frame, titlebar, status bar, and column composition;
- left sidebar, central workspace, and right panel containers;
- Settings shell geometry;
- semantic theme tokens for the frame and panel hierarchy;
- dark and light palette updates across native UI surfaces;
- platform-aware translucent material selection and fallback;
- focus-visible treatment for the active panel;
- layout, theme, drawn-workspace, and visual regression coverage.

### Out of scope

- changes to project, worktree, tab, chat, terminal, or right-panel data flow;
- changes to sidebar widths (`325 px` left, `405 px` right);
- changes to terminal ANSI palettes or syntax highlighting;
- automatic sidebar hiding at narrow window widths;
- new user-facing translucency preferences;
- typography, iconography, row density, or interaction redesigns unrelated to the shell;
- simulated CSS-style blur or a bespoke software blur implementation.

## Architecture

### Shell presentation module

Add `rust/crates/tiller/src/shell_chrome.rs`. This module owns presentation-only helpers for the application frame and panels. It must not own application state, visibility state, focus handles, or event routing.

The module exposes a small semantic interface for:

- the outer frame;
- a rounded panel surface;
- keyboard-visible focus treatment;
- the single-panel Settings surface;
- the resolved material mode.

`TillerWorkspace` continues to own all state and passes the current theme, panel visibility, and focus-visible state into these helpers. The module may receive a focus flag, but it never determines or stores focus itself.

This boundary keeps the already-large `main.rs` focused on orchestration while avoiding a premature generic component in `tiller_ui`.

### Theme tokens

Extend `tiller_theme` with semantic shell tokens. Colour roles belong in `ThemeColors`; geometry belongs in the existing spacing and radius structures rather than as literals in the app crate.

Required roles are:

- `frame_surface`;
- `frame_fallback`;
- `panel_surface`;
- `panel_border`;
- `panel_focus_ring`;
- `shell_gap` (`4 px`);
- `shell_outer_inset` (`4 px`);
- `shell_panel_radius` (target `7 px`);
- dark and light frame opacity.

Existing semantic colours for warning, success, danger, activity, agent identity, ANSI output, and syntax highlighting are not remapped to these roles.

Existing surface roles are retuned rather than duplicated:

- `canvas` resolves to the frame material or its opaque fallback;
- `background`, `chat_surface`, and `sidebar` resolve to `panel_surface`;
- the existing selected-row role resolves to the approved selected fill;
- `raised`, `composer`, `card_fill`, `inset`, popover, and modal surfaces keep their current depth relationships but derive from the new panel scale;
- `terminal_surface` and terminal ANSI entries remain unchanged;
- code/diff washes keep their semantic hues and are adjusted only as needed to preserve contrast on the new panel surface.

### Workspace composition

The primary render hierarchy becomes:

1. outer shell frame;
2. titlebar rendered on the frame material;
3. compact work area with visible panels separated by 4 px frame gaps;
4. status bar rendered on the frame material;
5. root overlay layer for menus, command palette, modals, and toast notifications.

The work area retains its current ordering:

1. left project sidebar when visible;
2. flexible centre workspace;
3. right Files/Changes/Activity panel when visible.

The current one-pixel structural seams between columns are removed. Individual components may retain internal separators that express their own hierarchy, such as tab-bar or section separators.

When a sidebar is hidden, the centre consumes the released width. It does not become edge-to-edge: the 4 px outer inset and rounded panel shape remain visible.

### Settings composition

Settings continues to replace the workspace content, but it uses the same outer frame, titlebar, and status bar. Its content is wrapped in one rounded panel with the same surface, border, and radius tokens as the workspace panels.

Showing the status bar in Settings is an intentional change from the current Settings render branch. It keeps the outer frame structurally stable while navigating between workspace and Settings.

### Overlay and clipping boundary

Rounded surfaces must not clip floating UI. Panel backgrounds and content wells may clip where necessary to render clean corners, but popovers, context menus, command palette, modals, and toasts remain or are lifted to the workspace overlay layer.

The implementation must preserve current z-order and event delivery. A rounded panel is a visual boundary, not a new event or ownership boundary.

## Visual System

### Dark appearance

The target dark palette comes from dominant pixels in the supplied screenshot:

| Role | Value |
| --- | --- |
| Opaque frame fallback | `#222427` |
| Panel surface | `#18191A` |
| Selected or active row fill | `#2D2F34` |
| Panel border and internal seam | `#27292D` |
| Primary text | `#CBCDD4` |
| Sampled meta/disabled text | `#686B71` |
| Accessible secondary text | `#85888F` |

The native translucent frame uses `#222427` as its tint with a target opacity of `0.88`. Its final displayed pixel may vary with the desktop backdrop; the opaque fallback must render exactly `#222427`.

### Light appearance

The approved light direction is **cool glass**:

| Role | Value |
| --- | --- |
| Opaque frame fallback | `#DCE5E9` |
| Panel surface | `#F4F7F8` |
| Selected or active row fill | `#D7E2E7` |
| Panel border and internal seam | `#CCD8DD` |
| Primary text | `#313A40` |
| Sampled meta/disabled text | `#68757B` |
| Accessible secondary text | `#667379` |

The native translucent frame uses `#DCE5E9` as its tint with a target opacity of `0.82`. Its opaque fallback must render exactly `#DCE5E9`.

### Surface rules

- Only the frame, titlebar, status bar, and gaps use translucent material.
- Panels remain opaque so the desktop cannot interfere with terminal, code, chat, or editor readability.
- The centre is emphasized through its flexible width and content prominence.
- No panel receives a permanent accent shadow or outline.
- Keyboard-visible focus adds a 1 px coral ring using Tiller's existing accessible accent family.
- Pointer focus alone does not add the ring.
- Existing semantic status and agent colours remain meaning-bearing accents rather than structural colour.
- The sampled meta colours are reserved for nonessential metadata and disabled labels. Body-size secondary text uses `#85888F` dark and `#667379` light, which clear 4.5:1 on the approved panel surfaces.

## Material Resolution and Fallback

Introduce a presentation-level material resolution with two outcomes:

- `NativeTranslucent`: used only where the active platform/backend can reliably supply transparent composition and native backdrop blur;
- `OpaqueFallback`: uses the resolved appearance's exact fallback colour at full opacity.

The app already creates a window that may appear transparent. That capability must not be treated as proof that backdrop blur is available. The implementation must use an explicit known-supported path; unknown or unsupported backends select `OpaqueFallback`.

No error is shown to the user when native material is unavailable. Visual consistency is preferable to an unblurred, partially transparent frame that exposes distracting desktop detail.

## Layout and Data Flow

No domain or navigation data flow changes. Each frame reads the current global `Theme`, resolves shell material, and passes the existing entities into presentation wrappers.

The centre surface currently derives its height from the viewport minus fixed titlebar, tab-bar, and status-bar heights. The new calculation must also subtract the frame's vertical insets and any frame-owned gaps. This is required to keep restored chats, virtualized transcripts, composers, browsers, and terminal surfaces within the visible viewport.

Panel visibility continues to come from the existing sidebar and right-panel booleans. There is no automatic responsive state transition and no persistence schema change.

## Focus and Accessibility

The focus ring follows focus-visible semantics: it appears for keyboard navigation and disappears for pointer-only focus. The state is determined by the workspace's existing input/focus handling and passed to the shell presentation helper.

Acceptance requires:

- the coral ring to remain distinguishable on both panel palettes;
- primary and secondary text to meet the project's existing contrast expectations on panel surfaces;
- current keyboard traversal and focus restoration to continue working when sidebars or Settings mount and unmount;
- rounded borders and translucent material not to become the only indicator of selected, errored, running, or needs-input states.

## Failure and Edge Cases

- Unsupported or ambiguous compositor capability selects the opaque fallback.
- A hidden left or right panel removes only that panel; the centre expands and preserves the outer frame inset.
- With both sidebars hidden, the centre remains one rounded panel inside the frame.
- Overlay UI renders above panel clipping.
- Existing minimum-size behavior remains unchanged; the redesign does not silently hide panels.
- Settings uses the same material resolution as the workspace.
- A theme appearance change updates frame and panels from the global theme without recreating application state.

## Test Strategy

Implementation follows the repository's test-first convention.

### Theme unit tests

Add tests that pin:

- exact dark and light palette values;
- translucent tint opacity and exact opaque fallback values;
- 4 px gap, 4 px outer inset, and 7 px panel radius;
- primary and secondary text contrast on their panel surfaces;
- unchanged ANSI, syntax, status, and agent colour tokens.

### Shell presentation tests

Add focused tests for:

- three visible panels in left-centre-right order;
- centre expansion with either or both sidebars hidden;
- preserved outer inset in every visibility state;
- one Settings panel inside the shared frame;
- keyboard-visible focus ring and absence for pointer-only focus;
- absence of old column seam elements.

### Drawn workspace tests

Use GPUI test support and stable debug selectors to verify that the new hierarchy still mounts and exposes:

- chat and composer;
- terminal pane;
- Files/Changes/Activity panel;
- Settings;
- command palette, popovers, context menus, modals, and toasts.

Interaction assertions remain behavior-oriented; a static screenshot is not proof that controls are reachable.

### Verification gates

Run:

1. focused tests for every changed crate;
2. `Scripts/ci.sh`, which must print `CI OK`;
3. a real app launch and matched-size screenshots for dark and light appearances;
4. screenshots with both sidebars, each sidebar hidden, both hidden, Settings, and keyboard focus visible;
5. a visual check that composer, scrollbars, menus, terminal content, and overlays are not clipped.

Exercise both material-resolution branches through deterministic tests. Real native-blur evidence is required only when the active host/backend is a supported path; otherwise report it as not exercised and do not treat alpha rendering as blur proof.

## Acceptance Criteria

The work is complete when:

- titlebar, work area, and status bar read as one continuous outer chassis;
- the three visible work surfaces are individually rounded and separated by compact 4 px gaps;
- dark mode visibly matches the supplied IntelliJ palette;
- light mode matches the approved cool-glass direction;
- native blur is used only on a known-supported path and all other paths are cleanly opaque;
- hiding panels expands the centre without removing its inset or rounded treatment;
- Settings uses one rounded panel in the same frame;
- keyboard focus remains clearly visible without a permanent centre outline;
- terminal ANSI and syntax colours are unchanged;
- overlays and content are not clipped;
- `Scripts/ci.sh` prints `CI OK`.
