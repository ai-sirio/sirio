# P59 — The PTY tier: a harness that can test it, and the layer-A bug hiding in it

**You are codex11, pane `w1:p2`.** Your context was just reset, so this brief is everything you need.

## P56 landed, and you did the two things that were actually hard

You separated the rows that were genuinely absent from the ones **stale in your own favour**
(`DB-05`'s chat half, `DB-11`'s session/chat half, both closed by your own P52) and said which was
which instead of quietly re-marking. And you left legacy tabs, agent-account and browser/context
**deliberately absent, with an argument** — no Linux caller needs them, usage and credentials live
in `tiller_usage`. Closing a row by building something nothing calls is the defect this project has
the most of; you declined to add to it.

The fixture change is the one to remember: truncating at `original_len / 2` could still leave a
**SQLite-valid** file once the migration and index footprint grew, so the test passed for the wrong
reason. Cutting a whole page instead makes the corruption real. A test that passes for the wrong
reason is worse than one that fails.

## The piece

Two things, and they are the same crate: **`tiller_terminal` is yours alone.** `pi` is in
`tiller_ui` (Settings), `codex12` is in `main.rs` and `tiller_control` (chat wiring).

### 1. `F-TERM-PTY-03` — layer A is broken and nothing has noticed

The row reads: *"`TILLER_PANE_ID` set only on the control-panel path, never on regular panes
('always' false)"*.

Follow that through. Layer A is the **authoritative** agent-activity signal — agents call
`tillerctl notify --session <paneId> --status <status>` themselves, and a recent layer-A push
suppresses the weaker title signal for a debounce window. But an agent can only pass `--session
<paneId>` if `TILLER_PANE_ID` is in its environment. If regular panes never get it, **no real agent
pane can ever emit a layer-A signal**, and every activity verdict silently degrades to layers B/C/D
while the code that merges layer A sits there looking correct and fully tested.

That is the most expensive shape of bug this project produces: not a crash, but a strong signal that
never arrives, with a healthy-looking implementation downstream of it. Fix it, and prove it with a
**real** pane — spawn one, read its environment, and show an actual `tillerctl notify` round trip
landing on that pane id.

### 2. Give the PTY path a harness that can test it

`codex12` had to **quarantine** three tests in P55, and wrote honestly that *"PTY delivery/debounce
coverage is explicitly lost, not passed."* I promised that the real fix would be its own piece. This
is it.

The diagnosis, measured over three runs so you do not have to re-derive it:

```
test_scheduler.rs:509 — Detected activity on thread Some("PTY reader") ThreadId(102), but test
  scheduler is running on Some("tests::restore_replays_persisted_terminal_scrollback")
  ThreadId(101). Your test is not deterministic.
executor.rs:532    — local task dropped by a thread that didn't spawn it.
                     Task spawned at crates/tiller_terminal/src/lib.rs:594:12
async-task utils.rs:17 — aborting the process
```

A real PTY spawns a real OS thread. GPUI's `TestAppContext` scheduler asserts single-threaded
determinism and **aborts the whole process** when a stray thread ticks — and the panic is attributed
to whichever test happened to be running, which is why a persistence test kept getting accused. It
is deterministic that the harness breaks and nondeterministic who gets blamed.

Design the seam. Some options, and the choice is yours to make and state: a trait boundary so the
terminal host can be driven by a scripted transport in tests while using a real PTY in production; a
separate integration-test binary that never links `TestAppContext`; or a way to hand PTY output to
the scheduler from its own thread. **Then un-quarantine the three tests and show them green** —
`real_pty_activity_status_follows_osc_title_then_settled_content`,
`real_pty_layer_a_debounce_suppresses_first_title_and_accepts_second`, and
`process_refresh_preserves_process_ownership_until_process_gone`. Restoring the lost coverage is the
deliverable; a green gate achieved by leaving them quarantined is not.

### 3. If the seam makes them nearly free

`F-TERM-SCR-02` (no 200 ms settle / 120 ms resize debouncer anywhere in the crate) and
`F-TERM-PTY-06` (`terminal_file_drop` has **zero callers** — one of the seven dead models `pireview`
found). Both are testable only once the harness exists. Take them if the seam makes them cheap; say
so if it does not.

Leave `F-TERM-PTY-07` (stable-host/generation/teardown) and `F-TERM-PTY-08` (pane cache) — they are
their own piece.

## Evidence

Follow `docs/linux-rewrite/EVIDENCE-STANDARD.md`. Note the part `fable` added since your last piece:
reading cannot prove **presence**, but a *validated* read is the only possible disproof of
**absence** — and the mirror test for any "this is absent" claim is *would my probe have found the
feature if it existed?* Several rows here are absence claims; hold them to that.

For your tier: named tests, and for `TILLER_PANE_ID` an **executed** transcript against a real pane,
not an assertion that a variable was inserted into a map.

## The gate is fixed

Your P56 report was right that `Scripts/ci-linux.sh` was unrunnable for you: it inherits
`rustc-wrapper = "sccache"` from `rust/.cargo/config.toml`, and your sandbox cannot let sccache spawn
its daemon. I added a probe to the gate — if sccache cannot serve, it now builds without it and says
so, leaving the config alone for panes where it works. Run the real gate this time.

It may still be red in `tiller_ui` (that is `pi`, mid-piece) and it reported unrelated
`an_exe_outside…` / `the_cwd_fallback…` failures in the `tiller` bin — name anything like that
separately rather than absorbing it.

## Rules

- Work in `/home/enzopalmisano/Scrivania/Progetti/tiller-linux`, branch `linux/gpui-waku`. Confirm
  with `git branch --show-current`.
- **Do not edit** `tiller_ui/**` (`pi`), or `tiller/src/main.rs` / `tiller_control/**` (`codex12`).
  If un-quarantining requires a change in `main.rs`, **say so in your report** and let `codex12` make
  it.
- **Never copy code from the reference checkouts** (`../_tiller-refs/{waku,zed,orca,t3code}`).
- Mark rows `builder-claimed, unverified`, never `PASSED`. **Proceed without asking for approval** —
  the harness design is yours to choose and state.

## Reporting

**12 lines or fewer**: what `TILLER_PANE_ID` was doing and the transcript proving it now reaches a
real pane, the seam you chose and why, the three tests un-quarantined and green (or exactly which
resisted and why), whether the debouncer and the drop path came free, the gate result with
not-yours failures named separately, and the honest remainder.
