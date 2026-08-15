# D-MAIN-4 critic verdicts

Critic pass, independent of the builder. HEAD at verify time: `4560076` (post wave-D
integration). Instruments used: `Scripts/wayland-drive.sh` (labels `dm4crit1`, `dm4per`,
`dm4work`, `dm4edit`/`dm4edit2..5`), a manual `DISPLAY=:1` launch under the drive lock (label
`dm4crit-browser`, released promptly), the headless lane (`Xvfb :2`, label `dm4chatH`) for the two
persistence rows, direct sqlite3 (`python3 -m sqlite3`) reads of the scratch DBs, and source
inspection of `rust/crates/tiller/src/main.rs`, `rust/crates/tiller_ui/src/browser.rs`,
`rust/crates/tiller/src/session.rs`, `rust/crates/tiller_persistence/src/db.rs` and `model.rs`.

The machine was under severe contention throughout this pass (`uptime` load average peaked at
101 on 12 cores, from many other parallel critic/orchestrator instances) — noted per-row where it
affected an instrument's reliability.

## `F-CTRL-BROWSER-06` — half-proven

Live (`dm4crit1`, Wayland lane): `browser.eval script=1+1` and `browser.console` both now return
`{"ok":false,"error":"browser.eval failed: Browser child is unavailable"}` /
`"browser.console failed: Browser child is unavailable"` — a specific, state-dependent error
(distinct from `"no browser surface"` returned before `browser.open`), not the old blanket
`"unsupported on Linux: not implemented"` pre-rejection. Source confirms `BROWSER_CAPABILITIES`
now includes both methods and `evaluate_script`/`CONSOLE_CAPTURE_SCRIPT` are real, wired
implementations (`tiller_ui/src/browser.rs:978-1000`). This proves the door is genuinely
implemented and reachable — real progress from `FAILED — absent`.

Full content-level proof (a script actually executing and returning a value) needs a working
webview, which Wayland cannot provide (`WAYLAND-LANE.md`: "No webview content"). I took the
`DISPLAY=:1` drive lock (label `dm4crit-browser`) to check X11 directly: `project.add` +
`browser.open` + `browser.eval` still returned `"Browser child is unavailable"`, and a follow-up
`browser.wait` call hung for the full timeout window under the machine's peak load (101) before
the control action itself timed out — inconclusive, not a disproof, since the same run also
produced a `loading:true` state that never resolved. Released the lock immediately rather than
fight the load. Cannot tell a genuine defect from load-starved GTK-loop pumping here, so this stays
`half-proven`: door proven real and reachable, full JS-execution success unproven either way.

## `F-CTRL-WORK-01` — FAILED — defective

Live restart, Wayland lane, label `dm4work`, DB `/tmp/dm4work.sqlite`. Invocation 1:
`worktree.set worktree=p-c1fd7a5bbfd541af-wt-1 comment=RESTART_PROOF_CRIT_D4_7731` echoed the
comment back; a direct sqlite read immediately after confirmed the `worktree` table's `comment`
column held it. Process torn down fully (no `TILLER_WL_KEEP`, default cleanup kills it).
Invocation 2, fresh process against the identical DB file: `worktree.set worktree=...
session=probe` (session-only, no comment param) returned `"comment":""` — empty. A direct sqlite
read confirmed the DB column itself was now `NULL`, not just an in-memory miss.

Root cause, confirmed by source read: `main()` calls `session_store.schedule_catalog(&project_catalog)`
(`main.rs:9393`) **before** the comment-restoring `AppDatabase::open` + `apply_persisted_comments`
block (`main.rs:9417-9425`). `schedule_catalog` → `write_catalog` builds a **fresh**
`WorktreeRecord::new(...)` per worktree (`comment: None` by construction, `session.rs:869`) and
calls `db.save_worktree(&record)`, whose upsert unconditionally sets `comment = excluded.comment`
(`tiller_persistence/src/db.rs:238`) — wiping any previously-persisted comment to `NULL` on every
single startup, before the very same startup's own restore step gets a chance to read it back.
`schedule_catalog` is also called from ~9 other call sites during a running session
(`main.rs:3452` etc.), so the same wipe recurs on ordinary catalog syncs, not only at boot. The
write path the builder added (`persist_worktree_comment`) genuinely writes to the right DB file at
the right column — confirmed live — but a pre-existing, unrelated write path destroys it before a
restart can ever observe it. This is a real, reproducible defect, not merely unexercised: the
builder's own report admits "not re-driven live in this pass," and a live drive reproduces the
prior critic's exact finding.

