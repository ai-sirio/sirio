# Resizable Shell Panels Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the left sidebar and the right panel resizable by dragging their inner edge, with the chosen widths persisted across restarts.

**Architecture:** Live width lives in `TillerWorkspace` as two `f32` fields, seeded at startup from `AppSettings`. All contention logic (who yields when the window is too narrow) lives in one pure function in a new `panel_layout` module with no gpui dependency. The drag handle is an absolutely-positioned strip out of layout flow, so the existing shell-geometry assertions keep holding. Persistence is debounced 500 ms with a gpui timer.

**Tech Stack:** Rust, [gpui](https://github.com/zed-industries/zed), rusqlite (key-value settings table).

**Spec:** `docs/superpowers/specs/2026-08-23-panel-resize-design.md`

## Global Constraints

- Verification gate: `Scripts/ci.sh` must print `CI OK` before any task is considered done. Per-crate iteration: `cd rust && cargo test -p <crate>`.
- Commit messages: Conventional Commits, lower-case imperative subject.
- Tests first, standard Rust `#[test]`.
- Crate layering is one-way: `tiller` → `tiller_persistence`. `tiller_persistence` must never reference anything from `tiller`.
- Settings ranges: sidebar `160..=480`, right panel `220..=640`. Defaults: sidebar `325`, right panel `405`. These exact numbers appear in three places by design (struct default, load-time clamp fallback, range constant) — that is the existing convention for every other setting, not an accident to fix.
- `MIN_CENTER_WIDTH = 320.0`.
- Panel widths persist as `i64` pixels. Every other numeric key in `AppSettings` is `i64`.

---

### Task 1: `panel_layout` module — the pure width resolver

This is the whole feature's logic, with no window to drive it. Everything later is wiring.

**Files:**
- Create: `rust/crates/tiller/src/panel_layout.rs`
- Modify: `rust/crates/tiller/src/main.rs` — add `mod panel_layout;` beside `mod shell_chrome;` (line 74)
- Test: same file, `#[cfg(test)] mod tests`

**Interfaces:**
- Consumes: nothing.
- Produces: `PanelSide` (`Left`/`Right`, `Copy`), `MIN_CENTER_WIDTH: f32`, `PanelSide::range() -> (f32, f32)`, and `resolve_panel_widths(viewport_width: f32, left_pref: Option<f32>, right_pref: Option<f32>, dragging: Option<PanelSide>, outer_inset: f32, gap: f32) -> (Option<f32>, Option<f32>)`.

- [ ] **Step 1: Write the failing tests**

Create `rust/crates/tiller/src/panel_layout.rs` containing only the test module plus the item declarations they reference, so the file parses but the behaviour is absent.

```rust
#[cfg(test)]
mod tests {
    use super::*;

    /// 1470 wide, both panels asking for their defaults: everything fits, so
    /// nothing is touched.
    #[test]
    fn preferences_survive_untouched_when_they_fit() {
        let (left, right) =
            resolve_panel_widths(1470.0, Some(325.0), Some(405.0), None, 4.0, 4.0);

        assert_eq!(left, Some(325.0));
        assert_eq!(right, Some(405.0));
    }

    /// The panel under the hand keeps exactly what the drag asked for; the
    /// other one gives up the difference. Moving the panel the user is *not*
    /// touching reads as a bug, so this is the rule that matters most.
    #[test]
    fn the_dragged_panel_keeps_its_width_and_the_other_yields() {
        let (left, right) = resolve_panel_widths(
            1000.0,
            Some(400.0),
            Some(400.0),
            Some(PanelSide::Left),
            4.0,
            4.0,
        );

        assert_eq!(left, Some(400.0), "the dragged panel is authoritative");
        // budget = 1000 - 8 - 8 - 320 = 664; the right panel takes 664 - 400.
        assert_eq!(right, Some(264.0));
    }

    /// No drag means the window shrank. Neither panel is "the one being
    /// touched", so they shrink together and their ratio survives.
    #[test]
    fn without_a_drag_both_shrink_proportionally() {
        let (left, right) =
            resolve_panel_widths(1000.0, Some(400.0), Some(400.0), None, 4.0, 4.0);

        assert_eq!(left, Some(332.0));
        assert_eq!(right, Some(332.0));
    }

    /// The yielding panel stops at the low end of its own settings range
    /// rather than collapsing to nothing.
    #[test]
    fn the_yielding_panel_stops_at_its_own_floor() {
        let (left, right) = resolve_panel_widths(
            800.0,
            Some(480.0),
            Some(400.0),
            Some(PanelSide::Left),
            4.0,
            4.0,
        );

        // budget = 800 - 8 - 8 - 320 = 464. The right panel's floor is 220,
        // so the left panel cannot hold more than 464 - 220 = 244.
        assert_eq!(left, Some(244.0));
        assert_eq!(right, Some(220.0));
    }

    /// A hidden panel is not competing: it contributes neither width nor a
    /// floor, and it stays hidden in the output.
    #[test]
    fn a_hidden_panel_neither_takes_space_nor_holds_a_floor() {
        let (left, right) = resolve_panel_widths(700.0, None, Some(640.0), None, 4.0, 4.0);

        assert_eq!(left, None, "a hidden panel stays hidden");
        // budget = 700 - 8 - 4 - 320 = 368
        assert_eq!(right, Some(368.0));
    }

    /// A window too small to honour both floors still has to draw something.
    /// The centre column is allowed below its minimum rather than the panels
    /// going to zero or negative.
    #[test]
    fn a_window_too_small_for_both_floors_still_returns_the_floors() {
        let (left, right) =
            resolve_panel_widths(400.0, Some(325.0), Some(405.0), None, 4.0, 4.0);

        assert_eq!(left, Some(160.0));
        assert_eq!(right, Some(220.0));
    }

}
```

`PanelSide::range` restates in `f32` what `settings_ranges` holds in `i64`, and those two numbers live in different crates. The test that pins them together is **Task 2, Step 8** — it cannot live here, because the constants it reads do not exist until Task 2 adds them, and this task has to end green.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd rust && cargo test -p tiller panel_layout`
Expected: FAIL to compile — `cannot find function resolve_panel_widths in this scope`, `cannot find type PanelSide in this scope`. Six tests in total once they compile.

- [ ] **Step 3: Write the implementation**

Put this above the test module in `rust/crates/tiller/src/panel_layout.rs`:

```rust
//! Width resolution for the two shell side panels.
//!
//! The persisted width of a panel is a *preference*, not the width that gets
//! drawn. What is drawn is that preference narrowed to the space actually
//! available. Keeping the two apart is what makes shrinking a window
//! non-destructive: the clamp is a projection applied at render time, never
//! written back, so re-widening the window restores the chosen width.
//!
//! Every rule about the two panels competing for space lives here, in a
//! function with no gpui types, no state and no window — which is the only
//! reason those rules are testable at all.

/// Which of the two side panels a gesture or a clamp is talking about.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PanelSide {
    Left,
    Right,
}

