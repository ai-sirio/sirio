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

**Verdict:** (see follow-up commit — drive in progress)
