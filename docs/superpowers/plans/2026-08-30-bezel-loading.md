# Bezel Loading System Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Move Sirio onto the Bezel GPUI family and replace every ad-hoc loading affordance with one Sirio-owned loading adapter built on Bezel's primitives, without changing any domain state machine.

**Architecture:** Two phases that never overlap. Phase one swaps the GPUI dependency family (`gpui`/`gpui_platform` -> `bezel-gpui`/`bezel-gpui-platform`), rebases the local Wayland patch onto the released `bezel-gpui-linux`, and changes no pixels. Phase two adds `sirio_ui::loading` — the single module allowed to import Bezel loading primitives — and migrates Chat reasoning, tool rows, diffs, and the panel loading surfaces onto it. Existing typed `Entry`, task, and status semantics stay authoritative throughout: the adapter renders, it never owns state.

**Tech Stack:** Rust 2024, gpui (Bezel fork), `sirio_ui` crate, `sirio_theme` for resolved fonts/colors, standard `#[test]`/`gpui::test` in-file test modules, `Scripts/ci.sh` as the gate.

**Spec:** `docs/superpowers/specs/2026-08-29-bezel-loading-design.md`

## Global Constraints

- Bezel pins are exact: `bezel = "=0.1.3"`, `gpui = { package = "bezel-gpui", version = "=0.3.6" }`, `gpui_platform = { package = "bezel-gpui-platform", version = "=0.3.6", features = ["font-kit"] }`. A second GPUI package or a second Bezel GPUI version in the lockfile is a failed migration even if it compiles.
- Linux keeps the `wayland` and `x11` features on every GPUI user, added through the existing `[target.'cfg(target_os = "linux")'.dependencies]` sections.
- Only `rust/crates/sirio_ui/src/loading.rs` may `use` Bezel loading primitives. Every other call site goes through it.
- Sizes, verbatim from the spec: thinking glyph `14`, generic orb `44`, compact mini cell `2.5`, progress track `4px` with `280px` demo width, skeleton `3` rows of `28px` with `6px` gaps and `4px` vertical padding.
- Labels, verbatim: `Thinking` while running; `Thought for {:.0}s` only with a truthful duration, otherwise `Thought`; `Worked · N steps` for a tool-work zone. Never fabricate an elapsed time.
- Live transcript geometry: `700px` column, `24px` horizontal / `28px` vertical padding, `10px` turn gap, `8px` work-zone gap, `440px` user bubble. Activity reasoning keeps its own `660px` / `24px` / `32px` inner composition with a `10px` left margin.
- Sirio adds no per-entry repeating timers and copies no gallery RAF or demo streaming. Generic loaders ride Bezel's shared clock.
- Failed and cancelled never render as finished. A terminal label always wins over a loading label.
- Selection/copy, links, persistence, virtualization, tail-follow, Retry, and Stop are acceptance constraints, not polish.
- macOS keeps SF Pro / SF Mono. Geist and Geist Mono are bundled and registered before the first frame on Windows and Linux only.
- `Scripts/ci.sh` must print `CI OK` before any task is considered done.

## File Structure

**Phase one**
- `rust/Cargo.toml` — dependency family swap and `[patch]` retarget. The one place versions are decided.
- `rust/vendor/gpui_linux/` — the Wayland XDND patch, rebased onto `bezel-gpui-linux 0.3.6`. Keeps its own `[workspace]` table and its own tests.
- `rust/vendor/gpui_platform/` — patched only because its `gpui_linux` dependency is a path dependency; no behavior of its own.
- `rust/vendor/README.md` — records what is patched and against which upstream rev.
- `docs/superpowers/notes/2026-08-30-bezel-manifest-verification.md` — the release/API verification record produced by Task 1. Created by this plan, referenced by later tasks.

**Phase two**
- `rust/crates/sirio_ui/src/loading.rs` — NEW. The adapter: constants, pure label/fraction helpers, and the four render entry points (`thinking_indicator`, `indeterminate`, `compact`, `progress`, `skeleton_rows`). The only Bezel-facing file.
- `rust/crates/sirio_ui/src/lib.rs` — one `pub mod loading;` line.
- `rust/crates/sirio_ui/src/chat.rs` — reasoning header/body, tool-row grammar, diff paint, transcript geometry, and the removal of the braille spinner and rotating border.
- `rust/crates/sirio_ui/src/conformance.rs` — the frozen-geometry suite; split so the Chat transcript column can move to `700` while Settings and the markdown column stay at `720`.
- `rust/crates/sirio_ui/src/changes.rs`, `right_panel/mod.rs`, `file_view.rs`, `settings.rs`, `sidebar.rs`, `status_bar.rs` — call sites that adopt the matrix; each keeps its own state machine.
- `rust/crates/sirio_theme/src/lib.rs` — Geist family candidates for Windows/Linux.
- `rust/crates/sirio/build.rs`, `Scripts/build-app-bundle.sh` — font asset packaging.
- `rust/crates/sirio/assets/fonts/` — NEW. Geist and Geist Mono static TTFs plus `OFL.txt`.

---

## Task 1: Verify the Bezel release manifest and API

No product code changes. This task exists because the current lockfile contains no Bezel entries at all, and every later task assumes these packages resolve at these exact versions.

**Files:**
- Create: `docs/superpowers/notes/2026-08-30-bezel-manifest-verification.md`

**Interfaces:**
- Consumes: nothing.
- Produces: a written record of the resolved versions and the exact signatures of `loaders::orb`, the progress bar, and the redacted-rows helper as published, which Task 5 codes against.

- [ ] **Step 1: Resolve the four packages**

```bash
cd /tmp && cargo new --lib bezel-probe >/dev/null && cd bezel-probe
cargo add bezel@=0.1.3 --no-default-features 2>&1 | tail -20
cargo add bezel-gpui@=0.3.6 --no-default-features 2>&1 | tail -20
cargo add bezel-gpui-platform@=0.3.6 --no-default-features --features font-kit 2>&1 | tail -20
cargo add bezel-gpui-linux@=0.3.6 --no-default-features 2>&1 | tail -20
```

Expected: four successful adds. Record each resolved version.

- [ ] **Step 2: Record the loading API surface**

```bash
cd /tmp/bezel-probe && cargo doc --no-deps -p bezel 2>&1 | tail -5
grep -rn "pub fn orb\|pub enum Orb\|pub fn progress_bar\|pub fn redacted_rows\|pub fn mini_gradient_spinner" \
  ~/.cargo/registry/src/*/bezel-0.1.3/ | head -20
```

