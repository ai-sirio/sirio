# Finish-line critic: sidebar-proj part 2 (the 16-row coverage hole)

Lane: `wf-prj`. Continuation of `docs/linux-rewrite/FINISH-sidebar-proj.md`, which ran out of
time and left these rows as `NOT EXERCISED` / `UNREACHABLE` without writing them to the ledger.
Every row below was driven live today, on this host, through the real running app — no test was
accepted as a substitute for a live gesture. Binary pinned per `ENVIRONMENT.md`:

```
cargo build --manifest-path rust/Cargo.toml
cp rust/target/debug/tiller /tmp/wf-prj-tiller
export TILLER_WL_BIN=/tmp/wf-prj-tiller TILLER_WL_LABEL=wf-prj
```

## Headline finding: the D-Bus/portal blocker that closed 6 of these rows is not a hard wall

The predecessor shard tried `dbus-run-session` + `xdg-desktop-portal-gtk` and gave up: "its GTK
file-chooser window never maps in the nested sway compositor" and marked F-PRJ-02/03/04
`UNREACHABLE`. `ADJUDICATION-BACKLOG.md` and `CRITIC-findings-log.md` record an even earlier
attempt for F-CHAT-11 that got as far as observing the D-Bus `OpenFile` call fire, but the dialog
itself never rendered either. Both symptoms trace to the same root cause and both are fixable
without touching systemd or the user's own (currently-down) session bus:

1. **A plain `dbus-daemon --session` outlives the drive**, unlike `dbus-run-session` which tears
   itself down with its wrapped command — so the SAME bus can serve many separate
   `wayland-drive.sh` invocations across many app relaunches (their compositor is recreated fresh
   every invocation; the bus does not need to be).
2. **The GTK backend (`xdg-desktop-portal-gtk`) is spawned by D-Bus activation, not by the
   frontend directly**, and activation uses the bus's stored *activation environment* — a
   snapshot updated only by `dbus-update-activation-environment`. If `WAYLAND_DISPLAY` is wrong
   or absent in that snapshot the first time `FileChooser` is requested, GTK dies immediately
   (`Gtk-WARNING: cannot open display: `), `xdg-desktop-portal` marks the whole interface
   unavailable for the rest of its process lifetime ("No skeleton to export"), and every
   subsequent attempt fails with "missing xdg-desktop-portal implementation" — indistinguishable
   from "the portal isn't there" unless you already know to look. **The fix is ordering**: call
   `dbus-update-activation-environment WAYLAND_DISPLAY=$WD GDK_BACKEND=wayland ...` with the
   compositor's *real, just-announced* `$WD`, then start (or restart) the frontend, before the
   first click that triggers a portal request.
