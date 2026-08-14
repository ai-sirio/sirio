# P96 — eight rows the display was never the obstacle for

**Owner: `codex12`.** Worktree `/home/enzopalmisano/Scrivania/Progetti/tiller-linux`, branch
`linux/gpui-waku`. **Read `docs/linux-rewrite/HEADLESS-LANE.md` first — it is short and two of its
paragraphs will save you a wasted cycle.**

## Why you, and why now

You are idle and the other three are not, so this is throughput. More to the point: **nothing here
touches `main.rs`**, which is what blocked you from committing `P90`, and **nothing here takes the
display lock**, which is what `fable`, `sonnet` and `codex11` are all queued behind.

Tonight I tried to stand up a second X display so drives could run in parallel. GPUI cannot present
on `Xvfb` — the window captures as one flat colour, with lavapipe or without. **But the process is
otherwise entirely alive**: PTYs spawn and run commands, git is read correctly, and all 54
control-socket methods answer. An instance is running now on `/tmp/x2.sock`; stand up your own with
your own DB and socket per the doc.

That is a verification lane with no contention. These eight rows are `NOT EXERCISED` and, on
inspection, **none of them needed the screen** — they needed an instrument nobody had.

## What you may judge, and what you may not

You built the `F-CTRL-*` and `F-SET-*` control work under `P87`/`P88`/`P90`. **Do not judge those
rows.** A builder ratifying its own work is the one thing the critic rule exists to prevent, and the
socket makes self-ratification unusually easy — it will answer you exactly the way you designed it
to.

You did not build any of the eight below. Judge those.

## The eight

| row | reach it with | what the clause needs |
|---|---|---|
| `F-CORE-ACT-19` | `notify` → `list-notifications --json` | payload actually built and delivered |
| `F-CORE-ACT-20` | same, plus the gating condition | the notification is **suppressed** when it should be |
| `F-AGENT-OMP-01` | `panel create --cmd` with the omp adapter's command | the adapter's two names are no longer conflated |
| `F-AGENT-OMP-02` | as above | the row's stated cause was "blocked by OMP-01" — retest it |
| `F-BRW-04` | `browser.*` (raw JSON; no CLI wrapper) | an invalid address **and** an unreachable host |
| `F-PER-08` | sqlite on your own `TILLER_DB` | `browser_origin_grant` present and *usable*, not merely present |
| `F-BRW-07` | `browser.*` + the same table | a grant persists across a restart |
| `F-TAB-12` | `tab select` / `tab cycle` | **report only** — see below |

`F-TAB-12/15/17` are tab **context-menu** rows. The socket can select and cycle tabs but cannot
right-click, so the menu half is out of reach here. **Do not mark them anything.** Say in your report
how much of each is socket-reachable, so whoever takes the display lock next knows exactly what is
left to do rather than redoing the reachable half.

## Three things that will bite you

1. **A first read can be a race, not a finding.** `surface changes read` told me `0 0 0` against a
   worktree with 177 modified files. Three seconds later, `0 36 141`, matching `git status` exactly.
   I was one step from filing a false defect. **Read twice, a beat apart, before you believe a zero.**
2. **`capabilities` is a static list.** It names methods; it does not prove them. Trusting it is
   failure-mechanism (g) in `QUEUE.md` — *a channel that reports success for work it never does*.
   Exercise every method you rely on.
3. **`surface.chat.*` is not the chat the user sees.** There are two independent ACP paths in
   production: `ChatView::launch_with_command` (the UI) and `ChatSession::launch` (the socket).
   `tiller_ui/src/chat.rs` has zero references to `ChatSession`. If you touch chat at all, you are
   proving the protocol layer, **not** the chat surface — say so explicitly.

## Your uncommitted `P90` code

Do not leave it uncommitted — this repo lost its object database once already. Commit path-scoped.
For `main.rs` specifically, staging the file necessarily stages whatever another agent has in it at
that moment, so: run `cargo check -p tiller` first, commit only if it is green, and **say in the
commit message that the file may carry concurrent edits**. A red commit costs everyone; an honest
commit message costs nothing.

## The rules

- **Inspiration, never code.** waku, Zed, orca, comet are to look at. Transplanted code is a gap.
- **Code plus a green test is `NOT EXERCISED`, never `PASSED`.**
- **Do not edit `INVENTORY-LEDGER.md` totals by hand** — recount with the one-liner in its Totals block.
- `git status --short | grep '??'` before you finish.
- Kill only **your own** app process, matched on `TILLER_SOCKET` in `/proc/<pid>/environ` — never by
  name. Another agent's live drive is also called `tiller`.

## Done means

1. Each of the seven judgeable rows exercised and given a verdict **with the command and its output**.
2. `F-TAB-12/15/17` reported as reachable-fraction, not judged.
3. Anything the lane could **not** reach named as such, with the reason — `NOT EXERCISED` with an
   instrument reason is a real result; `FAILED` for an instrument gap is not.
4. Report the verdict deltas.
