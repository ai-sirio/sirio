# P14b — Hunt the crash under the conditions it was actually seen in

**You are codex11, pane `w1:p2`.** Your context was just reset, so this brief is everything you
need. Nothing here assumes you remember the previous run.

## What you already built, and it is good

`Scripts/crash-supervise.py` exists and is **proven**, by self-test, to do the one thing that
matters most: decompose the wait status. Your own checks captured `exit_code: 7` from a clean exit,
and from `kill -SEGV $$` captured `WIFSIGNALED: True`, `WTERMSIG: 11`, `signal_name: SIGSEGV`,
`core_dumped: True`. That instrument turns an invisible death into a report, which is exactly what
this hunt needs. Keep it. This brief is about the **conditions** you ran it under, not the tool.

## Why the zero result does not mean what it looks like

Three instances, 300s then 480s, no crash. That reads as strong evidence — roughly 39 minutes of
process time against a defect that strikes every 4–13 minutes. It is not, and the reports say why:

```
display: :102
screenshot: …-supervised.png (flat: 1 colors; presentation may be blank)
```

**Every single run captured a flat one-colour frame. The app never presented anything.** Vulkan
needs DRI3 to present on X11; the frame being one colour is the measured signal that presentation
failed. So for all six supervised runs the program was a healthy event loop drawing nothing — and a
defect in rendering, or in a surface the user navigates, cannot fire in a program that never draws
and that nobody touches.

Two further facts:

- The report says `:1 was unavailable`, so the runs fell back to `:102`. **`:1` answers `xdpyinfo`
  right now.** Whatever made it look unavailable was transient or was a misread; do not accept the
  fallback silently again — if `:1` cannot be used, say precisely what failed.
- **Every observed death happened while the app was being operated or was live on screen**, and
  three of the five were settings-adjacent. Nobody operated it in your runs.

None of this is a criticism of the reasoning — it is the same trap that has now caught this project
three times, and it is worth naming: **before believing a negative result about the product, check
that the instrument and the conditions could have produced a positive one.** A run where the app
never rendered is not a clean run. It is not a trial at all.

## What to do

### 1. Make the supervisor refuse to count an invalid trial

This is the most valuable change, because without it the harness will keep producing confident
zeros. A run must be classified, and the classification must appear in the report:

- **VALID TRIAL** — the app presented real pixels (capture has thousands of distinct colours, not
  one) for the duration, and, if you drove it, the input landed.
- **NOT A TRIAL** — flat frame, no window, died during startup, display without DRI3. These must
  **not** be added to any "no crash in N minutes" total. Report them separately with the reason.

Use the same flatness test the other scripts use: `identify -format '%k'`, fewer than ~200 distinct
colours means presentation failed. Sample more than once during the run, not only at the end — an
app that renders and then stops is itself a finding.

### 2. Run it where it renders

Only a display with **DRI3** can present. Measured, and not worth re-deriving:

| Display | DRI3 | Renders |
|---|---|---|
| Xvfb | no | window maps, every frame black |
| Xephyr `-glamor` | no | same |
| Xwayland `:101` rootless | yes | still black |
| **Xwayland `:1` (the session's own)** | yes | ~11.4k distinct colours |

Launch with `env -u WAYLAND_DISPLAY DISPLAY=:1 GPUI_X11_SCALE_FACTOR=1` — `WAYLAND_DISPLAY` must be
unset or GPUI ignores `$DISPLAY` entirely, and pinning the scale factor keeps window geometry sane
(without it the window has reported `3798x2152+-937+-536`, a negative origin, and every crop lands
off-screen).

**Several instances run on `:1` at once**, including other agents' builds. Anything that picks "the
largest window on the display" will eventually photograph or drive somebody else's process and tell
you something false about code you never ran. Match `_NET_WM_PID`, as `Scripts/linux-shot.sh` and
`Scripts/linux-drive.sh` already do.

### 3. Operate the app while it runs

A hands-off run tests a different program from the one that died. Drive it, in a loop, for the whole
supervised window:

- **Into and around Settings repeatedly** — 3 of 5 deaths were settings-adjacent. This is the
  strongest lead you have; spend the most time here.
- Switch tabs, open and close panes, resize the window.
- Start a hosted agent if you can — the crash is reported to kill hosted agents and unsaved state,
  so a running agent may be part of the trigger.

`Scripts/linux-drive.sh <out.png> '<actions>' [settle] [display]` exposes `click x y`
(window-relative), `type "text"`, `key <keysym>`, `shot [path]`. Input must go through **absolute
screen coordinates and plain XTEST**: this app is an X11 guest of a Wayland compositor, so X focus
cannot be forced onto it and `xdotool --window` targeting silently delivers nothing — a whole run
was once written off as "input does not work" because of exactly that. The control socket
(`tillerctl`, and the `panel.*` methods) is the more precise instrument where it reaches, and it
does not move the operator's pointer.

### 4. Give it a real budget

At a 4–13 minute mean time between deaths, half an hour of valid, rendering, driven process time is
the *minimum* that makes a zero meaningful, and an hour is better. State the number you achieved in
your report as **valid trial minutes**, separately from wall-clock.

When a death does happen, collect immediately: `WTERMSIG` (the single most discriminating fact —
`SIGSEGV`, `SIGABRT`, `SIGBUS` and `SIGKILL` point at four completely different causes),
`coredumpctl list` / `coredumpctl gdb` with `ulimit -c unlimited`, `dmesg -T | tail` and
`journalctl -k` for an OOM kill, and the last output lines. A `panic::set_hook` writing to a
**file** and `fsync`ing it will catch a panic on a background thread that stderr would lose.

## What "done" looks like

Either the cause with evidence, or an honest, *valid* non-reproduction: N valid trial minutes with
the app rendering and being driven, M of them in Settings, no death — plus the supervisor ready to
catch the next one. The second outcome is genuinely useful and must not be dressed up as the first.

**Failing to reproduce is not evidence of absence.** This defect was already written off once after
a single failed reproduction; five more deaths followed.

## Rules and ownership

- **Never copy code from the reference checkouts** (`../_tiller-refs/{waku,zed,orca,t3code}`).
  Every line of Tiller is written from scratch; transplanted code counts as a gap.
- You own `rust/crates/tiller_terminal/**`, `rust/crates/tiller/src/panes.rs`, and your own
  `Scripts/crash-*` files. **pi is editing `tiller_ui/**` and `tiller_theme/**` right now; codex12
  is editing `tiller/src/main.rs` and `tiller_control/**`.** A build failure in those is their work
  in flight, not your bug — wait and retry, or `cp -a` the tree aside. Diagnosis may read anything;
  if the culprit lives in a file you do not own, **report it, do not patch it.**

## Reporting

Reply in **12 lines or fewer**: valid trial minutes and how many were driven, the display used and
its presentation status, what you exercised, whether anything died, the wait status if so, and the
honest remainder.
