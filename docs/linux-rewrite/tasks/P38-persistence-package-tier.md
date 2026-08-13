# P38 — The persistence package tier, and the crates nobody has judged

**You are codex11, pane `w1:p2`.** Your context was just reset, so this brief is everything you
need.

## P36 was a large piece done carefully

45 entries assessed: **36 PASSED · 0 FAILED · 4 N/A · 4 half-proven · 1 display-blocked.** You
implemented file and markdown handling, inotify watching, usage parsing and credentials,
project/worktree/layout/settings/UI helpers, terminal keys and splits, and skill installation across
`tiller_project`, `tiller_usage`, `tiller_markdown` and `tiller_terminal` — and said which crates
you touched, which keeps the ownership map true.

Two things beyond the count:

- **You accepted the critic's reclassification and applied it** — F-TERM-02/04/05/06/11 moved to
  rendering/display-blocked, F-TERM-01 state-backed, 03/07/08/09 half-proven, 10 unexercised — and
  updated `INVENTORY-STATUS.md` yourself. Taking a correction and propagating it without argument is
  worth more here than being right the first time.
- **You marked USG-06/07/08 and WSP-04 half-proven** for the honest reason: no live OAuth, no
  backend, no Claude PTY, no UI execution. Four entries you could have counted and did not.

You also hardened `ci-linux.sh` against both sharp edges the critic found — runtime and gitignored
paths excluded from the fingerprint, and a git checkout made an explicit precondition. `CI OK`.

## Where

```bash
source ~/.cargo/env && cd /home/enzopalmisano/Scrivania/Progetti/tiller-linux
./Scripts/ci-linux.sh
cd rust && export TILLER_SOCKET=/tmp/codex11.sock
env -u DISPLAY -u WAYLAND_DISPLAY ./target/debug/tiller &
```

No display presents. pi is in `tiller_ui/**`; codex12 is in `tiller_control/**`,
`tiller/src/main.rs` and `tiller_git/**`. **The critic is judging `F-CTRL-*` and `F-AGENT-*` right
now** — leave those alone so it is not judging a moving target.

## The piece

`docs/linux-rewrite/02-inventory-packages.md`, section **`## TillerPersistence`** — roughly 16
entries by my count. **Count them yourself**; two of my counts have been corrected by agents today
and both corrections were right.

This is the layer everything else has been trusting all day. Every persistence claim already made —
tabs and worktrees surviving a restart, the pane-event history replaying, bounded scrollback,
`session.ref` in SQLite, the migrated project catalogue — rests on it, and **none of those claims
tested the storage layer itself**. They tested that a round trip happened to work.

So probe where storage actually fails:

- **Schema migration.** Old rows written by an earlier shape are on disk right now — the project
  catalogue was migrated only hours ago. What happens to a database from before that migration? To
  one from a *future* schema? A silent wipe is the worst possible answer and the easiest to ship.
- **Corruption and truncation.** A half-written file, a database truncated mid-write, a file that is
  not SQLite at all. Each should produce a stated failure, not an empty state — this codebase has
  now produced five surfaces that rendered failure as ordinary emptiness, and the critic's sentence
  for it is *the surface lies exactly when git breaks*. The same question applies here: **could this
  empty result also mean the store is broken?**
- **Concurrency.** Several app instances run at once on this machine, all writing. WAL mode, locking,
  the debounced write path with `flush_now` on quit. Two instances quitting together should not lose
  one's state or corrupt the file.
- **Bounds.** Scrollback is capped at 256 KiB per pane. What caps the database as a whole?

Where behaviour is missing, build it. `tiller_persistence/**` is unowned — **claim it and say so**.

## Evidence

Real files, not mocks. Write a corrupt database and open it. Write a v-old schema and open it.
Truncate one mid-write. Run two instances against one store and quit both. For each, show what the
app did — and if it lost data, say so plainly; a data-loss finding you produce about your own layer
is worth more than one found later by someone else.

## Rules

- **Never copy code from the reference checkouts** (`../_tiller-refs/{waku,zed,orca,t3code}`).
  The Swift original named in each `SRC:` field is a reference for behaviour, never a source to
  translate.
- **A capability nobody exercised does not exist.**
- Verdicts: **PASSED** · **FAILED** · **UNREACHABLE** (stated reason) · **N/A — platform** ·
  **NOT EXERCISED — blocked on display** · and **half-proven**, with the reason, where an entry's
  substance is partly visual.

## Reporting

Reply in **12 lines or fewer**: how many entries and the counts, what happens to an old schema, a
corrupt store and two concurrent writers, which crates you claimed, and the honest remainder.
