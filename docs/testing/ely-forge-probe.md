# Verifying the Ely components of the change-request surfaces

`Scripts/Tests/test-ely-forge-probe.sh` drives
`rust/crates/sirio_ui/examples/ely_forge_probe.rs` — Ely's `Tabs`,
`GitStatusBadge` and `DiffStat` with Sirio's theme — on an isolated X display,
and keeps PID-matched captures. It is the evidence for delivery 1 of
`docs/superpowers/specs/2026-10-03-change-requests-on-ely-design.md`.

## Run it

    sudo pacman -S --needed xorg-server-xvfb xorg-xwininfo xorg-xdpyinfo xdotool vulkan-swrast imagemagick
    Scripts/Tests/test-ely-forge-probe.sh --xvfb --out-dir /tmp/ely-forge-probe

`--xvfb` starts a private Xvfb on a free display between `:90` and `:99` and
stops it on exit; `--display :N` uses one you started. The probe runs with
Mesa's lavapipe (`VK_DRIVER_FILES`) unless `--no-software-vulkan`. A run ends
`ELY FORGE PROBE OK` with the artifact path, or `FAIL: <reason>` and exit 1.

## What it proves

| Check | How |
|---|---|
| Real pixels | every frame has at least 200 colours |
| The right window | input and captures go only to the window whose `_NET_WM_PID` is the probe's |
| A click reaches `on_change` with the tab clicked | the probe prints `probe tab: <value>`; the script expects `checks`, then `conversation` |
| A disabled tab ignores a click | the count of `probe tab:` lines stays at two |
| The badge tooltip | the hover frame differs from the frame before it |
| Dark and light | both drawn, and different |

The probe's three strips share one state; the second and third are rotated
so the tab under test comes first, which puts it at a known x without
measuring text. A click there runs the same component code as a click on a
tab in the middle of a strip.

## What was seen (2026-10-03, Xvfb 1280x800, lavapipe, interface size 13)

- Six badges read `M A D U R !` in amber, green, red, green, blue and red.
- The diff stats read `+0 −0` with grey dots, then all green, all red and
  mixed; `+123456 −7` stays on one line.
- Each strip shows its icons, labels and notes, with an underline under the
  selected tab; the disabled Files tab is dimmer than the others.
- Selecting Checks changes the panel to `Panel of checks` and moves the
  underline in all three strips.
- Hovering the first badge shows a tooltip reading `Modified`.
- Light is light: a pale grey ground with the same layout and readable tones.

## Found while building it

Ely's theme follows Sirio's through an observer that runs once the app's
start-up closure returns. A window opened in that same closure after
`set_mode(Light)` drew Ely's dark palette. The probe calls
`sirio_ui::ely::init` after choosing the mode; a surface in the running app
is not affected, because its theme changes in an event, not at start-up.

## Not established

- **Keyboard use of `Tabs`.** Its arrow keys act only while its strip holds
  focus; a click does not give it focus, and Sirio binds no Tab traversal.
  In Sirio the tabs are used with the pointer.
- **A blank frame failing the run.** On this machine the hardware Vulkan
  driver also draws under Xvfb (`--no-software-vulkan` passed), so the
  200-colour guard was never seen to fire. Lavapipe is a portability choice
  here, not a necessity.
- **Platforms.** Linux X11 under Xvfb only — not a physical Wayland session,
  macOS or Windows.
