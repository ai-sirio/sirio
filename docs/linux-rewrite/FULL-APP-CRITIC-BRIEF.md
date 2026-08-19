# The full-app critic pass — the goal's actual finish line

The contract says: *"Hai finito solo quando il critico full-app spunta ogni voce dell'inventario
esercitandola dal vivo."* Row-by-row waves got the ledger to 376/389, but **no single critic has
ever driven the whole inventory in one continuous app session**. That pass has not been run. This
file is its brief, written while the disk blocked all building, so it can be launched immediately
once `~/FIX-SYSLOG-FLOOD.sh` has run and `cargo build --workspace` is green.

## Why this pass is not "the waves again"

The waves proved rows in isolation, each in its own app instance, often with a fixture built for
that row. That is the right way to find defects and the wrong way to answer the goal's question,
which is whether *the assembled app* works. Three classes of failure are invisible to per-row
drives and are exactly what this pass is for:

- **Interaction between features.** A row proven with one worktree open says nothing about the same
  control with four projects, twelve tabs, and two agents streaming.
- **State that only accumulates over a long session.** Every wave restarted the app freely; the
  documented trap is that a second `wayland-drive.sh` invocation under the same label kills and
  relaunches it. A real user does not restart between actions.
- **Order dependence.** Rows were driven in whatever order suited the agent. A user does them in
  the order the UI suggests.

## Shape

Not one agent. One agent cannot hold 389 rows and a live session, and a killed agent would lose the
whole pass. Use a **pipeline over inventory sections**, each stage a fresh critic in its own lane,
with a final synthesis:

- Split the inventory by section (`F-SID`, `F-TAB`, `F-CHAT`, `F-TERM`, `F-CHG`, `F-SET`, `F-PRJ`,
  `F-CORE-*`, `F-AGENT-*`, `F-GIT`, `F-WIN`, `F-EDIT`, `F-BRW`).
- Each critic drives **its whole section in one uninterrupted `wayland-drive.sh` invocation**, in
  the order a user would meet the features, on an app instance it started itself and never
  restarts. That single-invocation rule is the point of the exercise, not an implementation detail.
- Concurrency ceiling is **about 5 lane-driving agents**, measured — not the 10 an earlier note
  claimed. Ten took this box to load 41 with 232 MB of swap left.
- A final agent reads every section report and answers the one question the goal asks: *does any
  inventory item remain unexercised, and what is the single largest gap?*

## What every critic must be told

Reuse the standing brief text used by waves N–Q (see any `CRITIC-wave*.md` for the exact wording).
The clauses that earned their place, each paid for in lost hours:

- **A feature nobody drove live does not exist.** Every verdict needs a hard discriminator — a PID,
  bytes on disk, a row in SQLite, a control-socket read. "The screenshot looks right" is not one.
- **A green test is not a passed row.** Six pieces of this port compiled, passed tests, and were
  never reached by the app. Grep for the symbol outside its defining crate before believing a test.
- **Cover the whole clause.** A row is one sentence with several clauses joined by "and".
- **`shot` forces a real window resize** and a resize moves things — never mid-gesture.
- **Context menus need a real sleep** between the right-click and the item click.
- **Check the virtual keyboard is alive** (`swaymsg -t get_inputs`, then a `key z` read back through
  `panel.scrollback`) before blaming a chord.
- **Native file dialogs need `XDG_CURRENT_DESKTOP` and `WAYLAND_DISPLAY`** pushed with
  `dbus-update-activation-environment` *before* the first portal-triggering click.
- **Verdict vocabulary is closed**: `PASSED`, `FAILED — defective`, `FAILED — absent`,
  `half-proven`, `UNREACHABLE`, `NOT EXERCISED`, `N/A — platform`. No qualifiers appended.
- **Commit after every section**, paths enumerated, never `git add -A`. Assume you will be killed.

## What it must check that the row-by-row waves could not

1. **The visual bar.** The goal froze reference screenshots (waku, comet; orca and t3code for
   uncovered patterns). No wave ever compared the assembled app against them. This pass must, and
   must say plainly where the port looks worse.
2. **No transplanted code.** The goal makes copied code from a reference *app* a gap — inspiration
   only. `longbridge/gpui-component` is exempt (code may be taken freely, even as a dependency) and
   `beautifului.dev` is a spec to translate. A reviewer should sample the UI crates for verbatim
   blocks from waku/comet/orca/t3code.
3. **Platform gating.** OS-touching API outside a `cfg`-gated module is a gap by the goal's own
   words. One was found this way already — a hardcoded `/bin/zsh` shell fallback with no gate at
   all, which meant a pane with `$SHELL` unset opened nothing on Linux. Sweep for others:
   `grep -rn '/bin/\|/usr/\|~/Library\|AppData' rust/crates --include='*.rs'` and check each hit
   sits behind a `cfg`.
4. **A real ACP harness.** The goal names it explicitly: connect a real agent, send messages, verify
   streaming and responses. `chat_fixture.py` is a scriptable ACP v1 stand-in; the real `claude`,
   `codex`, `opencode` and `pi` CLIs are installed. `oh-my-pi` is not usable — an upstream packaging
   defect confirmed across every published version.

## Entering it with an honest starting point

Three rows are not PASSED and the critic should not be told otherwise:

- `F-CORE-WSP-05` — divider steals focus. Diagnosed with the patch written out in
  `FIX-PLAN-wsp05-divider-focus.md`; fix it before this pass, or the critic will rediscover it.
- `F-TERM-PTY-04` and `F-GIT-RUN-01` — both fixed by the orchestrator and held at half-proven,
  because a builder does not pass its own work. Each names the drive it owes.

And three are blocked upstream (`F-AGENT-OMP-01/02/03`): every published `oh-my-pi` version either
declares no `bin` at all or ships an entry point that neither node nor bun can parse and that
imports source files absent from the tarball. Do not spend a pass on them.