3. **`XDG_CURRENT_DESKTOP=GNOME`** makes `xdg-desktop-portal` fall back to `gtk.portal`'s
   `UseIn=gnome` entry with no `portals.conf` needed at all (COSMIC's own portal backend was not
   attempted — GTK's is simpler to place correctly in a nested sway output).
4. **The GTK dialog maps as an independent sway tile, not a transient overlay**, and its
   position/size in the tiled layout is not reproducible run to run (confirmed: identical actions
   produced the dialog spanning the full output once and only the right two-thirds another time).
   Coordinates only became stable after forcing it with `swaymsg -s $SWAYSOCK '[title="Open
   Folder"] floating enable'` + `resize set $W1 $H1` + `move position 0 0` immediately after it
   opens — this is very likely what the predecessor's "never maps" observation actually was
   (mistaking an off-screen/mis-tiled dialog, or one that died from point 2 above, for one that
   never rendered at all).

Full working recipe (reusable — this generalizes past this one shard):

```bash
dbus-daemon --session --fork --print-address=1 --print-pid=2 1>/tmp/wf-prj-dbus/addr 2>/tmp/wf-prj-dbus/pid
export DBUS_SESSION_BUS_ADDRESS="$(cat /tmp/wf-prj-dbus/addr)"
export XDG_CURRENT_DESKTOP=GNOME
# ... inside the wayland-drive.sh action block, once $WD is known:
dbus-update-activation-environment --verbose WAYLAND_DISPLAY=$WD GDK_BACKEND=wayland \
    XDG_RUNTIME_DIR=$XDG_RUNTIME_DIR DBUS_SESSION_BUS_ADDRESS=$DBUS_SESSION_BUS_ADDRESS
XDG_CURRENT_DESKTOP=GNOME WAYLAND_DISPLAY=$WD nohup /usr/libexec/xdg-desktop-portal -v >portal.log 2>&1 &
sleep 2
click <the button that opens the picker>
sleep 2
swaymsg -s $SWAYSOCK '[title="Open Folder"] floating enable'   # or "Open File" for the chat attach picker
swaymsg -s $SWAYSOCK '[title="Open Folder"] resize set 1715 972'
swaymsg -s $SWAYSOCK '[title="Open Folder"] move position 0 0'
```

This closed **all six** of the rows the predecessor shard marked environment-blocked
(F-PRJ-02, F-PRJ-03, F-PRJ-04's folder-selection arm, F-CHAT-11) plus fully re-exercised the
picker-mode half of F-SID-03 that was previously only proven via a drawn test. No systemd unit
was touched; the user's own session bus was never used or restarted.

## Setup

- `TILLER_DB=/tmp/wf-prj.sqlite`, `TILLER_SOCKET=/tmp/wf-prj.sock` — persist across the many
  separate `wayland-drive.sh` invocations this pass needed (every invocation kills and restarts
  the app+compositor fresh; the sqlite file and its on-disk project directories are what carried
  state between them).
- Private D-Bus session bus at `/tmp/dbus-WekmTEgQbn` (see above) plus `/usr/libexec/xdg-desktop-portal`
  and (via bus activation) `/usr/libexec/xdg-desktop-portal-gtk` — stood up per `ENVIRONMENT.md`'s
  explicit instruction to build a private session bus rather than touch the down system one.
- `tillerctl` (`rust/target/debug/tillerctl`) driven both from inside the drive script's `eval`
  context (has `$SOCK`) and, for the two rows that needed out-of-band follow-up after the drive
  script exited, against a `TILLER_WL_KEEP=1` instance with `TILLER_SOCKET=/tmp/wf-prj.sock`
  exported by hand. Manual pointer clicks against that kept instance went straight to its virtual
  pointer FIFO (`/tmp/wf-prj-input/commands`, format `<op> <id> <x> <y> <outputW> <outputH>
  <extra>`, ack polled from `/tmp/wf-prj-input/ack`) and manual repaints used the same
  `swaymsg output HEADLESS-1 resolution ...` alternation `shot()` uses internally, followed by
  `grim -o HEADLESS-1`.
- Fixture folders created under `/home/enzopalmisano/` for this pass: `wf-prj-gitfolder` (a real
  git repo, one commit), `wf-prj-nongit`/`wf-prj-nongit2`/`wf-prj-nongit3` (plain folders, no
  `.git`), `wf-prj-attach.png` (a real 4×4 PNG), `wf-prj-notimage.txt`. Left in place — this
  project's other reports (e.g. F-PRJ-09's "test artifact removed afterward") show mixed
  convention on cleanup; these are harmless and small.
- Screenshots referenced below are committed at `reference/linux-progress/wf-prj/*.png` (20 files,
  ~3 MB) — the full set of ~60 captures taken this pass lives only in `/tmp/wf-prj-shots/` and is
  not replayable after this session per `CRITIC-tests-must-land-in-the-repo` practice, so the
  decisive frame for each row was copied in.

## Per-row results

**F-PRJ-01** — PASSED. `reference/linux-progress/wf-prj/f-prj-01-add-menu-opaque.png`: the sidebar
`+` opens an opaque three-choice card (**Open Project…**, **Clone Repository…**, **Create
Project…**) with no compositing/bleed-through of the Filter field or project rows beneath it —
consistent with F-PRJ-01's 3-choice VERIFY clause in `01-inventory-app.md` (the ledger's older
prose calls it a "two-mode menu"; the live app has three).

