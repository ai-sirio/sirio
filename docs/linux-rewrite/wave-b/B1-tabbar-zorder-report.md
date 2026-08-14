# B1-tabbar-zorder — build report

Slice: `docs/linux-rewrite/wave-b/B1-tabbar-zorder.md`
File owned this wave: `rust/crates/tiller/src/main.rs`
Branch: `linux/gpui-waku`, worktree `/home/enzopalmisano/Scrivania/Progetti/tiller-linux`
Commit: `2c1ee10` — `fix(tiller): defer tab-bar popover paint above centre-surface`

## Shared-cause confirmation

Confirmed as **one root cause**, exactly as triage described — with one real second
component folded into the same row (F-TAB-02) rather than a second unrelated cause.

**Cause 1 (drives 8/8 rows): tab-bar popovers paint underneath `#centre-surface`.**

`columns()` builds the tab strip and the centre surface as sibling `div`s inside one
`flex_col` container:

```rust
columns.child(
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
            div().id("centre-surface") /* ... */ .child(centre_surface),
        ),
);
```

Both popovers (`render_overflow_menu`, called from inside `render_open_tabs`; and the
`render_tab_context_menu` method) are `.absolute()` divs whose own bounds extend *below*
the `TAB_BAR_HEIGHT`-tall strip — `render_tab_context_menu` explicitly positions itself at
`.top(px(TAB_BAR_HEIGHT))`, i.e. squarely over `#centre-surface`. GPUI has no implicit
z-index: elements paint in tree order, and `#centre-surface` is a *later* sibling of the
tab-bar row that hosts both popovers. So every frame, GPUI painted the popover first (as a
descendant of the earlier sibling), then painted `#centre-surface` on top of it. The
popover was genuinely in the render tree — `cx.debug_bounds("tab-overflow-menu")` and
`cx.debug_bounds("workspace-tab-menu-N")` both found it, which is exactly why the existing
GPUI-harness tests (10 of them, all still green) never caught this — `debug_bounds` just
records the rectangle a `debug_selector` painted into; it does not model pixel occlusion
from a later sibling's paint pass. Only a real screen (or a live click, since GPUI's
`hit_test` walks hitboxes topmost-first) shows the popover as gone. This matches P104's
evidence verbatim: the button/right-click state changed (`overflow_menu_open` /
`tab_menu_open` flipped correctly — confirmed by reading `open_tab_menu`,
`handle_open_tab_menu`, and the toggle in the overflow button's `on_click`), but nothing
appeared on screen.

The codebase already has the correct pattern for this, used by the tab bar's own new-tab
menu (`tiller_ui::tab_bar`, not owned by this slice, read for reference only):

```rust
new_tab_button = new_tab_button.child(
    deferred(anchored().anchor(Anchor::TopLeft).position(/* ... */).child(menu)).priority(1),
);
```

`gpui::deferred(...)` keeps an element's layout in the normal tree (so its `.absolute()`
positioning is untouched) but defers its *paint* until after every ancestor has painted —
see `crates/gpui/src/elements/deferred.rs` in the vendored Zed checkout. **Fix:** wrap the
return value of `render_overflow_menu` and of the `render_tab_context_menu` method in
`deferred(...)`. Both now paint after `#centre-surface`, landing on top as intended.

**Cause 2 (F-TAB-02 only, not a second shared cause — it's the row's other named
defect): the overflow chevron could appear before the strip actually overflowed.**

`render_open_tabs` decided `has_overflow` by asking `tab_machinery::visible_tab_count`
whether the tabs fit in `available_width - overflow_width` — i.e. it always reserved room
for the chevron's own width before knowing whether a chevron would even be needed. That
means a strip whose tabs exactly (or almost) filled the *real* available width, with zero
room left over for a hypothetical chevron, still flipped into overflow mode and hid a
trailing tab behind a chevron nothing actually required. Confirmed algebraically and with
a new regression test (below): three 132px terminal tabs in a window sized so the strip's
true available width is exactly 396px used to trigger overflow (hiding the third tab)
purely because `396 + 26 > 396`, even though `396 <= 396` — the tabs fit exactly. Fixed by
deciding overflow against the full `available_width` first, and only computing the
chevron-reserving budget once that first check says the strip truly doesn't fit. This is
local to `render_open_tabs` in `main.rs`; `tab_machinery::visible_tab_count` itself
(`rust/crates/tiller/src/tab_machinery.rs`, not owned by this slice) was not touched or
found to need touching — its algorithm is correct for the budget it's given, the bug was
in what budget `render_open_tabs` handed it for the first, is-there-overflow-at-all,
question.

