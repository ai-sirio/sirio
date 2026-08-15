# D-P3 critic verdicts

Critic pass, independent of the builder (fce0c00) and the D-P3 report I was given as claims only.
HEAD at verify time: `440f203`. Instruments: `Scripts/wayland-drive.sh` (labels `usg05verify`,
`file03verify4`, `act11explore*`/`act11split1`, sockets under `/tmp/`), a real Python HTTP fixture
server on an ephemeral loopback port, a hand-built `CODEX_HOME`/`auth.json` fixture, `cargo test
-p tiller_usage codex::`, and source inspection of `tiller_usage/src/codex.rs`,
`tiller_agents/src/codex.rs`, `tiller_activity/src/model.rs`, `tiller_ui/src/tab_bar.rs`, and
`tiller_ui/src/right_panel.rs`. `reference/linux-progress/sweep-D*/` frames were not cited; all
frames below are freshly captured this pass.

## F-CORE-USG-05 — PASSED

Live end-to-end drive of the real `tiller` binary, not just the unit test the builder cites.
Built a fixture `CODEX_HOME/auth.json` with `access_token: "stale-access-BEFORE"`,
`last_refresh` backdated to 2026-07-01 (past the 8-day `needs_refresh` gate), plus
`custom_field`/`id_token` markers to prove the merge doesn't clobber unknown fields. Started a
one-shot-capable local HTTP server on an ephemeral port answering `POST /oauth/token` with
`200 {"access_token":"FIXTURE-NEW-ACCESS","refresh_token":"FIXTURE-NEW-REFRESH"}`. Launched
`tiller` under the Wayland lane with `CODEX_HOME=<fixture dir>` and
`TILLER_CODEX_TOKEN_URL=http://127.0.0.1:<port>/oauth/token` exported into the launcher's
environment (the script inherits unlisted vars). The fixture server logged real hits, and the
on-disk `auth.json` afterward reads `tokens.access_token: "FIXTURE-NEW-ACCESS"`,
`tokens.refresh_token: "FIXTURE-NEW-REFRESH"`, while `tokens.id_token`, `tokens.account_id` and
the top-level `custom_field` all survived unchanged — a value only this drive's fixture could have
produced, so it discriminates cleanly from the pre-fix hardcoded-`auth.openai.com` behaviour
(which could never reach a loopback fixture). `cargo test -p tiller_usage codex::` — 13/13 pass,
including the new `refresh_token_honors_the_token_url_override` test. Instrument: real binary +
real filesystem read of the resulting `auth.json`, not a screenshot or a unit test in isolation.

## F-CORE-USG-07 — PASSED

Source-confirmed the builder's specific claim: `tiller_agents/src/codex.rs` (this row's listed
file) has zero usage/refresh/`LoggedOut` logic — `grep -n "LoggedOut\|Err(_)\|refresh"` returns
nothing; the file only implements `AgentAdapter` (`prepare`/`command`/`resume_command`/
`notify_override`). The `Err(_) -> LoggedOut` branch and the refresh/merge gap this row shared
with USG-05 both live in `tiller_usage/src/codex.rs`, which is the file the live drive above
exercised end to end (`needs_refresh` gate -> `refresh_token()` -> real HTTP round trip ->
`save_credentials_to`'s merge). That live proof closes this row's previously-owed "refresh/merge
half" since it is the identical code path. The row's own already-standing claim (a positive
`'Codex 100% 5h'` vs `'Codex logged out'` label pair) was not independently re-driven this pass,
but the newly-closed shared gap was the only owed half per the ledger's own carried-forward note.

## F-CORE-ACT-11 — half-proven (unchanged; new negative live evidence)

Attempted the exact live simultaneous-3-pane drive both Wave-C and the builder left owed. Could
not reach it: the "+" new-tab menu (`tab_bar.rs`'s `new-tab-button`, needed for a spawn-owned
Claude Code tab) never rendered its dropdown in any captured frame, despite (a) pixel-measuring the
"+" glyph's true center via a 5x crop and confirming click coordinates landed on it dead-on
(cursor position in post-click diffs matches within a few px), (b) a same-resolution
before/after pixel diff (`compare -metric AE`, both frames 1715x972) showing only cursor/clock
movement — 3374 differing px, far short of the ~40000+ a 170x250 menu box would cost — proving the
click produced no dropdown at all, not merely a mis-timed capture, (c) forcing extra render passes
via pointer jiggles and double-clicks, and (d) confirming click mechanics themselves work fine in
this same session (a control click on a Files-panel folder row expanded it correctly). The
right-click terminal context menu — which `WAYLAND-LANE.md` documents as a proven positive control
for this exact resize-then-capture technique — also failed to appear in my hands under the same
method (`act11-split1/03-context-menu.png`, cursor on-target, no menu drawn). Given both an
already-documented-working gesture and a fresh one failed identically, this reads as session-level
flakiness (30 concurrent `sway`/`tiller` instances were competing for the same nested-Wayland
machinery at the time) rather than a proven regression, so I am not filing it as a new defect
against `F-CORE-ACT-11` itself — but it means I could not improve on Wave-C's evidence: the
spawn-owned leg's live reachability could not be reproduced today, and the simultaneous 3-pane
cross-check remains unproven. `tiller_activity/src/model.rs` itself is unchanged and still reads
correctly (three disjoint ownership sets, independent clear paths — confirmed by re-reading
`title_owned_panes`/`process_owned_panes`/`agent_spawned`/`pane_closed`), matching the builder's
"no code gap" claim. Stays half-proven: model-level and cross-slice unit proof stands, live
simultaneous proof does not yet exist.

## F-CORE-FILE-03 — PASSED

The builder's own report calls this row "blocked; fix belongs in a foreign file
(`tiller_ui/src/right_panel.rs`)" — but `docs/linux-rewrite/wave-d/INTEGRATION.md` records that an
integrator already applied exactly that patch in commit `329a44e` ("wire drag source onto Files
panel rows"), which is an ancestor of current HEAD (`git merge-base --is-ancestor 329a44e HEAD`
confirms). The builder's "blocked" claim is therefore stale, not current truth, and I live-verified
the result rather than trusting either claim. Drive: `project.add` on this repo, waited for the
Files panel to populate, then used the Wayland lane's new `drag <x1> <y1> <x2> <y2> [steps]`
primitive (added today per `WAYLAND-LANE.md`'s P124 update) to drag the real, drawn `AGENTS.md`
file row onto the open Terminal pane. The terminal's input line received
`'/home/enzopalmisano/Scrivania/Progetti/tiller-linux/AGENTS.md` — the real, shell-quoted absolute
path `terminal_file_drop`/`shell_quote_path` produce — with the `AGENTS.md` row shown selected in
the Files panel, in `file03-shots4/04-after-drag-2.png`. This is a value no default/idle state
could produce (a prior attempt at the wrong file row's y-coordinate produced no text at all,
confirming the result discriminates on a correct vs. incorrect drag, not just app liveness).
`tiller_project/src/file.rs` (this slice's owned file) needed no change, consistent with both the
builder's and Wave-C's finding that the target side was always correct; the only gap was the
now-landed foreign-file drag source.
