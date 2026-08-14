# W01-core evidence log

Slice: `docs/linux-rewrite/wave-a/W01-core.md`. One section per row, appended as each row
finishes (never batched). Lane: Wayland only (`TILLER_WL_LABEL=wavea-W01-core`).

## `F-CORE-ACT-02`

**Claim:** could-not-reach (D-Bus half only; sidebar half was already proven on record).

**Drove:** The manifest's approach — register a pane via a real `add_agent_tab` spawn
(the tab-bar `+` menu's `Claude Code` item), then `tillerctl notify` against that pane
while `dbus-monitor` watches `org.freedesktop.Notifications` — is the right route:
`post_desktop_notification` (`main.rs:1604`) really does shell out to `notify-send`, wired
from `NotificationPolicy::should_notify` at `main.rs:3750/3786`, and `P120-report.md`
already proved this exact route works end to end on this HEAD (background pane fired a
real `Notify` D-Bus call; foreground pane was correctly suppressed).

What blocked a fresh re-drive this pass was purely mechanical: locating the `+` button's
click coordinates without being able to see the frame. P120's cited pixel `(978, 49)` at
1400×900 did not reproduce for me — repeated clicks there and in a wide sweep around it
(x 900–1180, y 34–200) produced either no diff at all against baseline or, in a couple of
spots, a large-area low-magnitude diff consistent with hitting an unrelated control (most
likely triggered `open_settings`, given the diff spans nearly the full window). One click
(`925, 34`) did flip the active tab from Terminal to Chat — it landed on the `Chat` tab
button itself, not the `+`. I derived a menu-region estimate from `p120`'s own
`new-tab-menu-try3.png` (diffed against its `01-baseline.png`, bounding box roughly
x:919–1139, y:49–380) and computed candidate item centers from `tab_bar.rs`'s known row
heights (29px) and gap (6px), but no candidate click produced a new pane in `panel.list`.

I also tried the command palette (`Ctrl+Shift+P` → type `Claude Code` → Return), which has
an identical `NewTabAction::ClaudeCode` entry (`command_palette.rs:398`) and would avoid
the coordinate hunt entirely. The chord never reached the app: `WAYLAND_DEBUG=1` confirmed
the virtual keyboard did send `modifiers`/`key` events, but no visible change occurred
before or after, with or without first clicking into the window. Modifier chords are
already flagged in `WAYLAND-LANE.md` as unverified on this lane (right-click/drag/chords
require `DISPLAY=:1`), and this is consistent with that gap rather than a new finding.

**Observed:** No new pane in `panel.list` from any attempt (stayed at exactly `pane-0`
Chat / `pane-1` Terminal throughout). No `Claude Code` tab ever appeared. Did not reach
`tillerctl notify` / `dbus-monitor` at all this pass — the D-Bus half remains exactly
where P120 already proved it works, just not re-verified live today.

**Captures:** `reference/linux-progress/wavea-W01-core/act02/` — `01-baseline.png`,
`02-menu-open.png`, `03-new-tab-menu.png`, `manual-menu.png`, `manual-menu2.png`,
`sweep-{950,1000,1030,1060,1090,1120,1150,1180}.png`, `palette-{open,typed,entered}.png`,
`c1.png`–`c3.png`.

**Verdict left as:** half-proven, unchanged. Sidebar half stands as already on record.
D-Bus half is not newly disproven — P120 already demonstrated it live on this HEAD — but
this pass could not reproduce the gesture to re-confirm it, purely for lack of a working
click coordinate for the `+` button under this lane's constraints (no `DISPLAY=:1`, no
visual feedback).

## `F-CORE-ACT-10`

**Claim:** partially-exercised.

**Drove:** `panel.write`/`panel.create` turned out to be the wrong instrument —
`tiller_control::panel::PaneRegistry::write` only resolves ids from its own
control-owned `panes` map (`panel.rs:333,538-545`), which is a headless PTY registry kept
"without reaching into the renderer" (`panel.rs:1-7`); it never touches the real
GPUI-rendered workspace tabs, so `panel.write id=pane-1` (the real Terminal tab)
fails with `unknown pane: pane-1` even though `panel.list` reports it — the two report the
same ids but only one is writable over the socket. `panel.create` makes a *different*,
invisible pane (`pane-<pid>-N`) that Layer D's tick never inspects, since it isn't a real
workspace terminal.

