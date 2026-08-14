# T4-tab build plan — F-TAB, 14 rows

Read-only triage output. No verdicts changed, no code touched, app not built or launched.
Each section names what a row actually needs and which files a fix would touch, per
`docs/linux-rewrite/triage/T4-tab.md`. Worktree: `/home/enzopalmisano/Scrivania/Progetti/tiller-linux`,
branch `linux/gpui-waku`.

## Shared cause — the tab-bar popover paints behind the terminal, not on top of it

**F-TAB-02, F-TAB-12, F-TAB-13 (second conjunct), F-TAB-14, F-TAB-15, F-TAB-17, F-TAB-21 — seven
of fourteen rows — are one defect, not seven.** Every one of them is blocked on the same
observable: a right-click (or the overflow chevron click) flips the right boolean state, but
no menu is ever visible on the real screen. P104 independently drove this gesture at least six
times across different rows/sessions and got the identical "nothing appeared" result every
time, while the *terminal's own* right-click menu (a structurally different code path) works
reliably in the same sessions — that contrast is the tell.

Read the render tree (`rust/crates/tiller/src/main.rs:6437-6454`):

```rust
columns = columns.child(
    div().flex().flex_col().flex_1().h_full().bg(theme.background)
        .child(
            div().relative().h(px(TAB_BAR_HEIGHT)).w_full()
                .child(self.tab_bar.clone())
                .child(self.render_open_tabs(*theme, entity.clone(), window, cx))
                .when(self.tab_menu_open, |this| {
                    this.child(self.render_tab_context_menu(*theme, entity.clone()))
                }),
        )
        .child(
            div().id("centre-surface").flex_1().w_full().overflow_hidden()
                .child(centre_surface),
        ),
);
```

The tab-menu popover (`render_tab_context_menu`, main.rs:5900-5923) and the overflow dropdown
(`render_overflow_menu`, main.rs:6170-6249, mounted at :6344-6351) are both plain `.absolute()`
children of a `.relative()` box whose own declared height is exactly `TAB_BAR_HEIGHT` — and both
popovers are positioned with `.top(px(TAB_BAR_HEIGHT))` / `top(theme.spacing...height)`, i.e.
they are *supposed* to extend downward past that box's own bottom edge, into the screen area
the very next sibling (`centre-surface`, the terminal/pane content, `flex_1()`) also paints
into. Neither popover is wrapped in anything that removes it from normal stacking order, so
ordinary paint order applies: `centre-surface` is declared after the tab-bar container and
paints over it wherever the two overlap — which is exactly where the popover renders. The
state toggle is real (hence the "faint highlight near the chevron" F-TAB-02 reports — that's
the button's own hover/active rect, which doesn't extend past the box and so isn't covered);
the popover's own content is real (the drawn `TestAppContext` suite passes, because those tests
check `debug_bounds`/tree presence, not actual occluded-pixel visibility); only the on-screen
result is wrong.

This is not a new failure mode in this codebase — it is the *same* class of bug already named
in `ENVIRONMENT.md`'s right-click notes: "the Files panel paints over the context menu... The
menu itself is fine — that is a z-order defect... paint order is ours to fix." That entry
concerns a different popover (the file-row context menu in `right_panel.rs`, which uses GPUI's
`anchored()`), and it is worth noting `anchored()` alone does **not** fix this — it only
controls *position*, not *paint order* (confirmed by reading GPUI's own source: `anchored()` in
`crates/gpui/src/elements/anchored.rs` has no paint-order logic at all). The primitive that
actually escapes normal stacking is `gpui::deferred(child).with_priority(n)`
(`crates/gpui/src/elements/deferred.rs`): "delays the painting of its child until after all of
its ancestors." GPUI's own prompt dialogs (`Window::draw`, `crates/gpui/src/window.rs:3218-3230`)
use exactly this technique at the framework level — prompts are painted after the whole root
tree, which is why `window.prompt()` (relevant to F-TAB-16 below) does not share this bug.

**The fix, once someone is assigned to build it, is small and mechanical**: wrap the return
value of `render_tab_context_menu` and `render_overflow_menu` in `gpui::deferred(...)` (import
already available transitively through `gpui::*`), pick a priority, and the same technique is
the correct fix for the already-known Files-panel-over-terminal-menu bug too — a second,
already-filed defect that shares this one's root cause and could close alongside it for free if
whoever owns that surface is read in.

