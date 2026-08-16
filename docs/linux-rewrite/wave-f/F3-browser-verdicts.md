# F3-browser — re-verification verdicts

Re-judged live, from scratch, against a freshly rebuilt binary (`rust/target/debug/tiller`,
rebuilt 2026-08-16 09:39 — the worktree's `target/` was found missing at task start, apparently
mid-recovery by shared infrastructure; rebuilt with `cargo build -p tiller` before any drive).
Instrument: `Scripts/wayland-drive.sh` (nested headless Wayland compositor, `grim` captures,
persistent virtual pointer/keyboard) unless stated otherwise.

## `F-TAB-06` — ledger line 122

**Verdict: PASSED. `heldUp: true`.**

Live re-drive, label `f3crit03`. Single continuous session (`shot base` → `click` the tab-bar `+`
at (1289,48) → `shot menu-open` confirmed the New Tab dropdown with "New Browser" (globe icon) at
row (1043,143) → `click` it → `shot browser-tab` showed a new "Browser" tab in the tab strip with
a globe icon → `click` that tab to bring it forward → `shot browser-selected`).

`/tmp/f3crit03/02-browser-selected.png` (read directly, not just described) shows: the Browser tab
selected and highlighted in both the tab bar and the sidebar tree under the `linux/gpui-waku`
worktree; an address bar reading `https://example.com`; and the browser chrome's own
"Direct XCB build failed ... Wayland ... not supported" banner, which is the documented, expected
Wayland-lane limitation (chrome renders, page content does not — WAYLAND-LANE.md), not a defect of
this row. The row's VERIFY clause is tab creation only, which this drive proves end-to-end through
a real gesture, not a socket call.
