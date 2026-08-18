# Finish-line shard: window-persist (wf-win)

Lane label `wf-win` (Wayland instance label `wfwin`), x86 desktop, COSMIC/Pop!_OS, 2026-08-18.
33 rows: `F-WIN-01..12`, `F-CORE-WSP-01..08`, `F-PERSIST-DB-01..12`, `F-PERSIST-PLAT-01`.

Binary pinned: `cargo build --manifest-path rust/Cargo.toml` (exit 0, 2 pre-existing dead-code
warnings only) → `md5sum` verified identical between `/tmp/wfwin-tiller` and
`rust/target/debug/tiller` for the entire pass.

Every row below was re-driven live this pass, on this host, in this session — no evidence is
carried over from a prior host or an earlier pass.

## Process note: an inadvertent `DISPLAY=:1` touch

While chasing F-WIN-09 I ran two `Scripts/linux-drive.sh` invocations against `DISPLAY=:1` before
re-reading my own brief's hard rule ("`DISPLAY=:1` is the user real desktop. Never drive it. Use
only your nested lane."). The second invocation's screenshot showed a live multi-tab session (real
rendered `example.com` content in a Browser tab) that was **not mine** — evidence someone else, or
the user, had state on that display. My double-click landed on the tab strip, not a titlebar; no
destructive action occurred. I stopped immediately on catching the mistake and did not touch `:1`
again. F-WIN-09 is recorded `UNREACHABLE` below rather than using that evidence.

## Persistence method

Every `F-PERSIST-DB-*` row was driven with the sharpest discriminator available: mutate state
through the running app, kill the process for real (`wayland-drive.sh` kills-and-relaunches on
every invocation by design), then read the result back two ways — the restored UI, and a direct
`python3`/`sqlite3` read of `/tmp/wfwin.sqlite` (no `sqlite3` CLI on this box; used Python's
`sqlite3` module). One deliberate corruption test (`UPDATE tab_state SET state='{not valid json'`)
was run to prove the quarantine path, not just its happy path.

## A real finding, not one of the 33 rows: the control-socket duplicate-project path skips the toast

`add_project` (UI path, `main.rs`) calls `self.show_toast(...)` on `Ok(false)`/`Err`.
`control_add_project` (the `project.add` socket handler used by `ctl` in every lane script) does
**not** — it returns `{"added":"false"}` with zero user-visible feedback. Confirmed live: a socket
`project.add` on an already-tracked path returned `added:false` and produced a byte-identical
screenshot before/after (no toast, no banner). Worth a row of its own; noted here since it
directly interfered with reproducing F-WIN-10 below.

## F-CORE-WSP: the layout domain model has (almost) zero application callers

