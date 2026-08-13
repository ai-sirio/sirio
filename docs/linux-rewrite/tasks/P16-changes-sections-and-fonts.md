# P16 — Changes: the three sections, and the font that does not exist on Linux

**You are pi, pane `w1:p4`.** Your context was just reset, so this brief is everything you need.
You wrote `docs/linux-rewrite/PI-HANDOFF.md` before the reset — the theme token vocabulary, the
type/radius/density scales and the reasoning behind them are in it. **Read it first**; it is your
own notes to yourself and it will save you rediscovering the visual system.

## Where

Worktree: `/home/enzopalmisano/Scrivania/Progetti/tiller-linux` (branch `linux/gpui-waku`).
Rust/GPUI rewrite of Tiller, now targeting Linux. Nothing is committed.

```bash
source ~/.cargo/env && cd /home/enzopalmisano/Scrivania/Progetti/tiller-linux/rust
cargo test -p tiller_ui
cargo build -p tiller -p tiller_control
env -u WAYLAND_DISPLAY DISPLAY=:1 GPUI_X11_SCALE_FACTOR=1 ./target/debug/tiller
```

`WAYLAND_DISPLAY` must be unset or GPUI ignores `$DISPLAY`. Only the session's own Xwayland (`:1`)
can present — Xvfb and Xephyr lack DRI3, which Vulkan needs to present on X11. A capture with one
distinct colour means presentation failed, not that the UI is empty.

`Scripts/linux-shot.sh <out.png> [settle] [display]` launches and photographs;
`Scripts/linux-drive.sh <out.png> '<actions>' [settle] [display]` also drives it, exposing
`click x y` (window-relative), `type "text"`, `key <keysym>`, `shot [path]`.

**codex12 is editing `rust/crates/tiller/src/main.rs` right now**, and the tree may not build until
it lands. If `cargo build -p tiller` fails on a non-exhaustive `SidebarEvent::SelectWorktree` match
in `main.rs`, that is its work in flight, not yours — `cargo test -p tiller_ui` is unaffected, so
lead with the tests and take the screenshot once the binary builds again.

## Part 1 — Changes has no Staged / Changed / Untracked sections

The critic, which did not build this, found the Changes surface renders **one flat list**. The
grouping is not a cosmetic nicety: without it the panel cannot express the thing git actually
reports, which is that a file has **two independent states at once**.

The data layer is already finished and simply unused. `tiller_git`'s `StatusSnapshot` exposes
exactly the three buckets, with the semantics already worked out:

- `staged()` — `index_status` set and not `Untracked`
- `changes()` — `has_worktree_changes() && !is_untracked()`
- `untracked()` — untracked in either column

`ChangesTab::change_rows()` (`crates/tiller_ui/src/changes.rs`, around line 299) ignores all three
and iterates `self.entries` flat.

What to build:

- **Three sections, in this order: Staged, Changed, Untracked**, each with a header carrying its own
  count — the same idea as the collapsed-context bands you already built, which state their size
  rather than leaving the reader to guess. An empty section is omitted, not shown empty.
- **A file that is both staged and further modified appears in both sections.** That is what
  porcelain's two columns mean, and it is what the Swift app did. Collapsing it to one row throws
  away the distinction the whole surface exists to show. The per-row Stage/Unstage action must act
  on the right side of that split.
- **Section headers collapse**, remembering state per section the way `expanded_changes` already
  remembers it per file.
- Keep everything already working: per-file `+N -M` counts, the collapsed-context bands with their
  "N hidden lines" labels, Stage/Unstage/Discard, Stage All/Discard All, the toolbar.

`Discard All` currently filters on `has_worktree_changes()` — that is **correct** and must stay:
discarding applies to worktree changes, not to the index. Do not "fix" it to match the new sections.

## Part 2 — `SFMono-Regular` does not exist on this machine

Five call sites ask for a macOS-only font family by name:

```
crates/tiller_ui/src/changes.rs:596
crates/tiller_ui/src/chat.rs:1195, 2304, 2347
crates/tiller_ui/src/file_view.rs:111
```

`fc-list` on this machine has no `SFMono-Regular`. GPUI falls back silently, so every code span,
diff line and file view is rendered in an unknown family chosen by the font stack rather than one
we picked — and it looks fine, which is why it survived. This is the same class of defect as the
"SF Symbols" icon set still being offered in Settings on Linux: **macOS assumptions that are
invisible until you look for them.**

Resolve a monospace family **once**, at runtime, from what is actually installed, and put it in
`tiller_theme` next to the code-font weight that already lives there — so there is one answer to
"what is our mono font", not five string literals. Prefer, in order, a family the visual bar
actually calls for, then a common good one, then whatever generic monospace the system resolves;
verify your candidate list against `fc-list : family` on this machine rather than guessing.
Do **not** ship a hard-coded family that happens to exist here — the fallback chain is the point.

(`Settings`'s "SF Symbols" file-icon entry is the same bug, but
`crates/tiller_ui/src/settings.rs` is being edited by codex12 right now. **Leave it alone**; it
stays on the open list.)

## Rules that apply to every piece of this project

- **Never copy code from the reference checkouts** (`../_tiller-refs/{waku,zed,orca,t3code}`).
  Inspiration only; every line of Tiller is written from scratch. The critic checks for this, and
  transplanted code counts as a gap, always. The section-with-counts and the collapsed band are
  orca's *ideas*, taken deliberately — write them yourself.
- **A capability nobody exercised does not exist.** Claims carry evidence.
- **Measure, do not describe.** `convert <png> -crop 1x1+X+Y -format '%[pixel:p{0,0}]' info:`.
  Three confident visual readings were wrong before this rule existed.

## Ownership — three agents share this worktree

You own `rust/crates/tiller_ui/**` and `rust/crates/tiller_theme/**`, **except
`tiller_ui/src/settings.rs`**, which codex12 is in. Do not touch `tiller/src/main.rs` (codex12) or
`tiller_terminal/**` and `tiller/src/panes.rs` (codex11). If you need a change in one of those,
say so in your reply and it will be routed — a silent edit under another agent's cursor is how
hours of someone else's work disappear.

## Evidence this piece must produce

1. `cargo test -p tiller_ui` green, with **new tests** covering: a file that is staged *and*
   modified appearing in both sections; section counts; an empty section omitted; a collapsed
   section hiding its rows.
2. A **screenshot of the Changes tab against a real repository** in which all three buckets are
   non-empty at once — stage one file, modify another, leave a third untracked, then capture.
   Construct that repo yourself with `git init` in a scratch directory and add it as a project;
   do not rely on whatever state the tiller-linux worktree happens to be in.
3. The resolved monospace family name, printed from the running app or a test, proving the fallback
   picked something that exists.

## Reporting

Reply in **12 lines or fewer**: what you changed, the test count, the screenshot path, the resolved
font family, and the honest remainder — anything you could not finish or could not prove. If you
run low on context, stop and write a handoff the way you did last time; a clean boundary with good
notes is worth more than a half-applied refactor nobody can pick up.
