# P49 — The composer: six absent entries that share one machinery

**You are pi, pane `w1:p4`.** Your context was just reset, so this brief is everything you need.

## P44 did the thing this project keeps failing to do

You judged 37 chat entries **one at a time, in order — exist, exercisable, VERIFY** — and came back
with 25 absent. A builder auditing its own surface usually returns a better number than that. You
returned the worse one, and it is the true one.

Three things in that report are worth naming:

- **You closed the four entries the previous brief had wrong**, and when `F-CHG-20` turned out not to
  exist at all you said *"the brief was optimistic"* rather than quietly building it and calling the
  entry closed.
- **`F-SET-03` stayed PARTIAL because the handler is still empty.** The version renders, the click
  dispatches, and you refused to promote it on that basis. That is exactly the distinction that has
  caught five defects here.
- **The `LC_ALL=C` fix in `tiller_git`.** Clone progress parsed nothing on an Italian git, so the
  failure was deterministic *on this machine* and invisible on any English one. A developer's locale
  silently changing what a program parses is the kind of bug that survives a whole project.

## Where the project stands

The critic's pass 10 converted 102 rows. The ledger now reads: **146 PASSED · 102 FAILED—absent ·
64 half-proven · 32 NOT EXERCISED · 7 defective · 7 display-blocked · 23 N/A.**

The display gates 7 of 388. It is not the wall. The wall is 102 features that were never built.

## The piece: the composer, not the cards

**Six absent entries, one machinery:**

| entry | what is absent |
|---|---|
| `F-CHAT-09` | slash commands in the composer |
| `F-CHAT-10` | `@` file mentions |
| `F-CHAT-11` | image attachment control |
| `F-CHAT-12` | attachment chips |
| `F-CHAT-14` | overflow menu, incl. Follow Edited Files |
| `F-CHAT-17` | effort levels |

Read each entry in `01-inventory-app.md` and **count them yourself** — a number in a brief is
somebody's recollection; the file is the contract.

Four of the six are the same shape: **a trigger character opens a filtered popup over the input, a
selection inserts a token, and the token survives editing and submission.** Build that once. The two
that are not (attachment chips, effort levels) share the composer's own state.

The three questions per entry, in order: **does it exist; can a user reach it; does it do what the
`VERIFY` clause says** — not what it plausibly does.

## Why you are not getting the card blocks yet

`F-CHAT-24/25/26/27` (plan card, question answering, pending-question bar, expired state) and the
transcript-expansion block are deliberately held back. A fifth agent, `fable`, currently holds a
mandate over the **design** — whether this is a coherent program or an inventory that grew a face —
with the authority to change what gets built. Its verdict lands soon. The composer's behaviour is
invariant under any answer it could give; a plan card's is not.

If you finish the six early, take `F-CHAT-19` (the context warning colour above 80%) and
`F-CHAT-35/36` (the two empty states) rather than starting a card.

## Evidence

No display is available and none is needed for behaviour: `TestAppContext` with
`VisualTestContext`, `.debug_selector(id)`, `simulate_keystrokes`, real mouse events. **Harden every
drawn test with the full `run_until_parked()` pump** — you are the one who established that an
unhardened test passes on timing luck, so an unhardened one is not evidence here.

Type the trigger character through `simulate_keystrokes` rather than calling the handler; the popup
that only a direct handler call can open is the exact failure mode this codebase produces.

## Rules

- **Never copy code from the reference checkouts** (`../_tiller-refs/{waku,zed,orca,t3code}`). The
  Swift original in each `SRC:` field is the reference for **behaviour** only.
- **Keep the gate green** — `./Scripts/ci-linux.sh`, ~20 s, strict, `-D warnings` on owned crates.
- **Update each ledger row as you close it, marked `builder-claimed, unverified`** — never `PASSED`.
  Only the critic's verdict counts toward done.
- Proceed without asking for design approval.

## Reporting

Reply in **12 lines or fewer**: what you built, the evidence per entry, what the popup does when the
filter matches nothing and when the file list is huge, anything the protocol tier owes you, the gate
result, and the honest remainder.
