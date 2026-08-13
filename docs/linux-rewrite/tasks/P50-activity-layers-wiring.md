# P50 — Three evidence layers are built, tested, and dead. Wire them.

**You are codex11, pane `w1:p2`.** Your context was just reset, so this brief is everything you need.

## The finding

The critic's pass 10 ended with this:

> Layers B, C and D of agent activity detection are complete and fully tested — yet the running app
> wires only Layer A. `handle_title_change`, `detect_content_status` and `refresh_process_signal`
> have zero callers.

I verified it independently and it is **worse** than stated:

- Those three functions have **no non-test callers anywhere in the workspace.**
- `crates/tiller_terminal` has **no title plumbing at all.** `alacritty_terminal` raises
  `Event::Title(String)`; nothing forwards it. `tiller_terminal/src/lib.rs:171` carries a comment
  saying title changes would be distinguished *later*. **Layer B has no source, not merely no
  consumer.**
- The app holds exactly one `AgentActivityModel` (`tiller/src/main.rs:1785`) and only ever feeds it
  spawn events and `tillerctl notify` hooks.

So every agent status a user sees comes from Layer A alone. **An agent Tiller did not launch, or one
whose CLI has no hooks, is invisible** — and that is the entire reason the layered design exists.

This is the sixth instance of this codebase's characteristic failure: **a crate does the right thing
and the surface does something simpler and wrong.** Unit tests cannot see this class of defect; all
three layers have green suites.

## What is already correct, so you do not rebuild it

`tiller_activity` is good work — yours. Do not redesign it:

- `handle_title_change` applies Layer B with the Layer-A debounce window already implemented.
- `content::detect_content_status` matches per-agent prompt conventions.
- `process::inspect_foreground_agent` walks `/proc/<pid>/task/<pid>/children` depth-bounded, with
  fake-`/proc` tests. **The Linux port of Layer D is done** — the Swift original's `proc_listchildpids`
  has no equivalent here and you already solved that.
- Ownership kinds — **spawn-owned, title-owned, process-owned** — are modelled and each has its own
  clearing path.

**Ownership is the part most easily broken by wiring.** A title that stops matching may clear only
title-owned panes; a vanished process may clear only process-owned ones. Wiring that lets one layer
wipe another layer's state reintroduces bugs the model was written to prevent.

## Why this piece and not a larger block

**The queue rule changed today.** A fifth agent, `fable`, was given the design mandate nobody held,
and its verdict is binding: *the ledger stops being the work queue.* Work is now ordered by
**journey** — see `docs/linux-rewrite/05-the-design-of-the-program.md` and `DESIGN-LEDGER.md`.

**J1 is the active journey**, and two of its steps are yours: *"done state on the sidebar row"* and
*"relaunch, the session restores"*. The first is this piece. A sidebar that shows a done state only
for agents Tiller itself launched is J1 failing at its last step, which is why this outranks any
larger block you own.

## The piece

**Yours: `tiller_terminal`, `tiller_activity`, `tiller/src/panes.rs`.**

1. **Give Layer B a source.** Forward `alacritty_terminal`'s `Event::Title` out of the PTY listener as
   something the app can observe, alongside the exit signal you already surface. Delete or fulfil the
   `lib.rs:171` comment; do not leave it saying "later".
2. **Give Layer C a tail.** `detect_content_status` needs pane scrollback text on output-settle.
   `panel.scrollback` already exists over the socket, so the buffer is reachable — settle the output
   first rather than matching on every chunk.
3. **Give Layer D a tick.** A periodic refresh per pane with the pane's shell PID. State the interval
   you chose and why; a poll nobody can justify becomes a poll nobody can change.

**`tiller/src/main.rs` is codex12's file and you must not edit it.** Where the final subscription
belongs there, write it into a contract file — `P50-activity-wiring-contract.md`, same pattern as
`P43-tab-command-layer-contract.md`, which worked. Say exactly what main.rs must subscribe to and call.
Editing outside your ownership is how an hour of somebody's work disappeared today.

## Evidence, and what will not count

The failure mode here is wiring that compiles while the signal never arrives. So:

- **A test that asserts the function is now called is not evidence.** The evidence is end-to-end: a
  PTY that emits a real OSC title sequence (`\x1b]0;✳ idle\x07`), and a pane whose observable status
  changes as a result.
- Layer C: real text through a real PTY, settled, and a status that follows.
- Layer D: a real child process with a matching comm name under the pane's shell, discovered, and the
  status cleared on `processGone` — **not** cleared by an unrelated title change.
- Layer A's debounce still suppresses B: prove the suppression window with two signals, not one.

`TestAppContext` + `VisualTestContext`, `.debug_selector(id)`, `simulate_keystrokes` are available and
no display is needed. **Harden every drawn test with the full `run_until_parked()`** — unhardened ones
pass on timing luck and do not count here.

## Rules

- **Never copy code from the reference checkouts** (`../_tiller-refs/{waku,zed,orca,t3code}`).
  Zed's terminal is the strongest transplant temptation in this piece; the critic audits for it and
  transplanted code counts as a gap, always.
- **Keep the gate green** — `./Scripts/ci-linux.sh`, strict, `-D warnings` on owned crates. You made
  it mean something; do not be the one who costs it.
- **Update every ledger row you close, marked `builder-claimed, unverified`** — never `PASSED`.
- Proceed without asking for design approval.

## Reporting

Reply in **12 lines or fewer**: what each layer's source now is, the end-to-end evidence per layer,
the poll interval and your reason, what went into the contract file for codex12, which ownership rule
was hardest to preserve under wiring, the gate result, and the honest remainder.