Expected: the published equivalents of the gallery's `ui::loaders::orb(shape, id, size, &theme, painter, cx)`, `theme.progress_bar(fraction)`, `popover::redacted_rows(id, &theme, count, painter, cx)`, and `loaders::mini_gradient_spinner(id, cell, painter, cx)`.

- [ ] **Step 3: Write the verification note**

Write `docs/superpowers/notes/2026-08-30-bezel-manifest-verification.md` containing: the four resolved versions; the exact published path and signature of each of the five items above; whether `Orb::Cluster` is public; and whether the shared clock (`motion::PulseClock` in the gallery) is public. If any item is absent or differs from the gallery source, write down the published form — Task 5 codes against this note, not against the gallery.

- [ ] **Step 4: Stop if the family does not resolve**

If any of the four packages fails to resolve at its exact version, do not start Task 2. Record the failure in the note, report it, and stop: the spec's phase-one gate cannot be met and the decision goes back to the spec author.

- [ ] **Step 5: Commit**

```bash
git add docs/superpowers/notes/2026-08-30-bezel-manifest-verification.md
git commit -m "docs: record the Bezel release and API verification"
rm -rf /tmp/bezel-probe
```

---

## Task 2: Swap the GPUI family in the workspace manifest

**Files:**
- Modify: `rust/Cargo.toml:57-61` (the `gpui` / `gpui_platform` workspace dependencies) and `rust/Cargo.toml:82-91` (the `[patch]` section)

**Interfaces:**
- Consumes: the resolved versions from Task 1's note.
- Produces: a workspace where `gpui` and `gpui_platform` are Bezel packages under their existing names, so no `use gpui::...` in any crate changes.

- [ ] **Step 1: Write the failing check**

There is no test harness for dependency shape, so the check is a command. Record the current state first:

```bash
cd rust && cargo tree -p sirio_ui -i gpui 2>&1 | head -5
```

Expected now: `gpui v0.1.0 (https://github.com/zed-industries/zed?rev=c05e346...)`. After this task it must name `bezel-gpui v0.3.6`.

- [ ] **Step 2: Replace the two workspace dependencies**

In `rust/Cargo.toml`, replace the `gpui` and `gpui_platform` lines (keep the long comment above them — it explains the Linux feature split, which still applies):

```toml
gpui = { package = "bezel-gpui", version = "=0.3.6", default-features = false }
gpui_platform = { package = "bezel-gpui-platform", version = "=0.3.6", default-features = false, features = ["font-kit"] }
bezel = { version = "=0.1.3", default-features = false }
```

- [ ] **Step 3: Retarget the patch section**

The `[patch."https://github.com/zed-industries/zed"]` section patches a git source that no longer exists in the graph. Replace it with a registry patch:

```toml
[patch.crates-io]
bezel-gpui-linux = { path = "vendor/gpui_linux" }
bezel-gpui-platform = { path = "vendor/gpui_platform" }
```

Leave the explanatory comment above it in place and add one line: the patch now targets the published `bezel-gpui-linux` rather than the zed git rev.

- [ ] **Step 4: Give `sirio_ui` the Bezel dependency**

In `rust/crates/sirio_ui/Cargo.toml`, add to `[dependencies]`:

```toml
bezel = { workspace = true }
```

No other crate gets it.

- [ ] **Step 5: Build and inspect the family**

```bash
cd rust && cargo build --workspace 2>&1 | tail -20
cargo tree -d 2>&1 | grep -i gpui
```

Expected: build succeeds; `cargo tree -d` prints no duplicate GPUI package. If it prints two, the migration has failed — fix the pins before continuing.

- [ ] **Step 6: Commit**

```bash
git add rust/Cargo.toml rust/Cargo.lock rust/crates/sirio_ui/Cargo.toml
git commit -m "build: move the workspace onto the Bezel GPUI family"
```

---

## Task 3: Rebase the Wayland XDND patch onto bezel-gpui-linux 0.3.6

**Files:**
- Modify: `rust/vendor/gpui_linux/src/linux/wayland/client.rs` (the `PendingDrop` handling and its tests)
- Modify: `rust/vendor/gpui_linux/Cargo.toml`, `rust/vendor/gpui_platform/Cargo.toml` (package names and versions)
- Modify: `rust/vendor/README.md`

**Interfaces:**
- Consumes: the Bezel family from Task 2.
- Produces: a patched `bezel-gpui-linux` that keeps the slow-provider XDND fix, with its focused test still passing.

- [ ] **Step 1: Run the existing patch test to see it pass before the rebase**

```bash
cd rust && cargo test --manifest-path vendor/gpui_linux/Cargo.toml pending_drop 2>&1 | tail -10
```

Expected: PASS. This is the behavior the rebase must preserve.

- [ ] **Step 2: Vendor the released source**

```bash
cd rust/vendor && cargo vendor --versioned-dirs --manifest-path ../Cargo.toml /tmp/bezel-vendor 2>&1 | tail -3
diff -ru /tmp/bezel-vendor/bezel-gpui-linux-0.3.6/src/linux/wayland/client.rs \
         gpui_linux/src/linux/wayland/client.rs | head -80
```

Read the diff: everything that is not the `PendingDrop` fix is upstream drift to take.

- [ ] **Step 3: Replace the vendored tree, then reapply only the fix**

Copy `/tmp/bezel-vendor/bezel-gpui-linux-0.3.6/` over `rust/vendor/gpui_linux/`, keeping the local `[workspace]` table in its `Cargo.toml`, then reapply the `PendingDrop` fix: store the pending drop when `DragState::window` is unset, resolve it when the `Enter` pipe read completes, and always `finish()`/`destroy()` the offer. Reapply its tests with it.

- [ ] **Step 4: Run the focused test**

```bash
cd rust && cargo test --manifest-path vendor/gpui_linux/Cargo.toml pending_drop 2>&1 | tail -10
```

Expected: PASS, same assertions as Step 1.

- [ ] **Step 5: Update the README**

In `rust/vendor/README.md`, replace every reference to the zed git rev with the `bezel-gpui-linux 0.3.6` release, and state that the patch is applied against that release. Keep the root-cause writeup link.

- [ ] **Step 6: Commit**

```bash
git add rust/vendor rust/Cargo.lock
git commit -m "build: rebase the Wayland XDND patch onto bezel-gpui-linux 0.3.6"
```

---

## Task 4: Phase-one gate

**Files:**
- Modify: none (verification only, plus any fix the gate forces)

**Interfaces:**
- Consumes: Tasks 2 and 3.
- Produces: a green tree with zero visual change, which phase two builds on.

- [ ] **Step 1: Run the workspace gate**

```bash
Scripts/ci.sh 2>&1 | tail -20
```

