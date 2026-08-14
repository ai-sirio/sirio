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

## `F-GIT-RUN-02` — NOT EXERCISED

**needs: exercise.** Belongs to Cluster 3. `GitRunner::run_streaming`
(`rust/crates/tiller_git/src/git.rs:107-121`, splitting stderr on CR/LF and delivering each line
as it arrives) is real, tested streaming and has a genuine production caller —
`rust/crates/tiller_git/src/clone.rs:33`. `docs/linux-rewrite/P120-report.md:177-193` already
drove this live: the "New Worktree..." form opens, the branch-name field takes real typed text
(`wtype` text injection confirmed twice), but **`wtype -k Return`, an explicit press/release
pair, a literal embedded newline, and `wtype -k Escape` all failed to submit or cancel the
form** — and the identical "text lands, Return/Escape do nothing" failure was independently
reproduced on the unrelated Chat message box, ruling out a form-specific bug. P120 itself
concludes this is an instrument limit ("`wtype`'s non-printable-keysym delivery does not
register... at all"), not a dead feature.

**files:** none — `run_streaming`/`clone.rs` are already believed correct; no source change is
indicated by current reading.

**approach:** submit the New Worktree form with a driver that can deliver a real `Return`
keypress (X11 `xdotool key Return`, or any driver operating below the compositor's synthetic-
keysym layer) and observe whether stderr progress lines arrive incrementally before the git
command completes, per the row's VERIFY.

**size:** S (once a working key-delivery driver is available).

## `F-PER-01` — FAILED — defective (chats conjunct only; four of five objects pass)

**needs: build.** Belongs to Cluster 2 — this is the same defect as `F-PERSIST-DB-05`; fixing
one fixes both. Pass 17's own evidence is precise: after two completed real ACP exchanges in the
UI chat tab, `chat_turn` and `session_ref` both held 0 rows (WAL-aware read), and a relaunch
showed an empty transcript, even though `projects/worktrees`, `tabs/splits`,
`terminal scrollback`, and now `browser tabs` (added 2026-08-14, confirmed round-tripping)
all genuinely persist. Root cause, confirmed by reading: `rust/crates/tiller_ui/src/chat.rs`'s
`restore_transcript` (`chat.rs:1732`) has exactly two callers — `chat.rs:8271` and
`rust/crates/tiller/src/main.rs:4226`, both inside `resume_chat`, which only reopens a tab from
the in-memory `retained_chats` list (populated at `main.rs:4003` when a chat tab is closed
*within the same process*). `rust/crates/tiller/src/session.rs` — the DB-backed snapshot that
survives a real relaunch — has **zero** occurrences of `transcript`, so there is no field for a
transcript to travel through even if something tried. Meanwhile
`tiller_persistence::save_chat_transcript`/`load_chat_transcript`
(`rust/crates/tiller_persistence/src/db.rs:486,558`) are real, schema-backed, and already
tested — but their only callers are in `rust/crates/tiller_acp/src/chat.rs:487,514`, the
ACP-protocol/socket-driven `ChatSession` path, never the UI's own `Chat`/`ChatView`.

**files:** `rust/crates/tiller_ui/src/chat.rs` (the `Chat`/`ChatView` needs a turn-completion
hook to trigger a save, and a restore path that isn't only `retained_chats`);
`rust/crates/tiller/src/session.rs` (needs a transcript-carrying field in the persisted tab
snapshot, or a lookup key such as `persistence_id` that a restore call can hand to
`load_chat_transcript`); `rust/crates/tiller/src/main.rs` (chat-tab creation/restore call sites —
`add_chat_tab`, `restore_tabs`, `restore_tabs_in_workspace` — need to call
`save_chat_transcript`/`load_chat_transcript` the way `tiller_acp/src/chat.rs` already does);
`rust/crates/tiller_persistence/src/db.rs` (no change expected — reuse as-is);
`rust/crates/tiller_acp/src/chat.rs` is the reference implementation to model the UI-side wiring
on, not itself broken.

**approach:** give the UI `Chat` the same save/load discipline the ACP `ChatSession` already
has: on turn completion, call `save_chat_transcript` keyed by the tab's `persistence_id`; on
restore (both `restore_tabs` and `restore_tabs_in_workspace` — a fix to only one leaves the
other dead, same trap `P100` already documented for `register_agent_id`), call
`load_chat_transcript` and feed the result into `Chat::restore_transcript`. Prove it live with a
real quit/relaunch, not a green test — the pass-14 tests already covered the store and the
socket door and still missed this.

**size:** L — crosses turn-lifecycle, tab-restore, and persistence-call-site wiring in at least
three files, with an open design question (what key identifies a chat tab's transcript across a
process restart) that needs a decision, not just a call.

## `F-PER-06` — FAILED — defective

**needs: build.** `terminate_process_group` (`rust/crates/tiller_terminal/src/lib.rs:513-529`)
sends `SIGTERM` (then `SIGKILL` after a 500ms grace period) via `libc::killpg` to a **single**
process group — `self.shell_pid`, captured once at `pty.child().id()`
(`tiller_terminal/src/lib.rs:345`). This is correct for a simple shell, whose own pgid covers
its whole job. It is insufficient for a "compound-command" pane (a shell running something like
`cmd1 && cmd2`, a backgrounded `&` job, or anything else that triggers job-control to spin up a
*new* process group) — such children detach from the shell's own pgid and `killpg` on the
original group never reaches them, matching pass 6's live finding (compound panes leak,
simple panes flush) and the project's own paired write-up (`F-TERM-08`, `WORK-BREAKDOWN.md` D-2,
same root, not in this group).

