# P35 — Projects that breed, and a session association that forgets

**You are codex12, pane `w1:p3`.** Your context was just reset, so this brief is everything you
need.

## P31 closed the last two unreachable surfaces, with the cross-check that matters

`surface.changes.*`, `surface.settings.*`, `panel.state` and `panel.scrollback`, all advertised in
`system.capabilities` and exposed in `tillerctl`. 35 control integration tests, 59 UI tests, `CI OK`.

And you proved them the right way — **two independent sources agreeing**:

- the live Changes report against `git status --porcelain`: staged 1, changed 1, untracked 1, each
  `+1 -0`;
- the Settings badges against `which`: `claude`, `codex`, `pi` available; `opencode` and `omp`
  unavailable.

You also honoured the constraint that made those checks meaningful: the getters **read the mounted
`ChangesTab`/`Settings` state** rather than recomputing an answer. A socket that computes its own
result can agree with `git status` while the user's screen shows something else — that is this
codebase's characteristic failure, and you avoided it.

Because of this, the critic can now exercise both surfaces headless. That was the point.

## Where

```bash
source ~/.cargo/env && cd /home/enzopalmisano/Scrivania/Progetti/tiller-linux
./Scripts/ci-linux.sh
cd rust && export TILLER_SOCKET=/tmp/codex12.sock
env -u DISPLAY -u WAYLAND_DISPLAY ./target/debug/tiller &
```

No display presents. pi is in `tiller_ui/**` building a text editor; codex11 is in
`tiller_terminal/**`, `tiller/src/panes.rs` and `tiller_activity/**`.

## Two findings from the critic, both in your files

### 1. F-012 — projects breed on restore

The sidebar shows the same worktrees under several project rows, and the row count grows. The critic
found the mechanism:

- **`restore` re-discovers git worktrees under every project row** — and **a linked worktree's root
  is itself a repo**, so each discovered worktree becomes a project, which then discovers the
  others. The set grows every time.
- **`write_catalog` runs on create and close but never on quit**, and writes **two id conventions
  into one table**, so the persisted catalogue disagrees with itself about what identifies a row.

Worth knowing how this survived: it was **seen and dismissed** hours ago, in a screenshot showing
three project rows with identical worktree lists, on the reasoning that any checkout of a repo lists
all of that repo's worktrees — which is true, and was half the story. The tell was that the count
kept growing, and nobody counted. **A plausible explanation is not a verified one.**

What to fix: give a project a stable identity that survives a restore, stop re-discovery from
promoting a linked worktree's root into a new project, and settle on **one** id convention in that
table. Decide what happens to catalogues already written with the other convention — this database
exists on disk right now, so a migration or a tolerant read is part of the work, not a follow-up.

Then prove it the way the defect showed itself: **a real quit and relaunch cycle, repeated at least
three times, with the project and worktree rows counted after each.** The count must be stable. One
cycle cannot distinguish a fix from a coincidence.

### 2. `session.ref` is in-memory only

The critic exercised it and found the association does not survive. `F-AUTO-04` wants `notify`,
`session.ref` and `worktree.set` to produce an identity association that can then be *observed* —
an association that evaporates on restart satisfies the call and not the entry.

Persist it alongside whatever already survives correctly, and prove it across a real restart with a
nonce-bearing session id: set it, quit the process, relaunch, and read it back.

## Evidence

Both defects are provable headless and both need **real process exits**, not reloads. Paste: the row
counts across three restart cycles for F-012, and the set/quit/relaunch/read-back transcript for
`session.ref`. Then `./Scripts/ci-linux.sh`.

## Rules and ownership

- **Never copy code from the reference checkouts** (`../_tiller-refs/{waku,zed,orca,t3code}`).
  Every line of Tiller is written from scratch; transplanted code counts as a gap, always.
- **A capability nobody exercised does not exist.**
- You own `tiller_control/**` and `tiller/src/main.rs`. `tiller_project/**` is unowned — if project
  discovery lives there, it is yours for this piece; say so in your reply so the map stays accurate.
- Clean up the instances you start and the sockets you create.

## Reporting

Reply in **12 lines or fewer**: the mechanism you found for the duplication and what you changed,
the row counts across three restarts, what you did about catalogues already written with the other
id convention, the `session.ref` transcript, and the honest remainder.
