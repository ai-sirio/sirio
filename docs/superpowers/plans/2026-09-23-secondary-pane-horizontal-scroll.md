# Secondary Content Horizontal Scroll Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Show draggable horizontal scrollbars when text files or Changes diff code exceed the width of the Secondary pane; give Split diff columns independent horizontal positions.

**Architecture:** Keep the existing full-size child surfaces and their vertically virtualized lists. FileView exposes its list's existing X scroll via a horizontal control. ChangesTab keeps one vertical `ListState`, renders untruncated code inside fixed-width clipped cells, and stores separate X offsets for Unified, Split-left and Split-right. The small horizontal bar uses bezel's thumb geometry and the app's theme instead of adding a dependency.

**Tech Stack:** Rust, GPUI (`bezel-gpui` 0.3.8), bezel 0.1.4, `sirio_ui` drawn-frame `#[gpui::test]` tests.

**Spec:** `docs/superpowers/specs/2026-09-23-secondary-pane-horizontal-scroll-design.md`

## Global Constraints

- `main.rs`'s pane geometry, splitter and tab strip are unchanged; no outer scrolling wrapper around `secondary-surface`.
- File source and Changes code only; Markdown preview, Browser, Project Settings and empty launcher are unchanged.
- Changes retains **one** vertical `ListState`; file/section headers, diff line numbers, signs, separator and toolbar stay fixed, and neither pane nor column grows to the longest line.
- Split columns have independent X offsets; all bars disappear when their content fits. No database changes or new dependencies.
- Bezel has a vertical scrollbar only: reuse `bezel::ui::scroll::{thumb, offset_for_thumb, MIN_THUMB}` and `bezel::theme::ink` for horizontal bar geometry/styling.
- Never run `Scripts/ci.sh` or `Scripts/ci-linux.sh` without the user's explicit request. Use targeted `cargo test -p sirio_ui` and any affected `cargo test -p sirio <test>`.

## Review Focus

- A wide Unicode/fallback-font line: the last glyph must be reachable; test shaped width, not UTF-8 byte length (Task 3).
- A pane resized below/above the width of an expanded diff: offset clamps to the new maximum and the bar vanishes when it fits (Task 3).
- A collapsed file/section or a background diff refresh: no stale extent or bar from the previously visible rows (Task 3).
- Split overflow on only one side, and switching modes: the opposite side and vertical list position must remain unchanged (Task 4).
- A source file with both tall and wide content: dragging the horizontal thumb must not change Y or the vertical bar; the Markdown Preview must remain unaffected (Task 2).

---

## File map

- Create `rust/crates/sirio_ui/src/horizontal_scroll.rs`: one small horizontal thumb with bezel geometry and GPUI drag, given viewport/extent/offset by callers; it does not own a content wrapper or a vertical list.
- Modify `rust/crates/sirio_ui/src/lib.rs`: register the private helper module.
- Modify `rust/crates/sirio_ui/src/file_view.rs`: hold a drag state beside `SurfaceScroll::source`, draw a conditional bottom overlay using the source list's `base_handle`, and add drawn tests.
- Modify `rust/crates/sirio_ui/src/changes.rs`: store per-mode/code-region offsets and cached extents in `ChangesTab`; measure the expanded code text, clip/shift code only, draw bars inside body, add drawn tests. Tests in this file construct `ChangesTab` literally at multiple sites: update those fixtures when adding fields.
- Keep `rust/crates/sirio/src/main.rs` untouched unless a drawn shell-boundary test reveals a genuine ancestor clipping bug.

### Task 1: Horizontal thumb control

**Files:** Create `rust/crates/sirio_ui/src/horizontal_scroll.rs`; modify `rust/crates/sirio_ui/src/lib.rs` (near other private modules).

