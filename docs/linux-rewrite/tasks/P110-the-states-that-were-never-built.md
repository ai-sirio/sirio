# P110 — four rows the census confirms nobody built

**Owner: `codex12`, as builder.** Worktree `/home/enzopalmisano/Scrivania/Progetti/tiller-linux`,
branch `linux/gpui-waku`.

## Why these four

`P105` re-censused the 86-row `FAILED — absent` bucket and found most of it wrong — 20 rows built,
36 partial. **Fourteen were genuinely absent**, with the needle that found nothing quoted verbatim.
Four of them are yours. They are not stale ledger rows; they are missing features.

| row | clause | ledger evidence |
|---|---|---|
| `F-SID-18` | no-terminal empty state for a selected worktree | no "No Terminals" empty state (pass 8) |
| `F-TERM-11` | no-worktree empty state in the terminal surface | no no-worktree empty state (pass 7) |
| `F-CHG-02` | no-worktree state in the changes surface | pass 13 drove it: after `close-workspace`, `surface changes open` **still serves the last worktree's data**. There is no no-worktree state and no explanation text |
| `F-TAB-25` | attach to the current terminal | `P65` built Move-to-Pane — a different feature — and explicitly refused this one (pass 14 recheck) |

Read each clause and its `VERIFY:` line in `docs/linux-rewrite/01-inventory-app.md`, which also carries
the `SRC:` pointer into Tiller's Swift (`F-SID-18` → `App/EmptyWorktreeView.swift:4`). **The Swift
source is the functional contract, not a design to transplant** — match what the feature does, and
follow COSMIC for how it looks.

`F-CHG-02` is the one with teeth. The other three are absent; that one is actively *wrong* — it shows
stale data from a worktree that is no longer selected, which is worse than an empty state, because a
user reads it as current. Treat it as a correctness fix that happens to need an empty state.

## Stay out of these files

`rust/crates/tiller_ui/src/chat.rs` and `composer.rs` are **being edited right now** by `codex11`
for `P107`. Nothing in your four rows needs them. If you find yourself opening chat.rs, stop and say
so in your pane instead — a merge collision there costs more than the row is worth.

## Proving it

`cargo test` green is **not** proof; this project's whole problem is that green tests coexist with
features nobody can reach. A drawn test that pins the pixels is the floor, and every one of these
four rows also needs to be **seen**.

`Scripts/wayland-drive.sh <outdir> '<actions>'` gives you a real window and a real `grim` capture,
takes no lock, and runs in parallel with other agents — set `TILLER_WL_LABEL=codex12` so your
sockets and paths don't collide. Read `WAYLAND-LANE.md` first; it has five traps, each of which has
already cost someone a false result. The lane has **no synthetic input**, so anything needing a
click or a keystroke you drive over the control socket, or you record `owed: gesture — <exact
gesture>` and it goes to whoever holds the `DISPLAY=:1` lock next (`sonnet` holds it now for `P109`).

An empty state is a good fit for this lane: reach the empty condition over the socket
(`workspace.*`, `tab.*`), then photograph it.

## What to produce

Working code, plus `docs/linux-rewrite/P110-report.md` with, per row: what you built, the drawn test
that pins it, the capture that shows it, and — stated plainly — **which conjuncts of the clause you
did not exercise.** `F-SID-18` is two conjuncts: the "No Terminals" text *and* a New Terminal action
that works. Do not report the pair as one.

Commit per row, path-scoped, never `git add -A`. `grep '??'` before calling a row done — the rule
that stops collisions never catches a file nobody is thinking about.

## The rules

- **Do not edit `INVENTORY-LEDGER.md`.** `pireview` owns it; your report is the input.
- If a row turns out to be genuinely unbuildable on Linux, that is a finding — say why, name the
  platform facility that is missing, and do not fake it.
- Do not idle on an approval gate — `ENVIRONMENT.md` §"Working with the orchestrator".
