# Critic verdicts — W01-core

Adjudicated by a critic that neither drove nor built this slice. Driver return:
`docs/linux-rewrite/wave-a/W01-core-evidence.md`, captures under
`reference/linux-progress/wavea-W01-core/`. HEAD under test: `4073297`.

Nothing under `rust/` was touched or compiled. Every image cited below was opened and looked at
directly (`Read`), several were cropped with `convert`/`compare` (ImageMagick) to isolate the one
region a claim actually rests on, and every source citation was re-grepped/re-read against the
current tree. Two rows needed cross-referencing evidence recorded elsewhere in the repo
(`docs/linux-rewrite/P120-report.md`, `reference/linux-progress/p120/logs/`); commit ancestry and
a `git diff` of the relevant crates were checked before trusting that evidence still applies at
this HEAD.

## `F-CORE-ACT-02` (ledger line 338) — verdict: `PASSED` (upgraded from `half-proven`)

Sidebar half: already reconfirmed live and pixel-verified in an earlier pass (E05-core:
hollow `○` → `?` glyph on Terminal/pane-1, independently re-zoomed by that pass's own critic).
Nothing to redo here.

Notification (D-Bus) half: this pass's own attempt produced nothing — verified directly,
`act02/02-menu-open.png` and `act02/palette-entered.png` both show a completely inert app (no
menu, no palette, blank Terminal prompt), matching the driver's "no new pane, ever" account
exactly. But the driver's own evidence log points at `P120-report.md`, which already drove this
exact route live and successfully on this codebase: a real `add_agent_tab` spawn (tab-bar `+` →
`Claude Code`) followed by `tillerctl notify` against the spawned pane, with `dbus-monitor`
watching `org.freedesktop.Notifications`. I read the raw transcripts myself, not just the
report's prose —
`reference/linux-progress/p120/logs/act19-act20-notify-background-fired.log` contains a genuine
`Notify` method call (`app-name="Tiller"`, `title="Claude Code — finished"`,
`body="linux/gpui-waku · tiller"`) when the pane is backgrounded;
`act20-notify-foreground-suppressed.log` shows only bus housekeeping signals (no `Notify` call)
when the same pane is foregrounded; `act19-positive-control-dbus.log` is a manual
`notify-send` confirming the monitor itself was alive throughout. This is wire-level D-Bus
capture, not a pixel diff — it isn't vulnerable to the visual-diff confounds found elsewhere in
this slice (see `F-CORE-ACT-10` below).

Checked that this evidence still applies at the current HEAD rather than assuming it: the P120
commit (`a76e2ff`) is a direct ancestor of `4073297`
(`git merge-base --is-ancestor a76e2ff HEAD`), and `git diff --stat a76e2ff..4073297 --
rust/crates/tiller_activity rust/crates/tiller_control` is empty; the only `main.rs` diff in that
range is 13 lines adding an unrelated `ollama_show_in_bar` settings field. `post_desktop_notification`
(`main.rs:1604`) and `post_activity_notification` (`main.rs:3738`) are untouched. The D-Bus
evidence transfers cleanly.

Both halves of the clause ("observe the sidebar state and transition-driven notification
behavior") now have live, on-HEAD, discriminating evidence — from two different valid drives on
the same code, not one continuous session, but the ledger already treats this row as a composite
of separately-verified halves and that precedent is sound here too. Upgrading to `PASSED`.
(`F-CORE-ACT-19`'s separately-tracked defect — the notification *title format* is agent+status,
not the agent+worktree-label some other clause wants — doesn't affect this row's more general
wording, and stays `FAILED — defective` on its own line.)

**Evidence discriminates:** yes (D-Bus transcript, not pixels).

## `F-CORE-ACT-06` (ledger line 342) — verdict: `half-proven` (upgraded from `NOT EXERCISED`)

The driver's own prose undersells its own captures. Their "clean" run (`act06/clean-x0..x4`) is
one continuous 24-second capture (file mtimes: x0 23:33:35.38 → x4 23:33:59.62, no gap consistent
with an app restart) that types the identifying title, switches to Chat, types an unrelated
title, and switches to Chat again. Cropping just the Terminal tab's status glyph
(`tab_status_glyph`, `main.rs:2007-2015`: `Idle` = `"○"`, `Running` = `"●"`) across that sequence
gives an unambiguous result: `clean-x1` (before typing) hollow → `clean-x2` (after `title ". Working"`,
Chat active) **filled** → `clean-x4` (after `title UNRELATEDPLAINTEXT`, Chat active) **hollow
again**. That is identify-then-clear working correctly, live, in one sitting. The driver's own
"mean pixel diff" read missed this because their diff baseline (`clean-x0-baseline.png`) is
actually the pre-worktree-selection "No worktree selected" screen, not a true Terminal-active
baseline — a mismatched comparison that buried the real, narrow, correct signal under an
unrelated full-UI-state change. The second ("f") run independently corroborates the failure mode
the driver reports for *that* run: `f1` (post-identify) is filled, but `f2` (post-clear-attempt)
is *still* filled — consistent with a dropped keystroke on that specific attempt's clear step,
not a code defect (the clean run's clear step, driven identically, worked).

This confirms, live and cleanly, the clause's first half: "identify a pane by title, then change
only its title to unrelated text and confirm title-owned state clears." The clause's second half
— "repeat with a process-identified pane and confirm unrelated title text does not clear it" —
was not attempted this pass at all (no process-owned pane was ever combined with a title-change
test). That is the owed half. `half-proven`, not `PASSED`.

**Evidence discriminates:** yes, once cropped to the actual glyph region and checked against
`tab_status_glyph`'s source-defined mapping.

## `F-CORE-ACT-07` (ledger line 343) — verdict: `NOT EXERCISED` (unchanged)

Matches the driver's account exactly. Re-grepped: `TITLE_DEBOUNCE = Duration::from_millis(1500)`
(`tiller_activity/src/title.rs:23`) and `should_apply_title_signal` (`title.rs:201`) are real and
wired into `handle_title_change` (`tiller_activity/src/model.rs:223`). No live race (hook status,
then immediate contradictory title, then repeat after >1.5s) was attempted this pass. Source
confirmation is not a live gesture; verdict unchanged.

## `F-CORE-ACT-10` (ledger line 346) — verdict: `NOT EXERCISED` (downgraded from the driver's `partially-exercised` / the prior `builder-claimed, unverified`)

The driver's claimed "clean positive run" does not survive independent re-inspection. Cropped
both signals the app actually exposes for this: the tab's own status glyph
(`tab_status_glyph`: `Idle`="○", `Running`="●") and, separately, the sidebar worktree row's
status dot (`tiller_ui/src/sidebar.rs:2056-2065` — note this one uses the **opposite**
convention: `Running` maps to `None`, i.e. no dot at all, while `Idle`/`NeedsInput`/`Done`/`Error`
all render a colored dot; "a dot only appears for a notable status" per the code comment).

- The tab glyph is hollow (`Idle`) in **all seven** captures (`u0`,`u1`,`v0`,`v1`,`w0`,`w1`,`w2`)
  — it never once shows filled. There is no frame in this row's evidence where the Terminal tab's
  own status glyph reflects a running process.
- The driver's cited "discriminating" `u0`→`u1` diff (top-strip crop, mean 2.93) is explained
  entirely by an unrelated confound: `u0` has the Terminal tab active/bold, `u1` has the Chat tab
  active/bold — switching which tab carries the active-highlight produces a large diff in that
  crop region by itself, with or without any agent detection. `compare -metric AE` on the full
  frames confirms the diff's bounding box is nearly the entire window (1338×828), not a small
  status-glyph region.
- The "negative control" (`v0`→`v1`, mean 0) isn't actually controlling for that confound — both
  `v0` and `v1` already have Chat active (verified by direct crop), so of course nothing changes;
  it measures "did anything happen" between two identical states, not "does the fake process get
  detected."
- Most importantly: `act10/w1-running.png` and `w2-after-kill.png` — the pair the driver logged
  as the *non-reproducing* repeat — actually show the **Chat tab itself running a real, live
  Claude Code conversation**: the composer shows "Type to queue for the next turn…", a `working`
  pill, `Opus Plan Mode`, a token-budget spinner, and (in `w2`) genuine assistant text — "I'll
  check the script's contents first before running it, since it may spawn agents or perform
  actions with side effects." plus a `Read /tmp/act10-spawn.sh` tool call. The driver's typed
  test script was evidently delivered to the Chat message composer instead of (or in addition to)
  the Terminal pane's shell in that run — an accidental cross-contamination of the drive itself,
  not a demonstration of Layer-D/process-owned detection. (This also fully explains why the
  sidebar dot vanishes in `w1`/`w2` only: `Running` hides that dot, and the Chat surface's own
  `is_streaming()` genuinely was `true` there — nothing to do with the Terminal pane's fake
  `claude`-named process.)

None of the three attempts (`u`, `v`, `w`) produced clean, uncontaminated evidence either way for
the row's actual claim. One earlier, separate capture (`t1-after-spawn-typed.png`, before the
tab-switch technique was added) does confirm the spawn script's shell mechanics work — it echoes
`SPAWNED` in the Terminal pane — but that attempt never switched away to check the resulting tab
glyph, so it doesn't close the loop either. Net: no valid discriminating evidence produced this
pass. `NOT EXERCISED`.

**Evidence discriminates:** no — the driver's own claimed discriminating pair does not isolate
the variable it claims to, and the other pair is contaminated by an apparent input-routing
mistake worth a closer look on the next drive (confirm which pane/composer keystrokes are
actually landing in before trusting a screenshot diff for this row again).

## `F-CORE-ACT-11` (ledger line 347) — verdict: `NOT EXERCISED` (unchanged)

No new capture for this row. It requires title-owned, process-owned, and spawn-owned identity
exercised together on three separate panes with close/kill observed to clear only the matching
one. The spawn-owned leg is still blocked (`F-CORE-ACT-02`'s `+`-menu coordinate problem,
reconfirmed above). Title-owned individually now has real evidence (`F-CORE-ACT-06` above), and
process-owned individually still has none (`F-CORE-ACT-10` above) — but neither was combined with
the other two on separate panes as this row's claim requires. Verdict unchanged.

## `F-CORE-ACT-25` (ledger line 361) — verdict: `FAILED — absent` (reclassified from `NOT EXERCISED`, matches driver)

Independently re-ran the grep rather than trusting the count: `grep -rn "BootstrapRestoreOrder"
rust/**/*.rs` returns exactly three hits — the struct's own definition
(`tiller_activity/src/bootstrap.rs:13`), its `pub use` re-export (`tiller_activity/src/lib.rs:50`),
and its own integration test (`tiller_activity/tests/activity_domain_integration.rs:179`). Zero
production call sites. Also confirmed directly: `restore_tabs_in_workspace` (`main.rs:7703`), the
actual startup/restore function, contains no reference to `BootstrapRestoreOrder` anywhere in its
body. Same shape as `F-CORE-ACT-26`'s already-`FAILED — absent` `ids_to_evict`: a real,
unit-tested, pure function the production restart path simply never calls. Reachability settles
this without needing a new restart capture; the manifest's own prior ambiguity (single snapshot
can't distinguish "absent" from "resolves faster than sampled") is resolved by the caller count,
not by timing. Matches the driver's claim exactly.

## Notes / disagreements with the driver

- **`F-CORE-ACT-02`**: driver was appropriately conservative (declined to claim credit for
  evidence it didn't personally re-produce this pass) but that leaves the row under-scored; the
  D-Bus half has real, on-HEAD, wire-level proof on record elsewhere in the repo and I'm crediting
  it. `PASSED`, not `half-proven`.
- **`F-CORE-ACT-06`**: the opposite problem — the driver's own "clean" run captures a full,
  correct identify-then-clear cycle, but the prose evidence log undersells it as "identify only,
  clear never isolated" because the pixel-diff metric used a mismatched baseline frame. Cropping
  the actual glyph region tells a cleaner story than the driver's own numbers do. Upgraded to
  `half-proven`.
- **`F-CORE-ACT-10`**: this is the significant one. The driver reports a "clean, positive-control-backed"
  result and recommends crediting a partial pass; on inspection, the positive pair's diff is a
  tab-switch-highlight confound, the negative "control" doesn't control for that confound, and
  the run the driver logged as *non-reproducing* actually shows an accidental live Chat/Claude-Code
  conversation, not the intended Terminal-pane process test. None of this pass's evidence
  actually speaks to the row's claim. Downgraded to `NOT EXERCISED`.
- If the driver re-attempts `F-CORE-ACT-10`, worth doing two things differently: (1) crop/compare
  the sidebar worktree status dot specifically (`sidebar.rs:2056`), not just the tab strip — it's
  the more informative signal, but its "Running ⇒ no dot" convention is the *opposite* of the tab
  glyph's "Running ⇒ filled dot" and is easy to misread as "nothing happened"; (2) after typing
  into what's meant to be the Terminal pane, capture (or otherwise confirm) the Terminal pane's
  own scrollback before switching tabs, to catch a repeat of this pass's apparent
  keystrokes-into-Chat mixup before it invalidates the run.
