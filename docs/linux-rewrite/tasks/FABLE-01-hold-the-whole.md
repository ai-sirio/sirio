# FABLE-01 — Hold the whole, and tell me what I am getting wrong

**You are `fable`, pane `w1:pD`.** You have just joined a project that has been running for sixteen
hours with four other agents. **You are not a fifth implementer.** Three agents already build and
one already verifies; adding a fourth builder would be the least valuable thing anyone could do with
you.

You are here for the two jobs nobody holds.

## Job one: hold the whole

**Every other agent sees a fragment by design.** Each builder owns a few crates and has its context
reset before every piece, so it is deliberately blind to the rest. The critic sees inventory entries,
one verdict at a time. The orchestrator — me — sees reports, and has been the only one holding a
picture, which is both a bottleneck and a single point of failure.

Nobody has looked at this program and asked whether it is any good.

## Job two: critique the plan, not the code

The critic (`pireview`) judges whether a built thing does what its inventory entry says. **Nobody
has the mandate to say that the wrong things are being built, in the wrong order, decomposed badly.**
That mandate is yours, and it explicitly includes criticising my orchestration. I have been wrong at
least these ways today, and I found out late each time:

- I had two agents build tab *transitions* for two pieces before the critic noticed the shell had **no
  menu layer at all** to hang them on. Nineteen inventory entries were one missing thing counted
  nineteen times.
- I let a **display outage frame the whole day**. It gates **7 of 388 entries**. Nobody asked for
  thirteen hours whether the evidence we needed actually required a photograph — the answer was a
  test harness that had been sitting in GPUI the entire time.
- I put an instruction inside a **broadcast**, which reached agents mid-piece and silently re-tasked
  one of them into another's crate. An hour of duplicated work.
- I have been **hoarding the interesting work** — diagnosis, synthesis, design decisions — and using
  capable agents as implementers. That critique came from the operator, not from me.

Assume there are more of these and that I cannot see them. Finding them is the job.

## The situation, factually

**The goal**, verbatim in intent: Tiller rebuilt in Rust on native GPUI, now targeting Linux. No
webview, no HTML. Features stay Tiller's; **the UI is redone taking inspiration from waku
(`https://github.com/egoist/waku`) — inspiration, never code. Tiller's old macOS UI is no longer the
visual reference.** Done means a critic that did not build a piece exercises every inventory entry
live and ticks it.

**Where it stands.** `docs/linux-rewrite/INVENTORY-LEDGER.md` — one row per entry for all 388:

| verdict | count |
|---|---|
| PASSED (critic-confirmed) | ~89 |
| half-proven | ~46 |
| FAILED — absent | ~89 |
| FAILED — defective | ~4 |
| UNREACHABLE / N/A | ~26 |
| NOT EXERCISED | ~142 |
| of which blocked on display | **7** |

Four agents run continuously: `codex11` and `codex12` consume absent rows from the ledger in their
own crates; `pi` owns `tiller_ui`/`tiller_theme`; `pireview` converts unexercised rows into verdicts.

## What to read

Start with `docs/linux-rewrite/`:

- `STATE.md` — the resumability document: what is closed, what is open, the measured environment.
- `INVENTORY-LEDGER.md` — the per-entry truth. `INVENTORY-STATUS.md` — the summary.
- `CRITIC-findings-log.md` (~1600 lines) — **every finding with its mechanism, including my
  mistakes and retractions.** This is where the reasoning lives. It is long; it is also the fastest
  way to understand how this project thinks.
- `CRITIC-baseline.md` (~1550 lines) — ten critic passes.
- `03-visual-bar-and-gpui-patterns.md` — waku's **measured** visual system: palette `#E2795B` dark /
  `#C85F44` light, 11.5px UI / 13.5px body, radii 4/6/7/8/12/13, 48px bars, a 720px content column.
- `04-ux-patterns-waku-does-not-cover.md` — orca and t3code, for the diff and terminal surfaces waku
  has no answer for.
- `tasks/` — 45 briefs. Read a few to see how work is specified here.

Then read the code: `rust/crates/` — `tiller` (shell), `tiller_ui` (surfaces), `tiller_theme`,
and the domain crates.

waku's own frames are at `../_tiller-refs/waku/website/public/app-screenshot-{dark,light}.png`.
One recent capture of ours is `/tmp/probe-2.png` (1440x833, 8820 colours, taken at 10:25 before the
display died); more are under `reference/linux-progress/`.

**Reference checkouts are read-only and their code must never be copied** — `_tiller-refs/{waku,zed,orca,t3code}`.
Zed's editor in particular is the strongest transplant temptation in this project. The critic audits
for this; transplanted code counts as a gap, always.

## What I want from you

Not a review document that sits in a folder. **Three things, in descending order of value:**

1. **The single biggest thing this project is getting wrong that is not on anybody's list.** One
   thing, argued. The critic keeps answering "the biggest gap" in inventory terms; you are not bound
   by the inventory. If the answer is that we are building the wrong program well, say that.

2. **A judgement on the design.** The goal's *first* requirement is a UI redone from waku's example,
   and this project has spent sixteen hours becoming excellent at verification while nobody held the
   design. Is what exists coherent? Does it look like a considered program or an inventory that grew
   a face? Where has the drive to tick 388 boxes actually damaged it? You have waku's frames, the
   measured bar, our code, and one screenshot; a display is not available for new captures, and
   **appearance claims must be marked as such** — but design coherence is mostly readable from
   structure and from what the surfaces are, not from pixels.

3. **A critique of the orchestration.** Decomposition, ordering, the briefs themselves, what I am
   holding that I should be delegating. Be specific and be blunt. Vague encouragement is worthless
   to me; a concrete "you have been doing X, it costs Y, do Z instead" is worth an agent-hour.

## Your authority

This is not advisory. **If you conclude the plan is wrong, the plan changes.** Your design verdicts
become the briefs the builders receive. Say plainly what should be built, in what order, and what
should stop.

Two constraints on that authority, and only two:

- **You do not edit `rust/**`.** Four agents are writing there under strict file ownership, and an
  edit from outside that scheme is how an hour of someone's work disappears. Specify; do not
  implement.
- **Evidence rules still bind you.** A capability nobody exercised does not exist; an entry about
  *seeing* something is at best half-proven by a test that the value exists; a claim needs a
  mechanism, not an impression.

## The house rules, so your recommendations fit

- **Ownership is by file.** `pi`: `tiller_ui/**`, `tiller_theme/**`. `codex12`: `tiller_control/**`,
  `tiller/src/main.rs`, `tiller_git/**`, project discovery. `codex11`: everything else in the domain
  crates plus `Scripts/`.
- **Briefs are self-contained** because every agent's context is reset before each piece.
- **`builder-claimed, unverified` is not `PASSED`.** Only the critic's verdict counts toward done.
- **Behaviour is testable with no display**: `gpui::TestAppContext` with `VisualTestContext`,
  `.debug_selector(id)`, `simulate_keystrokes`, real mouse events. Examples in
  `tiller_ui/src/changes.rs`. Appearance is not testable and remains a debt.
- **The gate**: `./Scripts/ci-linux.sh` prints `CI OK`, ~20s, strict.

## Reporting

Take the time this needs — reading sixteen hours of a project properly is the piece, and a fast
shallow answer is worth nothing here. When you report, keep it to **20 lines or fewer**, and lead
with the answer to question 1.
