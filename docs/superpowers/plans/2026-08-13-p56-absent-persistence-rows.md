# P56 absent persistence rows Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close the useful persistence gaps in P56 without creating callerless Linux models: persist worktree metadata, enforce one primary per project, look up worktrees by exact path, and quarantine malformed serialized rows while valid records survive.

**Architecture:** Extend the existing `tiller_persistence` value records and forward-only SQLite migrations. Migration v6/chat persistence and v5/session references remain the P52 baseline; new migrations add worktree metadata, the primary invariant, and a quarantine table. Physical SQLite corruption still fails closed and remains untouched, while malformed tab/tab-state/chat rows are moved to quarantine and omitted from the active read result.

**Tech Stack:** Rust 2024, `rusqlite` bundled SQLite, `serde_json`, `cargo test`, `Scripts/ci-linux.sh`.

## Global Constraints

- Work only in `/home/enzopalmisano/Scrivania/Progetti/tiller-linux` on `linux/gpui-waku`.
- Preserve the existing P52 `chat_turn`/relaunch work and other agents' files; do not edit `tiller_ui/**`, `tiller/src/main.rs`, or `tiller_control/src/**`.
- Use real SQLite files and relaunch/process tests for stored-data behavior; do not use mocks for corruption or migration proofs.
- Physical database corruption remains `PersistenceError::Corrupt`, untouched; malformed serialized rows are quarantined individually.
- Do not add agent-account, browser-content, context-usage, or duplicate legacy-tab tables without a caller in the Linux rewrite; record those deliberate absences in the ledger/report.
- Mark ledger evidence `builder-claimed, unverified`, never `PASSED`.

---

### Task 1: Worktree metadata, primary invariant, and exact-path lookup

**Files:**
- Modify: `rust/crates/tiller_persistence/src/model.rs` — add `comment`, `created_at`, and `updated_at` to `WorktreeRecord`, using nullable Unix-millisecond integers.
- Modify: `rust/crates/tiller_persistence/src/migrations.rs` — append named migrations for metadata and the per-project primary unique index.
- Modify: `rust/crates/tiller_persistence/src/db.rs` — read/write metadata, normalize primary saves, expose `worktree_by_path`.
- Test: `rust/crates/tiller_persistence/tests/persistence_integration.rs` — real relaunch round trip, duplicate-primary migration/constraint proof, and exact path proof.

**Interfaces:**
- Consumes: existing `AppDatabase`, `WorktreeRecord`, and migration runner; P52's current schema version is derived from `MIGRATIONS.len()`.
- Produces: `WorktreeRecord { comment: Option<String>, created_at: Option<i64>, updated_at: Option<i64> }`, `AppDatabase::worktree_by_path(&str) -> Result<Option<WorktreeRecord>, PersistenceError>`, and one-primary-per-`project_id` enforcement.

- [ ] **Step 1: Write the failing tests**

Add tests named `worktree_metadata_and_exact_path_survive_a_relaunch`, `a_duplicate_primary_is_reconciled_and_future_writes_are_rejected`, and `exact_path_lookup_does_not_normalize_nearby_paths`. The first writes non-empty metadata, drops the database, reopens it, and compares the complete record. The second creates a pre-index database with two primaries, opens it through `AppDatabase`, checks the first ordered row remains primary, then performs a raw second-primary insert and expects an SQLite constraint error. The third saves `/tmp/project/worktree`, looks it up by that exact string, and verifies `/tmp/project/./worktree` does not match.

```rust
#[test]
fn worktree_metadata_and_exact_path_survive_a_relaunch() {
    let dir = TempDir::new();
    let path = dir.db_path("worktree-metadata");
    let mut expected = sample_worktree("wt-1", "proj-1", "main");
    expected.comment = Some("release checkout".into());
    expected.created_at = Some(1_700_000_000_123);
    expected.updated_at = Some(1_700_000_100_456);
    {
        let db = AppDatabase::open(&path).expect("open");
        db.save_project(&sample_project("proj-1", "tiller")).expect("project");
        db.save_worktree(&expected).expect("worktree");
    }
    let db = AppDatabase::open(&path).expect("reopen");
    assert_eq!(db.worktree_by_path(&expected.path).expect("lookup"), Some(expected.clone()));
    assert_eq!(db.worktrees().expect("worktrees"), vec![expected]);
}
```

