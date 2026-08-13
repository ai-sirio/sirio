# The evidence standard

Cite this file in one line. It governs what may be written in the VERDICT column of
`INVENTORY-LEDGER.md` and `DESIGN-LEDGER.md`.

## The rule

**A verdict made by reading code is not a verdict.**

This is not a style preference. It is the measured result of auditing the whole 146-row `PASSED`
bucket (`PASSED-AUDIT.md`):

| how the row was judged | sample | false verdicts |
| --- | --- | --- |
| by **reading** the source | 48 UI rows | **10 — about 21%** |
| by **executing** a test or socket transcript | 13 machine rows | **0** |

The display outage was priced all day as costing seven display-blocked rows. It cost far more than
that: it quietly forced code-reading verdicts across the entire UI half of the ledger, and every
false `PASSED` found came from there. `VisualTestContext` removes that excuse — a UI row can now be
drawn, clicked and asserted with no display at all.

> A verdict cannot outrun its transcript.

## What reading can and cannot prove (fable, from the audit itself)

The rule above is about `PASSED`. It does not make reading worthless — every one of the ten
overturns was itself a read: a validated grep or a line-read of the live tree. The asymmetry is
the point:

- Reading **cannot prove presence.** Nouns co-existing in a file is not the feature existing —
  that is the conjunction trap below.
- A **validated** read is the only possible disproof of **absence** — you cannot execute a control
  that does not exist. Validated means habit 1 below: the pattern was proven to hit before its
  zero was believed.

The demotion mirror of the one-line test at the bottom: **would my probe have found the feature if
it existed?** A `FAILED — absent` written without that check is as unreplayable as a `PASSED`
written without a test.

## What each tier owes

| tier | the only acceptable proof |
| --- | --- |
| **UI** — anything drawn, clicked, focused, hovered | a **named** test using `TestAppContext` / `VisualTestContext` that draws the element and dispatches the real event |
| **machine** — models, persistence, sockets, git, process control | a **named** test, or an **executed** socket/CLI transcript pasted into the evidence |
| **appearance** — colour, spacing, typography, layout feel | a **screenshot**, listed in `SHOT-LIST.md` |

"Named" means the function name is written in the evidence, so the next reader can run it. Evidence
that paraphrases the clause without naming anything is unreplayable, and unreplayable evidence
cannot support `PASSED` however true it happens to be.

## When you have none of that

Say so. The vocabulary exists precisely for this, and the audit shows the failure is **always** a row
skipping straight to `PASSED`:

- `half-proven` — part of the clause is proven, part is not. Name which part.
- `NOT EXERCISED` — never tried. Add `— blocked on display` only when that is genuinely the reason.
- `builder-claimed, unverified` — a builder says it works. **This is not `PASSED`.** Only the critic
  promotes it, and only by exercising it.

## The conjunction trap, which is how all ten failed

Name it, because it will recur: **verdict by adjacency.**

A VERIFY clause that says "drawn connecting / send / stop states" contains three nouns. Each noun
exists *somewhere* in the file. The verdict then assembles several true facts about several
different things into one verdict about a thing that does not exist — in the founding case
(`F-CHAT-08`) there is a single `↑` control that goes enabled/disabled and never becomes a stop
button.

**Every noun in a conjunction existing somewhere in a file is not evidence that they are the same
control.** That sentence is the whole of the mechanism. When a clause is a conjunction, the proof
must exercise the conjunction, not its members.

The trap has a machine-tier form, and it is where the remaining false `PASSED` hide: a clause
whose machine half has a real transcript and whose final conjunct is UI- or delivery-side.
`F-AUTO-06`'s "confirm a notification is **delivered**" had a live create/list/clear round-trip
and no delivery path at all. A transcript only covers what it exercised — check the conjunct the
transcript never touched.

## Verdicts expire

The ledger drifts in **both** directions, by one mechanism: a row judged at pass N and never
revisited after the build that closed it.

- Inflated `PASSED`: ten confirmed false, whole-bucket estimate ~12–15 of 146 (~8–10%).
- Stale `FAILED`: six `F-GIT` rows still marked absent from pass 5 though P41 built all six;
  `F-SID-14` marked FAILED though the worktree context menu now has a drawn test.

A stale `FAILED` is the same defect as a false `PASSED` with the sign flipped, and it is more
expensive: it sends a builder to build something that already exists. **Before citing any row as a
reason to build, re-check it against the current tree.**

## Two habits that produce false evidence

1. **A negative grep is not evidence of absence until the pattern is proven to match something.**
   `grep -E 'a\|b'` finds a literal pipe in ERE, and returns zero for every term. Six `tiller_git`
   behaviours were nearly reported absent this way while all six were present. Run the pattern
   against a case you know matches, *then* trust the zero.
2. **A narrow grep is not the grep you think you ran.** `grep '"chat\.'` misses doors actually named
   `surface.chat.*`. State the pattern you ran, not the conclusion you drew from it.

## Who may change a verdict

Only the critic, and only by exercising. Builders and auditors **report**; they do not re-mark. A
verdict changed by whoever built the thing is the hole this whole standard exists to close.

Exercising governs **promotions**. A demotion to `FAILED — absent` is applied by validating the
disproof — re-running the probe against the live tree — because absence offers nothing to execute.

## The one-line test, before writing `PASSED`

> Can the next person re-run my evidence from what I wrote, and would it fail if the feature were
> removed?

If no to either, it is not `PASSED`.
