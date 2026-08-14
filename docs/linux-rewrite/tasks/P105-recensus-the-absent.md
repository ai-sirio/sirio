# P105 — re-census the 86 rows the ledger calls absent

**Two owners, disjoint slices. Slice A: `fable`. Slice B: `codex12`.** Worktree
`/home/enzopalmisano/Scrivania/Progetti/tiller-linux`, branch `linux/gpui-waku`.

## Why this exists

`FAILED — absent` is the ledger's largest bucket at 86 rows, and **it is substantially wrong.**
Not marginally — three unrelated spot-checks on 2026-08-14 each hit a stale row on the first try:

| row | ledger says | today's tree |
|---|---|---|
| `F-TAB-23` | "no left/up split actions anywhere" | `TerminalContextAction::SplitLeft`, label `"Split Left"` (`tiller_terminal/src/context_menu.rs:15,68`), wired at `tiller/src/main.rs:2451` |
| `F-SID-16` / `F-SID-17` | "never built in the Linux rewrite — no row-drag code exists" | `crate::row_reorder::{ReorderScope, RowDrag, accepts_drop, insertion_index}` imported at `sidebar.rs:32`, scopes for Projects/Worktrees/Tabs at :529-531 |
| `F-CHAT-21` / `-22` / `-23` | "no Thinking expand/collapse", "no grouped-steps expansion", "`Entry::ToolCall` carries {id,title,status} only" | `Entry::Thought{expanded}` and `Entry::ToolCall{content, locations, raw_input, raw_output, expanded, group_expanded}` (`chat.rs:133-171`), `toggle_thought_expanded` :865, `toggle_tool_call_expanded` :874, `toggle_tool_call_group_expanded` :885, all three click-wired at :3006/:3367/:3468, all three with drawn tests at :6535/:6719/:6806 |

`FABLE-08` already found 39 such rows (`STALE-FAILED-CENSUS.md`). **None of the six above are in
that census** — they carry ordinary pass-8/pass-12 verdicts. So the census was not exhaustive, and
the number of already-built rows sitting in `FAILED — absent` is unknown and larger than 39.

**This blocks both tiers.** A builder handed a stale row rewrites working code — the exact failure
`P100` opens by warning about. A critic handed a stale row never routes it, because "absent" reads
as nothing to exercise. Every row you correctly reclassify turns into either an exercise route or
a real gap.

## The failure mode that produced this

A grep scoped to exclude the crate that later got wired keeps returning zero forever. The needle
is checked, the note says "re-checked today", and the answer is stale anyway. See the closing
paragraph of `UNPROVEN-ROWS-RECIPES.md`. **Assume every "no X exists anywhere" string in the
ledger was written against a tree that no longer exists.**

## What to produce

Append to `docs/linux-rewrite/RECENSUS-2026-08-14.md` — **create it if absent, and commit after
every 10 rows.** Three sessions were interrupted mid-work today and only survived because someone
found the diff; do not hold 40 rows of findings in an uncommitted buffer.

One row per line, in this shape:

```
| F-XXX-NN | BUILT | <symbol> at <file:line> — <what it is in one clause> | <the needle you ran> |
| F-XXX-NN | ABSENT | <the needle that found nothing, verbatim> | — |
| F-XXX-NN | PARTIAL | <conjunct that exists> at <file:line>; <conjunct that does not> | <needle> |
```

Rules for the three states:

- **BUILT** — you can name a symbol and a file:line. A test alone is not BUILT; a test plus a
  production call site is. If the only caller is a test, that is **PARTIAL**, and say so.
- **ABSENT** — you ran a needle that would have found it and it returned nothing. **Quote the
  needle.** A needle scoped to one crate is not enough; that is precisely how this doc's subject
  came to exist. Search `rust/crates/` whole.
- **PARTIAL** — the row's clause has conjuncts (look for "and"/"then" in the clause) and they
  disagree. Name which conjunct is which. This is the most common true answer and the one the
  old census had no vocabulary for.