So instead: clicked into the real Terminal pane's content (`700,500`), typed a two-line
script (`bash /tmp/act10-spawn.sh`) that symlinks `/tmp/act10claude -> $(command -v
sleep)` and backgrounds `/tmp/act10claude 300 &` — giving the process comm name `claude`,
one of `CATALOG_IDS` (`tiller_activity/src/model.rs:44`) — then switched the active tab to
Chat (`tab.select index=1`) so the Terminal tab's own status indicator, if any, would be
visible rather than suppressed while focused.

**Observed:** Two runs of the identical procedure disagreed. Run 1 (`u0`→`u1`, captures
prefixed `u`) showed a real, positive-control-backed tab-strip change: top-70px-row diff
`u0` vs `u1` = mean 2.93 (bbox `1058x38+325+32`), against a same-action-minus-fake-process
negative control (`v0`→`v1`, plain tab-select with no spawn) that measured **exactly
mean=0** in the same crop — a clean discriminating result. Run 2 (`w0`→`w1`→`w2`, same
script, same timing) did not reproduce it: top-strip diff was only mean 0.075 for both
"running" and "after-kill" frames, and `w1` vs `w2` (running vs. killed) was itself exactly
0 — no visible change from killing the process either, in that run. I could not isolate
why the two runs disagreed within the time available (candidates: virtual-keyboard
keystroke drop on the typed `Return`, so the spawn script never actually ran in run 2; or a
race between the 500ms Layer-D tick and the capture). Terminal body content did change in
both runs (echoed typed text), so keystrokes were reaching the pane in both cases; I did
not confirm the spawn script's own `SPAWNED`/`KILLED` echo text landed correctly in either
run, since the real Terminal pane's scrollback isn't readable over the socket (same
control/app-pane split as above) and the app's own tab was not focused at capture time.

**Captures:** `reference/linux-progress/wavea-W01-core/act10/` —
`u0-baseline.png`/`u1-chat-active-agent-running.png` (positive), `v0-baseline.png`/
`v1-chat-active-control-noagent.png` (negative control, mean=0), `w0-baseline.png`/
`w1-running.png`/`w2-after-kill.png` (non-reproducing second attempt),
`t0-baseline.png`/`t1-after-spawn-typed.png`/`t2-after-kill-typed.png` (terminal-body-only
diffs from the first typing check, before the tab-switch was added).

**Verdict left as:** builder-claimed, unverified — one clean run produced discriminating
evidence for the "status changes while the process runs" half against a true negative
control, but a same-session repeat did not reproduce it and the "clears when killed" half
has no positive result at all. Not enough for PASSED; recording as partial rather than
either could-not-reach (the gesture plainly *can* reach it once) or a clean pass.

## `F-CORE-ACT-06`

**Claim:** partially-exercised.