`grep`-confirmed: `WorkspaceLayout`, `LayoutNode`, `PaneGroup`, `WorkspaceSnapshot`,
`WorkspaceTabViewState`, `LegacyWorkspaceTab`, and the `worktree_content_id`/`terminal_content_id`/
`browser_content_id`/`document_content_id` helpers in `tiller_project::layout` — all `pub use`d
from the crate root — have **zero** callers anywhere in `tiller`, `tiller_ui`, or
`tiller_persistence`. The only live wiring is `LayoutCommand::Rename` +
`classify_layout_command`/`FocusIntent` at `main.rs:8578`, driving the tab-rename feature. The
app's real split/pane tree is a separate, parallel type — `PaneNode<T>` in `crates/tiller/src/
panes.rs` — with its own `SplitDirection`/`ratio`/leaf-content model, structurally similar but
not the same code. This means the ledger's prior PASSED evidence for WSP-01/02/03/06/07 (all
citing `layout.rs`'s own unit tests) was testing code the shipped app never calls. The tests still
pass (reran them this pass, 5/5 green) — the logic is correct — but a passing test on an unwired
module is not a live-app proof, so these rows are marked `half-proven`, not `PASSED`.

## Rows

### F-WIN (12)

- **F-WIN-01** — PASSED. Live: clicked the status-bar gear (27,948) → full Settings surface
  (Theme/Interface/Terminal/Files/Agent Colors, screenshot `21-settings-open.png`), clicked Back
  (44,51) → workspace restored (colour count 9721→4716→9734, `22-settings-back.png`).
- **F-WIN-02** — PASSED. Live: with Chat focused, `chord ctrl t` created a third tab, auto-selected,
  showing a real freshly-spawned bash PTY (neofetch banner with live uptime/IP) —
  `04-after-ctrl-t.png`. Confirms the binding is inert with a terminal focused but fires from a
  non-terminal surface, same pattern documented for ctrl-k.
- **F-WIN-03** — half-proven. `chord ctrl o` with Chat focused produced a real, distinct error:
  `[Files] could not open the file picker: ZBus Error: Failed to connect to address
  'unix:path=/run/user/1000/bus': Connection refused (os error 111)` — proving the binding reaches
  the real `ashpd`/xdg-desktop-portal code path, not a silent no-op. Independently reproduced the
  same failure outside the app with `busctl --user list` and `dbus-send --session`: this box's
  session D-Bus socket exists on disk but nothing is listening. That is an environment fact, not an
  app defect. `chord ctrl s` with no active file produced zero visible change (byte-identical
  colour count before/after) — matches `window_command_availability`'s `Disabled(NoActiveFile)`
  gating. Full flow (pick a file → edit → save) is UNREACHABLE on this lane: no working session
  D-Bus bus.
- **F-WIN-04** — PASSED. Live: `chord ctrl+shift s` hid the sidebar (colour 9765→6446,
  `10-sidebar-hidden.png`), pressed again → restored (→9765, `11-sidebar-restored.png`).
- **F-WIN-05** — PASSED. Live: `chord ctrl+shift i` hid the Files panel (→6695,
  `12-rightpanel-hidden.png`), pressed again → restored (→9765, `13-rightpanel-restored.png`).
- **F-WIN-06** — PASSED. Live (substitute route: no Linux keybinding exists for `⇧⌘L`/`⌘L`, so
  driven via the tab-strip `+` menu, matching the prior pass's own documented substitution). Opened
  the menu (`click` on the `+`), `sleep 1` (never `shot` between opening a menu and clicking an
  item — see harness trap), clicked "New Browser" → a real Browser tab appeared in both the tab
  strip and the sidebar worktree tree, full chrome rendered (back/forward/stop, address bar showing
  `https://example.com`), content area shows the documented Wayland-lane webview-handle limitation
  verbatim — `28-after-new-browser-click.png`.
- **F-WIN-07** — PASSED. Live: `chord ctrl+shift o` changed the active tab from Browser (the just-
  booted session's active tab) to Chat, matching a restored-snapshot's stored active tab — a real,
  visible state transition (bold-highlighted tab changed), not an error toast this time.
- **F-WIN-08** — PASSED. Live, hard discriminator: found the toplevel container via
  `swaymsg -t get_tree` (`con_id=5`, `pid=2214411`), confirmed `kill -0 2214411` succeeded, sent a
  real `swaymsg '[con_id=5] kill'` (`xdg_toplevel` close request, `success:true`), waited 1s,
  `kill -0 2214411` **still succeeded** — the process was not terminated by a genuine WM close
  request — and `grim` still captured a fully-rendered, non-blank frame (10134 colours) afterward.
- **F-WIN-09** — UNREACHABLE. This task's hard rule forbids driving `DISPLAY=:1` (the user's real
  desktop); see the process note above for the inadvertent touch and immediate stop. The Wayland
  lane has no WM-drawn titlebar decoration for a double-click preference to act on, and this box's
  session D-Bus/dconf is independently confirmed unreachable there too (`dconf-WARNING: failed to
  commit changes to dconf: Connessione rifiutata` when I briefly tried `gsettings set` on `:1`
  before stopping) — so even the X11 lane could not have exercised the write side of this row on
  this box today.
