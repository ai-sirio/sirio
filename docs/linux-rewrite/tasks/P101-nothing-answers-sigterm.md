# P101 — Nothing answers SIGTERM

`F-PER-01`, `F-PER-03` and `F-PER-06` were all downgraded to `FAILED — defective` on 2026-08-19 for
the same symptom: after a quit, `tab_state.state` reads back
`{"pane_events":[],"scrollback":{"0":[]}}` — the key is there, the content is gone — and the restored
tab comes up as a brand-new shell. The F-PER critic noted "regardless of idle wait", which rules out
the debounce and is the detail that points at the cause.

Anchors read 2026-08-19 at `56a10c0c`.

## The capture is fine. The trigger is missing.

Everything downstream works when it is asked to run:

- `Workspace::layout()` (`rust/crates/tiller/src/main.rs:3833`) really does re-read every terminal
  live at save time — `state.scrollback.insert(pane_id, view.read(cx).capture_scrollback())` — and
  `capture_scrollback` (`crates/tiller_terminal/src/lib.rs:590`) walks the real alacritty grid from
  `-history_size` to `screen_lines`. It returns empty only for `TerminalState::Pending | Failed`.
- The debounce is not the problem either: `SessionStore` runs a background flush thread
  (`crates/tiller/src/session.rs:1262` → `flush_if_due`), so a scheduled layout does reach disk on
  its own.
- The graceful quit path is correct and ordered correctly
  (`main.rs:12169`, `cx.on_app_quit`): `schedule_save` **first**, while the terminals are still
  `Running`, then `shutdown_terminals`, then `flush_now`.

**What is missing is any signal handling at all.** `grep -rn 'SIGTERM|signal_hook|sigaction' ` over
`crates/tiller/` returns nothing. SIGTERM's default disposition kills the process on the spot, so
`on_app_quit` never runs, `schedule_save` never re-captures, and the newest thing on disk is
whatever the last *mutation* happened to schedule — a tab creation, with an empty terminal. That is
exactly `{"0":[]}`.

And typing does not schedule a save; nor should it, at every keystroke. So "type a marker, wait for
the debounce" cannot work by design: there is nothing pending to flush. Only the quit-time
`schedule_save` captures what the user typed, and on SIGTERM it does not run.

This is not only a test artefact. On the original, ⌘Q reaches `applicationWillTerminate`. On Linux,
a desktop logout, `systemctl --user stop`, a session-manager shutdown and any supervisor all send
SIGTERM, and every one of them currently loses the session.

## The fix

Install a handler for SIGTERM (and SIGINT) that runs the same path `on_app_quit` runs — capture,
flush, shut down PTYs — and then exits. It must be async-signal-safe: do not do the work in the
handler. Set a flag or write to a self-pipe/eventfd and have the GPUI loop drain it, the way the
control socket already integrates. Keep it in a platform-gated module: this is POSIX-only, and
Windows needs its own console-control path.

Two things not to break:

- The order in `on_app_quit` is load-bearing. `schedule_save` must run **before**
  `shutdown_terminals`, or `capture_scrollback` sees `Pending`/`Failed` and writes the same empty
  vector by a different route.
- `on_window_closed` (`main.rs:12160`) only calls `flush_now()`, with no fresh `schedule_save`.
  That is fine only if closing the last window also quits the app and therefore runs `on_app_quit`.
  Confirm that live rather than assuming it; if it does not, that path loses the same data.

**Proof, not a plausible patch.** A test that sends a real SIGTERM to a real app process, then reads
the sqlite back, is the only thing that closes these three rows.
`Scripts/Tests/test-x11-unmap-needs-a-flush.sh` is the shape to copy: a private display, a real
process, and an assertion on state observed from outside it.

## One lead that is not explained by the above, and must not be folded into it

`F-PER-03` reports `pane_events` persisting as `[]` after a real 2-pane split made over the control
socket (`pane.split direction=right`), with both panes confirmed live via `panel.read`. Splits *do*
record themselves — `PaneEvent::Split` is pushed at `main.rs:7836` and `:7928` — so if the split
scheduled a save, the flush thread should have written a non-empty `pane_events` well before the
kill.

Two hypotheses, and they call for different fixes:

1. The same SIGTERM cause: the split scheduled a save, but the value read back is older still.
2. The control-socket `pane.split` handler reaches a split path that does not call
   `schedule_save`, in which case a *graceful* quit would lose it too.

**Hypothesis 2 is refuted by reading, 2026-08-19.** `ControlAction::SplitPane` (`main.rs:3541`)
calls `split_focused_terminal_with_placement`, which is one of the two sites that push
`PaneEvent::Split` (`:7836`, `:7928`), and both are followed by `self.schedule_save(cx)` a few lines
down. The socket split does schedule. So the surviving explanation is (1) — but note (1) is not
free either: the store has a real background flush thread (`session.rs:1262`), so a scheduled
layout should reach disk on its own within the debounce, and the critic waited. Something still
does not add up.

What discriminates it, and this comes **before any code**: make the split over the socket, wait past
the debounce, and read the sqlite **while the app is still running**. Non-empty `pane_events` means
the write path works and only the SIGTERM kill lost it, which the fix above already covers. Empty
means there is a third cause nobody has named yet, and finding it is the task — do not fold it into
the signal handler and call the row closed.
