# IntelliJ-Inspired Translucent Shell Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give Tiller an IntelliJ-inspired compact shell: one translucent outer frame containing the titlebar and status bar, plus rounded left, center, right, and Settings panels with 4 px spacing, 7 px radii, the sampled dark/light palette, and keyboard-only focus rings.

**Architecture:** Extend `tiller_theme` with semantic shell tokens, add a small `shell_chrome` module in the app crate to resolve native material/fallback behavior and build consistent panel containers, and keep `TillerWorkspace` responsible only for arranging its existing child entities. Reuse the persisted `SettingsSnapshot.translucency` preference and the existing workspace action queue so startup and live changes update GPUI's window background without adding another setting. Terminal ANSI/syntax colors and terminal surfaces remain untouched.

**Tech Stack:** Rust 2024, GPUI, `tiller_theme`, `tiller_ui`, inline Rust tests with GPUI `TestAppContext`/`VisualTestContext`, repository gates in `Scripts/ci.sh` and `Scripts/ci-linux.sh`.

---

## Source of truth and constraints

- Design spec: `docs/superpowers/specs/2026-08-20-intellij-inspired-translucent-shell-design.md`.
- Preserve existing sidebar, tab, terminal, chat, Settings, right-panel, overlay, drag/drop, selection, and persistence behavior. This is a shell/palette change, not a navigation rewrite.
- Keep the existing `appearance.translucency` persistence contract. Do not add a second preference or schema field.
- Native blur is enabled only on targets where this GPUI revision has a reliable backend: macOS and Windows. Linux/FreeBSD and headless tests use the opaque fallback even when the preference is on.
- Do not change `ThemeColors::terminal_surface`, ANSI colors, syntax colors, diff-status hues, agent-brand colors, or activity-status colors.
- Do not edit or stage the user's untracked `.idea/` directory.
- Tests first for every behavior change. Each RED command must fail for the stated reason before implementation is added.
- Commit messages use Conventional Commits with a lower-case imperative subject.

## File map

- Modify `rust/crates/tiller_theme/src/lib.rs`
  - Add the shell palette and geometry tokens.
  - Remap existing native-surface/text roles onto the new palette.
  - Add palette, contrast, and geometry contract tests.
- Modify `docs/linux-rewrite/THEME-PROVENANCE.md`
  - Record which values came from the IntelliJ screenshot and which are accessibility adjustments or platform fallbacks.
- Create `rust/crates/tiller/src/shell_chrome.rs`
  - Resolve blurred versus opaque shell material.
  - Return the GPUI window-background mode and frame fill.
  - Build the shared rounded/focus-visible panel container.
  - Unit-test material and focus decisions.
- Modify `rust/crates/tiller/src/main.rs`
  - Declare `shell_chrome`.
  - Carry the persisted translucency flag into the workspace.
  - Replace seam dividers with the inset/gapped three-panel shell.
  - Render Settings as one shell panel while retaining titlebar and status bar.
  - Apply live translucency changes through the existing action queue.
  - Add drawn geometry, visibility, Settings, and focus tests.
- Modify `rust/crates/tiller_ui/src/titlebar.rs`
  - Let the outer frame show through the titlebar region.
- Modify `rust/crates/tiller_ui/src/status_bar.rs`
  - Let the outer frame show through the status bar while keeping tooltips raised and opaque.
- Modify `rust/crates/tiller_ui/src/controls.rs`
  - Move Settings cards to the semantic raised-surface token.
- Modify `rust/crates/tiller_ui/src/settings.rs`
  - Add/adjust a drawn Settings surface assertion only if needed to expose a stable root selector; do not alter the preference contract.

### Task 1: Add semantic IntelliJ shell palette and geometry tokens

**Files:**
- Modify: `rust/crates/tiller_theme/src/lib.rs`
- Modify: `docs/linux-rewrite/THEME-PROVENANCE.md`

**Produces:**
- `ThemeColors::{frame_surface, frame_fallback, panel_surface, panel_border, panel_focus_ring}`.
- `Spacing::{shell_gap, shell_outer_inset}`.
- `Radii::shell_panel`.
- Existing semantic roles mapped to the approved palette without touching terminal/syntax/ANSI roles.

- [ ] **Step 1: Add failing palette and geometry tests**

In `rust/crates/tiller_theme/src/lib.rs`'s existing `#[cfg(test)] mod tests`, add:

```rust
#[test]
fn intellij_shell_palette_matches_the_approved_reference() {
    let dark = Theme::dark();
    assert_eq!(dark.frame_fallback, rgb_hex(0x222427));
    assert_eq!(dark.frame_surface, color(0x22 as f32 / 255.0, 0x24 as f32 / 255.0, 0x27 as f32 / 255.0, 0.88));
    assert_eq!(dark.panel_surface, rgb_hex(0x18191A));
    assert_eq!(dark.panel_border, rgb_hex(0x27292D));
    assert_eq!(dark.title, rgb_hex(0xCBCDD4));
    assert_eq!(dark.subtitle, rgb_hex(0x85888F));
    assert_eq!(dark.meta, rgb_hex(0x686B71));
    assert_eq!(dark.selected_fill, rgb_hex(0x2D2F34));

    let light = Theme::light();
    assert_eq!(light.frame_fallback, rgb_hex(0xDCE5E9));
    assert_eq!(light.frame_surface, color(0xDC as f32 / 255.0, 0xE5 as f32 / 255.0, 0xE9 as f32 / 255.0, 0.82));
    assert_eq!(light.panel_surface, rgb_hex(0xF4F7F8));
    assert_eq!(light.panel_border, rgb_hex(0xCCD8DD));
    assert_eq!(light.title, rgb_hex(0x313A40));
    assert_eq!(light.subtitle, rgb_hex(0x667379));
    assert_eq!(light.meta, rgb_hex(0x68757B));
    assert_eq!(light.selected_fill, rgb_hex(0xD7E2E7));
}

#[test]
fn shell_body_text_meets_wcag_aa_on_its_panel() {
    for (name, theme) in [("dark", Theme::dark()), ("light", Theme::light())] {
        assert!(
            contrast_ratio(theme.title, theme.panel_surface) >= 4.5,
            "{name} title text must clear AA"
        );
        assert!(
            contrast_ratio(theme.subtitle, theme.panel_surface) >= 4.5,
            "{name} subtitle text must clear AA"
        );
    }
}

#[test]
fn shell_geometry_is_compact_and_consistent() {
    let spacing = Spacing::default();
    let radii = Radii::default();
    assert_eq!(spacing.shell_gap, px(4.0));
    assert_eq!(spacing.shell_outer_inset, px(4.0));
    assert_eq!(radii.shell_panel, px(7.0));
}
```

- [ ] **Step 2: Run the tests and verify RED**

Run:

```bash
cd rust && cargo test -p tiller_theme intellij_shell_palette_matches_the_approved_reference
```

Expected: compilation fails because the shell fields do not exist yet.

- [ ] **Step 3: Add the semantic fields and exact palette values**

Add these fields to `ThemeColors`:

```rust
/// Material behind the floating panels when native blur is active.
pub frame_surface: Rgba,
/// Opaque frame used when native blur is disabled or unsupported.
pub frame_fallback: Rgba,
/// Shared opaque fill for left, center, right, and Settings panels.
pub panel_surface: Rgba,
/// One-pixel outline around each shell panel.
pub panel_border: Rgba,
/// Keyboard-only focus outline for the active shell panel.
pub panel_focus_ring: Rgba,
```

At the top of `ThemeColors::for_appearance`, construct the palette once:

```rust
let frame_fallback = Self::adaptive(rgb_hex(0x222427), rgb_hex(0xDCE5E9), appearance);
let frame_surface = Self::adaptive(
    color(0x22 as f32 / 255.0, 0x24 as f32 / 255.0, 0x27 as f32 / 255.0, 0.88),
    color(0xDC as f32 / 255.0, 0xE5 as f32 / 255.0, 0xE9 as f32 / 255.0, 0.82),
    appearance,
);
let panel_surface = Self::adaptive(rgb_hex(0x18191A), rgb_hex(0xF4F7F8), appearance);
let panel_border = Self::adaptive(rgb_hex(0x27292D), rgb_hex(0xCCD8DD), appearance);
let selected_fill = Self::adaptive(rgb_hex(0x2D2F34), rgb_hex(0xD7E2E7), appearance);
let text = Self::adaptive(rgb_hex(0xCBCDD4), rgb_hex(0x313A40), appearance);
let text_secondary = Self::adaptive(rgb_hex(0x85888F), rgb_hex(0x667379), appearance);
let text_tertiary = Self::adaptive(rgb_hex(0x686B71), rgb_hex(0x68757B), appearance);
let surface = panel_surface;
```

Replace the existing `text`, `text_secondary`, `text_tertiary`, `surface`, `raised`, `inset`, `composer`, `sidebar`, and `selected_fill` declarations; do not leave duplicate bindings below this block. `surface` remains as the compatibility name used by the existing derived-role code, but now aliases `panel_surface`.

Use the new values in the returned `ThemeColors`:

```rust
frame_surface,
frame_fallback,
panel_surface,
panel_border,
panel_focus_ring: accent,
background: panel_surface,
canvas: frame_fallback,
chat_surface: panel_surface,
sidebar: panel_surface,
selection_fill: selected_fill,
selected_fill,
title: text,
subtitle: text_secondary,
meta: text_tertiary,
```

Derive raised/recessed surfaces from `panel_surface`, not the retired base surface:

```rust
let raised = Self::adaptive(rgb_hex(0x1D1E21), rgb_hex(0xFBFCFC), appearance);
let composer = raised;
let inset = Self::adaptive(scaled(panel_surface, 0.72), scaled(panel_surface, 0.93), appearance);
```

Remove the old derived `sidebar` declaration entirely; the returned `sidebar` role is the approved `panel_surface` value.

Keep the existing `terminal_surface` expression and every ANSI/syntax/status/agent color byte-for-byte unchanged. Update old tests that asserted the retired Waku surface/text constants so they assert the new semantic contract instead of preserving stale values.

Add these two fields to the existing `Spacing` struct:

```rust
pub shell_gap: Pixels,
pub shell_outer_inset: Pixels,
```

Initialize them in the existing `Spacing::default()` `Self` literal:

```rust
shell_gap: px(4.0),
shell_outer_inset: px(4.0),
```

Add this field to `Radii` and initialize it in `Radii::default()`:

```rust
pub shell_panel: Pixels,
```

```rust
shell_panel: px(7.0),
```

- [ ] **Step 4: Document provenance and accessibility adjustments**

Add an “IntelliJ-inspired shell (2026-08-20)” section to `docs/linux-rewrite/THEME-PROVENANCE.md` recording:

- direct screenshot samples: panel, frame fallback, selected fill, border, primary, and meta colors;
- `#85888F` dark / `#667379` light as the adjusted secondary-text roles that clear 4.5:1;
- frame alpha targets 0.88 dark / 0.82 light;
- opaque fallback on targets without reliable native blur;
- preserved terminal/ANSI/syntax/status/agent palettes.

- [ ] **Step 5: Run the focused and crate tests GREEN**

Run:

```bash
cd rust && cargo test -p tiller_theme intellij_shell_palette_matches_the_approved_reference
cd rust && cargo test -p tiller_theme shell_body_text_meets_wcag_aa_on_its_panel
cd rust && cargo test -p tiller_theme shell_geometry_is_compact_and_consistent
cd rust && cargo test -p tiller_theme
```

Expected: all commands pass with non-zero test counts.

- [ ] **Step 6: Commit**

```bash
git add rust/crates/tiller_theme/src/lib.rs docs/linux-rewrite/THEME-PROVENANCE.md
git commit -m "feat: add IntelliJ shell theme tokens"
```

### Task 2: Add the shell material and shared panel primitive

**Files:**
- Create: `rust/crates/tiller/src/shell_chrome.rs`
- Modify: `rust/crates/tiller/src/main.rs` (module declaration only in this task)

**Produces:**
- A deterministic `ShellMaterial` resolver.
- The platform-specific GPUI `WindowBackgroundAppearance`.
- A consistent rounded panel with opaque fill, one-pixel border, clipping, focus containment, and an optional keyboard focus-ring selector.

- [ ] **Step 1: Create the module with failing unit tests first**

Create `rust/crates/tiller/src/shell_chrome.rs` with the imports, types, deliberately opaque initial decisions, and tests below. The initial implementation compiles but fails the blur/focus assertions, giving a behavioral RED rather than a missing-symbol error:

```rust
use gpui::{
    Div, FocusHandle, InteractiveElement, Rgba, Stateful, Window,
    WindowBackgroundAppearance, div, prelude::*,
};
use tiller_theme::Theme;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ShellMaterial {
    Opaque,
    Blurred,
}

pub(crate) fn resolve_material(
    translucency_enabled: bool,
    native_blur_supported: bool,
) -> ShellMaterial {
    ShellMaterial::Opaque
}

pub(crate) fn current_platform_material(translucency_enabled: bool) -> ShellMaterial {
    resolve_material(
        translucency_enabled,
        cfg!(any(target_os = "macos", target_os = "windows")),
    )
}

impl ShellMaterial {
    pub(crate) fn window_background(self) -> WindowBackgroundAppearance {
        WindowBackgroundAppearance::Opaque
    }

    pub(crate) fn frame_fill(self, theme: &Theme) -> Rgba {
        theme.frame_fallback
    }
}

pub(crate) fn panel_border(theme: &Theme, focus_visible: bool) -> Rgba {
    theme.panel_border
}

pub(crate) fn panel(
    id: &'static str,
    focus_handle: &FocusHandle,
    focus_visible: bool,
    theme: &Theme,
) -> Stateful<Div> {
    div()
        .id(id)
        .debug_selector(move || id.to_owned())
        .relative()
        .size_full()
        .bg(theme.panel_surface)
        .border_1()
        .border_color(panel_border(theme, focus_visible))
        .rounded(theme.radii.shell_panel)
        .overflow_hidden()
        .track_focus(focus_handle)
}

pub(crate) fn focus_is_keyboard_visible(
    focus_handle: &FocusHandle,
    window: &Window,
    cx: &gpui::App,
) -> bool {
    window.last_input_was_keyboard() && focus_handle.contains_focused(window, cx)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn material_requires_both_the_preference_and_native_support() {
        assert_eq!(resolve_material(false, false), ShellMaterial::Opaque);
        assert_eq!(resolve_material(false, true), ShellMaterial::Opaque);
        assert_eq!(resolve_material(true, false), ShellMaterial::Opaque);
        assert_eq!(resolve_material(true, true), ShellMaterial::Blurred);
    }

    #[test]
    fn material_maps_to_gpui_background_and_matching_frame_fill() {
        let theme = Theme::dark();
        assert_eq!(ShellMaterial::Opaque.window_background(), WindowBackgroundAppearance::Opaque);
        assert_eq!(ShellMaterial::Opaque.frame_fill(&theme), theme.frame_fallback);
        assert_eq!(ShellMaterial::Blurred.window_background(), WindowBackgroundAppearance::Blurred);
        assert_eq!(ShellMaterial::Blurred.frame_fill(&theme), theme.frame_surface);
    }

    #[test]
    fn panel_border_uses_accent_only_for_keyboard_focus() {
        let theme = Theme::dark();
        assert_eq!(panel_border(&theme, false), theme.panel_border);
        assert_eq!(panel_border(&theme, true), theme.panel_focus_ring);
    }
}
```

Add `mod shell_chrome;` next to the app crate's existing module declarations in `main.rs`.

- [ ] **Step 2: Run RED**

Run:

```bash
cd rust && cargo test -p tiller shell_chrome::tests::material_requires_both_the_preference_and_native_support
```

Expected: assertion failure for `resolve_material(true, true)` because the initial implementation returns `Opaque`.

- [ ] **Step 3: Implement the material and panel helpers**

Replace the deliberately opaque decision bodies with:

```rust
pub(crate) fn resolve_material(
    translucency_enabled: bool,
    native_blur_supported: bool,
) -> ShellMaterial {
    if translucency_enabled && native_blur_supported {
        ShellMaterial::Blurred
    } else {
        ShellMaterial::Opaque
    }
}

impl ShellMaterial {
    pub(crate) fn window_background(self) -> WindowBackgroundAppearance {
        match self {
            Self::Opaque => WindowBackgroundAppearance::Opaque,
            Self::Blurred => WindowBackgroundAppearance::Blurred,
        }
    }

    pub(crate) fn frame_fill(self, theme: &Theme) -> Rgba {
        match self {
            Self::Opaque => theme.frame_fallback,
            Self::Blurred => theme.frame_surface,
        }
    }
}

pub(crate) fn panel_border(theme: &Theme, focus_visible: bool) -> Rgba {
    if focus_visible {
        theme.panel_focus_ring
    } else {
        theme.panel_border
    }
}

pub(crate) fn panel(
    id: &'static str,
    focus_handle: &FocusHandle,
    focus_visible: bool,
    theme: &Theme,
) -> Stateful<Div> {
    div()
        .id(id)
        .debug_selector(move || id.to_owned())
        .relative()
        .size_full()
        .bg(theme.panel_surface)
        .border_1()
        .border_color(panel_border(theme, focus_visible))
        .rounded(theme.radii.shell_panel)
        .overflow_hidden()
        .track_focus(focus_handle)
        .when(focus_visible, |panel| {
            panel.child(
                div()
                    .id(match id {
                        "shell-left-panel" => "shell-left-panel-focus-ring",
                        "shell-center-panel" => "shell-center-panel-focus-ring",
                        "shell-right-panel" => "shell-right-panel-focus-ring",
                        _ => "shell-settings-panel-focus-ring",
                    })
                    .debug_selector(move || format!("{id}-focus-ring"))
                    .absolute()
                    .inset_0()
                    .border_1()
                    .border_color(theme.panel_focus_ring)
                    .rounded(theme.radii.shell_panel),
            )
        })
}
```

If GPUI rejects a dynamic closure capture for `debug_selector`, keep the fixed `.id(id)` as the test locator and remove only `debug_selector`; do not replace the semantic IDs with per-call string allocation.

- [ ] **Step 4: Run GREEN and the app crate tests**

```bash
cd rust && cargo test -p tiller shell_chrome::tests
cd rust && cargo test -p tiller
```

Expected: all shell-chrome tests and the existing app tests pass.

- [ ] **Step 5: Commit**

```bash
git add rust/crates/tiller/src/shell_chrome.rs rust/crates/tiller/src/main.rs
git commit -m "feat: add shell chrome material and panel primitive"
```

### Task 3: Wire persisted and live translucency into the window

**Files:**
- Modify: `rust/crates/tiller/src/main.rs`

**Produces:**
- Initial `WindowOptions.window_background` derived from the saved setting.
- `TillerWorkspace::translucency_enabled` for frame painting.
- `WorkspaceAction::SetTranslucency(bool)` so Settings changes update the live window through the existing queue.

- [ ] **Step 1: Add failing pure and integration tests**

In `main.rs`'s tests, add a constructor-level test using `palette_test_workspace`:

```rust
#[gpui::test]
async fn applying_translucency_updates_workspace_and_window_material(
    cx: &mut gpui::TestAppContext,
) {
    let window = cx.add_window(|_window, cx| palette_test_workspace(cx));
    let mut cx = gpui::VisualTestContext::from_window(window.into(), cx);
    let workspace = cx.update(|window, _| {
        window
            .root::<TillerWorkspace>()
            .flatten()
            .expect("workspace root")
    });
    workspace.update_in(&mut cx, |workspace, window, cx| {
        workspace.apply_translucency(true, window, cx);
        assert!(workspace.translucency_enabled);
    });
}
```

Wire the existing `palette_test_workspace_with_tab_count` Settings fixture through its local `pending_actions` queue:

```rust
let pending_for_settings_change = pending_actions.clone();
let settings = cx.new(|cx| {
    Settings::new(cx).on_change(move |snapshot| {
        if let Ok(mut actions) = pending_for_settings_change.lock() {
            actions.push(WorkspaceAction::SetTranslucency(snapshot.translucency));
        }
    })
});
```

Then add an end-to-end test proving the real Settings control crosses that queue:

```rust
#[gpui::test]
async fn settings_translucency_toggle_reaches_the_live_workspace(
    cx: &mut gpui::TestAppContext,
) {
    cx.set_global(Theme::dark());
    let window = cx.add_window(|_window, cx| palette_test_workspace(cx));
    let mut cx = gpui::VisualTestContext::from_window(window.into(), cx);
    let workspace = cx.update(|window, _| {
        window
            .root::<TillerWorkspace>()
            .flatten()
            .expect("workspace root")
    });
    workspace.update(&mut cx.cx, |workspace, cx| {
        workspace.open_settings(Some(SettingsCategory::Appearance), cx)
    });
    cx.run_until_parked();

    let toggle = cx
        .debug_bounds("appearance-translucency")
        .expect("translucency toggle");
    cx.simulate_click(toggle.center(), gpui::Modifiers::none());
    cx.background_executor.advance_clock(Duration::from_millis(50));
    cx.run_until_parked();

    assert!(workspace.read_with(&cx.cx, |workspace, _| {
        workspace.translucency_enabled
    }));
}
```