Expected: `CI OK`. Zig 0.15.2 must be on PATH (`sirio_terminal` needs it).

- [ ] **Step 2: Re-check the one-family invariant**

```bash
cd rust && grep -A1 '^name = "bezel-gpui"$' Cargo.lock | grep -c '^version'
```

Expected: `1`. Two GPUI packages here is a failed migration.

Do NOT use `cargo tree -d | grep -i gpui` for this. `cargo tree -d` prints the *inverse*
tree of every duplicated package, and `bezel-gpui` appears inside those trees because most
of the graph depends on it — so the grep matches on every run regardless of whether
`bezel-gpui` itself is duplicated. It is a structural false positive, and a check that can
never pass is how a real duplicate gets waved through. Count versions in the lockfile.

- [ ] **Step 3: Confirm no pixels moved**

```bash
git diff HEAD~2 --stat -- rust/crates | cat
```

Expected: no changes under `rust/crates` except the `sirio_ui/Cargo.toml` dependency line. Phase one changes no rendering code.

- [ ] **Step 4: Commit any gate fix**

If the gate forced a fix, commit it on its own:

```bash
git add -A && git commit -m "fix: settle the Bezel migration against the workspace gate"
```

---

## Task 5: The `sirio_ui::loading` adapter

**Files:**
- Create: `rust/crates/sirio_ui/src/loading.rs`
- Modify: `rust/crates/sirio_ui/src/lib.rs`

**Interfaces:**
- Consumes: the published API recorded in Task 1's note.
- Produces, for every later task:
  - `loading::THINKING_GLYPH: f32`, `GENERIC_ORB: f32`, `COMPACT_MINI_CELL: f32`, `PROGRESS_TRACK: f32`, `PROGRESS_MAX_WIDTH: f32`, `SKELETON_ROW_HEIGHT: f32`, `SKELETON_ROW_GAP: f32`, `SKELETON_ROWS: usize`
  - `loading::thought_label(elapsed: Option<Duration>) -> String`
  - `loading::clamp_fraction(fraction: f32) -> f32`
  - `loading::thinking_indicator(id: &'static str, theme: &Theme, window: &mut Window, cx: &mut App) -> AnyElement`
  - `loading::indeterminate(id: &'static str, size: f32, theme: &Theme, window: &mut Window, cx: &mut App) -> AnyElement`
  - `loading::compact(id: &'static str, window: &mut Window, cx: &mut App) -> AnyElement`
  - `loading::progress(fraction: f32, theme: &Theme) -> Div`
  - `loading::skeleton_rows(id: &'static str, count: usize, theme: &Theme, window: &mut Window, cx: &mut App) -> AnyElement`

- [ ] **Step 1: Write the failing test**

Create `rust/crates/sirio_ui/src/loading.rs` with only the test module first:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn a_run_without_a_truthful_duration_is_labelled_thought() {
        assert_eq!(thought_label(None), "Thought");
    }

    #[test]
    fn a_run_with_a_duration_reports_whole_seconds() {
        assert_eq!(thought_label(Some(Duration::from_millis(4400))), "Thought for 4s");
        assert_eq!(thought_label(Some(Duration::from_millis(600))), "Thought for 1s");
    }

    #[test]
    fn a_fraction_outside_the_unit_range_is_clamped_not_rejected() {
        assert_eq!(clamp_fraction(-0.5), 0.0);
        assert_eq!(clamp_fraction(1.7), 1.0);
        assert_eq!(clamp_fraction(0.35), 0.35);
    }

    #[test]
    fn the_geometry_matches_the_gallery_evidence() {
        assert_eq!(THINKING_GLYPH, 14.0);
        assert_eq!(GENERIC_ORB, 44.0);
        assert_eq!(COMPACT_MINI_CELL, 2.5);
        assert_eq!(PROGRESS_TRACK, 4.0);
        assert_eq!(PROGRESS_MAX_WIDTH, 280.0);
        assert_eq!(SKELETON_ROW_HEIGHT, 28.0);
        assert_eq!(SKELETON_ROW_GAP, 6.0);
        assert_eq!(SKELETON_ROWS, 3);
    }
}
```

Add `pub mod loading;` to `rust/crates/sirio_ui/src/lib.rs`, in alphabetical position between `pub mod git_status_style;` and `pub mod modal;`.

- [ ] **Step 2: Run the tests to verify they fail**

```bash
cd rust && cargo test -p sirio_ui loading:: 2>&1 | tail -20
```

Expected: FAIL to compile — `thought_label` and the constants are not defined.

- [ ] **Step 3: Write the minimal implementation**

Above the test module in `loading.rs`:

```rust
//! The one place Bezel loading primitives are called from.
//!
//! OWNERSHIP: this module renders; it owns no business state. Every surface
//! keeps its own task/status state machine and asks here only for a picture of
//! it. Bezel imports live here and nowhere else, so a Bezel API change is a
//! one-file change.

use std::time::Duration;

use gpui::{AnyElement, App, Div, IntoElement, ParentElement, Styled, Window, div, px, relative};
use sirio_theme::Theme;

/// The glyph slot in an Activity-derived reasoning header.
pub const THINKING_GLYPH: f32 = 14.0;
/// The generic orb for a full-surface first load or empty state.
pub const GENERIC_ORB: f32 = 44.0;
/// The cell size of the compact refresh spinner.
pub const COMPACT_MINI_CELL: f32 = 2.5;
/// Determinate progress: track thickness and the width the gallery demos.
pub const PROGRESS_TRACK: f32 = 4.0;
pub const PROGRESS_MAX_WIDTH: f32 = 280.0;
/// A subordinate skeleton: three rows, never the sole activity signal.
pub const SKELETON_ROW_HEIGHT: f32 = 28.0;
pub const SKELETON_ROW_GAP: f32 = 6.0;
pub const SKELETON_ROWS: usize = 3;

/// The label for a settled reasoning header.
///
/// `None` means Sirio has no truthful duration for the run, and the label says
/// so by omission rather than inventing an elapsed time.
pub fn thought_label(elapsed: Option<Duration>) -> String {
    match elapsed {
        Some(elapsed) => format!("Thought for {:.0}s", elapsed.as_secs_f32()),
        None => "Thought".to_string(),
    }
}

/// Clamp a caller's fraction into the unit range. A determinate bar shows a
/// truthful fraction of a known total; a caller that overshoots is pinned, not
/// panicked.
pub fn clamp_fraction(fraction: f32) -> f32 {
    fraction.clamp(0.0, 1.0)
}