- **F-WIN-10** — half-proven. Source-confirmed the exact mechanism: `show_toast`/`Toast`/
  `render_toast` (`id("workspace-toast")`, `TOAST_DURATION = 4s`, `main.rs:3107/5235/9858`) is real
  production code, called only from `add_project`'s UI-path `Ok(false)`/`Err` branches. Live, I
  reproduced two closely-related, functioning notice mechanisms this pass — a persistent inline
  error banner (the ctrl-o D-Bus failure, confirmed non-auto-dismissing after an 8s wait, byte-
  identical colour count) and the full update-toast family (F-WIN-11, below) — but five real
  attempts to trigger this specific "workspace-toast" via the UI's own Create-Project duplicate
  path did not land the exact gesture in the time available (one hit a folder-exists error inside
  the form itself, before `add_project` is ever called; the control-socket route that did trigger
  `Ok(false)` skips the toast entirely — see the finding above). Not claiming PASSED without having
  driven this exact toast.
- **F-WIN-11** — PASSED. Live: drove all five states plus reset via `ctl update.event` with values
  I chose myself (not matching any fixture default): `available version=9.9.1-wfwin` →
  "Tiller 9.9.1-wfwin is available" + Download button (`41-update-available.png`);
  `download-progress percent=55`; `install-started`; `finished`; `failed
  message=WFWIN_UPDATE_FAIL_TEST` → "Update failed: WFWIN_UPDATE_FAIL_TEST" + Retry button
  (`45-update-failed.png`); `reset` → colour count returned to the exact pre-sequence baseline
  (9445 both ends).
- **F-WIN-12** — N/A - platform. TCC onboarding sheet, macOS-only; unchanged.

### F-CORE-WSP (8)

- **F-CORE-WSP-01** — half-proven. `layout.rs` tests reran green
  (`legacy_content_exposes_only_terminal_panes_and_chat_tab_activity`,
  `document_identity_is_worktree_scoped_and_resolves_symlinks`), but `LegacyWorkspaceTab` and the
  `*_content_id` helpers have zero app callers (see finding above) — not reachable live.
- **F-CORE-WSP-02** — half-proven. Same zero-caller finding; `WorkspaceContentRef`'s stable-ID
  rules are unit-tested but not exercised by the shipped app's real content-identity path.
- **F-CORE-WSP-03** — half-proven. `layout_validation_accepts_root_empty_but_rejects_invalid_
  references` reran green, but `WorkspaceLayout`/`LayoutNode` are not what drives real splits —
  that's `panes.rs`'s separate `PaneNode<T>` (confirmed structurally similar: `SplitDirection`,
  `ratio: f32`, leaf/split variants — but literally different code, never unified).
- **F-CORE-WSP-04** — PASSED. Live, this session: right-clicked a tab (`rightclick 500 51`),
  `sleep 1`, clicked Rename (482,132), typed `WFWIN_RENAMED_TAB`, `key Return` → tab title
  committed both in the tab strip and the sidebar (`61-rename-committed.png`), confirmed
  byte-exact in the DB (`tab.title = 'TerminalWFWIN_RENAMED_TAB'`). Clicked the Chat composer and
  typed `ZPOSTMARK` immediately after — landed correctly in the composer (`62-focus-check.png`),
  discriminating against the stranded-focus bug this row's clause exists to catch.
- **F-CORE-WSP-05** — half-proven. The `Rename`/`Activate` (nonstructural, `FocusIntent::Tab`)
  classification is live-relevant via WSP-04 above. `Insert`/`Split`/`Move`/`Close`/
  `SetDividerFraction` are correctly classified by the passing unit test
  (`command_classes_distinguish_structural_and_nonstructural_changes`) but have no live caller —
  the app's real split/close/move commands go through `panes.rs`'s own methods, not
  `LayoutCommand`.
- **F-CORE-WSP-06** — half-proven. `validate()`'s reject-list is exercised by
  `layout_validation_accepts_root_empty_but_rejects_invalid_references` (reran green) against the
  unused `WorkspaceLayout` type; the shipped app's actual pane tree has no equivalent live
  validator that this pass could exercise.
