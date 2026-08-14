# Critic verdicts — W09-term (F-TERM)

Adjudicated by a critic that neither drove nor built this slice. Driver return:
`docs/linux-rewrite/wave-a/W09-term-evidence.md`, captures under
`reference/linux-progress/wavea-W09-term/`. HEAD under test: `4073297`.

## F-TERM-04 (ledger line 322) — verdict: `half-proven` (unchanged)

Driver did a source-only re-check, no new drive, no new capture (own `discriminating: false`).
Independently re-verified: `open_context_menu` is wired only to
`.on_mouse_down(MouseButton::Right, ...)` at `rust/crates/tiller_terminal/src/lib.rs:1446` and
`:1505`, no keyboard alternative anywhere in the file; `Scripts/wayland-drive.sh`'s
`pointer_command` (`move`/`click`, lines ~256-269) drives only a plain left button over the
virtual-pointer FIFO, no right-click or modifier parameter exists; the full `ControlAction`
dispatch table in `rust/crates/tiller/src/main.rs` has no clipboard/copy-paste variant. All
claims check out. This is a re-confirmation of a known limit of the *Wayland* lane only — it
does not touch the existing basis for `half-proven`: `reference/linux-progress/p17-rclick-term.png`
(verified present and opened — it shows the terminal context menu genuinely live on-screen with
Copy/Paste as the first two items, via the X11/`DISPLAY=:1` lane's `rclick`). A route to open the
menu demonstrably exists on this platform; this driver's lane just isn't it. Owed half (select,
copy, paste, verify clipboard content) is unchanged. Verdict carries forward unchanged.

## F-TERM-06 (ledger line 324) — verdict: `half-proven` (unchanged)

Same source-only re-check and same confirmed blocker as F-TERM-04 (own `discriminating: false`).
Independently confirmed no `ControlAction` variant reaches `CopyPaneId`/`CopyTerminalId`
(`rust/crates/tiller_terminal/src/context_menu.rs:54-66`). No new drive, no new capture. The
already-proven half (menu live with both copy-ID items visible in `p17-rclick-term.png`, albeit
truncated by the Files-panel z-order defect also named in F-TERM-UI-01) is untouched. Verdict
carries forward unchanged.

## F-TERM-08 (ledger line 326) — verdict: `FAILED — absent` (changed from `half-proven`)

**Disagreement with the driver.** The driver proposes `exercised-working` (PASSED-equivalent).
That overclaims: it conflates "the underlying kill mechanism works once reached" with "the row
passes," but the row's own clause (`01-inventory-app.md:99`) is "See a terminal/process close
**confirmation** and process termination... invoke Close Terminal…, **confirm the prompt, accept
it**, and confirm the process/pane ends" — a two-part clause, and the confirmation half is not
merely unproven, it is independently confirmed **absent** from the build:

- `requires_close_confirmation` (`rust/crates/tiller_activity/src/activity.rs:30`) has exactly
  the citations `grep -rn` finds workspace-wide: its own definition plus four assertions inside
  one test function, `f_core_act_23_only_live_activity_statuses_require_close_confirmation`
  (`tiller_activity/tests/activity_domain_integration.rs:138-144`). **Zero references from any
  app crate** — nothing in `rust/crates/tiller/src/main.rs` ever calls it.
- Grepped `main.rs` for every confirmation-dialog site: "Close dirty tab?" /
  "Discard unsaved work in {}?" exist only on the **tab**-close path (`close_tab`, lines
  3959-4053, and `:5722-5723`). There is no analogous dialog anywhere in the **pane**-close path.
- `close_terminal_at` (`main.rs:5149-5161`) traced end to end: guards on
  `tab.panes.leaf_ids().len() <= 1 || !tab.panes.contains(focused_pane)`, silent `return` on
  either, otherwise removes the pane immediately — no confirmation gate of any kind sits in front
  of it.
- Confirmed the driver's "identical delegation target" claim by independently tracing both call
  paths: the right-click "Close Terminal…" item (`TerminalContextAction::CloseTerminal` →
  `context_menu.rs:2207` → `TerminalContextCommand::Close` → `main.rs:2819`) and the control
  socket's `pane.close` (`"pane.close" => ControlAction::ClosePane` → `main.rs:2450` →
  `close_focused_pane` → `main.rs:5146`) both terminate in the exact same
  `workspace.close_terminal_at(tab_id, pane_id, ..., cx)` call — genuinely one function, not two
  parallel paths that merely look alike.