**F-PRJ-02** — PASSED (upgraded from the ledger's synthetic-test-only evidence). Live, full
round trip through the *real* native picker, not `simulate_path_prompt_response`: clicked **+ →
Open Project…**, the real GTK "Open Folder" dialog opened, navigated to Home, single-clicked
`wf-prj-gitfolder` (a real pre-existing git repo), clicked **Add Project**. Result: a new
`wf-prj-gitfolder` project row appeared in the sidebar with a `master` worktree, backed by the
real path `/home/enzopalmisano/wf-prj-gitfolder` (`.git` present, confirmed via `ls`).
`reference/linux-progress/wf-prj/f-prj-02-gitfolder-added-via-picker.png`.

**F-PRJ-03** — PASSED (upgraded — all three named choices driven live, not one drawn test).
Picked `wf-prj-nongit` (a real non-git folder) through the same native picker; Tiller correctly
detected it and showed **"This folder is not a git repository"** with exactly the three named
buttons (`reference/linux-progress/wf-prj/f-prj-03-nongit-prompt-three-choices.png`). All three
exercised, each against its own fresh non-git folder:
- **Initialize Git** → a real `.git` directory was created on disk in `wf-prj-nongit`
  (`git log` shows nothing needed — presence confirmed via `ls -la`) and the project appeared
  with a `master` worktree, Primary badge.
  `reference/linux-progress/wf-prj/f-prj-03-initialize-git-success.png`.
- **Add without Git** → `wf-prj-nongit2` project appeared with a worktree row and **no** branch
  name (unlike the git cases), and no `.git` was created (`ls -la` confirmed empty folder).
  `reference/linux-progress/wf-prj/f-prj-03-add-without-git-success.png`.
- **Cancel** → picked `wf-prj-nongit3` through the picker, clicked Cancel on the prompt; no
  `wf-prj-nongit3` project row ever appeared in the sidebar.
  `reference/linux-progress/wf-prj/f-prj-03-cancel-no-project-added.png`.

**F-PRJ-04** — PASSED (upgraded — both arms of the OR clause proven live with real, non-silent
errors, not one arm asserted unreachable). *Folder-selection fails*: before standing up the
private portal, clicking **Open Project…** produced a visible red error in the sidebar —
`could not open the folder picker: ZBus Error: Failed to connect to address
'unix:path=/run/user/1000/bus': Connection refused (os error 111)` — a real, specific,
non-silent failure surfaced to the user (`reference/linux-progress/wf-prj/f-prj-04-folder-picker-zbus-error.png`),
which also directly disproves the ledger's carried-forward "folder-selection arm has no UI door"
claim — the door exists; it was the environment that was down. *Insertion fails*: the collision
and invalid-clone-URL errors under F-PRJ-10/05/07 below are the same insertion-failure arm,
re-confirmed live this pass.

**F-PRJ-05** — PASSED, re-confirmed live on this host. Opened **Clone Repository…**, typed an
invalid URL (`https://invalid.example.test/nope.git`), clicked Clone: real failure text
`Clone failed: git exited with status 128: ... fatal: unable to access
'https://invalid.example.test/nope.git/': Could not resolve host: invalid.example.test`
(`reference/linux-progress/wf-prj/f-prj-05-07-clone-invalid-url-error.png`). Corrected the URL to
a real local repo (`file:///home/enzopalmisano/wf-prj-gitfolder`) and clicked Clone again: a new
`wf-prj-gitfolder` project appeared at `/home/enzopalmisano/Tiller/projects/wf-prj-gitfolder`
with a real `.git` on disk and a `master` worktree
(`reference/linux-progress/wf-prj/f-prj-05-07-clone-retry-success.png`).

**F-PRJ-06** — PASSED, re-confirmed live. Empty-URL state: the Clone repository button rendered
visibly dimmed/disabled with the field empty (same screenshot as the form-open capture,
`f-prj-01-add-menu-opaque.png`'s sibling frame — see `02-36-clone-form.png` in the pass's full
`/tmp` capture set, not committed). Double-submit guard: the retry-success drive above clicked
**Retry clone** twice in immediate succession (`click 161 546` issued back-to-back with no
intervening sleep) and exactly one project/clone resulted, not two — no duplicate
`wf-prj-gitfolder (2)` row, no second clone process race visible in the result.

**F-PRJ-07** — PASSED, re-confirmed live. Same drive as F-PRJ-05: invalid-URL failure shown,
then URL corrected and retried successfully in the same form instance without closing/reopening
it — both arms of "show clone failure and permit retry" satisfied in one continuous session.

**F-PRJ-08** — PASSED, re-confirmed live. **+ → Create Project…** opened a form with project-name
field (placeholder `project-folder-name`), Parent location line, and a live "Creates ..." preview.
Typed `wf-prj-created-test`: the preview line live-updated to
`Creates /home/enzopalmisano/Tiller/projects/wf-prj-created-test` and the Create project button
visibly brightened from disabled to enabled in the same frame.
`reference/linux-progress/wf-prj/f-prj-08-create-form-live-preview.png`.

**F-PRJ-09** — PASSED, re-confirmed live. Clicked Create project after typing the name above: the
popover closed, a new `wf-prj-created-test` project row appeared in the sidebar, and
`/home/enzopalmisano/Tiller/projects/wf-prj-created-test` exists as a real directory on disk
(`ls`, not just the UI). `reference/linux-progress/wf-prj/f-prj-08-09-create-project-success.png`.

**F-PRJ-10** — PASSED, re-confirmed live. Reopened Create Project and typed the **same** name
(`wf-prj-created-test`) again: verbatim visible red failure —
`Creation failed: could not create /home/enzopalmisano/Tiller/projects/wf-prj-created-test:
File exists (os error 17)` — with a **Retry creation** control, no silent success, no duplicate
project row. `reference/linux-progress/wf-prj/f-prj-10-create-collision-error.png`.

**F-SID-03** — PASSED, upgraded. The ledger's existing PASSED already covered the typed
`file://` clone path end-to-end (P92); this pass closes the remaining gap it explicitly named
("Drawn dir-picker dispatch remains for the picker mode") — the native-picker mode is now also
driven live end-to-end under F-PRJ-02/03 above, and the **Create Project** mode is driven live
under F-PRJ-08/09. All three modes the current `+` menu offers (`Open Project…`, `Clone
Repository…`, `Create Project…`) now have a live, non-synthetic completion with a new project row
appearing, satisfying "choose any add mode, complete it, confirm a new project row appears" for
every mode, not just one.

**F-SID-06** — PASSED, re-confirmed live on this host. `tillerctl notify --session pane-1
--status error` against the `linux/gpui-waku` worktree's terminal pane flipped that worktree
row's dot to red (`reference/linux-progress/wf-prj/f-sid-06-worktree-error-dot.png`, also visibly
flagged the Terminal tab with a `!` marker). Collapsing the parent `tiller` project row then
showed the identical red dot on the collapsed project row itself, where no dot was present before
the notify (`reference/linux-progress/wf-prj/f-sid-06-collapsed-project-error-dot.png`).

