# Project Settings as a window-centred modal — Design

Date: 2026-08-27
Branch: `main`
Map: #154 · Children: #155, #156, #157

## Goal

Move Project Settings from a sheet trapped inside the sidebar's ~280px to a modal
centred over the whole window, at 720px.

`render_project_settings` (`sidebar.rs:2901`) is a `div().absolute()` **inside the
`Sidebar` entity**, so it is bounded by the sidebar's width — that is the entire
cause of the squeeze. `deferred()` changes paint order, not layout bounds, and
nothing in the repo escapes the sidebar's bounds today.

## What the squeeze actually looks like

Captured live from the running app before any change, because the report
understates it:

- `Colour` renders **one character per line** — `C / o / l / o / u / r`, six lines
  of height for one word.
- `Project icon` breaks across two lines.
- The six icon glyphs wrap **5 + 1**; the eight colour swatches wrap **7 + 1**,
  each orphaning its last item.
- The helper string splits **mid-path**: `D:` ends one line, `\Progetti\tiller`
  starts the next.
- `Use Primary` is jammed against the right edge, wedged between two baselines so
  it belongs to neither.

## Non-goals

- `render_project_form` and the worktree prompt (map: out of scope). The modal
  layer is designed to be reusable so they can follow as a separate effort.
- Project Settings as a category inside `settings.rs` — rejected mid-charting:
  this is per-project and opens from a row's context menu; Settings is global.
- A real separate OS window (decision 1). `cx.open_window` appears exactly once
  (`main.rs:13886`).
- A focus trap (decision 6, and see §6).

---

## 1. Architecture

State stays in the `Sidebar` entity (decision 4): `project_settings:
Option<ProjectSettingsCard>`, the three `FocusHandle`s, the drafts.
`Sidebar` exposes

```rust
pub fn render_settings_modal(&self, …) -> Option<AnyElement>
```

and `TillerWorkspace` (`main.rs`) renders it **last**, so it covers the window
rather than the sidebar. `SidebarEvent::ProjectSettingsChanged` is untouched.

## 2. The extracted backdrop — settled in #157

Extract **only** the backdrop from `modal.rs::render_modal`. `ModalSpec` stays
exactly as it is; Project Settings would use none of `body`, `buttons`,
`text_field` (decision 5).

```rust
pub fn modal_backdrop(id: &'static str, theme: Theme) -> Stateful<Div>
```

**The backdrop only — no width parameter, no card, no padding.** Two things vary
for Project Settings, not one: the width *and* the fact that the card holds
arbitrary content. A `width` parameter addresses only the first and still forces
`card(theme).p(px(16.0))` on every caller — which this sheet cannot accept,
because it needs its own padding and a footer pinned outside the scrolling body
(§4).

The helper owns exactly what is shared: `.absolute().inset_0().flex()
.items_center().justify_center()`, the scrim (`black().opacity(0.6)`), the
occlusion, and the id plus `debug_selector`. Width, padding, the card wrapper,
focus tracking and content stay at the call site.

`render_modal` keeps its current behaviour by calling the helper and adding its
own `card(theme).w(px(360.0)).p(px(16.0))`. **Its two existing users see no
change.**

### `.occlude()` — unconditional, inside the helper

Not a per-caller option. `sidebar.rs:2957-2960` documents why in its own comment,
and F-PRJ-13 records the symptom: without it, clicks leaked through to the sidebar
rows and the active worktree silently jumped to a different project the instant
Reset was clicked. At window scale the same hazard covers everything the backdrop
spans. A backdrop that does not occlude is not a backdrop.

### Backdrop-click dismissal is opt-in per caller

**Not baked into the helper**, because the two existing `render_modal` users are
not alike:

- **The terminal close confirmation** is a pure yes/no over a destructive action.
  Click-outside genuinely reads as Cancel, the safe branch. It gets dismissal, as
  a deliberate separate call.
- **"Set Title" holds typed text.** `render_title_prompt` (`main.rs:9604`) builds
  a `ModalTextField`; the user is mid-edit. Click-outside there is not Cancel, it
  is *discarding an edit with no confirmation*. **It does not get dismissal**, and
  the reason is recorded next to it so nobody adds it later for consistency.

If typed-text modals should ever dismiss on click-outside, the answer is a
confirm-on-discard, not a shared default.

## 3. Focus and dismissal — settled in #155

- A **backdrop-level handle** takes focus on open — never `display_name_focus`,
  whose first keystroke would silently rename the project.
- `handle_root_key_down` gains a case swallowing any key `window.focused(cx)`
  says is not bound for the modal.
- The three field handles become `tab_stop(true)` inside a `.tab_group()`.
- One `close_project_settings(window: Option<&mut Window>, …)` restores previous
  focus; the context-menu pre-emption passes `None`.
- No `FocusHandle` re-parenting: handles live in the window's `FocusMap`.

Esc closes; backdrop click closes. Edits persist per keystroke
(`on_display_name_key`, `apply_icon_change` emit on every change), so **no
dismissal can destroy work** (decision 6).

## 4. Layout — settled by prototype in #156

Prototype: branch `proto/156-project-settings`,
`cargo run -p tiller_ui --example project_settings_proto`. Throwaway.

Single column, **720px**, reusing `settings::CONTENT_WIDTH` (decisions 7, 8).
Three layouts were built and judged blind; **label-left won outright**.

