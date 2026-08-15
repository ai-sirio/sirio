# C-MAIN-4 — critic verdicts

**The builder returned nothing usable for this slice: no `C-MAIN-4-report.md` exists, and
`git log` on `rust/crates/tiller/src/main.rs` shows zero commits touching any of these 15 row
IDs (only C-MAIN-1/2/3's and the integrator's own commits landed in that file).** This pass
therefore worked from the slice brief's prior evidence plus fresh, independent verification —
live drives on both the Wayland lane (`Scripts/wayland-drive.sh`) and the X11 lane
(`Scripts/linux-drive.sh`, drive lock held under label `critic-c-main-4`), and direct reads of
the current HEAD source where a live drive wasn't the right instrument. Several rows carried
**stale evidence**: fixes for their underlying defects landed in sibling files on 2026-08-14/15
(after the prior critic passes ran) that the row's evidence never caught up to. Those are called
out explicitly below.

Binary used: `rust/target/debug/tiller`, freshly built (`ls -la` confirmed timestamp
2026-08-15 14:02, newer than HEAD), no compilation performed by this pass.

## Rows

### `F-TAB-01` — ledger line 117
**Verdict: FAILED — defective** (unchanged)

Live-driven, Wayland lane, fresh instance, 2026-08-15. Expanded `docs` (chevron opened,
`linux-rewrite`/`superpowers`/`visual-reviews` children rendered — confirms `toggle_file` itself
works when clicked directly). Then clicked the child row `superpowers` at its rendered position:
the *parent* `docs` row collapsed (chevron closed, children gone) and the click was consumed by
whatever row the list-reflow left under that pixel afterward (`Packages`, now shown
hover/selected) — `superpowers` was never opened or toggled. Reproduces the builder's claimed
defect exactly, with a clean before/after pair (both frames read back and compared).
Instrument: `Scripts/wayland-drive.sh`, frames `c4c/02-docs-expand.png` (parent expanded) and
`c4d/02-child-click.png` (parent collapsed after child click), saved under this session's
scratchpad.

### `F-TAB-08` — ledger line 124
**Verdict: half-proven** (unchanged)

Fresh source read, 2026-08-15: `tab_bar.rs::render_chat_empty` (the "Other agents…" fallback row
in the New Chat submenu) still has no `.on_click`/`.hover`, unlike the sibling
`render_chat_agent_item` a few lines above which does. Byte-for-byte the same code the prior
critic quoted. Not re-driven live this pass (static confirmation only); the click-to-Settings
half stays proven-absent on this evidence, and the fallback-text-shown half was already proven
live in the prior pass.

### `F-TAB-11` — ledger line 127
**Verdict: FAILED — absent** (unchanged verdict, evidence corrected — was stale)

Live-driven, X11 lane, 2026-08-15. Right-clicking a terminal pane now renders a **full 12-item
menu** — Copy, Paste, Copy Context, Set Title, Copy Pane ID, Copy Terminal ID, Split Left, Split
Right, Split Above, Split Down, Clear Terminal, Close Terminal… — contradicting the prior
evidence of "only Copy, no Split items at all." That prior evidence is now stale: source dates
put the full `TerminalContextAction`/`context_menu::items()` list at commit `5e55ed0`
(2026-08-13), before the "sweep A1-P109, 2026-08-14" drive that recorded the empty-menu finding,
so something else was wrong with that drive (not this code). This pass's own screenshot
(`x2-rclick.png`) shows the complete menu.

The verdict is unchanged anyway, because the row's actual clause (`01-inventory-app.md:71`) is
about a **disabled split item with an explanatory reason** when the pane is too small or is the
sole tab — and that capability is confirmed genuinely absent: `context_menu::items()` returns a
flat `const [TerminalContextItem; 12]` with no eligibility field at all, and
`grep -rn "disabled|reason"` across `tiller_terminal/src/{lib,context_menu}.rs` returns zero
hits. Every item renders unconditionally enabled, always, regardless of split-eligibility.
So: menu renders (new finding, corrects stale evidence), but the disabled/reason capability the
row is graded on does not exist (verdict unchanged).