impl PanelSide {
    /// The panel's own settings range, mirroring
    /// `settings_ranges::SIDEBAR_WIDTH` / `RIGHT_PANEL_WIDTH` in
    /// `tiller_persistence`. Duplicated rather than imported because the
    /// persisted range is `i64` and layout arithmetic is `f32`; the two are
    /// pinned together by `ranges_match_the_persisted_settings_ranges`.
    pub(crate) fn range(self) -> (f32, f32) {
        match self {
            PanelSide::Left => (160.0, 480.0),
            PanelSide::Right => (220.0, 640.0),
        }
    }
}

/// Narrowest the centre column may get before the side panels start yielding.
/// `MIN_SPLIT_PANE_SIZE` (160, `main.rs:3339`) is the comparable floor for
/// terminal splits; the centre column carries a whole tab strip above a
/// terminal, so it gets twice that.
pub(crate) const MIN_CENTER_WIDTH: f32 = 320.0;

/// Narrows each panel's preferred width to what the viewport can actually
/// give it. `None` means the panel is hidden, on the way in and on the way
/// out.
pub(crate) fn resolve_panel_widths(
    viewport_width: f32,
    left_pref: Option<f32>,
    right_pref: Option<f32>,
    dragging: Option<PanelSide>,
    outer_inset: f32,
    gap: f32,
) -> (Option<f32>, Option<f32>) {
    let visible = usize::from(left_pref.is_some()) + usize::from(right_pref.is_some());
    let budget =
        viewport_width - (2.0 * outer_inset) - (gap * visible as f32) - MIN_CENTER_WIDTH;

    let left = left_pref.unwrap_or(0.0);
    let right = right_pref.unwrap_or(0.0);
    // A hidden panel holds no floor: it is not in the competition at all.
    let left_floor = left_pref.map_or(0.0, |_| PanelSide::Left.range().0);
    let right_floor = right_pref.map_or(0.0, |_| PanelSide::Right.range().0);

    if left + right <= budget {
        return (left_pref, right_pref);
    }

    let (left, right) = match dragging {
        // The dragged panel is authoritative; the other absorbs the rest,
        // down to its own floor.
        Some(PanelSide::Left) => {
            let held = left.clamp(left_floor, (budget - right_floor).max(left_floor));
            (held, (budget - held).max(right_floor))
        }
        Some(PanelSide::Right) => {
            let held = right.clamp(right_floor, (budget - left_floor).max(right_floor));
            ((budget - held).max(left_floor), held)
        }
        // No drag: the window shrank under them. Neither is "the one being
        // touched", so they scale together and keep their ratio.
        None => {
            let requested = left + right;
            let scale = if requested > 0.0 { budget / requested } else { 0.0 };
            (
                (left * scale).max(left_floor),
                (right * scale).max(right_floor),
            )
        }
    };

    (left_pref.map(|_| left), right_pref.map(|_| right))
}
```

- [ ] **Step 4: Register the module**

In `rust/crates/tiller/src/main.rs`, beside `mod shell_chrome;` (line 74):

```rust
mod panel_layout;
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cd rust && cargo test -p tiller panel_layout`
Expected: PASS, 6 tests.

If `without_a_drag_both_shrink_proportionally` is off by a fraction, do not loosen the assertion — recompute it by hand. budget = 1000 − 8 − 8 − 320 = 664; requested = 800; scale = 0.83; 400 × 0.83 = 332.

- [ ] **Step 6: Commit**

```bash
git add rust/crates/tiller/src/panel_layout.rs rust/crates/tiller/src/main.rs
git commit -m "feat(shell): add the pure panel-width resolver"
```

---

### Task 2: Persist the two widths

**Files:**
- Modify: `rust/crates/tiller_persistence/src/model.rs:347` (`AppSettings` fields), `:392` (`Default`), `:420` (`settings_keys`), `:442` (`settings_ranges`)
- Modify: `rust/crates/tiller_persistence/src/db.rs:900` (`settings()`), `:982` (`save_settings`)
- Test: `rust/crates/tiller_persistence/tests/persistence_integration.rs`

**Interfaces:**
- Consumes: nothing from Task 1.
- Produces: `AppSettings::sidebar_width: i64` and `AppSettings::right_panel_width: i64`, populated by `AppDatabase::settings()` and written by `AppDatabase::save_settings()`.

- [ ] **Step 1: Write the failing test**

Append to `rust/crates/tiller_persistence/tests/persistence_integration.rs`:

```rust
/// The panel widths are Linux-rewrite-only keys with no Swift antecedent.
/// They round-trip, clamp into their ranges, and fall back to the drawn
/// geometry (325 / 405) when never written — which is what makes wiring them
/// visually a no-op on first launch.
#[test]
fn panel_widths_round_trip_and_clamp_into_their_ranges() {
    let dir = TempDir::new();
    let path = dir.db_path("panel-widths");

    {
        let db = AppDatabase::open(&path).expect("open");
        let fresh = db.settings().expect("load defaults");
        assert_eq!(fresh.sidebar_width, 325, "matches the geometry drawn today");
        assert_eq!(fresh.right_panel_width, 405, "matches the geometry drawn today");

        db.save_settings(&AppSettings {
            sidebar_width: 900,     // above the 160...480 range
            right_panel_width: 100, // below the 220...640 range
            ..AppSettings::default()
        })
        .expect("save");
    }

    let db = AppDatabase::open(&path).expect("reopen");
    let settings = db.settings().expect("load");
    assert_eq!(settings.sidebar_width, 480, "clamped to the upper bound");
    assert_eq!(settings.right_panel_width, 220, "clamped to the lower bound");

    db.save_settings(&AppSettings {
        sidebar_width: 300,
        right_panel_width: 500,
        ..settings
    })
    .expect("save in-range values");
    let settings = db.settings().expect("reload");
    assert_eq!(settings.sidebar_width, 300, "an in-range width survives verbatim");
    assert_eq!(settings.right_panel_width, 500);
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cd rust && cargo test -p tiller_persistence panel_widths_round_trip`
Expected: FAIL to compile — `struct AppSettings has no field named sidebar_width`.

- [ ] **Step 3: Add the fields and their default**

In `rust/crates/tiller_persistence/src/model.rs`, at the end of `pub struct AppSettings` (after `pub translucency: bool,`):

```rust
    /// "appearance.sidebarWidth" — default 325, clamped to 160...480. A
    /// Linux-rewrite-only key: the Swift app's sidebar was a fixed width, so
    /// unlike every other key here this one has no `@AppStorage` antecedent.
    pub sidebar_width: i64,
    /// "appearance.rightPanelWidth" — default 405, clamped to 220...640.
    /// Linux-rewrite-only for the same reason as `sidebar_width`.
    pub right_panel_width: i64,
```

In `impl Default for AppSettings` (`model.rs:392`), after `translucency: false,`:

```rust
            sidebar_width: 325,
            right_panel_width: 405,
```

- [ ] **Step 4: Add the key and range constants**

In `pub mod settings_keys` (`model.rs:420`), after `TRANSLUCENCY`:

```rust
    /// Linux-rewrite-only: no Swift antecedent (the Swift sidebar was fixed).
    pub const SIDEBAR_WIDTH: &str = "appearance.sidebarWidth";
    /// Linux-rewrite-only: no Swift antecedent.
    pub const RIGHT_PANEL_WIDTH: &str = "appearance.rightPanelWidth";
```

In `pub mod settings_ranges` (`model.rs:442`), after `REFRESH_INTERVAL_MIN`:

```rust
    /// Linux-rewrite-only; no Swift range to mirror.
    pub const SIDEBAR_WIDTH: std::ops::RangeInclusive<i64> = 160..=480;
    /// Linux-rewrite-only; no Swift range to mirror.
    pub const RIGHT_PANEL_WIDTH: std::ops::RangeInclusive<i64> = 220..=640;
```

- [ ] **Step 5: Read them on load**

In `AppDatabase::settings()` (`db.rs:900`), beside the other `clamp_setting` calls:

```rust
        if let Some(value) = self.setting_value(settings_keys::SIDEBAR_WIDTH)? {
            defaults.sidebar_width =
                clamp_setting(&value, crate::model::settings_ranges::SIDEBAR_WIDTH, 325);
        }
        if let Some(value) = self.setting_value(settings_keys::RIGHT_PANEL_WIDTH)? {
            defaults.right_panel_width = clamp_setting(
                &value,
                crate::model::settings_ranges::RIGHT_PANEL_WIDTH,
                405,
            );
        }
```

- [ ] **Step 6: Write them on save**

In `AppDatabase::save_settings()` (`db.rs:982`), beside the other `set_setting` calls:

```rust
        set_setting(
            &transaction,
            settings_keys::SIDEBAR_WIDTH,
            &settings.sidebar_width.to_string(),
        )?;
        set_setting(
            &transaction,
            settings_keys::RIGHT_PANEL_WIDTH,
            &settings.right_panel_width.to_string(),
        )?;
```

- [ ] **Step 7: Repair the `AppSettings` literal — and stop it eating the widths**

`app_settings_from_snapshot` (`main.rs:12838`) builds `AppSettings { … }` field by field and closes on `translucency: snapshot.translucency,` with **no** `..Default::default()`. Two new fields break it, so this task owns the repair or the tree does not compile.

Do **not** repair it with `..AppSettings::default()`. That would compile and introduce a worse bug than the one it fixes: `SettingsSnapshot` has no width fields, so every save from the Settings UI would write the default widths back and silently undo whatever the user had dragged. The panel widths are owned by the drag, not by the settings sheet, and the seam has to say so.

Add to the literal (`main.rs:12865`, after `translucency`):

```rust
        // Not in the Settings UI snapshot: the widths belong to the drag.
        // Callers must re-apply the live values — see the `on_change`
        // handler below. Filling these from `Default` here would reset a
        // dragged panel every time any unrelated setting changed.
        sidebar_width: AppSettings::default().sidebar_width,
        right_panel_width: AppSettings::default().right_panel_width,
```

Then make the settings `on_change` handler (`main.rs:13194`) preserve what is already stored, instead of writing the placeholder through:

```rust
                            let stored = session_store_for_settings.load_settings();
                            let mut settings = app_settings_from_snapshot(snapshot);
                            settings.sidebar_width = stored.sidebar_width;
                            settings.right_panel_width = stored.right_panel_width;
                            session_store_for_settings.save_settings(&settings);
```

replacing the existing single `session_store_for_settings.save_settings(&app_settings_from_snapshot(snapshot));` call.

Then run `cd rust && cargo build --workspace` and add the two fields to any other bare `AppSettings { … }` literal the compiler points at — there is at least one more in the test module around `main.rs:18554`.

- [ ] **Step 8: (moved) — the cross-crate range test lives in Task 3, Step 8**

Nothing to do here. The test that pins `PanelSide::range` against `settings_ranges` needs files from both Task 1 and Task 2, so it belongs to the first task that already depends on both. Keeping it here would chain Task 2 behind Task 1 for no reason, and those two are otherwise independent — which matters if they are being executed in parallel.

For reference, the test it refers to (do **not** write it in this task):

```rust
    /// Two numbers, two crates, no compiler tying them together. This test is
    /// the only thing that notices when one moves without the other.
    #[test]
    fn panel_ranges_match_the_persisted_settings_ranges() {
        use tiller_persistence::settings_ranges;

        let (left_floor, left_ceiling) = PanelSide::Left.range();
        assert_eq!(left_floor, *settings_ranges::SIDEBAR_WIDTH.start() as f32);
        assert_eq!(left_ceiling, *settings_ranges::SIDEBAR_WIDTH.end() as f32);

        let (right_floor, right_ceiling) = PanelSide::Right.range();
        assert_eq!(
            right_floor,
            *settings_ranges::RIGHT_PANEL_WIDTH.start() as f32
        );
        assert_eq!(
            right_ceiling,
            *settings_ranges::RIGHT_PANEL_WIDTH.end() as f32
        );
    }
```

- [ ] **Step 9: Run the tests to verify they pass**

Run: `cd rust && cargo test -p tiller_persistence`
Expected: PASS. No database migration is needed — settings are key-value rows and an absent key falls through to its default, which the first assertion in the new persistence test pins.

Then `cd rust && cargo build --workspace` to confirm the `AppSettings` literal repairs in Step 7 left nothing broken.

- [ ] **Step 10: Commit**

```bash
git add rust/crates/tiller_persistence/src/model.rs rust/crates/tiller_persistence/src/db.rs rust/crates/tiller_persistence/tests/persistence_integration.rs rust/crates/tiller/src/main.rs
git commit -m "feat(persistence): persist the sidebar and right-panel widths"
```

---

### Task 3: Wire the widths into the shell and delete the constants

The task that actually changes what the user sees, and the one that closes the false-green hole.

**Files:**
- Modify: `rust/crates/tiller/src/main.rs:318-319` (delete both constants), `:332` (signature), `:3433` (new fields), `:3641` (constructor params), `:4100` (constructor body), `:10467-10512` (render), `:12944` (call site)
- Test: `rust/crates/tiller/src/main.rs:20476`, `:20498`, `:20509` (rewrite), plus one new test

**Interfaces:**
- Consumes: `panel_layout::{PanelSide, resolve_panel_widths}` (Task 1); `AppSettings::sidebar_width` / `right_panel_width` (Task 2).
- Produces: `TillerWorkspace::sidebar_width: f32`, `TillerWorkspace::right_panel_width: f32`, `TillerWorkspace::dragging_panel: Option<PanelSide>`, and `tab_strip_available_width_for_shell(viewport_width: f32, sidebar_width: Option<f32>, right_panel_width: Option<f32>, outer_inset: f32, gap: f32) -> f32`.

- [ ] **Step 1: Write the failing test**

Add to the test module in `rust/crates/tiller/src/main.rs`, beside the other shell-geometry tests:

```rust
/// The test whose absence let `right_panel_width` sit in the settings model
/// for months with a clamp, a test, and no reader: it compares the *drawn*
/// panel against the *configured* width. The pre-existing geometry tests
/// compare drawn bounds against the constant the render code itself reads,
/// so they cannot fail when the render ignores configuration — they assert
/// that 405 == 405.
#[gpui::test]
async fn a_configured_panel_width_is_the_width_actually_drawn(cx: &mut TestAppContext) {
    cx.set_global(Theme::dark());
    let window = cx.add_window(|_window, cx| palette_test_workspace(cx));
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    cx.run_until_parked();
    let workspace = cx.update(|window, _| {
        window
            .root::<TillerWorkspace>()
            .flatten()
            .expect("workspace root")
    });

    // 280 + 380 = 660, deliberately inside the test window's budget. The
    // gpui test window is 1024x768, so budget = 1024 - 8 - 8 - 320 = 688.
    // Asking for more (500 on the right, say) makes the resolver correctly
    // narrow the panels and this test then fails against perfectly good
    // wiring — a false red that looks exactly like a real one.
    workspace.update(&mut cx.cx, |workspace, cx| {
        workspace.sidebar_width = 280.0;
        workspace.right_panel_width = 380.0;
        cx.notify();
    });
    cx.run_until_parked();

    let left = cx.debug_bounds("shell-left-panel").expect("left panel");
    let right = cx.debug_bounds("shell-right-panel").expect("right panel");

    assert_eq!(left.size.width, px(280.0));
    assert_eq!(right.size.width, px(380.0));
}
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cd rust && cargo test -p tiller a_configured_panel_width_is_the_width_actually_drawn`
Expected: FAIL to compile — `no field sidebar_width on type TillerWorkspace`.

- [ ] **Step 3: Add the workspace fields**

In `struct TillerWorkspace` (`main.rs:3433`), after `right_panel_visible: bool,`:

```rust
    /// Preferred width, not the drawn width: `panel_layout::resolve_panel_widths`
    /// narrows both of these to what the viewport can give, and the narrowed
    /// value is deliberately never written back here.
    sidebar_width: f32,
    right_panel_width: f32,
    /// Which edge is under the hand right now. Gives that panel priority when
    /// the two compete for space, so the panel the user is *not* touching
    /// stays still.
    dragging_panel: Option<panel_layout::PanelSide>,
```

- [ ] **Step 4: Seed them through the constructor**

In `TillerWorkspace::new` (`main.rs:3641`), beside `translucency_enabled: bool,` — the existing precedent for a settings-derived scalar reaching the workspace:

```rust
        sidebar_width: f32,
        right_panel_width: f32,
```

In the struct literal (`main.rs:4100`), after `right_panel_visible: true,`:

```rust
            sidebar_width,
            right_panel_width,
            dragging_panel: None,
```

`TillerWorkspace::new` has **five** call sites, not one. Missing the four test fixtures leaves `cargo test -p tiller` unable to compile:

| `main.rs` line | what it is | pass |
|---|---|---|
| 13230 | production boot | `saved_settings.sidebar_width as f32`, `saved_settings.right_panel_width as f32` |
| 13921 | `palette_test_workspace_with_translucency` | `325.0`, `405.0` |
| 13995 | test fixture | `325.0`, `405.0` |
| 14083 | test fixture | `325.0`, `405.0` |
| 14184 | test fixture | `325.0`, `405.0` |

At the production site the values come from the settings already loaded at `main.rs:12944` (`saved_settings`):

```rust
            saved_settings.sidebar_width as f32,
            saved_settings.right_panel_width as f32,
```

The fixtures get the literals `325.0` and `405.0` — the same geometry they drew before this change, so every pre-existing geometry assertion keeps its old meaning. Locate all five with `grep -n "TillerWorkspace::new" rust/crates/tiller/src/main.rs` rather than trusting the line numbers above, which shift as earlier steps edit the file.

- [ ] **Step 5: Delete the constants and use the resolver in render**

Delete both lines at `main.rs:318-319`:

```rust
const SIDEBAR_WIDTH: f32 = 325.;
const RIGHT_PANEL_WIDTH: f32 = 405.;
```

They cannot become the defaults: `tiller_persistence` owns the defaults now and sits *below* `tiller` in the crate graph, so it could not read them. Keeping them would be a fourth copy of 405.

In the shell row render (`main.rs:10467`), resolve once before building the row:

```rust
        let (left_width, right_width) = panel_layout::resolve_panel_widths(
            // Same expression `tab_strip_available_width` already uses at
            // `main.rs:10116` — not `viewport_size()`.
            f32::from(window.bounds().size.width),
            self.sidebar_visible.then_some(self.sidebar_width),
            self.right_panel_visible.then_some(self.right_panel_width),
            self.dragging_panel,
            f32::from(theme.spacing.shell_outer_inset),
            f32::from(theme.spacing.shell_gap),
        );
```

Then replace `.w(px(SIDEBAR_WIDTH))` with `.w(px(left_width.unwrap_or(0.0)))` and `.w(px(RIGHT_PANEL_WIDTH))` with `.w(px(right_width.unwrap_or(0.0)))`.

- [ ] **Step 6: Change the tab-strip width calculation**

Replace `tab_strip_available_width_for_shell` (`main.rs:332`) wholesale:

```rust
/// Space the tab strip has left once the side panels, the gaps and the outer
/// inset are taken out. `None` means the panel is hidden — visibility and
/// width are one concept here, because a hidden panel subtracts neither a
/// width nor a gap.
///
/// The widths passed in must be the **rendered** ones from
/// `panel_layout::resolve_panel_widths`, never the preferences: when the
/// viewport clamp is active a preference is wider than what was actually
/// taken, and the tab strip would size itself against space that does not
/// exist.
fn tab_strip_available_width_for_shell(
    viewport_width: f32,
    sidebar_width: Option<f32>,
    right_panel_width: Option<f32>,
    outer_inset: f32,
    gap: f32,
) -> f32 {
    let fixed_panels = sidebar_width.unwrap_or(0.0) + right_panel_width.unwrap_or(0.0);
    let visible_gaps = usize::from(sidebar_width.is_some())
        + usize::from(right_panel_width.is_some());

    viewport_width - fixed_panels - (gap * visible_gaps as f32) - (2.0 * outer_inset)
}
```

Update its caller, `TillerWorkspace::tab_strip_available_width` (`main.rs:10114`), to resolve the widths the same way the render does and pass those instead of `self.sidebar_visible` / `self.right_panel_visible`:

```rust
    fn tab_strip_available_width(&self, window: &Window, theme: Theme) -> f32 {
        let (left_width, right_width) = panel_layout::resolve_panel_widths(
            f32::from(window.bounds().size.width),
            self.sidebar_visible.then_some(self.sidebar_width),
            self.right_panel_visible.then_some(self.right_panel_width),
            self.dragging_panel,
            f32::from(theme.spacing.shell_outer_inset),
            f32::from(theme.spacing.shell_gap),
        );
        tab_strip_available_width_for_shell(
            f32::from(window.bounds().size.width),
            left_width,
            right_width,
            f32::from(theme.spacing.shell_outer_inset),
            f32::from(theme.spacing.shell_gap),
        ) - f32::from(theme.spacing.titlebar_control_frame.width)
    }
```

- [ ] **Step 7: Fix the three tests that pinned the constants**

Rewrite `tab_strip_available_width_accounts_for_visible_shell_panels_and_gaps` (`main.rs:20509`) so the numbers are derived, not copied:

```rust
    #[test]
    fn tab_strip_available_width_accounts_for_visible_shell_panels_and_gaps() {
        let viewport_width = 1_000.0;
        let outer_inset = 4.0;
        let gap = 4.0;
        // Named rather than inlined: these used to be 405 and 325 in
        // disguise, which is exactly why the numbers below never moved when
        // the panels were meant to become resizable.
        let sidebar = 325.0;
        let right = 405.0;

        assert_eq!(
            tab_strip_available_width_for_shell(
                viewport_width, Some(sidebar), Some(right), outer_inset, gap
            ),
            viewport_width - sidebar - right - (2.0 * gap) - (2.0 * outer_inset),
        );
        assert_eq!(
            tab_strip_available_width_for_shell(
                viewport_width, Some(sidebar), None, outer_inset, gap
            ),
            viewport_width - sidebar - gap - (2.0 * outer_inset),
        );
        assert_eq!(
            tab_strip_available_width_for_shell(
                viewport_width, None, Some(right), outer_inset, gap
            ),
            viewport_width - right - gap - (2.0 * outer_inset),
        );
        assert_eq!(
            tab_strip_available_width_for_shell(viewport_width, None, None, outer_inset, gap),
            viewport_width - (2.0 * outer_inset),
        );
    }
```

In the two drawn-geometry tests, replace `px(SIDEBAR_WIDTH + 4.0)` (`main.rs:20478`) with `px(workspace_sidebar_width + 4.0)` and `px(RIGHT_PANEL_WIDTH + 4.0)` (`main.rs:20498`) with `px(workspace_right_panel_width + 4.0)`, reading both from the workspace before the visibility toggle:

```rust
        let (workspace_sidebar_width, workspace_right_panel_width) =
            workspace.read_with(&cx.cx, |workspace, _| {
                (workspace.sidebar_width, workspace.right_panel_width)
            });
```

- [ ] **Step 8: Pin the two ranges across the crate boundary**

`PanelSide::range` (Task 1) restates in `f32` what `settings_ranges` (Task 2) holds in `i64`. The duplication cannot be removed — layout arithmetic and persistence use different types, and `tiller_persistence` cannot depend on `tiller` — so make it noisy instead. This is the first task that has both halves in hand.

Append to the `tests` module of `rust/crates/tiller/src/panel_layout.rs`:

```rust
    /// Two numbers, two crates, no compiler tying them together. This test is
    /// the only thing that notices when one moves without the other.
    #[test]
    fn panel_ranges_match_the_persisted_settings_ranges() {
        use tiller_persistence::settings_ranges;

        let (left_floor, left_ceiling) = PanelSide::Left.range();
        assert_eq!(left_floor, *settings_ranges::SIDEBAR_WIDTH.start() as f32);
        assert_eq!(left_ceiling, *settings_ranges::SIDEBAR_WIDTH.end() as f32);

        let (right_floor, right_ceiling) = PanelSide::Right.range();
        assert_eq!(
            right_floor,
            *settings_ranges::RIGHT_PANEL_WIDTH.start() as f32
        );
        assert_eq!(
            right_ceiling,
            *settings_ranges::RIGHT_PANEL_WIDTH.end() as f32
        );
    }
```

`model` is a private module, but `tiller_persistence/src/lib.rs:55` already re-exports `settings_keys` and `settings_ranges` at the crate root — that is the import to use, not `tiller_persistence::model`.

- [ ] **Step 9: Run the tests to verify they pass**

Run: `cd rust && cargo test -p tiller`
Expected: PASS, including the new `a_configured_panel_width_is_the_width_actually_drawn` and `panel_ranges_match_the_persisted_settings_ranges`. `panel_layout` is now seven tests.

- [ ] **Step 10: Commit**

```bash
git add rust/crates/tiller/src/main.rs rust/crates/tiller/src/panel_layout.rs
git commit -m "feat(shell): drive panel widths from settings instead of constants"
```

---

### Task 4: Stop the right panel sizing itself

**Files:**
- Modify: `rust/crates/tiller_ui/src/right_panel/mod.rs:32` (delete `PANEL_WIDTH`), `:525` (render)
- Test: `rust/crates/tiller_ui/src/right_panel/mod.rs:720` (verify, adjust only if it fails)

**Interfaces:**
- Consumes: nothing.
- Produces: nothing. `RightPanel` becomes width-agnostic; its host sizes it.

- [ ] **Step 1: Delete the constant and fill the host instead**

Delete `main.rs`-independent duplicate at `right_panel/mod.rs:32`:

```rust
const PANEL_WIDTH: f32 = 405.0;
```

At `right_panel/mod.rs:525`, replace `.w(px(PANEL_WIDTH))` with:

```rust
            .w_full()
```

- [ ] **Step 2: Run the crate's tests**

Run: `cd rust && cargo test -p tiller_ui`
Expected: PASS.

The spec flagged a risk here worth checking rather than assuming: `the_history_view_renders_commit_rows_in_the_drawn_frame` (`:720`) makes `RightPanel` the window root via `cx.add_window(|_window, _cx| RightPanel::new(dir.0.clone()))` (`:729`), so `w_full` fills the test window and no fixture change should be needed. If that test fails on a zero width, wrap the panel in `div().w(px(405.0)).h_full().child(...)` inside the fixture — not in the production render.

- [ ] **Step 3: Commit**

```bash
git add rust/crates/tiller_ui/src/right_panel/mod.rs
git commit -m "refactor(right-panel): let the host own the panel width"
```

---

### Task 5: The drag handle

**Files:**
- Modify: `rust/crates/tiller/src/main.rs` — new `DraggedPanelEdge` beside `DraggedPaneDivider` (`:3332`), new `render_panel_resize_handle` and `update_panel_width` in the `impl TillerWorkspace` at `:10518`, handle children and drag listeners in the render at `:10467-10512`
- Test: `rust/crates/tiller/src/main.rs` test module

**Interfaces:**
- Consumes: `panel_layout::PanelSide`, `TillerWorkspace::{sidebar_width, right_panel_width, dragging_panel}` (Tasks 1 and 3).
- Produces: `TillerWorkspace::update_panel_width(&mut self, side: PanelSide, event: &DragMoveEvent<DraggedPanelEdge>, cx: &mut Context<Self>)`, and the drawn hit targets `panel-resize-left` / `panel-resize-right`.

- [ ] **Step 1: Write the failing test**

```rust
/// Dragging the right panel's inner edge leftwards makes it wider, because
/// that edge is its left border. The handle is drawn out of layout flow, so
/// this must not disturb the 4px shell gaps the geometry tests pin.
#[gpui::test]
async fn dragging_the_right_panel_edge_changes_its_drawn_width(cx: &mut TestAppContext) {
    cx.set_global(Theme::dark());
    let window = cx.add_window(|_window, cx| palette_test_workspace(cx));
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    cx.run_until_parked();
    let workspace = cx.update(|window, _| {
        window
            .root::<TillerWorkspace>()
            .flatten()
            .expect("workspace root")
    });

    // Start from widths that fit, so rendered == preferred and the assertion
    // below measures the drag rather than the viewport clamp. The gpui test
    // window is 1024 wide (budget 688); the shipped defaults are 325 + 405 =
    // 730, which the resolver would correctly narrow — and then `before`
    // would not be the preference this drag is moving.
    workspace.update(&mut cx.cx, |workspace, cx| {
        workspace.sidebar_width = 280.0;
        workspace.right_panel_width = 300.0;
        cx.notify();
    });
    cx.run_until_parked();

    let before = cx.debug_bounds("shell-right-panel").expect("right panel");
    let handle = cx.debug_bounds("panel-resize-right").expect("resize handle");
    let centre = cx.debug_bounds("shell-center-panel").expect("centre panel");
    assert_eq!(before.size.width, px(300.0), "baseline: nothing is clamped yet");

    // Hover first, as a real pointer would, then press.
    cx.simulate_event(MouseMoveEvent {
        position: handle.center(),
        pressed_button: None,
        modifiers: Modifiers::none(),
    });
    cx.run_until_parked();
    cx.simulate_event(MouseDownEvent {
        position: handle.center(),
        button: MouseButton::Left,
        modifiers: Modifiers::none(),
        click_count: 1,
        first_mouse: false,
    });
    // Two moves, not one. The first crosses GPUI's drag-start threshold and
    // is swallowed; only the second reaches `on_drag_move`. The codebase's
    // other drag tests (`drawn_divider_drag_leaves_pane_focus_untouched`,
    // `main.rs:16603`) carry the same comment — a single move here silently
    // asserts nothing.
    cx.simulate_event(MouseMoveEvent {
        position: point(handle.center().x - px(4.0), handle.center().y),
        pressed_button: Some(MouseButton::Left),
        modifiers: Modifiers::none(),
    });
    cx.simulate_event(MouseMoveEvent {
        position: point(handle.center().x - px(60.0), handle.center().y),
        pressed_button: Some(MouseButton::Left),
        modifiers: Modifiers::none(),
    });
    cx.run_until_parked();

    let after = cx.debug_bounds("shell-right-panel").expect("right panel");
    assert_eq!(
        after.size.width,
        before.size.width + px(60.0),
        "dragging the left border leftwards widens the right panel"
    );

    let centre_after = cx.debug_bounds("shell-center-panel").expect("centre panel");
    assert_eq!(
        after.left() - centre_after.right(),
        px(4.0),
        "the shell gap is untouched: the handle is out of layout flow"
    );
    assert!(centre_after.size.width < centre.size.width);
}
```

- [ ] **Step 2: Write the second failing test — focus survives the gesture**

GPUI's drag machinery clears focus after mouse-down. Without this test the
restore in `on_drop` is unverified, and a terminal silently loses keyboard
focus every time someone widens a panel.

```rust
/// Resizing a panel must not cost the focused pane its keyboard focus.
/// GPUI clears focus after mouse-down and a next-frame re-focus loses the
/// race, so `on_drop` is the only place the restore sticks — and this is what
/// proves it does.
#[gpui::test]
async fn dragging_a_panel_edge_leaves_the_focused_pane_focused(cx: &mut TestAppContext) {
    cx.set_global(Theme::dark());
    let window = cx.add_window(|_window, cx| palette_test_workspace(cx));
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    cx.run_until_parked();
    let workspace = cx.update(|window, _| {
        window
            .root::<TillerWorkspace>()
            .flatten()
            .expect("workspace root")
    });

    // Capture the focused pane's handle the same way
    // `drawn_divider_drag_leaves_pane_focus_untouched` (main.rs:16603) does,
    // and assert the baseline before touching anything: a test that cannot
    // observe a real focus change proves nothing when focus does not move.
    let focus = workspace.read_with(&cx.cx, |workspace, cx| {
        workspace.focused_pane_focus_handle(workspace.active_tab, cx)
    });
    cx.update(|window, _| {
        assert!(focus.is_focused(window), "baseline: the pane starts focused");
    });

    let handle = cx.debug_bounds("panel-resize-right").expect("resize handle");
    let start = handle.center();

    cx.simulate_event(MouseMoveEvent {
        position: start,
        pressed_button: None,
        modifiers: Modifiers::none(),
    });
    cx.run_until_parked();
    cx.simulate_event(MouseDownEvent {
        position: start,
        button: MouseButton::Left,
        modifiers: Modifiers::none(),
        click_count: 1,
        first_mouse: false,
    });
    cx.simulate_event(MouseMoveEvent {
        position: point(start.x - px(4.0), start.y),
        pressed_button: Some(MouseButton::Left),
        modifiers: Modifiers::none(),
    });
    cx.simulate_event(MouseMoveEvent {
        position: point(start.x - px(30.0), start.y),
        pressed_button: Some(MouseButton::Left),
        modifiers: Modifiers::none(),
    });
    cx.simulate_event(MouseUpEvent {
        position: point(start.x - px(30.0), start.y),
        button: MouseButton::Left,
        modifiers: Modifiers::none(),
        click_count: 1,
    });
    cx.run_until_parked();

    cx.update(|window, _| {
        assert!(
            focus.is_focused(window),
            "the drop must hand focus back to the pane"
        );
    });
    workspace.read_with(&cx.cx, |workspace, _| {
        assert_eq!(
            workspace.dragging_panel, None,
            "the drop clears drag priority, so the panels stop favouring one side"
        );
    });
}
```

If `TillerWorkspace` exposes no `focused_pane_focus_handle`, read the handle the
way `drawn_divider_drag_leaves_pane_focus_untouched` captures `focus_new` at
`main.rs:16614-16660` and use that expression instead. Do not add a production
accessor just for the test.

- [ ] **Step 3: Run both to verify they fail**

Run: `cd rust && cargo test -p tiller panel_edge`
Expected: FAIL — `debug_bounds("panel-resize-right")` returns `None`, so `.expect("resize handle")` panics in both.

- [ ] **Step 4: Add the drag payload**

Beside `DraggedPaneDivider` (`main.rs:3332`):

```rust
/// Payload for a panel-edge drag. Carries only which edge; the arithmetic
/// lives in `update_panel_width` against the anchor taken at mouse-down.
#[derive(Clone, Copy, Debug)]
struct DraggedPanelEdge {
    side: panel_layout::PanelSide,
}
```

Also add the anchor to `struct TillerWorkspace`, beside `dragging_panel` (added in Task 3):

```rust
    /// Pointer x and panel width at the moment the edge was grabbed.
    ///
    /// The width is derived as `grab_width ± (pointer − grab_x)` rather than
    /// from the pointer's absolute position, for a reason that is only 3px
    /// wide but visible: the grab strip is 6px, so the pointer starts at the
    /// panel edge *plus* wherever inside the strip it landed. Reading the
    /// absolute position makes the edge jump to the cursor on the first
    /// move. Anchoring is not the same as accumulating — every frame still
    /// measures from the grab, so a dropped frame cannot make the panel
    /// drift.
    panel_drag_anchor: Option<(f32, f32)>,
```

Initialise it as `panel_drag_anchor: None,` in the struct literal at `main.rs:4100`.

- [ ] **Step 5: Render the handle**

In the `impl TillerWorkspace` block at `main.rs:10518`:

```rust
    /// A 6px grab strip on the panel's inner edge, absolutely positioned and
    /// therefore out of layout flow.
    ///
    /// Out of flow is not a style choice. The shell row is
    /// `.flex().flex_row().gap(shell_gap)`, so a flex child here would put
    /// two gaps where there is one and break the five existing assertions on
    /// `right.left() - center.right() == px(4.0)`. Inside the panel rather
    /// than straddling its border, because `shell_chrome::panel` sets
    /// `.overflow_hidden()` and would clip anything hanging outside — which
    /// also means the handle travels with the panel and cannot drift out of
    /// the window if the window is resized mid-drag.
    ///
    /// Accepted cost: 6px of the adjacent content stops being clickable.
    fn render_panel_resize_handle(
        &self,
        side: panel_layout::PanelSide,
        entity: Entity<Self>,
    ) -> impl IntoElement {
        let id: &'static str = match side {
            panel_layout::PanelSide::Left => "panel-resize-left",
            panel_layout::PanelSide::Right => "panel-resize-right",
        };
        let active_tab = self.active_tab;
        let handle = div()
            .id(id)
            .debug_selector(move || id.to_owned())
            .absolute()
            .top_0()
            .bottom_0()
            .w(px(SPLIT_DIVIDER_SIZE))
            .cursor_col_resize()
            .on_mouse_down(gpui::MouseButton::Left, {
                let entity = entity.clone();
                move |event, window, cx| {
                    entity.update(cx, |workspace, cx| {
                        // Take the anchor here, not in the drag payload: the
                        // payload is built at render time and cannot know
                        // where inside the 6px strip the pointer landed.
                        let width = match side {
                            panel_layout::PanelSide::Left => workspace.sidebar_width,
                            panel_layout::PanelSide::Right => workspace.right_panel_width,
                        };
                        workspace.panel_drag_anchor =
                            Some((f32::from(event.position.x), width));
                        workspace.refocus_focused_pane(active_tab, window, cx);
                    });
                }
            })
            .on_drag(DraggedPanelEdge { side }, |_, _, _, cx| {
                cx.new(|_| gpui::Empty)
            });
        match side {
            // Each panel's *inner* edge: the sidebar's right, the right
            // panel's left.
            panel_layout::PanelSide::Left => handle.right_0(),
            panel_layout::PanelSide::Right => handle.left_0(),
        }
    }
