# P14 — Find what kills the app: the silent crash

**You are codex11, pane `w1:p2`.** Your context was just reset, so this brief is everything you
need. Nothing below is optional context — it is the state of the work.

## Where

Worktree: `/home/enzopalmisano/Scrivania/Progetti/tiller-linux` (branch `linux/gpui-waku`).
This is a Rust/GPUI rewrite of Tiller targeting Linux. Nothing is committed; all state is in the
working tree.

```bash
source ~/.cargo/env && cd /home/enzopalmisano/Scrivania/Progetti/tiller-linux/rust
cargo build -p tiller -p tiller_control      # tillerctl is a BINARY of tiller_control,
                                             # `-p tillerctl` fails and leaves a stale binary
env -u WAYLAND_DISPLAY DISPLAY=:1 GPUI_X11_SCALE_FACTOR=1 ./target/debug/tiller
```

`WAYLAND_DISPLAY` must be unset or GPUI prefers Wayland and ignores `$DISPLAY`. Only the session's
own Xwayland (`:1`) can present — Xvfb and Xephyr have no DRI3, and Vulkan needs DRI3 to present on
X11. A capture with one distinct colour means presentation failed, not that the UI is empty.

**Another agent (codex12) is editing `rust/crates/tiller/src/main.rs` right now** and the tree may
not build until it lands its change. If `cargo build` fails on a non-exhaustive
`SidebarEvent::SelectWorktree` match in `main.rs`, that is codex12's work in flight, not your bug —
copy the tree aside (`cp -a`) and work from a snapshot, or wait and retry.

## The defect

The critic — which did not build any of this — named this **the single biggest gap in the whole
project**: until it is fixed, nothing else in the inventory matters, because it kills hosted agents
and unsaved state.

Observed, and this is the entire evidence base:

- **Five deaths**, across **two different builds** and **two different displays**.
- Roughly every **4–13 minutes** of use.
- **No panic message. No log line. No stderr output.** The process is simply gone.
- **Not deterministic.** It is not one reproducible click path.
- **Settings-adjacent in 3 of 5** — the operator was navigating the Settings surface. Treat this as
  a weak correlation, not a diagnosis; 2 of 5 were not.

Already ruled out: it is **not a Rust panic** (no panic output, and panics here unwind visibly), and
it is **not a startup failure** (the app runs for minutes first).

**Failing to reproduce it is not evidence of its absence.** This was already got wrong once: after
a single failed reproduction the defect was written off as environmental contention, and the critic
then observed five more deaths. Budget real wall-clock time and run several instances at once.

## What "done" looks like

Either of these is acceptable — the first is better, the second is genuinely useful:

1. **The cause, with evidence.** Name the mechanism and show the artefact that proves it: a signal
   number, a backtrace, a core dump frame, a kernel message, a device-lost error.
2. **A wrapper that captures the death.** A supervisor that runs the app and records, at minimum:
   the wait status decomposed into exit code vs terminating signal (`WIFSIGNALED`/`WTERMSIG`), the
   last N lines of output, the wall-clock uptime, and whatever the app was showing. If the app
   dies again, this turns an invisible event into a report — which is what the next hunt needs.

Lines of enquiry worth spending time on, in rough order of likelihood:

- **A signal, not an exit.** Run it under a supervisor that decomposes wait status. `SIGSEGV`,
  `SIGABRT`, `SIGBUS` and `SIGKILL` all look identical from the outside — "it vanished" — and they
  point at four completely different causes. This single fact splits the search space more than
  anything else, so get it first.
- **A core dump.** `ulimit -c unlimited`, then `coredumpctl list` / `coredumpctl gdb`. Check whether
  systemd-coredump is running and whether `/proc/sys/kernel/core_pattern` sends dumps somewhere you
  can read.
- **An abort from a non-main thread.** A panic in a GPUI background thread, or a `panic` while
  another panic unwinds, aborts the process — and the message can be lost if stderr is buffered or
  the terminal is gone. Try `RUST_BACKTRACE=full` plus a `panic::set_hook` that writes to a **file**
  and `fsync`s it, not to stderr.
- **The GPU.** wgpu/Vulkan device loss (`ERROR_DEVICE_LOST`) or an out-of-memory on the swapchain
  will take the process down. `VK_LOADER_DEBUG=all`, validation layers if installed, and look for a
  correlation with resize or with the Settings surface repainting.
- **The kernel killed it.** `dmesg -T | tail`, `journalctl -k`, and check for OOM. Several instances
  of a debug-build GPUI app on one machine is not a small amount of memory.
- **Someone calls `exit`/`abort`.** `grep -rn "process::exit\|process::abort\|unwrap_unchecked" crates/`
  — a clean `exit(0)` from a stray handler looks exactly like this and logs nothing.

## Rules that apply to every piece of this project

- **Never copy code from the reference checkouts** (`../_tiller-refs/{waku,zed,orca,t3code}`).
  Inspiration only; every line of Tiller is written from scratch. Transplanted code counts as a gap.
- **A capability nobody exercised does not exist.** Claims carry evidence: a command's output, a
  nonce, a file on disk, a frame.
- **Measure, do not describe.** Three confident visual readings were wrong before this rule existed.

## Ownership — three agents share this worktree

You own `rust/crates/tiller_terminal/**` and `rust/crates/tiller/src/panes.rs`.

**Do not edit** `rust/crates/tiller/src/main.rs` (codex12 is in it now) or
`rust/crates/tiller_ui/**` (pi's). Diagnosis is read-only anywhere — read whatever you need. But if
the culprit turns out to live in a file you do not own, **report it, do not patch it**: say which
file and what the fix is, and it will be routed. A silent edit under another agent's cursor is how
two hours of someone else's work disappears.

New files you create for the hunt (a supervisor script, a log) are yours; put scripts in
`Scripts/` with a name starting `crash-`.

## Reporting

Reply in **12 lines or fewer**: what you ran, how long, what died or did not, the wait status /
signal if you got one, the cause if you found it, and the honest remainder. If you did not
reproduce it, say so plainly — that is a real result and it must not be dressed up as a fix.