/// A determinate progress bar. `4px` track, rounded, filled by `fraction`.
/// The caller owns the width: the gallery demos `280px`, surfaces narrower
/// than that constrain it.
pub fn progress(fraction: f32, theme: &Theme) -> Div {
    div()
        .w_full()
        .max_w(px(PROGRESS_MAX_WIDTH))
        .h(px(PROGRESS_TRACK))
        .rounded_full()
        .bg(theme.colors.hairline)
        .child(
            div()
                .h_full()
                .w(relative(clamp_fraction(fraction)))
                .rounded_full()
                .bg(theme.colors.title),
        )
}
```

Then add the four Bezel-backed renderers — `thinking_indicator` (Cluster at `THINKING_GLYPH`), `indeterminate` (Cluster at a caller size, default `GENERIC_ORB`), `compact` (mini gradient spinner at `COMPACT_MINI_CELL`), and `skeleton_rows` (`SKELETON_ROWS` rows of `SKELETON_ROW_HEIGHT` with `SKELETON_ROW_GAP` gaps and `4px` vertical padding) — calling the published entry points exactly as Task 1's note records them. All four take the shared clock from Bezel; none of them arms a timer of its own.

- [ ] **Step 4: Run the tests to verify they pass**

```bash
cd rust && cargo test -p sirio_ui loading:: 2>&1 | tail -20
```

Expected: 4 passed.

- [ ] **Step 5: Check the reduced-motion and unmount behaviour**

The spec's §6 rule is that a generic loader freezes at phase `0` under reduced
motion and leases no ticks, and that an unmounted loader stops renewing. Both
belong to the adapter, not to its callers:

```bash
cd rust && grep -n "reduced_motion\|prefers_reduced" crates/sirio_ui/src/loading.rs
```

Expected: the four renderers consult the shared clock's reduced-motion state and
return a static frame when it is on. If the published Bezel clock exposes no
such flag, record that in Task 1's note and hold the loaders at a single frame
behind a Sirio-side check rather than animating unconditionally.

- [ ] **Step 6: Verify the import boundary holds**

```bash
cd rust && grep -rn "bezel::" crates/ --include=*.rs | grep -v "crates/sirio_ui/src/loading.rs"
```

Expected: no output. Any other file importing Bezel is a boundary violation.

- [ ] **Step 7: Commit**

```bash
git add rust/crates/sirio_ui/src/loading.rs rust/crates/sirio_ui/src/lib.rs
git commit -m "feat(ui): add the Bezel loading adapter"
```

---

## Task 6: Activity-derived reasoning header and body in Chat

**Files:**
- Modify: `rust/crates/sirio_ui/src/chat.rs:84-102` (delete `GENERATING_SPINNER_FRAMES`, `GENERATING_SPINNER_CYCLE`, `generating_spinner_frame`)
- Modify: `rust/crates/sirio_ui/src/chat.rs:4928-4986` (the `Entry::Thought` render arm)
- Modify: `rust/crates/sirio_ui/src/chat.rs:8078-8108` (the streaming indicator sibling of the transcript)
- Modify: `rust/crates/sirio_ui/src/chat.rs:9013-9028` (delete `generating_spinner_cycles_through_all_ten_frames_in_one_second`)

**Interfaces:**
- Consumes: `loading::thinking_indicator`, `loading::thought_label`, `loading::THINKING_GLYPH`.
- Produces: the reasoning header/body composition that Task 9's tool rows sit beside. The debug selector `chat-generating-spinner` keeps its name, so the seven lifecycle tests around `chat.rs:9047-9240` keep working unchanged.

- [ ] **Step 1: Write the failing test**

Add to the test module in `chat.rs`, next to the spinner lifecycle tests:

```rust
#[test]
fn a_settled_reasoning_header_never_invents_an_elapsed_time() {
    // Sirio has no truthful per-thought duration today, so the label is the
    // deterministic one. If a duration is ever threaded through, this test
    // is what says the other branch is allowed.
    assert_eq!(crate::loading::thought_label(None), "Thought");
}