```

Add each handle as a child of its panel in the render (`main.rs:10476` and `:10502`), after the existing `.child(self.sidebar.clone())` / `.child(self.right_panel.clone())`:

```rust
                    .child(self.render_panel_resize_handle(
                        panel_layout::PanelSide::Left,
                        entity.clone(),
                    ))
```

```rust
                    .child(self.render_panel_resize_handle(
                        panel_layout::PanelSide::Right,
                        entity.clone(),
                    ))
```

- [ ] **Step 6: Handle the drag on the work area**

On the `shell-work-area` div (`main.rs:10467`), mirroring the split divider's listeners at `main.rs:8921`:

```rust
            .on_drag_move::<DraggedPanelEdge>({
                let entity = entity.clone();
                move |event, _, cx| {
                    let drag = *event.drag(cx);
                    entity.update(cx, |workspace, cx| {
                        workspace.update_panel_width(drag.side, event, cx)
                    });
                }
            })
            .on_drop::<DraggedPanelEdge>({
                let entity = entity.clone();
                move |_, window, cx| {
                    entity.update(cx, |workspace, cx| {
                        // GPUI's drag machinery clears focus after mouse-down
                        // and a next-frame re-focus loses the race — the
                        // split divider carries the same comment. Drop is the
                        // only point where restoring focus actually sticks.
                        let active_tab = workspace.active_tab;
                        workspace.refocus_focused_pane(active_tab, window, cx);
                        workspace.dragging_panel = None;
                        workspace.panel_drag_anchor = None;
                        cx.notify();
                    });
                }
            })
