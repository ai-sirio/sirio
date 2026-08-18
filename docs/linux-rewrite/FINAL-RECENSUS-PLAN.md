# The finish line: re-exercising all 389 rows on this host

## Why this exists, and why it is not paranoia

The goal's bar is one sentence: **done means a critic that did not build a piece exercises every
inventory entry live and ticks it.** Today the ledger reads 363 / 389 PASSED — but those passes were
earned across **three different hosts**: an x86 desktop, a Raspberry Pi 5, and this x86 box. The code
did not change under them. The platform did, twice.

`EVIDENCE-STANDARD.md` already states that verdicts expire, and this project has paid for stale ones
in *both* directions within the last four days:

- 2026-08-17: eight rows parked `UNREACHABLE` were re-run. **Four were finished work the ledger was
  denying; four were genuine port gaps the stale reason was hiding.** One reason was load-bearing
  for four rows at once, and a single real click retired it.
- The `F-BRW` bucket is the sharpest case: eight `PASSED` rows whose every pass came from a host
  whose X11 lane the *next* host did not have at all.

So closing the tail of 26 open rows does not reach the bar. **A full re-exercise on this host does**,
and it is the only thing that lets a final report say "ticked live, here, today" without an asterisk.

## The partition — 12 shards, 389 rows, no leftovers

Computed from `INVENTORY-LEDGER.md`, not estimated. Shards are grouped **by UI surface**, so each
critic drives one coherent region of the app and amortises its app launches instead of relaunching
per row.

| shard | rows | area prefixes |
|---|---:|---|
| `activity` | 42 | `F-CORE-ACT` `F-CORE-USG` `F-USE` |
| `changes-git` | 38 | `F-CHG` `F-GIT-*` |
| `chat` | 37 | `F-CHAT` |
| `sidebar-proj` | 37 | `F-SID` `F-PRJ` |
| `settings` | 35 | `F-SET` `F-CORE-SET` `F-PER` |
| `window-persist` | 33 | `F-WIN` `F-PERSIST-DB` `F-PERSIST-PLAT` `F-CORE-WSP` |
| `agents-auto` | 32 | `F-AUTO` `F-AGENT-*` `F-CORE-AUTH` |
| `terminal` | 31 | `F-TERM*` `F-CORE-TERM` |
| `editor-files` | 30 | `F-EDIT` `F-CORE-FILE` `F-CORE-DOM` |
| `tabs` | 28 | `F-TAB` |
| `control` | 28 | `F-CTRL-*` (not `F-CTRL-BROWSER`) |
| `browser` | 18 | `F-BRW` `F-CTRL-BROWSER` `F-CORE-UI` `F-CORE-PLAT` |

Fan-out here is `min(16, cores-2)` = **10**, so 12 shards run as one queue, not two hand-managed
waves.

## Rules for the shard critics

1. **Judge from the VERIFY clause** in `01-inventory-app.md` / `02-inventory-packages.md`. Never from
   the ledger's prose, a commit message, or a builder's claim.
2. **A prior PASSED is not evidence.** It is the hypothesis under test. Re-drive it here.
3. **Do not carry a verdict across hosts.** If a row genuinely cannot be driven on this box, say so
   with a measurement dated today and name the host that last passed it — never silently inherit it.
4. **Removing an unearned claim must not manufacture a pass.** Census evidence — a symbol and a line
   number — makes a row `FAILED — absent` or `UNREACHABLE`, never `PASSED`.
5. **The critic never edits the ledger.** Verdicts are returned structured and applied centrally by
   `Scripts/apply-verdicts.py`, which is the single writer; `Scripts/ledger-totals.py` is the gate.
6. Every shard names its lane and its capture directory, so any row can be replayed.

## What a shard critic is expected to cost

One app launch per coherent group of rows, a drive script per group, and a capture per claim that is
visual. `SHOT-LIST.md`, `UNPROVEN-ROWS-RECIPES.md` and `STALE-FAILED-RECIPES.md` hold routes that
already work — a recipe says what to *do*, never what to *conclude*.
