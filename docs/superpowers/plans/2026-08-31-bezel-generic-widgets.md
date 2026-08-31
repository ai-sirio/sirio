# Bezel Generic Widgets Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace `sirio_ui`'s hand-rolled tooltip, context menus and text inputs with bezel 0.1.4 equivalents by direct adoption, deleting the hand-rolled code, after wiring Sirio's branded theme into bezel's registry.

**Architecture:** Four implementation tasks on one branch (`feat/bezel-generic-widgets`), one commit each: theme wiring first (prerequisite — bezel components read `Theme::of(cx)` internally), then tooltip → menus → inputs in rising risk order. A final task compares full test results against the sub-project-1 baseline and gates the merge on a single user visual review.

**Tech Stack:** Rust, gpui `=0.3.8` (bezel-gpui), bezel `=0.1.4` (`bezel::ui::{tooltip, popover, input}`, `bezel::theme`).

**Spec:** `docs/superpowers/specs/2026-08-31-bezel-generic-widgets-design.md`

## Global Constraints

- NEVER run `Scripts/ci.sh` or `Scripts/ci-linux.sh` — only the user may request the gates. Iterate with `cargo build -p <crate>` / `cargo test -p <crate>` from `rust/`.
- Zig **exactly 0.15.2** on PATH for anything building `sirio_terminal` (`export PATH="/opt/homebrew/opt/zig@0.15/bin:$PATH"`; `zig version` must print 0.15.2; never brew-link or edit profiles).
- No user-visible behavior change beyond the bezel look: focus order, keyboard shortcuts, submit/dismiss semantics stay identical.
- Existing tests update only where they asserted rendering details of the old implementation; never where they assert behavior.
- Test failures compare as **lists of test names** against the baseline files (see Task 5); `sirio`'s known-red pool is 5 oscillating names.
- Each family's commit deletes exactly the hand-rolled code it obsoletes, nothing else. Conventional Commits, lower-case imperative.
- Sub-project-3 surfaces are off limits except where a step names an exact mechanical edit (the three tooltip lines in `changes.rs`): chat, composer, sidebar internals beyond its menus, changes/diff logic, editor/file_view, orbit, project_identity.
- `caret.rs` is NOT deleted — other surfaces still use it after this sub-project.

---

### Task 1: Wire Sirio's branded theme into bezel's registry

**Files:**
- Modify: `rust/crates/sirio_theme/src/lib.rs` (near `for_appearance`, line ~259, which already builds `bezel::theme::Theme::branded(&bezel::theme::Brand {...}, appearance)`; and `sync_appearance`, line ~1199)
- Modify: `rust/crates/sirio/src/main.rs` (every call site of `sync_appearance` gains the install call via the new helper — find them with `rg -n "sync_appearance" rust/crates/sirio/src/main.rs`)
- Test: `rust/crates/sirio_ui/src/conformance.rs`

**Interfaces:**
- Consumes: `sirio_theme::Theme`'s existing `for_appearance` internals (the `bezel::theme::Theme::branded(...)` expression) and `sync_appearance(&self, ...)`.
- Produces: `pub fn install_into_bezel(&self, cx: &mut gpui::App)` on `sirio_theme::Theme` — installs this theme's branded bezel palette via `bezel::theme::Theme::install_custom` so `bezel::theme::Theme::of(cx)` returns it. Tasks 2–4 rely on this being called wherever the app already calls `sync_appearance`.

- [ ] **Step 1: Extract the branded-theme builder**

In `sirio_theme/src/lib.rs`, the body of `for_appearance` builds `let bezel = bezel::theme::Theme::branded(&bezel::theme::Brand { ... }, match appearance { ... });`. Extract that expression into a method so it is buildable from an existing `Theme` too:

```rust
/// The bezel theme this Sirio theme is derived from — the same
/// `Theme::branded` call `for_appearance` uses, exposed so the app can
/// hand it to bezel's own registry (`install_into_bezel`).
pub fn to_bezel_theme(&self) -> bezel::theme::Theme {
    // move the existing `branded(...)` expression here, reading the
    // brand tint from `self.base_color` and the appearance from
    // `self.appearance`, exactly as `for_appearance` builds them today;
    // then make `for_appearance` call `self`-independent logic through a
    // shared private fn so the two can never drift.
}
```

Concretely: create `fn bezel_theme_for(base_color: BaseColor, appearance: Appearance) -> bezel::theme::Theme` holding the moved expression; `for_appearance` calls it, and `to_bezel_theme` forwards `Self { base_color, appearance, .. }`'s fields to it.