```

- [ ] **Step 7: Implement the width update**

In the same `impl TillerWorkspace` block:

```rust
    /// Derives the panel's width from how far the pointer has travelled since
    /// the edge was grabbed, then clamps it into that panel's settings range.
    ///
    /// Dragging the left sidebar's right edge rightwards widens it; dragging
    /// the right panel's left edge leftwards widens it — hence the opposite
    /// signs.
    fn update_panel_width(
        &mut self,
        side: panel_layout::PanelSide,
        event: &gpui::DragMoveEvent<DraggedPanelEdge>,
        cx: &mut Context<Self>,
    ) {
        let Some((grab_x, grab_width)) = self.panel_drag_anchor else {
            return;
        };
        let travelled = f32::from(event.event.position.x) - grab_x;
        let width = match side {
            panel_layout::PanelSide::Left => grab_width + travelled,
            panel_layout::PanelSide::Right => grab_width - travelled,
        };
        if !width.is_finite() {
            return;
        }
        let (floor, ceiling) = side.range();
        let clamped = width.clamp(floor, ceiling);
        let current = match side {
            panel_layout::PanelSide::Left => self.sidebar_width,
            panel_layout::PanelSide::Right => self.right_panel_width,
        };
        // Sub-pixel jitter would repaint every frame for nothing.
        if (current - clamped).abs() < 0.5 {
            return;
        }
        match side {
            panel_layout::PanelSide::Left => self.sidebar_width = clamped,
            panel_layout::PanelSide::Right => self.right_panel_width = clamped,
        }
        self.dragging_panel = Some(side);
        cx.notify();
    }
