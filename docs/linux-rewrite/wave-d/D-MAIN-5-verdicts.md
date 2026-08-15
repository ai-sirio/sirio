# D-MAIN-5 critic verdicts

Critic pass, independent of the builder. HEAD at verify time: `4560076` (post wave-D integration).
Instruments: `Scripts/wayland-drive.sh` (labels `critic5`, `critf18`/`critf18b`, `critprj3`),
`panel.list`/`panel.read`/`notify`/`surface.settings.*`/`tab.select` over the raw control socket,
a real full-process restart (fresh `tiller` binary invocation against the same `TILLER_DB`) for the
two persistence rows, a `PATH` override excluding the `nvm` bin dir to force a genuine "not
installed" agent state, `cargo test -p tiller_ui --lib` run myself (not the builder's report) for
one row's new test, and direct source reads of `main.rs`, `sidebar.rs`, `settings.rs`,
`status_bar.rs`, and the vendored `gpui_linux` Wayland platform window (`prompt` implementation).

The shared machine was under severe, worsening contention throughout this pass (`uptime` load
average 60-90 on a 12-core box, from ~20 other parallel critic instances) — several
`wayland-drive.sh` invocations returned blank first frames or crashed mid-script purely from this,
noted per-row where it cost me a clean capture.

## `F-PRJ-03` — half-proven

