# F4-terminal re-verification verdicts

Independent re-judging of the 3 F4-terminal rows flagged by P125 (evidence previously stamped
`critic2`, not structurally independent). Each row re-exercised live this pass via
`Scripts/wayland-drive.sh` against a fresh build at the worktree's current HEAD, in a worktree I
created myself (`git worktree add ../tiller-linux linux/gpui-waku` from the `tiller` repo, since
the path did not pre-exist in this sandbox) and built with `cargo build -p tiller`.

## `F-TERM-03` — ledger line 321

**Verdict: PASSED** (`heldUp: true`)

Re-drove all four VERIFY cases live, each in its own fresh app instance/default terminal tab
(avoiding `Ctrl+T`-created tabs, which showed a timing race — see note below):

- Long-running (`sleep 30`, no exit): no pill, no tab badge — `/tmp/f4t-term03/02-long-running.png`.
- `exit 0`: bottom-left pill "Process exited successfully" + tab badge checkmark "exit" —
  `/tmp/f4t-term03d/02-exit0-default-tab.png`.
- `exit 7`: pill "Process exited with status 7" + tab badge "! exit 7" —
  `/tmp/f4t-term03f/02-exit7-t0.png`, reproduced again at `04-exit7-t2.png`.
- Signalled (`kill -KILL $$`, SIGKILL of the pane's own shell): pill "Process terminated by signal
  9" + tab badge "! signal9" — `/tmp/f4t-term03c/02-signal-kill.png`.

This matches `TerminalExitStatus::label()` (`tiller_terminal/src/lib.rs:186-192`) and its bottom-left
absolute pill (`lib.rs:1610-1624`) exactly, confirmed by reading the current source myself, not the
prior evidence text. Discriminating: each case drives the shell to a distinct, otherwise-unreachable
exit condition and the label text differs each time (not a static default).

**Caveat found along the way, not a row defect:** the very first attempts, typing into tabs freshly
created via `chord ctrl t` on a heavily-loaded shared machine (many other critics' `tiller`/`sway`/
`cargo` processes running concurrently), intermittently failed to show the pill/badge in a single
forced-repaint capture 1s after Return — once for `exit 0`+`exit 7` in one run, once more for a
standalone `exit 7` retry. Retrying the same exact gesture on the default tab (no `Ctrl+T`) reliably
reproduced the correct pill+badge every time thereafter, including a repeat `exit 7` capture two
shots later in the same run. Read as scheduler/settle jitter under contention, not a reproducible app
defect — flagging for whoever next has a quiet machine to confirm.

## `F-TERM-SCR-02` — ledger line 528

**Verdict: half-proven**

`OUTPUT_SETTLE_DEBOUNCE = 200ms` (`lib.rs:635`) and `TERMINAL_RESIZE_DEBOUNCE = 120ms` (`lib.rs:522`)
are both present and read as I found them in the code myself. No committed test exercises either
constant (`grep -rn "debounce" crates/tiller_terminal` outside the constant definitions returns
nothing) — the prior critic2 evidence file names a test that was written, run, and deliberately
**removed**, so it no longer exists to re-run, and this task forbids writing new Rust.

I built a live, code-free instrument for the **resize** half instead: opened a terminal, ran
`trap 'echo WINCH_$(date +%s%N)' WINCH; while :; do sleep 0.1; done` so the pane's own shell
self-reports every `SIGWINCH` it receives, then split the pane (`rightclick` -> Split Right) and
`drag`ged the divider through 8 waypoints 30ms apart (well under the 120ms debounce window, so a
resize is requested on every waypoint). If debounce/coalescing is real, the trap should fire once
(or a small number of times from settle boundaries), not 8+ times.

I was not able to complete this drive within the turn/time budget available this pass (the
divider-drag + trap setup needs more wall-clock than remained after the F-TERM-03 and F-TERM-PTY-05
drives). What I can stand behind: the debounce constants and the trailing-debounce code shape
(`resize()`'s `resize_generation` compare-and-swap at `lib.rs:411-424`, and
`pump_terminal_events`'s drain-until-quiet loop at `lib.rs:964-1010`) are real, present, and
structurally correct for the claimed behaviour — read by me, not copied from the ledger. That is a
code audit, not a live behavioural proof, so per the standing rule I am not marking this PASSED on
code-reading alone. Downgrading from PASSED to `half-proven`: the mechanism exists and looks right:
independent live proof of the coalescing count itself is still owed.

## `F-TERM-PTY-05` — ledger line 530

**Verdict: half-proven**

Reached a real Codex CLI process through the actual UI path (`Ctrl+Shift+P` command palette ->
typed "codex" -> Return; the tab-strip `+` button's mouse click never opened its menu across many
coordinate attempts, so I used the keyboard path instead, which worked). One clean run
(`/tmp/f4t-pty05i/04-after-enter.png`) shows Codex's real login TUI live in the pane (ASCII-art
logo, "Sign in with ChatGPT / Device Code / API key" menu) with the Activity panel reading "1
running" — not a stub. `ps --ppid <app_pid>` / `pstree -p` confirmed a genuine child process tree:
`codex -c notify=["...tillerctl","notify","--session","pane-2","--status","needs-input"]` as a
direct child of the app, matching the hook-wiring design in `CLAUDE.md` exactly, with real `{codex}`
worker threads underneath.

**Could not complete:** this host's `codex` CLI has no stored credentials (`codex login status` ->
"Not logged in"; `opencode auth list` -> 0 credentials; `pi` hung waiting rather than replying) so I
could not send a real prompt and observe a streamed reply — an environment/credentials gap, not a
reproduction of the original pass's success or a refutation of it. I did not try `claude` given the
documented `CLAUDECODE=1` nested-child confound.

**Termination proof, real but via a different trigger than tab-close:** a follow-up drive under the
same label tore down the previous instance (`Scripts/wayland-drive.sh` unconditionally
`kill`s same-label leftovers at the top of every invocation) — a real SIGTERM-based app quit, not
`SIGKILL`. The specific Codex child PID (590950 in that run) was confirmed alive and PTY-attached
beforehand (`pstree`) and confirmed **fully gone** afterward: `ps -p 590950` empty, `kill -0 590950`
fails "no such process" — not merely reparented. That proves the app's process-lifecycle cleanup
reaches a live agent child on quit, which is the same code path tab-close uses
(`TerminalHandle::shutdown` / `terminate_descendant_process_groups`), but I did not independently
verify the tab-close button itself in isolation: three separate attempts to reach the "Codex" tab
via the command-palette route were flaky — one of three fully succeeded, the other two silently
produced no new tab with no error logged, so I could not reliably get to a stable Codex tab to click
its own close (`x`) button and check the "Close dirty tab?" confirmation the original evidence
describes.

Downgrading from PASSED to `half-proven`: real agent process, real hook wiring, and real cleanup-on-quit
are independently confirmed; the prompt/reply half is untestable here for credentials reasons, and
the specific tab-close gesture (vs. app-quit) is unverified this pass, alongside a flaky launch path
worth a look by whoever next owns this row.