Also extend the existing snapshot conversion test so both directions assert the already-persisted `translucency` field remains unchanged. This guards against accidentally introducing a second setting.

- [ ] **Step 2: Run RED**

```bash
cd rust && cargo test -p tiller applying_translucency_updates_workspace_and_window_material
```

Expected: compilation fails because `TillerWorkspace::translucency_enabled`, `apply_translucency`, and `WorkspaceAction::SetTranslucency` do not exist.

- [ ] **Step 3: Add workspace state and the action**

Add the field to `TillerWorkspace`:

```rust
translucency_enabled: bool,
```

Add the argument immediately before `cx` in `TillerWorkspace::new`, set it in `Self`, and pass `false` from test fixtures unless a test explicitly exercises translucency.

Add this variant to the existing `WorkspaceAction` enum:

```rust
SetTranslucency(bool),
```

Add the workspace method:

```rust
fn apply_translucency(
    &mut self,
    enabled: bool,
    window: &mut Window,
    cx: &mut Context<Self>,
) {
    self.translucency_enabled = enabled;
    window.set_background_appearance(
        shell_chrome::current_platform_material(enabled).window_background(),
    );
    cx.notify();
}
```

Handle it in the existing action-drain match:

```rust
WorkspaceAction::SetTranslucency(enabled) => {
    workspace.apply_translucency(enabled, window, cx);
}
```

- [ ] **Step 4: Wire startup and the Settings callback**

Before `cx.open_window`, capture the initial value without moving the full snapshot:

```rust
let initial_translucency = settings_snapshot.translucency;
```

Import `WindowBackgroundAppearance` only through `shell_chrome`; set the initial `WindowOptions` field:

```rust
WindowOptions {
    window_bounds: Some(WindowBounds::Windowed(bounds)),
    window_background: shell_chrome::current_platform_material(initial_translucency)
        .window_background(),
    titlebar: Some(TitlebarOptions {
        appears_transparent: true,
        traffic_light_position: Some(point(px(12.), px(12.))),
        ..Default::default()
    }),
    ..Default::default()
}
```

Capture a dedicated queue clone before building `Settings`:

```rust
let pending_for_settings_change = pending_actions.clone();
```

Inside the existing Settings `on_change` callback, preserve all current socket, agent-color, and persistence work, then enqueue only the live flag:

```rust
let translucency = snapshot.translucency;
session_store_for_settings.save_settings(&app_settings_from_snapshot(snapshot));
if let Ok(mut actions) = pending_for_settings_change.lock() {
    actions.push(WorkspaceAction::SetTranslucency(translucency));
}
```

Pass `initial_translucency` to the production `TillerWorkspace::new` call. Do not change `Settings::set_translucency`, `SettingsSnapshot`, `AppSettings`, or the existing conversion functions beyond test coverage; they already persist the correct key.

- [ ] **Step 5: Run GREEN and relevant existing tests**

```bash
cd rust && cargo test -p tiller applying_translucency_updates_workspace_and_window_material
cd rust && cargo test -p tiller settings_translucency_toggle_reaches_the_live_workspace
cd rust && cargo test -p tiller settings_snapshot
cd rust && cargo test -p tiller_ui appearance_controls_drive_theme_translucency_and_font_size
cd rust && cargo test -p tiller
```

Expected: all commands pass with non-zero test counts.

- [ ] **Step 6: Commit**

```bash
git add rust/crates/tiller/src/main.rs
git commit -m "feat: apply shell translucency at startup and live"
```

### Task 4: Replace seams with the inset three-panel shell

**Files:**
- Modify: `rust/crates/tiller/src/main.rs`

**Produces:**
- A 4 px outer inset around the work area.
- A 4 px gap between visible left/center/right panels.
- 7 px rounded clipped panels with one-pixel outlines.
- Center expansion when either sidebar is hidden, without losing the frame inset or center radius.

- [ ] **Step 1: Replace the stale seam test with failing shell geometry tests**

Delete `seam_width_matches_reference_divider`. Add a drawn test using the existing `palette_test_workspace` and selectors from `shell_chrome::panel`:

```rust
#[gpui::test]
async fn workspace_draws_three_inset_panels_with_compact_gaps(
    cx: &mut gpui::TestAppContext,
) {
    cx.set_global(Theme::dark());
    let window = cx.add_window(|_window, cx| palette_test_workspace(cx));
    let mut cx = gpui::VisualTestContext::from_window(window.into(), cx);
    cx.run_until_parked();

    let work = cx.debug_bounds("shell-work-area").expect("work area");
    let left = cx.debug_bounds("shell-left-panel").expect("left panel");
    let center = cx.debug_bounds("shell-center-panel").expect("center panel");
    let right = cx.debug_bounds("shell-right-panel").expect("right panel");

    assert_eq!(left.left() - work.left(), px(4.0));
    assert_eq!(center.left() - left.right(), px(4.0));
    assert_eq!(right.left() - center.right(), px(4.0));
    assert_eq!(work.right() - right.right(), px(4.0));
    assert_eq!(left.top() - work.top(), px(4.0));
    assert_eq!(work.bottom() - left.bottom(), px(4.0));
}
```

Add a second drawn test:

```rust
#[gpui::test]
async fn hiding_sidebars_expands_center_but_keeps_its_inset(
    cx: &mut gpui::TestAppContext,
) {
    cx.set_global(Theme::dark());
    let window = cx.add_window(|_window, cx| palette_test_workspace(cx));
    let mut cx = gpui::VisualTestContext::from_window(window.into(), cx);
    cx.run_until_parked();
    let workspace = cx.update(|window, _| {
        window
            .root::<TillerWorkspace>()
            .flatten()
            .expect("workspace root")
    });
    let initial = cx.debug_bounds("shell-center-panel").expect("center panel");

    workspace.update(&mut cx.cx, |workspace, cx| {
        workspace.sidebar_visible = false;
        workspace.right_panel_visible = false;
        cx.notify();
    });
    cx.run_until_parked();

    assert!(cx.debug_bounds("shell-left-panel").is_none());
    assert!(cx.debug_bounds("shell-right-panel").is_none());
    let work = cx.debug_bounds("shell-work-area").expect("work area");
    let center = cx.debug_bounds("shell-center-panel").expect("center panel");
    assert!(center.size.width > initial.size.width);
    assert_eq!(center.left() - work.left(), px(4.0));
    assert_eq!(work.right() - center.right(), px(4.0));
}
```

Use the repository's actual field names for sidebar visibility if they differ; do not trigger the public toggle actions when the test is intended to isolate geometry.

- [ ] **Step 2: Run RED**

```bash
cd rust && cargo test -p tiller workspace_draws_three_inset_panels_with_compact_gaps
```

Expected: failure because the `shell-*` selectors do not exist and the old seams consume 6 px.

- [ ] **Step 3: Add panel focus handles to `TillerWorkspace`**

Add:

```rust
left_panel_focus: FocusHandle,
center_panel_focus: FocusHandle,
right_panel_focus: FocusHandle,
settings_panel_focus: FocusHandle,
```

Initialize each once in `TillerWorkspace::new` with `cx.focus_handle()`. These are focus-containment handles for the panel wrappers; they do not replace the children’s existing focus handles.

- [ ] **Step 4: Replace seam composition with shell composition**

Remove `SEAM_WIDTH`, `fn seam`, and every width calculation that subtracts a seam. Keep `SIDEBAR_WIDTH` and `RIGHT_PANEL_WIDTH` unchanged.

In `columns`, compute focus visibility once:

```rust
let left_focus_visible = shell_chrome::focus_is_keyboard_visible(
    &self.left_panel_focus,
    window,
    cx,
);
let center_focus_visible = shell_chrome::focus_is_keyboard_visible(
    &self.center_panel_focus,
    window,
    cx,
);
let right_focus_visible = shell_chrome::focus_is_keyboard_visible(
    &self.right_panel_focus,
    window,
    cx,
);
```

Build the work area as one padded row:

```rust
div()
    .id("shell-work-area")
    .debug_selector(|| "shell-work-area".into())
    .flex()
    .flex_row()
    .flex_1()
    .min_h_0()
    .w_full()
    .p(theme.spacing.shell_outer_inset)
    .gap(theme.spacing.shell_gap)
    .when(self.sidebar_visible, |row| {
        row.child(
            shell_chrome::panel(
                "shell-left-panel",
                &self.left_panel_focus,
                left_focus_visible,
                theme,
            )
            .w(px(SIDEBAR_WIDTH))
            .flex_none()
            .child(self.sidebar.clone()),
        )
    })
    .child(
        shell_chrome::panel(
            "shell-center-panel",
            &self.center_panel_focus,
            center_focus_visible,
            theme,
        )
        .flex_1()
        .min_w_0()
        .child(center_column),
    )
    .when(self.right_panel_visible, |row| {
        row.child(
            shell_chrome::panel(
                "shell-right-panel",
                &self.right_panel_focus,
                right_focus_visible,
                theme,
            )
            .w(px(RIGHT_PANEL_WIDTH))
            .flex_none()
            .child(self.right_panel.clone()),
        )
    })
```

Make the center column `size_full().min_w_0().min_h_0()` and let flex determine its height. Remove the manual `viewport - TITLE_BAR_HEIGHT - STATUS_BAR_HEIGHT - TAB_BAR_HEIGHT` surface height; the root stack already owns those regions.

Update `tab_strip_available_width` to subtract only visible fixed panel widths, the visible gaps, and twice `shell_outer_inset`. Encode the arithmetic in a helper with a unit test for all four left/right visibility combinations so overflow-menu behavior does not regress.

- [ ] **Step 5: Run GREEN and existing layout/overflow tests**

```bash
cd rust && cargo test -p tiller workspace_draws_three_inset_panels_with_compact_gaps
cd rust && cargo test -p tiller hiding_sidebars_expands_center_but_keeps_its_inset
cd rust && cargo test -p tiller tab_strip_available_width
cd rust && cargo test -p tiller tab_overflow
cd rust && cargo test -p tiller
```

Expected: all focused tests and the full app crate pass.

- [ ] **Step 6: Commit**

```bash
git add rust/crates/tiller/src/main.rs
git commit -m "feat: compose workspace as rounded shell panels"
```

### Task 5: Integrate titlebar, status bar, and Settings into the frame

**Files:**
- Modify: `rust/crates/tiller/src/main.rs`
- Modify: `rust/crates/tiller_ui/src/titlebar.rs`
- Modify: `rust/crates/tiller_ui/src/status_bar.rs`
- Modify: `rust/crates/tiller_ui/src/controls.rs`
- Modify: `rust/crates/tiller_ui/src/settings.rs` only if a stable Settings root selector is absent

**Produces:**
- Titlebar and status bar painted by the outer frame.
- Settings as one rounded shell panel between those bars.
- Raised opaque status tooltip and Settings cards.