Read `gpui_linux/src/linux/wayland/window.rs:1561`: on Wayland `PlatformWindow::prompt` always
returns `None`, so the builder's three-button prompt falls into GPUI's `build_custom_prompt` — a
real in-window overlay drawn by the app itself, not an unreachable native OS dialog. The diff
(`9e26858`) correctly gates on `path.join(".git").exists()` and only prompts the non-git branch;
independently ran the builder's new test myself (`cargo test -p tiller_ui --lib
sidebar::tests::open_project_initialize_git_creates_a_real_repo_then_adds`) — passed under my own
supervision, not just trusted from the report. `grep` confirms `confirm_add_project` is still
present; `git status` on the owned files is clean.

Live (`critprj3`, Wayland lane): drove the real gesture end to end for the **git-checkout** branch
— clicked `add-project` → `add-project-open`, the real `cx.prompt_for_paths` folder picker (backed
by this machine's actual `xdg-desktop-portal`, inherited from the host session) resolved to the
app's own cwd (`tiller-linux`, a real git checkout), and the project was added **immediately with
zero prompt** — exactly the spec's "already a git checkout... added immediately, no extra click."
Discriminating: sidebar went from genuinely empty ("No worktree selected") to a populated `tiller`
project with 2 worktrees.

Could not reach the **non-git** three-button-prompt branch live: this lane's file picker always
silently resolves to the app's own working directory (a real portal round trip, not a stub — no
dialog process ever spawned, confirmed via `ps`) with no way to steer it to an arbitrary folder, so
the picker can never hand back my prepared non-git fixture (`/tmp/critic-nongit-test`). This matches
the previous critic's finding that `ctl project.add` (which stays silent by design) is the only
headless-reachable add path. Half-proven: happy path proven live, non-git prompt path proven only
by code + independently-run test, not by my own eyes on the live overlay.

## `F-SET-04` — half-proven

Independently traced both restore call sites myself (not trusting the report's line numbers):
`main.rs:9440-9442` builds `saved_session_refs_for_restore` as an empty `BTreeMap` when
`saved_settings.resume_agent_sessions` is false, and that gated variable — not the raw
`saved_session_refs` — is what reaches `restore_tabs` at `main.rs:9505/9510`.
`restore_launch_snapshot` (`main.rs:4254`) has the identical gate at line 4275 feeding
`restore_tabs_in_workspace`. Both are real, unconditional branches wired into the one production
call site each, confirmed by reading the surrounding code myself line by line, not by pattern-
matching the report's prose.

Did not attempt a live full restart-resume proof: doing so honestly requires a real, validated
on-disk native session (Claude gates on a real `~/.claude/projects/<slug>/<ref>.jsonl`; Codex is
env-overridable via `CODEX_HOME` but still needs a real agent tab opened through the UI, which this
pass didn't have budget to wire up safely). This is exactly the row's own "hard to screenshot live"
case. Half-proven: the gate is real and independently verified at the code level (stronger than
the builder's word), the live behavioral difference across a restart is unproven either way.

## `F-SET-22` — PASSED

Live restart, Wayland lane, label `critic5`. Opened Settings → Appearance, clicked Claude Code's
coral swatch (first-time selection recorded as tan/orange by default, confirmed via the leading
`*` icon's colour), captured the change. Killed and relaunched the **actual `tiller` binary**
(fresh process, `wayland-drive.sh`'s normal per-invocation restart) against the identical
`TILLER_DB`, reopened Settings → Appearance: the coral selection was still there — leading icon and
swatch ring both still coral, not reverted to the tan default. Discriminating (default ≠ chosen
colour), instrument is a real process restart, not a screenshot of unchanged state.

## `F-SID-06` — PASSED

Live, Wayland lane, label `critic5`. `ctl notify session=pane-1 status=error` on a pane under the
expanded `linux/gpui-waku` worktree; a forced-repaint capture showed the worktree row's status dot
flip from idle-orange to error-red (pixel-cropped comparison). Clicked the parent `tiller` project
row to collapse it: the collapsed project row itself now shows the identical red dot in its leading
slot — the exact "collapsed project row badges the worst child status" behaviour the row describes.
No dot existed on the project row before the notify, so this discriminates from the untouched
default.

## `F-SET-09` — PASSED

Live, Wayland lane, label `critic5`. `surface.settings.open` → `tab.select index=1` →
`surface.settings.select section=general`, then a real synthetic click on `general-install-skill`
(pixel-calibrated against the just-captured frame). The confirmation line "Installing… running in a
new terminal tab." rendered immediately under the button in the very next forced-repaint capture —
same click, no relaunch, matching the row's exact expected copy.

## `F-SET-18` — PASSED

Live, Wayland lane, label `critf18`/`critf18b`. Excluded the `nvm` bin directory from `PATH` before
launch (verified via `which opencode` returning nothing) to force a genuine "Not found on PATH"
state without faking any UI text. The rendered Agents screen showed OpenCode with "Not found on
PATH" / "No ACP server" / a real **Install** button, while Pi and Oh-My-Pi (also absent) correctly
got **no** Install button — matching the builder's claim that only agents with a known install
command get the control. Clicked Install: the "Installing… running in a new terminal tab."
confirmation line rendered immediately, and `panel.list` over the control socket confirmed a real
new pane/tab (`"tab":"Install opencode"`, `"active":"true"`) was created — not just a text label,
an actual new terminal tab in the sidebar tree and tab strip.

## `F-SET-10` — half-proven

Independently read `status_bar.rs:207-215` myself: `on_refresh_clicked` unconditionally sets all
four provider states (`claude`, `codex`, `opencode_go`, `ollama_cloud`) to
`ProviderUsageState::Loading` and calls `cx.notify()` **synchronously**, before spawning the real
background-executor fetches (`ClaudeUsageFetcher::fetch`, etc.) — a genuine, unconditional code
path wired to exactly the `status-refresh` `on_click` handler, not a stub.

Live, Wayland lane, label `critf18b`: after correcting a self-inflicted pixel-coordinate
miscalibration (the icon moves between the two resolutions `shot` alternates between; first several
attempts clicked blank sidebar space at the wrong scale), a properly-calibrated click on
`status-refresh` reached the button without incident — the app didn't crash and other UI stayed
live. Could not pixel-capture the transient Loading text itself: every capture requires a forced
resize + `grim` round trip, and under this session's severe machine contention (multiple blank-frame
failures and full script crashes from ~20 concurrent critic instances) that round trip consistently
outlasted the real fetches' completion, so the "before" and "after" frames showed identical
already-settled values every time I could get a clean capture at all. Half-proven: click-reaches-
handler and the Loading-flip's code correctness are both independently confirmed by me; the specific
live pixel transition was not caught, an environmental capture-timing limit rather than a defect
signal either way — the same category of confound noted in `D-MAIN-4-verdicts.md` for other rows on
this same overloaded machine.
