# P34 — There is no editor. Build one, and do not copy Zed's.

**You are pi, pane `w1:p4`.** Your context was just reset, so this brief is everything you need.
Your own notes are in `docs/linux-rewrite/PI-HANDOFF.md`.

## P32 is closed, and the ledger you kept is the reason it is trustworthy

`LocalAccountState` in `tiller_usage` with the AI Providers cards now deriving their status instead
of asserting `"Active"`; a `Radii` token set in `tiller_theme` with the named component radii moved
onto it; the type-scale drift fixed (12 → 11.5). `CI OK`.

And the part that was actually asked for: **you recorded every departure from the frozen waku bar
with its reason** — tab bar and toolbars at 34, activity rows at 48, status bar at 10.5, the user
pill at 13.5/21 against waku's 14/20, settings cards on r12, popovers on r10, the
full-circle-as-half-box idiom — and you put the caveat in the module header, that this proves
conformance to the bar and not that the result looks right. Without that ledger the suite would
quietly become an authority over decisions somebody made for good reasons.

You also flagged two boundary touches: a one-line `#[cfg(test)] mod conformance;` in the
integrator-owned `lib.rs`, and `render_provider_card` taking a struct to stay under clippy's
argument budget. Both noted.

## The finding

The critic — which built none of this — exercised `## Documents and editors` and reported:

> **There is no editor.** `FileView` is a read-only viewer — no editing, no save, no dirty state,
> no dedupe. 8 entries fail by **structural absence**, not by defect.

It drew that distinction deliberately and it matters: an absent feature and a broken one need
different work. This is now the largest missing area in the project.

## The trap, named up front

Zed's source is checked out at `../_tiller-refs/zed`, and it contains a complete, excellent text
editor. **A text editor is genuinely hard, and the answer is sitting right there.** This is the
single strongest temptation to transplant code in this entire project.

**Do not.** The rule has been the same since the first line: the references are for inspiration,
never for code; every line of Tiller is written from scratch; and the critic checks for this
explicitly — its last audit measured 3–8% substance-line overlap with waku and zed and found it was
all unavoidable GPUI boilerplate. Transplanted code counts as a gap, always, and it would be found.

Read Zed's editor to understand *how the problem is shaped* if that helps. Then close it and write
ours.

## The scope, and be honest about it

`docs/linux-rewrite/01-inventory-app.md`, section `## Documents and editors` — **13 entries**. Count
them yourself.

**You are not building Zed's editor.** You are building what these entries require, which is much
smaller and mostly not about text rendering at all:

Testable headless, and where most of the substance is:

- **F-EDIT-04** — edit and save; the change survives closing and reopening.
- **F-EDIT-05** — the file changed on disk while open: a conflict state offering Reload and Keep,
  and **both must work**, not just the banner appearing.
- **F-EDIT-06** — the file was deleted externally; saving recreates it, or shows a visible error.
- **F-EDIT-07** — language detected from the extension, plain-text fallback for unknown ones, no
  wrapping, four-space indentation.
- **F-EDIT-08** — opening the same path twice focuses the existing document instead of making a
  second copy. This is the dedupe the critic found missing.
- **F-EDIT-02** — the Markdown formatting operations are **pure text transformations** over a
  buffer and a selection: bold, italic, headings, lists, links. Testable without any UI at all.
- **F-EDIT-03** — the large-file threshold and its manual-preview state.
- **F-EDIT-13** — the specific missing-file and unreadable-file messages, each distinct.

Genuinely pixel-bound, and they stay `NOT EXERCISED — blocked on display`: F-EDIT-01 (Code and
Preview modes side by side), F-EDIT-09 (double-click to open), F-EDIT-12 (drag a file into a pane).

Platform-adapted, and say which you chose: **F-EDIT-10** names "Show in Finder" — the Linux
equivalent is opening the containing directory with `xdg-open`, so adapt it rather than marking it
`N/A`. **F-EDIT-11** copies a path to the clipboard, which on X11 needs a display — the *derivation*
of the path is testable, the clipboard write is not.

**Dirty state matters beyond this section.** `F-TAB-16` requires a confirmation before closing a
dirty document, and today nothing can be dirty. Wire the flag so that entry becomes reachable, even
if the prompt itself is pixel-bound.

## Where

```bash
source ~/.cargo/env && cd /home/enzopalmisano/Scrivania/Progetti/tiller-linux
cargo test -p tiller_ui -p tiller_theme
./Scripts/ci-linux.sh
```

No display presents; everything above is provable without one. codex12 is in
`tiller/src/main.rs` and `tiller_control/**`, codex11 in `tiller_terminal/**`,
`tiller/src/panes.rs` and `tiller_activity/**`. **`tiller_ui/**` and `tiller_theme/**` are yours.**
If opening a document needs a change in the shell's tab machinery, specify it and it will be routed.

## Evidence

Tests for every headless entry above — and for F-EDIT-05 and 06 in particular, exercise the
**external mutation** for real: write the file from outside the app, then check the conflict state
and both resolutions. A conflict test that never touches the filesystem proves nothing.

For F-EDIT-08, prove it with two opens of the same path and one resulting document.

## Rules

- **Never copy code from the reference checkouts.** Stated twice deliberately.
- **A capability nobody exercised does not exist.**
- Anything needing pixels is `NOT EXERCISED — blocked on display`; never approximated.

## Reporting

Reply in **12 lines or fewer**: how many of the 13 you closed and the four counts, what the editor
can and cannot do, how you adapted F-EDIT-10/11 for Linux, whether dirty state now reaches
`F-TAB-16`, and the honest remainder.
