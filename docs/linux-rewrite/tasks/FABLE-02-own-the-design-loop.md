# FABLE-02 — Own the design loop, and write the briefs that carry it

**You are `fable`, pane `w1:pD`.** Your context was reset. This brief is self-contained.

## What FABLE-01 changed, so you can see the mandate is real

You said the plan changes if you conclude it should. Here is what changed within twenty minutes of
your report, so you can calibrate how much weight your next verdict carries:

1. **"The contract does not contain the goal."** This was the finding, and it was right.
   `docs/linux-rewrite/DESIGN-LEDGER.md` now exists: your D-rows as first-class entries —
   `D-SID-01..03`, `D-CMD-01/02`, `D-CHAT-01/02/03`, `D-EMPTY-01/02/03`, `D-CHROME-01/02`,
   `D-TAB-01`, plus `D-J1` and `D-J2` as end-to-end rows. They count toward done exactly as the 388
   do, closed only by the critic, only by exercising them. Kept in a separate file because the critic
   rewrites `INVENTORY-LEDGER.md` wholesale each pass.
2. **The queue rule is adopted.** Work is ordered by journey; J1 is active; the ledger stops being
   the work queue. `codex11`'s current brief (P50) cites J1 and says why it outranks the larger block
   it owns.
3. **The commit is prepared, not requested again.** 289 files / 44,459 insertions staged, message
   written to `.git-checkpoint-msg`, `__pycache__` excluded and gitignored. The operator commits; the
   standing rule here is that I do not commit unasked. You were right that "prepare" was the move.
4. **Your point about eyes is the one I had not seen at all.** Every builder and the critic are
   text-only. Only two participants in this project have ever read a frame, and I spent one of them
   on bookkeeping. That is why design was never judged — not an oversight in scheduling but a
   structural fact nobody had stated.

The one I have **not** acted on is (b), gating dispatch on critic capacity. It is correct that
`pireview` is the rate limiter at 3:1 and that done is critic-gated. I want your read on whether the
answer is fewer builders, a second critic pane, or cheaper verdicts, before I change the topology.

## Your standing job from here

**You own the design loop.** Not advisory review — authorship.

Division of labour, so we never re-task each other's agents (an hour was lost to exactly that today):
**you write briefs, I dispatch them.** One dispatcher, one author per surface. You do not send
prompts to panes and you do not edit `rust/**`.

## The piece: three deliverables

### 1. The J1 briefs for `pi` — the main one

`pi` owns `tiller_ui/**` and `tiller_theme/**`, and every D-row except `D-CMD-01/02` lands there.
Write the J1 work as **self-contained brief files** in `docs/linux-rewrite/tasks/`, named
`D1-*.md`, `D2-*.md` … in the order you want them dispatched. Each must stand alone — every agent's
context is reset before every piece, so a brief that assumes yesterday is a brief that fails.

Write them the way the briefs here are written; read three or four first (`P44`, `P46`, `P49`,
`CRITIC-pass10`). The house pattern: open by naming what the agent's last piece actually did well and
why it mattered; state the piece; state the evidence that will and will not count; state the rules;
cap the report length. **The specificity is the point** — "build the empty states" is not a brief;
"three states, this anatomy, these CTAs, dispatching these real actions, proven this way" is.

Size them so one is a few hours at most, and say which are parallel-safe.

Two things to resolve while you write them:

- **`pi` is mid-piece on P49, the composer** (`F-CHAT-09/10/11/12/14/17`: slash, mentions,
  attachments, chips, overflow, effort). I deliberately held back the card blocks pending your
  verdict. Your `D3` says the absent chat entries are built *into* the composer anatomy rather than
  bolted around it — so say plainly whether P49's six belong inside `D-CHAT-01`'s card, and whether
  anything it is building right now should be reshaped when it lands.
- **`D-CHAT-02` (send becomes stop) and `D-CHAT-03` (queued typing) are `00`'s first two behaviours
  and do not exist.** If they are J1's spine, they may deserve to precede the popover work rather
  than follow it. Order them and say why.

### 2. Reconcile `D2` against what P46 actually landed

Your `D2` supersedes P46's "the chrome that exposes it is a judgement call", and says to reconcile
toward the palette rather than keep both front doors. **Nobody has checked what P46 actually built.**
Read it — `codex12`'s report, `P43-tab-command-layer-contract.md`, the code in `tiller/src/main.rs`
and `tiller_control/**` — and specify the reconciliation as a brief for `codex12`. If P46's front
door already *is* the palette, say so and close the question; if it is a menu-bar strip, say what
gets deleted. **Do not assume it conflicts because your decision says it might.**

### 3. The shot list, for the moment the display returns

The compositor restart is being scheduled rather than standing-requested — your point (c). When it
happens we get **one window with eyes in it**, and every appearance row in `DESIGN-LEDGER.md` plus
the seven display-blocked inventory entries will want evidence at once.

Write `docs/linux-rewrite/SHOT-LIST.md`: the exact application states to capture, in order, each with
what it is evidence *for* and how to reach it (`tillerctl` invocations where the socket can drive it —
see `Scripts/visual-sweep.sh` for what already exists). The failure this prevents is real and cheap
to prevent: discovering after the restart that we photographed the wrong states.

## Constraints

- **You do not edit `rust/**`** and you do not prompt panes.
- **Never copy code from the reference checkouts** (`../_tiller-refs/{waku,zed,orca,t3code}`), and
  your briefs must repeat that rule. Zed is the standing temptation; transplanted code is a gap.
- **Evidence rules bind you.** Appearance claims must be marked as such; nothing newer than probe-2
  (10:25) can be claimed about pixels, because nothing newer exists.
- Behaviour is drawn-testable today: `TestAppContext` + `VisualTestContext`, `.debug_selector(id)`,
  `simulate_keystrokes`, real mouse events, every drawn test hardened with the full
  `run_until_parked()` pump. Your VERIFY clauses should say which harness proves them.

## Reporting

**20 lines or fewer.** Lead with the ordered list of brief filenames you wrote and which are
parallel-safe. Then: the P49 reconciliation, the D2 verdict, and your answer on critic capacity.
