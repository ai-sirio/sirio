# T9-misc build plan — F-USE, F-WIN, F-EDIT, F-PER, F-GIT, F-PERSIST, F-AUTO

Read-only triage output for `docs/linux-rewrite/triage/T9-misc.md`'s 21 rows. No code changed,
no verdict set, `INVENTORY-LEDGER.md` untouched. Each row below answers: what does it actually
need, and which files would a fix touch.

## Shared causes found (read this first — this is where the leverage is)

**Cluster 1 — four rows are marked FAILED on evidence that a same-day builder fix already
falsifies.** `F-AUTO-06`, `F-AUTO-09`, `F-USE-06`, `F-WIN-06` all carry `FAILED — defective`
verdicts whose cited evidence (empty D-Bus capture, `{"queued":"true"}` stubs, "zero production
callers", "empty match arm") is **contradicted by the current source**, confirmed by direct
reading plus `git log -S`/`git blame` timestamps showing the fix landed *before* the ledger
evidence's own timestamp in three of the four cases. See each row for the specific commit and
line. This is not a request to flip the verdict — it's flagged `reclassify` per the brief — but
a critic re-running these four with a fresh build should find them already working. Likely
mechanism: the evidence was gathered against a stale snapshot/binary, or carried forward across
a sweep without rebuilding against the concurrent builder's landed commit.

**Cluster 2 — the UI chat transcript never reaches persistence, and two rows are the same
defect.** `F-PER-01`'s failed "chats" conjunct and `F-PERSIST-DB-05` are one root cause:
`tiller_ui/src/chat.rs`'s `restore_transcript` is fed only by the in-memory `retained_chats`
list (same-process tab reopen), `tiller_persistence`'s `save_chat_transcript`/
`load_chat_transcript` are called only from `tiller_acp/src/chat.rs` (the socket-driven
`ChatSession` path), and `rust/crates/tiller/src/session.rs` — the DB-backed restore-across-
relaunch snapshot — has zero occurrences of `transcript`. One wiring fix (UI chat turn
completion → `save_chat_transcript`; UI chat tab restore → `load_chat_transcript` →
`restore_transcript`) closes both rows.

**Cluster 3 — this Wayland lane cannot deliver the input a few gestures need.**
`F-EDIT-12` (drag has no button-down-only/motion-while-held primitive in
`wayland-virtual-pointer.c`) and `F-GIT-RUN-02` (`wtype -k Return`/`Escape` do not register in
GPUI on this compositor, reproduced on an unrelated field) are both instrument limits, not code
defects — the code underneath is already correct per `P81`/`P120-report.md`. `F-EDIT-08`'s
"no picker ever appeared" likely belongs in this cluster too (see its row) but needs one more
check (a D-Bus portal capture) before it's certain. None of these three need a source change;
they need an X11 lane or a driver that can deliver real key/pointer events.

**Cluster 4 — F-USE-02 and F-USE-03 share one broken precondition.** The "unavailable segment"
and "PATH-stripped" halves of both rows were tested by editing `PATH` on an *already-running*
Tiller process. A process's `PATH` is captured at exec and does not observe later shell edits,
so the strip never reached the app — that's why both providers still read "Signed in". Any
retest needs the strip applied **before launch** (`env -u PATH` or a scrubbed `PATH` at process
start), not after.

---

## `F-AUTO-06` — FAILED — defective (ledger)

**needs: reclassify.** `record_notification` (`rust/crates/tiller/src/main.rs:717-744`) already
stores the notification *and* calls `self.notification_poster` — confirmed by reading the exact
commit (`391dd5d`) the ledger evidence was written against: the source at that commit already
contains the poster call. Commit `a5d09d7` ("fix: wire notification delivery and restored
agents", 2026-08-14 13:42) landed **nine hours before** the ledger's `391dd5d` entry
(22:39) that still claims "record_notification pushes to an in-memory Vec... without ever
touching the notifier." `docs/linux-rewrite/P118-report.md` (17:45, between the two) documents
a live proof: a preserved `dbus-monitor` capture at
`reference/linux-progress/p118-notify/dbus-monitor.txt` shows a real
`org.freedesktop.Notifications.Notify` call carrying the exact title/body sent through
`tillerctl notify`. The ledger's "empty capture" evidence for the same day either used a stale
binary or hit an environment/timing race (dbus-monitor started after the call, or a different
`DBUS_SESSION_BUS_ADDRESS` than the live app).

**files:** none required if the reclassify holds. If a critic wants to settle it definitively:
`rust/crates/tiller/src/main.rs` (`record_notification`, `post_desktop_notification`) is where
to look; no edit is indicated by current reading.

**approach:** re-run the exact `P118-report.md` gesture — start `dbus-monitor` first, then call
`tillerctl notify --title ... --body ...` against a freshly built binary — and compare against
the preserved artifact. If it reproduces empty, that's a *new*, real regression worth its own
row; if it reproduces non-empty, the ledger row should be closed as stale.

**size:** S.

## `F-AUTO-09` — FAILED — defective (ledger)

**needs: reclassify.** The ledger's evidence is an "orchestrator socket probe, 2026-08-14
03:02:56" showing all seven non-open/navigate/act `browser.*` methods answering
`{"ok":true,"result":{"method":...,"queued":"true"}}`. Commit `988d9e9` ("fix: make browser
control responses honest", 2026-08-14 04:29:51 — **86 minutes after** the probe) added
`browser_request_error()` (`rust/crates/tiller/src/main.rs:200-236`), which now rejects any
method outside `BROWSER_CAPABILITIES = ["browser.open", "browser.navigate", "browser.act"]`
with an explicit `"{method} is unsupported on Linux: browser automation is not implemented"`
error, checked **before** the request is queued (`main.rs:1552-1562`, `queue_action`'s
`result.recv_timeout` then turns that `Err` into a real `ControlResponse::failure`, not a bare
`ok:true`). This is exactly the "explicit unsupported error" the row's VERIFY clause accepts as
a passing outcome.

**files:** none required. If re-verification is wanted: `rust/crates/tiller/src/main.rs`
(`browser_request_error`, `BROWSER_METHODS`/`BROWSER_CAPABILITIES`, `handle_browser_action`).

**approach:** re-probe the same eight methods over the control socket against a current build;
expect `ok:false` with the unsupported-method message for the seven, and a real result for
`browser.open`/`navigate`/`act`.

**size:** S.

## `F-EDIT-02` — FAILED — defective

**needs: build.** The formatting *logic* is correct — `Editor::format_bold`/`format_italic`
(`rust/crates/tiller_ui/src/editor.rs:707-736`, both call `toggle_wrap`) are pure, tested string
transforms over a `Selection`. The defect is upstream: **there is no mouse-driven text
selection at all.** `rust/crates/tiller_ui/src/file_view.rs`'s only mouse handler on the editor
region is `.on_mouse_down(MouseButton::Left, ...)` (`file_view.rs:589`), and it does nothing but
call `.focus()` — no caret placement, no click-drag selection, no double-click word-select. The
only way `source_selection` becomes non-`None` is keyboard `Shift+Arrow`/`Ctrl+A`
(`on_editor_key`, `move_caret`, `file_view.rs:380-393,461-462`). When a driver clicks/drags to
"select a word" (the row's own VERIFY wording), nothing is recorded, so
`formatting_selection()` (`file_view.rs:464-471`) falls back to `Selection::point(buffer.len())`
— a collapsed point at the **end of the file** — which explains "Bold left the selected word
unchanged" (the edit landed off-screen, at EOF, not on the visible word). This is a genuine gap,
not a logic bug in the format ops themselves.

**files:** `rust/crates/tiller_ui/src/file_view.rs` (add mouse-down caret placement, mouse-drag
selection, and ideally double-click word-select — needs a text-position-from-pixel mapping,
which this file does not currently have anywhere); `rust/crates/tiller_ui/src/editor.rs` only if
a word-boundary helper is added for double-click.

**approach:** implement pixel→buffer-offset hit-testing on mouse-down (needed for caret
placement regardless of selection), extend it to mouse-drag for range selection, and wire both
into `source_selection`/`caret` the same way `move_caret` already does for keyboard. Undo
(`ctrl-z`, also noted broken in the same evidence) is a separate, larger gap: there is no
undo/redo stack anywhere in `editor.rs`/`file_view.rs` (zero occurrences of `undo`) — out of
this row's own VERIFY clause but worth its own row if the ledger doesn't already have one.

**size:** M.

## `F-EDIT-08` — FAILED — defective

**needs: exercise** (leaning `build` only if the exercise below comes back negative).
`add_file_tab` (`rust/crates/tiller/src/main.rs:4331-4361`) already implements the row's actual
behavioral clause correctly: it scans open tabs for a matching `FileView::path()`, and on a
match sets `self.active_tab = index`, selects the tab in `tab_machinery`, and returns early
without creating a duplicate — exactly "focuses the existing document rather than presenting a
second copy." The evidence ("no picker ... in four focus contexts") never got far enough to
exercise that logic: `ctrl-o` calls `cx.prompt_for_paths` (`handle_open_file`,
`main.rs:6490-6516`), a native/portal file chooser, and it produced **zero** visible response
across four attempts. `docs/linux-rewrite/P118-report.md` independently confirms this exact API
*does* reach the D-Bus portal for a different flow (Add Project's `+`): "ashpd FileChooser
request created on the D-Bus" was accepted as sufficient proof there, without a rendered dialog
ever being screenshotted. `F-EDIT-12`'s Wayland lane already has one documented case
(`WAYLAND-LANE.md`) of a GPUI/portal surface that doesn't paint under this synthetic compositor
even though the request fires — this may be the same shape.

**files:** none indicated for the dedup logic. If the portal request itself doesn't fire (see
approach), the gap would be in `rust/crates/tiller/src/main.rs`'s `handle_open_file`.

**approach:** run `dbus-monitor --session "interface='org.freedesktop.portal.FileChooser'"`
while pressing `ctrl-o`, the same technique already validated for the Add-Project picker. If a
request appears, the picker mechanism is proven reachable and this lane simply can't render its
UI (tooling limit, matches Cluster 3) — the row then needs either an X11 lane or a scripted
portal response to actually supply a path and prove the dedup/focus behavior live. If no request
appears at all, that's a real dispatch gap worth re-diagnosing in `handle_open_file`.

**size:** S.

## `F-EDIT-12` — NOT EXERCISED

**needs: exercise.** Belongs to Cluster 3. `P81`'s `codex11` audit
(`docs/linux-rewrite/ADJUDICATION-BACKLOG.md:175-179`) already corrected the row's own framing:
the drag payload is `(PathBuf, String)`, not a bare path, and both the source
(`rust/crates/tiller_ui/src/changes.rs:651,992`) and the terminal drop target
(`rust/crates/tiller_terminal/src/lib.rs:1187`) implement it. The only blocker is this lane's
input tooling: `wayland-virtual-pointer.c` parses move and click only — click is hard-coded
move+press+release with no button-down-only/motion-while-held primitive, so a drag literally
cannot be composed from this harness. `WAYLAND-LANE.md` documents this as needing an X11
lane (`DISPLAY=:1`).

**files:** none — the code is already believed correct per P81's read; no source change is
indicated.

**approach:** exercise on an X11 lane (`DISPLAY=:1`) with a real button-down → motion → button-up
sequence dragging a Changes-list row into a terminal pane, confirming the terminal receives the
`(PathBuf, String)` payload.

**size:** S (once an X11 lane is available — this is pure driving time, no build time).

## `F-GIT-BRANCH-01` — NOT EXERCISED

**needs: build.** `GitBranches::list`/`list_branches`
(`rust/crates/tiller_git/src/branches.rs:14,36`) are correctly implemented but have **zero
production callers** anywhere in the app — confirmed by grep, matching P116's finding. This
isn't just an untested function: there is **no UI surface that lists branches at all**. The only
branch-related UI is `sidebar.rs`'s "New Worktree" branch-name **prompt**
(`rust/crates/tiller_ui/src/sidebar.rs:280-289,1294-1383`), which is free-text entry feeding
`create_worktree` directly — it never reads existing branches back through the Git layer (no
autocomplete, no duplicate-name check, no picker). The row's VERIFY ("list them through the Git
layer, and compare exact names") has no reachable route today, by design, not by oversight.

**files:** `rust/crates/tiller_git/src/branches.rs` (already correct, reusable as-is);
`rust/crates/tiller_ui/src/sidebar.rs` (the branch-name prompt is the natural call site for a
duplicate-name/validation check against `list_branches`); alternatively/additionally
`rust/crates/tiller/src/main.rs` if a control-socket door (`git.branches`, mirroring the
existing `notification.*`/`session.*` pattern) is the chosen route instead of a UI picker — a
socket door is the smaller, more surgical fix since the row's VERIFY doesn't require a full
autocomplete widget, just an exercisable route through the Git layer.

**approach:** pick one reachable route (branch-name duplicate validation in the New Worktree
prompt is the smallest UI-shaped fix; a `tillerctl`-reachable listing method is the smallest
socket-shaped fix) and wire `list_branches` into it, including a fixture with a space in the
branch name to satisfy the row's own naming requirement.

**size:** S–M depending on route chosen (socket door: S; UI validation/autocomplete: M).

## `F-GIT-REMOTE-01` — half-proven

**needs: build.** `GitRemote::project_name` is confirmed reachable
(`rust/crates/tiller_ui/src/project_forms.rs:704`, the repo-clone form's `destination_for`) —
that half is genuinely proven. `github_owner`/`github_owner_from_url`
(`rust/crates/tiller_git/src/remote.rs:13,20,76`) have zero callers outside `tiller_git` itself
(re-export + tests), confirmed independently by the critic's own re-grep. Cross-checking the
Swift original: `GitRemote.githubOwner` fed exactly one feature there —
`App/ProjectSettingsSheet.swift:240-293`, a "Use GitHub Avatar" button that's enabled only when a
GitHub remote is auto-detected and pre-fills the avatar picker. The Rust rewrite's equivalent
Avatar picker (`rust/crates/tiller_ui/src/project_identity.rs`, `AvatarSource::GitHub`,
documented under `F-PRJ-14`) **exists and works live** (P118/E09 sweep: typing "octocat"
manually and clicking "Use GitHub Avatar" fetches and applies it) — but the identifier field is
always free-typed; nothing pre-fills or auto-detects it from the repo's actual remote via
`github_owner`. That's the specific gap: the auto-detect step from the Swift original was
dropped in the rewrite.

**files:** `rust/crates/tiller_git/src/remote.rs` (already correct, reusable as-is);
`rust/crates/tiller_ui/src/project_identity.rs` (where the GitHub-avatar identifier field and
`AvatarSource::GitHub` picker live — this is the single file that would need the pre-fill wired
in, per the earlier F-PRJ-14 sweep's own file reference).

**approach:** when the Avatar tab opens (or the GitHub-avatar option is selected) for a project
with a GitHub remote, call `github_owner(repo_path)` and pre-fill the identifier field with the
result, leaving manual entry as a fallback for non-GitHub or ambiguous remotes.

**size:** S.