No other file in `TillerAgents`/`TillerUI` popovers were touched or needed touching for
these 8 rows — every row's evidence traces to one of the two `main.rs`-local popovers
above.

## Changes made (all in `rust/crates/tiller/src/main.rs`)

1. Added `deferred` to the `gpui::{...}` import.
2. `render_overflow_menu`: returns `deferred(menu)` instead of `menu`.
3. `render_tab_context_menu` (the `&self` method, ~line 5900): returns `deferred(menu)`
   instead of the bare `div()` chain.
4. `render_open_tabs`: `has_overflow` is now decided against the full `available_width`
   before the chevron-reserving budget is applied (see Cause 2 above).
5. New regression test:
   `drawn_tab_strip_does_not_reserve_overflow_room_when_all_tabs_actually_fit` — proves
   Cause 2 is fixed (3 tabs at an exact-fit window width no longer produce a
   `tab-overflow-button`).

## Tests

```
cargo test -p tiller --bin tiller -- tab_menu tab_context overflow attach_to_current_terminal has_overflow visible
...
running 10 tests
test tab_machinery::tests::overflow_is_reported_only_when_the_strip_exceeds_available_width ... ok
test tab_machinery::tests::visible_tab_count_reserves_the_overflow_control_for_hidden_tabs ... ok
test tests::drawn_tab_context_open_file_uses_the_picker_and_adds_an_editor_tab ... ok
test tests::drawn_tab_context_menu_moves_a_tab_to_another_pane_group ... ok
test tests::drawn_all_tabs_overflow_lists_every_hidden_tab_and_marks_the_active_one ... ok
test tests::drawn_tab_context_menu_moves_and_renames_the_selected_tab ... ok
test tests::drawn_tab_context_menu_invokes_close_other_and_close_right_routes ... ok
test tests::drawn_tab_context_resume_chat_reopens_the_retained_session ... ok
(+ 2 unrelated matches)
test result: ok. 10 passed; 0 failed

cargo test -p tiller --bin tiller -- drawn_terminal_menu_attaches drawn_terminal_attach_command
running 2 tests
test tests::drawn_terminal_attach_command_is_disabled_for_the_current_terminal ... ok
test tests::drawn_terminal_menu_attaches_an_eligible_terminal_to_the_current_tab ... ok
test result: ok. 2 passed; 0 failed

cargo test -p tiller --bin tiller -- drawn_tab_strip_does_not_reserve_overflow_room
running 1 test
test tests::drawn_tab_strip_does_not_reserve_overflow_room_when_all_tabs_actually_fit ... ok
test result: ok. 1 passed; 0 failed

cargo test -p tiller --bin tiller   (full crate suite, run once before a sibling agent's
concurrent edit to tiller_acp/tiller_ui transiently broke the workspace build — see caveat)
test result: ok. 140 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 58.45s
```

