# P51 — J1 cannot be exercised at all. Give it a spine.

**You are codex12, pane `w1:p3`.** Your context was just reset, so this brief is everything you need.

## P48 closed the block and took the routed handoff with it

The Linux window command block, and the terminal title/split/close subscriptions codex11 handed off —
for new, split, restored **and clicked** panes, which is the set that actually covers a session rather
than the set that makes a test pass. Stable pane/terminal identities with routing tests. Contract file
and ledger claims updated. `cargo fmt --check`, clippy, sequential tests and the full
`Scripts/ci-linux.sh`: **CI OK**.

Two lines in your report earned this next piece:

> *"Linux remains keyboard-first; no macOS menu-bar transliteration."*

You made the platform judgement and stated it, rather than transliterating a menu bar nobody on this
platform expects. **That decision has since been ratified as design decision `D2`** — commands live in
a palette, there is no menu-bar strip — so the front door you chose is now the specified one, and
nothing you built needs reconciling.

> *"Entries never independently judged by the critic: 122 (82 builder-claimed, unverified; 40 never
> claimed by anyone)."*

You computed a number that separates *nobody has looked* from *only its author has looked*. Those are
different debts with different owners, and nobody had split them.

## The plan changed at 15:20. Read this before you pick anything up

A fifth agent, `fable`, was given the design mandate nobody held, and its verdict was that **the
contract did not contain the goal**: "done = 388 ticks" quietly dropped the goal's first requirement,
a UI redone from waku's example. Design now has rows — `docs/linux-rewrite/DESIGN-LEDGER.md` — and
decisions, in `05-the-design-of-the-program.md` (D1–D8, J1/J2). Read both; they are short.

**The queue rule changed with it.** Work is ordered by *journey*, not by the largest coherent block
you own. **J1 is the active journey:**

> launch clean → empty state → add project → sidebar → open worktree → empty state → start a chat →
> composer → streamed turn with tool and permission cards → **stop it** → type mid-turn and see it
> queue → done state on the sidebar row → relaunch, the session restores.

`D-J1` is a ledger row now, and the critic must exercise it **end to end in one run**. That is the
integration evidence per-entry ticks cannot give, and the only check that catches this codebase's
characteristic failure — *a crate does the right thing and the surface does something simpler and
wrong* — **between** entries rather than inside one.

## The piece

**The critic cannot run J1. There is no route to most of it.**

The socket exposes 50 methods. Grouped, they are: `panel.*` and `pane.*` (terminals),
`surface.changes.*` (J2's spine, largely there), `surface.settings.*`, `workspace.*`, `tab.*`,
`notification.*`, `system.*`, `session.*`, `browser.*`.

**There is no `project.*` and no `chat.*` anything.** So a headless critic cannot add a project,
start a chat, send a message, observe a streaming turn, stop one, queue typing during one, or read a
transcript back. Eight of J1's thirteen steps have no door.

**Build J1's spine in `tiller_control` and `tiller/src/main.rs`.** Work the journey in order and stop
where you run out of piece; say where you stopped. Name the methods to match the existing scheme
(`surface.*` for a surface, dotted, kebab-free) rather than inventing a second convention — the CLI
mirrors these names and a second scheme costs twice.

What each door must expose is set by what the critic has to *observe*, not by what is convenient to
send: a turn that is streaming must be distinguishable from one that has finished, a stopped turn from
a completed one, queued text from sent text. **A door that can drive a step but not observe its result
leaves the entry unexercisable, which is the whole defect this piece exists to remove.**

`D8` limits new socket surface to *what verification consumes* until J1 is critic-green. This piece is
that, and nothing beyond it.

## Ownership, and the one thing that is not yours

**Yours: `tiller_control/**`, `tiller/src/main.rs`, `tiller_git/**`, project discovery.**

The chat surface itself is `pi`'s and it is being rebuilt around the composer anatomy (`D-CHAT-01/02/03`).
**Do not draw anything.** Where a door needs state the surface owns, write it into the contract file
you already keep rather than reaching into `tiller_ui/**`. That file has worked twice now.

`codex11` is on P50, wiring three dead evidence layers — Layers B/C/D of agent activity detection are
built, tested, and have zero non-test callers. Its contract file will ask `main.rs` to subscribe. Expect
it; do not pre-build it.

## Evidence

A `tillerctl` transcript is the natural evidence here and it is as good as a drawn test when the
transition is genuinely reachable over the socket. **Show the observation, not just the call** — the
transcript must contain the state that proves the step happened, and for the stop and queue steps it
must show the state *before* and *after*.

`TestAppContext` + `VisualTestContext` remain available for anything that needs the element tree.
**Harden every drawn test with the full `run_until_parked()`** — unhardened ones pass on timing luck.

## Rules

- **Never copy code from the reference checkouts** (`../_tiller-refs/{waku,zed,orca,t3code}`).
- **Keep the gate green** — strict, `-D warnings` on owned crates; a warning you add is a red gate
  for four agents.
- **Update ledger rows as `builder-claimed, unverified`** — never `PASSED`.
- Proceed without asking for design approval.

## Reporting

**12 lines or fewer**: the methods you added and why each is named what it is, which J1 steps are now
drivable *and observable*, which are still doorless and what they need, what went into the contract
file, the gate result, and the honest remainder.
