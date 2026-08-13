# Critic pass 7 — audit the reclamation, because it is exactly where over-claiming happens

**You are pireview, pane `w1:p6`. You are the critic.** Your context was just reset, deliberately:
you judge this build without having seen any builder's reasoning. You built none of it.

> **A feature you have not successfully exercised does not exist.**

## Pass 6 found the worst defect still in the tree

> **The control tier's lifecycle lie.** `workspace.close` leaves control panes' PTYs and orphaned
> process groups running — its own `VERIFY` demands termination. `terminate()` kills only the direct
> child, so compound-command panes leak on `panel.close` and on app quit too.

That contradicts a claim already recorded as PASSED. F-PER-06 was proven with a child pid alive
before quit and gone after — a true measurement that watched **the direct child only**. A pane
running a pipeline leaves the pipeline behind. *Exercised narrowly* is its own failure mode, and you
are the only reason it surfaced.

It also explains something everyone had been misreading all day: this machine has been accumulating
stray app and shell processes for hours, and that was repeatedly attributed to other causes.

You also found `worktree.set`'s **comments claiming a persistence the code does not have** — no DB
column, dies on restart. A comment asserting a property the code lacks is worse than no comment.
Both routed.

And you confirmed the Codex `-c notify=[...]` override loads in real codex 0.147.0 — the TOML
escaping trap holds.

## The piece

Two things happened while you were working, and together they are exactly the situation where
optimistic bookkeeping creeps in.

**First**, GPUI's own view-test harness was found to work with no display:
`gpui::TestAppContext` with `VisualTestContext` runs the real element tree, produces a **drawn frame
with a debug-bounds map**, and dispatches **real mouse and key events through the real dispatch
path** — including `simulate_keystrokes`, drags via mouse down/move/up, and double-clicks via
`click_count`. Elements are found with `.debug_selector(id)`.

**Second**, the builders then went back through everything previously written off as
`NOT EXERCISED — blocked on display` and **reclaimed the entries they judged to be interaction
rather than appearance** — across `F-TAB`, `F-EDIT`, `F-TERM`, `F-CHG`, `F-PER`, `F-WIN` and
`F-AUTO`.

**Audit that reclamation.** It is the highest-risk bookkeeping event of the whole project: a large,
self-assessed reclassification, made by the people whose work it credits, under pressure from a
blockage everyone wants to be smaller than it is. You overturned exactly this kind of claim in
pass 4, when a builder counted 7 of 8 entries as state-backed and 5 of them were rendering.

The line to hold:

- **Behaviour** — clicking, typing, dragging, focusing, the element being present and hit-testable
  in the laid-out frame. Provable today.
- **Appearance** — colour, type, spacing, the comparison against waku, anything whose `VERIFY`
  clause turns on how it *looks*. Still blocked, still a debt.
- **Mixed** — prove the behaviour half, and the appearance half stays owed. A mixed entry silently
  counted whole is the failure to look for.

For each reclaimed entry, ask: does its `VERIFY` clause actually turn on seeing something, and does
the test that now claims it **drive the element in a drawn frame**, or does it call a handler
directly? A unit test on a callback is weaker than the socket transcripts this project already had,
and it must not be dressed as interaction proof.

Also check the other direction, since it costs nothing: **are there entries still marked blocked
that this harness could now reach?** The builders had an incentive to reclaim; they had none to go
looking for more work.

Then, if budget remains: `F-PERSIST-*` is being worked by a builder right now — avoid it. Prefer
finishing whatever remains unexercised in `01-inventory-app.md`.

## Where and how

```bash
cp -a /home/enzopalmisano/Scrivania/Progetti/tiller-linux /tmp/critic-pass7
source ~/.cargo/env && cd /tmp/critic-pass7/rust
cargo build -p tiller -p tiller_control
export TILLER_SOCKET=/tmp/critic7.sock
env -u DISPLAY -u WAYLAND_DISPLAY ./target/debug/tiller &
```

Working examples of the harness: the tests in `tiller_ui/src/changes.rs`. **Record the snapshot
time**, check whether a file has moved before filing against it, and **never call an X tool** —
they block forever here.

## Verdicts

**PASSED** · **FAILED** · **UNREACHABLE** (stated external reason) · **N/A — platform** ·
**NOT EXERCISED — blocked on display** (appearance only) · **half-proven**, with the reason.

Note the distinction a builder drew and check it holds: native Finder, file chooser, clipboard and
browser effects are **external-backend** limits, not display limits — a compositor restart would not
fix those, and they should not be filed as though it would.

## Method

- **Append, never rewrite**: `## PASS 7 — <area> — <time>` in `CRITIC-baseline.md`.
- **You do not fix anything.**
- Correct `INVENTORY-STATUS.md` where it misstates what you have exercised — it currently reflects
  the builders' reclamation, not yours.

## Reporting

Reply in **12 lines or fewer**: how many reclaimed entries you audited, how many you upheld and how
many you overturned with the reason, any entries still marked blocked that are actually reachable,
and **the single biggest gap that is not the display**.
