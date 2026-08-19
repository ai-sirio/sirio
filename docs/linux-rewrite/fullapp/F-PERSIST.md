# F-PERSIST — persistence package (13 rows) — fresh critic pass

Lane label prefix `fpersist26*` (six independent app invocations: `fpersist26`,
`fpersist26b`..`fpersist26f`), outdir `/dev/shm/sweep-26-F-PERSIST*`. Binary
`/dev/shm/tt/debug/tiller`. `CARGO_TARGET_DIR=/dev/shm/tt`,
`CARGO_PROFILE_DEV_DEBUG=none`. Host was under extreme contention throughout
this pass (`load average: 35` on a 12-core box, dozens of concurrent `rustc`
jobs from other critics) — noted wherever it affects a verdict.

I did **not** just replay the wave-H "PASSED" verdicts. I re-derived each one
against the live Rust app and the current `tiller_persistence`/`tiller`
source, and I disagree with two of them (DB-03, DB-08) — see Defects below.

## Method

1. `cargo test -p tiller_persistence` and `cargo test -p tiller` (session
   module) for the schema/write-path claims that are really unit-level.
2. Six live app drives via `Scripts/wayland-drive.sh`, each against a fresh
   throwaway git fixture under `/dev/shm/<label>-fixture`, reading
   `/tmp/<label>.sqlite` directly with Python's stdlib `sqlite3` (no
   `sqlite3` CLI on this host) both mid-session (via `ctl project.list`) and
   post-mortem (after the app process exited).
3. One same-label relaunch (`fpersist26` reused) to exercise a real
   restart-and-reattach, screenshotted and compared against the DB.
4. Cross-checked every "why is this wrong" hypothesis against the actual
   source (`db.rs`, `migrations.rs`, `session.rs`, `main.rs`) before writing
   it up, rather than guessing from behaviour alone.

## Table

