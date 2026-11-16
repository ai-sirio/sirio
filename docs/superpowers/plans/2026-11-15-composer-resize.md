# Composer Resize Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A drag on the chat composer's top edge sets how many rows the field may grow to (3 rows up to half the chat's height); the value is one app-wide preference saved across relaunches.

**Architecture:** Ely's `TextInput` gains a live rows setter. `sirio_ui` holds the value in a GPUI global (`ComposerPrefs`, the `EditorPrefs` pattern). Each `Chat` pushes `(min, max)` into its field from that global, from its own drag and from the cap the existing chat probe measures. The workspace installs the global from `AppSettings` at startup and persists it when a chat reports a drag's result.

**Tech Stack:** Rust, GPUI (bezel-gpui 0.3.8), vendored Ely GPUI Components, SQLite settings (`sirio_persistence`), bash + python + xdotool E2E.

**Spec:** `docs/superpowers/specs/2026-11-15-composer-resize-design.md` (read it; this plan argues from it).

## Global Constraints

- Field minimum `3` rows; default maximum `12`; row height `18` px (the composer's fixed `line_height(px(18.0))`); edge strip `6` px.
- Cap: `cap_rows = max(3, floor(chat_height / 2 / 18))`, `chat_height` = the chat root's height from the existing probe; non-finite heights give `3`. It applies to the default 12 too.
- Drawn maximum at rest: `max(3, min(stored, cap_rows))`. Before the chat's first paint (cap unknown) it is `min(stored, 12)`, so a chat shown for the first time grows into its cap rather than overshooting it for a frame (a plan-time refinement of spec §3.3, which left the value uncapped there).
- Stored value: settings key `chat.composerMaxRows`, `i64`, default `12`, clamped on load to `3..=120`.
- During a drag that has moved, the field is `min = max = rows`; otherwise `min = 3`. A drag whose rows never differed from the anchor's saves nothing.
- Every change to vendored Ely is logged in `rust/vendor/ely-gpui-component/LOCAL-CHANGES.md`.
- `ComposerSnapshot` (`composerRow`) carries geometry, enums and numbers only, never the draft (`composer`/`composerText` are the draft).
- Tests: features are proven by E2E against the real binary. A `#[test]` only for a unit proven in isolation, written before its code, starting from its failure list; no constant-pinning asserts. No tests written after the code. Never run `Scripts/ci.sh` or `Scripts/ci-linux.sh` (the user runs them).
- Every commit leaves the workspace compiling. Builds touching `sirio` (and every E2E run, which builds it) need Zig exactly 0.15.2 first on `PATH`.
- GPUI drags: `on_drag` needs an element `.id(..)`; a drag starts only after the pointer moves past 2 px, and the move that starts it never reaches `on_drag_move`. Every E2E drag therefore moves at least twice.
- Commits: Conventional Commits, lower-case imperative subject, ending with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.
- Version: Task 1's commit is the cycle's first `feat:`, so it moves `rust/Cargo.toml` `0.40.1` → `0.41.0` through `Scripts/set-workspace-version.sh 0.41.0`, unless `git log --oneline v0.40.0..origin/main | grep ' feat'` shows one already.

## Review Focus

1. A save from the Settings window rebuilds the whole settings row and would reset a dragged composer to 12 → Task 5's `a_settings_save_keeps_the_stored_composer_rows`.
2. A drag released away from the edge leaves the field pinned at `min = max` → Task 4's E2E steps 4–5 release over the transcript, then empty the draft and expect 3 rows.
3. A hand-edited or corrupt stored value (`0`, `10000`, `abc`) → Task 1's clamp test.
4. A chat shorter than 3 rows × 2, an unmeasured chat, a non-finite pointer, a cap below 3 (Rust's `clamp` panics when min > max) → Task 2's tests.
5. A plain click on the edge, or a drag under half a row, rewrites the maximum → Task 4's E2E step 7 (two moves, 3 px then 5 px, so `on_drag_move` does run).

Not covered by any test, by design: a drag the platform ends without any release reaching the window. Task 4's render guard handles it; X11 cannot produce it on demand.

---

### Task 1: The stored preference

**Files:**
- Modify: `rust/crates/sirio_persistence/src/model.rs` (`AppSettings` ~643–800 and its `Default` ~760–800, `settings_keys` ~803, `settings_ranges` ~854)
- Modify: `rust/crates/sirio_persistence/src/db.rs` (load ~1380–1410, save ~1630–1650)
- Modify: every `AppSettings { … }` literal in the workspace that lacks `..AppSettings::default()` (the compiler lists them), including `rust/crates/sirio/src/main.rs` `app_settings_from_snapshot` (~30743) and the `sirio` tests (`main.rs` ~44974–45190, `session.rs` ~3730)
- Modify: `rust/Cargo.toml` (only through `Scripts/set-workspace-version.sh`)
- Test: `rust/crates/sirio_persistence/tests/persistence_integration.rs`

**Interfaces:**
- Produces: `AppSettings::composer_max_rows: i64`; `settings_keys::CHAT_COMPOSER_MAX_ROWS: &str = "chat.composerMaxRows"`; `settings_ranges::CHAT_COMPOSER_MAX_ROWS: RangeInclusive<i64> = 3..=120`.

- [ ] **Step 1: Write the failing test**, next to `the_centre_split_ratio_survives_a_relaunch_and_clamps_a_corrupt_value` and in its shape:

```rust
#[test]
fn the_composer_rows_survive_a_relaunch_and_clamp_a_corrupt_value() {
    // save composer_max_rows: 20 -> reopen -> 20 ("a value inside the range comes back verbatim")
    // raw UPDATE setting SET value = '10000' WHERE key = 'chat.composerMaxRows' -> reopen -> 120
    // raw '0' -> 3
    // raw 'abc' -> 12 (what clamp_setting gives an unparsable value: the default)
}
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cd rust && cargo test -p sirio_persistence --test persistence_integration the_composer_rows`
Expected: compile error, no field `composer_max_rows`.

- [ ] **Step 3: Add the key, range, field (doc line `"chat.composerMaxRows" — default 12, clamped to 3...120.`), default `12`, load via `clamp_setting(&value, settings_ranges::CHAT_COMPOSER_MAX_ROWS, defaults.composer_max_rows)` and the save line**, each beside its `CENTER_SPLIT_RATIO` counterpart. Fix every literal the compiler reports by adding the field. In `app_settings_from_snapshot` it is `AppSettings::default().composer_max_rows`, with a comment beside the widths' saying the snapshot does not carry it because the composer's edge owns it.

- [ ] **Step 4: Run it to verify it passes, and that the workspace still compiles**

Run: `cd rust && cargo test -p sirio_persistence && cargo check -p sirio --tests`
Expected: PASS, including the existing settings tests; `sirio` compiles. If `'abc'` does not give 12, read `clamp_setting` and match the test to what it does for the other keys (it is the crate's one rule).

- [ ] **Step 5: Move the version** (Global Constraints, Version): `Scripts/set-workspace-version.sh 0.41.0`.

- [ ] **Step 6: Commit** — `feat: store the composer's maximum rows as a setting`

---

### Task 2: The arithmetic and the global

**Files:**
- Create: `rust/crates/sirio_ui/src/composer_prefs.rs` (and `pub mod composer_prefs;` in `rust/crates/sirio_ui/src/lib.rs` beside `editor_prefs`)
- Create: `rust/crates/sirio_ui/src/chat/composer_height.rs` (and `mod composer_height;` in `rust/crates/sirio_ui/src/chat/mod.rs`)

**Interfaces:**
- Consumes: Task 1's field and range.
- Produces:
  - `pub struct ComposerPrefs { pub max_rows: usize }`, `impl gpui::Global`, `Default` (= `from_settings(&AppSettings::default())`), `pub fn from_settings(&AppSettings) -> Self` (clamped into the range, like `EditorPrefs::from_settings`), `pub fn current(cx: &App) -> Self`, `pub fn apply(prefs: Self, cx: &mut App)` (sets the global only when absent or different; no window refresh: chats observe it).
  - In `composer_height.rs`: `pub(crate) const ROW_HEIGHT: f32 = 18.0;` `pub(crate) const MIN_ROWS: usize = 3;` `pub(crate) const DEFAULT_MAX_ROWS: usize = 12;` `pub(crate) const EDGE_HEIGHT: f32 = 6.0;` `pub(crate) fn cap_rows(chat_height: f32) -> usize;` `pub(crate) fn drag_rows(anchor_rows: usize, anchor_y: f32, pointer_y: f32, cap: usize) -> Option<usize>`.

- [ ] **Step 1: Write the failing tests** in `composer_height.rs` (the failure list first: non-finite inputs, a chat too short, a cap under 3, jitter):

```rust
#[test]
fn the_cap_is_half_the_chat_in_whole_rows() {
    assert_eq!(cap_rows(432.0), 12);
    assert_eq!(cap_rows(431.0), 11);
    assert_eq!(cap_rows(1000.0), 27);
}

#[test]
fn a_chat_too_short_or_unmeasurable_still_caps_at_three_rows() {
    for height in [0.0, 50.0, -10.0, f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        assert_eq!(cap_rows(height), 3, "height {height}");
    }
}

#[test]
fn the_field_follows_the_pointer_in_whole_rows() {
    assert_eq!(drag_rows(3, 500.0, 464.0, 20), Some(5), "36 px up is two rows");
    assert_eq!(drag_rows(12, 500.0, 536.0, 20), Some(10), "36 px down is two rows fewer");
    assert_eq!(drag_rows(3, 500.0, 492.0, 20), Some(3), "8 px is under half a row");
    assert_eq!(drag_rows(3, 500.0, 490.0, 20), Some(4), "10 px rounds to a row");
}

#[test]
fn the_field_stops_at_three_rows_and_at_the_cap() {
    assert_eq!(drag_rows(5, 500.0, 5000.0, 20), Some(3));
    assert_eq!(drag_rows(5, 500.0, -5000.0, 20), Some(20));
    assert_eq!(drag_rows(5, 500.0, 400.0, 1), Some(3), "a cap under 3 never panics");
}

#[test]
fn a_pointer_that_is_not_a_number_moves_nothing() {
    assert_eq!(drag_rows(5, 500.0, f32::NAN, 20), None);
    assert_eq!(drag_rows(5, f32::INFINITY, 500.0, 20), None);
}
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cd rust && cargo test -p sirio_ui composer_height`
Expected: compile errors (`cap_rows`, `drag_rows` undefined).

- [ ] **Step 3: Implement both functions and `ComposerPrefs`.** `drag_rows` computes in `f32`, `round()`s the row delta, and clamps with `cap.max(MIN_ROWS)` as the upper bound.

- [ ] **Step 4: Run them to verify they pass**

Run: `cd rust && cargo test -p sirio_ui composer_height`
Expected: 5 passed.

- [ ] **Step 5: Commit** — `feat: add the composer's row arithmetic and its app-wide preference`

---

### Task 3: The field follows the stored value and the cap

**Files:**
- Modify: `rust/vendor/ely-gpui-component/src/forms/text/mod.rs` (after `set_disabled`, ~317) and `rust/vendor/ely-gpui-component/LOCAL-CHANGES.md` (new `### A multi-line field's rows, changed live (composer resize)`)
- Modify: `rust/crates/sirio_ui/src/chat/mod.rs` (field creation ~2593–2604)
- Modify: `rust/crates/sirio_ui/src/chat/composer_height.rs` (the `impl Chat` part, and its tests)
- Modify: `rust/crates/sirio_ui/src/chat/composer_row.rs` (`RowFrame` ~126, `ComposerSnapshot` ~175, `composer_snapshot` ~278)
- Modify: `rust/crates/sirio_ui/src/chat/session_panel/mod.rs` (probe ~555–596)
- Modify: `rust/crates/sirio_ui/src/chat/composer_view.rs` (`FIELD_PAD_Y` ~709 becomes `pub(super)`; `composer-input` ~1296–1318)
- Modify: `Scripts/Tests/test-chat-session-panel-e2e.sh`

**Interfaces:**
- Consumes: Task 2's constants, `cap_rows`, `ComposerPrefs`.
- Produces:
  - `TextInput::set_rows(&mut self, min: usize, max: usize, cx: &mut Context<Self>)`: asserts the field is multi-line and `0 < min && min <= max` (the builder's message); returns without notifying when unchanged.
  - `RowFrame` gains `field: Cell<Option<Bounds<Pixels>>>`, `cap_rows: Cell<Option<usize>>` and `stored_max_rows: Cell<usize>` (the chat's copy of the global, so `&self` readers such as `composer_snapshot` need no `cx`).
  - `Chat::sync_composer_rows(&mut self, cx: &mut Context<Self>)`: first copies `ComposerPrefs::current(cx).max_rows` into `RowFrame.stored_max_rows`, then computes the wanted `(min, max)` (Global Constraints; Task 4 adds the drag case) and calls `set_rows` only when `composer_field.read(cx).rows()` differs. Never called from `render`.
  - `Chat::drawn_max_rows(&self) -> usize`: the drawn maximum at rest, from the `RowFrame` cells.
  - `ComposerSnapshot` gains (camelCase on the wire) `field: Option<[f32; 4]>`, `field_rows: Option<usize>` (`round((field height − 2 × FIELD_PAD_Y) / 18)`), `max_rows: usize` (`drawn_max_rows()`), `stored_max_rows: usize`, `cap_rows: Option<usize>`.

- [ ] **Step 1: Write the failing test** `every_open_chat_takes_a_new_maximum` (`#[gpui::test]`, in `composer_height.rs`, built with the chat test helpers `chat_view` uses in `chat/mod.rs` ~8258): two chats exist; `ComposerPrefs::apply(ComposerPrefs { max_rows: 20 }, cx)`; after `run_until_parked`, both chats' `composer_snapshot().stored_max_rows == 20`, and a chat painted in a window whose chat is at least 720 px tall has `composer_field.read(cx).rows() == Some((3, 20))`. It goes red if a chat does not observe the global, which no E2E can see (the harness drives one chat surface at a time).
- [ ] **Step 2: Run it to verify it fails**

Run: `cd rust && cargo test -p sirio_ui every_open_chat_takes_a_new_maximum`
Expected: FAIL to compile (`stored_max_rows` not on the snapshot), then, once the fields exist, FAIL on `20`.

- [ ] **Step 3: Add `set_rows` and its `LOCAL-CHANGES.md` entry** (what it does, why Sirio needs it, that a sync must keep it).
- [ ] **Step 4: Build the field from the global** (`.multi_line(MIN_ROWS, ComposerPrefs::current(cx).max_rows.min(DEFAULT_MAX_ROWS))`, seed `RowFrame.stored_max_rows`) and `cx.observe_global::<ComposerPrefs>(|chat, cx| chat.sync_composer_rows(cx)).detach()` beside the existing `cx.observe(&composer_field, …)`.
- [ ] **Step 5: Extend the probe**: compute `cap_rows(bounds.size.height)` each paint; when it differs from `row.cap_rows`, store it and defer `entity.update(cx, |chat, cx| { chat.sync_composer_rows(cx); cx.notify() })`. No other new notify.
- [ ] **Step 6: Measure the field**: a paint-nothing `canvas` inside `composer-input` (absolute, `inset_0`, like the band's) writes `RowFrame.field`. Fill the new snapshot fields.
- [ ] **Step 7: Run the chat tests**

Run: `cd rust && cargo test -p sirio_ui chat::`
Expected: the new test and every existing chat test pass (none pins 12 rows).

- [ ] **Step 8: Add the E2E scenario `composer_resize`** to `ALL_SCENARIOS`, `PAINTED_SCENARIOS` and `FRAMED_SCENARIOS`, with a header comment block like `composer_keys`'s. Shape: `launch composer_resize`, `ctl project add "$PRIMARY"`, `open_chat`, `show_chat`, wait for `idle|completed`, then, with `compose()` = `E2E rpc surface.chat.compose surfaceId="$SURFACE" text="$1"` and `r = c["composerRow"]`:
  1. `r["capRows"] >= 12 and r["maxRows"] == 12 and r["storedMaxRows"] == 12 and r["fieldRows"] == 3`
  2. compose `"$(printf 'line %d\n' $(seq 1 40))"` (real newlines; a `"\n"` in bash double quotes is a backslash and an n): `r["fieldRows"] == 12`
  9. `xdotool windowsize` the window to `2200x400`; first assert `r["root"][3] < 432` (fail with that message if the layout keeps the chat taller), then `r["maxRows"] == r["capRows"] and r["capRows"] < 12 and r["capRows"] == max(3, int(r["root"][3] / 2 / 18)) and r["storedMaxRows"] == 12 and r["fieldRows"] == r["capRows"]`. Capture `composer-resize-short`. Back to `2200x1000`: `r["maxRows"] == 12 and r["fieldRows"] == 12`.

  Step numbers are the spec's §6. In the script they run in the order 1, 2, 9 (so step 9 runs while the stored value is still 12); Task 4 appends 3–7 after them and Task 5 appends 8.

- [ ] **Step 9: Run it**

Run (an Xvfb you started, e.g. `Xvfb :97 -screen 0 2400x1200x24 &`): `Scripts/Tests/test-chat-session-panel-e2e.sh --display :97 --window-size 2200x1000 --only composer_resize`
Expected: `CHAT SESSION PANEL E2E PARTIAL (1 of <n> scenarios: composer_resize)`; frames under the run's `--out-dir`.

- [ ] **Step 10: Commit** — `feat: let the composer's field follow a stored maximum and a cap at half the chat`

---

### Task 4: The edge and the drag

**Files:**
- Modify: `rust/crates/sirio_ui/src/chat/composer_height.rs`
- Modify: `rust/crates/sirio_ui/src/chat/composer_view.rs` (the `composer` div ~1375–1405)
- Modify: `rust/crates/sirio_ui/src/chat/mod.rs` (`ChatEvent` ~1748; `Chat` fields, constructor and `render`)
- Modify: `rust/crates/sirio_ui/src/chat/composer_row.rs` (`RowFrame`, `ComposerSnapshot`)
- Modify: `rust/crates/sirio/src/main.rs` (`bind_chat`'s exhaustive `match event` ~9665; a new workspace method)
- Modify: `docs/agents/architecture/claude-chat.md` (the `composerRow` paragraph: the new fields)
- Modify: `Scripts/Tests/test-chat-session-panel-e2e.sh`

**Interfaces:**
- Consumes: Task 3's `sync_composer_rows`, `drawn_max_rows`, `RowFrame` cells; Task 2's `drag_rows`, `ComposerPrefs::apply`; Task 1's field.
- Produces:
  - `ChatEvent::ComposerMaxRowsChanged(usize)`.
  - `SirioWorkspace::save_composer_max_rows(&mut self, rows: usize)`: `load_settings`, set the field, `save_settings` (the `silence_language_server_offer` pattern; once per drag, no debounce), called from the new `bind_chat` arm. The arm lands in this task because the match is exhaustive: without it `sirio` stops compiling.
  - `struct DraggedComposerEdge;` `pub(super) struct ComposerDrag { anchor_y: f32, anchor_rows: usize, rows: usize, dragged: bool, changed: bool }`; `Chat.composer_drag: Option<ComposerDrag>`.
  - `Chat::begin_composer_drag(&mut self, pointer_y: f32)` (anchor rows = the measured field rows, falling back to `drawn_max_rows()`; replaces any stale drag), `Chat::composer_drag_to(&mut self, pointer_y: f32, cx)` (sets `dragged`; `drag_rows` against `RowFrame.cap_rows`, or `120` before the first paint; sets `changed` once rows differ from the anchor; `sync_composer_rows`), `Chat::finish_composer_drag(&mut self, cx)` (idempotent; takes the drag; when `changed`, `ComposerPrefs::apply(ComposerPrefs { max_rows: rows }, cx)` **then** `sync_composer_rows`, then `cx.emit(ChatEvent::ComposerMaxRowsChanged(rows))`; otherwise only `sync_composer_rows`).
  - `sync_composer_rows` wants `(rows, rows)` only while a drag with `dragged` set is held; a press that never moved pins nothing.
  - `RowFrame.edge` and `ComposerSnapshot.edge: Option<[f32; 4]>`, `resizing: bool` (a drag with `dragged` set is held).

- [ ] **Step 1: Render the edge**: `.id("composer-edge")` and debug selector `composer-edge`, absolute, `top_0`, `left_0`, `right_0`, `h(px(EDGE_HEIGHT))`, `cursor_row_resize`; `on_mouse_down(Left)` → `begin_composer_drag`; `on_mouse_up(Left)` → `finish_composer_drag`; `on_drag(DraggedComposerEdge, |_, _, _, cx| cx.new(|_| gpui::Empty))`; a paint-nothing canvas writes `RowFrame.edge`. It sits inside the band's 10 px top padding, so it covers no text row, and it is drawn whenever the composer is, streaming and disabled included. Copy the drawer's edge (`rust/crates/sirio_ui/src/drawer/view.rs:619-641`).
- [ ] **Step 2: Wire the drag on the `composer` div**: `on_drag_move::<DraggedComposerEdge>` → `composer_drag_to(event.event.position.y)`; `on_drop::<DraggedComposerEdge>` and `on_mouse_up_out(Left)` → `finish_composer_drag`. Keep the existing focus-on-press handler. For a drag the platform ends without any of these running: in `Chat::render`, when `composer_drag` has `dragged` set and `!cx.has_active_drag()`, `cx.defer_in(window, |chat, _, cx| chat.finish_composer_drag(cx))`.
- [ ] **Step 3: Add the event, its `bind_chat` arm and `save_composer_max_rows`.**
- [ ] **Step 4: Build and run the tests**

Run: `cd rust && cargo test -p sirio_ui chat:: && cargo check -p sirio --tests`
Expected: all pass; `sirio` compiles.

- [ ] **Step 5: Add E2E steps 3–7** with two helpers. `drag_edge_to Y [hold]` moves the pointer to the centre of `r["edge"]` (window pixels + `window_origin`), `xdotool mousedown 1`, moves 3 px up (this only starts GPUI's drag), then in 9 px steps to window y `Y`, then `mouseup 1` unless `hold`. `drag_edge ROWS [hold]` is `drag_edge_to` with `Y = edge centre − ROWS × 18`.
  3. `drag_edge 4`: `fieldRows == 16 and maxRows == 16 and storedMaxRows == 16`. Capture `composer-resize-taller`. (Released on the edge.)
  4. `drag_edge_to` the transcript, at `Y = root_y + root_h / 4` (above any composer the cap allows): `fieldRows == capRows and storedMaxRows == capRows`. (Released outside the composer: `on_mouse_up_out`.)
  5. compose `""`: `fieldRows == 3` (nothing is left pinned).
  6. `drag_edge 5 hold`: `fieldRows == 8 and resizing` (capture `composer-resize-held`); `xdotool mouseup 1`: `fieldRows == 3 and maxRows == 8 and not resizing`.
  7. press and release on the edge without moving: `maxRows == 8`. Then press, move 3 px up, move 5 px more (8 px in all, under half a row), release: `maxRows == 8`.
- [ ] **Step 6: Run it** — same command as Task 3 Step 9; expected the same `PARTIAL` line.
- [ ] **Step 7: Commit** — `feat: drag the composer's top edge to set how tall it grows`

---

### Task 5: The workspace restores it

**Files:**
- Modify: `rust/crates/sirio/src/main.rs`: startup (~32240, beside `EditorPrefs::apply`), `app_settings_for_settings_save` (~30811), tests (~45185)
- Modify: `Scripts/Tests/test-chat-session-panel-e2e.sh`, `docs/agents/e2e-tests.md`

**Interfaces:**
- Consumes: Task 1's field, Task 2's `ComposerPrefs`, Task 4's `save_composer_max_rows`.

- [ ] **Step 1: Write the failing test** beside `a_settings_save_keeps_the_stored_forge_hosts`:

```rust
#[test]
fn a_settings_save_keeps_the_stored_composer_rows() {
    let stored = AppSettings { composer_max_rows: 20, ..AppSettings::default() };
    let saved = app_settings_for_settings_save(SettingsSnapshot::default(), &stored);
    assert_eq!(saved.composer_max_rows, 20, "a Settings save must not reset the dragged composer");
}
```

- [ ] **Step 2: Run it to verify it fails** (Task 1 left `app_settings_from_snapshot` at the default). Run: `cd rust && cargo test -p sirio a_settings_save_keeps_the_stored_composer_rows` — Expected: FAIL, `left: 12, right: 20`.
- [ ] **Step 3: Carry `stored.composer_max_rows` in `app_settings_for_settings_save`** (and add it to that function's doc list of what is re-applied). Re-run — Expected: PASS.
- [ ] **Step 4: Install at startup**: `ComposerPrefs::apply(ComposerPrefs::from_settings(&saved_settings), cx)` beside `EditorPrefs::apply`. Check the other `save_settings` call sites in `main.rs`: each must start from `load_settings()` or go through `app_settings_for_settings_save`.
- [ ] **Step 5: Run the crate's tests**

Run: `cd rust && cargo test -p sirio`
Expected: all pass.

- [ ] **Step 6: Add E2E step 8**: `stop_app`, `launch composer_resize` (same name: same database), `open_chat` (it opens a new chat), `show_chat`: `maxRows == 8 and storedMaxRows == 8 and fieldRows == 3`. Add the scenario to `docs/agents/e2e-tests.md` (the chat session panel block: what it proves, its flags, its frames).
- [ ] **Step 7: Run the scenario, then the default set it joins**

Run: `Scripts/Tests/test-chat-session-panel-e2e.sh --display :97 --window-size 2200x1000 --only composer_resize`
Expected: `CHAT SESSION PANEL E2E PARTIAL (1 of <n> scenarios: composer_resize)`.
Run: `Scripts/Tests/test-chat-session-panel-e2e.sh --display :97 --window-size 2200x1000`
Expected: `CHAT SESSION PANEL E2E OK (…)`. Keep the `--out-dir` path for the hand-off: it is the artifact.

- [ ] **Step 8: Commit** — `feat: restore the composer's maximum rows on launch`
