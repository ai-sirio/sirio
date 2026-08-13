# FABLE-03 — 146 rows say PASSED. One is known false. Audit the bucket.

**You are `fable`, pane `w1:pD`.** Your context was reset. This brief is self-contained.

## FABLE-02 landed everything, and one line of it outranks the rest

Six briefs with a defended dispatch order — `D1` stop-and-queue first because `D-CHAT-02/03` are the
first two behaviours the inventory lists, `D6` strictly last because a chrome count cannot stabilise
before the surfaces it counts. `SHOT-LIST.md` with a Phase-0 abort gate, so a dead display fails fast
instead of consuming another 1h26m. A `D2` verdict that **closed** the question rather than
manufacturing a conflict from your own decision: *"P46's chords, two-control title strip and typed
context menus are not a menu bar; reconciliation is completion, not correction."* You went and looked.

Your answer on critic capacity is adopted whole — keep 3:1, make verdicts **cheaper**: a claimed row
must name a **replayable proof** so `pireview` replays rather than re-derives. That is now in every
brief this project issues.

And then this:

> Flagged: `F-CHAT-08`'s PASSED is false — no stop glyph exists in `chat.rs`.

**Verified independently. You are right, and the mechanism is worse than a missing glyph.**

`chat.rs:2120-2160`: the composer's primary control is a single `↑` whose only variation is
`can_send` — a background and text colour. `can_send` is `!streaming && !connecting && !empty`, so
**while a turn streams the control is merely disabled.** It never becomes stop. The VERIFY clause
asks for three states; two exist, and neither is one of the three.

The mechanism has a name now — **verdict by adjacency.** Every noun in that VERIFY clause is present
*somewhere* in the file: a `connecting` flag with a status pill that renders the word, a `cancel_turn`
that genuinely cancels, Escape bound to `Cancel`. Three true facts about three different things were
assembled into one verdict about a control that has never existed. The components were each checked;
that they were the *same* component was assumed.

## The piece

**146 rows are PASSED. Every claim this project makes about its progress rests on that bucket, and
nobody has ever audited it** — by construction, because the critic cannot audit its own verdicts and
no one else had the mandate. One row in it is now known false, found by an agent that was not even
looking.

**Audit it. Find the false PASSEDs.**

You are not re-running the critic's work and you must not try to re-verify 146 rows by exercising
them; you would not finish, and that is not where the yield is. **Hunt the mechanism.** Verdict by
adjacency leaves a signature, and you can read it from the ledger and the tree:

- The VERIFY clause is a **conjunction** — *"changes between loading, send and stop"*, *"A and then
  B"*, *"and confirm X, Y and Z"* — while the evidence column names a single artefact. `F-CHAT-08`'s
  evidence was six words for a three-state claim.
- The evidence column **paraphrases the VERIFY clause** instead of naming a test, a transcript or a
  measurement. A row that says *"drawn connecting/send/stop states"* asserts the conclusion; a row
  that says `chat_composer_stop_state_test` can be replayed.
- The VERIFY clause is about **one control, one row, one surface** — a *single thing having several
  states* — which is precisely the shape where adjacency fools a reader. Contrast a row asserting
  that a function returns a value, which is hard to get wrong.
- **Cheap disproof exists**: for a claim about a control's states, grep the surface for the states.
  Prefer these. A finding you can establish in two minutes and state with a file and a line is worth
  more here than a suspicion you spent twenty on.

Work in **descending order of consequence, not ledger order**: rows on J1's spine first (`D-J1`'s
thirteen steps), then the human-facing surfaces where absent rows cluster (F-CHAT, F-TAB, F-SID,
F-EDIT), then the machine-facing families where PASSED concentrates (F-CTRL, F-AUTO, F-AGENT) —
those were verified over a socket with real transcripts and are the least likely to be adjacency
verdicts. **Say what you did not reach**; a bounded audit that states its bound is evidence, and one
that implies completeness it does not have would repeat the very failure you are hunting.

## What a finding must contain

For each row you overturn or doubt: **the id, the VERIFY clause's actual demand, what exists at
`file:line`, and which of the two it is** —

- **false PASSED** — the feature does not do what the clause says. Give the cheap disproof.
- **unreplayable PASSED** — it may well be true, but the evidence cannot be replayed by anyone. This
  is not an accusation; it is a debt, and it is what the replayable-proof rule exists to stop
  accruing.

Distinguishing those two is the whole value of the piece. Do not inflate the first with the second.

**Then write the totals.** How many rows you examined, how many fell, how many are unreplayable, and
**your honest estimate of the false rate in the bucket you did not examine** — with the reasoning that
produced the estimate. That number is what the project's completion claim is actually worth, and
right now nobody can state it.

## Constraints

- **You do not edit `rust/**`, you do not prompt panes, and you do not edit `INVENTORY-LEDGER.md`** —
  `pireview` rewrites it wholesale each pass and is mid-pass now. Report your overturns; the critic
  applies them, because a verdict changed by a non-critic is exactly the hole this project has been
  closing all day. Write your findings to `docs/linux-rewrite/PASSED-AUDIT.md`.
- **Evidence rules bind you.** Failing to find something is not proof of absence — say which you
  established. Appearance claims stay marked; nothing newer than probe-2 (10:25) exists to read.
- **Never copy code from the reference checkouts** (`../_tiller-refs/{waku,zed,orca,t3code}`).

## Reporting

**20 lines or fewer.** Lead with the count of false PASSEDs and the ids. Then the unreplayable
count, the families you did not reach, and your estimated false rate for the remainder with its
reasoning.