| id | verdict | evidence |
|---|---|---|
| `F-PERSIST-DB-01` | PASSED | `cargo test -p tiller_persistence`: 9/9 unit + 40/41 integration green (the 1 failure is `concurrent_writers_save_disjoint_records_and_exit`, host-load flake, see Defects). Live: relaunched the same `/tmp/fpersist26.sqlite` under the same label (a real process kill + restart) and the app reopened it, re-migrated (no-op, already current), and restored the exact same project/worktree/tab set — screenshot `sweep-26-F-PERSIST-restart/02-01-relaunched.png`. `AppDatabase::in_memory()` exists and is exercised throughout the persistence test suite's own fixtures. |
| `F-PERSIST-DB-02` | PASSED | Direct `sqlite3` read (via Python) of a live-driven DB shows all 14 tables from `migrations.rs` present and populated after real UI actions: `project`, `worktree`, `tab`, `tab_state` (scrollback/pane-events/chat-draft JSON), `chat_turn`, `session_ref` (real ACP session UUID for pane-2), `account_identity` (`('claude', 'e.palmisano@reply.it', <ts>)` — matches the actual signed-in account), `agent_account`, `quarantine_record`, `browser_origin_grant`, `sidebar_state`/`sidebar_expanded_project`, `setting`. PLATFORM clause: schema is Linux-native (rusqlite/libsqlite3, not GRDB) — see PLAT-01. |
| `F-PERSIST-DB-03` | half-proven | Branch/path/order/comment fields round-trip correctly across a real restart (verified live, `fpersist26` → relaunch). **But** `is_primary` does not: see Defect 1 below — a freshly added-and-selected worktree's `is_primary` is written as `0` to the DB by the time the app quits, even though every live `ctl project.list` read up to the moment of quit reports `true`. The "worktree fields persist" claim is therefore only half true. |
| `F-PERSIST-DB-04` | PASSED | Live, via the app's own quit path: typed `echo GRACEFUL_QUIT_MARK_44f1` into a real terminal, `ctl system.quit`, then read `tab_state.state` JSON's `scrollback` bytes directly from `/tmp/fpersist26f.sqlite` — marker present (`marker: True`, 1855 bytes). `capture_scrollback()` at `main.rs`'s `layout()` correctly serializes live terminal content at quit time. (One earlier run relied on the *harness's* raw `kill` cleanup instead of `ctl system.quit` and did not see the marker — see the harness-vs-defect note under Defect 1; not counted against this row since a real user's only quit affordance here is the Quit action, not an OS signal — F-WIN-08 make window-close a minimize, not a quit.) |
| `F-PERSIST-DB-05` | PASSED | Direct DB read shows `tab` rows with correct `kind`/`title`/`is_active` (`terminal`/`Terminal`/1, `chat`/`Chat`/0) and `tab_state` carrying pane events + chat draft. Live restart screenshot shows the "Terminal" tab reopened under the correct worktree with the sidebar row intact. `cargo test`: `tab_state_round_trips_and_is_removed_with_replaced_tabs`, `chat_transcript_survives_process_relaunch_with_tool_and_permission_outcome`, `v9_tab_rows_upgrade_to_current_with_no_recorded_agent_identity` all green today. |
| `F-PERSIST-DB-06` | PASSED | Live: `account_identity` table holds a real row — `('claude', 'e.palmisano@reply.it', 1787159729502)` — matching the actual signed-in Claude account on this host, captured with no explicit action on my part (the app's own background detection wrote it). `cargo test`: `account_identity_table_stores_one_upserted_row_per_provider`, `account_identity_upserts_one_row_per_provider_and_survives_a_relaunch` green. |
| `F-PERSIST-DB-07` | PASSED | `cargo test -p tiller_persistence`: `a_corrupt_tab_state_is_quarantined_without_poisoning_siblings`, `a_corrupt_tab_is_skipped_and_quarantined_while_valid_tabs_survive`, `a_corrupt_chat_turn_is_quarantined_without_losing_other_turns` all reran green today against real corrupted fixtures (not inherited from a prior wave — I ran them myself this pass). Did not additionally inject corruption into a live-driven DB (the unit fixtures already exercise the exact `quarantine_record` write path with a real SQLite file; a live repeat would exercise the identical code). |
| `F-PERSIST-DB-08` | FAILED — defective | Live, reproduced 4/4 independent runs (`fpersist26c/d/e/f`): `project.add` on a single-worktree git repo correctly sets `is_primary=1` in memory and in the DB (confirmed via `fpersist26b`, add-only, no select — stays `1`). The instant `workspace.select` is issued for that same worktree, the value that eventually lands in the DB at quit time is `0` — reproduced identically whether the app is torn down by the harness's `kill` (SIGTERM) *or* by the app's own `ctl system.quit` (ruling out "harness artifact"). Exclusivity itself (never two `is_primary=1` rows per project) still holds — the SQL unique index and `clear_other_primaries` are correctly scoped and unit-tested — but the *specific value* set by `add()`/CRUD does not survive to the persisted row. See Defect 1. |
| `F-PERSIST-DB-09` | PASSED | `cargo test`: `save_tab_keeps_a_single_active_tab_per_worktree`, `the_unique_index_rejects_a_second_active_tab_at_the_sql_level`, `save_tabs_normalizes_the_active_flag`, `a_corrupt_tab_is_skipped_and_quarantined_while_valid_tabs_survive` all green today. Live restart (screenshot) shows the correct tab reconstructed as `terminal` kind with its title intact. |
| `F-PERSIST-DB-10` | PASSED | Live: `session_ref` table holds a real row (`pane-2` → a genuine ACP session UUID) written with no explicit action from me — the default Chat tab's ACP session registered itself. `cargo test`: `session_references_upsert_load_and_delete` green. |
| `F-PERSIST-DB-11` | PASSED, with a scope caveat | `cargo test`: `an_old_version_database_is_migrated_forward_with_rows_intact`, `forward_migration_preserves_pre_existing_session_ref_tab_and_chat_turn_data`, `current_version_is_the_migration_count`, `current_schema_contains_named_persistence_migrations` all green today — forward migration integrity (the row's general claim) genuinely holds. Caveat: the row's specific examples (rowid backfill, "universal workspace tables", renaming legacy fields) describe the *Swift* migration history verbatim and don't map onto the Rust migrator's own from-scratch v1–v14 sequence (worktree ordering + primary-exclusivity index at v3/v8, quarantine at v9, account identity at v13, agent accounts at v14 — different shape, same intent). Judged on the generalizable claim, which is verified; a critic reading literally for "rowid backfill" would find nothing to check, same root cause as DB-12. |
| `F-PERSIST-DB-12` | N/A — platform (reclassification independently re-verified) | Re-ran the wave-H claim myself rather than trusting it: `grep -rn 'terminalTab\|legacyTerminalTab' rust/crates` is empty workspace-wide, and I read `migrations.rs` in full — `migrate_v1` creates the `tab` table with its current name and columns from the start; no migration ever renames a table. The compatibility risk the row describes (GRDB v17 renaming `terminalTab`→`legacyTerminalTab_v15` while legacy code still names `terminalTab`) is structurally absent in a migrator that was never GRDB and never had that table. Agree with the reclassification. |
| `F-PERSIST-PLAT-01` | PASSED, with one flagged flake | Source: `app_support_root_for` in `rust/crates/tiller/src/session.rs` is cleanly `cfg(target_os)`-gated — Linux resolves `$XDG_STATE_HOME` (falling back to `$HOME/.local/state`) then `TillerRust`, scoped per-checkout via an FNV hash of the canonical repo root under `checkouts/`; macOS keeps `Application Support`. Live-exercised throughout this pass via `$TILLER_DB` override (six fresh DB files created, migrated, and reopened without incident). Concurrent-access unit coverage is real (`concurrent_first_opens_from_two_processes_both_succeed`, `concurrent_opens_of_an_already_migrated_database_both_succeed`, `a_contended_write_waits_for_the_peer_instead_of_failing` all green in isolation). Flag: `concurrent_writers_save_disjoint_records_and_exit` failed 9/10 times when run in the middle of this host's extreme load (avg 35/12 cores) but passed 3/3 in isolation moments earlier — see Defect 2, reported as a flake, not a confirmed defect. |

**Summary: 13 exercised, 10 passed outright, 1 passed with a caveat, 1 half-proven, 1 failed (defective).**

## Defects (with reproduction)

### Defect 1 — `worktree.is_primary` does not survive selecting the worktree, even via a graceful quit (DB-03, DB-08)

**Reproduction** (any of these four is sufficient):

```bash
export TILLER_WL_BIN=/dev/shm/tt/debug/tiller
export TILLER_WL_LABEL=<fresh>
# fresh git fixture with one commit at /dev/shm/<label>-fixture
Scripts/wayland-drive.sh <outdir> '
  ctl project.add path=/dev/shm/<label>-fixture
  sleep 1
  WS=$(ctl workspace.list)                      # -> primary implied (only one worktree)
  ctl workspace.select workspace=<the returned id>
  sleep 4
  ctl project.list                               # <- shows "primary":"true", every time
  ctl system.quit                                # graceful quit, not a signal
' 5
python3 -c "
import sqlite3
db = sqlite3.connect('/tmp/<label>.sqlite')
print(db.execute('SELECT id, is_primary FROM worktree').fetchall())
"
# -> is_primary is 0, contradicting every live read above
```

Concretely observed (verbatim from this pass):

- `fpersist26b` (add, **no** select): DB after quit → `is_primary=1`. Correct.
- `fpersist26c` (add, select, New Terminal, SIGTERM-cleanup quit): live `ctl project.list` right after the New Terminal click still says `"primary":"true"`; DB after process death → `is_primary=0`.
- `fpersist26d` (add, select, **no** terminal, SIGTERM-cleanup quit): same outcome — live read `true` after a 4s settle, DB after death → `0`.
- `fpersist26e` (add, select, **`ctl system.quit`** instead of SIGTERM): live read `true` immediately before quitting; DB after the app's own graceful exit → `0`. This rules out "it's just an unclean-kill artifact" — the app's own Quit path produces the same wrong value.
- `fpersist26f` (add, select, New Terminal, typed marker, `ctl system.quit`): same `is_primary=0` result, while in the same run the scrollback marker *did* round-trip correctly (Defect proven independent of DB-04's pass).

I read every mutator of `CatalogWorktree.is_primary` I could find (`ProjectCatalog::add`, `set_primary`, `refresh_project`, `create_workspace`, `select_worktree`, `evict_over_capacity_worktrees`, the `on_app_quit`/`on_window_closed` handlers) and the SQL write path (`save_worktree`'s `ON CONFLICT DO UPDATE SET is_primary = excluded.is_primary`, `clear_other_primaries` correctly scoped `WHERE project_id = ?1`) — none of them visibly zero the flag, and `discover_project`'s own is-primary detection is independently unit-tested and passing (`discovers_primary_checkout_of_real_repo` in `tiller_project`). I could not pin the exact line inside my time budget; I'm reporting the reproducible, restart-observable *symptom* rather than a guessed root cause. Whoever picks this up should start at `cx.on_app_quit`'s `workspace.session.schedule_catalog(&workspace.project_catalog)` call in `main.rs` and instrument what `workspace.project_catalog` actually holds at that exact moment vs. what `ctl project.list` was reading immediately before — the two are supposed to be the same live catalog and empirically are not by quit time.

**User-visible impact is narrower than the DB bug alone suggests**: on a normal restart, `restore_catalog`'s success path re-derives `is_primary` fresh from `git worktree list --porcelain` rather than trusting the stored column (confirmed live: the relaunch screenshot `sweep-26-F-PERSIST-restart/02-01-relaunched.png` shows the "Primary" badge correctly restored despite the underlying DB row saying `0`). The wrong persisted value only surfaces through `degraded_catalog_project` — the fallback used when git discovery itself fails at the next launch (transient `.git` unreadability, git binary missing, etc.) — at which point a project that should show as primary would not. That is still a real, restart-observable persistence-correctness defect for the exact column DB-03/DB-08 ask a critic to check, which is why I disagree with wave-H's blanket PASSED on both rows.

### Defect 2 (flake, not confirmed as an app defect) — `concurrent_writers_save_disjoint_records_and_exit` fails under extreme host load

```
cargo test -p tiller_persistence --test persistence_integration \
  concurrent_writers_save_disjoint_records_and_exit -- --exact
# in isolation, moments apart: 3/3 pass (one took 10.58s — itself a sign of contention)
# run inside the full 41-test suite, or repeated back-to-back while the host sat at
# `load average: 35` on 12 cores: 9/10 failed with
#   "writer open failed: Sqlite(SqliteFailure(Error { code: DatabaseBusy, ... }))"
```

The failure is inside `AppDatabase::open`'s `initial_open_lock`/migration path (`AppDatabase::initialize`'s `busy_timeout(5s)`), not inside `save_tabs` (the read-then-write-upgrade bug `write_transaction`'s own doc comment already fixed by switching every write to `BEGIN IMMEDIATE`). This is a *different* failure point than the one that comment describes, and it correlates cleanly with host CPU contention severe enough to starve a spawned child process past a 5-second wait — this box was at 35 runnable processes on 12 cores while I reproduced it. I could not get an idle window on this shared host to establish a true baseline, so per the standard-of-proof instructions I am reporting this as **"could not fully verify, because host contention this severe can legitimately blow a 5s timeout" rather than asserting a confirmed app defect.** It is worth another pass on a quiet host: if it still fails there, the 5s `busy_timeout` at the initial-open-lock layer is too short for real-world contention (an agent CLI + the GUI both opening the DB at launch, say) and deserves a proper fix, not just a longer timeout.

## Not reachable / not exercised

- Did not independently re-verify `F-PERSIST-DB-07`'s quarantine with a *live*, UI-driven corruption injection (hand-editing the SQLite file mid-session and confirming the app doesn't crash on next read) — relied on the unit/integration suite's real-SQLite-file fixtures instead, which exercise the identical `quarantine_record` code path. Time budget went to chasing Defect 1 instead; a live corruption repro would strengthen DB-07 further but I judge the existing coverage (three distinct corruption shapes, all reran green today) sufficient for PASSED.
- Did not test the persistence layer against a truly idle host to get a clean baseline for Defect 2 — this machine had 14+ other agents building/testing concurrently for the entire pass (confirmed via `uptime`/`ps aux`), which is exactly the condition the task's own environment notes warned about.