**Interfaces:** Produce `pub(crate) struct HorizontalBarState` (`Default`, cloneable), `fn horizontal_range(viewport: Pixels, overflow: Pixels, offset: Pixels) -> Option<Range<Pixels>>`, and `pub(crate) fn bar(id: &'static str, viewport: Pixels, overflow: Pixels, offset: Pixels, state: &HorizontalBarState, on_offset: impl Fn(Pixels, &mut App) + Clone + 'static) -> AnyElement`. `offset` is negative, `overflow = max(content_width - viewport, 0)`. File and Changes provide their own source of geometry and setter. `bar` returns `gpui::Empty` if `horizontal_range` is `None`.

- [ ] **Step 1: Register `mod horizontal_scroll;` in `lib.rs` and write the failing geometry/drag-clamp test in its new file.** The test calls `horizontal_range`, which is not yet defined: the red build must fail rather than silently running zero tests. `offset_for_thumb` maps the midpoint to `-50`, clamps a drag beyond the end to `-100`, and zero overflow returns no range.

```rust
#[test]
fn thumb_geometry_is_horizontal_and_clamped() {
    let range = horizontal_range(px(100.), px(100.), px(-50.)).unwrap();
    assert_eq!(bezel::ui::scroll::offset_for_thumb(range.start, px(100.), px(100.), range.end - range.start), px(-50.));
    assert_eq!(bezel::ui::scroll::offset_for_thumb(px(999.), px(100.), px(100.), range.end - range.start), px(-100.));
    assert!(horizontal_range(px(100.), px(0.), px(0.)).is_none());
}
```

- [ ] **Step 2: Run the test red.** `cd rust && cargo test -p sirio_ui thumb_geometry_is_horizontal_and_clamped` → compile error: `horizontal_range` is undefined. Long-running compile: launch via `bg_run`.
- [ ] **Step 3: Implement the shared horizontal control.** A bottom-aligned absolute track (`h(px(10.0)).left_0().right_0().bottom_0()`), thumb at `range.start` with width `range.end - range.start`, `ink(0.2)` and hover `ink(0.32)`, a unique `ScrollbarDrag(id.into())` payload, a `Rc<Cell<Option<Pixels>>>` grab distance in `HorizontalBarState`. On `on_drag_move`, ignore other IDs; use `event.event.position.x - event.bounds.left()`, subtract first grab distance, then call `scroll::offset_for_thumb(..., viewport, overflow, thumb_width)` and `on_offset(result, cx)`. Clear the grab on left mouse-up and mouse-up-out. Use the same `on_drag(..., |_,_,_,cx| cx.new(|_| gpui::Empty))` preview pattern as bezel's vertical scrollbar. Expose a `debug_selector` on the track and thumb for drawn tests.

```rust
#[derive(Clone, Default)]
pub(crate) struct HorizontalBarState {
    grab: std::rc::Rc<std::cell::Cell<Option<gpui::Pixels>>>,
}
fn horizontal_range(viewport: gpui::Pixels, overflow: gpui::Pixels, offset: gpui::Pixels)
    -> Option<std::ops::Range<gpui::Pixels>>
{
    bezel::ui::scroll::thumb(viewport, overflow, offset, bezel::ui::scroll::MIN_THUMB)
}
// Inside `bar`, after `let Some(range) = horizontal_range(...) else { return gpui::Empty.into_any_element(); };`
let pointer = event.event.position.x - event.bounds.left();
let grab = state.grab.get().unwrap_or_else(|| {
    let grab = (pointer - range.start).clamp(px(0.), range.end - range.start);
    state.grab.set(Some(grab));
    grab
});
on_offset(bezel::ui::scroll::offset_for_thumb(
    pointer - grab, viewport, overflow, range.end - range.start,
), cx);
```

- [ ] **Step 4: Run the test green.** `cd rust && cargo test -p sirio_ui thumb_geometry_is_horizontal_and_clamped` → PASS; LSP diagnostics on the two files must show no errors.
- [ ] **Step 5: Commit.** `git add rust/crates/sirio_ui/src/{horizontal_scroll.rs,lib.rs} && git commit -m "feat(ui): add horizontal thumb for overflow"`.

### Task 2: File source scrollbar

**Files:** Modify and test `rust/crates/sirio_ui/src/file_view.rs` (SurfaceScroll near line 216, `render_content` near line 1839, source container near line 2020, drawn tests near line 5524).

