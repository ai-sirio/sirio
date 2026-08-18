# Critic pass on wave K — eight builder claims, judged live (`wf-judge`)

Fresh critic, did not build any of the work under judgment. Host: the x86 desktop described in
`ENVIRONMENT.md`'s 2026-08-18 section. Lane: the nested Wayland lane (`Scripts/wayland-drive.sh`)
plus a hand-rolled dbus/notification-daemon rig for the F-CORE-ACT-20 gap. Binary pinned once from
current HEAD and reused for every drive in this pass:

```
git rev-parse HEAD          # 4c9f552164f34a555e4a98ba05408890bb9ef737
cargo build --manifest-path rust/Cargo.toml --workspace   # exit 0, warm, ~34s
cp rust/target/debug/tiller /tmp/wf-judge-tiller
export TILLER_WL_BIN=/tmp/wf-judge-tiller
```

Every row's evidence below was personally driven by this pass unless explicitly marked otherwise.
Screenshots referenced below are committed under `reference/linux-progress/waveK-critic/`.

## Environment note: the box hit literal 0 bytes free, repeatedly, mid-pass

`ENVIRONMENT.md`'s "keep concurrent lane-driving agents at about 5" was well exceeded during this
pass — `ps` showed at least 8 concurrently-running `tiller` instances under distinct labels
(`wf-act`, `wf-chg`, `wf-tab`, `wf-rest`, `wf-rest2`, `wfj-notifhost`, `wf-sweep`, plus this pass's
own) at the point this pass started. The root filesystem (`/`, which is also `/tmp`) went from
`df`-reported 2.2G free at pass start to **repeated, sustained stretches at literally 0 bytes
free** — not momentary, but for minutes at a time, several separate times. At 0 free, every tool
in this harness that needs to write output (Bash, Write) fails outright, including a bare `true`;
only `Read` kept working. Concrete effect: this pass lost a meaningful fraction of its wall-clock
budget waiting out these stretches, and had to delete ~2GB of clearly-orphaned debris (dead
`wf-fix-tiller-*` binaries from the already-committed, already-merged wave-I builder pass, plus
this pass's own prior interrupted attempt's leftover `/tmp/wf-judge-{PREFIX,DEBUG}-tiller`
binaries — confirmed via `fuser`/`ps` that nothing live held them) twice, by hand, just to get
enough headroom to run `git commit`. **This same scratchpad session directory
(`.../scratchpad/wfjudge/`) already contained a prior, interrupted attempt at this exact task**
(same lane label, same eight rows, screenshots timestamped up to 21:07 today) — it died mid-drive
on `F-CORE-ACT-20`'s real-daemon gap without ever writing a report. Its leftover screenshots were
reviewed as a sanity check where legible, but every row below was independently re-driven by this
pass on this pass's own pinned binary; the predecessor's `F-SID-19` screenshots in particular were
internally inconsistent (a `Chat`+`Terminal` tab pair visible in what should have been an empty
zero-tab state) and are **not** relied on for any verdict below.

---

## 1. CENTER-01 — the safety gate: a needs-input pane must survive a worktree switch (mandatory gap)

Commits `790096b0`/`1abc7b2c`, doc `CENTER-PANE-DESYNC.md`. Both confirmed ancestors of HEAD
(`git merge-base --is-ancestor`). The builder's own doc names this exact gap unproven: "exercise
the safety-gate path live (put a pane in `NeedsInput` via `tillerctl notify`, switch away, confirm
the pane survives with its live PTY rather than a fresh empty one) — the unit test proves this in
isolation but was not independently redriven live in this pass." This is the brief's mandatory gap;
driven here over the real control socket with a **hard discriminator: a host PID that must not
change**, not a screenshot alone.

**Setup**: two fresh git repos, `/tmp/wfj-c01-repoA` and `/tmp/wfj-c01-repoB` (`git init` +
one commit each), added as two Tiller projects via `ctl project.add`. `TILLER_WL_LABEL=wfjc01`,
`TILLER_WL_BIN=/tmp/wf-judge-tiller` (this pass's own fresh build of current HEAD,
`4c9f552164f34a555e4a98ba05408890bb9ef737`).

