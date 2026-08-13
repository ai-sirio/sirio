# P24 — The terminal and agent surface, proven headless

**You are codex11, pane `w1:p2`.** Your context was just reset, so this brief is everything you
need.

## Why the freeze hunt is stopping, and it is not your failure

You built `Scripts/crash-freeze-supervise.py` with 4 new tests on top of the 8 the earlier
supervisor had, and it behaved correctly: it **refused to call a run a trial** when `xdpyinfo` timed
out, instead of reporting a clean 1.3 minutes. That refusal is the feature. It is also the finding.

Here is what was measured after your run, and it settles the question you were asked:

- `:1` died at 09:56. `:2`'s Xwayland process is **alive and sitting in `ep_poll`**, but `xdpyinfo`
  from a fresh, unrelated client times out.
- The app instance on `:2` was killed to test whether it was holding a server grab. **The display
  did not recover.** So no client is wedging it — **Xwayland itself is stuck**, blocked on a
  compositor that cannot import its buffers (`cosmic-comp: … modifier: Unrecognized(0x0200000000000005)`,
  an AMD tiling modifier its renderer does not know).
- The app's own thread state at that moment: main thread in `poll_schedule_timeout`, 12 workers
  parked in `futex_do_wait`, timer in `ep_poll`. **Not spinning, not blocked in a GPU ioctl.** That
  is a program waiting politely for a frame callback that is never coming.

Which means the "presentation freeze" you were sent to hunt is, on the evidence, **the display
stack, not Tiller**: pixels stop, the process stays healthy, the control socket keeps answering —
exactly what a wedged display server produces in a well-behaved client.

**This is a hypothesis, not a verdict.** It is possible there is *also* a genuine freeze in the app;
that cannot be tested until the environment is healthy, and it stays on the board as unproven
either way. What is certain is that no valid trial can be run today. Your detector stays for the day
it can be.

## The way around the blockage

**The app runs headless and still serves the control socket.** Verified:

```bash
cd /home/enzopalmisano/Scrivania/Progetti/tiller-linux
source ~/.cargo/env
cargo build -p tiller -p tiller_control    # tillerctl is a BINARY of tiller_control
export TILLER_SOCKET=/tmp/codex11.sock
env -u DISPLAY -u WAYLAND_DISPLAY rust/target/debug/tiller &
rust/target/debug/tillerctl current-workspace     # returns real state
```

`TILLER_SOCKET` isolates the endpoint, so every agent can run its own instance concurrently with no
display contention. PTYs need no display, so **terminals are fully exercisable this way.**

## The piece

`docs/linux-rewrite/01-inventory-app.md`, section **`## Terminals and agents`** — 14 entries, each
`- [ ] F-TERM-NN | capability | VERIFY: how to check it | SRC: the Swift original`. The `VERIFY`
clause is the specification.

Two things, in this order:

1. **Establish what actually works**, by driving a headless instance through the socket:
   `panel.create`, `panel.write`, `panel.key`, `panel.read` (base64, strip ANSI), `panel.wait`,
   `panel.focus`, `panel.close`, plus `notify` and `session.ref` where an entry needs them. Use a
   **nonce** — a random string echoed through a real shell and read back — so a pass cannot be
   confused with a plausible-looking blank.
2. **Build what is missing** in your own crates. `tiller_terminal/**` and `tiller/src/panes.rs` are
   yours. Several entries concern agent adapters: `AgentCatalog` covers five CLIs, and on this
   machine `claude`, `codex` and `pi` are installed while **`opencode` and `omp` are not** — so
   entries naming those are **UNREACHABLE, not FAILED**, and must be labelled that way.

Where an entry needs something outside your files, say exactly what and it will be routed. Where an
entry genuinely needs pixels — a rendered cursor, a colour, a layout — mark it **NOT EXERCISED —
blocked on display** and move on. Do not approximate it, and do not tick it from the code.

## Rules and ownership

- **Never copy code from the reference checkouts** (`../_tiller-refs/{waku,zed,orca,t3code}`).
  Every line of Tiller is written from scratch; transplanted code counts as a gap, always.
- **A capability nobody exercised does not exist.** A nonce, a command's output, a file on disk.
- You own `tiller_terminal/**`, `tiller/src/panes.rs`, `Scripts/crash-*`. codex12 is in
  `tiller/src/main.rs` and `tiller_control/**` **right now**, mounting the Changes surface — expect
  churn there and do not edit it. pi is in `tiller_ui/**` and `tiller_theme/**`.
- `source ~/.cargo/env` before using cargo; it is not on the default PATH in these panes, which is
  why your last run reported `cargo: command not found`.

## Reporting

Reply in **12 lines or fewer**: how many F-TERM entries you exercised and the counts (PASSED /
FAILED / UNREACHABLE / NOT EXERCISED — blocked on display), what you built, the nonce evidence for
at least one live terminal round trip, and the honest remainder.
