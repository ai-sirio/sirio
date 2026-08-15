# Wave C integration report

Integrator pass over `linux/gpui-waku` after 12 agents committed concurrently to one
shared worktree. Range examined: `f3b6169..HEAD` (46 commits: 27 touch `rust/`/`Scripts/`,
19 are `docs/linux-rewrite/wave-c/*` reports or workflow-data checkpoints).

## 1. Build status

`cargo build -p tiller` was already green at the start of this pass — no builder had left
it broken. Every fix below was re-verified green after landing, and the workspace is green
now: `cargo build -p tiller` finishes in under a second (nothing to recompile) with a single
pre-existing, unrelated warning (`tiller_ui::browser::BrowserSurface::pump_task` is never
read — present before this pass, not touched by it).

## 2. Per-crate test results (`cargo test -p <crate>`, never `--workspace`)

| Crate | Result |
|---|---|
| `tiller` | 142 passed |
| `tiller_ui` | 293 passed |
| `tiller_persistence` | 7 (lib) + 38 (integration) passed |
| `tiller_control` | 46 passed |
| `tiller_theme` | 42 passed |
| `tiller_terminal` | 35 passed |
| `tiller_acp` | 22 + 11 + 6 passed (1 ignored, pre-existing) |
| `tiller_project` | 46 + 6 + 3 passed |
| `tiller_git` | 25 + 13 + 9 + 5 + 11 passed |
| `tiller_agents` | 14 + 32 + 1 + 5 + 8 passed |
| `tiller_activity` | 35 + 10 + 25 passed |
| `tiller_markdown` | 12 + 1 + 16 passed |
| `tiller_usage` | 39 + 3 + 1 + 7 passed |

Zero failures, zero flakes observed across two full runs of `tiller_ui` and one of every
other crate (the two timing-sensitive pre-existing tests the brief warned about were not
seen to flake in per-crate isolation).

## 3. Cross-slice sweep

Read `git log --format='%h %s' f3b6169..HEAD -- <file>` for every file touched in the range
and compared each commit against `docs/linux-rewrite/wave-c/manifest.json`'s **file**
ownership map (not commit-subject text, per the house rule — a `fix(main)` subject can
legitimately be any of C-MAIN-1..4's work since all four collectively own
`tiller/src/main.rs`). For every file with more than one commit, additionally diffed each
commit in isolation and read its deleted lines against its own commit message, looking for
the wave-B pattern: a deletion that quietly removes a prior, unrelated fix.

**27 code-touching commits examined** (7 by this integration pass, 20 by wave-C builders).
**Zero genuine cross-slice reverts found.**

Files with more than one commit in range, checked in detail:

- **`tiller/src/main.rs`** (7 commits: `32fffaf`, `0e7672d`, `e17f3f7`, `14e9eee` from
  C-MAIN-1..4; `620b9a3`, `cddf094`, `6af7ff4` by this pass). All four wave commits are
  overwhelmingly additive (194/66/11/14 insertions vs. 11/0/1/0 deletions); every deleted
  line matches its own commit's stated replacement (e.g. `e17f3f7` deletes the line that
  returned early with the unmodified payload, replacing it with the corrected title — not a
  revert of someone else's work).
- **`tiller_ui/src/chat.rs`** (3 commits, all C-CHAT chain: `ac696cb`, `391792e`, `2803ff1`).
  `ac696cb`'s 14 deleted lines are the hardcoded status-pill rendering it replaces with the
  mode-catalog-aware version its own message describes; `391792e`'s 8 deleted lines are the
  single `ErrorKind::Connection` styling path it splits into `Connection`/`AuthRequired`.
  `2803ff1` is pure addition.
- **`tiller_ui/src/sidebar.rs`** (3 commits: `0f227b9`, `2b26bd2` — both C-P3, chronological,
  building on each other; `fa1a330` by this pass on C-P1's behalf, see §4). No overlap
  conflict between the two C-P3 commits; `fa1a330` touches only `open_project_settings`,
  untouched by either.
- **`tiller_ui/src/right_panel.rs`** (2 commits, both C-P4: `9dd2216` then `d286759`,
  chronological). Each deletes only the exact lines its own message names (the
  under-reserved height calc; the `if refresh_error.is_none()` guard).
- **`tiller_ui/src/settings.rs`** (3 commits: `e7d8ffa` — C-CHAT-3; `cddf094`, `6af7ff4` by
  this pass). `e7d8ffa`'s 4 deleted lines are the single `kill -TERM <pid>` call it replaces
  with the `/proc` descendant walk its message describes.

All other touched files (`tiller_agents/*`, `tiller_terminal/src/lib.rs`,
`tiller_ui/src/{changes,controls,project_forms,project_identity,status_bar}.rs`,
`tiller_usage/src/{codex,model}.rs`) had one wave commit each — checked for
insertion/deletion ratio and, where deletions were non-trivial (`codex.rs`: 11 lines), read
the diff directly. All match their stated purpose exactly; no unexplained deletions.

`tiller_control/src/panel.rs`, `tiller_persistence/src/{db,model}.rs`, and
`Scripts/linux-drive.sh` have no owner in the manifest and no wave-C commits at all — every
commit against them in range is this integration pass's own (§4).

## 4. Foreign-file fixes applied

Every row the task listed, applied exactly as each builder's report described (file
ownership verified against the manifest first, so nothing here is invented):

