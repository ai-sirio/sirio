# P44 — The chat surface: the centrepiece, unexercised since 01:00

**You are pi, pane `w1:p4`.** Your context was just reset, so this brief is everything you need.
Your own notes are in `docs/linux-rewrite/PI-HANDOFF.md`.

## The reconciliation went the way it needed to

*"codex11's work reconciled, not reverted — one implementation per feature, no duplicates, suite
stable 3×."* You took a crate that two agents had been writing into for half an hour and produced
one coherent result, keeping what was better from either side. And you ran the suite **three times**
rather than once, which is the right answer to a test-health finding about timing luck: a suite that
passes once has told you less than you think.

You also updated the audit tables **per entry** with the present-versus-reachable rule, which is
exactly what the mega-row got wrong.

The overlap was my fault: I put an instruction in a broadcast, and broadcasts reach agents
mid-piece. Facts belong in broadcasts, work belongs in briefs.

## Where

```bash
source ~/.cargo/env && cd /home/enzopalmisano/Scrivania/Progetti/tiller-linux
cargo test -p tiller_ui -p tiller_theme
./Scripts/ci-linux.sh   # may be red from another agent's in-flight crate; check whose before acting
```

codex11 is in `tiller_acp/**` and `tiller_agents/**` — the ACP **protocol** tier. codex12 is in
`tiller/src/main.rs`, `tiller_control/**` and `tiller_git/**`, building the absent tab operations
and will send you specs for the affordances that live in `tab_bar.rs`.
**`tiller_ui/**` and `tiller_theme/**` are yours.**

## The piece

**The chat surface has not been exercised since about 01:00**, and it is the centre of this
application — waku is a chat program, the visual bar was measured from one, and Tiller's whole
purpose is conversing with agents.

That verdict is now the oldest surviving claim in the project. Since it was recorded: the shell was
rebuilt, panes gained a command layer, the session store changed shape twice, process lifetimes were
fixed, Changes was mounted, an editor appeared, and the control socket grew from six methods to
several dozen. **A PASSED verdict from thirteen hours ago is a hypothesis with a timestamp.**

Exercise `tiller_ui/src/chat.rs` with the drawn-frame harness — `TestAppContext`,
`VisualTestContext`, `.debug_selector(id)`, `simulate_keystrokes` — against
`## Chat and composer` in `docs/linux-rewrite/01-inventory-app.md`. Count the entries yourself.

Ask the three questions in order, per entry, because this is exactly where the mega-row went wrong:

1. **Does it exist at all?** If not: `FAILED — absent`.
2. Can it be exercised here?
3. Does it do what the `VERIFY` clause says — not what it plausibly does?

Where the substance is behaviour, prove it in a drawn frame: typing in the composer, Enter sending,
Shift-Enter not sending, the transcript growing as a reply streams, a tool call rendering, a
permission prompt appearing and **both** of its answers working, cancellation, and the empty state
before any message exists.

Where the substance is appearance — the coral accent, type scale, bubble geometry, the comparison
against waku — it stays `NOT EXERCISED — blocked on display` and stays a debt.

**Harden every drawn test** with the full `run_until_parked()` and pump. The critic found the
existing ones can pass on timing luck, and an unhardened test does not count as evidence here — not
"is weaker", does not count.

### Also: four entries the audit missed, already reachable

The critic looked in the direction nobody had an incentive to look and found these are provable
today: **`F-SET-03`**'s update click, **`F-CHG-20`**'s running count, **`F-TAB-15`**'s close
control, and **`F-SET-21`**, whose behaviour half already has a drawn test. Free wins someone had
already earned — pick them up.

## A note on the seam

codex11 is exercising the ACP protocol underneath you: connection, streaming, tool calls,
permissions, and the failure paths — a dying agent, a malformed frame, a denied permission, a
cancelled request. **Your half is what the surface does with each of those.** A stream that stops
mid-reply, a denied permission, a cancelled request: each must produce a stated state in the
transcript, not an ordinary-looking empty one. That is the failure this codebase has produced five
times, and the chat transcript is the surface where a user would notice it least.

If you need something from the protocol tier to express a state, say what and it will be routed.

## Rules

- **Never copy code from the reference checkouts** (`../_tiller-refs/{waku,zed,orca,t3code}`).
  waku is a chat program and the strongest visual reference here; take the ideas, write the lines.
- **A capability nobody exercised does not exist**, and *exercised narrowly* is its own failure.
- Proceed without asking for design approval; the acceptance criteria above are the gate.

## Reporting

Reply in **12 lines or fewer**: how many chat entries you judged and the counts, what the surface
does when a stream dies mid-reply and when a permission is denied, whether the four missed entries
are closed, what you need from the protocol tier, and the honest remainder.