Read each row's **clause** in `docs/linux-rewrite/01-inventory-app.md` (or `02-inventory-packages.md`)
before deciding — the ledger's evidence column tells you what a past pass believed, and that is the
thing under suspicion. The clause is the subject; the evidence is the claim.

## What NOT to do

- **Do not edit `INVENTORY-LEDGER.md`.** You produce a census, not verdicts. A row you find BUILT
  becomes `NOT EXERCISED` only when a critic exercises it, and that critic will not be you.
- **Do not fix anything.** If you find a two-line gap, record it and move on. A census that stops
  to build finishes a tenth of its rows.
- **Do not conclude from a green test.** Code plus a green test is `NOT EXERCISED`. Both defects
  in `P100` are covered by green tests and that is exactly how they survived four passes.

## Slice A — `fable` (30 rows)

```
F-PRJ-03 F-PRJ-04 F-PRJ-07 F-PRJ-10 F-PRJ-11 F-PRJ-12 F-PRJ-17 F-PRJ-18
F-CHAT-02 F-CHAT-16 F-CHAT-18 F-CHAT-21 F-CHAT-22 F-CHAT-23 F-CHAT-28 F-CHAT-29
F-CHAT-30 F-CHAT-31 F-CHAT-32 F-CHAT-34 F-CHAT-35
F-SET-11 F-SET-12 F-SET-13 F-SET-15 F-SET-16 F-SET-17 F-SET-18 F-SET-21 F-SET-24
```

Two of these are already settled — record them from here and spend no needles:
`F-CHAT-28` is **ABSENT** (`grep -niE "subagent|task_card|TaskCard" chat.rs` → zero hits) and
`F-CHAT-23`'s Dismiss conjunct is **ABSENT** (the only `dismiss` in `chat.rs` is the slash popup's,
:709-1754). Both are assigned to `pi` as `P102`. `F-CHAT-21`/`-22`/`-23`-expand are **BUILT** per
the table above. That leaves you 26 to actually work.

## Slice B — `codex12` (40 rows)

```
F-WIN-01 F-WIN-07 F-WIN-10
F-SID-06 F-SID-11 F-SID-15 F-SID-16 F-SID-17 F-SID-18
F-TAB-01 F-TAB-11 F-TAB-18 F-TAB-23 F-TAB-24 F-TAB-25 F-TAB-28
F-CHG-01 F-CHG-02 F-CHG-20
F-PER-07 F-BRW-08 F-USE-01 F-USE-02 F-USE-03 F-TERM-11
F-CORE-ACT-24 F-CORE-ACT-25 F-CORE-ACT-26 F-CORE-DOM-03
F-CORE-WSP-04 F-CORE-WSP-08 F-CORE-FILE-08 F-CORE-SET-01
F-CORE-USG-06 F-CORE-USG-07 F-CTRL-CLI-02 F-AGENT-API-01
F-AGENT-OPENCODE-03 F-AGENT-OMP-03 F-TERM-SPLIT-01
```

`F-TAB-23`, `F-SID-16`, `F-SID-17` are **BUILT** per the table above — copy that evidence and move
on. `F-SID-18` is **ABSENT** (`grep -rniE "no terminals" tiller/src tiller_ui/src` → zero hits).
That leaves you 36.

Rows deliberately excluded from both slices because another agent is on them right now:
`F-CORE-FILE-03/06`, `F-EDIT-12`, `F-TERM-UI-02` (`pireview` is judging `P95`);
`F-SID-07/09/19`, `F-TAB-13/14/16/21`, `F-CHG-11/13/16/18/22` (`sonnet` is exercising them
live as `P104`).

## The rules

- **Inspiration, never code.** waku, Zed, orca and comet are to look at; a transplant is a gap.
- Commit path-scoped, never `git add -A`. Several agents are editing `rust/` right now.
- Do not idle on an approval gate — `ENVIRONMENT.md` §"Working with the orchestrator".
