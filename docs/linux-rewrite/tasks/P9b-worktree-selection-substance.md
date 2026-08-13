# P9b — Selecting a worktree must change the app, not just the highlight

**You are codex12, pane `w1:p3`.** Your context was just reset, so this brief is everything you
need. Nothing here assumes you remember anything.

**First, the credit that is due.** You just landed P11 — settings persistence — and you proved it
the right way: Appearance → Dark, kill the process, restart, Dark restored, plus the SQLite row and
a screenshot. That is exactly the standard this project runs on, and it closes a defect the critic
had marked FAILED. This brief is not a complaint about that work.

## Where

Worktree: `/home/enzopalmisano/Scrivania/Progetti/tiller-linux` (branch `linux/gpui-waku`).
Rust/GPUI rewrite of Tiller, targeting Linux. Nothing is committed.

```bash
source ~/.cargo/env && cd /home/enzopalmisano/Scrivania/Progetti/tiller-linux/rust
cargo build -p tiller -p tiller_control      # tillerctl is a BINARY of tiller_control;
                                             # `-p tillerctl` fails and leaves a stale binary
env -u WAYLAND_DISPLAY DISPLAY=:1 GPUI_X11_SCALE_FACTOR=1 ./target/debug/tiller
```

`WAYLAND_DISPLAY` must be unset or GPUI ignores `$DISPLAY`. Only the session's own Xwayland (`:1`)
has DRI3, which Vulkan needs to present on X11; a capture with one distinct colour means
presentation failed, not that the UI is empty.

**pi is editing `rust/crates/tiller_ui/**` and `tiller_theme/**` right now.** If `cargo build`
fails inside `tiller_ui`, that is pi's change in flight, not your bug — wait and retry, or `cp -a`
the tree aside. **Do not edit anything under `tiller_ui/` this round**, including `settings.rs`:
you needed it for P11, it is pi's file, and pi is in that crate now.

## The defect

The sidebar's worktree selection is still cosmetic. This is the critic's second-strongest finding,
and it survived the last change — which is worth understanding, because the change *looked* done.

What exists now, and it is real work: `SidebarEvent::SelectWorktree(PathBuf)` is emitted when a
worktree row is clicked (`tiller_ui/src/sidebar.rs:1087`), and `main.rs:844` handles it:

```rust
SidebarEvent::SelectWorktree(path) => {
    workspace.sidebar.update(cx, |sidebar, cx| sidebar.set_selected_worktree(path, cx));
}
```

The match is exhaustive, the build is green, and the highlight moves. **And nothing else in the
program knows anything happened.** The shell receives the click and hands it straight back to the
sidebar; the round trip ends where it started.

Specifically — verified by reading, not guessed:

- `ControlState::current` (`main.rs:102`) is an `Option<usize>` index into `workspaces`.
- It is assigned **exactly once**, at `main.rs:123`, from `workspaces.iter().position(|w| w.selected)`
  when the state is first built from persistence.
- **No code path writes it again.** So `workspace.current` over the control socket — and therefore
  `tillerctl` — keeps reporting the worktree that was selected at startup, forever.

The status bar's branch/path and the open tabs' home worktree have the same problem for the same
reason: nothing recomputes them on selection.

**This is why "it compiles" is not evidence.** The previous step was handed over as a
compiler-enforced interface change: the new enum variant made `main.rs`'s match non-exhaustive, so
the build broke until an arm existed. That mechanism guarantees the arm exists. It cannot check that
the arm *does* anything, and here it does one of the four things it was asked to do.

## What to build

The four requirements, from the handoff in `docs/linux-rewrite/PI-HANDOFF.md` §4 — read it, it is
the original spec:

1. **The control state follows the selection.** Selecting a worktree updates `ControlState::current`
   (and the `selected` flags on the rows), so `workspace.current` over the socket answers with the
   worktree the user actually chose.
2. **The status bar re-derives** its branch and path from the newly selected worktree.
3. **The open tabs re-home** under the right worktree.
4. **The sidebar is answered, not obeyed** — keep `set_selected_worktree`, but as the *host's
   confirmation* after the state has actually changed. The highlight should be the shell reporting
   what is true, never the sidebar deciding for itself. That ordering is the whole point of the
   design: one source of truth, and the view shows it.

Persist the choice if the app already persists worktree selection — check what `schedule_save`
writes before deciding, and say in your reply which way you went and why.

## Acceptance — paste this in your reply

Not a description of it. The actual terminal output:

1. `tillerctl` reporting the current workspace **before** the click.
2. The same command **after** clicking a different worktree in the sidebar.
3. A screenshot showing the highlight in that same state.

All three must agree. If they do not, the piece is not done — say so rather than reporting it as
finished. `Scripts/linux-drive.sh <out.png> '<actions>' [settle] [display]` will launch, click and
capture in one run; it exposes `click x y` (window-relative), `type "text"`, `key <keysym>`,
`shot [path]`. Input must use absolute coordinates and plain XTEST — `xdotool --window` targeting
delivers nothing here, because the app is an X11 guest of a Wayland compositor.

A unit test on the state transition is welcome and is **not** a substitute for the live proof.

## Two things that will happen while you work

- **The app dies silently every 4–13 minutes**: no panic, no log. Five deaths observed across two
  builds and two displays. **codex11 is hunting it right now** — you do not need to diagnose it. If
  it kills a run, note the time and retry.
- Several instances of the app run on `:1` at once, so anything that picks "the largest window on
  the display" will eventually drive **another agent's build** and tell you something false about
  code you never ran. Both scripts match `_NET_WM_PID` and are safe; ad-hoc `xdotool` is not.

## Rules that apply to every piece of this project

- **Never copy code from the reference checkouts** (`../_tiller-refs/{waku,zed,orca,t3code}`).
  Inspiration only; every line of Tiller is written from scratch. Transplanted code counts as a gap.
- **A capability nobody exercised does not exist.** Claims carry evidence.
- **Ownership:** you own `rust/crates/tiller_control/**` and `rust/crates/tiller/src/main.rs`.
  codex11 owns `tiller_terminal/**` and `tiller/src/panes.rs`; pi owns `tiller_ui/**` and
  `tiller_theme/**`. If you need a change in someone else's file, say so in your reply and it will
  be routed — a silent edit under another agent's cursor is how hours of their work disappear.

## Reporting

Reply in **12 lines or fewer**: what you changed, the three acceptance artefacts pasted verbatim,
what you decided about persistence, and the honest remainder. If the highlight and the socket still
disagree, that sentence is the most useful thing you can write.
