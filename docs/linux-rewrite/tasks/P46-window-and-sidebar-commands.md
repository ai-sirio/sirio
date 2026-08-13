# P46 — Hang the window and sidebar operations off the layer you just built

**You are codex12, pane `w1:p3`.** Your context was just reset, so this brief is everything you
need.

## P43 was reshaped mid-flight and you took the reshape well

You built the command layer first and hung the operations off it: pure tab machinery and typed GPUI
actions for overflow, close, reorder, pane moves and resume; dirty-tab confirmation; retained-chat
transcript restoration. 107 scoped tests. You wrote `P43-tab-command-layer-contract.md` as a handoff
and **did not touch pi's `tab_bar.rs`**, even though the menus you need are drawn there.

And you said this in your report: *"Critic finding agreed: the seven entries shared one missing
command surface."* You checked the critic's root-cause claim against your own reading instead of
inheriting it. That is what makes an agreement worth something.

Your note about `cargo` missing from the gate's environment has been routed to the agent that owns
it — that is a failure which looks real and is not, which is worse than the drift it sits next to.

## Where

```bash
source ~/.cargo/env && cd /home/enzopalmisano/Scrivania/Progetti/tiller-linux
cargo test -p tiller -p tiller_control
./Scripts/ci-linux.sh    # another agent is making this green; check whose failure before acting
```

pi is in `tiller_ui/**` on the chat surface, and has your menu contract queued after it.
codex11 owns everything else. **You own `tiller_control/**`, `tiller/src/main.rs`, project discovery
and `tiller_git/**`.**

## The piece

The critic's root-cause finding named three blocks that were absent for the same reason. You have
now built the thing they were missing. Two blocks remain unhung:

**`F-WIN 02/03/04/05` — window and application shell.** Read `## Window and application shell` in
`01-inventory-app.md`; count the entries yourself. These are the application-level commands: window
lifecycle, the close behaviour, whatever the Swift original drove from its menu bar. Note the
platform difference and state how you resolved it — macOS has an application menu bar that owns
these; Linux conventions differ, and a straight transliteration would be wrong. **The capability is
the contract; the chrome that exposes it is a judgement call, so make it and say why.**

**`F-SID 07/08/09/12/14` — sidebar operations.** Project and worktree context actions: the things a
user reaches by right-clicking a row. The critic counted `F-SID` as 7 PASSED, 2 PARTIAL and 9
FAILED-absent, so check its list rather than only these five.

For each, the same discipline that made P43 work:

- The **transition** is yours. The **affordance that reaches it** is drawn in `tiller_ui/**` — write
  it into the same contract file you already started rather than opening a second channel.
- **Eligibility and disabled reasons must be typed**, reusing the `F-TAB-11` pattern. A context menu
  item that is greyed with no explanation is indistinguishable from a bug, and this codebase has
  produced that shape five times.
- Ask the three questions per entry, in order: **does it exist; can it be exercised; does it do what
  the `VERIFY` clause says.** The mega-row skipped the first, and that is how nineteen entries came
  to be counted as nineteen problems.

## Evidence

Behaviour is provable with no display: `TestAppContext` and `VisualTestContext`, elements by
`.debug_selector(id)`, `simulate_keystrokes`, real mouse events. **Harden every drawn test with the
full `run_until_parked()`** — the critic found unhardened ones pass on timing luck, and an
unhardened test does not count as evidence here.

Where a transition is reachable over the socket, a `tillerctl` transcript is cheaper and just as
good.

## Rules

- **Never copy code from the reference checkouts** (`../_tiller-refs/{waku,zed,orca,t3code}`).
  The Swift original tells you what the command *does*, never how to write it.
- **A capability nobody exercised does not exist**, and *exercised narrowly* is its own failure —
  F-PER-06 was true and one process too narrow; `F-CHAT-24/25` was true of the mechanism and false
  of the feature.
- Proceed without asking for design approval; the acceptance criteria above are the gate.

## Reporting

Reply in **12 lines or fewer**: which entries you built and the evidence, how you resolved the
macOS-menu-bar-versus-Linux question and why, what went into the contract file for pi, where your
reading of what is absent disagreed with the critic's, and the honest remainder.
