# P21 — Mount the Changes surface. It exists, it works, and the app never shows it.

**You are codex12, pane `w1:p3`.** Your context was just reset, so this brief is everything you
need.

**P18 is closed, all five of them, with evidence.** `worktree.set` + `session.ref` + `notify` giving
a real identity association; workspace list/current/select/create/close with the UI selection
confirmed in a screenshot; `session.restore` bringing back the worktree and its Chat/Terminal panes;
every `browser.*` answering with an explicit Linux-unsupported error and none of them advertised;
socket enable/disable with `socketEnabled` and `socketPath` in capabilities. 70 tests, rustfmt and
diff checks clean. You also shut down your `:2` instance and removed your stale socket afterwards —
on a shared display that is not housekeeping, it is what keeps other agents' evidence trustworthy.

The settings-row spec you wrote (`Control socket — Enabled/Disabled`, secondary `Socket path: …`)
is recorded and will be routed to pi, who owns that file.

## Where

Worktree: `/home/enzopalmisano/Scrivania/Progetti/tiller-linux` (branch `linux/gpui-waku`).

```bash
source ~/.cargo/env && cd /home/enzopalmisano/Scrivania/Progetti/tiller-linux/rust
cargo test -p tiller && cargo build -p tiller -p tiller_control
env -u WAYLAND_DISPLAY DISPLAY=:2 GPUI_X11_SCALE_FACTOR=1 ./target/debug/tiller
```

**`:2` is the only display on this machine that still presents** (8820 colours at 1440x833). Every
display created after ~09:55 comes back blank — software Vulkan and `MESA_VK_WSI_DEBUG=sw` included,
all tested, do not spend budget on it. `:2` is shared and the critic has priority there: keep runs
short and close your instance when you are done, as you did last time.

pi is in `tiller_ui/**` and `tiller_theme/**`; codex11 is in `tiller/src/panes.rs` and
`tiller_terminal/**`.

## The gap, and it is the biggest one on the board

The critic — which did not build any of this — put it plainly:

> The git Changes surface is not wired into the app. Tiller is a git workspace, and right now a user
> cannot see staged/untracked files, expand a diff, stage, or discard from the running build.

Verified independently before writing this brief:

```
$ grep -rn "ChangesTab" crates/ --include=*.rs | grep -v crates/tiller_ui/src/changes.rs
crates/tiller_ui/src/bin/changes_preview.rs:7:  use tiller_ui::changes::ChangesTab;
crates/tiller_ui/src/bin/changes_preview.rs:26: |_, cx| cx.new(|cx| ChangesTab::new(cwd, cx)),
```

**The only thing in this repository that mounts `ChangesTab` is a demo binary.** The app's right
panel is Files and Activity; there is no Changes anywhere in it.

And the component is not a stub. It is ~900 lines that already do the work: three sections (Staged /
Changed / Untracked) with a file correctly appearing in two at once, per-file `+N -M` counts,
collapsed-context bands labelled with their own size, Stage / Unstage / Discard per row, Stage All /
Discard All, all backed by real `tiller_git` calls. It has 38 passing tests. It renders. Nobody can
reach it.

F-CHG-07 through F-CHG-18 all fail for this one reason, and the critic counts roughly **20 inventory
entries** restored by wiring it in.

### Name the pattern, because this is the third time

- The sidebar's `SelectWorktree` arm: added, compiled, did one of the four things asked (F-009).
- The Changes surface: built, tested, unmounted.

Each time the work was real and the tests were green. **Green tests prove a component behaves; only
mounting it proves a user can reach it.** A crate that compiles and a binary that runs are different
claims, and this project keeps discovering the gap between them the expensive way.

So this piece ends with a guard: **a test that the application shell can actually construct and
mount a Changes surface.** Not a test of `ChangesTab` — pi has 38 of those — but a test at the shell
level that fails if the surface stops being reachable. That is the assertion that would have caught
this on the day it happened.

## What to build

**Changes is a first-class tab, not a right-panel section.** That decision is already recorded in
`docs/linux-rewrite/04-ux-patterns-waku-does-not-cover.md`: waku has no diff surface, so the
reference is orca, where changes open as their own tab (`All Changes`) in the same strip that holds
conversations — a diff is a peer of a chat, not an inspector. The Swift original put it in the right
inspector; orca's placement was judged the stronger idea and cheaper to adopt while the shell is
being rebuilt anyway. Read that file before deciding the shape.

So:

1. A Changes tab kind that the workspace can create, activate, close and persist like any other tab.
2. A way to open it that a user can find — the new-tab affordance is the natural home. **Note the
   critic also found the `+` new-tab button does not render at all**; if that is shell-side, it is
   yours and it belongs in this piece, because a tab nobody can open is the same bug again.
3. The tab is bound to the **currently selected worktree**, and follows it. You built
   `select_worktree` in P9b — reuse it; do not add a second route into that state.
4. Tab persistence across restart, matching what already works for Chat and Terminal (the critic
   verified two clean restart cycles preserving tabs, active tab and worktrees — keep that true).

**Do not modify `crates/tiller_ui/src/changes.rs`** — it is pi's, and it is finished. If mounting it
needs an API it does not expose, say exactly what in your reply and it will be routed.

## Evidence this piece must produce

1. `cargo test -p tiller` green, including the new shell-level mount test.
2. **A live screenshot on `:2`** of the Changes tab open in the running app against a repository with
   one staged file, one modified-unstaged file and one untracked file at the same time — build that
   repo yourself with `git init` and add it as a project.
3. **Stage and discard exercised live through the UI**, with `git status --porcelain` captured
   before and after as the oracle. That cross-check between what the app does and what git reports
   is the strongest instrument this project has.

A passing test is not a substitute for the screenshot. The whole point of this piece is that tests
were already passing while the feature did not exist for any user.

## Rules and ownership

- **Never copy code from the reference checkouts** (`../_tiller-refs/{waku,zed,orca,t3code}`).
  orca is the source of the *idea* — write every line yourself. Transplanted code counts as a gap.
- **A capability nobody exercised does not exist.**
- You own `tiller_control/**` and `tiller/src/main.rs`.
- The app freezes its rendering every 4–13 minutes — see the note below. If a run freezes, restart
  it and say so.

## One correction to what you may believe about the crash

It is **not** a process death. The critic established that the process and the control socket stay
alive while the window pixmap stops changing entirely — a presentation freeze, not a crash. So if
your app instance stops responding visually, `tillerctl` will still answer. That is diagnostic
information, not a reason to abandon the run.

## Reporting

Reply in **12 lines or fewer**: how Changes is mounted, how a user opens it, the test count including
the mount guard, the screenshot path, the before/after `git status --porcelain` around a live stage
and discard, and the honest remainder.
