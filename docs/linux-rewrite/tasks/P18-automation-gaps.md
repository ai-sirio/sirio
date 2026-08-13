# P18 — Close the five real automation gaps, and make the socket stop over-promising

**You are codex12, pane `w1:p3`.** Your context was just reset, so this brief is everything you
need.

**P17 was exactly right, including the part where you did nothing.** You re-exercised three
findings instead of assuming them:

- `panel.list` — **STALE**: app panes `pane-0`/`pane-1` are listed; the stale workspace selection
  had made it look blind.
- `panel.wait` — **STALE**: a 180 ms timeout returned in 0.18 s, and a separate pane returned exit
  code 7, so timeout and exit are properly distinguished.
- `notify` — **REAL**: raw title/body returned "notify requires session"; fixed at `main.rs:223`
  with a regression test, now returns `ok:true` and lists the notification.

Two accusations retracted with evidence and one genuine bug fixed is a better outcome than three
speculative repairs, and it saved you from "fixing" two methods the critic is currently depending
on.

## Where

Worktree: `/home/enzopalmisano/Scrivania/Progetti/tiller-linux` (branch `linux/gpui-waku`).
Rust/GPUI rewrite of Tiller, targeting Linux. Nothing is committed.

```bash
source ~/.cargo/env && cd /home/enzopalmisano/Scrivania/Progetti/tiller-linux/rust
cargo build -p tiller -p tiller_control      # tillerctl is a BINARY of tiller_control;
                                             # `-p tillerctl` fails and leaves a stale binary
env -u WAYLAND_DISPLAY DISPLAY=:1 GPUI_X11_SCALE_FACTOR=1 ./target/debug/tiller
```

`WAYLAND_DISPLAY` must be unset or GPUI ignores `$DISPLAY`. A capture with one distinct colour means
presentation failed, not that the UI is empty. **pi is in `tiller_ui/**` and `tiller_theme/**`,
codex11 is in `tiller_terminal/**`** — a build failure there is their work in flight, not yours.

`Scripts/ci.sh` is the macOS gate (it shells out to `xcodegen`) and **is not runnable on Linux**.
Do not treat its absence as a failure; `cargo test` and live transcripts are the gate here.

## What is left, from your own measurement

You exercised all nine entries of `## Control socket and automation surface` in
`docs/linux-rewrite/01-inventory-app.md` and four passed (F-AUTO-02, 03, 06, 07). These five did
not:

| Entry | What it wants | What is missing |
|---|---|---|
| F-AUTO-04 | `notify` + `session.ref` + `worktree.set`, then observe status/identity/association | `worktree.set` |
| F-AUTO-05 | workspace list / current / **select** / **create** / **close** | select, create, close |
| F-AUTO-08 | `session.restore` restores the launch snapshot | the method |
| F-AUTO-09 | `browser.*` when the universal workspace is on, **or explicit unsupported errors** | either |
| F-AUTO-01 | socket enable/disable **with the socket path displayed** in General settings | see below |

## The design constraint that matters most here

**`workspace.select` must run the same code path as clicking a worktree in the sidebar.** Not a
parallel implementation that happens to do the same thing — the same `select_worktree` you wrote for
P9b, called from a second entry point.

The reason is the bug you just spent a piece fixing. When two routes into the same state exist and
only one maintains it, the app develops exactly the disease F-009 had: a view that says one thing,
a socket that says another, and no way to tell which is lying. One source of truth, many doors into
it. Apply the same rule to `create` and `close`.

`session.restore` (F-AUTO-08) has the same shape: it must restore through whatever
`schedule_save`/session machinery already exists, not reconstruct the snapshot its own way.

## On F-AUTO-09 and honest refusals

The entry itself allows **"or explicit unsupported errors"**, so a browser surface that does not
exist is not automatically a failure — but silence is. Every `browser.*` method named in that entry
must answer with a clear, specific unsupported error rather than an unknown-method response or a
hang. Say what is unsupported and why.

While you are there, check the converse, because it is the same class of lie in the other
direction: **does `system.capabilities` advertise anything that does not work?** The capability list
lives at `main.rs:250`. A method named there that errors, or is missing, is worse than one that was
never advertised — automation trusts that list. This project already has a defect on the books where
provider badges say "Available" when they only mean "an adapter exists". Do not add another.

## On F-AUTO-01 — a split you must not resolve alone

The entry needs two halves: the socket genuinely honouring an enable/disable setting, and General
settings **displaying the enabled state and the socket path**. The first is yours. The second lives
in `rust/crates/tiller_ui/src/settings.rs`, **which is pi's file and which pi is editing right
now.** Build your half, expose what the UI needs to show it, and say in your reply exactly what the
settings row must display — it will be routed to pi. Do not edit `tiller_ui/`.

## Evidence this piece must produce

A live `tillerctl` transcript per method, pasted, against the running app — not a unit test
standing in for one. For `workspace.select` specifically, prove the single-source-of-truth
property: select through the socket, then show the **UI** agreeing (a screenshot, or
`workspace.current` plus the visible highlight). That cross-check between what the app paints and
what it reports is the strongest instrument this project has, and it is what caught F-009.

## Rules and ownership

- **Never copy code from the reference checkouts** (`../_tiller-refs/{waku,zed,orca,t3code}`).
  Every line of Tiller is written from scratch; transplanted code counts as a gap, always.
- **A capability nobody exercised does not exist.**
- You own `rust/crates/tiller_control/**` and `rust/crates/tiller/src/main.rs`. codex11 owns
  `tiller_terminal/**` and `tiller/src/panes.rs`; pi owns `tiller_ui/**` and `tiller_theme/**`.
  Need a change in someone else's file? Say so; it will be routed.
- The app dies silently every 4–13 minutes, no panic and no log. codex11 is hunting it. If it kills
  a run, note the time and retry — and if you happen to catch a death with useful state, say so,
  because that evidence is scarce.

## Reporting

Reply in **12 lines or fewer**: which of the five now pass with their transcripts, what
`system.capabilities` claims that is not true (if anything), what the F-AUTO-01 settings row needs
to display, and the honest remainder.