```

- [ ] **Step 8: Run the tests to verify they pass**

Run: `cd rust && cargo test -p tiller`
Expected: PASS, including the five pre-existing shell-gap assertions, which must not have needed edits. If any of them moved, the handle is in layout flow — recheck `.absolute()`.

- [ ] **Step 9: Commit**

```bash
git add rust/crates/tiller/src/main.rs
git commit -m "feat(shell): drag the panel edges to resize them"
```

---

### Task 6: Debounced persistence

**Files:**
- Modify: `rust/crates/tiller/src/main.rs` — new constant, two fields, `schedule_panel_width_save`, one call in `update_panel_width`
- Test: `rust/crates/tiller/src/main.rs` test module

**Interfaces:**
- Consumes: `update_panel_width` (Task 5), `AppSettings` width fields (Task 2).
- Produces: `TillerWorkspace::schedule_panel_width_save(&mut self, cx: &mut Context<Self>)`.

- [ ] **Step 1: Write the failing test**

```rust
/// A drag writes to SQLite after it settles, not once per frame: nothing is
/// stored while the width is still moving, and the width that lands is the
/// last one asked for.
///
/// What this test does *not* prove is a literal write count — that would need
/// a spy `SessionStore`, which this codebase has no seam for. What it does
/// prove is the property the debounce exists for: two superseded schedules
/// leave no trace.
///
/// The debounce is a gpui timer rather than the `std::thread::sleep` shape
/// `Terminal::resize` uses, precisely so `advance_clock` can drive it:
/// neither that sleep nor the session flusher's `Instant::now` polling can be
/// advanced by the executor, and a test for either would have to sleep for
/// real.
#[gpui::test]
async fn panel_width_persists_once_after_the_drag_settles(cx: &mut TestAppContext) {
    cx.set_global(Theme::dark());
    let window = cx.add_window(|_window, cx| palette_test_workspace(cx));
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    cx.run_until_parked();
    let workspace = cx.update(|window, _| {
        window
            .root::<TillerWorkspace>()
            .flatten()
            .expect("workspace root")
    });

    for width in [420.0, 440.0, 460.0] {
        workspace.update(&mut cx.cx, |workspace, cx| {
            workspace.right_panel_width = width;
            workspace.schedule_panel_width_save(cx);
        });
    }
    cx.run_until_parked();

    let session = workspace.read_with(&cx.cx, |workspace, _| workspace.session.clone());
    assert_eq!(
        session.load_settings().right_panel_width, 405,
        "nothing is written while the drag is still moving"
    );

    cx.background_executor.advance_clock(PANEL_WIDTH_SAVE_DEBOUNCE);
    cx.run_until_parked();

    assert_eq!(
        session.load_settings().right_panel_width, 460,
        "the last width wins, and only it is written"
    );
}
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cd rust && cargo test -p tiller panel_width_persists_once_after_the_drag_settles`
Expected: FAIL to compile — `no method named schedule_panel_width_save`.

- [ ] **Step 3: Add the constant and the fields**

Beside `SPLIT_DIVIDER_SIZE` (`main.rs:3338`):

```rust
/// How long the panel width sits still before it is written to SQLite.
/// `save_settings` opens a write transaction and rewrites every key, so
/// calling it per drag frame would be one transaction per frame.
const PANEL_WIDTH_SAVE_DEBOUNCE: Duration = Duration::from_millis(500);
```

`Task` is **not** in `main.rs`'s `use gpui::{…}` list (`main.rs:1-6`) and is not covered by the prelude. Add it:

```rust
use gpui::{
    AnyElement, App, Bounds, ClickEvent, Context, DefiniteLength, DragMoveEvent, Entity,
    FocusHandle, Focusable, FontWeight, InteractiveElement, KeyBinding, KeyDownEvent, MouseButton,
    PathPromptOptions, PromptLevel, Render, StatefulInteractiveElement, Task, TitlebarOptions,
    Window, WindowBounds, WindowOptions, actions, deferred, div, point, prelude::*, px, size,
};
```

In `struct TillerWorkspace` (`main.rs:3433`), beside `dragging_panel`:

```rust
    /// Bumped per scheduled save; a timer that wakes to find it stale does
    /// nothing. Same generation-counter shape as `Terminal::resize`.
    panel_width_save_generation: u64,
    panel_width_save_task: Option<Task<()>>,