#[gpui::test]
async fn the_running_reasoning_header_shows_the_thinking_indicator(cx: &mut TestAppContext) {
    let (chat, cx) = spinner_test_chat(cx);
    chat.update(cx, |chat, cx| {
        chat.set_streaming_for_test(true, cx);
    });
    cx.draw_frame();
    assert!(
        cx.debug_bounds("chat-generating-spinner").is_some(),
        "a streaming turn shows the Activity-derived indicator"
    );
}
```

- [ ] **Step 2: Run the tests to verify they fail**

```bash
cd rust && cargo test -p sirio_ui reasoning_header 2>&1 | tail -20
cd rust && cargo test -p sirio_ui the_running_reasoning_header 2>&1 | tail -20
```

Expected: FAIL to compile — `crate::loading` is reachable but `set_streaming_for_test` may not exist. If it does not, use the same helper the existing `the_generating_spinner_appears_while_a_turn_streams` test uses to start a turn; read that test first and copy its setup exactly.

- [ ] **Step 3: Replace the braille spinner with the adapter**

At `chat.rs:8078-8108`, keep the `.when(self.streaming, ...)` structure, the `chat-generating-spinner` id, the `debug_selector`, the `max_w(px(TRANSCRIPT_WIDTH))`, and the "transient by construction" comment — all of that is still true. Replace only the child:

```rust
.child(
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(px(6.0))
        .px(px(4.0))
        .py(px(5.0))
        .child(
            div()
                .flex()
                .w(px(loading::THINKING_GLYPH))
                .justify_center()
                .child(loading::thinking_indicator("chat-thinking", theme, window, cx)),
        )
        .child(
            div()
                .text_size(px(12.5))
                .text_color(theme.colors.subtitle)
                .child("Thinking"),
        ),
)
```

The `elapsed` local and its comment about riding the composer's timer go away with the braille frames.

- [ ] **Step 4: Rebuild the `Entry::Thought` arm on the same grammar**

In the `Entry::Thought` arm, keep `toggle_thought_expanded`, the entry index, the debug selector, and the click target on the row itself. Change: the glyph slot becomes `px(loading::THINKING_GLYPH)` wide, the gap becomes `px(6.0)`, the row gains `.px(px(4.0)).py(px(5.0))`, and the label becomes `loading::thought_label(None)` when collapsed rather than `thought_summary(&text)`. Expanded, the body becomes subordinate under the header: `border_l_1()`, `.pl(px(12.0))`, `.pr(px(14.0))`, `.max_h(px(160.0))`, `.gap(px(4.0))`, `overflow_y_scroll`, with a `20px` top fade painted (not masked) over it. Keep the italic reasoning styling and `render_plain_text` — selection, links, and copy ride on it.

- [ ] **Step 5: Delete the braille frames and their test**

Delete `GENERATING_SPINNER_FRAMES`, `GENERATING_SPINNER_CYCLE`, `generating_spinner_frame`, and the test `generating_spinner_cycles_through_all_ten_frames_in_one_second`. Nothing else may reference them:

```bash
cd rust && grep -rn "GENERATING_SPINNER\|generating_spinner_frame" crates/
```

Expected: no output.

- [ ] **Step 6: Run the full Chat spinner suite**

```bash
cd rust && cargo test -p sirio_ui spinner 2>&1 | tail -25
```

Expected: the seven lifecycle tests pass unchanged (`appears_while_a_turn_streams`, `disappears_when_the_turn_ends`, `the_transcript_gains_no_entry_for_the_spinner`, `leaves_when_a_real_turn_completes`, `leaves_when_a_turn_is_cancelled`, `leaves_when_the_transport_fails`, `does_not_move_or_resize_the_composer`), plus the two new ones.

- [ ] **Step 7: Commit**

```bash
git add rust/crates/sirio_ui/src/chat.rs
git commit -m "feat(chat): replace the braille spinner with the Activity reasoning header"
```

---

## Task 7: Retire the rotating streaming border

**Files:**
- Modify: `rust/crates/sirio_ui/src/chat.rs:69-77` (the three `STREAMING_BORDER_*` constants), `104-109` (`streaming_border_angle`), `6045-6076` (the border lifecycle in `render_composer`)
- Modify: `rust/crates/sirio_ui/src/chat.rs:8933-8942`, `9378-9386`, `9388-9432`, `9597-9642` (the four border tests)

**Interfaces:**
- Consumes: Task 6 (the indicator that replaces this signal).
- Produces: a composer with no animation timer of its own, which is the spec's "no custom Chat animations or per-entry timers" acceptance line.

- [ ] **Step 1: Write the failing test**

```rust
#[gpui::test]
async fn the_composer_arms_no_repaint_timer_while_streaming(cx: &mut TestAppContext) {
    let (chat, cx) = spinner_test_chat(cx);
    chat.update(cx, |chat, cx| {
        chat.set_streaming_for_test(true, cx);
    });
    cx.draw_frame();
    chat.read_with(cx, |chat, _| {
        assert!(
            !chat.streaming_border_timer_pending,
            "the streaming border timer is retired; the shared clock drives the indicator"
        );
    });
}
```

- [ ] **Step 2: Run it to verify it fails**

```bash
cd rust && cargo test -p sirio_ui arms_no_repaint_timer 2>&1 | tail -20
```

Expected: FAIL — the field is still set to `true` while streaming.

- [ ] **Step 3: Remove the border**

Delete `STREAMING_BORDER_REVOLUTION`, `STREAMING_BORDER_TICK`, `STREAMING_BORDER_WIDTH`, `streaming_border_angle`, the `streaming_border_deg` block in `render_composer`, and the border element it feeds. Keep `streaming_border_started_at` only if Task 6 still reads it; if nothing reads it, delete the field and its resets too. The composer keeps its static 1px border in every state, so its footprint does not move.

- [ ] **Step 4: Update the border tests**

Delete `streaming_border_angle_completes_one_linear_revolution` and `streaming_border_angle_rotates_one_revolution` — they test a deleted function. Rewrite `the_rotating_border_wraps_the_composer_only_while_streaming` and `the_rotating_border_shrinks_with_a_narrow_pane` as assertions that the composer's border is unchanged between idle and streaming, keeping their pane-width setup.

- [ ] **Step 5: Run the composer suite**

```bash
cd rust && cargo test -p sirio_ui composer 2>&1 | tail -25
cd rust && cargo test -p sirio_ui border 2>&1 | tail -20
```

Expected: PASS, with no reference to a rotation angle left.

- [ ] **Step 6: Commit**

```bash
git add rust/crates/sirio_ui/src/chat.rs
git commit -m "refactor(chat): retire the rotating streaming border"
```

---

## Task 8: Live transcript geometry

**Files:**
- Modify: `rust/crates/sirio_ui/src/chat.rs:53` (`TRANSCRIPT_WIDTH`), `63` (`USER_PILL_MAX_WIDTH`), and the transcript container around `7867-8176` (its `max_w(px(TRANSCRIPT_WIDTH))` call sites are `7477`, `7705`, `7902`, `7932`, `7958`, `8031`)
- Modify: `rust/crates/sirio_ui/src/conformance.rs:306-340`

**Interfaces:**
- Consumes: nothing from earlier tasks.
- Produces: `TRANSCRIPT_WIDTH = 700.0` and `USER_PILL_MAX_WIDTH = 440.0`, which the conformance suite and Task 10's diff both read.

- [ ] **Step 1: Write the failing test**

In `conformance.rs`, split the frozen-720 test. Replace `the_content_column_is_the_frozen_720` with two tests:

```rust
/// Settings and the file-tab markdown column keep waku's frozen 720. The chat
/// transcript no longer does: it follows the Bezel Transcript pattern's 700
/// (`docs/superpowers/specs/2026-08-29-bezel-loading-design.md` §2).
#[test]
fn the_non_chat_content_columns_are_the_frozen_720() {
    assert_eq!(CONTENT_WIDTH, 720.0, "settings content column");
    assert_eq!(MARKDOWN_COLUMN_WIDTH, 720.0, "file-tab markdown column");
    assert_eq!(CARD_H_PADDING, 14.0, "composer card px(14)");
    assert_eq!(CARD_V_PADDING, 10.0, "composer card p(10)");
}

#[test]
fn the_live_transcript_follows_the_bezel_transcript_pattern() {
    assert_eq!(TRANSCRIPT_WIDTH, 700.0, "transcript column (Bezel Transcript)");
    assert_eq!(USER_PILL_MAX_WIDTH, 440.0, "user bubble (Bezel Activity)");
}
```

Update the second assertion in the waku-components test at `conformance.rs:335-340` the same way, and its comment: the pill is now the Bezel 440, radius and padding unchanged.

- [ ] **Step 2: Run it to verify it fails**

```bash
cd rust && cargo test -p sirio_ui conformance 2>&1 | tail -20
```

Expected: FAIL — `TRANSCRIPT_WIDTH` is 720, `USER_PILL_MAX_WIDTH` is 540.

- [ ] **Step 3: Move the constants**

```rust
/// The transcript's content column maximum — the Bezel Transcript pattern's
/// 700 (spec §2). Settings and the markdown column keep waku's 720; this one
/// column follows Bezel because the live transcript is what the migration
/// copies.
pub(crate) const TRANSCRIPT_WIDTH: f32 = 700.0;