**Interfaces:** Consume `horizontal_scroll::{bar, HorizontalBarState}` from Task 1. `SurfaceScroll` owns one `source_horizontal_bar`; use `scroll.source.0.borrow().base_handle.clone()` to obtain the handle (as for the existing vertical bar). Do not replace or reflow `uniform_list`.

- [ ] **Step 1: Add drawn tests first.** Follow `a_long_source_file_shows_a_scrollbar_and_a_short_one_does_not`: mount a file with a 400-character line and 400 rows, refresh a second frame, assert `debug_bounds("file-horizontal-bar")` is present; mount a short file and assert it is absent. Use a `md` file in Preview and assert absent. For the long file simulate a thumb drag toward its right edge and read `scroll.source.0.borrow().base_handle.offset()` from the mounted `FileView`: X becomes negative, Y remains its previous value; the vertical `file-text-bar` still exists.

```rust
let wide = TempFile::with_extension("rs", &format!("{}\n", "w".repeat(400)));
let (mut cx, view) = mounted_file_view(cx, wide.path().to_path_buf());
cx.update(|window, cx| { window.refresh(); window.simulate_next_frame(cx); });
assert!(cx.debug_bounds("file-horizontal-bar").is_some());
let before_y = view.read_with(&cx.cx, |v, _| v.scroll.source.0.borrow().base_handle.offset().y);
let thumb = cx.debug_bounds("file-horizontal-bar-thumb").unwrap();
let end = gpui::point(cx.debug_bounds("file-horizontal-bar").unwrap().right() - px(2.), thumb.center().y);
cx.simulate_mouse_down(thumb.center(), MouseButton::Left, Modifiers::none());
cx.simulate_mouse_move(end, MouseButton::Left, Modifiers::none());
cx.simulate_mouse_up(end, MouseButton::Left, Modifiers::none());
cx.run_until_parked();
view.read_with(&cx.cx, |v, _| {
    let position = v.scroll.source.0.borrow().base_handle.offset();
    assert!(position.x < px(0.));
    assert_eq!(position.y, before_y);
});
```

- [ ] **Step 2: Run red.** `cd rust && cargo test -p sirio_ui file_horizontal` → the wide-file bar assertion fails.
- [ ] **Step 3: Add one `HorizontalBarState` to `SurfaceScroll`, initialize it in `new`, and draw `bar(...)` inside the existing `.relative()` source viewport, beside the vertical bar.** Read `handle.bounds().size.width`, `handle.max_offset().x`, and `handle.offset().x`; in the setter call `handle.set_offset(point(x, handle.offset().y))` and notify the view. Like `file_view::scrollbar`'s canvas, request one follow-up animation frame if first layout reveals X overflow but the previous frame could not draw the thumb. Keep the Markdown Preview branch untouched.

```rust
let handle = scroll.source.0.borrow().base_handle.clone();
let drag_handle = handle.clone();
// child after `lines` and before/after the vertical bar in the source container:
.child(horizontal_scroll::bar(
    "file-horizontal-bar", handle.bounds().size.width, handle.max_offset().x,
    handle.offset().x, &scroll.source_horizontal_bar,
    move |x, cx| {
        drag_handle.set_offset(gpui::point(x, drag_handle.offset().y));
        entity.update(cx, |_, cx| cx.notify());
    },
))
```

- [ ] **Step 4: Run green.** `cd rust && cargo test -p sirio_ui file_horizontal` and `cd rust && cargo test -p sirio_ui a_long_source_file_shows_a_scrollbar_and_a_short_one_does_not` → PASS. Probe LSP diagnostics for `file_view.rs`.
- [ ] **Step 5: Commit.** `git add rust/crates/sirio_ui/src/file_view.rs && git commit -m "feat(file): show horizontal scrollbar for long lines"`.

### Task 3: Unified diff code viewport

**Files:** Modify and test `rust/crates/sirio_ui/src/changes.rs` (`ChangesTab` near line 505, `section_rows` near line 1290, `render_change_row` near line 1490, `render_diff_line` near line 2030, `render_body` near line 2410). Update all literal `ChangesTab { ... }` test fixtures in the same file.

