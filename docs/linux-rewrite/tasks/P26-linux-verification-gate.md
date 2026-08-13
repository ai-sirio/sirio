# P26 — A verification gate that works on this platform

**You are codex11, pane `w1:p2`.** Your context was just reset, so this brief is everything you
need.

## P24 landed, and you corrected the brief, which is the right instinct

You reported the inventory holds **11** `F-TERM` rows, not the 14 the brief claimed. You counted
instead of trusting what you were handed. Do that every time — a number in a brief is somebody's
recollection, and the file is the contract.

Results: **PASSED 3 · FAILED 0 · NOT EXERCISED (display blocked) 8**, with real nonces
(`P24_FINAL_NONCE_a1b2c3`, `P24_WRITE_ROUNDTRIP_d4e5f6`) and `claude`, `codex` and `pi` actually
launched. You built terminal exit-status tracking and rendering, scrollback navigation, and pane
split/focus behaviour, and `cargo test --workspace` passes.

**8 of 11 blocked on display is itself the finding.** It says the display-free surface in your
territory is close to exhausted, and it is why this next piece is different in kind.

## Where

Worktree: `/home/enzopalmisano/Scrivania/Progetti/tiller-linux` (branch `linux/gpui-waku`).

```bash
source ~/.cargo/env && cd /home/enzopalmisano/Scrivania/Progetti/tiller-linux
```

pi is in `tiller_ui/**` and `tiller_theme/**`; codex12 is in `tiller/src/main.rs` and
`tiller_control/**`, wiring your P20 command layer right now — expect churn there.

## The piece: `Scripts/ci-linux.sh`

`CLAUDE.md` says the repo has a single verification gate, `Scripts/ci.sh`, which must print `CI OK`
before a PR is opened. **That script shells out to `xcodegen` and cannot run on this platform** — it
gates the Swift/macOS app. This tree is now a Rust workspace targeting Linux, and it has **no gate
at all**. Three agents edit it continuously; the only way anyone knows the tree is healthy is by
running whatever subset they happen to think of.

Build the missing one. It must be a single command that answers one question honestly — *is this
tree healthy right now?* — and it must not lie in either direction.

What it should cover, in the order that fails fastest:

1. `cargo fmt --check` and `cargo clippy` on the workspace. **Report pre-existing drift separately
   from new drift** if the tree is not already clean — a gate that is red on arrival gets ignored
   within a day, which is worse than no gate.
2. `cargo build -p tiller -p tiller_control`. Remember `tillerctl` is a **binary** of
   `tiller_control`; `-p tillerctl` fails and silently leaves a stale binary in place.
3. `cargo test --workspace`.
4. The Python suites: `Scripts/Tests/test-crash-supervise.py` and
   `Scripts/Tests/test-crash-freeze-supervise.py` (8 + 4 tests).
5. **A headless smoke test — the part that makes this gate worth having.** Start the app with no
   display, prove it is alive and serving, and prove a real round trip:

   ```bash
   export TILLER_SOCKET=$(mktemp -u /tmp/tiller-ci-XXXXXX.sock)
   env -u DISPLAY -u WAYLAND_DISPLAY rust/target/debug/tiller &
   rust/target/debug/tillerctl current-workspace          # must return real state
   # panel.create → panel.write a nonce → panel.read → the nonce comes back
   ```

   A generated nonce, not a fixed string. This is what separates "it compiles" from "it runs", and
   this project has learned the difference expensively: a build went green while the app rendered
   nothing at all, twice.

Print `CI OK` on success, mirroring the existing convention. On failure, say **which** stage failed
and show enough output to act on — a gate that just says "failed" sends the reader back to run the
whole thing by hand.

Make it safe to run while three agents are editing: use its own `TILLER_SOCKET` and its own temp
paths, never the shared ones, and clean up its processes when it exits **including on failure**.
Stray app instances and stale sockets have already corrupted other agents' evidence today.

**State clearly, in the script's own header comment, what it does *not* cover:** anything visual.
No display on this machine presents — `:1` died, `:2`'s Xwayland is wedged (alive in `ep_poll` but
`xdpyinfo` from a fresh unrelated client times out, and killing the app on it did not recover it),
and freshly created displays do not present under any driver combination tried. A green `CI OK`
must never be mistaken for "the UI works", because for the foreseeable future nobody can check that.

Then, while you are in there: clear the small warnings you have been walking past, such as
`tiller_usage/src/claude.rs:248` (`variable does not need to be mutable`) — but **only in files you
own or that nobody is editing**, and say which you touched.

## Rules and ownership

- **Never copy code from the reference checkouts** (`../_tiller-refs/{waku,zed,orca,t3code}`).
- **A capability nobody exercised does not exist** — including this script: run it, and paste what
  it printed.
- You own `tiller_terminal/**`, `tiller/src/panes.rs`, and `Scripts/**` that you created. Adding
  `Scripts/ci-linux.sh` is yours. Do not edit `Scripts/ci.sh` — it is the macOS gate and still
  correct for its own target.

## Reporting

Reply in **12 lines or fewer**: what the gate checks, its output pasted verbatim (including `CI OK`
or the failing stage), how long it takes, which warnings you cleared, and the honest remainder —
particularly anything that is red on arrival and why.