**Drove:** With the fixed `title <text>` action (`WAYLAND-LANE.md`'s 2026-08-14 addition),
selected the Terminal tab, clicked into it, and typed `title ". Working"` — the `". "`
prefix `identify_agent_from_title` (`title.rs`) recognizes unconditionally as Claude,
assigning identity + `title_owned_panes` + `Running` status on an unregistered pane
(`model.rs:187-207`). Then switched to the Chat tab (so the Terminal tab's own status
indicator, if any, is not suppressed by being the focused tab — same technique as
`F-CORE-ACT-10`) and captured. Then switched back to Terminal, clicked in again, typed an
unrelated plain-text title (`UNRELATEDPLAINTEXT`, no glyph/keyword match), which should
hit the `title_owned_panes.contains → clear` branch (`model.rs:210-219`), and captured
again after switching back to Chat.

Ran this twice, once against a freshly-emptied DB (`project.add` on a brand-new instance,
so no leftover activity state) and once as one continuous app lifetime covering both the
identify and the clear step (a second, separate script invocation restarts the app process,
which resets `AgentActivityModel`'s in-memory state — activity is not DB-persisted — so
identify-then-clear only means anything measured inside one continuous run).

**Observed:** Result was not reproducible run to run, matching the same instability seen
on `F-CORE-ACT-10`. In the isolated single-step run, the identify step alone produced a
real top-strip diff (baseline-Terminal-active vs. after-identify-Chat-active: mean 2.50,
full-frame mean 3.10, plus a real body-text diff of mean 2.85 confirming the `title`
command's `printf` genuinely executed and echoed in the pane). In the combined
identify-then-clear run, the same procedure barely moved the pixels at all (mean 0.063 for
identify, and exactly mean 0 between the identified and cleared captures) — either the
`title` command's keystrokes did not land that time (most likely, given the low
body-visible delta too) or the identify step silently no-op'd. I could not isolate which
within the time available; both runs used the identical typed sequence and timing.

**Captures:** `reference/linux-progress/wavea-W01-core/act06/` —
`clean-x0-baseline.png`/`clean-x1-after-title-focused.png`/`clean-x2-chat-active.png`
(isolated identify step, real positive result), `f0-baseline-term-active.png`/
`f1-chat-active-identified.png`/`f2-chat-active-after-clear.png` (combined
identify-then-clear, near-null both steps).

## `F-CORE-ACT-25`

**Claim:** could-not-reach (source-grounded, no live drive needed).

**Drove:** `grep -rn "BootstrapRestoreOrder::partition\|BootstrapRestoreOrder"
rust/crates/*/src/*.rs` across the whole workspace. Confirmed the triage's claim exactly:
the type's only appearances are its own definition (`bootstrap.rs:13-15`), the `pub use`
re-export (`tiller_activity/src/lib.rs:50`), and its own tests — **zero** call sites for
`partition` anywhere else in the crate graph, including `tiller/src/main.rs`'s actual
startup/restore code (`restore_tabs_in_workspace`, `main.rs:7703`, which does not
reference `BootstrapRestoreOrder` at all).

**Observed:** This is the same shape as `F-CORE-ACT-26`'s already-`FAILED` `ids_to_evict`:
a real, unit-tested, pure ordering policy that the production restart/restore path simply
never calls. No live restart drive can produce evidence either way for a function with no
caller — reachability alone settles it without needing a session snapshot.

**Verdict left as:** reclassify to FAILED — absent. The manifest's own prior NOT EXERCISED
verdict cited an ambiguous single-snapshot restart test as the reason it couldn't decide
between "absent" and "present but resolving faster than sampled"; the caller-count check
above removes that ambiguity directly, without needing a new restart capture.

## `F-CORE-ACT-07`

**Claim:** could-not-reach.

**Drove:** Attempted the manifest's debounce-race gesture — send a hook `notify`, then
immediately force a contradictory recognized title (must not replace it), then repeat
after >1.5s (title may now win) — using the same click+`title`+`tab.select` visual-diff
technique that produced inconsistent results on `F-CORE-ACT-06` in this same pass (one
clean isolated run showed the title gesture landing with a real pixel/body-text delta; a
combined multi-step run in one continuous app lifetime did not). Given that instability was
observed twice today on the identical typed-title mechanism this row also depends on, and
the debounce race additionally needs precise sub-2-second timing around a `notify` call
sandwiched between two `title` calls, I did not spend further budget attempting it live —
a null result here would be indistinguishable from "the keystrokes didn't land" rather than
"the debounce doesn't work", which is not useful evidence either way.

**Observed:** Confirmed via source read that the debounce mechanism itself
(`should_apply_title_signal`, `title.rs:201`, `TITLE_DEBOUNCE = 1500ms`) is real,
unit-tested code wired into `handle_title_change` (`model.rs:222-229`) — consistent with
the manifest's "debounce logic already built" claim. Did not attempt the live race.

**Verdict left as:** NOT EXERCISED, unchanged — the live gesture needs a more reliable
title-injection timing than this pass could establish; see `F-CORE-ACT-06`'s evidence for
the same instability on the underlying mechanism.

## `F-CORE-ACT-11`

**Claim:** could-not-reach.

**Drove:** Nothing new. This row's claim needs all three ownership classes (title,
process, spawn) exercised together on three separate panes, with close/kill on each
observed to clear only the matching pane's state. The spawn-owned leg requires
`add_agent_tab` — the tab-bar `+` menu's `Claude Code`/`Codex`/etc. item — which this pass
could not reach: see `F-CORE-ACT-02`'s evidence above for the coordinate-hunt that did not
find a working click position for that menu under this pass's window state. The
title-owned and process-owned legs individually have partial evidence on record this pass
(`F-CORE-ACT-06`, `F-CORE-ACT-10`), but neither is clean enough, and neither was driven
concurrently with the other two on separate panes as this row requires.

**Observed:** No new capture for this row specifically.

**Verdict left as:** NOT EXERCISED, unchanged — same shared blocker as `F-CORE-ACT-06`/`07`
(title injection, now technically fixed but unreliable in practice this pass) plus the
unreached `+` menu for the spawn-owned leg.

Back to `F-CORE-ACT-06`'s own verdict:

**Verdict left as:** NOT EXERCISED — blocker (broken title injection) is genuinely fixed
and the identify half of the mechanism was driven and produced discriminating evidence
once, but the clear half was never cleanly isolated: the one run that attempted both steps
in sequence did not show the identify step taking effect either, so it cannot speak to
clearing. Recommend a follow-up pass with more settle time between `click`/`title` pairs
and a body-text check after each `title` call to confirm the keystrokes landed before
trusting the visual diff.