- [ ] **Step 1: Add failing drawn Settings/frame tests**

Extend the existing `ctrl_comma_opens_settings_surface` coverage or add:

```rust
#[gpui::test]
async fn settings_uses_one_shell_panel_and_keeps_both_frame_bars(
    cx: &mut gpui::TestAppContext,
) {
    cx.set_global(Theme::dark());
    let window = cx.add_window(|_window, cx| palette_test_workspace(cx));
    let mut cx = gpui::VisualTestContext::from_window(window.into(), cx);
    let workspace = cx.update(|window, _| {
        window
            .root::<TillerWorkspace>()
            .flatten()
            .expect("workspace root")
    });
    workspace.update(&mut cx.cx, |workspace, cx| {
        workspace.open_settings(None, cx)
    });
    cx.run_until_parked();

    assert!(cx.debug_bounds("tiller-titlebar").is_some());
    assert!(cx.debug_bounds("tiller-status-bar").is_some());
    assert!(cx.debug_bounds("shell-settings-panel").is_some());
    assert!(cx.debug_bounds("shell-left-panel").is_none());
    assert!(cx.debug_bounds("shell-center-panel").is_none());
    assert!(cx.debug_bounds("shell-right-panel").is_none());
}
```

Add a lightweight UI crate test asserting `controls::card` draws with `theme.raised` in both appearances, updating the existing light-mode card assertion instead of duplicating it.

- [ ] **Step 2: Run RED**

```bash
cd rust && cargo test -p tiller settings_uses_one_shell_panel_and_keeps_both_frame_bars
```

Expected: failure because the current Settings branch omits the status bar and has no shell panel.

- [ ] **Step 3: Paint the root frame and integrate Settings**

At the beginning of `Render for TillerWorkspace`, resolve:

```rust
let material = shell_chrome::current_platform_material(self.translucency_enabled);
let frame_fill = material.frame_fill(theme);
```

Use `frame_fill` as the root background. Add stable IDs/selectors to the wrapper regions:

```rust
div().id("shell-frame").debug_selector(|| "shell-frame".into()).bg(frame_fill)
```

Keep the normal branch ordered as titlebar → work area → status bar. Change the Settings branch to the same order, with its middle region:

```rust
let settings_focus_visible = shell_chrome::focus_is_keyboard_visible(
    &self.settings_panel_focus,
    window,
    cx,
);

div()
    .id("shell-settings-work-area")
    .flex_1()
    .min_h_0()
    .p(theme.spacing.shell_outer_inset)
    .child(
        shell_chrome::panel(
            "shell-settings-panel",
            &self.settings_panel_focus,
            settings_focus_visible,
            theme,
        )
        .child(self.settings.clone()),
    )
```

Then append `self.status_bar.clone()` before the existing root-level overlays. Settings remains a single panel; do not split its internal category navigation/content into shell panels.

- [ ] **Step 4: Make frame bars transparent and secondary surfaces opaque**

In `titlebar.rs`, change only the root bar fill:

```rust
.bg(gpui::transparent_black())
```

In `status_bar.rs`, give the root a stable selector and transparent fill:

```rust
.id("tiller-status-bar")
.debug_selector(|| "tiller-status-bar".into())
.bg(gpui::transparent_black())
```

Keep `StatusBarTooltip` opaque by changing its background to `theme.raised` rather than inheriting the now-transparent bar.

In `controls.rs`, change `card` from `theme.cosmic.containers.secondary.base` to:

```rust
.bg(theme.raised)
```

Do not make popovers, menus, tooltips, composer fields, or Settings cards translucent; only the outer frame carries material.

- [ ] **Step 5: Run GREEN and interaction regressions**

```bash
cd rust && cargo test -p tiller settings_uses_one_shell_panel_and_keeps_both_frame_bars
cd rust && cargo test -p tiller ctrl_comma_opens_settings_surface
cd rust && cargo test -p tiller_ui controls
cd rust && cargo test -p tiller_ui status_bar
cd rust && cargo test -p tiller_ui titlebar
cd rust && cargo test -p tiller_ui
```

Expected: all commands pass; Settings Escape/back behavior remains covered by the existing focused-surface tests.

- [ ] **Step 6: Commit**

```bash
git add rust/crates/tiller/src/main.rs rust/crates/tiller_ui/src/titlebar.rs rust/crates/tiller_ui/src/status_bar.rs rust/crates/tiller_ui/src/controls.rs rust/crates/tiller_ui/src/settings.rs
git commit -m "feat: integrate bars and settings into shell frame"
```

If `settings.rs` did not need a selector/test adjustment, omit it from `git add`.

### Task 6: Prove keyboard-only focus treatment and overlay safety

**Files:**
- Modify: `rust/crates/tiller/src/main.rs`
- Modify: `rust/crates/tiller/src/shell_chrome.rs` only if the test exposes an issue

**Produces:**
- Focus ring appears when focus is inside a panel and the last input was keyboard.
- Mouse focus does not leave a permanent outline.
- Context menus, tab overflow, command palette, toasts, and drag overlays remain root overlays and are not clipped by rounded panels.

- [ ] **Step 1: Add failing keyboard-focus integration coverage**

Use the existing sidebar focus fixture/action to move focus into the left panel. Add a drawn test that first simulates a keyboard event and then checks for `shell-left-panel-focus-ring`. Next simulate a mouse click in the same panel and assert the focus-ring selector disappears while `shell-left-panel` remains.

