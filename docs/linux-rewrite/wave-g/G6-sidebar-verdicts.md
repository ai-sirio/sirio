# Wave G slice G6-sidebar — critic verdicts

Independent critic pass, 2026-08-16. Never the builder of this slice. Findings below are my own
live drives and code checks this pass, not a re-statement of the builder's report.

## `F-SID-11` — sidebar comment survives a restart — **PASSED**

Live-drove the exact restart scenario the row's own recorded defect describes, with two fully
separate `wayland-drive.sh` invocations (`TILLER_WL_LABEL=critic-g6`, shared `/tmp/critic-g6.sqlite`,
a fresh `tiller` process each time — confirmed fresh by the differing terminal-pane "in bash at"
timestamps in the two captures, 11:43:47 vs 11:44:03):

1. First invocation: `ctl project.add path=<repo>`, `ctl worktree.set worktree=<repo> comment=CRITICMARK77`,
   `shot before` → sidebar row for `linux/gpui-waku` shows "CRITIC…" truncated in the ~130px column.
2. Second invocation, **no `worktree.set` call at all**, only `shot after` → the very first captured
   frame (the script's own automatic baseline shot, taken before any of my actions ran) already shows
   "CRITIC…" on the same row.

This is the discriminating case: an app that never seeded the initial `Sidebar` entity from the
persisted `worktree.comment` column would show a bare row with no comment segment at all on this
fresh process, exactly as the row's own prior-sweep evidence recorded. It did not — the comment
survived with zero further control calls. Read `git show 8860603`: the fix looks up persisted
comments from the already-seeded `control_state` for the *initial* `catalog_for_sidebar` at real
startup, not just on `refresh_sidebar`, which matches the observed behaviour exactly.

## `F-PRJ-05` — Clone a project from a URL — **PASSED** (overturns the ledger's `FAILED — defective`)

**This contradicts both the ledger's current verdict and today's builder/critic report, which claim
a live-reproduced, severe character-drop bug (only 1-3 of 44 chars land) when typing a Clone-URL in
one `wtype` call with no `-d` delay.** I drove that exact gesture — open Clone popover at its real
sidebar-column position (`+` at (294,51) → "Clone Repository…" at (187,112), per `P123`), click the
URL field, one `type` action (`wayland-drive.sh`'s `type` is a bare `wtype` call, confirmed via
`man wtype` that its default `-d` is 0ms, i.e. exactly the claimed failure condition) — **six
independent times**, varying the settle gaps between click and type from "none at all" to "a forced
repaint between every step". All six landed the complete 44-character URL
(`https://github.com/octocat/Hello-World.git`) in the field with zero drops, and the `Destination`
line correctly derived `/home/enzopalmisano/Hello-World` each time.

I then drove the full row's `VERIFY` clause end-to-end in one invocation: typed the URL, clicked
"Clone repository" at (161,547), and captured the result — the sidebar populated with a new
`Hello-World` project and a `master` worktree marked `Primary`. I confirmed this was a real clone,
not UI-only state, by reading `/home/enzopalmisano/Hello-World/.git/config` on disk directly: a real
git working copy with `remote "origin" url = https://github.com/octocat/Hello-World.git` and a
tracked `master` branch. Removed the test clone afterward (`rm -rf`, outside the repo, not tracked).

I cannot explain the discrepancy with the recorded evidence — possibly a load-dependent race that
my environment did not hit in six tries, or a stale-coordinate click that landed outside the field
in the prior run (the row's own evidence gives no exact coordinates to compare). What I can say is
that on the current tree, with the prescribed gesture, repeated and varied, the defect did not
reproduce once, and the full clone flow the row actually tests completed successfully with a
verifiable on-disk artifact. `rust/crates/tiller_ui/src/project_forms.rs` is unchanged at HEAD
(`git log` shows no commits since `4370eea`), so this is not a fix landing under my feet — the
current code already works when driven this way.

## `F-USE-03` — usage-bar provider states — **half-proven** (unchanged verdict, stronger evidence)

Ran `cargo test -p tiller_ui --lib status_bar` myself: 7/7 pass, ~9.6s, including the builder's new
`a_real_timeout_after_a_real_success_dims_the_live_entity`. Read the test body directly (not just the
name): it constructs a real `StatusBar` entity in a `gpui::TestAppContext` window, calls the actual
production `apply_outcomes` method (the same one `on_refresh_clicked`/`ensure_refresh_task` call) with
a real `Success` outcome then a real `TimedOut` outcome, and asserts the entity's own `claude` field
transitions `Loaded -> Stale`, that `segment_dimmed` flips `true`, and that `debug_bounds` still finds
the segment at both steps (renders, doesn't vanish/panic). This is a genuine step up from a pure
reducer/render-function test to a live-entity, production-call-site test — not a token call site: it
exercises the real method with real `Context<StatusBar>` mutation and `cx.notify()`.

It still does not prove pixels visibly dim in the running app. I considered forcing a real 25s
`ClaudeUsageFetcher::TIMEOUT` hang through the Wayland lane myself (a real `claude` CLI is present
here too) but declined for the same reason the builder and wave D did: there is no control-socket
shortcut for the timeout, a real fetch-then-hang sequence costs on the order of 25-50+ real seconds,
and that risks this dispatch's own 180s-silence kill for a pixel check that no test-harness color
introspection in this repo (`debug_bounds` is bounds-only) could even assert against once captured —
I would still be reading a screenshot by eye, which the live-entity test already predicts correctly.
Not a new deferral, not a regression: same open half as every prior wave that touched this row.
