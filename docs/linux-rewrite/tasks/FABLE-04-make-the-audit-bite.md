# FABLE-04 — Make the audit bite

**You are fable, pane `w1:pD`.** Your context was just reset. `PASSED-AUDIT.md` is yours from an
hour ago; re-read it, it is the input to this piece.

## What you produced, and why it is the most useful number today

You did not just find ten bad rows. You found the **mechanism's habitat**, and that is the part that
generalises:

> the adjacency mechanism needs a display-blocked critic reasoning from code presence — it operated
> on UI rows (observed false rate 10/48 ≈ 21%). The machine rows were verified by executing the
> protocol or named tests, where **a verdict cannot outrun its transcript**.

That splits the ledger cleanly by *how a row was judged* rather than by what it covers. Where
evidence was executed, the false rate is ~0 (13/13 sample real). Where it was read, it is 21%. So
the display outage did not cost this project seven display-blocked rows, which is how we have been
pricing it all day — **it silently degraded the epistemics of every UI verdict**, and the ledger's
headline number carries an ~8–10% discount because of it.

Three things follow, and they are the piece.

## 1. Make the overturns change what gets built (the urgent half)

Five of the ten sit on J1's chat segment — `F-CHAT-02/08/16/18/23` — and `pi` is queued onto exactly
that ground: P49 now, then **D1**, then **D3**, both of which you wrote. Right now those briefs were
written believing those rows had passed.

**Fold the overturns into your own D briefs** so the builder builds the real hole:

- `D1-stop-and-queue.md` — F-CHAT-08 is the founding case and already yours. Add F-CHAT-02: there is
  no auth state anywhere, only a generic retryable connection-error banner. Decide whether an auth
  banner belongs in D1's scope or is a separate row, and say which.
- `D3-composer-one-card.md` — F-CHAT-16 (no model search, no no-match, no "Recommended"), F-CHAT-18
  (popover has percent/used/size/cost, and no input-output-cache breakdown), F-CHAT-23 (tool cards
  carry `{id,title,status}` and have no click, no expansion, no links, no Dismiss). F-CHAT-23 is
  corroborated by F-CHAT-21/22 already FAILED for the same missing machinery — that is one build,
  not three.

Do not re-scope the briefs around the whole audit. Add the holes, keep the journey order.

`F-SET-16/21`, `F-SID-15`, `F-USE-02/03` are outside the J1 chat leg — leave them to the ledger and
the queue; noting where they *should* eventually land is enough.

## 2. The evidence standard — ALREADY WRITTEN, do not write it again

`docs/linux-rewrite/EVIDENCE-STANDARD.md` exists. This part of the brief was reassigned and
discharged by the orchestrator while Fable 5 was returning API 529 on every attempt for ~50 minutes;
the standard was blocking four agents and could not wait. It is built from your finding and quotes
your sentence — *a verdict cannot outrun its transcript.*

**Read it, and say where you disagree.** You are the one person who audited all 146 rows, so a
correction from you carries more than the file does. But do not rewrite it wholesale: pi, codex11,
codex12 and pireview are already citing it.

## 3. The bounded sweep you did not reach

You estimated 2–5 false rows in the unexamined 98 and named where they hide: *rows whose clause
smuggles a UI-side conjunct — the F-AUTO-06 pattern.* That is a cheap, targeted pass, not a re-audit.
Find the machine-tier rows whose VERIFY clause has a UI-side or delivery-side conjunct, and check
only that conjunct. Report them the same way, with `file:line`.

If it runs long, **1 and 2 come first.** They change what four agents do next; 3 only sharpens a
number you have already estimated honestly.

## Rules

- **Report your overturns; the critic applies them.** Unchanged and load-bearing — a verdict changed
  by a non-critic is exactly the hole this project has been closing all day. You may edit **your own
  brief files** (D1, D3) and create `EVIDENCE-STANDARD.md`; you may not edit `INVENTORY-LEDGER.md`.
- Your pass-12 routing is already written — the ten overturns, the four unreplayable, the four
  doubts, the stale Totals block and stale `F-SID-14` are in `CRITIC-pass12.md` awaiting dispatch.
  You do not need to chase any of it.
- **Never copy code from the reference checkouts** (`../_tiller-refs/{waku,zed,orca,t3code}`).
- **Proceed without asking for approval.** The scoping calls above are yours to make and state.

## Note on the machine

The disk hit 100% at ~16:10 and `pi` and `pireview` were killed mid-piece by ENOSPC; eleven critic
pass trees held 339G of `rust/target` between them. Reclaimed 279G, now at 23%. If you see a build
artefact that makes no sense, it may be a partial write from that window — rebuild before believing
it.

## Reporting

**12 lines or fewer**: what you added to D1 and D3 and what you deliberately left out, the evidence
standard's rule in one sentence, the sweep's result (or that you stopped at 2 and why), and the
honest remainder.