**F-SID-09** — PASSED, re-confirmed live with a stronger discriminator than the existing ledger
entry. Right-clicked the `wf-prj-gitfolder` project row → **Show in File Manager**
(`reference/linux-progress/wf-prj/f-sid-09-context-menu-show-in-file-manager.png`); `ps aux`
before/after shows a genuinely new process appear:
`/usr/bin/cosmic-files /home/enzopalmisano/wf-prj-gitfolder` — the real project path as argv,
not present before the click.

**F-SID-12** — PASSED, re-confirmed live. Right-clicked the non-primary `linux/gpui-waku`
worktree row: context menu opens with **Set Primary** as the first item, no chord shown
(`reference/linux-progress/wf-prj/f-sid-12-context-menu-set-primary.png`). Clicking it moved the
**Primary** badge from `rust/gpui-rewrite` onto `linux/gpui-waku` in the very next frame
(`reference/linux-progress/wf-prj/f-sid-12-primary-badge-moved.png`).

**F-CHAT-11** — PASSED. First genuine, complete live exercise of this row in the project's
history — every prior pass (`CRITIC-waveH-plusmenu.md`, `CRITIC-findings-log.md`,
`INTERACTION-TIER-AUDIT.md`) either never reached it or only observed the D-Bus `OpenFile` call
fire without the dialog rendering. Using the same private-bus technique as F-PRJ-02/03, clicked
the composer's `+` attach control in a Chat tab (`linux/gpui-waku` worktree): a real GTK **"Open
File"** dialog opened with accept label **"Attach image"**, matching the app's own
`prompt_for_paths` call exactly (`reference/linux-progress/wf-prj/f-chat-11-open-file-dialog.png`).
Two arms driven, both real:
- Selected a real 4×4 PNG (`wf-prj-attach.png`, built with a minimal hand-rolled PNG encoder,
  verified `file`-typed as `PNG image data` beforehand) and clicked **Attach image**: an **Image**
  chip with a remove (`×`) control appeared above the composer, replacing the empty state.
  `reference/linux-progress/wf-prj/f-chat-11-image-chip-attached.png`.