```

Initialise both in the struct literal (`main.rs:4100`):

```rust
            panel_width_save_generation: 0,
            panel_width_save_task: None,
```

- [ ] **Step 4: Implement the debounced save**

```rust
    /// Persists the current widths once the drag stops moving.
    ///
    /// Deliberately not driven off `on_drop` alone: a drag can end without a
    /// drop — Escape, a panel hidden by shortcut mid-gesture, a release
    /// outside the window — and the rendered width would then diverge from
    /// the stored preference until the next restart. The timer closes that,
    /// leaving `on_drop` doing only what only it can do, which is restoring
    /// focus.
    fn schedule_panel_width_save(&mut self, cx: &mut Context<Self>) {
        self.panel_width_save_generation += 1;
        let generation = self.panel_width_save_generation;
        // Rounded here, not at the range check: every other numeric settings
        // key is an i64 and drag positions are fractional.
        let sidebar = self.sidebar_width.round() as i64;
        let right_panel = self.right_panel_width.round() as i64;
        self.panel_width_save_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(PANEL_WIDTH_SAVE_DEBOUNCE)
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.panel_width_save_generation != generation {
                    return;
                }
                let mut settings =
                    app_settings_from_snapshot(this.settings.read(cx).snapshot());
                settings.sidebar_width = sidebar;
                settings.right_panel_width = right_panel;
                this.session.save_settings(&settings);
            });
        }));
    }
