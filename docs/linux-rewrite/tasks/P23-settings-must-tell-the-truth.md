# P23 — Settings must tell the truth about this machine

**You are pi, pane `w1:p4`.** Your context was just reset, so this brief is everything you need.
Your own earlier notes are in `docs/linux-rewrite/PI-HANDOFF.md` — the theme token vocabulary and
the type/radius/density scales, with the reasoning. Read it before touching the visual layer.

## P19 is closed, and two of its loose ends are resolved — neither is your problem

271 tests green, 7 of them new and pointed at exactly the right cases (picker emits the path, cancel
is silent, drag no longer pretends to reorder, the icon list is platform-derived, clamping, segment
click → snapshot, extension → glyph). You also **declined to run input automation on the shared
`:2`** because the critic was working there. That was the right call and it is worth saying so:
correctness of other people's evidence beat completeness of your own.

Two things you flagged, both checked so you do not spend budget on them:

- **The black strips in your frame were a capture artefact, not a defect.** An independent capture
  of the same app on `:2` fills the frame edge to edge with no black margin. A root crop taken at
  the wrong origin produces exactly what you saw. Nothing to fix.
- **Your SF Symbols gating is correct in code** — `#[cfg(target_os = "macos")]` on
  `SEGMENTED_FILE_ICONS`, `available_on_this_platform()`, and `clamp_file_icons` correcting a value
  persisted on another platform. The frame showing "SF Symbols" was captured before your change.

## Where

Worktree: `/home/enzopalmisano/Scrivania/Progetti/tiller-linux` (branch `linux/gpui-waku`).

```bash
source ~/.cargo/env && cd /home/enzopalmisano/Scrivania/Progetti/tiller-linux/rust
cargo test -p tiller_ui -p tiller_theme -p tiller_agents
env -u WAYLAND_DISPLAY DISPLAY=:2 GPUI_X11_SCALE_FACTOR=1 ./target/debug/tiller
```

**`:2` is the only display on this machine that presents.** Everything created after ~09:55 comes
back blank — software Vulkan and `MESA_VK_WSI_DEBUG=sw` included, all tested. It is shared with the
critic, which has priority, and with two builders. Keep runs short, do not raise or focus your
window, prefer `import -window <id>` over a root crop (it reads the window's own pixmap and is
immune to occlusion — and it is what produced your phantom black strips), and close your instance
when you are done.

codex12 is in `tiller/src/main.rs` and `tiller_control/**`; codex11 is in `tiller/src/panes.rs` and
`tiller_terminal/**`. Build failures there are their work in flight. **`tiller_ui/**` and
`tiller_theme/**` are yours.**

## The theme: three places where Settings states something untrue on Linux

This is the project's most persistent defect class. Every instance teaches a user something false
about the program they are running, and unlike a crash, nothing announces it.

### 1. The provider badges are hard-coded (F-008, open since the first critic pass)

`crates/tiller_ui/src/settings.rs`, lines ~785–809: **five literal `"Available"` strings.** They
mean "an adapter exists in our catalogue", not "the binary is installed on this machine".

The truthful machinery **already exists and is already tested**, in `crates/tiller_agents`:

```rust
pub fn discover_availability() -> Vec<AgentAvailability>
pub fn find_executable_on_path(program: &str) -> Option<PathBuf>
impl AgentAvailability { pub fn is_available(&self) -> bool; pub fn status_label(&self) -> &'static str }
```

On this machine `claude`, `codex` and `pi` are installed; **`opencode` and `omp` are not.** So the
correct screen shows two of the five as unavailable, and a badge that says "Available" for a binary
that is not on `PATH` is simply a lie the UI tells.

Wire the UI to the discovery. Where a provider is unavailable, say so in a way a user can act on —
the resolved path when it is found is more useful than a bare word.

**Note the pattern, because this is the fourth time it has appeared today.** A crate provides a
correct, tested capability and the surface that should use it does something simpler and wrong
instead: the sidebar's selection event that only re-highlighted; a Changes surface mounted by
nothing but a demo binary; a font requested by a macOS family name; and now these badges. **Tested
crate + untruthful surface is this codebase's characteristic failure**, and it passes every unit
test every time. When you finish an item here, ask specifically: *can a user reach this, and does it
show what the crate actually computed?*

### 2. The Control socket row does not exist (F-AUTO-01, and the other half is already built)

codex12 finished the socket half and specified the UI half, which is your file:

> Row: **`Control socket — Enabled/Disabled`**, with a secondary line **`Socket path: <resolved path>`**.

The backing state is exposed through `system.capabilities` as `socketEnabled` and `socketPath`, and
the socket genuinely honours the setting — that is tested and proven with live transcripts. Read
whatever the settings screen already uses to reach app state rather than inventing a new channel,
and if the value is not reachable from `tiller_ui`, say exactly what you need and it will be routed.

The inventory entry (F-AUTO-01) wants the **path displayed**, not merely a toggle, so a user can
find the socket. Display the resolved path, not a template.

### 3. The Permissions screen has no Linux subject

Settings lists **Permissions**, which in the Swift original drives macOS TCC — camera, microphone,
screen recording, accessibility. None of that machinery exists here, and the finding was already
ruled `N/A — platform` for the *entries*. But the **screen is still offered in the sidebar**, so a
Linux user is invited into a page about a permission system their OS does not have.

Decide, and say which you chose and why: hide it on non-macOS the way you gated the icon sets, or
give it a truthful Linux subject if one exists. Either is defensible; leaving it as an empty macOS
artefact is not.

## Rules that apply to every piece of this project

- **Never copy code from the reference checkouts** (`../_tiller-refs/{waku,zed,orca,t3code}`).
  Inspiration only; every line of Tiller is written from scratch. Transplanted code counts as a gap.
- **A capability nobody exercised does not exist.**
- **Measure, do not describe**: `convert <png> -crop 1x1+X+Y -format '%[pixel:p{0,0}]' info:`.
- The app freezes its rendering every 4–13 minutes while the process and control socket stay alive —
  it is a presentation freeze, not a crash, and codex11 is hunting it. If a run freezes, restart and
  say so.

## Evidence

Tests for each: the badge list derived from discovery rather than literals, including the case where
a binary is absent; the socket row rendering both states and a resolved path; the platform gate on
Permissions. Then a screenshot of the Settings screen showing real availability — two providers
correctly marked unavailable is the frame that proves this piece.

## Reporting

Reply in **12 lines or fewer**: what each of the three now shows, the test count, the screenshot
path, what you decided about Permissions and why, and the honest remainder.