**Drive**:
1. Selected repoA (`ctl workspace.select workspace=/tmp/wfj-c01-repoA`), opened a new terminal tab
   (`chord ctrl t`), clicked into it and typed `sleep 600` + Return — a bare running command, no
   agent, exactly F-TERM-10/CENTER-01's shared scenario.
2. Confirmed on the **host** the child process is real and running:
   `ps -p 173355 -o pid,etimes,args` → `173355 6 sleep 600` (screenshot
   `center01-02-sleep-running.png`).
3. `ctl panel.list worktree=/tmp/wfj-c01-repoA` → the active pane's id is `pane-2`.
4. `tillerctl`-equivalent socket call: `ctl notify session=pane-2 status=needs-input` → `{"queued":
   "true"}`.
5. `ctl workspace.select workspace=/tmp/wfj-c01-repoB` → `ok`, path/branch confirm the switch.
6. Immediately re-checked the **same host PID**: `ps -p 173355 -o pid,etimes,args` →
   `173355 25 sleep 600` — **same PID, `etimes` advanced continuously from 6 to 25 with no gap**,
   i.e. the process was never killed and respawned. Forced-repaint screenshot
   `center01-03-repoB-selected-gate.png` shows the sidebar highlight, the Files panel path
   (`/tmp/wfj-c01-repoB`) and the status bar (`master · /tmp/wfj-c01-repoB`) all agreeing the app
   really switched to repoB, while the **centre pane still shows repoA's `sleep 600` terminal** —
   the documented "stale-but-safe" behaviour, not a bug: the reload that would tear down and
   replace the centre pane's tabs is the exact thing the gate skips.
7. Three more switches (`repoA → repoB → repoA`, i.e. 5 total switches from the first), then a
   final host check: `ps -p 173355` → `173355 53 sleep 600` — still the same PID, `etimes` still
   advancing continuously. Screenshot `center01-04-after-more-switches.png` shows repoA reselected
   with the identical terminal content still mounted.

**Verdict: PASSED.** The hard discriminator (a real host PID that must not change) held across five
worktree switches while the pane was marked `needs-input`: the live PTY was never torn down and
replaced by a fresh one. This is my own fresh drive against this pass's own pinned current-HEAD
binary, not a reused screenshot. I did not separately re-drive the plain forward-case reload
(content resets on a clean switch) since that half is not in dispute and the builder's doc already
demonstrates it live; the safety gate itself — the half nobody had driven — is what this pass
closes.

---

## 2. F-CORE-ACT-20 — notification suppression must follow REAL window focus (mandatory gap)

Commit `1abc7b2c` (ancestor of HEAD), doc `CENTER-PANE-DESYNC.md`. The builder's own gap: its live
proof used "a private, ad hoc D-Bus session bus **because the box's real user session bus was
down**" with "a minimal Python `org.freedesktop.Notifications` stub" — and it explicitly asked for
this to be "repeat[ed]... independently" with "a real notification daemon (not a stub) in the loop
... to rule out any difference between a stub answering `Notify()` and a real listener." Driven here
with the **real, installed `notification-daemon` (GNOME) 3.20.0 package** — not a stub — on a
private bus, per the brief's own recipe.

**Rig** (all on one throwaway, isolated bus — the operator's real `/run/user/1000/bus` was never
touched):
1. `dbus-daemon --session --fork` → private `DBUS_SESSION_BUS_ADDRESS`.
2. A dedicated tiny nested sway (`WLR_BACKENDS=headless`, `xwayland enable`) just to give the real
   GTK3 daemon somewhere to run — `GDK_BACKEND=wayland` alone segfaulted this build
   (`Gtk-WARNING **: cannot open display:` then SIGSEGV under the headless pixman Wayland path);
   `DISPLAY=:N` (Xwayland, lazily started by that same private sway) + `GDK_BACKEND=x11` runs
   cleanly.