Each affected row below is still written up on its own, because each has its own residual
gesture/evidence to re-check once the popover is visible (e.g. F-TAB-13's *first* conjunct
already passed independently; F-TAB-16 is **not** on this list — its defect is unrelated, see
its own section).

---

## `F-TAB-01` — FAILED — defective

**Needs: build**, then **exercise** the two conjuncts this unblocks.

The evidence (`sweep/A1-P109-verdicts.md:355-365`) is explicit that the live bug is in the
**Files panel's file tree**, not the tab strip itself: clicking an expanded folder's child row
(file or subfolder) collapses the parent instead of acting on the child, three isolated
attempts, no document tab ever opens. Reading `render_file_row`
(`rust/crates/tiller_ui/src/right_panel.rs:445-576`) in isolation, the click handler looks
correct — each row's `.id()` is keyed by its own full path
(`format!("file-{}", path.display())`, :467), and the left-click handler
(`.on_mouse_down(MouseButton::Left, ...)`, :500-512) reads `click_path` from that same closure,
so it should route to whichever row was actually hit. The rows are not rendered as nested
divs either (no parent/child DOM nesting that could explain event bubbling) — `file_rows()`
(:439-443) flattens the tree once per render via `flatten_files` (:1116-1126) into a single
`Vec<FileRow>`, and that flat vec is fed into a **virtualized** `uniform_list("right-panel-files",
rows.len(), ...)` (:738-754). `uniform_list` computes each visible row's on-screen position from
a single assumed item height; every row div does declare a fixed `.h(px(ROW_HEIGHT))` (:483,
`ROW_HEIGHT = 30.0` at :27), so nothing here obviously breaks uniformity — but given the
handler logic reads clean and the symptom is a clean off-by-one (clicked "the next row down"
consistently resolves as "the row above"), the `uniform_list` height/index mapping in that
mount call is the most likely place a real defect lives; it needs an instrumented repro (log
the resolved `click_path` on every left-click and compare it to the row actually visually
struck) rather than a source-only read to pin exactly, hence marking this "build" rather than a
one-line fix.

Also worth carrying forward, not as this row's own defect but as a blocker P104 recorded while
driving it: "opening a file does not add a new tab to the top strip... opening `main.rs` or
`README.md` instead replaces whatever the content pane already showed." Reading
`RightPanelEvent::OpenFile(path) => workspace.add_file_tab(path.clone(), cx)`
(`rust/crates/tiller/src/main.rs:2738`) and `add_file_tab` itself (:4331-4391), the code **does**
push a genuine new `OpenTab` and reassign `self.active_tab` to it — this contradicts the live
observation and the discrepancy is unresolved. Whoever re-drives F-TAB-01 after the click-routing
fix above should watch for whether the new tab appears in the strip at all; if it still doesn't,
the next place to look is `render_open_tabs`' filter by `tab.group_id == active_group`
(`tiller_command_machinery`/`rebuild_tab_machinery`) for a new tab landing in a group the strip
isn't currently rendering. That half of the puzzle is shared with several `F-EDIT` rows in a
different triage group and is flagged here only so it isn't lost.

- **files**: `rust/crates/tiller_ui/src/right_panel.rs` (`render_file_row` :445-576,
  `file_rows`/`flatten_files` :439-443/:1116-1126, the `uniform_list` mount :738-754); if the
  tab-creation half turns out to be real too, `rust/crates/tiller/src/main.rs` (`add_file_tab`
  :4331, `render_open_tabs` tab-group filtering).
- **size**: M

---

## `F-TAB-02` — FAILED — defective

**Needs: build.** Part of the shared cause above — this is the overflow-chevron half of it.
`render_overflow_menu` (`rust/crates/tiller/src/main.rs:6170-6249`) is mounted at
`:6344-6351`, inside the exact same `TAB_BAR_HEIGHT`-tall `.relative()` box the tab-context-menu
shares, with the identical "`.absolute()`, `.top(...)`, no paint-order escape" shape. The
"chevron rendered before real pixel overflow" half of this row's evidence is a **second, minor,
independent** defect worth fixing in the same pass but not blocking anything else: whatever
computes `has_overflow` (feeding the `if has_overflow` branch at :6315) is evidently firing
before the strip's actual measured width exceeds its container, which is a simple
measured-width-vs-declared-width comparison bug, not related to the paint-order issue.

- **files**: `rust/crates/tiller/src/main.rs` (`render_overflow_menu` :6170-6249, its mount site
  :6315-6352, and the `has_overflow` computation feeding :6315).
- **size**: S

---

## `F-TAB-08` — half-proven