- [ ] **Step 2: Run the focused tests to verify they fail for the missing API/schema**

Run: `source ~/.cargo/env && cargo test -p tiller_persistence --test persistence_integration worktree_metadata_and_exact_path_survive_a_relaunch -- --exact`

Expected: FAIL because the metadata fields, `worktree_by_path`, and migration-backed columns do not exist yet.

- [ ] **Step 3: Implement the minimal model, migrations, and store behavior**

Add v7 `ALTER TABLE worktree` columns (`comment`, `created_at`, `updated_at`) and v8 migration logic that keeps the first primary per project by `(order_idx, id)` before installing `worktree_one_primary_per_project` on `project_id WHERE is_primary = 1`. Make `save_worktree` transactional and clear other primaries in the destination project before writing a primary. Make `save_worktrees` use the same first-primary rule. Select/map the three metadata columns everywhere. Implement exact string equality in `worktree_by_path` with `WHERE path = ?1 ORDER BY project_id, order_idx, id LIMIT 1`.

- [ ] **Step 4: Run the focused tests to verify they pass**

Run: `source ~/.cargo/env && cargo test -p tiller_persistence --test persistence_integration worktree_metadata_and_exact_path_survive_a_relaunch -- --exact && cargo test -p tiller_persistence --test persistence_integration a_duplicate_primary_is_reconciled_and_future_writes_are_rejected -- --exact && cargo test -p tiller_persistence --test persistence_integration exact_path_lookup_does_not_normalize_nearby_paths -- --exact`

Expected: PASS, including the real reopen and raw SQLite uniqueness proof.

---

### Task 2: Row-level quarantine and corrupt-tab skipping

**Files:**
- Modify: `rust/crates/tiller_persistence/src/model.rs` — add the public quarantine record returned by inspection.
- Modify: `rust/crates/tiller_persistence/src/migrations.rs` — append the named quarantine-table migration.
- Modify: `rust/crates/tiller_persistence/src/db.rs` — validate serialized tab state/chat rows, skip malformed tab rows, move bad rows to quarantine atomically, and expose quarantine inspection.
- Test: `rust/crates/tiller_persistence/tests/persistence_integration.rs` — real corrupt-row write/reopen proofs with valid siblings preserved.

**Interfaces:**
- Consumes: the metadata/index migrations from Task 1 and P52's `tab_state`/`chat_turn` rows.
- Produces: `QuarantinedRecord`, `AppDatabase::quarantined_records()`, per-record filtering for `tabs`, `tabs_of_worktree`, `tab_states_of_worktree`, and `load_chat_transcript`.

- [ ] **Step 1: Write the failing tests**

Add tests named `a_corrupt_tab_is_skipped_and_quarantined_while_valid_tabs_survive`, `a_corrupt_tab_state_is_quarantined_without_poisoning_siblings`, and `a_corrupt_chat_turn_is_quarantined_without_losing_other_turns`. Seed each database with valid siblings, use a raw `rusqlite::Connection` to write malformed data, close/reopen through `AppDatabase`, assert valid rows remain, assert the corrupt row is absent from the active result, and assert `quarantined_records()` contains the source type, record identity, and original payload/reason.

```rust
#[test]
fn a_corrupt_tab_state_is_quarantined_without_poisoning_siblings() {
    let dir = TempDir::new();
    let path = dir.db_path("quarantine-state");
    let db = AppDatabase::open(&path).expect("open");
    db.save_project(&sample_project("proj-1", "tiller")).expect("project");
    db.save_worktree(&sample_worktree("wt-1", "proj-1", "main")).expect("worktree");
    db.save_tabs("wt-1", &[sample_tab("tab-1", "wt-1", "Terminal", "terminal"), sample_tab("tab-2", "wt-1", "Chat", "chat")]).expect("tabs");
    db.save_tab_states("wt-1", &[TabStateRecord::new("tab-1", r#"{"ok":true}"#), TabStateRecord::new("tab-2", r#"{"ok":false}"#)]).expect("states");
    drop(db);
    let conn = rusqlite::Connection::open(&path).expect("raw open");
    conn.execute("UPDATE tab_state SET state = 'not json' WHERE tab_id = 'tab-1'", []).expect("corrupt one state");
    drop(conn);
    let db = AppDatabase::open(&path).expect("reopen");
    let states = db.tab_states_of_worktree("wt-1").expect("load states");
    assert_eq!(states, vec![TabStateRecord::new("tab-2", r#"{"ok":false}"#)]);
    assert!(db.quarantined_records().expect("quarantine").iter().any(|row| row.record_type == "tab_state" && row.record_id == "tab-1"));
}
```