**Interfaces:** Consume `horizontal_scroll::{bar, HorizontalBarState}` from Task 1. Produce `ChangesTab` unified X position, measured content extent and code viewport; split code will consume the same offset/extent pattern in Task 4. Expose no public API. The fixed gutter is `DIFF_NUMBER_WIDTH * 2 + DIFF_SIGN_WIDTH + 3 * DIFF_ROW_GAP + 2 * DIFF_ROW_PADDING` relative to the list width; use matching constants in the row and bar positioning.

- [ ] **Step 1: Add red tests using the existing `wide_line_fixture` (near line 5425) and `ChangesTab` drawn fixtures.** With Unified selected, expand `wide_line_fixture`'s changed file and assert `changes-unified-horizontal-bar` appears while the toolbar and `changes-surface` share the pane's bounded width. With a short line / a collapsed file / a git error, assert the bar is absent. Assert a line containing wide Unicode characters has a measured extent beyond the code viewport and can be reached by a drag. Simulate narrowing then widening the window: X clamps into `[-max, 0]` and eventually resets to zero. Snapshot replacement with shorter text also clears overflow; include an expanded off-screen long row and assert the bar's extent does not change when only Y scrolls.

```rust
// Use `wide_line_fixture(&dir.0)` followed by `changes_view` / `wait_for_tab`:
let row = cx.debug_bounds("changes-file-row").unwrap();
cx.simulate_click(row.center(), Modifiers::none());
cx.run_until_parked();
assert!(cx.debug_bounds("changes-unified-horizontal-bar").is_some());
let bounds = cx.debug_bounds("changes-surface").unwrap();
assert!(cx.debug_bounds("changes-unified-horizontal-bar").unwrap().right() <= bounds.right());
```

