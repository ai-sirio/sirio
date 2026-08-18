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



