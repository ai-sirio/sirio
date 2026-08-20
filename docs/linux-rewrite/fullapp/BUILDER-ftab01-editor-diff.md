# Builder live drive — F-TAB-01, TabKind::Editor and TabKind::Diff opened live

Closes the row's remaining gap: `TabKind::Editor`/`TabKind::Diff`
(`rust/crates/tiller_project/src/tab.rs:10-21`) had never been opened live —
only Terminal/AgentChat/Browser tab decorations had direct screenshots. The
gap was blamed on the missing xdg-desktop-portal (see F-TAB-09), but that
portal is not actually a precondition for this row: most routes to both
`TabKind`s carry an already-known path and never touch a file dialog. This
drive opens both kinds through those no-portal routes and, separately, as a
second deliverable, attempts the Ctrl-O/native-picker path against F-TAB-09.

Worktree `/var/tmp/tt-ftab01-610923`, branch `verify/f-tab-01-610923`, off
`origin/linux/gpui-waku` at `f45fa9b0`. No code changes — verification only.
Own `CARGO_TARGET_DIR` (`/var/tmp/cargo-target-ftab01-610923`), own binary
snapshot (`/var/tmp/tt-ftab01-bin-610923`), own `TILLER_WL_LABEL`
(`ftab01-610923`), own fixture: a fresh standalone git repo at
`/var/tmp/tt-ftab01-fixture-610923` with one committed file (`notes.md`,
`README.md`, `src/main.rs`) and one uncommitted edit to `notes.md`, so the
Changes view has real content without touching any other agent's project.

## Outcome: both TabKinds opened live, through two routes each

## TabKind::Editor — file-tree double-click, no portal

`add_file_tab` (`main.rs:6964`) is reached from four routes; none of them
need the native picker (`RightPanelEvent::OpenFile`, `FileViewEvent::OpenFile`,
`ChatEvent::OpenFile`, `ChangesTabEvent::OpenFile` — only `Ctrl-O`/the
tab-context "Open File" menu item goes through `cx.prompt_for_paths`
instead). Used the simplest: the Files panel's file-tree row
(`tiller_ui/src/right_panel.rs:630-641`).

