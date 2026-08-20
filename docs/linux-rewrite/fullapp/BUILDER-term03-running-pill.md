# Builder fix — F-TERM-03 the terminal pane's status pill had no "running" state

Restores the missing positive "running" indicator on the terminal pane's status pill, per the
user's ruling that the terminal pane is the accepted surface for this row (Swift's own surface —
the chat tool-call card's embedded terminal output — is deliberately out of scope for this port;
see `ToolCallContentInfo::Other` in `tiller_acp/src/lib.rs:356-360`).

Branch: `feat/f-term-03-running-pill-8801`, worktree `/var/tmp/tt-ruling-term03-8801`, off
`ed9dc05f` on `linux/gpui-waku`. Commit `e1b2a8a4`
`fix(F-TERM-03): restore the terminal pane's missing running-state pill`.

## The bug

Swift's `statusChip` (`App/Chat/TerminalOutputView.swift:49-62`) has four branches, `isRunning`
first, then the three exit variants. The Rust port only ever rendered the exit branch
(`rust/crates/tiller_terminal/src/lib.rs`, was `.when_some(self.exit_status.map(...), ...)` around
line 1977) — nothing was drawn while a command was merely executing. Three of four states worked;
"running" had no positive visual at all, only the *absence* of a pill, indistinguishable from
"nothing has run yet."

## The definition, and why it is the hard part

A terminal pane's shell is alive from spawn to teardown, so "a live child process exists" can
never report idle — that is the exact bug this fix corrects, not a fix for it. The signal that
actually distinguishes "idle at the prompt" from "running a command" is the PTY's **foreground
process group**: `tcgetpgrp()` on the master fd, compared against the shell's own pid.

