# P19 — The controls that are drawn but do nothing

**You are pi, pane `w1:p4`.** Your context was just reset, so this brief is everything you need.

**P16 was excellent work and it is done.** Three-section Changes with a file correctly appearing in
two sections at once; 6 new tests covering exactly the cases that could go wrong (dual-section file,
counts, empty omitted, collapsed hides rows, per-section expansion, row-action mapping); 49 tests
green; clippy clean. The font token now resolves at runtime through a real candidate list and picked
**Fira Mono** here, with `fc-list` cited as proof that JetBrains Mono is absent. And you pixel-
measured the screenshot — three header bands at y 33–57 / 118–141 / 202–225, 2+2+1 rows, `#1A1A1A`
canvas — rather than describing it. That is the standard.

Your honest remainder was right too, and the environment note in it has since been confirmed
independently — see below.

## Where

Worktree: `/home/enzopalmisano/Scrivania/Progetti/tiller-linux` (branch `linux/gpui-waku`).
Rust/GPUI rewrite of Tiller, targeting Linux. Nothing is committed.
Your own notes from an earlier session are in `docs/linux-rewrite/PI-HANDOFF.md` — the theme token
vocabulary and the type/radius/density scales. Read it before touching the visual layer.

```bash
source ~/.cargo/env && cd /home/enzopalmisano/Scrivania/Progetti/tiller-linux/rust
cargo test -p tiller_ui -p tiller_theme
cargo build -p tiller -p tiller_control
```

**codex12 is in `tiller/src/main.rs` and `tiller_control/**`, codex11 is in `tiller_terminal/**`.**
A build failure there is their work in flight, not yours.

**`tiller_ui/src/settings.rs` is yours again** — codex12 has left it. It needed it for settings
persistence (P11) and that work is done and proven.

## The environment, measured — do not spend your budget re-deriving it

You reported that this machine's Xwaylands had stopped presenting. Confirmed, with the cause:

```
cosmic-comp: Failed to render texture … import for wrong devices DrmNode { dev: 57984, ty: Render }?
             … modifier: Unrecognized(144115188076389125)
```

`57984` is `renderD128`, the only render node; `0x0200000000000005` is an **AMD** tiling modifier
the compositor's renderer does not recognise, so it cannot import the buffer the app rendered into.
This is a Mesa/compositor mismatch **outside this project**. Also measured just now: forcing
software Vulkan (`VK_DRIVER_FILES=…/lvp_icd.json`) does not help, a fresh `Xwayland` does not help,
`Xvfb` does not help, and native Wayland runs fine but `grim` cannot capture it because cosmic-comp
does not implement `wlr-screencopy-unstable-v1`.

**It is intermittent, not dead** — another agent captured a valid 9303-colour frame at ~09:38, after
the compositor had already logged those failures. So: **retry.** `Scripts/linux-shot.sh` fails
loudly on a blank frame and will never report one as a pass, so a retry loop is safe. If you cannot
get a frame after several attempts, say so and deliver the tests — that is an honest remainder, not
a failure of the piece.

## The piece: three affordances that promise something they do not do

These are three separate items on the critic's FAILED list, and they are the same defect wearing
three faces. **The UI is offering a capability the program does not have.** That is the most
corrosive kind of bug in a tool people drive by eye: every one of them teaches the user something
false, and unlike a crash, nothing announces it.

### 1. The `+` add-project control opens nothing

The critic clicked it and no picker, no dialog, nothing appeared. The shell already handles the
result — `SidebarEvent::AddProject(path)` is emitted and `main.rs` calls `workspace.add_project` —
so the missing half is the affordance actually producing a path. Give it a real directory chooser
(GPUI has a path-prompt facility; find how the rest of this codebase opens one rather than
inventing a mechanism), and emit `AddProject` with what the user picked. Cancelling must do
nothing, silently and correctly.

### 2. Dragging a sidebar row to reorder has no handlers

The rows look draggable and are not. Either implement reordering — with the order persisted through
whatever the sidebar already uses for its state — or, if that is too large for this piece, remove
the affordance so nothing suggests it works. **A control that visibly invites an action it cannot
perform is worse than no control**, and saying so in your reply is a legitimate outcome.

### 3. "SF Symbols" is offered as a file-icon set on Linux

`tiller_ui/src/settings.rs:17`:

```rust
const SEGMENTED_FILE_ICONS: &[&str] = &["SF Symbols", "Material"];
```

SF Symbols is Apple's system icon API. On this machine it cannot exist — there is no such font and
no such API — so the option is either inert or silently falls back, and either way the settings
screen states something untrue about the running program. This is the same class of defect as
`SFMono-Regular` that you fixed in P16, and the same class as the provider badges elsewhere in this
app that say "Available" when they only mean "an adapter exists".

Resolve it the way you resolved the font: **let the platform answer**. Offer the sets that actually
exist here, and if the codebase keeps SF Symbols for a future macOS build (`tiller_ui/src/icons.rs`
documents a three-path icon strategy, and `sfsymbol.rs` is explicitly macOS-only), then gate the
option on the platform rather than deleting the concept. Whatever the settings screen lists must be
selectable and must visibly change the icons.

## Rules that apply to every piece of this project

- **Never copy code from the reference checkouts** (`../_tiller-refs/{waku,zed,orca,t3code}`).
  Inspiration only; every line of Tiller is written from scratch. The critic checks, and
  transplanted code counts as a gap, always.
- **A capability nobody exercised does not exist.**
- **Measure, do not describe**: `convert <png> -crop 1x1+X+Y -format '%[pixel:p{0,0}]' info:`.
- You own `tiller_ui/**` and `tiller_theme/**`. Need a change elsewhere? Say so; it will be routed.
- The app dies silently every 4–13 minutes, no panic and no log. codex11 is hunting it; if it kills
  a run, note the time and retry.

## Evidence

Tests for each behaviour that can be tested without a display — the picker emitting `AddProject`
with the chosen path and doing nothing on cancel; the icon-set list being platform-derived; the
reorder producing the new order. Then a screenshot if the machine lets you take one, and an explicit
statement if it does not.

## Reporting

Reply in **12 lines or fewer**: what each of the three now does, the test count, whether you got a
frame, and the honest remainder. If you chose to remove the drag affordance rather than implement
it, say so and why — that is a decision worth recording, not a shortfall.
