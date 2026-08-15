# Wave D slice D-U — verdicts

Critic pass, 2026-08-15. Wayland lane only (`Scripts/wayland-drive.sh`), instances labelled
`du2`..`du6`, all torn down after use. No code edited. `INVENTORY-LEDGER.md` not touched — this
file is the input the orchestrator merges in.

Note: this run's copy of `Scripts/wayland-drive.sh` has an uncommitted local diff (not made by me)
that adds `rightclick`/`down`/`up`/`drag`/`scroll`/`chord`/`modclick` actions beyond the
`click/move/type/key/title/shot` vocabulary the prior sweep found. I did not rely on `drag` for
F-CHG-18 (still unproven either way — see below) but flag this so the next pass knows the script's
vocabulary is currently wider than committed history shows.

## `F-PRJ-07` — clone failure and retry — **PASSED** (was half-proven)

Files: `rust/crates/tiller_ui/src/project_forms.rs` (`CloneForm`, `on_url_key`, retry button),
`rust/crates/tiller_ui/src/sidebar.rs` (`start_clone_project`, `render_project_form` overlay),
`rust/crates/tiller_git/src/clone.rs` (`clone_repository`).

Drove the full arm end-to-end in one `wayland-drive.sh` invocation (state does not survive across
separate invocations — `kill_ours` runs unconditionally on every call, even under
`TILLER_WL_KEEP=1`, so a multi-step flow must live inside one action block). Opened `+` → "Clone
Repository…", typed `https://invalid.example.invalid/nope.git`, submitted: got a real failure —
`Clone failed: git exited with status 128: ... fatal: unable to access '...': Could not resolve
host: invalid.example.invalid` — and the button relabelled "Retry clone"
(`du3-shots/03-t8.png`). Selected the URL field, cleared it (50× BackSpace), typed
`https://github.com/octocat/Hello-World.git`: the error cleared, Destination re-derived live to
`/home/enzopalmisano/Hello-World`, button relabelled back to "Clone repository"
(`du4-shots/03-corrected-url.png` / `du5-shots/03-corrected-url.png`). Clicked it: the sidebar
gained a real "Hello-World" project with a `master` worktree (`du5-shots/04-final-result.png`), and
`/home/enzopalmisano/Hello-World/.git` existed on disk with a real `README` — a genuine `git clone`,
not a UI-only transition. Deleted the directory afterward.

This directly overturns the prior "anchored-popover input gap blocks the correct-URL retry arm"
finding: the URL field accepted edits and the destination re-derived correctly after a failure, in
this build, on this lane. Both arms (invalid-URL failure, corrected-URL retry-and-succeed) are now
proven with a real network clone as the discriminator — a state the app would never reach on its
own.

## `F-PRJ-14` — project avatar (GitHub / PNG / favicon) — **half-proven** (unchanged)

Files: `rust/crates/tiller_ui/src/project_identity.rs` (`AvatarSource`, `render_avatar_mode`),
mounted via `rust/crates/tiller_ui/src/sidebar.rs:31,853,2021`.

Did not re-drive the favicon arm — already live-proven in the prior sweep (label + persisted
restart) and unchanged in this tree. Read `render_avatar_mode`: for `AvatarSource::GitHub`, the UI
only ever shows a text caption `"Current: GitHub avatar for {id}"` — there is no code path that
fetches or renders an actual avatar image for this arm (the file's own header comment says the
avatar network-fetch seam is "deliberately" not built here, routed elsewhere). So the GitHub-avatar
arm is in the same class as the PNG-upload arm already found blocked (no visible image ever
appears), not a distinct proven/unproven split — it's untested-and-structurally-thin rather than
untested-and-blocked-by-portal. Favicon-domain arm stands proven; GitHub and PNG arms remain
unproven, GitHub for a new, more specific reason than previously recorded.

## `F-CHG-18` — drag a changed-file diff into a pane — **NOT EXERCISED** (unchanged)

