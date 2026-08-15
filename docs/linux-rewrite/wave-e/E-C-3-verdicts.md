# Wave E slice E-C-3 — critic verdicts

## `F-SID-11` — FAILED — defective

Code review confirms the wiring the builder describes (`ControlAction::RefreshSidebar`, pushed
from `worktree.set` when `comment.is_some()`, drained by the 40ms poll loop calling
`refresh_sidebar`) is real and does make the comment render **within the same live process**:
live drive (`TILLER_WL_LABEL=sid11clean`), fresh DB, `project.add` + `ctl worktree.set
comment=FIRSTMARK` + forced-repaint `shot` showed "FIRST..." on the `linux/gpui-waku` row.

But the row's own recorded defect was specifically about a **process restart against the same
DB** ("comment confirmed NOT working, live and after a genuine process restart"), and that exact
scenario still fails. Reproduced twice independently, once amid contamination from another
agent's stray compositor window and once in a clean isolated retest: killed the instance, relaunched
against the same DB (same label, same sqlite file — a real `tiller` process restart, not a UI
reset), immediately sent `ctl worktree.set comment=RESTARTMARK`, got back a confirming reply
(`{"comment":"RESTARTMARK",...}`), forced a repaint — the sidebar row shows the bare path with
**no comment segment at all**, not even truncated. `ctl workspace.list` in a separate check
confirmed the server-side state genuinely holds `"comment":"SIDMARK99"` for that path while the
rendered row shows nothing. The builder's fix closes the "nothing tells the GPUI thread to
re-render" root cause for a comment set while the process has been running the whole time, but the
restart case — the concrete scenario the previous critic drove and is what a real user hits after
quitting and relaunching Tiller — is unaddressed and still broken. Instrument: two independent
`Scripts/wayland-drive.sh` restart sequences (same `TILLER_DB`, fresh `tiller` process each time),
frames in scratchpad `ec3/sid11-2`, `ec3/clean2b`.

## `F-PRJ-03` — PASSED

Ran `cargo test -p tiller_ui open_project_initialize_git_creates_a_real_repo_then_adds` at HEAD —
passes. Read the test: it is not a trivial green check — it simulates real clicks through the
actual rendered `+`/"Open"/"Initialize Git" controls via `debug_bounds`/`simulate_click`, injects
the non-git temp folder through `simulate_path_prompt_response` (the one legitimate way to steer
the picker deterministically, since this lane's real `xdg-desktop-portal` picker cannot be aimed at
a non-git folder — a documented environment limitation, not something avoidable), and asserts a
real `.git` directory now exists on disk plus a real `AddProject` event fired. Combined with the
wave-D critic's own live-verified git-repo happy path, this closes the row.

## `F-SET-04` — PASSED

Ran `cargo test -p tiller restored_agent_shell_resumes_only_when_a_session_ref_is_supplied` at
HEAD — passes; read it — proves `restored_agent_shell` launches `claude --resume <ref>` for a
populated map and never emits `--resume` for an empty one. Independently re-read (not trusting the
builder's line numbers) both restore call sites in `rust/crates/tiller/src/main.rs`
(`~4330` and `~9513`): both still gate on `resume_agent_sessions` and pass `BTreeMap::new()` when
it is off, exactly the shape the test proves is safe. No live native-agent restart attempted (would
require fabricating an on-disk Claude session transcript, unnecessary since the gate never reads
one) — code-level proof, not pixels, which is the correct instrument for this row.

## `F-SET-10` — PASSED

Ran `cargo test -p tiller_ui refresh_click_flips_every_provider_to_loading_before_the_fetch_runs`
at HEAD — passes; read it. The test is genuinely discriminating: it first asserts the bar has
already settled to a non-`Loading` state (so it cannot pass by accident off the constructor's own
initial state), then calls the real `on_refresh_clicked` handler and reads state back *before*
`run_until_parked` lets the spawned fetch task run — so it captures exactly the synchronous
Loading-flip guarantee the row cares about, with no capture-timing race (unlike the wayland-drive
round trip that kept losing the race to the fetch under shared-machine contention).

## `F-SET-24` — PASSED

Did not take the builder's word for "already fixed pre-wave." Independently live-drove it fresh
(`TILLER_WL_LABEL=ec3brw24`, single session): `project.add`, `browser.open url=https://example.com`,
`tab.select index=3` to bring the Browser tab forward, `browser.act driving=true`, then
`browser.navigate url=https://agent-target.example`. Reply: `{"origin":"https://agent-target.example",
"permission":"requested"}`. Forced-repaint capture shows the real doorhanger banner rendered in the
Browser surface: "Allow agent browser access to https://agent-target.example? Allow Deny", plus an
"Agent driving" badge in the toolbar, and the address bar still shows the pre-navigation URL
(confirming the navigation itself was correctly intercepted, not merely that a banner appeared).
Frame: scratchpad `ec3/brw24c/02-doorhanger.png`. Confirms `request_permission` has a real,
reachable, agent-driven production caller, not just a caller that satisfies a grep.

## `F-EDIT-08` — PASSED

Did not reuse the builder's or wave-D's screenshots. Independently live-drove it fresh
(`TILLER_WL_LABEL=ec3edit08`): `project.add`, then two rapid synthetic `click` actions at the
CLAUDE.md row in the Files panel (1122,751 at the lane's 1400x900 baseline resolution, confirmed
from a same-session reference shot before clicking blind). First double-click: a single, distinct
`CLAUDE.md` tab opened showing the real rendered file content (title, headings, code blocks) next
to Chat/Terminal — not a placeholder. Second double-click on the same row, same session: tab strip
still shows exactly one `CLAUDE.md` tab — no duplicate. Frames: scratchpad
`ec3/edit08e/02-after-first-dblclick.png`, `03-after-second-dblclick.png`. Reproduces the builder's
claim end to end with a fresh drive, not their capture.