```

- [ ] **Step 5: Call it from the drag**

In `update_panel_width` (added in Task 5, Step 7), after `self.dragging_panel = Some(side);`:

```rust
        self.schedule_panel_width_save(cx);
```

- [ ] **Step 6: Run the tests to verify they pass**

Run: `cd rust && cargo test -p tiller panel_width`
Expected: PASS.

Both accessors the test and the implementation use already exist: `SessionStore::load_settings` (`tiller/src/session.rs:1398`) and `SessionStore::save_settings` (`:1420`), the same pair the settings `on_change` handler uses at `main.rs:13194`. Do not add a new accessor.

- [ ] **Step 7: Commit**

```bash
git add rust/crates/tiller/src/main.rs
git commit -m "feat(shell): persist panel widths once the drag settles"
```

---

### Task 7: Remove the dead settings and correct the ledger

Leaving these would recreate the exact condition this whole plan exists to fix: a clamp with a test and no reader.

**Files:**
- Modify: `rust/crates/tiller_project/src/settings.rs:16` (fields), `:44-45` (clamps), `:83-84` (test)
- Modify: `rust/crates/tiller_ui/src/conformance.rs:58` (ledger entry)

**Interfaces:**
- Consumes: nothing.
- Produces: nothing.

- [ ] **Step 1: Delete the dead fields**

In `rust/crates/tiller_project/src/settings.rs`, remove from `SettingsPolicy` (`:16`):

```rust
    pub sidebar_width: f32,
    pub right_panel_width: f32,