Files: drag source `rust/crates/tiller_ui/src/changes.rs` (`DiffPayload`, `.on_drag` ~L992-994);
drop target `rust/crates/tiller_terminal/src/lib.rs:1551` (`.on_drop::<(PathBuf, String)>` on the
running-terminal element, wired to `terminal.receive_diff_drop`). Both are real, non-test
production code — the terminal pane genuinely accepts a typed `(PathBuf, String)` GPUI drag payload
matching `changes.rs`'s `DiffPayload` exactly.

The comment in `right_panel.rs` claiming this needs a drop target routed to another owner is stale
— a real target already exists in `tiller_terminal`. The committed `wayland-drive.sh` has no `drag`
action; this working tree's *uncommitted* copy does add one (see note at top), but a button-held
drag needs the mouse to stay down across positions and I did not trust an unreviewed, uncommitted
script change enough to certify a PASS on it for this row — did not attempt it. Grading unchanged
until `drag` ships committed and someone drives it. Next wave: try
`drag <source-row-xy> <target-pane-xy>` once the script change lands for real.

## `F-CHG-22` — running/needs-input/done/error/idle activity statuses — **half-proven** (evidence strengthened)

Files: `rust/crates/tiller_activity/src/status.rs` (`AgentStatus`), `rust/crates/tiller_ui/src/right_panel.rs`
(`render_activity`, `activity_status_glyph`, `toggle_activity`).

Instrument: control-socket `notify` (the only legitimate way to set Layer-A status per
`WAYLAND-LANE.md`) against a real pane id from `panel.list`, forced-repaint screenshots after each.
Added `project.add path=<this repo>`, took the pane id `pane-1` (Terminal), then drove
`notify status=running` → tab badge became a live orange dot, Activity header showed "1 running"
(`du6-shots/03-activity-running.png`); `status=needs-input` → tab badge became `?`
(`04-activity-needsinput.png`); `status=error` → tab badge became `!` with a red worktree dot
(`05-activity-error.png`); `status=done` → tab badge became `✓` (`06-activity-done.png`). Idle is
the pane's own starting state before any notify (no badge at all,
`02-activity-idle.png`/`02-loaded.png`). All five states are visually distinct in a real build —
this is stronger, more specific evidence than the "data-tier" description previously on file.

Did not manage to open the collapsed "Activity" section itself: clicked its full-width header row
(`.on_click` in `right_panel.rs:826`, height 27px) at three plausible y-offsets in the same
bottom-anchored band (918/928/936 at 1715×972) and it stayed collapsed every time, tab badges
notwithstanding. Can't tell whether that's a real defect or a lane instrumentation limit this close
to the viewport edge — didn't attempt a fourth guess. Visual expand/collapse stays unproven;
verdict unchanged at half-proven, but the proven half is now backed by four distinct forced-repaint
frames instead of ambiguous ones the previous sweep found indistinguishable from cursor movement.

## `F-SET-24` — grant/revoke browser-origin permissions, empty state — **half-proven** (evidence strengthened)

Files: `rust/crates/tiller_ui/src/browser.rs` (`request_permission`, `allow_permission`,
`revoke_origin`), `rust/crates/tiller_ui/src/settings.rs` (`render_permissions`,
`render_browser_grants`), wiring `rust/crates/tiller/src/main.rs:8566` (`on_revoke_browser_origin`
only).

Re-drove the empty state live: `surface.settings.open` + `surface.settings.select
section=permissions`, forced-repaint capture shows "Granted browser origins" / "No browser origins
have been granted." with a "Revoke all" control (`du6-shots/02-settings-permissions.png`) — genuine,
reproduced fresh this pass.

Sharper finding on the other half than previously recorded: `grep -rn request_permission
--include=*.rs .` outside `tests/` returns exactly two definitions (`browser.rs:588` state method,
`886` view wrapper calling it) and zero production call sites anywhere in `crates/tiller/src`
(the app binary) or elsewhere — only unit tests call it. `revoke_origin`/`revoke_all` are wired from
`main.rs`; `request_permission` is not wired from anywhere. This means the grant arm isn't merely
blocked by the missing native-file-dialog/X11 gap the prior sweep filed it under — there is
currently no code path in this app, on any lane, that ever calls it outside a test. Grading stays
half-proven (empty state is real, live, reproduced), but the open half is now "unreachable
production code", a stronger and more precise claim than "owed to X11 lane".