### 4.1 A fixed label rail on the left

The four section labels — `Display name`, `Project icon`,
`Default Worktree Base`, `Worktree Location` — sit in a **fixed 180px rail**
(wide enough for the longest), with the control column `min_w_0` beside it.

Two things fall out at once, and the second is why it wins: every control gets a
**common left edge**, so the eye never re-finds where a row begins; and the
vertical budget drops enough that the sheet fits with slack instead of
overflowing.

Scanning is fastest here because the four section labels are the only text at
that x — nothing competes.

**Accepted cost:** roughly 110px of dead gutter beside the Colour and Reset rows,
which carry no label.

### 4.2 The footer is pinned OUTSIDE the scrolling body

**This corrects decision 7.** As written — `max_h` 80% plus
`.overflow_y_scroll()` over a body containing Remove Project and Close — the
footer scrolls away with the content. Two of the three prototype layouts lost it
outright: one ended at its last card with **no footer at all**, a sheet you cannot
leave; the other was sliced mid-row by the sheet's own bottom edge, `Choose…` cut
in half.

Only one layout happened to fit, and "happens to fit" is not a design. The
scrolling region is the **body**; the footer is a sibling of it, always on screen.

### 4.3 Below 720px — closes a map fog patch

Label-left **holds at 520px** with no separate layout: the rail stays 180px,
controls take the remainder, `Colour` stays on one line, the six glyphs stay on
one row, `Choose…` stays beside its field, and the helper text wraps **at a word
boundary** rather than mid-drive.

So the narrow case needs no alternate layout — it needs the rail fixed-width and
the control column `min_w_0`. Below 520px the modal should clamp to the window
width minus a margin rather than overflow it.

### 4.4 Remove Project's confirmation — closes the other fog patch

**Neither a modal-inside-a-modal nor an appended inline card.** The prototype
shows the inline version failing concretely: the question renders and its
Cancel/Remove buttons fall past the sheet's bottom edge. A destructive
confirmation whose buttons are unreachable is worse than none.

**The confirm replaces the footer row in place** — swapping `Remove Project` /
`Close` for the question and its two buttons. Inside the visible region by
construction, no second modal, and the confirmation appears exactly where the
action was.

## 5. Naming

The `project-settings-sheet` debug selector (`sidebar.rs:2927`) and the word
"sheet" stop being accurate once this is window-wide, and the repo would hold
three shapes under three names: `sheet`, `modal`/`backdrop`, `surface`.

**Rename to `modal` in this change**, not later: it touches test selectors, and
doing it with the tests already being rewritten (§7) is strictly cheaper than a
second pass.

## 6. Focus trap — still deferred, deliberately

#155 checked GPUI's `.tab_group()` (`div.rs:784`) and it is **not** a trap:
`TabStopMap::next` (`tab_stop.rs:111`) walks the global ordered tree and wraps to
the *window's* first stop, not the group's. Trapping Tab, if ever wanted, is
hand-written.

Deferred, and it returns as a ticket only if live-driving the 720px layout shows
it matters.

---

## 7. Tests — written first, per repo convention

1. The **8 tests in `sidebar.rs` that mount the sheet move to a local harness**
   modelled on `ModalHarness` (`modal.rs:262`) (decision 9).
2. `reset_button_click_does_not_leak_through_to_the_row_underneath`
   (`sidebar.rs:6403`) is **rewritten, not deleted** (decision 10). Its invariant
   — a click inside Project Settings never reaches the sidebar underneath — stays
   violable; only the mechanism changes. This is the test that catches a missing
   `.occlude()`.
3. `modal_backdrop` occludes: a click on the scrim does not reach an element
   rendered beneath it.
4. `render_modal`'s two existing users are unchanged — same drawn width (360px)
   and padding as before extraction.
5. **The footer is reachable at the tallest content**: with the body scrolled to
   the top, `Close` is drawn inside the window. This is the §4.2 guard and the one
   that would have caught the prototype's failure.
6. At **520px** the drawn width of every control stays inside the modal, and the
   label rail keeps its 180px.
7. Remove's confirmation replaces the footer row: after clicking Remove Project,
   `Close` is gone and both confirm buttons are drawn inside the modal.
8. Esc closes and restores previous focus; backdrop click closes.
9. Opening the modal focuses the **backdrop handle**, not `display_name_focus` —
   a first keystroke must not rename the project.
10. `project_settings_remove_project_confirms_before_emitting`
    (`sidebar.rs:7199`) still passes against the new confirmation shape.

## 8. Verification gate

`Scripts/ci.sh` must print `CI OK`.

Anything building `tiller_terminal` needs **Zig exactly 0.15.2** on PATH — a newer
Zig fails too. On Windows `Scripts/ci.sh` cannot pass: compare failing test
**names** against the machine's known baseline rather than counts.

## 9. Coordination

#151 (merged) removed the path line from sidebar rows and declared
`sidebar.rs:2961`'s `display_path` inside this sheet out of scope. At 720px that
path no longer truncates, so the two remain compatible.

## 10. Raspberry Pi 5

The modal is drawn only while open, is a single column of static controls, and
adds no polling, no timer and no background work. The backdrop replaces a
sidebar-local overlay with a window-level one — same element count, larger bounds.
No measurable idle cost either way.