### `F-TAB-13` — ledger line 129
**Verdict: half-proven** (unchanged)

Fresh source read, 2026-08-15: `tab_machinery.rs:150` — `pub(crate) fn add_group` is still
`#[cfg(test)]`-only; `grep -n "active_group()" main.rs` still finds all 12 live tab-creation call
sites hard-coding `self.tab_machinery.active_group()`. Multi-group attach is still structurally
unreachable in production. Z-order fix stands from the prior pass (not re-photographed; no code
in the affected path changed).

### `F-TAB-16` — ledger line 132
**Verdict: PASSED** (upgraded from FAILED — defective)

Live-driven, X11 lane, 2026-08-15. This directly overturns the prior finding, and the underlying
fix (`window.prompt(PromptLevel::Warning, "Close dirty tab?", ...)` in
`request_close_tab_by_id`) has existed since commit `5e55ed0` (2026-08-13), before either of the
two prior "defective" evidence dates (2026-08-14) — those drives evidently missed a real,
already-working feature.

Sequence, all captured: selected a dirty Terminal tab → `ctrl-w` → dialog appeared reading
**"Close dirty tab? / Discard unsaved work in Terminal?"** with Close/Cancel buttons
(`x12-ctrlw-terminal.png`). Clicked **Cancel** → dialog dismissed, tab count unchanged, the tab
was still present (`x13-ctrlw-cancel.png`). Re-ran, clicked **Close** → dialog dismissed and the
tab count dropped by one, tab actually closed (`x14-ctrlw-close.png`). This is a clean
discriminating positive control: Cancel and Close produce different, correct, observed outcomes
on the same dialog.

### `F-TAB-23` — ledger line 139
**Verdict: FAILED — defective** (unchanged, now confirmed via the previously-owed menu route too)

Live-driven, X11 lane, 2026-08-15, via the **right-click menu route** (previously "owed" per the
builder's own note). Typed a unique marker (`MARKER_ORIGINAL_PANE`) into the sole terminal,
right-clicked it, clicked **Split Left**. Result: the marker-bearing (original) terminal ended up
on the **left**, but a close reading of which pane is new vs. old (the freshly spawned one had
not yet produced its shell-integration breadcrumb bar, the original had) together with a repeat
of the same test confirms the **new** terminal is placed on the **right** regardless of
"Split Left" being clicked — i.e. Left and Right are visually indistinguishable in effect.
Screenshots `x4-splitleft.png` and `x5-marker-before.png`.

Source read corroborates two separate bugs converging on the same symptom:
- The **control-socket** route (`pane.split`) collapses `"left"`/`"right"` to the same
  `SplitDirection::Horizontal` with **no placement distinction at all** — `split_terminal_at`
  hard-codes `SplitPlacement::After` unconditionally (`main.rs:5086-5094`), so
  `direction=left` and `direction=right` are byte-identical over the socket. This was already on
  record.
- The **menu route** (`TerminalContextAction::SplitLeft` → `SplitPlacement::Before`) is wired
  *correctly* in the data model (`delegated_terminal_context_action`,
  `PaneNode::split_focused_inner`, and the `flex_row`/`first`-then-`second` render order all
  independently check out as correct on inspection) — yet the live capture still shows the new
  pane on the wrong side. The mismatch between "the placement code looks right" and "the pixels
  are wrong" was not root-caused further within this pass's time budget; recorded as an open
  question for whoever picks this row up next, not resolved.

### `F-TAB-28` — ledger line 144
**Verdict: PASSED** (upgraded from FAILED — defective)

Live-driven, X11 lane, 2026-08-15, using real `xdotool key ctrl+w` (not a synthetic action).
Clean tab (Chat, not dirty): `ctrl-w` closed it immediately, no dialog, tab count dropped by one
(`x11-ctrlw-chat.png` — Chat tab present before, absent after). Dirty tab (Terminal,
`tab_is_dirty` true): `ctrl-w` opened the same "Close dirty tab?" dialog as F-TAB-16, and Close /
Cancel behaved correctly as tested there. Directly contradicts the prior "zero observable effect
on either path" finding; the underlying code (`KeyBinding::new("ctrl-w", CloseTab, None)` →
`handle_close_tab` → `request_close_tab_by_id`) has been in place since `5e55ed0` (2026-08-13),
predating the prior "sweep A1-P109, 2026-08-14" evidence.

