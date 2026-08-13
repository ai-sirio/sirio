# P36 — The rest of TillerCore: 45 more entries, all headless

**You are codex11, pane `w1:p2`.** Your context was just reset, so this brief is everything you
need.

## P33 was the cleanest piece of the day

26 entries assessed: **PASSED 19 · FAILED 0 · UNREACHABLE 3 · display-blocked 4.** 70 activity
tests, strict clippy and rustfmt, `CI OK`. A live demonstration with `claude`, `codex` and `pi`
actually launched, and `notify` round-tripping `running → needs-input → done`.

Two things in that report are why it can be trusted:

- **F12/F13/F15 marked UNREACHABLE for real `opencode`/`omp`** — not FAILED. The distinction is
  between "this build is wrong" and "this machine cannot answer", and collapsing it would have
  quietly turned three environment facts into three defects.
- **F02/F03/F04/F20 recorded as "state-tested but half-proven visually"** — the exact standard the
  brief asked for, applied without being reminded. Nineteen honest passes are worth more than
  twenty-three generous ones, and the difference only shows up months later when someone trusts the
  number.

## Where

```bash
source ~/.cargo/env && cd /home/enzopalmisano/Scrivania/Progetti/tiller-linux
./Scripts/ci-linux.sh
cd rust && export TILLER_SOCKET=/tmp/codex11.sock
env -u DISPLAY -u WAYLAND_DISPLAY ./target/debug/tiller &
```

No display presents. pi is in `tiller_ui/**` building a text editor; codex12 is in
`tiller/src/main.rs`, `tiller_control/**` and project discovery.

## The piece

`docs/linux-rewrite/02-inventory-packages.md`, the rest of `## TillerCore` — everything except the
`F-CORE-ACT` block you just finished:

| Group | Entries | About |
|---|---|---|
| `F-CORE-FILE` | 9 | file tree, watching, path handling |
| `F-CORE-USG` | 9 | usage and quota parsing |
| `F-CORE-DOM` | 8 | domain model invariants |
| `F-CORE-WSP` | 8 | workspace/worktree domain rules |
| `F-CORE-AUTH` | 3 | credential state |
| `F-CORE-TERM` | 3 | terminal domain rules |
| `F-CORE-SET` | 2 | settings domain |
| `F-CORE-UI` | 2 | UI-facing domain helpers |
| `F-CORE-PLAT` | 1 | platform |

**Count them yourself** — those totals are my reading of the file, and you were right about 11
versus 14 once already.

Domain logic needs no pixels by construction, so nearly all of this should be reachable. Take them
in the order above: `F-CORE-FILE` and `F-CORE-USG` are the largest and the most likely to hold real
defects, because both parse things the outside world produces — file trees that change under you,
and credential files whose shape is not ours to choose.

**Two known Linux traps in this territory**, so you do not rediscover them:

- The usage figures are parsed from `~/.codex/auth.json` and Claude's own state. That has already
  been made to work, so the entries are exercisable — but the *absence* of a file must produce a
  stated "unknown", never a confident zero. A quota display that reads 0% because it could not find
  the file is the same class of defect as a badge that says "Available" because an adapter exists.
- Path handling must be XDG on Linux, not `~/Library/Application Support` — that was fixed early
  (F-003) and is the kind of thing that creeps back in a new code path.

For each entry: **PASSED** (exercised, with evidence) · **FAILED** · **UNREACHABLE** (a stated
external reason — `opencode` and `omp` are not installed) · **N/A — platform** (macOS-only
machinery with no Linux subject) · **NOT EXERCISED — blocked on display**, and where an entry's
substance is partly visual, say **half-proven** and why, exactly as you did in P33.

Where behaviour is missing, build it. `tiller_activity/**` is yours; `tiller_usage/**`,
`tiller_project/**` and `tiller_persistence/**` are unowned — claim what you need for this piece and
**say in your reply which crates you touched**, so the ownership map stays accurate for whoever
comes next. If something belongs in `tiller_ui/**` or `tiller/src/main.rs`, specify it and it will
be routed.

## Evidence

Tests for the pure logic, and for anything that reads the outside world, a real file: write a
malformed `auth.json`, delete it, make it unreadable, and check each produces its own honest state
rather than a plausible number.

## Rules

- **Never copy code from the reference checkouts** (`../_tiller-refs/{waku,zed,orca,t3code}`).
  The Swift original named in each `SRC:` field is a reference for **behaviour**, never a source to
  translate line by line.
- **A capability nobody exercised does not exist.**

## Reporting

Reply in **12 lines or fewer**: how many entries you assessed and the counts per verdict, which
crates you touched, what you built, any half-proven entries and why, and the honest remainder.
