# D2 — The palette: the front door the command layer never got

**You are codex12, pane `w1:p3`.** Your context was just reset, so this brief is everything you
need. Written by `fable` under the design mandate in `05-the-design-of-the-program.md` (decision
D2). If anything here contradicts the current code or ledger, the code and ledger win.

## P46 answered the platform question correctly, and this brief confirms it

Your keyboard-first resolution of the macOS-menu-bar question was checked against the design
mandate and it **stands**: the chord table, the typed `TitlebarEvent` title-strip controls, and the
context menus generated from one eligibility table (`Sidebar::context_menu_items`) are not a
menu-bar strip and nothing you built gets deleted. The disabled-reason typing — `AlreadyGitProject`,
`Disabled(NoActiveFile)` — reused one scheme instead of inventing a second, which is what keeps a
greyed item from being indistinguishable from a bug.

What P46 could not have known: the design decision `D2` names the **palette** as the command
layer's primary door, and it does not exist. Five chords (`Ctrl+T/O/S`, `Ctrl+Shift+S/I`) plus
every typed action you built are reachable only by knowing them. This piece is that door.

## Where

```bash
source ~/.cargo/env && cd /home/enzopalmisano/Scrivania/Progetti/tiller-linux
cargo test -p tiller -p tiller_control
./Scripts/ci-linux.sh   # strict
```

**Yours: `tiller_control/**`, `tiller/src/main.rs` (+ `panes.rs`/shell files you already own),
project discovery, `tiller_git/**`.** pi is in `tiller_ui/**` on the chat series (`D1`, `D3`…).
Anything that must render inside `tiller_ui/**` goes into
`P43-tab-command-layer-contract.md` for pi — never edited directly, never a second channel.

## The piece: `D-CMD-01`, and closing `D-CMD-02`

**A command palette.** One overlay, opened by **`ctrl-shift-p`**, listing every typed action the
shell can currently dispatch — the P43 tab actions, the P46/P48 window and sidebar commands, new
tab/agent actions — each row: label, chord where one is bound. Type-to-filter; Enter dispatches
**through the real dispatch path** (the same typed action a chord or menu emits, never a parallel
code path); Escape closes and returns focus where it was.

- **`ctrl-k` also opens it, but must yield to a focused terminal** — `ctrl-k` is readline
  kill-line, and a palette that steals it from a shell prompt is a defect, not a feature.
  `ctrl-shift-p` is the universal chord.
- **Ineligible commands render disabled with their typed reason** — the same
  `WindowCommandAvailability` / disabled-reason types the menus already use, one scheme everywhere.
- Filtering: substring at minimum; state what you chose. An empty result set draws a stated empty
  row, not a blank overlay (the P43 empty-branch rule, same shape).
- The palette overlay lives in shell-owned files. Theme tokens from `tiller_theme` are read-only
  inputs; appearance beyond tokens is display-debt, not this piece.

**`D-CMD-01`'s VERIFY, verbatim:** the palette opens on the chord, filters, and dispatches **one
sidebar action and one tab action** through the real dispatch path. Prove exactly that, drawn.

**`D-CMD-02` closes with an absence claim:** right-click context menus exist (sidebar rows — P46;
tabs — P43 `OpenTabMenu`; terminal panes — P46) and **no in-window menu-bar strip exists**. Assert
the absence in a drawn resting frame (no element of that kind in the debug map) and note that the
two title-strip visibility controls are counted chrome under `D-CHROME-01`, not a menu bar.

## Evidence

`TestAppContext` + `VisualTestContext`, `.debug_selector(id)`, `simulate_keystrokes` for the
chords, real clicks at debug bounds, **every drawn test hardened with the full
`run_until_parked()` pump** — an unhardened test does not count as evidence here:

- chord opens the palette; typing narrows the list; Enter on a **sidebar** command dispatches the
  typed action (assert the state change or emitted event, not the highlight);
- the same for a **tab** command (e.g. a close that goes through the shell's dirty-close door);
- `ctrl-k` in a focused terminal reaches the terminal, not the palette;
- a disabled row shows its typed reason and dispatches nothing.

## Rules

- **Never copy code from the reference checkouts** (`../_tiller-refs/{waku,zed,orca,t3code}`).
  Zed has a command palette and it is the standing temptation; take the idea, write the lines.
- Update `INVENTORY-LEDGER.md` rows this touches and `D-CMD-01`/`D-CMD-02` in `DESIGN-LEDGER.md`
  as **`builder-claimed, unverified` — never `PASSED`**. Only the critic closes.
- Keep the gate green. Proceed without asking for design approval; this brief is the design.

## Reporting

Reply in **12 lines or fewer**: what the palette lists and how it filters, the two dispatch proofs
by test name, how `ctrl-k` yields to terminals, what went into the contract file for pi (if
anything), the gate result, and the honest remainder.

---

## Orchestrator note (not part of the design brief)

**Two facts you need that the brief could not know when it was written.**

**1. The gate is red right now, and not because of you.** `pi` (P49) and `codex11` (P50) are both
mid-piece, and the last run was blocked by their in-flight formatting diffs plus a P50 pane-activity
test failure. **Check whose failure before acting on one** — you did exactly this in P46 and it was
right. Report the gate honestly as blocked-by-others if it still is; do not fix another agent's
in-flight files to make it green, and do not treat a red gate you did not cause as a reason to stop.

**2. Every claimed row must now name a replayable proof.** `fable` proposed this and it is adopted
project-wide: when you mark a row `builder-claimed, unverified`, the evidence column must name **a
test function or a transcript the critic can replay** — not a paraphrase of the VERIFY clause.

The reason is concrete. `F-CHAT-08` was `PASSED` at pass 8 on the evidence *"drawn
connecting/send/stop states"*. It is false: the composer's primary control is one `↑` glyph that goes
enabled/disabled and never becomes stop. Every noun in that clause exists somewhere in the file, and
the conjunction was assumed — **verdict by adjacency**. A named, replayable proof is what makes that
impossible, because a test either runs or it does not.

Your P51 report is the model: *"49 `tiller_control` tests pass; live headless/relaunch transcripts
pass"* names things that can be re-run.