- [ ] **Step 2: Run the focused tests to verify they fail for the absent quarantine behavior**

Run: `source ~/.cargo/env && cargo test -p tiller_persistence --test persistence_integration a_corrupt_tab_state_is_quarantined_without_poisoning_siblings -- --exact`

Expected: FAIL because invalid serialized state currently reaches the caller and no quarantine table/API exists.

- [ ] **Step 3: Implement the migration and one quarantine path**

Append v9 `quarantine_record` with an autoincrement id, source `record_type`, source `record_id`, original `payload BLOB`, `reason`, and Unix-millisecond `quarantined_at`. Centralize `quarantine_rows` so tab, tab-state, and chat-turn readers insert the original bytes and delete only the offending source row in one transaction. Treat physical SQLite/header/quick-check failures as the existing untouched `Corrupt` path. Treat invalid JSON tab state and chat payloads as row corruption. Have tab readers use row-by-row conversion, retain valid rows, and quarantine conversion failures rather than returning an error for the entire tab set. Validate JSON on new `save_tab_states` writes so newly-created malformed state is rejected before persistence.

- [ ] **Step 4: Run the focused tests to verify they pass**

Run: `source ~/.cargo/env && cargo test -p tiller_persistence --test persistence_integration a_corrupt_tab_is_skipped_and_quarantined_while_valid_tabs_survive -- --exact && cargo test -p tiller_persistence --test persistence_integration a_corrupt_tab_state_is_quarantined_without_poisoning_siblings -- --exact && cargo test -p tiller_persistence --test persistence_integration a_corrupt_chat_turn_is_quarantined_without_losing_other_turns -- --exact`

Expected: PASS with valid siblings returned and quarantine rows retained after reopening.

---

### Task 3: Migration and regression coverage

**Files:**
- Modify: `rust/crates/tiller_persistence/tests/persistence_integration.rs` — assert old v1/v2 data migrates to the current version and all new tables/indexes are present; keep P52 relaunch proof intact.
- Modify: `rust/crates/tiller_persistence/src/lib.rs`, `src/db.rs`, `src/migrations.rs`, and `src/model.rs` — update module docs and public exports to describe the policy and current schema.

- [ ] **Step 1: Add the failing migration-schema assertions**

Extend the existing old-version and helper-process tests to assert the metadata columns, `worktree_one_primary_per_project`, `quarantine_record`, and P52 `chat_turn`/`session_ref` tables exist after a real open. Assert `CURRENT_SCHEMA_VERSION == MIGRATIONS.len()` remains the source of truth.

- [ ] **Step 2: Run the migration-focused test and observe the missing schema assertion**

Run: `source ~/.cargo/env && cargo test -p tiller_persistence --test persistence_integration an_old_version_database_is_migrated_forward_with_rows_intact -- --exact`

Expected: FAIL until the migration list and schema checks include the new named migrations.

- [ ] **Step 3: Update documentation and ledger evidence**

Document the distinction between physical-store failure and row-level quarantine in the crate docs. Update only the P56 persistence rows in `docs/linux-rewrite/INVENTORY-LEDGER.md`: mark implemented behavior `builder-claimed, unverified`, explicitly call DB-05's chat half stale in P52's favour, and leave unsupported agent-account/browser/context/duplicate-legacy-table portions as absent by design with their caller evidence. Record the exact named tests and the final gate result after verification.

- [ ] **Step 4: Run the full persistence suite and the repository gate**

Run: `source ~/.cargo/env && cargo test -p tiller_persistence`

Then run: `source ~/.cargo/env && Scripts/ci-linux.sh`

Expected: the persistence suite passes; the final gate either prints `CI OK` or reports an honest failure caused by another agent's in-flight files, with no foreign-file edits made to manufacture green.

