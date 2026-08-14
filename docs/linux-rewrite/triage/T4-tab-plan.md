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

---

## `F-TAB-12` — FAILED — defective

**Needs: build.** Part of the shared cause above. The Move Earlier/Move Later logic itself is
already correct and unconditionally present in `tab_context_items()`
(`rust/crates/tiller/src/main.rs:5745-5883`, the Move Earlier/Move Later block at :5796-5819) —
right position-based enablement, right disabled reasons ("already the first/last tab"). This row
needs nothing beyond the popover-visibility fix; once the menu paints on top instead of behind
`centre-surface`, re-drive the exact P104 gesture (right-click the last tab, then the first tab,
compare Move Earlier's presentation) to close it out.

- **files**: `rust/crates/tiller/src/main.rs` (`render_tab_context_menu` :5900-5923 and its mount
  site :6451-6453 — same fix as F-TAB-02).
- **size**: S (once the shared popover fix lands)

---

## `F-TAB-13` — FAILED — defective

**Needs: build**, for the second conjunct only. This row has two independent conjuncts and
P104 already scored the first live: right-click → Split Right inside the terminal pane produced
a real, correct vertical split (`p104-g1-tab13-split-menu.png`/`-split-result.png`) — that half
holds today and needs no further work. The second conjunct — right-click a *tab* and read every
"Move to …" entry — hit the same "no menu appeared" wall as every other tab-strip right-click in
this group. The item logic for that half is already correct too: `tab_context_items()`
(:5838-5866) builds "Move to This Pane" (always disabled — a tab can't move to its own pane) and
one "Move to Pane N" entry per other group, each individually enabled/disabled by whether other
groups exist. Same fix, same file, as F-TAB-12.

- **files**: `rust/crates/tiller/src/main.rs` (`render_tab_context_menu` :5900-5923, mount site
  :6451-6453).
- **size**: S (once the shared popover fix lands)

---

## `F-TAB-14` — FAILED — defective

**Needs: build.** Part of the shared cause above. Rename logic already exists and is wired: the
menu's `Rename` item routes to `begin_tab_rename` and there's a drawn `tab-rename-field` with
its own commit/key handling (`rust/crates/tiller/src/main.rs`, the `renaming`/`rename_draft`
branch in `render_open_tab` at :5578-5604, and the Rename entry in `tab_context_items()` at
:5764). The row's own two conjuncts (menu Rename → type → Enter, then Escape) are both gated on
the same invisible popover; nothing else needs to change.

- **files**: `rust/crates/tiller/src/main.rs` (`render_tab_context_menu` :5900-5923, mount site
  :6451-6453; rename field itself at :5578-5604 needs no change).
- **size**: S (once the shared popover fix lands)

---

## `F-TAB-15` — FAILED — defective

**Needs: build.** Part of the shared cause above. The close-control half of this row (click the
✕) already PASSED independently and is unaffected — this row's remaining gap is specifically
the *context-menu* Close path, which routes through the same invisible tab-strip popover
(`TabContextAction::Close` in `tab_context_items()`, :5766, and `handle_tab_context_action`).
Nothing to add beyond the shared fix.

- **files**: `rust/crates/tiller/src/main.rs` (`render_tab_context_menu` :5900-5923, mount site
  :6451-6453).
- **size**: S (once the shared popover fix lands)

---

## `F-TAB-16` — FAILED — defective

**Needs: both.** This row is **not** part of the tab-strip-popover shared cause — its two
conjuncts fail for a different reason, and `window.prompt()` itself is not suspect: GPUI paints
its prompt dialogs at true window-root priority (`Window::draw`,
`crates/gpui/src/window.rs:3218-3230`, painted after the whole root tree, the same mechanism
`deferred()` uses), so a dirty-close confirm, if triggered, would be visible regardless of the
F-TAB-02-family bug.

The close-button code itself looks right: the strip's own `×` control calls
`request_close_tab_by_id` (`rust/crates/tiller/src/main.rs:5709-5735`), which checks
`tab_is_dirty` (:5688-5707) and only proceeds straight to `close_tab` when clean, otherwise
raising `window.prompt(PromptLevel::Warning, "Close dirty tab?", ...)` (:5720-5726). The gap is
almost certainly upstream of this function: P104's setup note records that a file opened from
the Files panel never visibly became its own tab — it "replaced whatever the content pane
already showed" (the live Chat view, in the recorded trial) — so the `×` click P104 drove
actually closed the **Chat** tab, and `tab_is_dirty`'s `TabContent::Chat` branch
(chat.`is_streaming()`) has no idea a file was dirty underneath it. Source reading contradicts
part of that story though: `RightPanelEvent::OpenFile(path) => workspace.add_file_tab(path.clone(),
cx)` (`main.rs:2738`) and `add_file_tab` (:4331-4391) do push a genuine new `OpenTab` with its
own `TabContent::File`, which `tab_is_dirty`'s `TabContent::File` branch (:5696) *does* check via
`view.read(cx).is_dirty()`. Both halves of the app read correctly in isolation; the live
behaviour says otherwise. This needs an instrumented re-drive (log `self.tabs.len()` and each
tab's title immediately after `add_file_tab` runs) before deciding whether the defect is in tab
creation, in `FileView::is_dirty()`, or somewhere in between — hence "build", not a source-only
fix.

The two-dirty-tabs precondition (second conjunct) is blocked on the same open question: if a
second file genuinely can't be open concurrently with a first, no amount of dialog-wiring work
fixes this row until that's resolved. This overlaps `F-EDIT` rows in a different triage group —
flagging the dependency rather than claiming it here.

- **files**: `rust/crates/tiller/src/main.rs` (`request_close_tab_by_id` :5709-5735,
  `tab_is_dirty` :5688-5707, `add_file_tab` :4331-4391, `RightPanelEvent::OpenFile` handler
  :2738), `rust/crates/tiller_ui/src/right_panel.rs` (`open_file` :335-336, the double-click
  gate at :500-512), `rust/crates/tiller_ui/src/editor.rs` (`FileView::is_dirty`).
- **size**: M

---

## `F-TAB-17` — FAILED — defective

**Needs: build.** Part of the shared cause above. Close Others and Close Tabs to the Right are
both already correctly modelled with per-position enablement in `tab_context_items()`
(`rust/crates/tiller/src/main.rs:5767-5794`: Close Others disabled with "no other tab is
available" when `group.tabs.len() <= 1`, Close Tabs to the Right disabled with "already the last
tab" when there's nothing after `position`). Same fix, same file.

- **files**: `rust/crates/tiller/src/main.rs` (`render_tab_context_menu` :5900-5923, mount site
  :6451-6453).
- **size**: S (once the shared popover fix lands)

---

## `F-TAB-21` — FAILED — defective

**Needs: build.** Part of the shared cause above, and the row that names it most directly — its
own evidence contrasts a verified-working terminal-body right-click against a tab-strip
right-click that never draws anything, which is the exact discriminator the shared-cause section
above is built on. `open_tab_menu` (`rust/crates/tiller/src/main.rs:5738-5743`) correctly flips
`tab_menu_open`/`tab_menu_tab` on every right-click; the state is right, the paint order is not.

- **files**: `rust/crates/tiller/src/main.rs` (`render_tab_context_menu` :5900-5923, mount site
  :6451-6453).
- **size**: S (once the shared popover fix lands)

---

## `F-TAB-23` — FAILED — defective

**Needs: both.** This is fully diagnosed, and the bug is precise and narrow — it is **not** the
tab-strip popover issue. Tracing `direction=left` through the control-socket route P106's fable
actually drove (`pane.split`):

1. The socket handler collapses the distinction it's supposed to preserve —
   `rust/crates/tiller/src/main.rs:1292-1310`: `"right" | "left" => SplitDirection::Horizontal`.
   Both strings produce the *same* value; "left" and "right" are indistinguishable from here on.
2. `ControlAction::SplitPane { direction, reply }` (:302, handled :2446-2449) carries only that
   axis — there is no placement field to carry the lost information even if step 1 were fixed.
3. The handler calls `workspace.split_focused_terminal(direction, None, cx)` (:4956-4970), which
   calls `self.split_terminal_at(...)` (:4972) — **not** `split_terminal_at_with_placement`.
4. `split_terminal_at` (:4972-4988) hardcodes `SplitPlacement::After` for every caller,
   regardless of direction. `After` is "right"/"down" in `split_focused_inner`'s placement match
   (`rust/crates/tiller/src/panes.rs:344-365`). So a horizontal split is *always* placed after —
   i.e. always on the right — no matter what direction the socket was asked for. This is the bug,
   completely: not a rendering issue, not a geometry issue, a placement-argument that never gets
   threaded through this one call path.

The **terminal-body context menu** route (right-click → Split Left/Right/Above/Down) is a
*different*, already-correct path: `delegated_terminal_context_action`
(`rust/crates/tiller/src/main.rs:2186-2215`) maps `SplitLeft → placement: Before` and
`SplitRight → placement: After` correctly, and its handler
(:2811-2815 — search for the `TerminalContextCommand::Split` match arm) calls
`split_terminal_at_with_placement` directly, which respects `SplitPlacement::Before` correctly
(`panes.rs:344-354`). Reading the code, that route should already place a left split on the
left. Nobody has live-driven it yet ("Menu-route exercise remains owed" in the manifest's own
evidence) — that's the "exercise" half of this row, to be done *after* the socket fix, both to
confirm the menu route independently works and to confirm the socket fix didn't regress it.

- **files**: `rust/crates/tiller/src/main.rs` — the `"pane.split"` socket handler (:1292-1310,
  needs to preserve "left" vs "right"/"up" vs "down", not just the axis), `ControlAction::SplitPane`
  (:302, needs a placement field), its handler (:2446-2449), `split_focused_terminal` (:4956-4970,
  needs to accept/forward placement), `split_terminal_at` (:4972-4988, currently hardcodes
  `SplitPlacement::After`). No change needed in `rust/crates/tiller/src/panes.rs` — its
  `SplitPlacement`/`split_focused_with_placement` machinery is already correct.
- **size**: S (precisely diagnosed; touches one call chain in one file)

*Cross-reference*: `F-TERM-SPLIT-01` (a different triage group) names the same left-placement
symptom plus an unrelated `TerminalPaneCache` lifecycle gap — the two rows share this fix but are
not the same defect; don't let one row's completion mark the other's cache-lifecycle half done.

---

## `F-TAB-25` — needs: reclassify

**The current verdict ("FAILED — absent... still no attach-to-terminal code... pass 14") is
stale and wrong on the evidence in the tree today.** The feature is built: `Attach to Current
Terminal` is a real, wired entry in the tab context menu
(`rust/crates/tiller/src/main.rs:5823-5836`), backed by `can_attach_tab_to_current_terminal`
(:5988-5999, mirroring the Swift `workspaceCanAdoptPane` contract per its own doc comment) and
`attach_tab_to_current_terminal` (:6005+, moves the terminal pane into a horizontal split beside
the active terminal without restarting the PTY), dispatched from
`TabContextAction::AttachToCurrentTerminal` (:5977-5981). Two drawn tests exist and are named in
`docs/linux-rewrite/P110-report.md` (`§F-TAB-25`):
`drawn_terminal_menu_attaches_an_eligible_terminal_to_the_current_tab` and
`drawn_terminal_attach_command_is_disabled_for_the_current_terminal` — both confirmed present at
`rust/crates/tiller/src/main.rs:11973` and `:12012`.

Why the ledger still says "absent": `git log -S` on the function name shows it landed in commit
`7289c84` (2026-08-14 17:32, "feat: add no-terminals worktree state" — P110/`codex12`'s session),
and `P110-report.md`'s own commits (17:35-17:39) document exactly this build. The ledger's
`F-TAB-25` row, by contrast, has been byte-for-byte unchanged since "pass 14" across every ledger
snapshot since, **including the 22:39 sweep-verdict commit that produced the copy this triage
manifest was generated from** (`git log -p` on `INVENTORY-LEDGER.md` shows the identical line at
every revision after pass 14). Nobody re-checked this row after P110 built the feature; the
"still absent" text is simply stale, carried forward unread.

That said, this is not a clean PASSED either: P110's own report says the gesture "cannot show the
context menu because this lane has no input devices" and explicitly lists it as an **owed
gesture** — right-click an eligible source tab, choose Attach to Current Terminal, confirm the
transition; then confirm it's disabled on the current terminal itself. And because this feature
lives inside the *same* tab-strip popover as F-TAB-12/13/14/15/17/21, proving it live is blocked
on the identical shared-cause fix above — reclassifying this row without also fixing the popover
still leaves it unprovable by real click.

- **files**: none for a code fix — this is a verdict-only correction. Once reclassified, the
  live-exercise dependency is `rust/crates/tiller/src/main.rs` (`render_tab_context_menu`
  :5900-5923, mount site :6451-6453 — the shared cause above).
- **size**: — (reclassify only; the residual gesture is S once the popover is visible)

---

## `F-TAB-28` — FAILED — defective

**Needs: both.** The binding is genuinely present, twice: `panes::bind_keys(cx)`
(`rust/crates/tiller/src/panes.rs:113-134`, called from `TillerWorkspace::new` at
`rust/crates/tiller/src/main.rs:2302`) registers `KeyBinding::new("ctrl-w", CloseTab, None)` at
`panes.rs:133`, and the very next lines in `main.rs` register it *again*:
`cx.bind_keys([KeyBinding::new("cmd-w", CloseTab, None), KeyBinding::new("ctrl-w", CloseTab,
None)])` (`main.rs:2303-2306`). Both point at the same `CloseTab` action with no context
predicate (`None`), so this redundancy is unlikely to be the actual bug by itself, but it is a
real, confirmed duplication worth resolving in the same pass — two hands have touched this chord
and left it registered twice. `handle_close_tab` (`main.rs:6724`) and `request_close_tab_by_id`
(`main.rs:5709-5735`, the same dirty-aware prompt path used by the strip's `×` control) both look
correct on read.

Live evidence shows **zero** effect on both a clean tab (Chat, three isolated presses, once with
explicit refocus) and a dirty tab (Terminal, no confirm dialog, no close) — not a partial or
flaky result, a total one on both paths. The leading candidate is key-event consumption by
whatever currently holds focus before the bound action ever fires: `ctrl-w` is also the
long-standing readline/terminal chord for "delete previous word," and the terminal view's own raw
key handler (`TerminalView::on_key_down`, `rust/crates/tiller_terminal/src/lib.rs:1081-1095`)
forwards *any* unmatched key straight to the PTY via `key_bytes(event)` with no allowlist — if a
focused terminal's raw handler or the chat composer's own key handler
(`Chat::on_composer_key`, `rust/crates/tiller_ui/src/chat.rs:2630+`) intercepts the chord before
GPUI's action-dispatch phase resolves it, the bound `CloseTab` action would never fire. Chat's own
`on_composer_key` comment (:2663-2665) documents the *intended* precedence — "gpui stops
propagation once an action listener fires" — which suggests this shouldn't happen, so the
contradiction between that comment and the live "zero effect" result needs an instrumented
repro (log which handler receives the ctrl-w keystroke first) to resolve, not a source-only read.

- **files**: `rust/crates/tiller/src/panes.rs` (`bind_keys` :113-134, duplicate `ctrl-w`
  registration at :133), `rust/crates/tiller/src/main.rs` (`KeyBinding` registration
  :2303-2306, `handle_close_tab` :6724, `request_close_tab_by_id` :5709-5735),
  `rust/crates/tiller_terminal/src/lib.rs` (`on_key_down` :1081-1095, candidate consumer),
  `rust/crates/tiller_ui/src/chat.rs` (`on_composer_key` :2630+, candidate consumer).
- **size**: M
