## F-TERM-04 (ledger line 322)

**Claim: could-not-reach.**

The remaining half of this row (per `UNPROVEN-ROWS-RECIPES.md`) requires opening the terminal
context menu, selecting `Copy`, pasting into a second pane, then selecting `Paste` from the
menu after copying elsewhere — i.e. it requires invoking the menu twice via right-click and
then clicking a specific menu item.

Checked the source directly: `rust/crates/tiller_terminal/src/lib.rs:1446` and `:1504` wire
`open_context_menu` only to `.on_mouse_down(MouseButton::Right, ...)`; there is no keyboard
alternative (no `Menu`/`Shift+F10` binding found for it). `Scripts/wayland-drive.sh`'s action
vocabulary (`ctl`, `click`, `move`, `type`, `key`, `shot`) implements only left-click
(`click <x> <y> — left-click absolute nested-output coordinates`, line 20 of the script); there
is no right-click, drag, or button-selection primitive available on this lane. This matches
`docs/linux-rewrite/WAYLAND-LANE.md`'s own statement: "Right-click, button-held drag, modifier
chords ... and IME input still require `DISPLAY=:1` until separately proven."

My instructions forbid using `Scripts/linux-drive.sh` or `DISPLAY=:1` for this slice. There is
no socket method (`system.capabilities`) that invokes a terminal context-menu action directly
without the click — the existing half-proven evidence (`p17-rclick-term.png`) was itself
produced on the X11 lane, not this one.

No drive performed; no new capture. Row is not reachable from the Wayland lane.

## F-TERM-06 (ledger line 324)

**Claim: could-not-reach.**

Same blocker as F-TERM-04: the remaining half requires opening the terminal context menu and
clicking `Copy Pane ID` / `Copy Terminal ID`, then reading back the pasted clipboard string.
`open_context_menu` is bound only to `MouseButton::Right` (`rust/crates/tiller_terminal/src/lib.rs:1446,1504`);
`Scripts/wayland-drive.sh` exposes no right-click primitive, only `click <x> <y>` (left-click).
Forbidden from falling back to `Scripts/linux-drive.sh`/`DISPLAY=:1` per this slice's
instructions, and no socket method exists to invoke `CopyPaneId`/`CopyTerminalId` directly.

No drive performed; no new capture. Row is not reachable from the Wayland lane.

## F-TERM-08 (ledger line 326)

**Claim: partially-exercised** (missing half driven, right-click gesture itself still unreachable here).

The recorded finding already covers Route A (Quit) and Route C (external `kill -TERM`) as
zero-survivor, and identifies Route B (right-click "Close Terminal…") as a no-op on a
single-pane tab because `close_terminal_at` (`rust/crates/tiller/src/main.rs:5149`) early-returns
when `tab.panes.leaf_ids().len() <= 1`. What was **not** measured: whether Route B's kill logic
actually works once the guard doesn't apply, i.e. on a **multi-pane** tab.

Drove exactly that, on the Wayland lane, via `pane.close` — confirmed by source read
(`rust/crates/tiller/src/main.rs:2819-2822` and `:6604`/`:5138-5146`) to invoke the identical
`close_terminal_at` function the right-click "Close Terminal…" item calls
(`TerminalContextAction::CloseTerminal` → `TerminalContextCommand::Close` →
`workspace.close_terminal_at(tab_id, pane_id, ...)`); this is the same close logic, reached by a
different trigger, not a different code path. Right-click itself remains unreachable on this
lane (see F-TERM-04/06/UI-01 above; `MouseButton::Right` only, no keyboard alternative,
`wayland-drive.sh` has no right-click primitive).

Drive: `project.add`, `tab.select index=2` (Terminal tab), `pane.split direction=right`, typed
`sleep 999` + Return into the new pane (confirmed running: title bar of the right pane reads
`sleep 999`, `reference/linux-progress/drive-E07-term/f-term-08-sleep-running.png`). Verified via
host `ps --forest` that PID 2698385 (`sleep 999`) was a live child of the pane's bash
(2697780), itself a child of the app process (2692212) — i.e. genuinely spawned by this pane,
not some other agent's fixture.

Sent `pane.close` over the control socket (`{"id":"close1","method":"pane.close","params":{}}`)
→ `{"ok":true,"result":{}}`. Immediately after: `ps` shows PID 2698385 **gone** — no `sleep 999`
process anywhere on the host. Screenshot
`reference/linux-progress/drive-E07-term/f-term-08-after-close.png` shows the tab back to a
single bash pane, split closed.

**Conclusion: on a multi-pane tab, Route B's close logic does kill the child process — it is not
merely a UI-only pane removal that orphans the process.** The defect previously reported (no-op
on single-pane tabs due to the `len() <= 1` guard) stands and is unaffected by this result — this
drive did not retest the single-pane case, only the multi-pane one that was previously unproven.
The row stays below full proof because the right-click gesture that a real user would press is
still not exercisable from this lane.
