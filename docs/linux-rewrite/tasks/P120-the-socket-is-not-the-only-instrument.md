# P120 — the socket is not the only instrument

**Owner: assigned on dispatch — not `pi`.** Worktree
`/home/enzopalmisano/Scrivania/Progetti/tiller-linux`, branch `linux/gpui-waku`.

`pi` drove `P116` Slice C honestly and well: it exercised a handful of rows with real evidence,
and for the rest it wrote, correctly, that **no control method exists** to drive them. It then
refused to infer, which is the behaviour this project wants — `F-TERM-UI-02`'s "this avoids treating
a socket transcript as a platform-click result" is exactly right.

But "the socket cannot drive it" is not "it cannot be driven." **Slice C was my slicing error.** I
put rows about file-icon glyphs, status-bar segments and provider tooltips into a headless slice
because of their `F-CORE-*` prefixes. They need a rendered display, or a different instrument
entirely. Re-driving them the same way would only reproduce the same answer.

`pi` must not take this task — it already formed a conclusion on every row here, and confirming your
own prior is how an independent control stops being one.

## The rows

```
F-CORE-ACT-19  F-CORE-ACT-20  F-CORE-ACT-24  F-CORE-ACT-25  F-CORE-ACT-26
F-CORE-FILE-03 F-CORE-FILE-06 F-CORE-FILE-08
F-CORE-USG-05  F-CORE-USG-06  F-CORE-USG-07
F-CORE-DOM-03  F-CORE-SET-01  F-CORE-AUTH-01
F-AGENT-API-01 F-AGENT-SAFE-02
F-GIT-RUN-02   F-TERM-UI-02
F-USE-01 F-USE-02 F-USE-03
```

`F-AGENT-SESSION-01` and `-02` are **held** — they turn on whether restore works at all, which is
`P119`. `F-AGENT-OMP-01/02` stay out: `ENVIRONMENT.md` documents the upstream instrument failure and
says not to re-drive them.

Read `P116-report.md` §Slice C first. It tells you precisely what was already tried and rejected, so
you never repeat it. Its captures are committed under `reference/linux-progress/p116-slice-c/`.

## Four instruments that are not the control socket

This is the whole point of the task. For each row, pick the instrument that can actually observe it.

1. **The rendered Wayland lane.** It takes no lock, runs any number in parallel, and since `P112`
   does `click`, `move`, `type` and named keys. Anything about the status bar, a tooltip, a file
   icon or a terminal link needs a drawn frame. Read `WAYLAND-LANE.md` first.
2. **The filesystem.** `F-CORE-FILE-06` is about the Changes surface reacting to an *external*
   modify/delete/rename. No control method needs to exist — **edit the file with `bash`**, then read
   the surface back. Same for anything about files appearing or disappearing under a worktree.
3. **The database.** `/tmp/<label>.sqlite` is plain SQLite. `F-CORE-SET-01` names malformed settings
   data: write the malformed row yourself, restart, and see what the app does. A row about recovery
   is drivable the moment you are willing to break something on purpose.
4. **A real agent in a real pane.** `F-AGENT-SAFE-02` is about `prepare` never touching user-global
   config. Launch an agent in a terminal pane and check `~/.claude/settings.json`, `~/.codex/config.toml`
   and friends — **record their `sha256sum` before and after.** That is a stronger proof than any
   socket reply, and it is the row's actual claim.

## When a row really is unreachable

Some of these may be genuinely unobservable from outside — internal policy like
`AgentSessionRestorePlan`, `BootstrapRestoreOrder` or mount eviction may have no user-facing
surface at all.

**That is a legitimate outcome and it has its own verdict: `UNREACHABLE`.** Say which instrument you
tried, why it cannot see the behaviour, and what would be needed. Do not convert an unreachable row
into a `PASSED` by reading the source, and do not leave it `NOT EXERCISED` as if nobody had looked.

## The standard, unchanged

- **A recipe says what to DO, never what to CONCLUDE.**
- **A row you did not successfully exercise does not exist.** Reading the code is how you find the
  control, never how you close the row.
- **Conjunctions split.** Drive each conjunct and report it separately; the second one gets skipped.
- **A green test is not the proof.** Today 134 tests passed over a chat transcript that draws
  nothing — `CRITIC-visual-baseline.md`, 17:40.
- **Write captures into `reference/linux-progress/p120/`, not `/tmp`.** A proof in `/tmp` is deleted
  by the next pass and the verdict it supports silently becomes unreplayable.

## What to produce

`docs/linux-rewrite/P120-report.md`, committing every 10 rows. Per row: the instrument you chose,
what you drove, what it actually did, and the capture path. **No verdict column** — `pireview` sets
verdicts, except that you should say plainly when you believe a row is `UNREACHABLE` and why.

## The rules

- **Do not edit `INVENTORY-LEDGER.md`.** Do not edit `rust/` — a bug you find is a finding.
- Commit path-scoped, never `git add -A`. `grep '??'` before calling a row done. Expect
  `index.lock` contention with six panes committing: **retry, never delete the lock.**
- Do not idle on an approval gate — `ENVIRONMENT.md` §"Working with the orchestrator".