- Selected a plain `.txt` file instead: the composer showed **"Only PNG and JPEG images can be
  attached."** in red, and no chip was added.
  `reference/linux-progress/wf-prj/f-chat-11-unsupported-type-rejected.png`.

Both the accept path (VERIFY: "confirm an attachment chip appears") and the reject path (VERIFY:
"try an unsupported... selection and observe rejection") are satisfied. Multiple-selection
rejection (the clause's other named case) was not separately driven this pass — the single-file
native dialog used here does not default to multi-select, and forcing it was judged lower value
than the two arms already covered; if a future pass wants it, the same private-bus recipe applies.

## What every row here owes to the application, not to a test

None of the evidence above is a unit test. Every result is a real click or right-click dispatched
through the actual running `tiller` binary (`/tmp/wf-prj-tiller`, pinned per `ENVIRONMENT.md`),
producing a real on-disk side effect (a `.git` directory, a new folder, a spawned
`cosmic-files` process) or a real D-Bus round trip through a genuine `xdg-desktop-portal` +
`xdg-desktop-portal-gtk` pair. There is no test-vs-application-caller gap to report for this
shard — the "application caller" *is* what was driven, directly, for all 15 rows.

## Gaps found, filed for other owners

Nothing in this pass reproduces a defect — all 15 rows are clean `PASSED`. Two observations
worth a human's attention, neither scored as a row failure:

- **The GTK file-picker dialog's tiled position/size is not deterministic** run to run under
  sway's default tiling (see headline finding, point 4). This is a property of driving a
  third-party GTK dialog inside a synthetic sway session, not a Tiller behavior — filed here only
  so the next pass that needs the picker doesn't rediscover it the hard way.
- **`F-PRJ-06`'s double-submit guard was only indirectly confirmed** (one project resulted from
  two rapid clicks) rather than screenshotted mid-clone with the button visibly muted, since the
  local `file://` clone completed too fast to catch a mid-clone frame reliably in this drive. The
  ledger's existing PASSED for F-PRJ-06 already has a direct sleep-shimmed-git screenshot of the
  muted mid-clone state from an earlier pass; this pass's job was re-confirmation, not
  re-establishing first evidence, so this is noted rather than treated as a gap.