**files:** `rust/crates/tiller_terminal/src/lib.rs` (`terminate_process_group`, and the
call site at `:387` that captures/passes `self.shell_pid`).

**approach:** `killpg` alone can't reach a job-control-spawned second process group. The fix
needs to enumerate the shell's actual descendant process groups at kill time (e.g. walk
`/proc/<shell_pid>/task/*/children` recursively and `killpg` each distinct pgid found, not just
the shell's own) rather than trusting one static pgid captured at spawn.

**size:** M.

## `F-PER-08` — half-proven

**needs: exercise.** Belongs to Cluster 3's family of X11-only gestures, though the specific
tooling gap is different: `save_browser_origin_grant` (called at `rust/crates/tiller/src/main.rs
:4578`, fed by `surface.allowed_origins()` diffed against `self.browser_origins`) is a real,
wired call — the general-settings half of this row is already live-proven with a genuine
restart round-trip. The browser-origin half is architecturally gated on the embedded browser
actually rendering **page content** and firing a JS permission-prompt event; this Wayland lane's
embedded browser renders chrome only (per `WAYLAND-LANE.md`), so no page ever loads and no
permission prompt can ever fire to populate `newly_allowed`. The Permissions panel correctly
shows zero grants throughout, consistent with the code path never being triggered rather than
being broken.

**files:** none indicated — `save_browser_origin_grant` and its call site look correct as read.

**approach:** exercise on an X11 lane where the embedded browser can render real page content:
load a page that requests a permission (e.g. clipboard or geolocation), grant it, quit/relaunch,
and confirm the origin grant persisted through `browser_origins`/settings.

**size:** M (mostly lane-availability, not code).

## `F-PERSIST-DB-05` — FAILED — defective

**needs: build.** Same defect as `F-PER-01`'s chats conjunct — see Cluster 2 and that row's
full analysis; this entry exists because the family view names the persistence-side half of the
identical gap. The row's own VERIFY is explicit and app-level ("Create ... chat tabs, restart,
and inspect ... transcript restoration"), and by construction that path doesn't exist:
`tiller_ui/src/chat.rs` (the `ChatView` the user actually types into) has no persistence
reference at all; its restore is fed only by in-memory `retained_chats`; `session.rs` has zero
`transcript` occurrences. The green test the ledger notes
(`chat_transcript_survives_process_relaunch_with_tool_and_permission_outcome`,
`persistence_integration`) proves the **store** and the **ACP/socket path**
(`tiller_acp/src/chat.rs`), not the UI path — code plus a green test is `NOT EXERCISED`, never
`PASSED`, per this project's own standard, and here the code itself doesn't even reach the UI.