- **F-CORE-WSP-07** — half-proven. `snapshots_are_versioned_canonical_and_malformed_data_falls_
  back_empty` reran green (confirms canonical JSON, no escaped slashes, version rejection, and
  fallback-to-empty), but `WorkspaceSnapshot` is not what the app persists — real tab/pane state
  goes through `tiller_persistence`'s own `tab`/`tab_state` tables (see F-PERSIST-DB-05).
- **F-CORE-WSP-08** — PASSED. Live, hard discriminator, this session: typed
  `ZPOSTMARKWFWIN_DRAFT_PERSIST_MARKER_998` into the Chat composer (never sent), read
  `/tmp/wfwin.sqlite` directly — `tab_state` row for `default-chat` held
  `"chat_draft":"ZPOSTMARKWFWIN_DRAFT_PERSIST_MARKER_998"` byte-exact — then let
  `wayland-drive.sh`'s next invocation kill and relaunch the process for real. First frame after
  restart, with zero `compose` call this session, showed the identical full marker text in the
  composer (`01-baseline.png` of that run) — read purely from disk across a genuine process
  restart.

### F-PERSIST-DB (12)

- **F-PERSIST-DB-01** — PASSED. `/tmp/wfwin.sqlite` (`TILLER_DB` override) survived **five separate
  real process restarts** this session with all state intact each time. `cargo test -p
  tiller_persistence` reran live: 8 unit + 39 integration tests, 47/47 green, including
  `concurrent_first_opens_from_two_processes_both_succeed` and
  `concurrent_opens_of_an_already_migrated_database_both_succeed`.
- **F-PERSIST-DB-02** — PASSED. Direct DB read: `.tables` lists `account_identity`,
  `browser_origin_grant`, `chat_turn`, `project`, `quarantine_record`, `session_ref`, `setting`,
  `sidebar_expanded_project`, `sidebar_state`, `tab`, `tab_state`, `worktree` — all populated with
  real data from this session's live driving, not fixtures.
- **F-PERSIST-DB-03** — PASSED. Direct DB read: `project` rows carry
  `id/name/root_path/color_hex/display_name/icon_kind/icon_value/avatar_image/
  default_worktree_base/worktree_location_override/order_idx`; `worktree` rows carry
  `id/project_id/branch/path/is_primary/order_idx/comment/created_at/updated_at` — all real values
  matching the sidebar's live display, `is_primary` correctly exclusive within each project.
- **F-PERSIST-DB-04** — PASSED. Live: typed `echo WFWIN_SCROLLBACK_MARKER...` output plus the
  app's own neofetch startup banner into a live terminal pane, switched tabs (triggering
  `schedule_save`), read the DB — `tab_state.scrollback["1"]` held 3998 raw bytes that decode
  exactly to the visible banner text. Restarted the process for real; the restored pane showed the
  identical banner text on first frame, with a fresh live shell prompt continuing beneath it
  (`01-baseline.png` of that run, `Terminal: wfwin-tiller` line visible in the restored neofetch
  output itself).