```

their entries in `Default` (`sidebar_width: 240.0,` and `right_panel_width: 320.0,`), and their clamp lines in `from_values` (`:44-45`).

These were never the right home. Persisted settings clamp through `settings_ranges` applied by `clamp_setting` at load (`db.rs:908`); no persisted setting uses `SettingsPolicy::from_values`. Their defaults also disagreed with the drawn geometry — 240/320 against 325/405 — so wiring them verbatim would have shrunk both panels on first launch.

- [ ] **Step 2: Update the test that pinned them**

In `values_are_clamped_to_safe_ranges` (`settings.rs:66`), delete:

```rust
        assert_eq!(settings.sidebar_width, 160.0);
        assert_eq!(settings.right_panel_width, 640.0);
```

and the two width fields from the `SettingsPolicy` literal that test builds.

- [ ] **Step 3: Correct the departures ledger**

In `rust/crates/tiller_ui/src/conformance.rs:58`, replace the entry:

```rust
//! - **Right panel header 40px, activity rows 48px, `PANEL_WIDTH` 405**:
```

with:

```rust
//! - **Right panel header 40px, activity rows 48px, right panel 220–640px
//!   (default 405)**: Tiller-only surface (waku's right panel is a native
//!   webview, "not part of Tiller's UI"). No waku measurement exists. The
//!   width stopped being a frozen constant when the panel became
//!   user-resizable; 405 survives as the default, not as the geometry.
```

Keep the rest of that bullet's existing text intact.

- [ ] **Step 4: Run the full gate**

Run: `cd rust && cargo test -p tiller_project && cargo test -p tiller_ui`
Then: `Scripts/ci.sh`
Expected: `CI OK`.

- [ ] **Step 5: Commit**

```bash
git add rust/crates/tiller_project/src/settings.rs rust/crates/tiller_ui/src/conformance.rs
git commit -m "chore(settings): drop the dead panel-width policy fields"
```

---

## Verification

After Task 7, `Scripts/ci.sh` must print `CI OK`. Then confirm by hand with `Scripts/build-dev.sh`:

1. Drag the right panel's left edge. It resizes; the terminal reflows; the tab strip re-sizes its tabs.
2. Release, quit, relaunch. The width is where you left it.
3. Narrow the window until the panels are squeezed, then widen it again. The panels return to the widths you chose — the squeeze was a projection, not a write.
4. Drag one panel to its limit, then the other. The centre column never disappears.
