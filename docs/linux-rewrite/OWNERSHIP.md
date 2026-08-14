# Ownership map of record

**This file is the map. Briefs cite it; they do not restate it.** Every restatement so far has gone
stale — `settings.rs` was still listed as `pi`'s in four briefs after it moved to `sonnet`, and P69
still hands `pi` a crate that is `codex11`'s.

Last set: 2026-08-14, 00:20, by the orchestrator — after the roster lost two panes.

## The roster is three builders, not five

At 00:15 on 2026-08-14 both `pi` panes died on an account-level `429 GoUsageLimitError` (monthly
limit, ten days to reset). It took `pi` (builder, `deepseek-v4-flash`) and `pireview` (critic,
`deepseek-v4-pro`) together, and with no `auth.json` present there is no second provider to fall
back to. Neither can be recovered without spending the user's money, which is theirs to authorise.

- **The critic role moved to `fable`** — see `tasks/CRITIC-pass17-handover.md`. `fable` has never
  written Rust here, so it is independent on every row.
- **`pi`'s files were redistributed** below. `pi` had just delivered `F-CHAT-24/25/26/27`, the ACP
  child-exit watchdog and the `ctrl-` chord fix; that work is in the tree and gated, and whoever
  inherits the file inherits it.

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
| `codex11` | `tiller_ui/`: `changes.rs`, `right_panel.rs`, `editor.rs`, `file_view.rs`, `browser.rs` · `tiller_git/**` · `tiller_terminal/**` · `tiller_acp/**` · `tiller_agents/**` · `tiller_persistence/**` · `tiller_project/**` · **`tiller_markdown/**`** |
| `codex12` | `tiller/`: `main.rs`, `session.rs`, `panes.rs`, `command_palette.rs`, `tab_machinery.rs` · `tiller_ui/`: `tab_bar.rs`, **`sidebar.rs`** · `tiller_control/**` · `tiller_usage/**` · `tiller_activity/**` |
| ~~`sonnet`~~ | **reassigned 2026-08-14 — `sonnet` became the second critic and now owns no source files.** See below. |

`fable` owns no source files **by design** — it is the critic, and a critic that has built something
cannot judge it. **`sonnet` now owns none for the same reason.**

## `sonnet`'s files, reassigned — 2026-08-14

Promoting `sonnet` to second critic bought critic throughput, which is the project's rate limit, and
paid for it by **orphaning a large part of `tiller_ui` in the same move** — including `chat.rs`,
which holds the ten-row transcript tier and most of what makes this an agent client rather than a
chat box. The handover doc anticipated builders dropping to two; it never said who inherits. This
does.

It is also a chance to apply `SEAMS.md`'s own advice — *shape the piece so a single owner holds both
halves* — so two open seams close by reassignment rather than by dispatch:

| file | new owner | why |
|---|---|---|
| `chat.rs`, `composer.rs` | `codex11` | chat **is** the ACP surface, and `tiller_acp/**` + `tiller_agents/**` are already `codex11`'s. Keeping the pair together also preserves the `composer.rs`/`render_composer` fix that put both halves with one owner in the first place. |
| `settings.rs` | `codex11` | **closes `F-BRW-08`.** That seam was "mount the browser Permissions section in `settings.rs`" against browser state in `browser.rs` — and `browser.rs` is `codex11`'s. One owner now holds both halves. |
| `status_bar.rs` | `codex12` | **closes the `tiller_usage` seam.** `status_bar.rs` consumes `ProviderUsage`/`UsageWindow` from `tiller_usage/**`, which is `codex12`'s; those very imports were the last thing blocking the `CI OK` gate. |
| `titlebar.rs`, `controls.rs`, `icons.rs`, `sfsymbol.rs`, `tiller_theme/**` | `codex12` | window chrome and the shared visual vocabulary, which pair with `tab_bar.rs` and `sidebar.rs`. |
| `project_identity.rs` | `codex12` | `sonnet` created it in P80 and it never reached this map — **a new file inside a split crate, which is precisely the case the top of this document calls the dangerous one.** Its mount target is `sidebar.rs::render_project_settings`, already `codex12`'s, so assigning it here puts the picker and its mount with one owner. |

**`codex11` is now carrying more than `codex12`.** That is deliberate: `codex12` owns `main.rs` and
is the integration bottleneck by construction, so loading it with chrome would slow every other
pane's Half B. Revisit if `codex11` becomes the queue.

