# P64 — The diff surface: the four rows P62 deliberately left

**You are codex11, pane `w1:p2`.** Your context was just reset, so this brief is everything you need.

## P62 landed, and two things in it were exactly right

The loading state reads the **same** `refresh_started` the reentrancy guard uses, so the spinner and
the guard cannot drift apart. `Unstage all` calls `tiller_git::stage`/`unstage` — the live free
per-path functions, not one of the two dead parallel APIs sitting next to them in `actions.rs`. That
file has three APIs for the same four operations and eight of them are dead; picking the live one
without being reminded is the difference between wiring a feature and adding a fourth caller pattern.

You also did the ownership rule correctly: `NeedsInput` needed match arms in `sidebar.rs:1464` and
`main.rs:3097`, both other people's files, and you touched **only those arms**. That is precisely the
standing amendment.

Two carried forward:

- **`F-EDIT-04` is a stale FAILED** and you said so twice, plainly. That is worth as much as a fix.
  It is now recorded in `QUEUE.md` alongside two more `fable` found the same way — `F-SET-09` and
  `F-AGENT-SAFE-01` claim "no skill code", but `skill.rs` has a tested provisioner; the ledger
  searched `tiller_agents` and it lives in `tiller_project`.
- **The `main.rs:3026` mapping is still `codex12`'s**, and they have it in P63.

## The gate is red and none of it is yours

`cargo check --workspace` **passes**. `Scripts/ci-linux.sh` fails only on `cargo fmt --check`, in
`crates/tiller/src/main.rs` (7 sites) and `crates/tiller_ui/src/settings.rs` (~14) — `codex12`'s and
`pi`'s files, both mid-piece. Each owner formats their own file at the end of their next piece.

**Do not run `cargo fmt` across the workspace.** It rewrites files other agents have open right now
and their next write lands on top of it. Report the gate as fmt-blocked and name the two files; that
is the honest result, not a failure of yours.

## The piece: the four `F-CHG` rows P62 set aside

P62 took the four that fit one machinery. These four are the diff surface, and they belong together.

### 1. `F-CHG-01` — Changes as a Diff tab

The architectural one, which is why it was held back. Today the Changes panel is a panel; the row
says it should be reachable as a **tab**. You own `changes.rs` and `right_panel.rs`, and `codex12`
owns `tab_bar.rs` and `main.rs` where tabs are actually created.

**Do the part that is yours and stop at the boundary.** If the tab kind must be registered in
`main.rs` or `tab_bar.rs`, define the surface so that wiring is a few lines, say exactly what those
lines are in your report, and leave them for `codex12`. Do not reach across; a clean seam described
precisely is worth more here than a working feature built in someone else's file.

### 2. `F-CHG-13` — open a diff

### 3. `F-CHG-16` — resolve in terminal

This one is genuinely yours end to end: `tiller_terminal/**` is your crate. A conflict that can be
opened in a terminal at the right path is the whole row.

### 4. `F-CHG-18` — drag

Take it if the machinery from the first three carries it. **Say plainly if it does not** — an honest
"did not fit, here is why" is a result. A drag implementation that draws but drops nothing is the
defect class this project produces most.

### Leave these

`F-CHG-02`, `F-CHG-06`, `F-CHG-20` — blocked on display, and `pireview` has a working display now.

## COSMIC

The visual bar is the **Pop!_OS COSMIC** design language, not waku. `sonnet` is wiring the token
layer into `tiller_theme::Theme` right now (as of this brief, `Theme` did **not** yet consume it —
`cosmic/` existed but nothing read it, so anything you saw drawn today is still waku-styled).

Take colours, spacing and radii from `tiller_theme::Theme`, **never a literal**. Your P62 note that no
token existed for compact actions is exactly the right move — name the gap, use the nearest, and it
reaches `sonnet`. A diff surface wants `success`/`destructive` for added and removed lines; COSMIC
defines both, so do not invent them.

## Evidence

Follow `docs/linux-rewrite/EVIDENCE-STANDARD.md`. **Named drawn tests** (`TestAppContext` /
`VisualTestContext`, `.debug_selector(id)`, full `run_until_parked()` pump).

For "resolve in terminal", the test that counts asserts a terminal **opened at the conflicted path** —
one that only asserts the button exists proves the button. For the Diff tab, assert the surface
renders as a tab, or if the seam is genuinely `codex12`'s, assert everything up to the seam and name
what is untested and why.

Your P62 rebuild was blocked by `codex12`'s in-flight `NewChatAgent` work. Expect that again; **name
not-yours failures separately**, as you have done in both previous pieces.

## Rules

- Work in `/home/enzopalmisano/Scrivania/Progetti/tiller-linux`, branch `linux/gpui-waku`.
- **Yours: `tiller_ui/src/changes.rs`, `right_panel.rs`, `editor.rs`, `file_view.rs`, `tiller_git/**`,
  `tiller_terminal/**`.**
- **Do not edit** `settings.rs`, `sidebar.rs`, `chat.rs`, `status_bar.rs` (`pi`), `tab_bar.rs`,
  `tiller/src/main.rs`, `tiller_control/**` (`codex12`), `tiller_theme/**`, `controls.rs`,
  `titlebar.rs`, `composer.rs` (`sonnet`).
- **Standing rule:** whoever widens an enum owns every match arm it breaks, in any file — but only
  those arms.
- **Never copy code from the reference checkouts.** Mark rows `builder-claimed, unverified`, never
  `PASSED`. **Proceed without asking for approval.**

## Reporting

**12 lines or fewer**: what the Diff tab does and the exact seam left for `codex12`, open-diff,
resolve-in-terminal and whether a terminal genuinely opens at the path, whether drag fitted or did
not and why, any token `tiller_theme` lacks, tests by name, the gate with the fmt blocker named as
not-yours, and the honest remainder.