### `F-TERM-08` — ledger line 326
**Verdict: FAILED — absent** (unchanged)

Fresh grep, 2026-08-15: `requires_close_confirmation` (`tiller_activity/src/activity.rs:30`)
still has zero callers anywhere in the workspace outside its own definition and its own test
module. `handle_close_pane` → `close_focused_pane` still has no confirmation gate of any kind.
Matches the prior critic's finding exactly on unchanged code — the state half (ClosePane
actually removes the pane) already stood confirmed; the confirmation half is confirmed absent
again, independently, this pass.

### `F-TERM-09` — ledger line 327
**Verdict: FAILED — defective** (unchanged, not re-driven this pass)

`tiller_activity/src/{title,model}.rs` last touched at `5e55ed0` (2026-08-13), before the prior
pass's four-live-agent-launch evidence (pass 17). Re-driving this properly needs real agent CLI
turns (Claude/Codex), which this pass's time budget did not allow for after the extensive
F-TAB/F-TERM split and close-confirm work above. Carrying the prior verdict forward on the
strength of unchanged code plus the prior pass's own thorough live catalog, not re-verified
independently here — flagging that distinction rather than silently inheriting.

### `F-TERM-11` — ledger line 329
**Verdict: PASSED** (upgraded from FAILED — absent)

Live-driven, Wayland lane, fresh instance/DB, 2026-08-15. Launched with zero projects added (no
worktree can be selected). Central terminal surface renders exactly the spec'd state: a terminal
glyph, **"No worktree selected"** (headline weight), and **"Add a project, then select a
worktree."** — not a terminal, not a blank pane. Screenshot `c4b/02-noworktree.png`. Source:
`main.rs:6538-6558`, `div().id("no-worktree-selected")…`; `git log -S"No worktree selected"`
dates this to commit `7289c84` (2026-08-14T17:33:29), which is *after* every prior census that
recorded "still absent" / "zero matches" that day — the fix landed later the same day those
censuses ran, and no subsequent pass caught up to it until now. There is also a same-repo test
(`main.rs:12269`) asserting the same string, but the verdict rests on the live screenshot, not
the test.

### `F-TERM-SPLIT-01` — ledger line 534
**Verdict: FAILED — defective** (unchanged; see F-TAB-23, identical mechanism and evidence)

Same live drive as F-TAB-23 (shared underlying code path: `split_terminal_at_with_placement`).
The recursive/cache lifecycle clause's placement half is defective for the reasons given there.
`TerminalPaneCache` integration was not independently re-checked this pass beyond the prior
finding (unchanged file, not re-read in full).

### `F-TERM-UI-01` — ledger line 535
**Verdict: half-proven** (unchanged verdict, but the previously-owed half is now substantially
closed — was blocked on lane availability, not on the code)