**Nobody may judge `F-CHAT-*`, `F-SET-*`, `F-USE-*` or anything resting on the files above by reading
`sonnet`'s tests as proof.** `sonnet` built those surfaces and cannot judge them; the new owners did
not build them and can. Verdicts on them belong to `fable`, or to a new owner exercising them live.

### Why `pi`'s files went where they did

- **`sidebar.rs` → `codex12`, not `sonnet`.** The obvious home was `sonnet`, which owns the rest of
  the `tiller_ui` chrome. But `F-SID-16`/`F-SID-17` (sidebar row reorder) and `F-TAB-18` (tab
  reorder) are **the same drag primitive**, and `tab_bar.rs` is already `codex12`'s. Split across two
  owners, that primitive gets written twice.
- **`chat.rs`, `status_bar.rs` → `sonnet`.** This **closes the worst seam in this file**: `sonnet`
  owned `composer.rs` while the chat composer actually renders from `render_composer` *inside*
  `chat.rs`. One owner now holds both halves.
- **`tiller_markdown/**` → `codex11`,** which already consumes its file monitor from `file_view.rs`.
  That seam closes too.

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

- `tiller_project/skill.rs` (`codex11`) is called by the Install Skill button in `settings.rs`
  (`sonnet`).

**Open seams now live in `SEAMS.md`, and that file is the record — not this one.** This section
kept going stale, which is the whole problem it was meant to solve: it listed `add_chat_tab`'s
`&mut Window` as waiting long after `codex12` had shipped it (`main.rs:4201`, verified 2026-08-14).
A seam tracked in prose in two places is a seam nobody dispatches.

**A brief that cuts a seam registers Half B in `SEAMS.md` in the same commit.** Half A landing is
not the end of a seam; a row moving is. Thirteen ledger rows currently read `FAILED — absent` for
code that exists, and at least one of them — the 1283-line browser — is orphaned because ownership
discipline correctly stopped `codex11` from mounting it and nothing tracked the other half.

Two seams that used to be listed here are **gone**, not resolved: `composer.rs`/`chat.rs` and
`tiller_markdown`/`file_view.rs` each now sit with a single owner. Losing a pane shrank the surface
where two owners had to agree, which is the one good thing to come out of it — and it is the
cheapest fix available whenever a seam looks permanent: give both halves to one owner.

## Amendment — 2026-08-14, 09:50: `chat.rs` + `tiller_acp/**` to `sonnet` for P91

**`chat.rs`, `composer.rs` and `tiller_acp/**` move to `sonnet` for the duration of `P91`.**
`codex11` must not touch those three while it is in flight.

### This is a correction of an orchestrator error, not a plan

I dispatched `P91` to `sonnet` as a builder without checking this file. Both files were
`codex11`'s and `sonnet` had been recorded as owning none. The brief even says "Owner:
`sonnet`, as builder" — written against a roster I remembered instead of one I read, which is
the exact failure the top of this document was created to prevent, committed by the person
maintaining it.

Recording it rather than reversing it, for two reasons that would hold even if I had planned it:

- **This document's own escape clause fires.** It says *"`codex11` is now carrying more than
  `codex12`… revisit if `codex11` becomes the queue."* `codex11` now owns thirteen crates plus
  six `tiller_ui` files and has `P89` in flight; it is the queue. `sonnet` was idle.
- **The seam principle is satisfied either way.** `P91`'s seam is `tiller_acp` (widen the event)
  → `chat.rs` (render it). Both halves are in this transfer, so one owner still holds both —
  which was the whole reason `chat.rs` went to `codex11` in the first place.

### What it costs, stated plainly

**Critic throughput, which this document calls the project's rate limit.** Two critics become
one (`fable`) while `P91` runs. That is the real price and it is why this is scoped to one
piece and not made permanent.

It costs nothing in *independence*: `sonnet` built the chat surfaces before the reassignment, so
the rule below — nobody may judge `F-CHAT-*` on `sonnet`'s tests — already applied and still
does. `sonnet` could not have judged this tier either way.

### On expiry

When `P91` lands, `chat.rs`, `composer.rs` and `tiller_acp/**` return to `codex11` and `sonnet`
returns to critic duty. Until then this amendment is the map for those three paths.
