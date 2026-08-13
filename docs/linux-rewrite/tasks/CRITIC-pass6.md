# Critic pass 6 — the control and agent tiers of the package inventory

**You are pireview, pane `w1:p6`. You are the critic.** Your context was just reset, deliberately:
you judge this build without having seen any builder's reasoning. You built none of it.

> **A feature you have not successfully exercised does not exist.**

## Pass 5 found the defects that only exercising finds

You counted `F-GIT` yourself and reported **16 entries, not the 19 my brief claimed** — 8 PASSED,
2 PARTIAL, 6 FAILED for absent behaviour. Correcting the brief is always right; the file is the
contract and a number in a brief is somebody's recollection.

Then four disagreements between what the app reports and what git actually says, found by testing
the cases nobody tries:

- **A repository whose `.git` was removed renders as a clean `0/0/0` with `error=''`** while git
  says `fatal`. `load_snapshot` swallows every git error, so F-CHG-09's Retry path is dead code.
  Your sentence for it — *the surface lies exactly when git breaks* — is the clearest statement of
  this codebase's failure mode anyone has produced.
- No-HEAD staged files show `+0 −0` while their expanded diff shows real lines.
- **Stage on a conflicted path succeeds and stages the conflict markers**; the Swift original
  refuses.
- A missing `git` binary reads as "no HEAD".

All four are routed. Your `INVENTORY-STATUS.md` corrections are folded in, including F-CHG's state
tier as critic-confirmed.

Closed since you looked, both proven the way you would ask: **F-PER-01** — scrollback captured on
quit and replayed on restore, nonce surviving two relaunches — and the **project duplication**,
with `projects=1 worktrees=2` stable across three quit/relaunch cycles.

## Where and how

```bash
cp -a /home/enzopalmisano/Scrivania/Progetti/tiller-linux /tmp/critic-pass6
source ~/.cargo/env && cd /tmp/critic-pass6/rust
cargo build -p tiller -p tiller_control
export TILLER_SOCKET=/tmp/critic6.sock
env -u DISPLAY -u WAYLAND_DISPLAY ./target/debug/tiller &
```

**Record the snapshot time.** Before filing a finding, check whether the file it names has moved —
three builders are editing continuously. **Never call an X tool**; they block forever here.

## The piece

`docs/linux-rewrite/02-inventory-packages.md`, the two groups nobody has touched and no builder is
currently inside:

- **`F-CTRL-*`** — the control package: protocol shapes, request parsing, error responses,
  transport behaviour. Roughly 37 entries by my count; **count them yourself.** This is the tier the
  whole project now measures with, so a defect here is worth more than its inventory weight: every
  headless verdict in the last four hours rests on it.
- **`F-AGENT-*`** — the adapters: roughly 20 entries. `claude`, `codex` and `pi` are installed;
  **`opencode` and `omp` are not**, so entries naming them are **UNREACHABLE, not FAILED**. Note
  that adapters must write only worktree-local hook config and never touch user-global files like
  `~/.claude/settings.json` — if any adapter writes outside the worktree, that is a serious finding
  regardless of what the entry says.

Avoid `F-CORE-*` this pass — a builder is inside it and you would be judging a moving target.

Two things worth probing hard, because they are where a protocol lies quietly:

- **Malformed input.** Truncated JSON, an unknown method, a missing parameter, a wrong type, a huge
  payload. Each should produce a specific error, not a silent success and not a hang. You have
  already seen this codebase render failure as an ordinary empty state four times.
- **`system.capabilities` in both directions.** Does it advertise anything that does not work, and
  does anything work that it fails to advertise? The first misleads automation; the second hides
  capability. Both matter.

## Verdicts, used precisely

**PASSED** · **FAILED** · **UNREACHABLE** (stated external reason) · **N/A — platform** ·
**NOT EXERCISED — blocked on display**, never approximated, never ticked from reading the source.
Where an entry's substance is partly visual, say **half-proven** and why.

A builder's claim is a hypothesis — you overturned one in pass 4. So is a finding in your own
earlier passes.

## Method

- **Check the whole set before doubting the witness.**
- **Failing to reproduce is not evidence of absence.**
- **Append, never rewrite**: `## PASS 6 — <area> — <time>` in
  `<live worktree>/docs/linux-rewrite/CRITIC-baseline.md`.
- **You do not fix anything.**
- Correct `INVENTORY-STATUS.md` where it misstates what you have exercised.

## Reporting

Reply in **12 lines or fewer**: how many entries in each group and the counts, what malformed input
does, what `system.capabilities` gets wrong in either direction, any adapter writing outside its
worktree, and **the single biggest gap that is not the display**.
