# FABLE-08 — The other direction: how many of the 141 `FAILED — absent` rows are already built?

**You are fable, pane `w1:pD`.** Your context was just reset, so this brief is everything you need.

## FABLE-07 landed and is committed

`f0b44ef` carries your three artefacts: `Scripts/adjudication-census.py`,
`docs/linux-rewrite/ADJUDICATION-BACKLOG.md`, and the pinned ledger. The line you had in your input
buffer — *"committa gli artefatti FABLE-07 su linux/gpui-waku"* — is **done**; the commit stages
exactly those three files and nothing else. Nothing further is owed on it.

Two things in that pass set the standard for this one:

- Your positive controls tested the **backend**, not just the heuristics — a symbol known present, a
  fabricated symbol confirmed absent, and a population recheck. That is the discipline that catches
  the failure this project keeps hitting: a check that executes, reports success, and proves nothing.
- You recorded the instrumental limit (`cfg(test)` split undercounts production in mixed files, e.g.
  `try_recv`) **and** stated that no conclusion rests on it alone. A limit disclosed is a limit that
  cannot silently corrupt a downstream verdict.

You also drew the boundary correctly: routes verified by *reading dispatch sites, not running the
app*, so these are inputs to a verdict rather than verdicts. Hold that line again here.

## The piece: the mirror of FABLE-07

FABLE-07 asked, of rows marked built: *is this reachable, or only a test?* — the false-PASSED
direction.

**This piece asks the opposite: of the 141 rows marked `FAILED — absent`, how many are already
built?** A false FAILED costs exactly what a false PASSED costs. A false PASSED ships a hole; a false
FAILED sends a builder to construct something that already exists, and we have now paid that price
repeatedly.

**Six are already known stale**, four of them found today:

| row | ledger says | truth |
|---|---|---|
| `F-EDIT-04` | "no ⌘S binding" | `main.rs:116` `(WindowCommand::SaveFile, "ctrl-s")` |
| `F-EDIT-06` | "no save path" | `editor.rs:641 pub fn save()`, `main.rs:5584` |
| `F-EDIT-08` | "`add_file_tab` pushes unconditionally" | `main.rs:4001` dedupes |
| `F-SET-09` / `F-AGENT-SAFE-01` | "no skill code" | `skill.rs`, in `tiller_project` not `tiller_agents` |
| `F-TAB-02`, `F-TAB-27` | "no overflow" / "no resume-chat" | **`codex12` built both in P65, within the hour** |

**The last row is the important one.** The ledger does not go stale once; it goes stale
*continuously*, every time a builder ships and no pass re-reads. Any process that catches these by
accident will always be behind. That is what this census is for.

## Two existing signals, both partial, both yours to use and neither to trust alone

- `Scripts/stale-failed.py` — ranks by how the absence was **worded** (`scoped`, `no-search`,
  `bare`). Its `HIT` signal is **circular** and its header says so: a row citing identifiers is a
  well-evidenced row, so rediscovering them rewards rows for being specific.
- `Scripts/assigned-but-absent.py` *(new)* — ranks by whether a builder was ever **assigned** the
  row, using the task briefs as an independent input. **60 of 141 rows were assigned; 81 never
  were**, and it prints that it has nothing to say about those 81 rather than letting silence read
  as a clean bill.

**The 81 are the blind spot and the largest part of your job.** Neither signal covers them well.

A caution earned twice today, in both scripts: **when a control fails, ask first whether the tool's
claim is too broad, not whether the control is too strict.** `assigned-but-absent.py` scored `live`
identifiers, which re-created the `HIT` circularity and pushed a genuinely stale row below the
median; the control caught it and the *formula* was wrong. And `F-AGENT-SAFE-01` is declared out of
scope there rather than made to pass, because it was found by census and never assigned. Loosening a
control until the script prints is how a triage becomes a ranking nobody should trust.

## What to produce

For each `FAILED — absent` row, one of:

1. **still absent** — searched properly and not found. Say what you searched for *and in what
   vocabulary*.
2. **already built** — name the file and line. This is a stale-FAILED candidate for `pireview`.
3. **cannot tell** — an honest result. Say what you checked.

**A vocabulary warning that will bite you.** `F-EDIT-04` reads "no ⌘S binding" while the Linux
binding is correctly `ctrl-s`. **A row phrased in the old platform's vocabulary defeats every search
made in that vocabulary.** Expect ⌘/cmd, `NSWindow`, `WKWebView`, `Keychain`, SwiftUI names. Search
the Linux/GPUI equivalent, not the row's words — and say when a row's wording is the reason it looks
absent, because that is itself a finding worth more than the row.

**You are not adjudicating.** Change no verdict, touch no ledger row — exactly as FABLE-06 and
FABLE-07 did. You produce evidence; `pireview` decides. That separation is why your work is trusted.

## Rules

- Work in `/home/enzopalmisano/Scrivania/Progetti/tiller-linux`, branch `linux/gpui-waku`. **You are
  currently sitting in the main repo on `rust/gpui-rewrite` — `cd` first.**
- **Pin your input** and say what you pinned to, as you did in FABLE-07. Five builders are editing.
- **Read-only over other people's code.** Artefacts are `Scripts/*.py` and a **new** doc under
  `docs/linux-rewrite/`. Do not edit crate source. Do not edit `INVENTORY-LEDGER.md`. Do not move
  `DEAD-MODELS.md` or `ADJUDICATION-BACKLOG.md` — both are pinned references for `pireview`.
- `rg` is **not installed**. Use `grep`, and make any tool you write **prove its own search backend
  finds a symbol known to be present** before it reports a single absence.
- **Proceed without asking for approval.**

## Reporting

**12 lines or fewer**: what you pinned to, the three counts (still absent / already built / cannot
tell), **every "already built" row named with file and line**, how many of the 81 never-assigned
rows you could settle, any row whose *wording* is why it looks absent, what your positive controls
asserted, artefact paths, and the honest remainder including which rows you could not settle and why.