The key assertion is:

```rust
assert!(cx.debug_bounds("shell-left-panel-focus-ring").is_some());
// after mouse input and a parked frame
assert!(cx.debug_bounds("shell-left-panel-focus-ring").is_none());
```

Use the existing `VisualTestContext` input helpers already present in `main.rs`; do not mutate `last_input_was_keyboard` directly.

- [ ] **Step 2: Run RED**

```bash
cd rust && cargo test -p tiller shell_panel_focus_ring_is_keyboard_only
```

Expected: the first focus-ring selector is absent until the tracked-focus wrapper is correctly placed around the panel's interactive descendants.

- [ ] **Step 3: Correct focus containment without stealing focus**

Ensure each `shell_chrome::panel` wrapper calls `.track_focus(&panel_focus_handle)` and remains the parent of the existing child entity. Do not call `.focus()` on panel wrappers and do not replace child focus handles. Keep the decision exactly:

```rust
window.last_input_was_keyboard() && focus_handle.contains_focused(window, cx)
```

This preserves keyboard navigation and prevents a pointer click from leaving a permanent accent outline.

- [ ] **Step 4: Add/extend overlay regression assertions**

Run existing command-palette, tab-context-menu, overflow-menu, toast, and row-drag tests. Where an existing test checks only state, add one selector/bounds assertion proving the overlay is present after the panel wrappers gain `overflow_hidden`.

Do not move overlays into panels. They must remain appended by the root render after the shell content, as they are today.

- [ ] **Step 5: Run GREEN**

```bash
cd rust && cargo test -p tiller shell_panel_focus_ring_is_keyboard_only
cd rust && cargo test -p tiller command_palette
cd rust && cargo test -p tiller context_menu
cd rust && cargo test -p tiller overflow
cd rust && cargo test -p tiller toast
cd rust && cargo test -p tiller row_drag
cd rust && cargo test -p tiller
```

Expected: all selected tests run with non-zero counts and pass.

- [ ] **Step 6: Commit**

```bash
git add rust/crates/tiller/src/main.rs rust/crates/tiller/src/shell_chrome.rs
git commit -m "test: cover shell focus and overlay behavior"
```

If no production correction was required, the commit may contain only `main.rs` test changes.

### Task 7: Visual verification and repository gates

**Files:**
- Modify only files already listed if verification exposes a defect.
- Do not add screenshot artifacts to the repository unless explicitly requested.

- [ ] **Step 1: Run formatting and focused static checks**

```bash
cd rust && cargo fmt --all -- --check
cd rust && cargo clippy -p tiller_theme -p tiller_ui -p tiller --all-targets -- -D warnings
git diff --check
```

Expected: all commands exit 0.

- [ ] **Step 2: Run the required repository gate**

```bash
Scripts/ci.sh
```

Expected: exit 0 and final output contains exactly `CI OK`.

If a workspace-wide test flakes, follow `Scripts/ci-linux.sh`'s documented per-crate fallback and report both the original failure and the successful scoped rerun; do not call the gate green unless `Scripts/ci.sh` itself ultimately prints `CI OK`.

- [ ] **Step 3: Run the stricter platform gate**

```bash
Scripts/ci-linux.sh
```

Expected: all mandatory stages pass. Report any script-declared `SKIP` or `BLOCKED` stage verbatim rather than treating it as a pass.

- [ ] **Step 4: Launch and visually inspect both themes**

```bash
Scripts/build-dev.sh
```

Verify in the real app, at minimum:

- dark workspace with both sidebars visible;
- light workspace with both sidebars visible;
- left sidebar hidden, right sidebar hidden, and both hidden;
- Settings open;
- translucency off and on;
- keyboard focus moved among left, center, right, and Settings panels;
- tab context menu, overflow menu, command palette, tooltip, and toast above rounded clipping;
- terminal ANSI colors and code syntax colors unchanged.

On macOS/Windows, confirm the outer frame reveals a blurred native backdrop while panels stay opaque. On Linux/FreeBSD, confirm the frame is the documented opaque fallback and still retains the same palette/insets/radii.

- [ ] **Step 5: Check exact scope and commit final verification fixes**

```bash
git status --short
git diff --stat HEAD~7..HEAD
git diff --check
```

Expected: only the files listed in this plan are changed; `.idea/` remains untracked and unstaged.

If verification required code changes, rerun the focused test that caught the issue plus `Scripts/ci.sh`, then commit:

```bash
git add rust/crates/tiller_theme/src/lib.rs docs/linux-rewrite/THEME-PROVENANCE.md rust/crates/tiller/src/shell_chrome.rs rust/crates/tiller/src/main.rs rust/crates/tiller_ui/src/titlebar.rs rust/crates/tiller_ui/src/status_bar.rs rust/crates/tiller_ui/src/controls.rs rust/crates/tiller_ui/src/settings.rs
git commit -m "fix: polish IntelliJ shell visual integration"
```

Do not create an empty verification commit.

## Completion evidence to report

- Every focused RED command and its expected failure reason.
- Every focused GREEN command and non-zero test count.
- `Scripts/ci.sh` exit result and the literal `CI OK` marker.
- `Scripts/ci-linux.sh` result, including explicit `SKIP`/`BLOCKED` stages.
- Visual checks actually performed, separated by theme/platform/translucency state.
- Final changed-file list and commit range.
- Any limitation that remains, especially compositor-dependent blur or visual states not exercised.