**files:** identical to `F-PER-01`: `rust/crates/tiller_ui/src/chat.rs`,
`rust/crates/tiller/src/session.rs`, `rust/crates/tiller/src/main.rs`,
`rust/crates/tiller_persistence/src/db.rs` (reuse, no change expected),
`rust/crates/tiller_acp/src/chat.rs` (reference pattern).

**approach:** identical to `F-PER-01` — this is the same fix, reported as one row's worth of
work, not two. **A single PR should close both rows together**; do not schedule them onto two
different build-fleet agents, or they'll conflict on the exact same files.

**size:** L (shared with `F-PER-01` — do not double-count when sizing the fleet).

## `F-PERSIST-DB-06` — half-proven

**needs: build.** The `session.ref` half is proven (`main.rs:1522` dispatch,
`load_session_refs` at `main.rs:6883`). The account half has no store at all: grep across
`rust/crates/tiller_persistence/src/` for `account` returns nothing — no migration creates an
account table, no `db.rs` function saves or loads one. The only "account" code that exists is
`rust/crates/tiller_usage/src/account.rs`, with its one caller
`discover_claude_identity`/`discover_codex_identity`-style functions in
`rust/crates/tiller_ui/src/settings.rs:539-560+`, which shell out live to `claude auth status`/
`codex login status` for a **Settings display line** — never written to or read from any
database row, so there is nothing to look up on restore, which is exactly what the row's VERIFY
("save references... restart, and inspect... account lookup") requires.

**files:** `rust/crates/tiller_persistence/src/migrations.rs` (new migration: an account table
keyed by agent id, presumably identity/login-state + timestamp); `rust/crates/tiller_persistence
/src/db.rs` (save/load functions, mirroring the existing `session_ref`/`chat_turn` shape);
`rust/crates/tiller_usage/src/account.rs` (the existing identity-discovery logic is the natural
source of what gets saved); `rust/crates/tiller_ui/src/settings.rs` (the display line should
read from the new store on restore instead of re-shelling-out unconditionally, or in addition to
it).

**approach:** add the account table via a new migration, wire discovery
(`discover_*_identity`) to persist on successful login detection, and have the Settings restore
path read the stored record instead of relying solely on a live shell-out (useful when the CLI
binary is transiently unavailable at boot). Note `delete_session_ref` already has no consumer
per FABLE-06 — don't let a symmetrical unused `delete_account` slip in without a caller.

**size:** M.

## `F-PERSIST-DB-11` — half-proven

**needs: exercise.** The schema-creation half is now proven stronger than before (a real file,
not a fresh `TempDir`, booted v1→v12 and landed at `user_version=12`, matching
`migrations.rs`'s 12 migrations). The unresolved half is data-*preservation* across a migration
boundary for the fields those migrations actually manage — ordering/rowid backfill, chat/session
field renames, universal-workspace tables, browser content — and the existing green test only
proves that `display_name`/`color_hex`/`icon_value` survive restore, plus that project identity
is deliberately re-derived live from git on every boot (`session.rs:839-878`), which is by
design and doesn't discriminate this row. No existing test plants data at an *old* schema
version in `chat_turn`/`session_ref`/tab-ordering columns and checks it survives forward
migration.

**files:** `rust/crates/tiller_persistence/src/migrations.rs` (read-only reference — the
migrations under test); `rust/crates/tiller_persistence/tests/persistence_integration.rs` (where
the new test belongs, alongside the existing display_name/color_hex/icon_value one).

**approach:** build a fixture DB at an old schema version (`user_version` before the chat/session
field renames, or before the ordering/rowid backfill), insert representative rows using that old
version's column shapes, open it through the current migration chain, and assert the same
logical rows survive with the new schema's field names/ordering intact — not just that the
tables exist.

**size:** M.
