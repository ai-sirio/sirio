# P129 — overlay click routing: root cause, and whether P123 and F-CHG-18 are one bug

## Verdict up front

- **P123** (Clone/Create popover confined to the Sidebar's ~280px column) is a real, already-confirmed
  defect: an `.absolute().left(0).right(0).top(0).bottom(0)` overlay resolving against the Sidebar's
  own `.relative()` root instead of the window. Not re-litigated here — see
  `docs/linux-rewrite/tasks/P123-popover-click-routing.md`.
- **I found a second, previously undocumented instance of the same defect class**: the terminal
  pane's own right-click context menu (`tiller_terminal/src/lib.rs`). This one is **proven** — a
  deterministic unit test (below) shows the menu paints offset from the click point by exactly the
  pane's own window origin, and that a click at the position a correctly-anchored menu *should* have
  put the item lands on ordinary pane content and dismisses the menu without firing anything — the
  exact "click outside dismisses, click on the item does nothing" signature described for F-CHG-18.
- **F-CHG-18 as actually recorded names a different menu** — the tab-strip's own right-click context
  menu (`Move to New Pane` / `Rename` / `Close`, in `tiller_ui/src/tab_bar.rs` +
  `tiller/src/main.rs`), not the terminal pane's menu. For *that* specific menu I could **not**
  reproduce a defect: the existing automated suite already clicks through every item in it
  (sidebar visible, non-zero origin) and passes, and my own careful live single-invocation drive
  fired `Rename` correctly at pixel-verified coordinates. My live attempts to reproduce the reported
  failure on `Move to New Pane` themselves failed for harness reasons (documented below), not for
  reasons that implicate the menu's code.
- **So: P123 and F-CHG-18 are not proven to be the same bug.** They are two different code sites.
  P123's defect pattern *does* recur elsewhere (the terminal's own menu, newly found here), but I
  could not make it recur at the specific site F-CHG-18 names. If F-CHG-18 is real, its cause is
  still unidentified; my working hypothesis is now that it was itself a calibration/harness artifact
  of the kind P123's own author admits making "repeatedly."

## Method