- [ ] **Step 2: Run red.** `cd rust && cargo test -p sirio_ui changes_unified_horizontal` → bar is absent.
- [ ] **Step 3: Add `unified_x` and cached max code width to `ChangesTab` (with `HorizontalBarState`), invalidated on `apply_snapshot`, expansion/collapse and a mode change.** Use `section_rows` / `sync_list_rows` to determine exactly the loaded/expanded code lines, including rows outside the current viewport; never rely on `list_fingerprint`, which deliberately ignores text. Measure line width using `window.text_system().layout_line(content, theme.typography.scaled(12.0), &[gpui::TextRun { len: content.len(), font: gpui::font(theme.typography.code_family), color: theme.text.into(), ..Default::default() }], None).width()` (match the row's font). Store the extent across vertical scroll frames; clamp X when the measured width, mode, or pane width changes. A `canvas` laid out inside the body reports the viewport width after layout, requests one repaint on change and never makes the list wider.

```rust
fn clamped_x(x: Pixels, content: Pixels, viewport: Pixels) -> Pixels {
    x.clamp(-(content - viewport).max(px(0.0)), px(0.0))
}
// Assert clamped_x(px(-100.), px(400.), px(400.)) == px(0.)
// and clamped_x(px(-100.), px(400.), px(350.)) == px(-50.).
```

- [ ] **Step 4: Change only `ChangeRow::Line`'s code child:** keep both number gutters and marker unchanged; replace `.text_ellipsis()` with a clipped `.relative().overflow_hidden()` code viewport containing a full-width, non-wrapping text child shifted left by `-unified_x` (the shared offset is negative). Do not let that child contribute intrinsic width to the flex row: position it absolutely, as the existing split cell does. Overlay the bar at the bottom of `changes-list`, aligned with the code viewport, using `bar("changes-unified-horizontal-bar", viewport, max_width - viewport, unified_x, ...)`; do not wrap the `list` in an unbounded scroll container.

```rust
// Replace the last child of `render_diff_line`; fixed gutters are untouched.
.child(div().flex_1().min_w_0().relative().overflow_hidden().child(
    div().absolute().left(unified_x).whitespace_nowrap().child(line.content)
))
```

- [ ] **Step 5: Run green.** `cd rust && cargo test -p sirio_ui changes_unified_horizontal` plus the existing Changes rendering tests → PASS. Probe LSP diagnostics on `changes.rs`; verify the long line never expands `changes-surface` bounds.
- [ ] **Step 6: Commit.** `git add rust/crates/sirio_ui/src/changes.rs && git commit -m "feat(changes): scroll unified diff code horizontally"`.

### Task 4: Independent Split diff columns and cross-surface verification

**Files:** Modify and test `rust/crates/sirio_ui/src/changes.rs` (`render_split_line`, `split_cell`, `render_body`, existing split drawn tests around line 5630). Touch `rust/crates/sirio/src/main.rs` only if the narrow-pane shell test finds a separate clipping issue.

**Interfaces:** Extend Task 3's per-tab offset/extent state with `split_left_x`, `split_right_x`, each with its own `HorizontalBarState`; reuse the same `clamped_x` and bar from Tasks 1/3. No second vertical list.

- [ ] **Step 1: Write drawn tests for independent columns.** Extend a `wide_line_fixture`-style git fixture so the old diff line is long and the replacement is short. In Split, expand it and assert `changes-split-left-horizontal-bar` exists and right bar does not. Drag the left thumb to its end and assert left X is negative while right X remains zero; assert both split cells and the full-width hunk header retain their old drawn alignment. Then load a long addition too, drag the right thumb and assert left X is unchanged. Switch Unified → Split → Unified; each mode retains a valid clamped position. Collapse all expanded files and check both bars disappear.

```rust
let left = cx.debug_bounds("changes-split-left").unwrap();
let right = cx.debug_bounds("changes-split-right").unwrap();
assert!((f32::from(left.size.width) - f32::from(right.size.width)).abs() < 1.0);
assert!(cx.debug_bounds("changes-split-left-horizontal-bar").is_some());
assert!(cx.debug_bounds("changes-split-right-horizontal-bar").is_none());
```

- [ ] **Step 2: Run red.** `cd rust && cargo test -p sirio_ui changes_split_horizontal` → no Split bars exist.
- [ ] **Step 3: Implement per-side measurements and painting.** Inspect all `ChangeRow::SplitLine` items from the expanded row stream; measure `left.content` and `right.content` separately with the Task 3 text-layout method (ignore a `None` cell). Reuse the existing equal-width split row, separator, and `split_cell`'s clipped absolute interior. Keep each number and sign fixed; shift only its untruncated code text by that side's X offset. Compute side code viewport from half the list width minus number/marker/gap/padding; position one bar along the bottom of each half's code viewport. Clamp each offset independently after layout or diff updates. Do not alter split row vertical heights.

```rust
// Within `split_cell`, after the fixed number and sign children:
.child(div().flex_1().min_w_0().relative().overflow_hidden().child(
    div().absolute().left(side_x).whitespace_nowrap().child(line.content)
))
// render_body overlays one bar per side, supplying its own max width/viewport/offset.
```

- [ ] **Step 4: Run green and verify.** `cd rust && cargo test -p sirio_ui changes_split_horizontal`, `cd rust && cargo test -p sirio_ui clicking_split_draws_paired_context_zipped_runs_and_full_width_hunks`, then `cd rust && cargo test -p sirio_ui` → PASS; run these through `bg_run` if long. Probe LSP diagnostics for changed paths and `git diff --check`. If the Changes surface exceeds the shell's `pane-secondary` in a narrow GPUI test, fix only the offending boundary and run `cd rust && cargo test -p sirio <test_name>`; otherwise leave `main.rs` alone.
- [ ] **Step 5: Commit.** `git add rust/crates/sirio_ui/src/changes.rs && git commit -m "feat(changes): scroll split diff columns independently"` (add `main.rs` only if its actual boundary needed fixing).

## Completion handoff

Inspect `git status --short`; report each targeted test actually run and its result. Do not claim the repo-wide CI gate passed: it requires an explicit user request. Review the final diff against the spec, including the five Review Focus cases above.