**Caveat on `cargo check` at HEAD:** while finishing this slice, a sibling agent's
in-progress edits to `tiller_acp/src/lib.rs` and several `tiller_ui` files
(`chat.rs`, `editor.rs`, `file_view.rs`, `sidebar.rs`) — none of them owned by this
slice — left the shared worktree in a transiently non-compiling state (an added
`AcpEvent::AuthRequired` variant not yet matched everywhere, then a borrow-checker error
in unrelated editor code, changing between checks). This is expected fleet churn, not a
defect in this row's work: `main.rs` itself, and the full `tiller` binary + all 140 tests,
compiled and passed cleanly at commit `2c1ee10` before that concurrent churn started, and
none of the sibling edits touch anything this fix depends on. `compiles: true` below
reflects that last clean measurement at this slice's own commit; re-run `cargo check -p
tiller` once the other slices land to reconfirm against a settled tree.

**Important limitation of the GPUI test harness for this bug class:** as the task brief
warned, `debug_bounds`-based tests cannot detect paint-order occlusion — they record
whatever a `debug_selector` painted, whether or not something later painted over it. All 10
tab-menu/overflow tests above were green *before* this fix too. They remain valuable as
regression coverage for the underlying state machine (menu open/closed, item
enable/disable, action routing) but none of them — before or after — is evidence that the
popover is visible on screen. That evidence can only come from a live drive or a real
screenshot, which is why every row below still needs a live gesture, not a green test, to
close.

## Rows

### F-TAB-02 — overflow chevron / All Tabs list
- **Outcome:** built
- **What changed:** `deferred(...)` wrap on `render_overflow_menu`'s return; `has_overflow`
  measurement fixed to not pre-reserve chevron width (see Cause 2).
- **Tests:** `drawn_all_tabs_overflow_lists_every_hidden_tab_and_marks_the_active_one`
  (pre-existing, still passes) + new
  `drawn_tab_strip_does_not_reserve_overflow_room_when_all_tabs_actually_fit`.
- **howToExercise:** Open a worktree and open enough terminal/chat tabs (or shrink the
  window) that the tab strip cannot fit them all — a chevron (⌄) appears at the right end
  of the tab strip. Click it twice (open, then open again after closing, to match the
  P104 trial). The "All Tabs" list must now visibly appear as a floating panel drawn *over*
  the terminal/chat content below the tab strip, listing every tab (including the ones
  hidden from the strip) with a ✓ next to the currently active one. Before this fix the
  panel never appeared at all on a live click. Also verify the chevron itself does **not**
  appear when the open tabs merely come close to filling the strip but still fit with room
  to spare — that was the false-positive half of this row.

### F-TAB-12 — tab context menu: Move Earlier / Move Later
- **Outcome:** built
- **What changed:** `deferred(...)` wrap on the `render_tab_context_menu` method's return
  only; item enablement logic was already correct (untouched).
- **Tests:** `drawn_tab_context_menu_moves_and_renames_the_selected_tab` (exercises the
  menu's move/rename actions via `debug_bounds`, pre-existing, still passes).
- **howToExercise:** With 3+ tabs open in one pane group, right-click a *middle* tab (not
  the first or last) in the tab strip. A context menu must now visibly appear over the
  centre surface, with "Move Earlier" and "Move Later" both enabled. Click "Move Later" and
  confirm the tab's position in the strip actually shifts one slot to the right. Also
  right-click the *first* tab and confirm "Move Earlier" is disabled ("already the first
  tab"), and the *last* tab for "Move Later" disabled likewise.

### F-TAB-13 — tab context menu: Move to Pane
- **Outcome:** built
- **What changed:** same `deferred(...)` wrap; the Split-Right-in-terminal conjunct was
  already reported PASSED live and is untouched by this slice (different surface, not a
  tab-bar popover).
- **Tests:** `drawn_tab_context_menu_moves_a_tab_to_another_pane_group` (pre-existing,
  still passes).
- **howToExercise:** Split the terminal pane so two pane groups exist (Split Right — this
  half already worked). Right-click a tab belonging to one group; the context menu must
  show a "Move to Pane `<id>`" entry for the *other* group and "Move to This Pane" disabled
  for its own group ("no other tab is available" — actually it's disabled because the
  target *is* the current pane; read the label literally). Click "Move to Pane `<id>`" and
  confirm the tab now renders in the other pane group's strip.

### F-TAB-14 — tab context menu: Rename
- **Outcome:** built
- **What changed:** same `deferred(...)` wrap; `begin_tab_rename` and the rename field
  wiring were already correct and untouched.
- **Tests:** `drawn_tab_context_menu_moves_and_renames_the_selected_tab` (pre-existing,
  still passes; exercises Rename via the menu).
- **howToExercise:** Right-click any tab; the context menu's "Rename" entry must now be
  visible (it renders first, right under "Open File"). Click it; the tab's title must turn
  into an inline editable field (`#tab-rename-field`) with the current title pre-filled;
  type a new name and press Enter; confirm the tab strip shows the new title.
  **Caveat for the verifier:** the manifest's evidence also names "a separately required
  double-click path" for entering rename. I read the tab strip's own `on_click`/rename
  wiring in `main.rs` end-to-end and found no double-click-to-rename gesture anywhere in
  this file (nor in `tiller_ui::tab_bar`) — rename is reachable only via this context-menu
  item. If a double-click gesture is genuinely expected, that is new functionality outside
  this fix's approach text (which only names the menu's Rename entry) and outside
  `main.rs`'s current design; flagging it rather than guessing at unreviewed scope.