Read the render sites (`tiller_terminal/src/lib.rs`'s context menu at the `Render` impl around
line 1454, `tiller_terminal/src/context_menu.rs`, `tiller_ui/src/tab_bar.rs`'s
`render_tab_context_menu` and its "+" new-tab menu, `tiller_ui/src/sidebar.rs`,
`tiller_ui/src/right_panel.rs`, and `tiller/src/main.rs`'s `render_tab_context_menu` call site),
then read GPUI's own `anchored()`/`deferred()`/Taffy-absolute-layout source at the pinned vendor rev
(`~/.cargo/git/checkouts/zed-a70e2ad075855582/c05e346/crates/gpui/src/elements/{anchored,deferred}.rs`
and `src/taffy.rs`) to establish ground truth for how GPUI resolves `.absolute().left()/.top()`
versus `anchored().position(...)`. Then built a deterministic reproduction with a same-crate
`#[gpui::test]` fixture that offsets the element under test from the window origin (the thing every
existing test in both crates fails to do — every one of them roots the widget under test directly at
the window, which is the one layout an origin-doubling bug cannot show up in), and cross-checked
with live drives under `Scripts/wayland-drive.sh` plus `ImageMagick`/pixel analysis to locate menu
items precisely rather than eyeballing screenshots. The scratch test code was written, run, and then
reverted (`git checkout --`) before finishing — this is a read-only investigation, nothing here was
committed as source; the test output quoted below is the evidence trail.

## GPUI ground truth

- `.absolute().left(x).top(y)` (plain Taffy `position: Position::Absolute`) resolves `x`/`y` as an
  offset from the nearest positioned ancestor's origin, and that ancestor's origin is *itself*
  already in window-absolute coordinates (`TaffyLayoutEngine`'s `absolute_outer_origin` is built by
  recursively adding each node's parent-relative offset — `src/taffy.rs:340-382`). So if you hand it
  a value that is *already* window-absolute, the ancestor's own offset gets added a second time.
- `anchored()` is a different mechanism entirely: `AnchoredPositionMode::Window` (its default) takes
  `anchor_position` and uses it as-is as the window coordinate, ignoring the parent's bounds for
  positioning purposes (`src/elements/anchored.rs:270-277` — `Bounds::from_anchor_and_size(anchor,
  anchor_position + offset, size)`), and translates the whole subtree at paint time via
  `window.with_element_offset(offset, ...)` (`anchored.rs:207-214`). This is why `anchored()` is
  ancestor-offset-safe and plain `.absolute()` is not.
- `deferred(x)` **only** changes paint/z-order (`window.defer_draw(child, element_offset, priority,
  None)`, `src/elements/deferred.rs:63-66`). It does nothing to positioning. Wrapping a
  mispositioned `.absolute()` element in `deferred(...)` fixes "painted underneath a sibling," not
  "painted at the wrong coordinate."
- The codebase already knows the correct idiom and uses it correctly in three places:
  `tiller_ui/src/tab_bar.rs:638-651` (the "+" new-tab menu — `deferred(anchored().anchor(TopLeft)
  .position(button_bounds.corner(BottomLeft))...)`, where `button_bounds` comes from a live
  `canvas()` measurement, not a hand-computed local offset), `tiller_ui/src/right_panel.rs:437`, and
  `tiller_ui/src/sidebar.rs:2058`.

## Finding A (new, proven): the terminal pane's own right-click context menu

`tiller_terminal/src/lib.rs`:

- `open_context_menu` (line ~1061-1070) stores the raw mouse-down position:
  `self.context_menu = Some(event.position);`
- The very next method down, `on_left_mouse_down` (line ~1072-1109), carries this comment about that
  *same* `event.position`:
  > `event.position` arrives in window coordinates; the paint path below... computes cell rects as
  > `bounds.origin + cell_width * column`, so hit-testing must undo that same offset or every pane
  > but one flush against the window's top-left corner maps clicks to the wrong cell (F-TERM-UI-02).
  That function correctly subtracts the pane's own `last_bounds.origin` before resolving a grid cell
  (`link_router::resolve_click_cell`).
- The context-menu render block (line ~1454-1502) does **not** do the equivalent subtraction. It
  builds the menu as:
  ```rust
  div().id("terminal-context-menu")....absolute().left(position.x).top(position.y)...
  ```
  a **plain** `.absolute()` div (no `anchored()`, no `deferred()`), nested as a `.child(...)` of the
  terminal pane's own `div().size_full().relative()...` container (line ~1564-1566). Per the GPUI
  ground truth above, `left(position.x)` is added to that container's own window origin — but
  `position.x` (== `event.position.x`) is *already* window-absolute. The pane's own origin gets
  counted twice.

**Proof** (temporary `#[gpui::test]` in `tiller_terminal/src/lib.rs`, run and reverted — not
committed): a fixture wraps `TerminalView` behind a 280px spacer div (standing in for the sidebar),
matching every real layout except a single fullscreen terminal:

```
P129 pane_bounds.origin = Point { x: 244.5px, y: 0px }
P129 click = Point { x: 284.5px, y: 40px }
P129 menu_bounds.origin = Point { x: 529px, y: 40px }
P129 delta (menu - click) = (244.5px, 0px)
```

The menu's real origin is `click + pane_origin`, not `click`. The delta is *exactly* the pane's own
window offset — this is the double-count, not a rounding artifact.

Then, the discriminating part — does a click land where a user (or a critic calibrating from a
screenshot) would expect it, versus where the code actually paints it:

```
P129 item6_bounds (Split Left) = Bounds { origin: Point { x: 536px, y: 221px }, size: 206px × 29px }
P129 naive_expected item6 center-ish = Point { x: 291.5px, y: 221px }
P129 clicking the NAIVE (un-doubled) expected position fired an event: false
P129 menu still open after naive click: false
P129 clicking item6's REAL painted center fired an event: true
P129 events so far: [TerminalContextEvent { ..., action: SplitLeft }]
```

The naive click (at the un-doubled, "correctly anchored" position) lands on ordinary pane content:
it fires nothing, and it dismisses the menu (via `on_mouse_down_out`, which is bounds-based and
correctly recognizes the click as outside the *actual* — shifted — menu rectangle). The real,
shifted position fires the action normally. **Paint and hit-test agree with each other; both simply
disagree with where the code implies the menu should be, and where a user's click naturally lands.**
This is not a stale-hit-region bug (P129's task brief's alternate hypothesis) — it is a pure
coordinate-frame bug, the same species as P123, at a different call site P123's report never looked
at (P123 covered `render_project_form`, and flagged `render_project_settings`/`render_context_menu`
in `sidebar.rs` as worth checking "at the same time" — it did not cover `tiller_terminal`'s own
menu, which lives in a different crate).

**Fix sketch (not applied — read-only task):** either subtract `terminal.last_bounds.origin` from
`position` before using it as `left()/top()` (mirroring `on_left_mouse_down`'s own fix for
F-TERM-UI-02), or better, replace the hand-rolled `.absolute()` block with
`deferred(anchored().position(position)...)`, matching the idiom already used correctly by
`tab_bar.rs`'s "+" menu, `right_panel.rs`, and `sidebar.rs`.

## Finding B: F-CHG-18's actual menu (tab-strip) does not reproduce

F-CHG-18's live evidence (`docs/linux-rewrite/wave-g/G5-terminal-verdicts.md`) is explicit that the
menu in question is **"`main.rs`'s `TabContextAction` menu, not the terminal-pane's own context
menu"** — reached by right-clicking a *tab*, not a terminal pane, with items `Rename`, `Close`,
`Move to New Pane` and others. Its render site is `tiller_ui/src/tab_bar.rs`'s free function
`render_tab_context_menu` (rows), called from `tiller/src/main.rs:7052`'s
`TillerWorkspace::render_tab_context_menu`, which builds:

```rust
div().id(...)....absolute().top(px(TAB_BAR_HEIGHT)).left(px(self.tab_context_menu_left()))...
deferred(menu)
```

This has the *same* smell as Finding A — plain `.absolute()`, no `anchored()`, `deferred()` used
only for z-order — but on inspection `tab_context_menu_left()` (`main.rs:7032-7044`) computes a
value by summing tab widths **starting from the tab strip's own left edge**, i.e. it is already a
*local* offset, not a window-absolute one reused verbatim (unlike Finding A's `event.position`). The
`.relative()` ancestor this `.absolute()` div resolves against (`main.rs:7626-7636`) is the same tab
bar row the offset was computed relative to. So the two frames are consistent by construction, not
by accident of a zero-origin test — I could not find a case where they diverge.

Three independent checks all came back clean:

1. **Existing automated tests already cover this exact menu with a non-zero origin.**
   `drawn_tab_context_menu_invokes_close_other_and_close_right_routes` and
   `drawn_tab_context_menu_moves_and_renames_the_selected_tab` (`tiller/src/main.rs`, ~line 10525+)
   build a `TillerWorkspace` with `sidebar_visible: true` (the default —
   `main.rs:3148`), right-click a tab, and `cx.simulate_click` directly on `cx.debug_bounds(...)` —
   ground truth, not a guess — for `tab-command-close-right`, `tab-command-close-others`,
   `tab-command-move-earlier`, and `tab-command-rename`. All pass at HEAD (`cargo test -p tiller
   drawn_tab_context_menu --lib`, not re-run destructively here but read in full and not touched).
2. **Live, single-invocation drive, `Rename`.** Under `Scripts/wayland-drive.sh`, with a real added
   project and a live PTY tab: right-click the tab at (400,51) → screenshot → pixel-measured the
   menu's actual text rows with ImageMagick + a small Python pass over `convert ... txt:-` output
   (not eyeballed) → clicked `Rename` at its measured center (450,132) → the tab visibly entered
   rename-edit mode (an editable field with the tab's title, a dirty-dot appeared) in the very next
   capture, in the same invocation. This is unambiguous positive evidence the menu is reachable and
   clickable at a non-zero tab-strip origin.
3. **Live, single-invocation drive, the pixel-measured position of `Move to New Pane` matches the
   code's own math.** A full pixel-row density scan of the same screenshot (11 text blocks detected,
   spacing consistent with the item list's separators) puts `Move to New Pane`'s label center at
   y=433 — 1px from where I'd independently clicked (432) before measuring. So the earlier live
   attempts that read as "no effect" were not a coordinate miss.

**My live attempts that looked like failures were harness artifacts, not menu defects**, and I want
to be explicit about this because it directly explains why a previous critic — driving the identical
gesture — might have recorded a real-looking 0/5:

- **`Scripts/wayland-drive.sh` restarts the app between invocations even with `TILLER_WL_KEEP=1`**
  (documented independently in `docs/linux-rewrite/wave-g/G4-chat-report.md`). Two of my own
  "the click did nothing" observations were `rightclick`+`shot` in one invocation followed by
  `click`+`shot` in a **separate** invocation — by the time the second one ran, the app had
  restarted and the menu was simply gone; the click landed on ordinary background. I did not notice
  this until I found a pile of stray `tiller`/`sway`/virtual-pointer processes left over from my own
  earlier invocations (`kill_ours`'s env-var match did not reliably reap the prior instance), which
  is what led me to check for this. Once I moved the entire gesture (project add → select worktree →
  open terminal → right-click → click item) into **one** invocation, `Rename` fired correctly on the
  first try.
- **A later single-invocation attempt at reproducing `Move to New Pane` specifically hit an
  unrelated state inconsistency**: the sidebar showed my scratch project/worktree selected and a
  `Terminal` tab present, but the live PTY's own breadcrumb read the *real* `tiller-linux` checkout
  and its `linux/gpui-waku` branch, not my scratch repo — i.e. the terminal that opened was not
  connected to the project the sidebar claimed was selected. This is itself possibly worth a
  separate row (a session/worktree wiring issue, not an overlay/click one), but it is not something
  I could pin down further in this budget, and it makes that specific drive unusable as evidence
  either way for or against F-CHG-18.

Given (1)+(2)+(3), I could not reproduce F-CHG-18 as a defect in the tab-strip context menu's
current code, and I'm not confident enough in the two failed live attempts (both explained by
harness artifacts unrelated to overlay positioning) to record it as confirmed either. If someone
re-drives it, the two things worth controlling for are staying inside a **single**
`wayland-drive.sh` invocation for the whole gesture, and independently confirming (e.g. via the
terminal breadcrumb, as I did) that the pane you're driving is actually rooted where the sidebar
claims before trusting a null result.

## Positive control

`Rename` firing correctly (Finding B, live check 2) and `Split Left` firing correctly at its real
painted position (Finding A's unit test) both double as the positive control the task asked for:
the exact same synthetic-click mechanism, in the exact same invocations, **did** actuate a normal
menu item at a correctly-computed coordinate. A null result elsewhere in this investigation is not
explained by the harness being unable to deliver clicks at all.

## Files read

- `rust/crates/tiller_terminal/src/lib.rs` (context menu render ~1454-1502, `open_context_menu`
  ~1061-1070, `on_left_mouse_down` ~1072-1109, `handle_context_action` ~1145-1184)
- `rust/crates/tiller_terminal/src/context_menu.rs` (menu item table)
- `rust/crates/tiller_terminal/src/link_router.rs` (`resolve_click_cell`, the already-fixed sibling
  of Finding A's bug)
- `rust/crates/tiller_ui/src/tab_bar.rs` (`render_tab_context_menu` ~107-176, the "+" menu's
  `deferred(anchored()...)` ~606-653, existing tests ~695-800)
- `rust/crates/tiller_ui/src/sidebar.rs` (`render_project_form` — P123's bug; the correct
  `deferred(...)` idiom at ~2058)
- `rust/crates/tiller_ui/src/right_panel.rs` (`anchored()` at ~437)
- `rust/crates/tiller/src/main.rs` (`render_tab_context_menu` ~7052-7086, `tab_context_menu_left`
  ~7032-7044, the tab-bar-row `.relative()` container ~7626-7636, `drawn_tab_context_menu_*` tests
  ~10525+)
- GPUI vendor source at the pinned rev (`Cargo.toml`'s `gpui = { git = "...", rev =
  "c05e34637b4f7f100a688bf6ac71cb70877fc8ad" }`,
  `~/.cargo/git/checkouts/zed-a70e2ad075855582/c05e346/crates/gpui/src/{taffy.rs,
  elements/anchored.rs, elements/deferred.rs}`)
- `docs/linux-rewrite/tasks/P123-popover-click-routing.md`,
  `docs/linux-rewrite/wave-g/G5-terminal-verdicts.md`,
  `docs/linux-rewrite/wave-g/G5-terminal-report.md`, `docs/linux-rewrite/wave-g/G4-chat-report.md`

## Rows this affects

- A **new** row should be opened against `tiller_terminal/src/lib.rs`'s own right-click context menu
  (Finding A) — it is not covered by P123, F-CHG-18, or any row I found in the wave-g/wave-h ledger.
  It reproduces the exact "click outside dismisses, click on the item does nothing" signature for
  any pane that is not flush against the window's top-left corner, i.e. essentially every real
  layout.
- `F-CHG-18` should **not** be closed as "same root cause as P123" — that framing is not supported.
  Whether it is a real, live-only, intermittent defect in the tab-strip menu remains open; my
  evidence leans toward "not currently reproducible," with the harness caveats above.
