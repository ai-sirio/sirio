# Critic pass 8 — sweep what remains unjudged in the app inventory

**You are pireview, pane `w1:p6`. You are the critic.** Your context was just reset, deliberately:
you judge this build without having seen any builder's reasoning. You built none of it.

> **A feature you have not successfully exercised does not exist.**

## Pass 7 was the most valuable pass of the project

You were asked to audit a large self-assessed reclassification, and you found what it was hiding:

> **The `F-TAB` mega-row — 21 entries under one claim, 13 absent.** The reclassification ran on
> lists, not per-entry checks. **The harness made interaction provable and absence hideable in the
> same stroke.**

That last sentence is the finding. A new capability arrived, everyone correctly saw that it made
interaction testable, and in the same motion thirteen entries for features **that do not exist**
moved out of "blocked" and into something that reads like progress. Nobody lied; the check was done
per list instead of per entry, and the difference is invisible unless somebody opens each one.

You also checked the direction nobody had an incentive to check — entries still marked blocked that
are actually reachable — and found four: `F-SET-03`'s update click, `F-CHG-20`'s running count,
`F-TAB-15`'s close control, and `F-SET-21`, whose behaviour half already had a drawn test the audit
had missed.

And you filed a test-health finding that is worth more than several entries: **the drawn mutation
tests need the full `run_until_parked()` and hardened pump before anything resting on them is
ticked.** A test that passes on timing luck is worse than one that fails, because it certifies an
entry that may not hold. That has been routed as blocking.

## Where and how

```bash
cp -a /home/enzopalmisano/Scrivania/Progetti/tiller-linux /tmp/critic-pass8
source ~/.cargo/env && cd /tmp/critic-pass8/rust
cargo build -p tiller -p tiller_control
export TILLER_SOCKET=/tmp/critic8.sock
env -u DISPLAY -u WAYLAND_DISPLAY ./target/debug/tiller &
```

**Record the snapshot time.** Check whether a file has moved before filing against it. **Never call
an X tool** — they block forever here.

For interaction, GPUI's own harness works with no display: `TestAppContext` with
`VisualTestContext`, elements found via `.debug_selector(id)`, `simulate_keystrokes`, drags via real
mouse down/move/up, double-clicks via `click_count`. Working examples are in
`tiller_ui/src/changes.rs`.

## The piece

Sweep what is still unjudged in **`01-inventory-app.md`**, applying the lesson of pass 7: **per
entry, not per list.** For each, three separate questions, in this order:

1. **Does the feature exist at all?** If not, it is **FAILED — absent**, and no amount of harness
   makes it otherwise. This is the question the mega-row skipped.
2. If it exists, **can it be exercised here** — behaviour through the harness or the socket?
3. If exercised, **does it do what the entry's `VERIFY` clause says**, not what it plausibly does?

Areas still owing a judgement, roughly in order of size:

- **`F-TAB`** — 28 entries, and you have already established that 13 are absent. Nail down exactly
  which, so the build list is precise rather than a count.
- **`F-WIN`** — window and application shell, 12 entries. Much of it is appearance, but some is
  behaviour: window restoration, close handling, menu commands.
- **`F-BRW`** — browser, 9 entries. The surface is unimplemented and answers explicit unsupported
  errors, which the entry text permits — confirm the errors are actually explicit and specific
  rather than a generic unknown-method.
- **`F-SID`, `F-SET`, `F-USE`, `F-CHAT`** — judged early, in some cases at 01:00, and the app has
  been rebuilt around them since. Spot-check the ones whose surfaces changed most; a PASSED verdict
  from thirteen hours ago is a hypothesis with a timestamp.

A builder is working `tiller_acp` and `tiller_agents` right now — **avoid `F-CHAT`'s protocol tier**
so you are not judging a moving target, but the chat *surface* is fair game.

## Verdicts

**PASSED** · **FAILED — absent** (the feature is not there) · **FAILED — defective** (it is there and
wrong; keep these two distinct, they need different work) · **UNREACHABLE** (stated external reason;
`opencode`/`omp` absent) · **N/A — platform** · **NOT EXERCISED — blocked on display** (appearance
only) · **half-proven**, with the reason.

Native Finder, file chooser, clipboard and browser effects are **external-backend** limits, not
display limits — a compositor restart would not fix them.

## Method

- **Append, never rewrite**: `## PASS 8 — <area> — <time>` in `CRITIC-baseline.md`.
- **You do not fix anything.**
- Keep `INVENTORY-STATUS.md` honest; your pass-7 corrections are the current truth there.

## Reporting

Reply in **12 lines or fewer**: how many entries you judged and the counts, **exactly which `F-TAB`
entries are absent**, whether the browser's unsupported errors are specific, which old PASSED
verdicts no longer hold, and **the single biggest gap that is not the display**.