- [ ] **Step 2: Add the install helper**

```rust
/// Installs this theme's palette into bezel's own registry so bezel
/// components that read `bezel::theme::Theme::of(cx)` (tooltips, menu
/// chrome, inputs) resolve Sirio's branded palette instead of the
/// unbranded default. Must be called wherever `sync_appearance` is —
/// the two registries drift otherwise.
pub fn install_into_bezel(&self, cx: &mut bezel::gpui::App) {
    bezel::theme::Theme::install_custom(self.to_bezel_theme(), cx);
    self.sync_appearance();
}
```

If `install_custom`'s exact path differs, resolve it from the source: `~/.cargo/registry/src/*/bezel-theme-0.1.4/src/theme/install.rs` defines `install_custom(theme: Theme, cx: &mut App)` and `of(cx: &App) -> &Theme`.

- [ ] **Step 3: Write the failing conformance test**

In `rust/crates/sirio_ui/src/conformance.rs` (follow the file's existing test style and its `TestAppContext` setup — copy the setup shape of a neighboring test in that file):

```rust
#[gpui::test]
async fn install_into_bezel_makes_theme_of_return_the_branded_palette(
    cx: &mut gpui::TestAppContext,
) {
    let _guard = bezel::theme::lock_appearance();
    let theme = sirio_theme::Theme::for_appearance(
        sirio_theme::Appearance::Dark,
        sirio_theme::BaseColor::Neutral,
    );
    cx.update(|cx| theme.install_into_bezel(cx));
    cx.update(|cx| {
        let installed = bezel::theme::Theme::of(cx);
        assert_eq!(installed.accent, theme.to_bezel_theme().accent);
    });
}
```

Adapt the two constructor calls to `for_appearance`'s real signature (check it — if it takes only `Appearance`, drop the base-color argument and build with the current default) and pick a field that exists on `bezel::theme::Theme` (`accent`, or the first color field `to_bezel_theme` sets). The assertion's point: `Theme::of` returns what we installed, not the default.

- [ ] **Step 4: Run the test — expect failure**

```bash
cd rust && cargo test -p sirio_ui install_into_bezel_makes_theme_of -- --nocapture
```

Expected: compile error (`install_into_bezel` not found) before Step 2 is written, or assertion failure if the helper is stubbed. If it passes immediately, the test is vacuous — fix the test.

- [ ] **Step 5: Wire the call sites and pass**

In `rust/crates/sirio/src/main.rs`, at every place that calls `sync_appearance` (theme install at startup, appearance change, base-color change), replace the call with `theme.install_into_bezel(cx)` (which itself calls `sync_appearance`, so the old behavior is preserved). Re-run the Step 4 test: PASS. Then:

```bash
cd rust && cargo build -p sirio_theme && cargo build -p sirio_ui && cargo build -p sirio
cargo test -p sirio_theme
```

Expected: builds green; `sirio_theme` suite at its baseline.

- [ ] **Step 6: Commit**

```bash
git add rust/crates/sirio_theme/src/lib.rs rust/crates/sirio_ui/src/conformance.rs rust/crates/sirio/src/main.rs
git commit -m "feat: install branded sirio theme into bezel registry"
```

---

### Task 2: Tooltip family

**Files:**
- Modify: `rust/crates/sirio_ui/src/controls.rs` (delete `text_tooltip` lines ~33–66 incl. `TextTooltip` + its `Render` impl; update the internal call at line ~293)
- Modify: `rust/crates/sirio_ui/src/changes.rs:2321,2347,2375` (three `.tooltip(controls::text_tooltip(...))` call sites — mechanical swap only, nothing else in this file)

**Interfaces:**
- Consumes: Task 1's installed bezel theme (bezel `Tooltip` reads `Theme::of(cx)` in its render).
- Produces: no new API — `controls::text_tooltip` and `TextTooltip` cease to exist; all tooltips go through `bezel::ui::tooltip::Tooltip::text`.

- [ ] **Step 1: Swap the four call sites**

The old shape (e.g. `changes.rs:2321`):

```rust
.tooltip(controls::text_tooltip(tooltip, theme))
```

becomes (bezel's documented usage — `Tooltip::text(text, window, cx) -> AnyView` inside the `.tooltip` builder closure):

```rust
.tooltip(move |window, cx| bezel::ui::tooltip::Tooltip::text(tooltip.clone(), window, cx))
```

Notes: the `theme` argument disappears (bezel reads the installed theme); the moved variable is a `String`/`SharedString` — clone it inside the closure. Add `use bezel::ui::tooltip::Tooltip;` to each file's imports and write the call as `Tooltip::text(...)`. Apply the same shape at `changes.rs:2347` (variable `label`), `changes.rs:2375` (variable `tooltip`) and `controls.rs:293` (variable `tooltip`).

- [ ] **Step 2: Delete the hand-rolled implementation**

Remove from `controls.rs`: `pub fn text_tooltip(...)` (lines ~33–46), `struct TextTooltip` (~48–51), `impl Render for TextTooltip` (~53–66), and any now-unused imports the compiler reports (`AnyView` if unused elsewhere).

- [ ] **Step 3: Build and grep for leftovers**

```bash
cd rust && cargo build -p sirio_ui
rg -n "text_tooltip|TextTooltip" crates/
```

Expected: build green; grep returns nothing.

- [ ] **Step 4: Run the affected suites**

```bash
cd rust && cargo test -p sirio_ui 2>&1 | tail -3
cargo test -p sirio 2>&1 | tail -3
```

Expected: failure lists match baseline (Task 5 explains the comparison; a quick eyeball here is enough — no NEW names).

- [ ] **Step 5: Commit**

```bash
git add rust/crates/sirio_ui/src/controls.rs rust/crates/sirio_ui/src/changes.rs
git commit -m "refactor: adopt bezel tooltip, delete TextTooltip"
```

---

### Task 3: Menus/popover family

**Files:**
- Modify: `rust/crates/sirio_ui/src/sidebar.rs` (`render_context_menu` ~2758, `render_add_project_menu` ~2837, `open_context_menu`/`close_context_menu` ~1125–1136, and the `Option<OpenContextMenu>` field they manage)
- Modify: `rust/crates/sirio_ui/src/tab_bar.rs` (`render_tab_context_menu` ~117, the overflow menu around `menu_open` ~194–309)
- Modify: `rust/crates/sirio_ui/src/right_panel/files.rs` (`open_file_context_menu` ~276, `close_file_context_menu` ~292, and the `FileContextMenu` render site — find with `rg -n "file_context_menu" …/files.rs`)
- Modify: `rust/crates/sirio/src/main.rs` (`render_tab_context_menu` ~10984 — the twin the sidebar comment F-SID-15 references)

**Interfaces:**
- Consumes: Task 1's installed theme; bezel API `bezel::ui::popover::{Popup, reap_popup, menu_at, anchored_menu_below, popover_card, menu_row, divider}` (source of truth: `~/.cargo/registry/src/*/bezel-ui-0.1.4/src/popover.rs`; every helper is documented there with its intended pairing).
- Produces: no new public API; each menu's open-state field becomes `popover::Popup<T>` where `T` is the existing payload type (`OpenContextMenu`, `FileContextMenu`, the overflow menu's `bool` becomes `Popup<()>`).

- [ ] **Step 1: Transform one menu completely (sidebar context menu) as the template**

State: the field holding `Option<OpenContextMenu>` becomes `popover::Popup<OpenContextMenu>`; `open_context_menu` calls `self.context_menu.open(payload)`, `close_context_menu` calls `self.context_menu.begin_close()` followed by `popover::reap_popup(...)` scheduling (see `reap_popup`'s doc for the exact pairing — it holds the state alive while the exit animation plays).

Render: the hand-built card in `render_context_menu` (absolute-positioned `div` + manual border/bg/shadow + `deferred(...).with_priority(1)`) becomes:

```rust
let menu = popover::menu_at(
    // id, position, and the card content — match menu_at's real
    // signature from popover.rs:613; it wraps deferred+anchored itself.
);
```

with the card body `popover::popover_card(&theme.to_bezel_theme())` (or the bezel theme via `Theme::of(cx)` where a `cx` is at hand) and each item row `popover::menu_row(...)` carrying the existing `debug_selector`, `on_click` dispatch to `dispatch_context_action`, the disabled state (text_faint, no hover, no on_click) and the `disabled_reason` trailing label. Outside-click dismissal moves to the helper's built-in `on_mouse_down_out` (verify `menu_at` provides it; if not, keep the existing `.on_mouse_down_out(...)` wrapper on the helper's output).

**F-SID-15 guard:** the old code needed `deferred(...).with_priority(1)` so later siblings could not paint over the menu. bezel's helpers wrap `deferred` themselves — after the swap, run the existing sidebar context-menu tests before touching the next menu:

```bash
cd rust && cargo test -p sirio_ui sidebar 2>&1 | tail -3
```

Expected: same failure list as baseline (no new names). If a click-through/paint-order test fails, add `.with_priority(1)` equivalent per the helper's API rather than reverting to a hand-rolled card.

- [ ] **Step 2: Apply the template to the remaining four menus**

Same transformation, one menu at a time, building after each: sidebar add-project menu (`render_add_project_menu` — an anchored trigger menu: use `anchored_menu_below` on the + button rather than `menu_at`), tab_bar tab context menu (`render_tab_context_menu`), tab_bar overflow menu (`menu_open: bool` → `Popup<()>`; `toggle_menu` uses `note_trigger_press`/`take_press_was_open` so clicking the trigger while open closes it — that pair exists precisely for this), files context menu, and `sirio/src/main.rs`'s `render_tab_context_menu` twin (same F-SID-15 comment applies there; run `cargo test -p sirio` names containing `tab_context` or `context_menu` after it).

- [ ] **Step 3: Delete obsolete hand-rolled menu scaffolding**

After all five: remove now-unused menu card styling helpers, `deferred` imports and dead `OpenContextMenu`-adjacent plumbing that the compiler or `rg -n "on_mouse_down_out" crates/sirio_ui/src/sidebar.rs crates/sirio_ui/src/tab_bar.rs` shows unused. Do not touch other `deferred` uses.

- [ ] **Step 4: Full crate suites**

```bash
cd rust && cargo test -p sirio_ui 2>&1 | tail -3 && cargo test -p sirio 2>&1 | tail -3
```

Expected: failure lists match baseline.

- [ ] **Step 5: Commit**

```bash
git add rust/crates/sirio_ui/src/sidebar.rs rust/crates/sirio_ui/src/tab_bar.rs rust/crates/sirio_ui/src/right_panel/files.rs rust/crates/sirio/src/main.rs
git commit -m "refactor: adopt bezel popover for context menus"
```

---

### Task 4: Input family

**Files:**
- Modify: `rust/crates/sirio/src/main.rs` (bootstrap: call `bezel::ui::input::init(cx)` once where gpui actions/keybindings are registered)
- Modify: `rust/crates/sirio_ui/src/settings.rs` (the caret-based fields: `field_blink` ~1046/1187, blink tick ~1991, and each field's render + key handling)
- Modify: `rust/crates/sirio_ui/src/project_forms.rs` (`CloneForm.blink` ~143, `CreateForm` fields — every `caret::Blink` user in the file, 18 references)
- Modify: `rust/crates/sirio_ui/src/browser.rs` (`AddressEditor` ~339–…: `text/anchor/caret` + its editing methods `selection` ~360, `select_all` ~369, and the render; `normalize_address`/`submit_address` logic stays untouched)
- Test: existing suites in the same files (`cookie_save_failure_keeps_the_input…` in settings, `address_submission_normalizes_http…` in browser, form tests in project_forms)

**Interfaces:**
- Consumes: Task 1's theme; bezel `input::{init, TextField, Shape}` — `TextField::new(cx)`, `.with_placeholder(…)`, `.with_key_context(…)`, `.content() -> &SharedString`, `.set_content(…, cx)` (source: `~/.cargo/registry/src/*/bezel-ui-0.1.4/src/input.rs`; `init` binds backspace/delete/arrows/selection keys under `KEY_CONTEXT`).
- Produces: no new API; the three sites hold a `gpui::Entity<bezel::ui::input::TextField>` instead of string+anchor+caret+Blink state.

- [ ] **Step 1: Wire `input::init` into bootstrap and tests**

In `sirio/src/main.rs`, alongside the app's existing `cx.bind_keys`/action registration at startup, add `bezel::ui::input::init(cx);`. Every `TestAppContext` setup that will exercise a converted field needs the same call — find the shared test-setup helper in each affected file (settings, project_forms, browser tests build their app context somewhere common) and add `cx.update(|cx| bezel::ui::input::init(cx));` there. Without it, arrow/backspace keys do nothing in the new fields and the behavior tests fail — that failure is the signal the init is missing, not a reason to re-add hand-rolled key handling.

- [ ] **Step 2: Convert the browser address bar first (smallest, best-tested)**

Replace `AddressEditor { text, anchor, caret }` state with an `Entity<TextField>` created in the pane's constructor (`TextField::new(cx).with_placeholder("…")` keeping the current placeholder text). Reads of `self.address.text` become `field.read(cx).content().to_string()`; writes become `field.update(cx, |f, cx| f.set_content(new, cx))`. `select_all`/`selection` logic is deleted — TextField owns selection. Submit: keep the existing Enter handling at the pane level calling `submit_address(&content)`; `normalize_address` and `submit_address` do not change. Run:

```bash
cd rust && cargo test -p sirio_ui browser 2>&1 | tail -3
```

Expected: baseline list (the address tests assert normalize/submit behavior, which is untouched).

- [ ] **Step 3: Convert project_forms, then settings, building and testing after each**

Same shape: each `caret::Blink` + string + caret-index cluster becomes one `Entity<TextField>`; per-field placeholder/validation copy stays; submit/validation logic reads `content()`. Delete the per-file blink tick handlers that no longer have users (settings ~1991, project_forms ~266). `caret.rs` itself stays.

```bash
cd rust && cargo test -p sirio_ui project_forms 2>&1 | tail -3
cargo test -p sirio_ui settings 2>&1 | tail -3
```

Expected: baseline lists; tests that asserted caret rendering details of the old editor (if any name `caret`/`blink` in these files) are updated to assert through `TextField` content/selection instead — behavior assertions (content kept on save failure, normalization, validation messages) must pass unmodified.

- [ ] **Step 4: Full suites**

```bash
cd rust && cargo test -p sirio_ui 2>&1 | tail -3 && cargo test -p sirio 2>&1 | tail -3
```

Expected: failure lists match baseline.

- [ ] **Step 5: Commit**

```bash
git add rust/crates/sirio/src/main.rs rust/crates/sirio_ui/src/settings.rs rust/crates/sirio_ui/src/project_forms.rs rust/crates/sirio_ui/src/browser.rs
git commit -m "refactor: adopt bezel TextField for settings, forms and address bar"
```

---

### Task 5: Baseline comparison, visual review, merge readiness

**Files:**
- None (verification and gate only).

**Interfaces:**
- Consumes: the four commits above; baseline failure lists.

- [ ] **Step 1: Establish the baseline lists**

Use `/tmp/bezel-bump/after-ab-{sirio_theme,sirio_ui,sirio}.failed` (the post-sub-project-1 state) if they still exist. If missing, regenerate on the branch's merge-base:

```bash
mkdir -p /tmp/bezel-widgets && cd rust
for c in sirio_theme sirio_ui sirio; do
  git stash --quiet 2>/dev/null || true   # only if uncommitted work; normally none
  cargo test -p "$c" 2>&1 | grep -E "^test .* FAILED" | sort > /tmp/bezel-widgets/baseline-$c.failed
done
```

(If regenerating, do it BEFORE the branch's commits via a detached checkout of the merge base in a temp worktree — never by reverting the branch.)

- [ ] **Step 2: Final comparison on the branch**

```bash
cd rust
export PATH="/opt/homebrew/opt/zig@0.15/bin:$PATH"
for c in sirio_theme sirio_ui sirio_terminal sirio; do
  cargo test -p "$c" 2>&1 | grep -E "^test .* FAILED" | sort > /tmp/bezel-widgets/final-$c.failed
  diff /tmp/bezel-bump/after-ab-$c.failed /tmp/bezel-widgets/final-$c.failed && echo "$c: no new failures"
done
```

Expected: `no new failures` for every crate (for `sirio_terminal` compare against an empty list — it was green). A new name that survives a solo re-run (`cargo test -p <crate> <test_name>`) is a regression: fix it in the family commit that caused it (amend or follow-up commit in the same family scope) before proceeding.

- [ ] **Step 3: Visual review (mandatory user gate)**

```bash
Scripts/build-dev.sh
```

Ask the user to check, light and dark: a tooltip (hover a changes toolbar icon), each converted menu (sidebar right-click, sidebar +, tab context, tab overflow, files panel right-click), and each converted input (settings provider field, clone/create project forms, browser address bar — typing, selection, Enter). Wait for explicit approval; on rejection, record what and stop.

- [ ] **Step 4: Merge readiness report**

Report to the coordinator: branch name, the four commit hashes, the comparison outputs, and any deviation taken. The merge into `main` is the coordinator's fan-in, not this task's.