/// The user turn's bubble: rounded, right-aligned, capped at the Bezel
/// Activity pattern's 440. The assistant reply has no container at all.
pub(crate) const USER_PILL_MAX_WIDTH: f32 = 440.0;
```

- [ ] **Step 4: Apply the transcript padding and gaps**

On the transcript container around `chat.rs:7897-8075`: `24px` horizontal padding, `28px` vertical padding, `10px` between turns, and `8px` between a work zone and its answer. Do not touch the list's virtualization, `ListSizingBehavior::Auto`, or the tail-follow behavior — they are acceptance constraints.

- [ ] **Step 5: Run the suites**

```bash
cd rust && cargo test -p sirio_ui conformance 2>&1 | tail -20
cd rust && cargo test -p sirio_ui chat 2>&1 | tail -25
```

Expected: PASS. If a Chat visual test asserts a 720 bound, update it to 700 with the same reasoning; if it asserts the pill at 540, update it to 440.

- [ ] **Step 6: Commit**

```bash
git add rust/crates/sirio_ui/src/chat.rs rust/crates/sirio_ui/src/conformance.rs
git commit -m "feat(chat): move the live transcript onto Bezel geometry"
```

---

## Task 9: Tool-row and work-zone grammar

**Files:**
- Modify: `rust/crates/sirio_ui/src/chat.rs:5671-5843` (`render_tool_call_card`) and `5898-5980` (`render_tool_call_group`)

**Interfaces:**
- Consumes: Task 8's geometry, `loading::THINKING_GLYPH`.
- Produces: the `Worked · N steps` work-zone header the spec's matrix names.

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn a_tool_run_is_labelled_as_a_work_zone() {
    assert_eq!(tool_group_label(1), "Worked · 1 step");
    assert_eq!(tool_group_label(4), "Worked · 4 steps");
}
```

- [ ] **Step 2: Run it to verify it fails**

```bash
cd rust && cargo test -p sirio_ui work_zone 2>&1 | tail -20
```

Expected: FAIL — `tool_group_label` is not defined.

- [ ] **Step 3: Add the label helper and use it**

```rust
/// The header for a run of consecutive tool calls, in the Bezel Transcript
/// pattern's words. Pure, so the singular/plural split is testable without a
/// window.
fn tool_group_label(count: usize) -> String {
    if count == 1 {
        "Worked · 1 step".to_string()
    } else {
        format!("Worked · {count} steps")
    }
}
```

Use it for the group header text in `render_tool_call_group`, replacing the current "N steps" wording. Keep `group_index`, the toggle, `group_expanded`, the per-member indices, and F-CHAT-23 detail toggles exactly as they are — the grouping behavior is correct, only its label and paint change.

- [ ] **Step 4: Adopt the step-row paint**

In `render_tool_call_card`, arrange each row as icon, verb/title, detail, then duration or status right-aligned, with the disclosure on the row itself. Failure and cancellation keep their existing colors and words — a failed row is never repainted as finished. Do not add a Cluster loader to nonterminal rows: the gallery Tool calls page shows none, and the spec marks that as a Sirio adaptation the matrix does not require.

- [ ] **Step 5: Run the tool suites**

```bash
cd rust && cargo test -p sirio_ui tool_call 2>&1 | tail -25
```

Expected: PASS, including the existing grouping and status tests.

- [ ] **Step 6: Commit**

```bash
git add rust/crates/sirio_ui/src/chat.rs
git commit -m "feat(chat): give tool runs the Bezel work-zone grammar"
```

---

## Task 10: Diff paint conventions

**Files:**
- Modify: `rust/crates/sirio_ui/src/chat.rs:4530-4680` (`render_tool_diff`)

**Interfaces:**
- Consumes: Task 8's `TRANSCRIPT_WIDTH`.
- Produces: nothing later tasks read; this is a leaf change.

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn a_diff_in_the_transcript_takes_the_column_rather_than_the_standalone_760() {
    // The gallery's standalone Diff pattern references 760; inside a 700
    // transcript the diff uses the width it has (spec §3).
    assert!(DIFF_STANDALONE_REFERENCE > TRANSCRIPT_WIDTH);
    assert_eq!(diff_column_width(TRANSCRIPT_WIDTH), TRANSCRIPT_WIDTH);
    assert_eq!(diff_column_width(400.0), 400.0);
}
```

- [ ] **Step 2: Run it to verify it fails**

```bash
cd rust && cargo test -p sirio_ui diff_column 2>&1 | tail -20
```

Expected: FAIL — neither item is defined.

- [ ] **Step 3: Add the width rule**

```rust
/// The gallery's standalone Diff pattern is drawn at 760. Recorded so the
/// number in the spec has a home in the code, and so the rule below can say
/// what it is *not* doing.
const DIFF_STANDALONE_REFERENCE: f32 = 760.0;

/// A diff inside the transcript uses the width it is given. It never forces
/// the standalone 760 — the transcript column is narrower, and a diff that
/// overflowed it would scroll the whole turn sideways.
fn diff_column_width(available: f32) -> f32 {
    available
}
```

- [ ] **Step 4: Apply the paint conventions**

In the diff renderer: fixed old/new number columns, a fixed sign column, `8px` row gaps, `10px` horizontal and `1px` vertical row padding, the code family at `12px/18px`, 10% success and danger washes, skip rows, and a path header carrying the `+N -M` tallies. Horizontal scrolling stays on the diff, not the turn. Keep clickable file headers, line numbers, +/-/context rows, selection, the 60-line preview, file opening, and revert — and keep the diff subordinate to its owning tool call: no timer of its own, no second loader.

- [ ] **Step 5: Run the diff suites**

```bash
cd rust && cargo test -p sirio_ui diff 2>&1 | tail -25
```

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add rust/crates/sirio_ui/src/chat.rs
git commit -m "feat(chat): adopt the Bezel diff paint conventions"
```

---

## Task 11: First-load surfaces

