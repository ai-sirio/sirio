# P37 — Doors for the git mutation tier, and a stage that must refuse

**You are codex12, pane `w1:p3`.** Your context was just reset, so this brief is everything you
need.

## P35 closed three things, and the evidence was the shape that was asked for

Project identity canonicalised, linked worktrees deduplicated, legacy rows migrated, `session.ref`
persisted in SQLite — and you took the routed F-PER-01 work as well, capturing bounded scrollback on
quit and replaying it on restore.

```
initial / relaunch 1 / 2 / 3 :  projects=1  worktrees=2
session.ref: agent-p35-nonce
before quit: F-PER-01_PROCESS_NONCE
after relaunch: F-PER-01_PROCESS_NONCE
after relaunch: F-PER-01_PROCESS_NONCE
```

Three cycles with a stable count is what distinguishes a fix from a coincidence, and the nonce
surviving two relaunches is what distinguishes a restored terminal from a fresh empty one. `CI OK`.

F-PER-01 was the gap the critic had named as the biggest one we could still fix. It is closed.

## Where

```bash
source ~/.cargo/env && cd /home/enzopalmisano/Scrivania/Progetti/tiller-linux
./Scripts/ci-linux.sh
cd rust && export TILLER_SOCKET=/tmp/codex12.sock
env -u DISPLAY -u WAYLAND_DISPLAY ./target/debug/tiller &
```

No display presents. pi is in `tiller_ui/**` building a text editor; codex11 is in
`tiller_terminal/**`, `tiller/src/panes.rs` and `tiller_activity/**`.

## What the critic found

It exercised `F-CHG` and the git tier against fixture repositories and reported **16 `F-GIT`
entries, not the 19 my brief claimed** — 8 PASSED, 2 PARTIAL, 6 FAILED for absent behaviour
(streaming runner, `branch --list`, clone, remote parsing, directory-status aggregation,
side-by-side). It counted; the brief was wrong.

Then it found four disagreements between what the app reports and what git actually says. **Two are
yours.**

### 1. Stage succeeds on a conflicted path and stages the conflict markers

The Swift original refuses this. Staging a file still carrying `<<<<<<<` / `=======` / `>>>>>>>`
commits a broken file into the index, and the app helped. `tiller_git` already knows about conflicts
— `StatusEntry::is_conflicted()` exists — so the guard has somewhere to live.

Refuse it, with an error a caller can act on. Then decide and state what "refuse" means for
`stage_all`: skipping conflicted paths silently is its own small lie; failing the whole call may be
worse. Whichever you choose, say why.

### 2. The whole `F-CHG` mutation tier has no socket door

`stage`, `unstage`, `discard`, `stage_all`, `discard_all` are verified inside `tiller_git` and
unreachable from outside, so with no display **nobody can exercise them at all.** The critic
predicted this hole sat next to the dead error path, and it does.

Add the methods, advertise them in `system.capabilities`, expose them through `tillerctl`. Same
constraint as `surface.changes.*`: **the method and the UI affordance call the same function.** A
mutation path that exists only for automation would drift from the button within a week, and then
the socket would prove something the user cannot do.

`tiller_git/**` is unowned — claim it for this piece and say so in your reply so the ownership map
stays true.

## What is being routed elsewhere, so you do not duplicate it

The other two disagreements are in `tiller_ui/src/changes.rs`, which is pi's:

- **F-CHG-09's error path is dead code.** A repository whose `.git` has been removed renders as a
  clean `0/0/0` with `error=''` while git says `fatal` — `load_snapshot` swallows every git error,
  so the Retry affordance can never appear. In the critic's words, **the surface lies exactly when
  git breaks**, which is the moment a user most needs it not to.
- No-HEAD staged files show `+0 −0` while their expanded diff shows real lines; a missing `git`
  binary reads as "no HEAD".

If your work uncovers that the fix belongs on your side of the seam — an error type that cannot
carry the distinction, say — write down exactly what pi needs rather than editing that file.

## Evidence

A live headless transcript: build a fixture repository with `git init`, create a genuine conflict
(two branches editing one line, then a merge), and show `stage` **refusing** it — then show a normal
file staging correctly through the socket and `git status --porcelain` confirming the index changed.
Same for `unstage` and `discard`. The check is always the app's answer against git's, not the app
against itself.

Then `./Scripts/ci-linux.sh`.

## Rules and ownership

- **Never copy code from the reference checkouts** (`../_tiller-refs/{waku,zed,orca,t3code}`).
  The Swift original is a reference for **behaviour** — it is why we know staging a conflict should
  be refused — never a source to translate.
- **A capability nobody exercised does not exist.**
- **Count entries yourself**; the brief's numbers are my reading, and the critic was right that 19
  was wrong.
- You own `tiller_control/**`, `tiller/src/main.rs`, project discovery, and now `tiller_git/**` if
  you claim it.

## Reporting

Reply in **12 lines or fewer**: which mutation methods now exist, what `stage` does with a conflict
and what you chose for `stage_all` and why, the fixture transcript with `git status` confirming
each mutation, anything routed to pi, the `ci-linux.sh` result, and the honest remainder.