alacritty's PTY setup calls the `setsid()` syscall in the spawned child's `pre_exec`
(`alacritty_terminal`'s `tty/unix.rs`), so the shell process starts as its own session **and**
process-group leader — `getpgid(shell_pid) == shell_pid`. When an interactive shell with job
control (bash/zsh/dash's ordinary `-i` behavior, which is what `TerminalShell::System` spawns)
runs a foreground command, it puts that command into a *new* process group and hands the PTY's
foreground group to it for the duration, reclaiming it once the command exits. So
`tcgetpgrp(master_fd) != shell_pid` is true exactly while a foreground command is running, and
false at an idle prompt — the same technique terminal multiplexers (tmux, iTerm2, screen) use to
report pane activity.

**Rejected alternatives**, and why:

- **A live child process exists** (the pattern `descendant_pids`/libproc already use elsewhere in
  this crate for process-group teardown). True for the *entire* life of the pane, since the shell
  itself is always a live child — cannot ever report idle. This is the exact bug, not a fix for
  it.
- **The tab's own agent-activity dot** (`AgentCatalog`/Layer A-D per `CLAUDE.md`). Identity-gated
  to the five supported agent CLIs; a prior pass (`F-TERMapp.md`, F-TERM-03 row) already confirmed
  a bare `sleep 20` never moves it, so it is silent for exactly the case this pill needs to cover.
  It also answers a different question ("is a recognized agent CLI active") from this row's ("is
  the terminal doing something right now").

## The fix

`TerminalHandle` (`rust/crates/tiller_terminal/src/lib.rs`) now captures the PTY master's raw fd
once at spawn time, *before* `pty` is moved into `EventLoop::new` — the event loop owns the `File`
from that point on, but the bare fd *number* stays valid and queryable for the process's whole
lifetime, so holding just the number is enough. `TerminalHandle::foreground_command_running()`
calls `tcgetpgrp()` on it (a read-only `TIOCGPGRP` ioctl, safe to call concurrently with the event
loop's own reads/writes on the same fd — it does not consume PTY data) and compares the result to
`shell_pid`. `TerminalView::is_command_running()` exposes this publicly; it is `false` once
`exit_status` is recorded, matching Swift's ordering (running wins while running, the exit state
replaces it once the child is actually gone) by construction — the PTY is gone by the time
`exit_status` is `Some`.

The render branch (`impl gpui::Render for TerminalView`) is now an `if self.is_command_running()
{ .. } else if let Some(label) = self.exit_status.map(...) { .. }` instead of a lone
`.when_some(...)`, so the two pills can never both paint. "Running" renders in
`theme.tab_needs_input` — the existing amber/orange status token, already used for
tab-needs-input and `git_modified`, the closest thing this palette has to Swift's plain `.orange`
— labelled **"Running"**, matching the app's own English UI-string convention ("Process exited
successfully", "No worktree selected") rather than importing Swift's Italian "in esecuzione". Exit
labels and their placement (`Process exited successfully` / `Process exited with status N` /
`Process terminated by signal N`, bottom-left, same pill chrome) are unchanged.

**Unix-only.** `tcgetpgrp`/process groups are a POSIX job-control notion with no Windows
equivalent, so `foreground_command_running()` always returns `false` there — a known, documented
gap (the pill simply never shows "Running" on that platform), not a silent wrong answer.

## Tests

`rust/crates/tiller_terminal/src/lib.rs`, all new, all would fail without this change (the method
they call did not exist before it):

- `tests::foreground_command_running_is_false_at_an_idle_prompt` — spawns an interactive
  `/bin/bash --norc --noprofile -i` directly as the PTY child (no dependency on `$SHELL` in the
  sandbox), drains startup, asserts `foreground_command_running()` is `false` at the prompt.
- `tests::foreground_command_running_is_true_while_a_command_executes_then_false_again` — same
  shell, sends `sleep 3\n`, polls for `foreground_command_running() == true` within 2s, then polls
  for it to become `false` again within 6s once `sleep 3` exits — proves the signal is
  self-correcting, not a latch that only ever turns on.
- `view_tests::running_pill_state_is_replaced_by_exit_status_when_the_child_exits` (`#[gpui::test]`)
  — the render-facing surface. Drives one real `TerminalView` through the full arc in the order
  Swift's `statusChip` uses: idle prompt (`is_command_running()` false, `exit_status()` `None`) →
  `sleep 2; exit 7` sent as input (`is_command_running()` becomes `true`, `exit_status()` still
  `None` — the exit pill must not appear early) → the shell's own `exit 7` tears down the PTY child
  (`exit_status()` becomes `Some(Code(7))`, `is_command_running()` is `false` again — the running
  pill does not linger past the hand-off). `TerminalView::render`'s pill branch is a direct,
  one-line function of these two getters, so this is the closest equivalent to asserting on the
  painted pill without a paint-inspection harness (this crate has none for pill *text* — the
  closest existing pattern, `debug_selector`, names an element for hit-testing, not its text
  content — so this is stated as a known limit of the unit-level proof, closed instead by the live
  drive below).

Run: `cargo test -p tiller_terminal --lib`.

- **Before**: 48 tests (`context_menu::` 6, `domain::` 2, `lifecycle::` 3, `link_router::` 4,
  `tests::`/`view_tests::` 33, all in `lib.rs` at `ed9dc05f`).
- **After**: 51 tests, all passing at `--test-threads=4` and standalone; **50 passed, 1 failed** at
  the default (unthrottled) thread count under this box's concurrent multi-agent load.

The one failure, `tests::shutdown_terminates_a_job_control_child_that_detached_into_its_own_process_group`,
is **pre-existing and unrelated** — this fix does not touch `shutdown()`, `ProcessGroupGuard`, or
that test's spawn path. It is a scheduling race in that test itself (it polls only for a *pid
file* to appear, then does one immediate, non-retried `getpgid()` check — under heavy concurrent
scheduling contention the backgrounded `setsid sleep 60 &`'s own fork/exec/`setsid()` chain can
still be in flight at that instant). Confirmed as environment-load flake, not a regression: reran
it in isolation 3/3 green, and the full 51-test suite green at `--test-threads=4` (throttled
parallelism, same box, same binary, run immediately after the failing default-threaded run).

## Live proof

`Scripts/wayland-drive.sh`, label `term03run8803`, binary pinned to a fresh build of this branch
at `/tmp/term03-tiller-bin` (`cargo build -p tiller --bin tiller`, `CARGO_TARGET_DIR=/var/tmp/cargo-target-ruling-term03-8801`).
Fixture: a throwaway one-commit git repo at `/tmp/term03-fixture`.

```
move 400 300
ctl project.add path=/tmp/term03-fixture
ctl workspace.select workspace=/tmp/term03-fixture
chord ctrl t
sleep 2
click 700 400
shot idle-prompt
type "sleep 8; exit"
key Return
shot running
sleep 8
shot after-exit
```

(The leading `move 400 300` works around a real, separate harness bug worth flagging for whoever
drives this script next: `start_virtual_keyboard` never `mkdir -p`s `$INPUT_DIR` itself — only
`start_virtual_pointer` does — so a keyboard-only action string with no pointer verb fails at
`wtype ... >"$VK_LOG"` with "No such file or directory" before a single keystroke is sent. Adding
a harmless `move` forces the pointer setup to run first and create the directory. Not fixed here —
out of scope for F-TERM-03 — but worth a one-line fix in `Scripts/wayland-drive.sh` itself.
Confirmed by first reproducing the bare failure without a `move` in a discarded first pass, then
fixing it forward with the workaround above; the 4-frame run that produced the screenshots below
completed cleanly, `OK 4 frames`.)

### 1. Idle prompt — has run nothing

`builder-term03-running-pill-shots/01-idle-prompt.png`. A freshly opened terminal pane, neofetch
banner and shell prompt visible, **no pill of any kind** at the bottom-left.

### 2. Running — `sleep 8; exit` in flight

`builder-term03-running-pill-shots/02-running.png`. The typed command is visible on the prompt
line (`sleep 8; exit`); the bottom-left pill reads **"Running"** in the amber/orange
`tab_needs_input` tone, on the same pill chrome the exit states use.

### 3. After exit — the shell's own `exit` hand-off

`builder-term03-running-pill-shots/03-after-exit.png`. `sleep 8` completed, the shell ran the
plain `exit` that followed it in the same typed line, the PTY child terminated, and the tab now
shows a green checkmark "exit" badge with the bottom-left pill reading **"Process exited
successfully"** — the pre-existing exit label, unchanged, and the Running pill is gone. This is
the live version of exactly what `running_pill_state_is_replaced_by_exit_status_when_the_child_exits`
asserts at the unit level (that test uses `exit 7` instead of a plain `exit`, so it is the one that
separately proves the non-zero-status label text; the live drive's job was proving the *hand-off*
itself, which holds regardless of which exit variant fires).

## What this does not cover

- **Windows.** `foreground_command_running()` is unix-only by construction (see "The fix" above);
  not exercised here since this box is Linux-only, and the gap is by design, not an oversight.
- **`TerminalShell::WithArguments` panes** (agent CLIs, the "New…" command panel, `for_conflict`).
  These run their command as the PTY's *direct* child — no interactive shell in between — so there
  is no separate job-control fork for `tcgetpgrp` to observe diverging from `shell_pid` in the
  ordinary case; `is_command_running()` stays `false` for the pane's whole life even though the
  command is genuinely running throughout. This is a known scope boundary, not a defect: those
  panes already show *something* running (the command's own output) the instant they're opened,
  which is a different UX shape than an interactive shell sitting idle at a prompt. Not exercised
  live or in the unit tests above — the fix targets the specific bug reported for the ordinary
  interactive-shell case.
