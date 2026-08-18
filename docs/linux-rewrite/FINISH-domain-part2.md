# Finish line — domain / settings / persistence, part 2 (lane wf-dom3)

Cites `docs/linux-rewrite/EVIDENCE-STANDARD.md`. Live drives on this host (x86, COSMIC/Wayland,
`WAYLAND_DISPLAY=wayland-1`), 2026-08-18, via `Scripts/wayland-drive.sh`. Binary pinned:
`cp rust/target/debug/tiller /tmp/wf-dom3-tiller && export TILLER_WL_BIN=/tmp/wf-dom3-tiller`.

## Rows in scope

Grep of `INVENTORY-LEDGER.md` for prefixes `F-CORE-DOM`, `F-SET`, `F-PER`, `F-WIN`, `F-CORE-USG`,
`F-CHG`, `F-TERM` that are not `PASSED` and not `N/A — platform`, as of HEAD before this pass:

| id | prior verdict |
|---|---|
| F-WIN-03 | half-proven |
| F-WIN-10 | half-proven |
| F-CHG-02 | FAILED — defective |
| F-CHG-15 | half-proven |
| F-CHG-18 | FAILED — defective |
| F-PER-01 | half-proven |
| F-PER-05 | NOT EXERCISED |
| F-PER-07 | NOT EXERCISED |
| F-SET-14 | half-proven |
| F-SET-15 | FAILED — absent |
| F-SET-18 | half-proven |
| F-SET-22 | half-proven |
| F-TERM-03 | half-proven |
| F-TERM-10 | half-proven |
| F-TERM-SCR-02 | half-proven |
| F-TERM-PTY-04 | half-proven |
| F-TERM-UI-02 | half-proven |
| F-CORE-DOM-02 | half-proven |
| F-CORE-DOM-03 | half-proven |
| F-CORE-DOM-05 | half-proven |
| F-CORE-DOM-07 | half-proven |
| F-CORE-DOM-08 | half-proven |
| F-CORE-USG-07 | half-proven |

Priority per brief: the five rows a predecessor left NOT EXERCISED (F-WIN-03, F-WIN-10, F-PER-01,
F-PER-05, F-PER-07) — driven first, below — then F-CORE-DOM-03/07 (currently carried on unit
tests only), then remaining half-proven rows as budget allows.

Status: IN PROGRESS — rows appended below as driven, each committed individually.

---

## F-WIN-03 — PASSED (was half-proven / UNREACHABLE-assumed)

**The old "no session D-Bus" assumption is wrong on this host.** Checked first, per the brief:
built a private session bus (`dbus-daemon --session --fork`), pointed its activation environment
at the nested compositor's real display (`dbus-update-activation-environment
WAYLAND_DISPLAY=$WD ...`), started `/usr/libexec/xdg-desktop-portal -v`, and confirmed live:
`org.freedesktop.portal.Desktop` registers `org.freedesktop.portal.FileChooser` backed by
`gtk.portal` (`XDP: providing portal org.freedesktop.portal.FileChooser` in the portal's own log),
and `dbus-send ... GetConnectionUnixProcessID org.freedesktop.portal.Desktop` returns a live PID.
A FileChooser backend **is** registered on this session bus.

One real trap found and fixed along the way: starting a **second** `xdg-desktop-portal` frontend
process against an already-running one (leftover from an earlier manual check) raced the bus-name
request and produced `Couldn't open file picker due to missing xdg-desktop-portal implementation`
— `ashpd::Error::PortalNotFound`, `rust/vendor/gpui_linux/src/linux/platform.rs:430` — even though
a portal genuinely was there. Killing the stale duplicate and starting exactly one frontend per
drive fixed it; recorded here since the error text is easy to misread as "no portal environment"
when the real cause is "two portals fighting over one bus name."

**Full live round trip, one continuous `wayland-drive.sh` invocation** (`TILLER_WL_LABEL=wf-dom3`,
binary pinned to `/tmp/wf-dom3-tiller`):

1. Fixture `/home/enzopalmisano/wf-dom3-fixtures/open-me.txt` seeded with `WFDOM3_ORIGINAL_LINE`.
2. `chord ctrl o` → GPUI's `handle_open_file` (`rust/crates/tiller/src/main.rs:9099`) called
   `cx.prompt_for_paths`, which dispatched a real `ashpd::desktop::file_chooser::OpenFileRequest`
   over the private bus; the **real GTK "Open File" dialog** mapped as an independent sway tile,
   floated/positioned via `swaymsg`.
3. Navigated by real clicks (not the location bar — see below): clicked **Home**, clicked the
   `wf-dom3-fixtures` row, `key Return` to descend, clicked `open-me.txt`, `key Return` to open.
4. The picker closed and a new `open-me.txt` editor tab appeared in the real workspace, showing
   line 1 `WFDOM3_ORIGINAL_LINE` — screenshot
   `reference/linux-progress/wf-dom3/f-win-03-file-opened.png`.