This is exactly what triage's "reclassify" called for: the row's confirmation clause is a build
gap, not a gesture the driver merely hasn't tried yet — "one half of two, still owed" undersells
a piece that is provably not there to be gestured at. `FAILED — absent` is the correct verdict
for the row as specified, not `PASSED`/`half-proven`.

The driver's live capture is real and useful, and does independently confirm the *other* half
(process/pane termination) plus a genuine secondary defect:

- `f-term-08/02-f-term-08-two-panes.png` shows the Terminal tab split into two live panes after
  `pane.split direction=right`; `f-term-08/03-f-term-08-after-close.png` shows it collapsed back
  to one pane after `pane.close` — consistent with `panel.list` going 3→2 panels as claimed.
- `f-term-08c/02-f-term-08-guard-noop.png` is visually indistinguishable from its own baseline
  (`01-baseline.png`, single pane) — consistent with the claimed no-op, though the screenshots
  alone cannot prove "`{"ok":true}` returned but nothing changed"; that rests on the driver's
  prose description of `panel.list` output, which was not saved as a log file in
  `reference/linux-progress/wavea-W09-term/`. The source-level guard trace above independently
  confirms the no-op mechanically (every path through the guard condition is a bare `return;`
  with no fallback), so the missing raw log does not change the verdict, but it is a real
  evidentiary gap worth naming.

Net: process kill genuinely works and reaches the real delegation target with zero gesture
required to prove it (the socket path *is* the same function). But the row asks to see a
confirmation prompt before that kill happens, and no such prompt exists anywhere in the pane-close
path — an affordance-absent failure, not a proven pass. The pre-existing single-pane no-op defect
(silent, no error surfaced, no tab-close fallback unlike `CloseTab`) stands as a second, compounding
defect in the same area.

## F-TERM-UI-01 (ledger line 535) — verdict: `half-proven` (unchanged)

Own `discriminating: false`. Same structural blocker as F-TERM-04/06 (menu is right-click-only,
no keyboard path, no socket bypass for menu *rendering*). Driver correctly declines to let
F-TERM-08's socket-level `ClosePane` proof stand in for this row's menu-render-plus-click clause —
that is the right call per `WAYLAND-LANE.md`'s own rule ("Socket-driving closes a row's state
half... the gesture half stays owed"). The z-order defect this row also names (Files panel
painting over an open context menu) is independently visible in `p17-rclick-term.png` itself
("Copy P…" / "Copy T…" truncated by the Files panel on the right) — a real, already-documented
defect, not re-photographed this session because the menu can't be opened from this lane. No new
drive, no new capture. Verdict carries forward unchanged; owed half is still the real
right-click-open-menu-then-click-item path for all 12 items.

## F-TERM-UI-02 (ledger line 536) — verdict: `NOT EXERCISED` (unchanged)

Own `discriminating: false`, no capture taken. Independently verified the claims: `opens_terminal_link`
is called with `event.modifiers.platform` at `rust/crates/tiller_terminal/src/lib.rs:988`;
`wayland-drive.sh`'s `pointer_command` (`move`/`click`) sends only a plain left-button event over
the virtual-pointer FIFO with no modifier field, and `WAYLAND-LANE.md`'s own keyboard-keeper
description (Shift pressed/released once at startup, unmodified thereafter) matches — there is
no facility in this lane to hold a modifier during a click. Per task constraints this driver could
not fall back to the `DISPLAY=:1`/X11 lane to attempt it there either. This restates, without
contradicting, the existing ledger cell almost verbatim. Verdict carries forward unchanged.

## Notes for the orchestrator

- **F-TERM-08 is the one substantive disagreement in this slice.** The driver's own prose
  ("Confirms reclassify: the row's confirmation-dialog clause is a build gap, not an unproven
  gesture") is exactly correct, but the driver's chosen top-level `claim` (`exercised-working`,
  i.e. PASSED-equivalent) contradicts its own finding — an affordance conclusively proven absent
  cannot support a passing verdict for a row whose clause requires seeing and accepting that
  affordance. Reclassify to `FAILED — absent`, not `PASSED`.
- The other four rows (F-TERM-04, F-TERM-06, F-TERM-UI-01, F-TERM-UI-02) are honestly
  non-discriminating re-confirmations, correctly flagged as such by the driver, and every
  underlying source citation checked out against the current tree. No hidden overclaim on those
  four.
- Evidentiary gap (not verdict-changing): the F-TERM-08 captures are screenshots only; no raw
  `panel.list` JSON/log was preserved to back the "byte-identical before/after" claim for the
  guard no-op. The source-level guard trace makes this moot for this slice, but a future driver
  touching this row again should save the raw socket transcript, not just prose-describe it.
