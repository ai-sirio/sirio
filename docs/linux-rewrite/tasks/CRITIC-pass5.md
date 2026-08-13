# Critic pass 5 — the Changes surface and the git tier, now that both are reachable

**You are pireview, pane `w1:p6`. You are the critic.** Your context was just reset, deliberately:
you judge this build without having seen any builder's reasoning. You built none of it.

> **A feature you have not successfully exercised does not exist.**

## Pass 4 was the pass that made the loop worth running

Ten judgements, ~24 PASSED, and — the part that matters — **you overturned a builder's claim.** It
reported 7 of 8 blocked `F-TERM` entries as state-backed with exactly 1 genuinely pixel-only; you
exercised them and ruled that **F-TERM-02, 04, 05, 06 and 11 are rendering entries**, and recorded
it as *pixel entries counted as state-backed*. That is precisely the failure mode a dead display
pushes everyone toward, and nobody else was positioned to catch it. It has been routed back.

You also judged the gate itself rather than trusting it: `ci-linux.sh` is honest — header disclaims
visuals, drift reported separately, the nonce smoke works — **and it correctly refused `CI OK` on a
real compile break in the live tree.** Then you found two sharp edges in it: the fingerprint hashes
`.remember/` runtime files that an ambient hook rewrites, producing false drift failures twice, and
the smoke test silently requires a git checkout at `ROOT`. Both routed.

And you named the biggest fixable gap correctly: **F-PER-01**, where the capture/replay and the
store both exist and nothing joins them — *"the same crate-right, shell-unwired shape as the Changes
surface and badges."* That is the fifth instance of this codebase's characteristic failure, and you
identified it as a pattern rather than as one more bug.

## Where and how

Live tree: `/home/enzopalmisano/Scrivania/Progetti/tiller-linux` (branch `linux/gpui-waku`).

```bash
cp -a /home/enzopalmisano/Scrivania/Progetti/tiller-linux /tmp/critic-pass5
source ~/.cargo/env && cd /tmp/critic-pass5/rust
cargo build -p tiller -p tiller_control
export TILLER_SOCKET=/tmp/critic5.sock
env -u DISPLAY -u WAYLAND_DISPLAY ./target/debug/tiller &
./target/debug/tillerctl current-workspace
```

**Record the snapshot time at the top of your section**, and before filing a finding check whether
the file it names has moved since — three builders are editing continuously and two of your pass-2
findings were already fixed when you filed them.

**Never call an X tool.** `xdpyinfo`, `import`, `xwininfo`, `xprop`, `xdotool` block forever on this
machine; that cost you 1h26m once. If you must, `timeout 10` it and treat the timeout as a stop.

## The piece

Two areas are now reachable that were not before, and together they are the largest block of
unverified inventory that does not need pixels.

### 1. `F-CHG` — files, changes and activity (22 entries, `01-inventory-app.md`)

Until recently the Changes surface was mounted by nothing but a demo binary. It is now a
first-class Diff tab, and since `surface.changes.*` exists you can read what it is actually showing
without a display.

Build your own fixture repository with `git init` — **one staged file, one modified-unstaged file,
one untracked file at once**, plus a file with a large unchanged middle so the collapsed-context
bands have something to collapse, and a rename. Then work the entries.

The instrument that matters here is **disagreement between two sources**: the surface's own report
versus `git status --porcelain` and `git diff --numstat` on the same repository. A builder already
demonstrated them agreeing for a simple case; your job is the cases it did not try — a file both
staged *and* modified appearing in two sections, a rename, a binary file, an empty repository, a
repository with no HEAD.

Stage, unstage and discard through the socket where methods exist, and check the effect **in git**,
not only in the app's answer.

### 2. `F-GIT` — the git tier of `02-inventory-packages.md` (19 entries)

These fell to UNREACHABLE in an early pass because the snapshot had no `.git`. `cp -a` preserves the
worktree pointer, and your fixture repository removes the excuse entirely. Worktree creation,
removal, parent-directory resolution, status parsing, diff parsing, stage/unstage/discard — all
domain logic over a real repository, all headless.

If budget remains, continue into the other untouched package groups: `F-CORE-FILE`, `F-CORE-USG`,
`F-AGENT-*`, `F-PERSIST-*`, `F-CTRL-*`. Note that a builder is working `F-CORE-*` right now, so
prefer `F-AGENT-*`, `F-PERSIST-*` and `F-CTRL-*` to avoid judging a moving target.

## Verdicts, used precisely

**PASSED** (exercised, with evidence) · **FAILED** · **UNREACHABLE** (stated external reason —
`opencode` and `omp` are not installed) · **N/A — platform** · **NOT EXERCISED — blocked on
display**, never approximated and never ticked from reading the source.

**A builder's claim is a hypothesis**, as pass 4 demonstrated. So is a finding in your own earlier
passes: several have been retired by looking, in both directions, and both are honest outcomes.

## Method

- **Check the whole set before doubting the witness.**
- **Failing to reproduce is not evidence of absence.**
- **Append, never rewrite**: add `## PASS 5 — <area> — <time>` to
  `<live worktree>/docs/linux-rewrite/CRITIC-baseline.md`.
- **You do not fix anything.** No edits to any `rust/**` source.

There is also now a consolidated `docs/linux-rewrite/INVENTORY-STATUS.md`, which separates
critic-confirmed entries from builder-claimed ones. **Only your verdicts move an entry into
critic-confirmed.** If it misstates anything you have exercised, say so in your reply.

## Reporting

Reply in **12 lines or fewer**: how many `F-CHG` and `F-GIT` entries you exercised and the counts,
the disagreements you found between the surface and git, anything in `INVENTORY-STATUS.md` you would
correct, and **the single biggest gap that is not the display**.
