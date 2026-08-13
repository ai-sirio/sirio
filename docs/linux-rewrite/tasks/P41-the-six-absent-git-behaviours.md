# P41 — The six git behaviours that are simply not there

**You are codex12, pane `w1:p3`.** Your context was just reset, so this brief is everything you
need.

## P40 closed the worst defect in the tree

The process leak is fixed — *"both gone"* — with worktree-scoped cleanup, and you documented
`worktree.set`'s metadata as **runtime-only** rather than leaving a comment that claimed a
persistence the code did not have. Correcting a lying comment is worth as much as the fix: the next
reader believes comments and stops checking.

38 control integration tests. You also reported honestly that full CI is red from **pre-existing
compilation errors in `tiller_ui`** — that is pi mid-piece in its own crate, not your work.

Why that fix mattered beyond its entry: this machine had been accumulating stray processes for
hours, and it was blamed on the compositor, on agents forgetting to clean up, and on the wedged
display. Some of it was Tiller leaking a process group on every close.

## Where

```bash
source ~/.cargo/env && cd /home/enzopalmisano/Scrivania/Progetti/tiller-linux
cargo test -p tiller_git -p tiller_control
./Scripts/ci-linux.sh     # may be red from tiller_ui while pi is mid-piece; check whose error it is
```

pi is in `tiller_ui/**`; codex11 is in `tiller_persistence/**`, `tiller_terminal/**` and
`tiller/src/panes.rs`. **`tiller_git/**` is yours.**

## The piece

The critic exercised the `## TillerGit` section of `02-inventory-packages.md` — **16 entries, not
the 19 an earlier brief claimed** — and found **8 PASSED, 2 PARTIAL, 6 FAILED for absent
behaviour.** Those six are this piece. Each one's `VERIFY` clause is the specification; read them in
the file rather than from this summary.

| Entry | What is missing |
|---|---|
| `F-GIT-RUN-02` | the **streaming** runner: stderr lines split on CR/LF *while the command runs*, with the final result reported after completion |
| `F-GIT-BRANCH-01` | branch listing from `git branch --list` — the entry specifically wants **branch names containing spaces** handled |
| `F-GIT-CLONE-01` | clone with progress: parse `Receiving objects` percentage, report completion or failure **through the streaming runner** |
| `F-GIT-REMOTE-01` | remote parsing: GitHub owner from **SSH or HTTPS** URLs, project name derived by stripping a trailing slash and `.git` |
| `F-GIT-STATUS-02` | directory status aggregation to ancestors, with **conflict outranking changed, and changed outranking untracked** |
| `F-GIT-DIFF-03` | side-by-side pairing: pair context lines, **zip deletion/addition runs**, preserve full-width hunk context, omit metadata lines |

Two notes so you aim correctly:

- **`F-GIT-DIFF-03` is data, not a view.** Pairing and zipping produce a structure; rendering it is
  someone else's problem. Build the transformation and test it on real diffs.
- **`F-GIT-RUN-02` is the foundation of `F-GIT-CLONE-01`.** Do the streaming runner first; clone is
  its first real consumer, and building clone on a non-streaming runner would have to be redone.

While you are in there: `F-GIT-ACT-02` also asks that mutation validation reject **stale and
duplicate** paths, not only conflicted ones. You did conflicted in P37 — check whether the other two
are covered and finish them if not.

And `F-GIT-PLAT-01` records that the package declares macOS 15 while its git subprocess and
filesystem work is cross-platform. On Linux that is either `N/A — platform` or a manifest that
should say so; decide, and say which.

## Evidence

Real repositories built with `git init`, not fixtures in memory:

- branches whose names contain spaces, and one that does not;
- a clone from a **local path** so it is fast and offline — progress parsing needs real output;
- SSH and HTTPS remotes, with and without `.git`, with and without a trailing slash;
- a nested tree where one file is conflicted, one changed and one untracked under the **same**
  ancestor, so the precedence rule is actually exercised rather than assumed;
- a diff with a rename, a binary, and a large unchanged middle.

For the streaming runner, prove it **streams**: show a line arriving before the process exits, not
just the final buffer. Otherwise it is the old runner with a new name.

## Rules and ownership

- **Never copy code from the reference checkouts** (`../_tiller-refs/{waku,zed,orca,t3code}`).
  The Swift original named in each `SRC:` field is the reference for **behaviour** — it is how we
  know the precedence order and the escaping rules — never a source to translate.
- **A capability nobody exercised does not exist**, and — from P40 — **ask what the smallest true
  version of your evidence is.** "The child is gone" was true and one process too narrow.
- **Count the entries yourself.** Two of my counts have been corrected today and both corrections
  were right.
- Proceed without asking for design approval; the acceptance criteria above are the gate.

## Reporting

Reply in **12 lines or fewer**: which of the six now work with the transcript for each, proof that
the streaming runner streams, what you found for `F-GIT-ACT-02`'s stale and duplicate paths, your
decision on `F-GIT-PLAT-01`, and the honest remainder.
