# P47 — Build from the ledger: the absent features in your crates

**You are codex11, pane `w1:p2`.** Your context was just reset, so this brief is everything you
need.

## P45 gave the gate its meaning back

`CI OK` in 20.27s, on a clean tree. The grandfathering is gone, owned crates build under
`-D warnings`, the gate sources `~/.cargo/env` itself and says so clearly when cargo is missing, and
there are contract tests for the gate itself. The scheduler panic did not reproduce, and you said
that rather than claiming you had fixed it.

Two lines in your report are the reason this piece worked:

> **No warnings suppressed; exclusions are ownership boundaries.**

An `#[allow]` would have produced the same green output and meant nothing. And itemising what
remains **per owner** — `tiller_ui` 6 lib, 1 example, 20 test; `tiller` 7 in `main.rs` — turns a
collective nuisance everyone mentions into two specific pieces of work with names on them. Both have
been routed.

## The state of the project, which is now knowable

The critic built `docs/linux-rewrite/INVENTORY-LEDGER.md`: one row per entry for all 388, with
verdict, evidence and the pass that judged it.

| verdict | count |
|---|---|
| PASSED | 88 |
| half-proven | 46 |
| **FAILED — absent** | **90** |
| FAILED — defective | 4 |
| UNREACHABLE | 9 |
| N/A — platform | 17 |
| **NOT EXERCISED** | **127** |
| NOT EXERCISED — blocked on display | **7** |

**The display gates seven entries** — 1.8% of the contract. It dominated the day and it is not the
wall. The wall is **90 features that were never built** and **127 nobody has ever exercised.**

The critic is converting the 127 into verdicts. **Your job is the 90.**

## The piece

**Read the ledger, take the `FAILED — absent` rows that fall in crates you own, and build them.**

Yours: `tiller_terminal`, `tiller/src/panes.rs`, `tiller_activity`, `tiller_persistence`,
`tiller_project`, `tiller_usage`, `tiller_markdown`, `tiller_acp`, `tiller_agents`, and the
`Scripts/` you wrote. **Not** `tiller_ui/**` (pi, on the chat surface and its own warnings), and
**not** `tiller/src/main.rs`, `tiller_control/**` or `tiller_git/**` (codex12, on window and sidebar
commands).

Choose the largest coherent block you own rather than picking one entry from each corner — a group
that shares machinery gets built once and verified together. **Say in your reply which rows you
selected and why**, so the next piece can start where you stopped rather than re-deriving the
choice.

For each entry you build, all three questions must end up answered:

1. It exists now.
2. It is exercisable here — and if the only route to a user is a surface you do not own, **say what
   that surface needs** rather than declaring the entry done. Nineteen entries were counted as
   built-but-unreachable before anyone noticed they shared one missing menu layer.
3. It does what the `VERIFY` clause says, not what it plausibly does.

**Update the ledger row when you finish one** — but mark it `builder-claimed, unverified`, never
`PASSED`. Only the critic's verdict counts toward done, and the two must stay visibly distinct: that
distinction is what caught the mega-row, the too-narrow `F-PER-06` proof, and the `F-CHAT-24/25`
mechanism-versus-feature confusion.

## Rules

- **Never copy code from the reference checkouts** (`../_tiller-refs/{waku,zed,orca,t3code}`).
  The Swift original named in each `SRC:` field is the reference for **behaviour** only.
- **A capability nobody exercised does not exist**, and *exercised narrowly* is its own failure.
- **Keep the gate green.** It is strict now; a warning you introduce is a red gate for four agents.
- Proceed without asking for design approval.

## Reporting

Reply in **12 lines or fewer**: which ledger rows you selected and why, what you built, the evidence
per entry, what you need from surfaces you do not own, the gate result, and the honest remainder.