| Builder / row | File | Commit | What |
|---|---|---|---|
| C-CHAT-1 · F-CHAT-14 | `tiller/src/main.rs` | `620b9a3` | `TillerWorkspace::new` binds restored `TabContent::Chat` tabs the same way it already binds restored `TabContent::Changes` tabs, so Follow Edited Files opens files on a session restored from disk, not only a freshly-launched chat. |
| C-P1 · F-GIT-REMOTE-01 | `tiller_ui/src/sidebar.rs` | `fa1a330` | `open_project_settings` passes the already-resolved checkout path into `ProjectIconPicker::with_value_and_repo`, so the Avatar tab's GitHub field pre-fills from the origin remote instead of starting empty. |
| C-P3 · F-AGENT-API-01 | `tiller_control/src/panel.rs` | `3fb90ec` | `PaneRegistry::read()` checked only the control-owned pane map; `panel.list`/`state`/`scrollback` all also check the renderer-owned `external` map. A pane `panel.list` had just reported wired (the reported case: an ACP-bridged OpenCode tab) could fail `panel.read` moments later with `UnknownPane`. Mirrored the dual-map lookup `state()`/`scrollback()` already use. The report named the file and symptom but not an exact patch; the fix follows the sibling methods' own established pattern. New regression test in `tiller_control/tests/control_integration.rs`. |
| C-P4 · F-CORE-FILE-03 | `Scripts/linux-drive.sh` | `e8f72a6` | Added a `drag x1 y1 x2 y2 [steps]` helper (mousedown, several intermediate `mousemove --sync` steps, mouseup) — `click`/`rclick` only deliver a single button event, which reads as two independent clicks rather than a drag to anything checking for held-button motion. |
| C-CHAT-3 · F-SET-20 | `tiller_ui/src/settings.rs`, `tiller/src/main.rs`, `tiller_persistence/src/{model,db}.rs` | `cddf094` | Threaded the Appearance Translucency toggle end to end: `SettingsSnapshot.translucency`, both directions of `main.rs`'s `settings_snapshot_from_app_settings`/`app_settings_from_snapshot`, and `AppSettings.translucency` + `appearance.translucency` load/save in `tiller_persistence` (same boolean pattern `control_socket_enabled` uses). The toggle previously flipped a UI-local field with nowhere to go. Required consequential fixes to two exhaustive round-trip tests (`tiller/src/main.rs`, `tiller/src/session.rs`) that assert every `AppSettings` field explicitly. |
| C-CHAT-3 · F-PERSIST-DB-06 | `tiller_persistence/src/db.rs`, `tiller/src/main.rs`, `tiller_ui/src/settings.rs` | `ea5071b`, `6af7ff4` | Migration v13's `account_identity` table had zero callers. Added `AppDatabase::account_identity`/`save_account_identity` (`ea5071b`, same upsert shape as `save_session_ref`), then wired `Settings::with_database_path` (mirroring `chat.rs`'s own `ChatPersistence` pattern — `tiller_ui` already depends on `tiller_persistence` and already opens ad hoc connections this way) so a fresh Claude/Codex identity is cached on every discovery sweep and a provider that is locally signed in but got no identity from a failed live shell-out falls back to the last cached one (`6af7ff4`). |

The task listed three files for C-CHAT-3 (`db.rs`, `main.rs`, `model.rs`) without naming
which of C-CHAT-3's two blocked rows (F-SET-20, F-PERSIST-DB-06) they belonged to; the
listed set is exactly the union both rows' own `wantedForeignFiles` need, so both were
implemented in full rather than guessing which one was meant.

Every fix above is covered by a new or strengthened test, all passing (see §2); the exact
symbol landing in its target file was `grep`-verified before each commit per the house rule.

## 5. Binary

`target/debug/tiller` rebuilt clean after every change in this pass and is current with
`HEAD` (`0.37s` finish on the final `cargo build -p tiller` — nothing left to recompile).
