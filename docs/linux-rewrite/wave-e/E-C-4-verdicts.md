# Wave E slice E-C-4 — critic verdicts

*Independent re-drive. A prior committed version of this file exists (commits `296ac63`,
`5ec3f0e`, `43bfd20`, `2c95491`) with matching verdicts for four rows; this pass corroborates those
four with a fresh live drive and supersedes the fifth (`F-TERM-UI-01`) with stronger evidence that
finds a real defect the prior pass ran out of budget before reaching.*

## `F-SID-15` — PASSED

Live drive (Wayland lane, `TILLER_WL_LABEL=ecverif2`/`ecverif3`, two independent runs):
`project.add` this repo, `rightclick 150 222` on the worktree row, settling `shot`, `click 82 340`
on "Remove Worktree". Both runs: the real "Remove worktree? This permanently deletes the
worktree's directory…" dialog (Remove Worktree/Cancel buttons) appeared centred over the whole
window, and the context menu closed — the `deferred(...)` fix (`sidebar.rs`) works and reproduces
cleanly. Second run additionally clicked Cancel: dialog closed, worktree still present in the
sidebar, nothing deleted — the cancel path is clean too.

New finding, outside this row's own pass condition: in both runs the same click **also** opened
the "New Worktree in tiller" inline creation form in the sidebar, simultaneously with the correct
confirm dialog. Reproduced twice. Plausible mechanism: the deferred menu item's mouse-down closes
the menu synchronously, so by the time the click's mouse-up is evaluated the "New Worktree…" row
underneath is what's under the cursor, and GPUI's `on_click` fires again there. This does not
violate `F-SID-15`'s stated acceptance clause (dialog appears centred, menu closes — both true),
so this row is PASSED, but the fan-out is a real collateral defect worth its own row.

## `F-TAB-01` — PASSED

Live drive (`TILLER_WL_LABEL=ectab01`): `project.add`, `click 1367 271` on the `docs` row to
expand it — `linux-rewrite`/`superpowers`/`visual-reviews` appeared as children. Then four
consecutive settling `shot`s (each with its own 1s repaint sleep, ~4s total, spanning multiple
1s periodic-refresh ticks — the status-bar clock advanced across captures) with no other input:
`docs` stayed expanded with the same three children visible in every frame. Discriminates cleanly
against the pre-fix symptom (collapsed back on the very next tick, no click needed).

## `F-TAB-11` — FAILED — absent

Source-confirmed at current HEAD, matching the builder's "blocked" claim exactly:
`TerminalContextItem` (`tiller_terminal/src/context_menu.rs:30`) has only `label`/`action`/
`route` — no `enabled`/`disabled_reason` field. `ITEMS` (line 36) is a flat compile-time
`const [TerminalContextItem; 12]`; `items()` (line 98) takes no parameters and cannot vary per
call. `split_disabled_reason` (`tiller/src/panes.rs:218`) carries `#[allow(dead_code)]` and its
only non-test reference is inside its own `#[cfg(test)]` module (lines 798/818/831) — zero
production callers. The feature genuinely does not exist; the builder correctly declined a
partial/fake wire-up rather than gaming a grep. FAILED — absent, not UNREACHABLE: the code path is
confirmed missing, not merely hard to drive.

## `F-TERM-08` — PASSED

Live drive (`TILLER_WL_LABEL=ecterm08c`/`ecterm08d`): `project.add`, clicked the Terminal pane
(`pane-1` per `panel.list`), `ctl notify session=pane-1 status=running` (confirmed by the red tab
dot and "1 running" in the Activity footer — a marker a fresh pane never shows). First `rightclick`
silently dropped (documented lane trap, reproduced live); a second `rightclick` opened the 12-item
menu. Clicked "Close Terminal…" — the real "This pane has running work. Close anyway?" banner
appeared with Close Anyway/Cancel controls, not an immediate close; did not click through.
Discriminating: a fresh, non-notified pane would close immediately with no banner.

## `F-TERM-UI-01` — FAILED — defective

Live drive across five separate instances, going beyond both the builder's and the prior critic
pass's coverage. Reconfirmed the 12-item menu renders correctly and completely in every capture.

**Confirmed working, with directly visible signatures:**
- **Clear Terminal**: typed `echo CLEARMARKER456`, confirmed it in scrollback, reopened the menu,
  clicked Clear Terminal — scrollback emptied to a bare prompt, menu closed. Clean, reproducible.
- **Split Above**: typed `echo ABOVEMARKER` as a marker, reopened the menu, clicked Split Above —
  a new empty pane appeared *above* the marked original (marker pane moved down). Correct
  direction.
- **Split Down**: same recipe with `echo DOWNMARKER`, clicked Split Down — new empty pane appeared
  *below* the marked original (marker pane stayed on top). Correct, and opposite of Split Above,
  confirming the direction parameter is real and wired correctly, not a coincidence.

**Confirmed broken:** **Set Title** produces no visible effect. Reproduced twice
(`TILLER_WL_LABEL=ectermui07`, `ectermui08`): rightclick → settle → rightclick (menu opens) →
click squarely on "Set Title" (hover-highlighted in the capture immediately after the click,
confirming the click landed on the right item) → typed `MYCUSTOMTITLE` → Enter. In both runs the
menu closes but the tab label is unchanged — still plain "Terminal" (confirmed by pixel-cropping
and zooming the tab bar region), no title-entry field ever appears anywhere in the frame, and the
typed text has no visible effect anywhere. Source read explains why: `handle_context_action`
(`tiller_terminal/src/lib.rs:1168`) emits `TerminalContextEvent` for `SetTitle` exactly like it
does for the Split actions (which I proved dispatch correctly), and `subscribe_terminal`
(`tiller/src/main.rs:3350`) routes it to `set_terminal_title`, which does **not** open a text
field at all — it's a stub: `tab.title = format!("Terminal {terminal_id}")` (main.rs:3497,
comment: "until a text-entry prompt is added"). Even that stubbed behaviour should have visibly
changed the tab label (e.g. to "Terminal terminal-0") and did not — the label never moved off
plain "Terminal" in either run, so the defect is not just "no text entry UI" (which the code
admits) but that the fallback auto-title doesn't render either. No crash, no error in the app log.
This item is squarely inside this row's own drive list and fails outright, not merely
under-evidenced.

**Not reached:** Copy, Copy Context, Copy Terminal ID (need a paste-back to observe — ran out of
budget).

Verdict: the row's acceptance requires driving each of Copy/Copy Context/Set Title/Copy Terminal
ID/Split Above/Split Down/Clear Terminal. Three are now positively confirmed working (Split
Above, Split Down, Clear Terminal) and one is positively confirmed broken (Set Title) — a
concrete defect, not an evidence gap, so `FAILED — defective` rather than `half-proven`.