## `F-EDIT-08` — half-proven

Independently opened the builder's own capture files (`/tmp/dm4-fedit02/03-01-first-open.png` and
`05-03-second-open-attempt.png`, not just their prose) and confirmed by direct pixel inspection: a
real `CLAUDE.md` tab with real rendered markdown content, present in both the tab strip and the
sidebar tree, byte-for-byte the same tab (no duplicate) after the second attempt — genuine evidence,
not a description of one. Source confirms the mechanism: `right_panel.rs:511`'s
`on_mouse_down` checks `event.click_count >= 2`, and `add_file_tab` (main.rs) does the described
scan-and-focus dedup.

My own three independent live attempts to reproduce the gesture (labels `dm4edit2`, `dm4edit3`,
`dm4edit5`; 3 and 5 rapid `click` calls at the Files-panel row) did not open a file tab, despite
measured inter-click intervals of 215-280ms (within GPUI's 400ms window). A later attempt
(`dm4edit4`) and a retry (`dm4edit5`) both produced a blank first frame — `uptime` showed load
average 91-101 on a 12-core box at the time, and `wayland-drive.sh` itself flagged the blank frame
as a presentation failure, not a layout one. Given the machine's confirmed, severe, worsening
overload during every attempt, I cannot distinguish "the gesture doesn't reliably land under this
much contention" from "the feature is broken" — so this is graded `half-proven` on independently-
verified frame evidence plus sound source logic, with my own live reproduction blocked by an
environmental confound I could not clear in this pass.

## `F-GIT-BRANCH-01` — PASSED

Live (`dm4crit1`, Wayland lane, this repo's own worktree): `ctl git.branches
worktree=/home/enzopalmisano/Scrivania/Progetti/tiller-linux` returned
`[{"name":"linux/gpui-waku"},{"name":"main"},{"name":"rust/gpui-rewrite"}]` — checked against a
direct `git branch -a` in the same worktree, which lists exactly those three local branches (plus
remotes, correctly excluded). Exact real branch names through the real Git layer, discriminating
against the previous `NOT EXERCISED` (zero-caller) state.

## `F-PER-08` — PASSED

Full restart roundtrip, Wayland lane, label `dm4per`, DB `/tmp/dm4per.sqlite`. Invocation 1:
`browser.open` → `browser.act driving=true` → `browser.navigate
url=https://permission-proof-origin.example.org` returned `{"permission":"requested"}` (the
doorhanger trigger) → `browser.permission action=allow` → a direct sqlite read of
`browser_origin_grant` showed the origin present immediately. Process torn down fully (default
cleanup). Invocation 2, fresh process against the same DB: `browser.navigate` to the **same**
origin, with `driving=true`, went straight to a real navigation attempt (DNS-resolution failure on
the fake domain, not a permission gate response) — proving no doorhanger was requested, i.e. the
grant survived the restart. Positive control in the same restarted process: `browser.navigate` to a
**never-granted** origin correctly returned `{"permission":"requested"}` again, proving the check
is discriminating rather than a no-op that always skips.

## `F-PER-01` / `F-PERSIST-DB-05` — PASSED (shared fix, shared evidence)

Full restart roundtrip on the headless lane (`Xvfb :2`, label `dm4chatH`, no display lock needed —
persistence rows need a real process tree, not pixels) with `TILLER_ACP_PROGRAM` pointed at a
one-line wrapper invoking `tiller_acp/tests/fixtures/acp_fixture.py normal` as a real ACP backend.
Invocation 1: `surface.chat.send text=hello_restart_proof_9182` streamed a real reply, hit the
fixture's `session/request_permission` (tool "write nonce"), resolved with
`surface.chat.permission optionId=deny`, and the turn completed (`status:"completed"`, assistant
text `" denied"`). Direct sqlite read: `chat_turn` and `session_ref` each held 1 row (the exact
tables pass-17's original finding showed as `0`/`0`). Process killed fully (matched on
`TILLER_SOCKET` env, confirmed dead). Invocation 2, fresh process against the identical DB:
`surface.chat.read` immediately — with zero further calls — showed the complete prior transcript
(`hello_restart_proof_9182`, the tool call, the denial, the assistant reply), proving
`restore_persisted_transcript` now runs on the restored tab. Sent a second turn
(`second_turn_after_restart_4471`) on the restored tab, resolved its own permission prompt, and
confirmed `chat_turn` grew from 1 to 2 rows in the DB — ongoing persistence survives a restart, not
just the very first save. Both rows share this one fix and this one piece of evidence per the
builder's own report.