### F-TAB-15 — tab context menu: Close
- **Outcome:** built
- **What changed:** same `deferred(...)` wrap; the strip's own close-control (X) half was
  already reported PASSED live and is untouched.
- **Tests:** none of the pre-existing drawn tests exercise the bare "Close" item by name
  (they cover Close Others/Close Tabs to the Right); I did not add one since it is a
  one-line action identical in shape to the already-tested siblings — flagging this as a
  coverage gap rather than claiming a test that doesn't exist.
- **howToExercise:** Right-click a tab; confirm the context menu is visible; click "Close".
  The tab must close and, if the tab held an active terminal/chat/file, whatever
  dirty-close confirmation the app normally shows must still trigger correctly (this row's
  logic was never in question, only the menu's visibility).

### F-TAB-17 — tab context menu: Close Others / Close Tabs to the Right
- **Outcome:** built
- **What changed:** same `deferred(...)` wrap; item logic already correct and untouched.
- **Tests:** `drawn_tab_context_menu_invokes_close_other_and_close_right_routes`
  (pre-existing, still passes).
- **howToExercise:** Open four terminal tabs in one group. Right-click the second tab; the
  menu must be visible with "Close Others" and "Close Tabs to the Right" both enabled.
  Click "Close Tabs to the Right" and confirm tabs 3 and 4 close, leaving 1 and 2. Reopen,
  right-click tab 1, click "Close Others", confirm only tab 1 remains.

### F-TAB-21 — tab-strip right-click reachability
- **Outcome:** built
- **What changed:** same `deferred(...)` wrap; `open_tab_menu`'s state toggle was already
  correct and untouched — this row's own evidence (working terminal-body right-click vs.
  dead tab-strip right-click) is the cleanest single demonstration of the shared cause,
  since the terminal's own context menu is a different, already-`deferred`-correct code
  path unaffected by this bug.
- **Tests:** the general context-menu tests above all drive the tab strip via
  `right_click_tab` and pass, but per the harness limitation noted above that only proves
  the state machine, not visibility.
- **howToExercise:** Right-click directly on the tab strip (on a tab, not in the empty area
  after the last tab). A menu must now visibly appear, with move/close/rename entries as
  described in the rows above. Separately confirm right-clicking inside the terminal body
  still opens its own (unrelated, already-working) context menu — the two must not be
  conflated, and both should now work.

### F-TAB-25 — Attach to Current Terminal (reclassified NOT EXERCISED → build)
- **Outcome:** built
- **What changed:** same `deferred(...)` wrap unblocks the live right-click path onto the
  menu; `can_attach_tab_to_current_terminal` / `attach_tab_to_current_terminal` and the
  "Attach to Current Terminal" menu item (`main.rs:5823-5836` in this file's current
  numbering) were already implemented and are untouched — confirming the manifest's own
  triage note that the prior ledger text describing "no attach-to-terminal code" was stale.
- **Tests:** `drawn_terminal_menu_attaches_an_eligible_terminal_to_the_current_tab` and
  `drawn_terminal_attach_command_is_disabled_for_the_current_terminal`, both pre-existing,
  both still pass.
- **howToExercise:** Open two terminal tabs. Right-click the *non-current* terminal tab;
  the menu must be visible with "Attach to Current Terminal" enabled. Click it and confirm
  that terminal's pane attaches into the currently active tab (panes merge into one tab).
  Then right-click the tab that *is* the current terminal itself and confirm the item is
  present but disabled, with reason text "select another terminal tab".

## Files touched
- `rust/crates/tiller/src/main.rs` (owned; all changes above)

## Files wanted but not owned
- None. Every row's fix and every row's evidence resolved entirely within
  `rust/crates/tiller/src/main.rs`.