**Needs: build.** The rendered half (the `Other agents…` / `No supported agent found on PATH`
fallback) is confirmed live and correct. The missing half — clicking that fallback should open
Agents settings — is missing because `render_chat_empty`
(`rust/crates/tiller_ui/src/tab_bar.rs:435-454`) is a plain `div()` with no `.on_click` at all,
unlike its sibling `render_chat_agent_item` (:406-433) which wires `.on_click(...)` to
`entity.update(cx, |this, cx| this.emit_chat_agent(id, cx))`. This is a small, contained gap,
not a paint-order problem — the fallback text is genuinely visible today, it's just inert.

The fix needs: a new callback field on `TabBar` alongside the existing `on_chat_agent` (see
`emit_chat_agent`, :280-287, for the pattern this codebase already uses — a boxed closure field,
not an `EventEmitter`), a click handler on `render_chat_empty` that closes the picker and
invokes it, and a builder call where `TabBar` is constructed in
`rust/crates/tiller/src/main.rs` (wherever `.on_chat_agent(...)` is currently set) that opens
`SettingsCategory::Agents` — `open_settings(Some(SettingsCategory::Agents), cx)`
(`main.rs:4672`) is the existing entry point, and `SettingsCategory::Agents`
(`rust/crates/tiller_ui/src/settings.rs:66`) already exists, so nothing needs adding on the
settings side.

- **files**: `rust/crates/tiller_ui/src/tab_bar.rs` (`render_chat_empty` :435-454, the
  `on_chat_agent`-style callback field(s) near :280), `rust/crates/tiller/src/main.rs` (wherever
  `TabBar` is constructed/wired, and `open_settings` :4672).
- **size**: S

---

## `F-TAB-11` — FAILED — absent

**Needs: build.** The clause wants a disabled Split item with an explanatory reason when the
pane is sole-tab or too small. The reason-computing model genuinely exists —
`split_disabled_reason` (`rust/crates/tiller/src/panes.rs:218-237`, gated `#[allow(dead_code)]`)
correctly models `SoleTabInGroup` and `PaneTooSmall{...}` — but it has no caller outside its own
tests (confirmed: `#[allow(dead_code)]` on the fn itself and on `SplitDisabledReason`, :168-170).
The actual rendered menu items come from a completely separate, unconditional list:
`TerminalContextItem` (`rust/crates/tiller_terminal/src/context_menu.rs:29-34`) has only
`label`/`action`/`route` fields — **no `disabled`/`reason` field exists on the type at all** —
and `items()` (:99-101) returns the same static 12-entry `ITEMS` array regardless of pane state,
tab count, or window size. There is no wiring path between the two; `split_disabled_reason`
cannot currently reach the menu even if something called it, because the item type has nowhere
to put the answer.

Building this needs: a `disabled: bool` + `reason: Option<&'static str>` (or an enum) added to
`TerminalContextItem`, an `items()` variant that takes the eligibility inputs
`split_disabled_reason` already needs (`PaneSize`, `tab_count`) and returns per-item
disabled/reason state for the four Split entries, the render/dispatch site
(`open_context_menu`/context-menu render in `tiller_terminal/src/lib.rs:970-1345`) updated to
show the reason text and refuse the click when disabled, and a caller in
`rust/crates/tiller/src/main.rs` that has the real pane size and tab count at hand to pass
through — this is a `tiller_terminal` ⇄ `tiller` boundary change, not confined to one crate.

Separately, live evidence found something odder worth a standalone re-check once the above
lands: on the sole-tab prompt line specifically, right-click opened a menu whose only entry was
a bare `Copy` — not the app's 12-item menu at all. That does not match anything in
`context_menu.rs`'s static `ITEMS`, so it may be a different element (an IME/native text-field
context menu) capturing the click before it reaches the terminal's own `.on_mouse_down(Right,
...)` handler (`tiller_terminal/src/lib.rs:1446`). Flagging it rather than folding it into this
row's fix, since it needs its own live repro to confirm.

- **files**: `rust/crates/tiller_terminal/src/context_menu.rs` (`TerminalContextItem` :29-34,
  `ITEMS`/`items()` :36-101), `rust/crates/tiller_terminal/src/lib.rs` (`open_context_menu` :970,
  context-menu render :1328-1345), `rust/crates/tiller/src/panes.rs` (`split_disabled_reason`
  :218-237, currently dead — needs its first real caller), `rust/crates/tiller/src/main.rs`
  (wherever the terminal context menu is invoked, to thread pane size/tab count down).
- **size**: M
