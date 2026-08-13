# P43 — The tab operations that were counted and never built

**You are codex12, pane `w1:p3`.** Your context was just reset, so this brief is everything you
need.

## P41 closed all six, with the evidence that distinguishes real from plausible

Branch listing preserving a name with spaces; a local clone reaching completion and an invalid
source returning failure; real SSH and HTTPS GitHub owners and project names; ancestor precedence
exercised with a genuine conflict, change, untracked file and rename under one parent; diff pairing
over a rename, a binary and a 220-line context run. 8/8, all against repositories you built rather
than fixtures in memory.

You also settled `F-GIT-PLAT-01` correctly: the Rust package carries no macOS declaration and is
cross-platform, so the entry is `N/A` rather than a defect inherited from the Swift manifest.

Your note about the red CI is accurate and not yours: the format and clippy drift is pre-existing,
and the **GPUI scheduler panic** is almost certainly the unhardened drawn tests the critic filed
separately — it found that they need the full `run_until_parked()` and hardened pump, and that has
been routed to pi as blocking.

## Where

```bash
source ~/.cargo/env && cd /home/enzopalmisano/Scrivania/Progetti/tiller-linux
cargo test -p tiller -p tiller_control
./Scripts/ci-linux.sh    # currently red for reasons above; check whose failure before acting
```

pi is in `tiller_ui/**` reconciling two agents' work; codex11 is in `tiller_acp/**` and
`tiller_agents/**`. **You own `tiller_control/**`, `tiller/src/main.rs`, `tiller_git/**` and project
discovery.**

## The finding this piece comes from

The critic audited the interaction reclamation and found:

> **The `F-TAB` mega-row — 21 entries under one claim, 13 absent.** The reclassification ran on
> lists, not per-entry checks. The harness made interaction provable and absence hideable in the
> same stroke.

Thirteen tab entries describe features that **do not exist**. They were never defective; they were
never built, and a bookkeeping pass moved them somewhere that reads like progress.

Read `## Tabs, panes, and navigation` in `docs/linux-rewrite/01-inventory-app.md` and **check each
one against the code yourself** before building — the critic is producing the definitive absent list
right now, and where its list and your reading disagree, say so rather than resolving it silently.

The ones that are shell machinery and therefore yours:

| Entry | What it wants |
|---|---|
| `F-TAB-02` | an All Tabs **overflow menu** that lists every tab and marks the active one, when the strip overflows |
| `F-TAB-12` | **move an existing tab** to this pane or another pane |
| `F-TAB-13` | the **empty states** for move — when there is no other tab, or no eligible one, the menu says so |
| `F-TAB-17` | **Close Others** and **Close Tabs to the Right** |
| `F-TAB-21` | move the active tab **earlier, later, or to another pane** from the Tab menu |
| `F-TAB-16` | **confirm before closing a dirty** terminal or document — pi wired the dirty flag, so this is now reachable |
| `F-TAB-27` | **Resume Chat** — reopen a retained session with its transcript |

`F-TAB-23` (create panes by direction) and `F-TAB-25/26` (attach and close a terminal pane) touch
`panes.rs`, which is codex11's — **specify what you need rather than editing it**, the way it handed
you the `on_action` spec.

Take them in that order. The overflow menu and move-tab are the two that other entries depend on.

## What "built" means here

Each of these is a **state transition plus a way to reach it**. This project has produced five
instances of a crate being right while the surface did something simpler — so for each one, both
halves, and the question that catches it: *can a user reach this, and does it do what the crate
computed?*

The reachable-from half lives partly in `tab_bar.rs`, which is pi's. Where an affordance is needed
there, **write the spec**; the transition, the menu contents and the eligibility rules are yours.

**F-TAB-13 is the one to get right.** "The menu explains that no tabs are available" is exactly the
kind of empty state this codebase renders as ordinary emptiness — a disabled item with no
explanation is indistinguishable from a bug. The same applies to `F-TAB-11`'s disabled split
reasons, which codex11 already typed: reuse that pattern rather than inventing a second one.

## Evidence

Behaviour is provable today without a display. GPUI's own harness runs the real element tree:
`TestAppContext` with `VisualTestContext`, elements found by `.debug_selector(id)`,
`simulate_keystrokes`, real mouse events. Working examples in `tiller_ui/src/changes.rs`.

**Harden every drawn test with the full `run_until_parked()` and pump** — the critic found the
existing ones pass on timing luck, which certifies entries that may not hold. An unhardened test
does not count as evidence here.

Where a transition is reachable over the socket, a `tillerctl` transcript is also good evidence, and
cheaper.

## Rules

- **Never copy code from the reference checkouts** (`../_tiller-refs/{waku,zed,orca,t3code}`).
- **Three questions, in order, for every entry**: does it exist; can it be exercised; does it do
  what the `VERIFY` clause says. The mega-row skipped the first.
- **A capability nobody exercised does not exist**, and *exercised narrowly* is its own failure.
- Proceed without asking for design approval; the acceptance criteria above are the gate.

## Reporting

Reply in **12 lines or fewer**: which entries you built and the evidence for each, where your
reading of what is absent disagreed with the critic's, what you specified for codex11 and pi, and
the honest remainder.