5. Clicked into the editor, `chord ctrl a`, `type WFDOM3_EDITED_CONTENT_9182`, `chord ctrl s`.
6. **Hard discriminator**: `cat /home/enzopalmisano/wf-dom3-fixtures/open-me.txt` on the real host
   filesystem, read directly (not through the app) after the drive, now reads
   `WFDOM3_EDITED_CONTENT_9182` — the file genuinely changed on disk. Tab shows no dirty-dot after
   save (`reference/linux-progress/wf-dom3/f-win-03-after-save.png`).

**A GTK location-bar (`chord ctrl l` + `type <path>`) trap found and abandoned**: the first
character(s) typed immediately after `chord ctrl l` were dropped twice in a row (`/home/...`
arrived as `/ome/...`; a second attempt lost both a doubled leading marker and `home`), and the
second attempt actually landed in GTK's **interactive search** ("Ricerca in Recenti") rather than
a location-entry, not the location bar at all — this GTK version's `ctrl+l` behavior is not
reliable through synthetic `wtype` input immediately after the dialog maps. Switched to real
navigation clicks (Home → folder row → Return → file row → Return), which worked cleanly and is
the more representative gesture anyway (a person browses more often than they type an exact path).
Filed here as a lane trap for the next builder who reaches for `ctrl+l`.

Evidence standard: named gesture sequence above is replayable; the disk-content assertion is the
hard discriminator (EVIDENCE-STANDARD.md's bar — a value that could only differ if the feature
truly worked).

---

## F-WIN-10 — PASSED (was half-proven)

Real UI gesture, not a socket call (`control_add_project`, the control-socket handler, is a
**separate** function from `add_project` — `rust/crates/tiller/src/main.rs:3306` calls
`workspace.control_add_project`, not `add_project` — and never calls `show_toast`. Driving this
row over `ctl project.add` produces no toast at all regardless of outcome; confirmed live first as
a negative control, then abandoned in favor of the real `+` → **Open Project…** UI path that
actually calls `add_project`, `rust/crates/tiller/src/main.rs:4031`).

Full live sequence, one continuous `wayland-drive.sh` invocation, real portal dialog both times:

1. `+` → **Open Project…** → real GTK "Open Folder" dialog → Home → `wf-dom3-fixtures` (a plain,
   non-git folder) → **Add Project** → "This folder is not a git repository" confirmation card →
   **Add without Git**. First time: added cleanly, no toast (`Ok(true)` branch, screenshot
   `reference/linux-progress/wf-dom3/f-win-03-...` sibling — sidebar now lists `wf-dom3-fixtures`
   as a project).
2. Repeated the **identical** gesture a second time against the same already-tracked folder.
3. **`reference/linux-progress/wf-dom3/f-win-10-toast-visible.png`**: a bottom-right floating card
   reading `already tracked or nested: /home/enzopalmisano/wf-dom3-fixtures` appears — matches
   `render_toast`'s real styling (`rust/crates/tiller/src/main.rs:9941`: `.absolute().bottom_20()
   .right_20()...`), distinct from the sidebar's own persistent inline banner (bottom-left,
   already visible in the same frame).
4. **`reference/linux-progress/wf-dom3/f-win-10-toast-gone.png`**, captured 4.5s later (>
   `TOAST_DURATION = Duration::from_secs(4)`, `main.rs:5310`): the bottom-right toast is gone while
   the sidebar's persistent inline banner (`Sidebar::set_notice`) is still showing the same text —
   exactly the distinguishing behavior the row's own doc comment names ("unlike
   `Sidebar::set_notice`'s persistent inline banner, which stays silent unless the sidebar happens
   to be the visible surface"). Both halves of the VERIFY clause (appears / auto-dismisses) driven
   live and distinguished from the lookalike persistent banner.

---

## F-PER-05 — PASSED (was NOT EXERCISED)

Boot state (persisted DB, `linux/gpui-waku` worktree) had exactly two launch-snapshot tabs: `Chat`
and `Terminal`. Live sequence, one continuous `wayland-drive.sh` invocation:

1. Clicked the `Terminal` tab, `chord ctrl w` → a real "Close dirty tab? Discard unsaved work in
   Terminal?" confirmation appeared (the terminal had a real neofetch-style banner as scrollback,
   correctly flagged dirty) — clicked **Close**.
2. `reference/linux-progress/wf-dom3/f-per-05-after-close.png`: tab bar and sidebar both now show
   only `Chat` — `Terminal` genuinely gone, not just visually hidden (sidebar's per-worktree tab
   list dropped it too).
3. `chord ctrl+shift o` → `handle_restore_launch_snapshot`
   (`rust/crates/tiller/src/main.rs:9083`, `RestoreLaunchSnapshot`).
4. `reference/linux-progress/wf-dom3/f-per-05-after-restore.png`: tab bar now shows **both** `Chat`
   and `Terminal` again — exactly one `Terminal` (not duplicated), and `Chat` is the same tab,
   untouched throughout (never closed, never recreated). Matches `restore_launch_snapshot`'s own
   merge logic (`main.rs:5249`: `merge_launch_snapshot_tabs` then only appends tabs past
   `current.len()`) exercised end-to-end, not read.

Both clauses of the VERIFY text — "the tab returns" and "an unrelated current tab remains" — driven
live in the same session with a real dirty-tab confirmation in between, not glossed over.

---
