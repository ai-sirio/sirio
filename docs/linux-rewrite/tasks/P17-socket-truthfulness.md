# P17 — The automation surface must be true, because everyone measures with it

**You are codex12, pane `w1:p3`.** Your context was just reset, so this brief is everything you
need.

**P9b is closed and you closed it properly.** `tillerctl` reported `linux/gpui-waku … wt-1` before
the click and `rust/gpui-rewrite … wt-0` after it, with a 1470x833 screenshot at 9303 colours — a
real rendered frame, not a blank one — plus 56 tests and a restart that restored the selection.
`ControlState::current` now has a writer. That is the standard this project runs on.

## Where

Worktree: `/home/enzopalmisano/Scrivania/Progetti/tiller-linux` (branch `linux/gpui-waku`).
Rust/GPUI rewrite of Tiller, targeting Linux. Nothing is committed.

```bash
source ~/.cargo/env && cd /home/enzopalmisano/Scrivania/Progetti/tiller-linux/rust
cargo build -p tiller -p tiller_control      # tillerctl is a BINARY of tiller_control;
                                             # `-p tillerctl` fails and leaves a stale binary
env -u WAYLAND_DISPLAY DISPLAY=:1 GPUI_X11_SCALE_FACTOR=1 ./target/debug/tiller
```

`WAYLAND_DISPLAY` must be unset or GPUI ignores `$DISPLAY`. Only a DRI3 display presents — a capture
with one distinct colour means presentation failed, not that the UI is empty. **pi is editing
`tiller_ui/**` and `tiller_theme/**` and codex11 is in `tiller_terminal/**` right now**; a build
failure in those is their work in flight, not yours.

## Why this piece matters more than its size suggests

The control socket is not just a feature — it is **the instrument the critic and every other agent
use to find out what the app is doing**. A socket method that answers wrongly does not merely fail
its own inventory entry; it corrupts every conclusion drawn through it. This project has already
lost three results to broken instruments, and the cost each time was a confident, false statement
about code that was fine.

## The job: re-exercise before you repair

The critic's pass-1 report lists three failures against the socket:

1. `panel.list` cannot see the app's own panes
2. `panel.wait` ignores its timeout
3. user-mode `notify` is missing

**Treat each as a hypothesis, not a fact.** They were recorded at 02:55, before your last two
pieces landed, and reading the code now suggests at least two may already be false:

- `panel.wait` (`main.rs:429`) parses `timeoutMs`, rejects a non-integer with a clear error, and
  passes `Some(Duration)` into `PaneRegistry::wait` (`tiller_control/src/panel.rs:277`), which uses
  `wait_timeout`. That is a handled timeout, not an ignored one.
- `panel.list` calls `list_for(working_directory)`, and `main.rs:1084` registers the app's own panes
  via `set_external`. So the panes ought to be visible.

**And there is a specific reason `panel.list` may have looked broken that is worth testing first.**
`panel.list` resolves its directory from `state().working_directory(worktree)`, which — when no
`worktree` parameter is given — falls back to `current_workspace()`. Until you fixed it an hour ago,
`ControlState::current` was assigned once at startup and never updated. So at the time the critic
tested, `panel.list` was very likely filtering panes against a **stale working directory** and
correctly returning none for it. If that is what happened, the finding was a true observation of a
different bug — the one you have already fixed — and the right outcome here is to prove it, not to
change `panel.list` at all.

**"Fixing" something that already works is not free**: it spends the piece, and it risks breaking a
method that the critic is depending on right now.

So for each of the three, produce one of these two, with the transcript pasted:

- **STALE** — a live `tillerctl` transcript showing the method behaving correctly, plus one
  sentence on why it looked broken before.
- **REAL** — a live transcript showing the failure, then the fix, then a transcript showing it
  fixed.

For `panel.wait` specifically, prove the timeout **empirically**: call it against a pane that will
not exit, with a short `timeoutMs`, and show that it returns at roughly that time rather than
hanging — and that the response distinguishes "timed out" from "exited with code N". A wait that
cannot tell those apart is worse than one that hangs, because it reports a lie quickly.

For `notify`, first establish what "user-mode notify missing" was actually claiming — `notify` is
dispatched at `main.rs:533`. Find out what the critic could not do, and either do it or state
precisely what is absent.

## Then: the whole automation section

`docs/linux-rewrite/01-inventory-app.md`, section `## Control socket and automation surface`, is
the contract — each entry is `- [ ] F-XXX-NN | capability | VERIFY: how to check | SRC: the Swift
original`. **The VERIFY clause is the script.** Work through every entry in that section and report
which pass, with the command output. Where an entry needs something that does not exist yet, say so
rather than approximating it.

## Rules and ownership

- **Never copy code from the reference checkouts** (`../_tiller-refs/{waku,zed,orca,t3code}`).
  Every line of Tiller is written from scratch; transplanted code counts as a gap, always.
- **A capability nobody exercised does not exist.** A passing unit test is not a substitute for a
  live transcript against the running app — that distinction is exactly what this piece is about.
- You own `rust/crates/tiller_control/**` and `rust/crates/tiller/src/main.rs`. codex11 owns
  `tiller_terminal/**` and `tiller/src/panes.rs`; pi owns `tiller_ui/**` and `tiller_theme/**`. If
  you need a change in someone else's file, say so and it will be routed.
- The app dies silently every 4–13 minutes, no panic and no log. codex11 is hunting it; if it kills
  a run, note the time and retry.

## Reporting

Reply in **12 lines or fewer**: for each of the three findings, STALE or REAL and the evidence; how
many automation-section entries you exercised and how many passed; and the honest remainder.
If a finding turns out to have been a symptom of the worktree-selection bug you already fixed, say
so plainly — a retracted finding is a real result and belongs in the record.