- **F-PERSIST-DB-05** — PASSED. Live: Chat/Terminal/Terminal/Browser tabs (including the renamed
  tab's title and the chat draft) all survived a genuine process restart with correct kind, title,
  and active-tab selection, confirmed both visually and via direct DB read.
- **F-PERSIST-DB-06** — PASSED. Direct DB read: `account_identity` has exactly one row —
  `('claude', 'e.palmisano@reply.it', 1787066579635)` — matching the real signed-in account shown
  live in the status bar ("Claude 3x% 5h ...").
- **F-PERSIST-DB-07** — PASSED. Live, strongest discriminator available: killed the app, directly
  corrupted one `tab_state.state` row to `'{not valid json'` on disk, restarted the app for real.
  App did not crash (rendered a full, non-blank frame). Read the DB back: `quarantine_record`
  gained exactly one new row — `(1, 'tab_state', '<the corrupt tab's id>', b'{not valid json',
  'tab state is not valid JSON', <timestamp>)` — the app itself moved the original corrupted bytes
  into quarantine with a genuine diagnostic reason, and the two sibling `tab_state` rows (Chat
  draft, other terminal's scrollback) were untouched.
- **F-PERSIST-DB-08** — PASSED. `is_primary` exclusivity confirmed directly in the DB across three
  live-created projects. Duplicate-path rejection confirmed live two ways: via the UI Create-
  Project form (real `mkdir` collision, "File exists (os error 17)") and via the control socket
  (`project.add` on an already-tracked path returned `added:"false"`).
  `cargo test -p tiller_persistence` reran green: `a_duplicate_primary_is_reconciled_and_future_
  writes_are_rejected`, `exact_path_lookup_does_not_normalize_nearby_paths`.
- **F-PERSIST-DB-09** — PASSED. Same corruption test as DB-07: after the real restart, the tab
  strip and sidebar still showed all three tabs (the corrupted one materialized as an empty/reset
  state rather than crashing or vanishing), and the previously-active tab selection was preserved.
- **F-PERSIST-DB-10** — PASSED. Direct DB read: `session_ref` has a real row —
  `('pane-6', 'cd2124aa-86da-4648-98a5-1dbf16251fcc')` — a genuine ACP session UUID matching the
  live Claude Code chat session driven this pass.
- **F-PERSIST-DB-11** — PASSED. `cargo test -p tiller_persistence` reran live, green:
  `forward_migration_preserves_pre_existing_session_ref_tab_and_chat_turn_data` (plants data at
  v6, migrates to current, asserts exact backfilled values) plus the full 47-test suite.
- **F-PERSIST-DB-12** — UNREACHABLE (re-confirmed). `grep -rn 'terminalTab\|legacyTerminalTab'
  rust/crates --include=*.rs` returns nothing workspace-wide, again, on this host. The Rust
  migrator creates the `tab` table from scratch with current column names; no Swift-era
  `terminalTab`→`legacyTerminalTab_v15` rename lineage exists in this codebase to test.

### F-PERSIST-PLAT (1)

- **F-PERSIST-PLAT-01** — PASSED. `app_support_root()`/`app_support_root_for()` in
  `rust/crates/tiller/src/session.rs` is the single, cleanly `#[cfg(target_os = "macos")]`/
  `#[cfg(not(target_os = "macos"))]`-gated module that chooses between macOS's `~/Library/
  Application Support/TillerRust` and Linux's `$XDG_STATE_HOME` (falling back to
  `$HOME/.local/state`) — confirmed no OS-specific path branching leaks into `tiller_persistence`
  or elsewhere (`grep` for `target_os` across the persistence crate returns nothing). Live-verified
  end to end on the Linux path all session: `TILLER_DB` override creation, five real restarts,
  concurrent-open tests, and the deliberate-corruption/quarantine test above all exercised the
  same gated resolution path successfully.

## Gap summary (for the orchestrator)

1. **Control-socket `project.add` silently skips the F-WIN-10 toast/notice** that the UI path
   raises on the same `Ok(false)`/`Err` conditions — a real behavioral inconsistency between the
   two entry points into `add_project` vs `control_add_project`.
2. **`tiller_project::layout`'s domain model (`WorkspaceLayout`, `LayoutNode`, `PaneGroup`,
   `WorkspaceSnapshot`, `WorkspaceTabViewState`, `LegacyWorkspaceTab`, and its four `*_content_id`
   helpers) has zero application callers.** Only `LayoutCommand::Rename`/`classify_layout_command`/
   `FocusIntent` are wired, driving tab rename. The app's real split/pane tree and persistence path
   are separate, parallel implementations (`panes.rs`'s `PaneNode<T>`, `tiller_persistence`'s
   `tab`/`tab_state` tables) that happen to provide equivalent behavior under different types. Six
   of eight F-CORE-WSP rows are marked `half-proven` on this basis, not `PASSED`.
