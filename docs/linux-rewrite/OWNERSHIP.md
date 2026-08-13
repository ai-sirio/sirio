# Ownership map of record

**This file is the map. Briefs cite it; they do not restate it.** Every restatement so far has gone
stale — `settings.rs` was still listed as `pi`'s in four briefs after it moved to `sonnet`, and P69
still hands `pi` a crate that is `codex11`'s.

Last set: 2026-08-14, early hours, by the orchestrator.

## Why every file is named

A file nobody owns is a file nobody fixes. `tiller/src/panes.rs` was in no map, and three
`FAILED — defective` rows (`F-CORE-ACT-06`, `-07`, `-11`) sat unassigned for two passes because of
it — their cited test has been failing reproducibly since pass 14, which is also the likeliest reason
the gate has never printed `CI OK`.

Measured when this map was written: **24 of the 69 source files over 200 lines had no owner.** The
dangerous ones were not the unowned *crates* — those were at least known to be unowned — but the
unowned *files inside owned crates*, which everyone assumed were somebody's.

## The map

| owner | files |
|---|---|
| `pi` | `tiller_ui/`: `chat.rs`, `sidebar.rs`, `status_bar.rs` · `tiller_markdown/**` |
| `codex11` | `tiller_ui/`: `changes.rs`, `right_panel.rs`, `editor.rs`, `file_view.rs`, `browser.rs` · `tiller_git/**` · `tiller_terminal/**` · `tiller_acp/**` · `tiller_agents/**` · `tiller_persistence/**` · `tiller_project/**` |
| `codex12` | `tiller/`: `main.rs`, `session.rs`, `panes.rs`, `command_palette.rs`, `tab_machinery.rs` · `tiller_ui/tab_bar.rs` · `tiller_control/**` · `tiller_usage/**` · `tiller_activity/**` |
| `sonnet` | `tiller_ui/`: `titlebar.rs`, `controls.rs`, `composer.rs`, `settings.rs`, `icons.rs`, `sfsymbol.rs` · `tiller_theme/**` |

Nothing is unowned. If a new file does not obviously belong to one of these, it belongs to whoever
owns the crate it lives in; if the crate is split, ask rather than assume.

## How the newly-assigned pieces were decided

- **`tiller_activity/**` → `codex12`.** The Layer A–D activity model. Fed by terminal events
  (`codex11`) and consumed by `panes.rs` (`codex12`) — the consumer wins, because the whole
  `F-CORE-ACT` cluster and its failing tests now sit with `codex12`.
- **`tiller/panes.rs`, `command_palette.rs`, `tab_machinery.rs` → `codex12`.** Same crate as
  `main.rs`/`session.rs`; `tab_machinery` is the natural pair of `tab_bar.rs`.
- **`tiller_persistence/**` → `codex11`.** Resolves the drift FABLE-11 flagged: P58 is a persistence
  job and has been `codex11`'s standing handoff all along. The map now says what practice already
  did.
- **`tiller_project/**` → `codex11`.** Adjacent to `tiller_git/**`, which it already owns.
  `skill.rs` stays here even though `sonnet` must *call* it for `F-SET-09` — **consuming a crate is
  not owning it**, and D-3 is a wiring job in `settings.rs`.
- **`tiller_markdown/**` → `pi`.** Markdown is a chat-rendering concern and `pi` carries the least
  backend load. `codex11` consumes the inotify monitor for B-06 — again, consumption, not ownership.
- **`tiller_ui/sfsymbol.rs` → `sonnet`.** Icon machinery, and `icons.rs` is already `sonnet`'s.

## The two rules that make this work

1. **Whoever widens an enum or struct owns every construction site and match arm it breaks, in any
   file — but only those.** This is the single exception to the map, and it is what has let five
   builders run all day in one repository without a collision.
2. **Where a piece genuinely needs two owners' files, cut it as a named two-half seam** (the P71
   pattern: Half A adds the setter, Half B calls it) rather than reaching across. Name the halves in
   the brief and dispatch them independently.

## Known consumption seams — not ownership

These are places where one agent's file must call another's, and they are the accidents waiting to
happen if the distinction blurs:

- `composer.rs` is `sonnet`'s, but the chat composer renders from `render_composer` **inside
  `chat.rs`**, which is `pi`'s. Any composer-area piece must name its file explicitly.
- `tiller_project/skill.rs` (`codex11`) is called by the Install Skill button in `settings.rs`
  (`sonnet`).
- `tiller_markdown`'s file monitor (`pi`) is consumed by `file_view.rs` (`codex11`).