3. Launched the real binary: `/usr/lib/notification-daemon/notification-daemon` with the private
   `DBUS_SESSION_BUS_ADDRESS` and that `DISPLAY`. Positive control before touching Tiller:
   `dbus-send ... GetServerInformation` → `"Notification Daemon" "GNOME" "3.20.0" "1.2"` (confirms
   the real daemon, not a stub, owns the name); `notify-send "sanity" "..."` → exit 0.
4. `dbus-monitor --session` attached to the same private bus, logging every frame verbatim.
5. Tiller (`TILLER_WL_BIN=/tmp/wf-judge-tiller`, this pass's own current-HEAD build) launched via
   `Scripts/wayland-drive.sh` with `DBUS_SESSION_BUS_ADDRESS` exported into my shell first (the
   script's `env` invocation for the app process does not clear it, so Tiller inherits it —
   verified: `post_desktop_notification` in `main.rs` just `Command::new("notify-send").spawn()`s,
   so it uses whatever `DBUS_SESSION_BUS_ADDRESS` its own process environment carries).

**Drive**, one worktree (`/tmp/wfj-c01-repoA`), one terminal tab (the only/active tab throughout,
so `pane_visible` is true the whole time):
1. `title ✳ notif-test` on the pane (OSC 0) — confirmed via a fresh forced-repaint screenshot the
   tab picked up the Claude-style `✳` icon and the sidebar worktree row lit up with the matching
   glyph (Layer B title identification, title-owned).
2. **Baseline, window genuinely focused** (Tiller is the only mapped window; confirmed later by
   contrast, see step 3): `ctl notify session=pane-0 status=needs-input` → queued. No new bus
   traffic (ambiguous case — `should_notify(None, NeedsInput, app_active=true, pane_visible=true)`
   is `false` either way; not yet a discriminating step, matches the builder's own baseline).
3. **Window genuinely defocused**: launched a second real Wayland client, `foot`, into the *same*
   nested compositor Tiller runs in. `swaymsg get_tree` on that compositor's own socket confirms
   `foot` `focused: true` and Tiller's own window node `focused: false` at this instant — a real
   compositor-level focus change, not a flag. Pane still the visible/active tab (untouched).
   `ctl notify session=pane-0 status=done` (genuine transition, `needs-input` → `done`) →
   **`dbus-monitor` captured a real `Notify` method call**, verbatim (full transcript committed at
   `reference/linux-progress/waveK-critic/act20-real-daemon-notify-transcript.txt`):
   ```
   method call ... sender=:1.20 -> destination=:1.6 ...; interface=org.freedesktop.Notifications; member=Notify
      string "Tiller"
      uint32 0
      string ""
      string "Claude Code — wfj-c01-repoA/master"
      string "master · wfj-c01-repoA"
      ...
   method return ... sender=:1.6 -> destination=:1.20 ... reply_serial=9
      uint32 2
   ```
   `:1.6` is the real notification-daemon (same connection that answered `GetServerInformation`
   with `"Notification Daemon"/"GNOME"/"3.20.0"` earlier on this same bus); the `uint32 2` reply is
   a genuine notification id allocated by the real daemon's own internal counter, not an echoed
   stub value.
4. **Window refocused**: `foot` killed; `swaymsg get_tree` confirms Tiller's window node
   `focused: true` again (sway auto-refocused it). Another genuine status change,
   `done` → `error`: `ctl notify session=pane-0 status=error` → queued, then a forced repaint. The
   monitor log's line count is **byte-identical before and after** (784 lines both times) — zero
   new bus traffic of any kind, let alone a second `Notify` — confirming suppression is restored by
   real focus alone, with pane visibility held constant across all three steps exactly as the
   builder's methodology intended.

**Verdict: PASSED.** This closes the named gap precisely: the same fire/suppress contrast the
builder demonstrated with a stub now holds against the real, installed GNOME notification daemon —
a real `GetServerInformation` handshake, a real `Notify` call with a real allocated id, and true
silence (not just "no visible popup") when the window is genuinely focused. Rig detail worth
recording for the next critic: this daemon's GTK3 build segfaults under `GDK_BACKEND=wayland` on a
headless/pixman compositor — use `DISPLAY=:N` + `GDK_BACKEND=x11` against a private sway with
`xwayland enable` instead.