The prior half-proven rested on "the menu cannot be opened from this lane" (Wayland has no
right-click device at all — confirmed this pass too: `wayland-virtual-pointer.c` hard-codes
`BTN_LEFT` (`0x110`), no button parameter exists). This pass used the **X11 lane** instead
(`linux-drive.sh`'s documented `rclick`, button 3 via XTEST), which the prior critic's own note
said should have been tried. Result: hit-testing resolves the right-clicked pane correctly, and
the menu offers every action the spec's clause enumerates (copy/paste/context/title/ID/split/
clear/close — 12 items, matches `context_menu::items()` exactly, see F-TAB-11). Invoked one item
end-to-end (**Split Left** → real pane-split effect observed, see F-TAB-23) — hit-testing and
dispatch both worked, though the resulting placement was itself wrong (a different row's
defect). The remaining 11 items were not each individually invoked and inspected this pass (the
clause's literal ask), so this stops short of PASSED; F-TERM-08 already independently proved the
confirmation-dialog half of "Close Terminal…" absent. One incidental observation, not scored:
the menu's rendered anchor position was visibly offset from the actual right-click point in this
pass's captures — noted for whoever next touches this row, not chased down.

### `F-USE-06` — ledger line 269
**Verdict: half-proven** (unchanged, not re-driven this pass)

`register_restored_agent` (`main.rs:7703`) still has exactly its two prior call sites
(`restore_tabs`, `restore_tabs_in_workspace`), unchanged since the prior pass's finding. A live
positive-fire test (a real agent-identified pane emitting `notify status=running` and observing
D-Bus traffic) needs a real agent launch this pass's remaining budget did not cover — not
independently re-verified live here.

### `F-WIN-07` — ledger line 59
**Verdict: half-proven** (unchanged)

Fresh grep, 2026-08-15: no History menu or previous-launch string anywhere in `tiller`/
`tiller_ui` src; all "history" hits are unrelated (browser back/forward, chat-history setting,
event-sourcing). Matches the prior critic's finding on unchanged code. Did not re-hit the live
socket for the `restoredCount:0` half this pass (relies on the prior pass's own reproduction,
per its note that it did the same).

### `F-WIN-10` — ledger line 62
**Verdict: FAILED — absent** (unchanged)

Fresh grep, 2026-08-15: zero occurrences of `toast` (case-insensitive) anywhere in
`crates/tiller/src/main.rs`. No toast mechanism exists to render, so the prior live-driven
finding (both real errors surfaced as persistent inline dialog text, never a dismissable
top-level notification) stands on unchanged code.

## Summary

| Row | Verdict | Change from prior |
|---|---|---|
| F-TAB-01 | FAILED — defective | unchanged, re-confirmed live |
| F-TAB-08 | half-proven | unchanged, re-confirmed by source |
| F-TAB-11 | FAILED — absent | unchanged verdict, evidence corrected (menu renders; disabled-reason capability confirmed absent) |
| F-TAB-13 | half-proven | unchanged, re-confirmed by source |
| F-TAB-16 | **PASSED** | **upgraded** from FAILED — defective |
| F-TAB-23 | FAILED — defective | unchanged, menu-route half now also confirmed defective |
| F-TAB-28 | **PASSED** | **upgraded** from FAILED — defective |
| F-TERM-08 | FAILED — absent | unchanged, re-confirmed by source |
| F-TERM-09 | FAILED — defective | unchanged, not re-driven this pass |
| F-TERM-11 | **PASSED** | **upgraded** from FAILED — absent |
| F-TERM-SPLIT-01 | FAILED — defective | unchanged, see F-TAB-23 |
| F-TERM-UI-01 | half-proven | unchanged verdict, owed half substantially closed |
| F-USE-06 | half-proven | unchanged, not re-driven this pass |
| F-WIN-07 | half-proven | unchanged, re-confirmed by source |
| F-WIN-10 | FAILED — absent | unchanged, re-confirmed by source |

Three rows (F-TAB-16, F-TAB-28, F-TERM-11) had genuine fixes land in sibling code between
2026-08-13 and 2026-08-14 that no subsequent critic pass had caught up to; this pass's live
drives caught the drift. None of these three were touched by any C-MAIN-4 commit — there were
none — so this is pre-existing code the ledger had simply fallen behind on, not new work by this
slice's (absent) builder.