One thing worth recording precisely, since it cost a wasted round trip: file
rows gate on `event.click_count >= 2` — **a single click only selects/focuses
the row** (`right_panel.rs:636-639`, "Files follow the inventory's
double-click contract"). A first attempt with one `click` action produced
exactly the half-state that looks like nothing happened (row highlighted,
no tab, `panel.list` empty) until the source made the double-click
requirement explicit. Two `click` DSL calls at the same coordinate, back to
back, registered as one double-click.

`01-editor-tab-live.png`: `notes.md` open as a real Editor tab — tab bar
shows `notes.md` with a close control, the file path
(`/var/tmp/tt-ftab01-fixture-610923/notes.md`), a `Markdown` language badge,
and a `Preview`/`Code` mode toggle (currently in Preview, rendering the
heading and both paragraphs — including the uncommitted second paragraph,
confirming this is the live working-tree content, not a stale/committed
copy). `panel.list` corroborates a real registered pane:
`{"active":"true","id":"pane-0","tab":"notes.md","title":"notes.md"}`.

## TabKind::Diff — two independent routes, neither needs the portal

**Generic route** — `+` (new-tab button) → `Changes` menu item
(`NewTabAction::NewChanges` → `add_changes_tab(None, cx)`, `main.rs:7531`).
`02-diff-tab-generic-live.png`: a `Changes` tab with `Local changes (1)`,
a `Changed (1)` section listing `notes.md` with its `-0 +2` stat, `Unified`/
`Split` toggle, `Expand All`/`Collapse All`/`Stage all`/`Discard all`
controls — unambiguously the Diff surface, not a generic file list.

**Path-specific route** — inside a Changes tab, clicking a changed-file row
expands it (`toggle_change`) to reveal `Discard`/`Stage`/`Open diff`; clicking
**Open diff** emits `ChangesTabActionEvent::OpenDiff(path)`
(`changes.rs:1273-1284`), handled at `main.rs:6453-6454` as
`workspace.add_changes_tab(Some(path), cx)` — a **second**, distinct
`TabKind::Diff` tab, pre-focused on that file
(`ChangesTab::focus_path`, `changes.rs:1036`). `03-diff-row-expanded.png`
shows the row expanded with the real unified diff hunk (`@@ -1,3 +1,5 @@`,
the two added lines highlighted) and the action row.
`04-diff-tab-pathspecific-live.png` shows the result: a new, active `Changes`
tab (fourth in the sidebar's tab list, since each `wayland-drive.sh`
invocation in this drive re-opens the app against the same session and
prior tabs restore) already expanded to the same hunk, with no extra click
needed — confirming `focus_path` actually ran, not just that a generic tab
opened again.

`panel.list` after this step: four `Changes` panes registered, the newest
(`pane-3`) active — corroborating the tab count and focus state independent
of the screenshots.

## Cleanup

Torn down: `dbus-daemon`, `sway`, virtual-pointer client, and app binary all
killed by PID under label `ftab01-610923`, confirmed via `pgrep`.

## Second deliverable: the Ctrl-O/native-picker leg (F-TAB-09) — reproduced

Separate instance, separate label (`ftab09-610923`), same binary snapshot,
same worktree. Kept independent of the row above by design: this is an
attempt against F-TAB-09's recipe, not a precondition for F-TAB-01, which was
already closable on the no-portal routes alone.

**A stale portal appeared exactly as F-TAB-09's own write-up describes.**
`wayland-drive.sh TILLER_WL_PORTAL=1` starts `xdg-desktop-portal` before the
compositor's real `WAYLAND_DISPLAY` is in the bus's activation environment —
so when the app's first `OpenFile` call activated the GTK backend, it died
immediately (`Gtk-WARNING: cannot open display: `) and the frontend latched
"No skeleton to export" for the rest of its process lifetime. Confirmed, not
assumed: `dbus-send ... GetNameOwner org.freedesktop.portal.Desktop` named
the frontend's PID, and `/proc/<pid>/environ` had no `WAYLAND_DISPLAY` at
all.

**The fix, applied by hand rather than via `TILLER_WL_PORTAL=1` alone** (that
flag starts a portal blindly — it performs neither of the two steps below,
which is exactly why it produces this trap):

```bash
dbus-update-activation-environment --verbose WAYLAND_DISPLAY=wayland-5 GDK_BACKEND=wayland \
    XDG_RUNTIME_DIR=/run/user/1000 DBUS_SESSION_BUS_ADDRESS=unix:path=/tmp/ftab09-610923-dbus.sock \
    XDG_CURRENT_DESKTOP=GNOME
kill -9 <stale xdg-desktop-portal pid>
XDG_CURRENT_DESKTOP=GNOME WAYLAND_DISPLAY=wayland-5 nohup /usr/libexec/xdg-desktop-portal -v \
    >/tmp/ftab09-610923-portal2.log 2>&1 &
```

Verified the fix landed before touching the UI: `GetNameOwner` on the new PID,
then `/proc/<gtk-backend-pid>/environ` showing `WAYLAND_DISPLAY=wayland-5
GDK_BACKEND=wayland` this time — not inferred from the dialog rendering
later, checked directly first.

**Driving the picker required a small technique change.** `wayland-drive.sh`
has no attach mode — a second invocation under the same label kills and
relaunches sway (`kill_ours` at start-of-run), which would have picked a new
`wayland-N` and orphaned the portal fix above (bound to `wayland-5`
specifically). Instead of restarting, I wrote a small driver
(`/tmp/attach-ftab09.sh`, not committed — a throwaway harness, not app code)
that copies `pointer_command`/`click`/`rightclick`/`shot`'s bodies verbatim
from `Scripts/wayland-drive.sh` and points them at the already-running,
`TILLER_WL_KEEP=1` session's existing `$VP_FIFO`/`$SWAYSOCK`/`$SOCK`, so
gestures land in the same coordinate space and protocol without touching the
compositor.

**Result:** `rightclick` on the Terminal tab opened its context menu
(`05-portal-context-menu.png`) with "Open File" as the top entry — clicking
it opened a **real native GTK file-chooser** (`06-portal-native-picker.png`):
titled "Open File", a genuine sidebar (Recenti/Home/Documenti/…), and —
conclusive that this is the real desktop portal and not a mock — a Recent
Files list populated with actual files from *other* agents' sessions on this
host (`wf-rest4-files/notes.md`, `wf-tab-fixture/README.md`, etc.), which
only a live `org.freedesktop.impl.portal.desktop.gtk` talking to the real
GTK recent-files store could produce. Selecting `notes.md` and clicking
"Open File" closed the dialog and opened a new tab titled `notes.md` in the
tab bar, with a close control and file icon matching `add_file_tab`'s pattern
from the no-portal route above (`07-portal-editor-tab-opened.png`).

One honest caveat, stated rather than smoothed over: this instance never had
a project/worktree added (I only booted it and fixed the portal — no
`project.add`), so the content pane still shows the app's workspace-level
"No worktree selected" placeholder under the new tab. That's expected and
orthogonal to what this deliverable is testing — the picker/portal path,
proven above — not a sign `add_file_tab` failed; the Editor tab's *content*
rendering was already fully proven separately in the no-portal section, on
an instance that had a real fixture worktree open.

**Verdict: reproduced.** F-TAB-09's two-layer recipe holds independently of
the predecessor's run — a second, cold instance, fixed by hand rather than
copy-pasting a working config, produced the same real dialog. One process
note for whoever runs this recipe next: the very first `TILLER_WL_PORTAL=1`
boot attempt for this instance hung for 90s with zero output (no `SAFE:`/
`SHOT` lines, no log files at all) under concurrent load from another
agent's own `wayland-drive.sh` session on this box (load average ~5–7 at the
time); a plain retry a few minutes later booted normally in under 10s. Not
investigated further since it didn't reproduce — logged here in case it
recurs for someone else under similar contention.

Cleanup: `dbus-daemon`, `sway`, the two portal generations, the GTK backend,
the virtual-pointer client and the app binary were all killed by PID and
confirmed gone via `pgrep`; `/tmp/ftab09-610923*` files removed.