**Files:**
- Modify: `rust/crates/sirio_ui/src/changes.rs:1898-1927` (`render_body`'s loading branch, id `changes-loading` at `1907`)
- Modify: `rust/crates/sirio_ui/src/right_panel/mod.rs` (the `settled == false` placeholder)
- Modify: `rust/crates/sirio_ui/src/file_view.rs` (the `ViewState` loading branch)
- Modify: `rust/crates/sirio_ui/src/right_panel/history.rs:150-230` (the `load_task` first load)
- Modify: `rust/crates/sirio_ui/src/project_forms.rs` (create-project in flight)

**Interfaces:**
- Consumes: `loading::indeterminate`, `loading::skeleton_rows`, `loading::GENERIC_ORB`, `loading::SKELETON_ROWS`.
- Produces: nothing later tasks read.

- [ ] **Step 1: Write the failing test**

In `changes.rs`'s test module:

```rust
#[gpui::test]
async fn a_first_load_shows_the_generic_loader_over_an_empty_list(cx: &mut TestAppContext) {
    let (tab, cx) = changes_test_tab(cx);
    tab.update(cx, |tab, cx| tab.begin_first_load_for_test(cx));
    cx.draw_frame();
    assert!(cx.debug_bounds("changes-loading").is_some());
}

#[gpui::test]
async fn a_refresh_keeps_the_settled_list_visible(cx: &mut TestAppContext) {
    let (tab, cx) = changes_test_tab(cx);
    tab.update(cx, |tab, cx| tab.settle_with_entries_for_test(cx));
    tab.update(cx, |tab, cx| tab.begin_refresh_for_test(cx));
    cx.draw_frame();
    assert!(cx.debug_bounds("changes-list").is_some(), "stale content stays");
    assert!(cx.debug_bounds("changes-loading").is_none(), "no full-surface loader on refresh");
}
```

Read the existing `changes.rs` test module first and reuse whichever helpers it already has instead of inventing the three `*_for_test` names if equivalents exist.

- [ ] **Step 2: Run them to verify they fail**

```bash
cd rust && cargo test -p sirio_ui first_load 2>&1 | tail -20
```

Expected: FAIL — the refresh case currently has no assertion holding it, and the loading branch is a bare text line.

- [ ] **Step 3: Rebuild the Changes loading branch**

Keep the `changes-loading` id, the three-state structure, and the `git_task.is_some() && entries.is_empty()` condition — that condition *is* "first load", and it already excludes refresh. Replace the bare text child with: a centered `loading::indeterminate("changes-loading-orb", loading::GENERIC_ORB, ...)`, the existing `Loading changes…` copy under it, and `loading::skeleton_rows("changes-skeleton", loading::SKELETON_ROWS, ...)` subordinate below. The error branch still wins over both.

- [ ] **Step 4: Apply the same grammar to the other three**

History gates its first load on "`load_task` in flight with no rows yet" and gets the full loader plus subordinate rows; its refresh is Task 12's. `right_panel/mod.rs` gates on `settled == false` (never on "a walk is in flight" — read the comment at `mod.rs:195-212`, it exists because that mistake was made once); `file_view.rs` gates on its `ViewState` loading variant; `project_forms.rs` gates on the create task. Each keeps its own copy: `Loading files…`, `Loading file…`, `Creating project…`, centered in its surface. Error and Retry keep precedence everywhere.

- [ ] **Step 5: Run the suites**

```bash
cd rust && cargo test -p sirio_ui changes 2>&1 | tail -25
cd rust && cargo test -p sirio_ui right_panel 2>&1 | tail -25
cd rust && cargo test -p sirio_ui file_view 2>&1 | tail -25
cd rust && cargo test -p sirio_ui history 2>&1 | tail -25
```

Expected: PASS, including the existing error/Retry tests.

- [ ] **Step 6: Commit**

```bash
git add rust/crates/sirio_ui/src/changes.rs rust/crates/sirio_ui/src/right_panel/mod.rs \
        rust/crates/sirio_ui/src/right_panel/history.rs \
        rust/crates/sirio_ui/src/file_view.rs rust/crates/sirio_ui/src/project_forms.rs
git commit -m "feat(ui): give first-load surfaces the Bezel loader grammar"
```

---

## Task 12: Refresh, determinate, and compact surfaces

**Files:**
- Modify: `rust/crates/sirio_ui/src/changes.rs` (toolbar refresh), `right_panel/mod.rs` (walk/expansion), the History toolbar, `rust/crates/sirio_ui/src/project_forms.rs` (clone progress), `rust/crates/sirio_ui/src/status_bar.rs` (usage provider), `rust/crates/sirio_ui/src/browser.rs` (reload/Stop), `rust/crates/sirio_ui/src/settings.rs:925-933,3423-3437,3529-3549` (agent installation), `rust/crates/sirio_ui/src/sidebar.rs:272-313` (running indicator)

**Interfaces:**
- Consumes: `loading::compact`, `loading::progress`, `loading::clamp_fraction`.
- Produces: nothing later tasks read.

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn a_clone_bar_reports_the_truthful_fraction() {
    assert_eq!(crate::loading::clamp_fraction(0.0), 0.0);
    assert_eq!(crate::loading::clamp_fraction(0.42), 0.42);
    assert_eq!(crate::loading::clamp_fraction(1.0), 1.0);
}

#[gpui::test]
async fn a_cancelled_clone_shows_no_progress_bar(cx: &mut TestAppContext) {
    let (form, cx) = clone_test_form(cx);
    form.update(cx, |form, cx| form.set_clone_status_for_test(CloneStatus::Failed { .. }, cx));
    cx.draw_frame();
    assert!(cx.debug_bounds("clone-progress").is_none(), "a terminal state wins over the bar");
}
```

Read `project_forms.rs`'s existing `CloneStatus` variants and its test helpers first, and use their real names and shapes in the second test.

- [ ] **Step 2: Run them to verify they fail**

```bash
cd rust && cargo test -p sirio_ui clone 2>&1 | tail -20
```

Expected: FAIL — no `clone-progress` element exists yet.

- [ ] **Step 3: Replace the clone progress with the determinate bar**

Render `loading::progress(progress, &theme)` under the id `clone-progress` while `CloneStatus::Running { progress }`, keeping the existing `Cloning… N%` copy beside it. Every terminal variant removes the bar before showing its own state.

- [ ] **Step 4: Apply the compact treatment to the refresh surfaces**

Changes toolbar refresh, Files walk/expansion, History refresh, usage provider, and browser navigation each get one `loading::compact` in their existing control slot — settled content stays visible under it. Browser keeps Stop explicit. Sidebar and Activity Panel get the same compact indicator in their existing status slot, with no reasoning body and no transcript composition; identity badges stay separate from activity.

- [ ] **Step 5: Cover Settings installation**

`settings.rs`'s agent installation uses the first-load grammar when there is no content yet and the compact grammar when a settled list is refreshing. This surface is named in the spec because it was omitted once; it is not optional.

- [ ] **Step 6: Run the suites**

```bash
cd rust && cargo test -p sirio_ui 2>&1 | tail -25
```

Expected: PASS across the crate.

- [ ] **Step 7: Commit**

```bash
git add rust/crates/sirio_ui/src
git commit -m "feat(ui): adopt the compact and determinate loading treatments"
```

---

## Task 13: Geist and Geist Mono for Windows and Linux

**Files:**
- Create: `rust/crates/sirio/assets/fonts/Geist-Regular.ttf`, `Geist-Medium.ttf`, `GeistMono-Regular.ttf`, `OFL.txt`
- Modify: `rust/crates/sirio_theme/src/lib.rs:993-1050` (the non-macOS `UI_FAMILY_CANDIDATES` at `993` and `CODE_FAMILY_CANDIDATES` at `1021`; leave `TERMINAL_FAMILY_CANDIDATES` at `1051` alone)
- Modify: `rust/crates/sirio/src/main.rs` (registration before the first window opens)
- Modify: `rust/crates/sirio/build.rs`, `Scripts/build-app-bundle.sh` (packaging)

**Interfaces:**
- Consumes: nothing from earlier tasks.
- Produces: nothing later tasks read.

- [ ] **Step 1: Write the failing test**

In `sirio_theme`'s test module:

```rust
#[test]
#[cfg(not(target_os = "macos"))]
fn geist_leads_the_non_macos_families() {
    assert_eq!(UI_FAMILY_CANDIDATES[0], "Geist");
    assert_eq!(CODE_FAMILY_CANDIDATES[0], "Geist Mono");
}

#[test]
#[cfg(target_os = "macos")]
fn macos_keeps_the_apple_faces() {
    assert_eq!(UI_FAMILY_CANDIDATES[0], "SF Pro");
    assert_eq!(CODE_FAMILY_CANDIDATES[0], "SF Mono");
}
```

- [ ] **Step 2: Run it to verify it fails**

```bash
cd rust && cargo test -p sirio_theme families 2>&1 | tail -20
```

Expected: on macOS the second test passes and the first is compiled out; run it on Linux (or with `--target x86_64-unknown-linux-gnu`) to see the first fail with `"JetBrains Sans"`.

- [ ] **Step 3: Bundle the faces**

Download the static TTFs of Geist Regular, Geist Medium, and Geist Mono Regular plus `OFL.txt` into `rust/crates/sirio/assets/fonts/`. Only the weights the current typography uses — not all nine. Static TTFs unless variable support is verified on both platforms.

- [ ] **Step 4: Register before the first frame**

In `main.rs`, register the bundled faces with the text system *before* `cx.open_window`, mirroring the gallery's order (`register_fonts` then window). Then put `"Geist"` at the head of `UI_FAMILY_CANDIDATES` and `"Geist Mono"` at the head of `CODE_FAMILY_CANDIDATES`, non-macOS only. Leave `TERMINAL_FAMILY_CANDIDATES` alone: it must keep its Nerd Font faces for agent TUI glyph coverage.

- [ ] **Step 5: Package the assets**

Add the fonts directory to `build.rs`'s asset handling and to `Scripts/build-app-bundle.sh` beside the icons, with the `OFL.txt` copyright and license text shipped alongside.

- [ ] **Step 6: Verify family resolution on both platforms**

```bash
cd rust && cargo test -p sirio_theme families 2>&1 | tail -20
cd rust && cargo build --target x86_64-pc-windows-msvc 2>&1 | tail -5   # if the target is installed
```

Expected: the family tests pass on their own platform, and the app resolves `Geist`/`Geist Mono` rather than falling through to JetBrains.

- [ ] **Step 7: Commit**

```bash
git add rust/crates/sirio/assets/fonts rust/crates/sirio_theme/src/lib.rs \
        rust/crates/sirio/src/main.rs rust/crates/sirio/build.rs Scripts/build-app-bundle.sh
git commit -m "feat(theme): bundle Geist and Geist Mono for Windows and Linux"
```

---

## Task 14: Date separators — verify feasibility before implementing

The spec's §2 and §5 both call for date separators in the live transcript. `Entry` (`chat.rs:211-294`) carries no timestamp on any variant, so this is the one requirement in the spec that cannot be met without touching the persisted transcript model — which the spec's own goals section excludes. Resolve that contradiction before writing code.

**Files:**
- Modify: `rust/crates/sirio_ui/src/chat.rs` (only if Step 2 finds a truthful timestamp)

**Interfaces:**
- Consumes: Task 8's transcript composition.
- Produces: nothing later tasks read.

- [ ] **Step 1: Confirm the gap**

```bash
cd rust && grep -n "timestamp\|created_at\|SystemTime\|DateTime" crates/sirio_ui/src/chat.rs | head -20
cd rust && grep -rn "timestamp\|created_at" crates/sirio_persistence/src/ | head -20
```

- [ ] **Step 2: Decide from what you find**

If a truthful per-turn timestamp already exists in `sirio_persistence` and reaches Chat, render a separator between turns whose dates differ, using `TODAY` for the current day, and add a pure test for the boundary rule (`same day -> no separator`, `different day -> one separator`).

If it does not exist, stop. Do not synthesize a timestamp at render time: a separator dated by when the transcript was *drawn* is a fabricated fact, exactly the class of thing the spec's "never a fabricated elapsed time" rule forbids. Report the contradiction — §2/§5 require separators, the goals section forbids the persistence change that would make them truthful — and let the spec author choose.

- [ ] **Step 3: Commit whichever outcome applies**

```bash
# if implemented
git add rust/crates/sirio_ui/src/chat.rs && git commit -m "feat(chat): date separators in the live transcript"
# if blocked
git commit --allow-empty -m "docs: record the date-separator blocker against the transcript model"
```

---

## Task 15: Final gate

**Files:**
- Modify: none (verification only)

- [ ] **Step 1: Run the gate**

```bash
Scripts/ci.sh 2>&1 | tail -20
```

Expected: `CI OK`.

- [ ] **Step 2: Re-check the boundaries**

```bash
cd rust && grep -rn "bezel::" crates/ --include=*.rs | grep -v "crates/sirio_ui/src/loading.rs"
cd rust && grep -rn "GENERATING_SPINNER\|streaming_border_angle" crates/
cd rust && cargo tree -d | grep -i gpui
```

Expected: all three print nothing.

- [ ] **Step 3: Check for stray timers**

```bash
cd rust && grep -rn "background_executor().timer" crates/sirio_ui/src/chat.rs
```

Expected: only the caret blink (`caret::schedule`'s own discipline), no loading timer.

- [ ] **Step 4: Run the fuller Linux gate where available**

```bash
Scripts/ci-linux.sh 2>&1 | tail -30
```

Expected: passing or explicitly SKIP/BLOCKED stages per its own header.

- [ ] **Step 5: Commit any gate fix**

```bash
git add -A && git commit -m "fix: settle the Bezel loading migration against the gates"
```
