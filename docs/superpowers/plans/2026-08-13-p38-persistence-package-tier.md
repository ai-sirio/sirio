# P38 Persistence Package Tier Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the Linux persistence package explicit and durable under schema drift, damaged files, concurrent writers, and bounded storage while closing session-reference deletion.

**Architecture:** Keep `tiller_persistence` as a pure Rust/SQLite boundary. Existing files are preflighted read-only before the write connection enables WAL and migrations; all schema changes remain transactional. The current reduced Linux v5 model stays intact, while unsupported Swift-only v18 entities are reported rather than invented.

**Tech Stack:** Rust 2024, `rusqlite` 0.31 with bundled SQLite, temporary real files, subprocess-based integration tests, shell verification via Cargo and `Scripts/ci-linux.sh`.

## Global Constraints

- Work only in `/home/enzopalmisano/Scrivania/Progetti/tiller-linux`.
- Never copy code from `../_tiller-refs/{waku,zed,orca,t3code}`.
- Preserve unrelated pre-existing worktree changes.
- Existing database files must fail explicitly when empty, non-SQLite, truncated, corrupt, oversized, or newer than the supported schema.
- Keep the Linux persistence schema at the behavior currently represented by the package; do not add placeholder Swift-only workspace/browser/chat/account tables.
- The package's logical database limit is 64 MiB; per-pane scrollback remains capped at 256 KiB by the session layer.
- Final task reporting must be 12 lines or fewer.

---

### Task 1: Reject damaged, future, and oversized databases before migration

**Files:**
- Modify: `rust/crates/tiller_persistence/src/error.rs`
- Modify: `rust/crates/tiller_persistence/src/lib.rs`
- Modify: `rust/crates/tiller_persistence/src/db.rs`
- Modify: `rust/crates/tiller_persistence/src/migrations.rs` only if migration-facing documentation needs the final schema wording
- Test: `rust/crates/tiller_persistence/tests/persistence_integration.rs`

**Interfaces:**
- Consumes: `AppDatabase::open`, `AppDatabase::in_memory`, `CURRENT_SCHEMA_VERSION`, and `PersistenceError`.
- Produces: `MAX_DATABASE_BYTES: u64`, `PersistenceError::DatabaseTooLarge { path, bytes, max_bytes }`, and explicit `PersistenceError::Corrupt` results for existing empty, malformed, truncated, or failed-integrity files.

- [ ] **Step 1: Write the failing tests for existing-file preflight.**

Add integration tests with real temporary files:

```rust
#[test]
fn an_existing_empty_file_is_corrupt_not_a_fresh_store() {
    let dir = TempDir::new();
    let path = dir.db_path("empty-existing");
    std::fs::File::create(&path).expect("create empty file");

    let error = AppDatabase::open(&path).expect_err("empty existing file must fail");
    assert!(matches!(error, PersistenceError::Corrupt { .. }));
    assert_eq!(std::fs::metadata(&path).expect("metadata").len(), 0);
}

#[test]
fn a_database_truncated_after_close_is_corrupt_and_untouched() {
    let dir = TempDir::new();
    let path = dir.db_path("truncated");
    {
        let db = AppDatabase::open(&path).expect("seed database");
        db.save_project(&sample_project("proj-1", "tiller"))
            .expect("seed row");
    }
    let original_len = std::fs::metadata(&path).expect("metadata").len();
    let file = std::fs::OpenOptions::new()
        .write(true)
        .open(&path)
        .expect("open for truncation");
    file.set_len(original_len / 2).expect("truncate database");
    let truncated_len = std::fs::metadata(&path).expect("metadata").len();

    let error = AppDatabase::open(&path).expect_err("truncated file must fail");
    assert!(matches!(error, PersistenceError::Corrupt { .. }));
    assert_eq!(std::fs::metadata(&path).expect("metadata").len(), truncated_len);
}

#[test]
fn each_database_connection_installs_a_logical_size_limit() {
    let dir = TempDir::new();
    let path = dir.db_path("size-limit");
    let _db = AppDatabase::open(&path).expect("open");
    let conn = rusqlite::Connection::open(&path).expect("raw inspection");
    let page_size: i64 = conn
        .query_row("PRAGMA page_size", [], |row| row.get(0))
        .expect("page size");
    let max_pages: i64 = conn
        .query_row("PRAGMA max_page_count", [], |row| row.get(0))
        .expect("max page count");
    assert!(page_size > 0);
    assert!((page_size as u64) * (max_pages as u64) <= MAX_DATABASE_BYTES);
}
```

- [ ] **Step 2: Run the focused tests to verify they fail.**

Run: `source ~/.cargo/env && cargo test --manifest-path rust/Cargo.toml -p tiller_persistence --test persistence_integration an_existing_empty_file_is_corrupt_not_a_fresh_store`

Expected: the empty-file test fails because SQLite currently treats an existing zero-byte file as a new database. Run the truncation and size-limit tests similarly; they must fail or expose the missing explicit policy before implementation.

- [ ] **Step 3: Add explicit error and limit definitions.**

In `error.rs`, add:

```rust
DatabaseTooLarge {
    path: PathBuf,
    bytes: u64,
    max_bytes: u64,
}
```

Format it as `database at <path> is <bytes> bytes, over the <max_bytes>-byte limit`. In `lib.rs`, export:

```rust
pub const MAX_DATABASE_BYTES: u64 = 64 * 1024 * 1024;
```

- [ ] **Step 4: Implement read-only preflight and post-migration integrity checks.**

In `db.rs`, make `AppDatabase::open` call a new `validate_existing_file(path)` before `Connection::open`. The function must:

1. Return success for a missing path.
2. Return `Corrupt` for an existing zero-byte file.
3. Read the 100-byte SQLite header, require `SQLite format 3\0`, validate the stored page size, and require a page-aligned file length.
4. Open the existing file read-only, set the five-second busy timeout, read `PRAGMA user_version`, refuse versions above `CURRENT_SCHEMA_VERSION`, run `PRAGMA quick_check(1)`, and calculate logical bytes from `page_count * page_size`.
5. Return `DatabaseTooLarge` when logical bytes exceed `MAX_DATABASE_BYTES`.

Change `initialize` to accept the path, preserve `PRAGMA foreign_keys = ON` and `PRAGMA journal_mode = WAL`, install `PRAGMA max_page_count` for the current page size before migration, run the existing migration runner, and run `PRAGMA quick_check(1)` again. Map both `SQLITE_NOTADB` and `SQLITE_CORRUPT` through `classify_open_error` to `PersistenceError::Corrupt`.

- [ ] **Step 5: Run focused tests and existing package tests.**

Run: `source ~/.cargo/env && cargo test --manifest-path rust/Cargo.toml -p tiller_persistence --test persistence_integration an_existing_empty_file_is_corrupt_not_a_fresh_store`

Expected: PASS.

Run: `source ~/.cargo/env && cargo test --manifest-path rust/Cargo.toml -p tiller_persistence --test persistence_integration a_database_truncated_after_close_is_corrupt_and_untouched`

Run: `source ~/.cargo/env && cargo test --manifest-path rust/Cargo.toml -p tiller_persistence --test persistence_integration each_database_connection_installs_a_logical_size_limit`

Expected: PASS.

Run: `source ~/.cargo/env && cargo test --manifest-path rust/Cargo.toml -p tiller_persistence`

Expected: all existing and new persistence tests pass, including old-schema migration, future-schema refusal, and non-SQLite preservation.

### Task 2: Complete session-reference deletion and prove concurrent writers

**Files:**
- Modify: `rust/crates/tiller_persistence/src/db.rs`
- Test: `rust/crates/tiller_persistence/tests/persistence_integration.rs`

**Interfaces:**
- Consumes: `AppDatabase::save_session_ref` and `AppDatabase::session_refs`.
- Produces: `AppDatabase::delete_session_ref(&self, session: &str) -> Result<(), PersistenceError>` and a subprocess evidence test for concurrent writes and process exit.

- [ ] **Step 1: Write the failing deletion test.**

Add:

```rust
#[test]
fn session_references_upsert_load_and_delete() {
    let dir = TempDir::new();
    let db = AppDatabase::open(&dir.db_path("session-ref-lifecycle")).expect("open");

    db.save_session_ref("pane-1", "session-a").expect("insert");
    db.save_session_ref("pane-1", "session-b").expect("replace");
    assert_eq!(db.session_refs().expect("load").get("pane-1"), Some(&"session-b".to_string()));

    db.delete_session_ref("pane-1").expect("delete");
    assert!(!db.session_refs().expect("load after delete").contains_key("pane-1"));
}
```

- [ ] **Step 2: Run the deletion test to verify it fails.**

Run: `source ~/.cargo/env && cargo test --manifest-path rust/Cargo.toml -p tiller_persistence --test persistence_integration session_references_upsert_load_and_delete`

Expected: FAIL because `AppDatabase::delete_session_ref` does not yet exist.

- [ ] **Step 3: Implement the minimal deletion API.**

Add this method beside `save_session_ref`:

```rust
pub fn delete_session_ref(&self, session: &str) -> Result<(), PersistenceError> {
    self.conn
        .execute("DELETE FROM session_ref WHERE session = ?1", [session])?;
    Ok(())
}
```

- [ ] **Step 4: Run the deletion test to verify it passes.**

Run: `source ~/.cargo/env && cargo test --manifest-path rust/Cargo.toml -p tiller_persistence --test persistence_integration session_references_upsert_load_and_delete`

Expected: PASS.

- [ ] **Step 5: Add the real two-process writer test.**

Extend the integration test helper pattern so each child waits on a `go` file, opens the same database, saves 20 uniquely named projects, worktrees, and tabs, prints `OK`, and exits. The parent must wait until both children signal ready, release them together, wait for both exit statuses, reopen the database, assert 40 projects/worktrees/tabs, and run `PRAGMA quick_check` expecting `ok`. Use no mocks or in-process-only locks.

- [ ] **Step 6: Run the writer test and full package tests.**

Run: `source ~/.cargo/env && cargo test --manifest-path rust/Cargo.toml -p tiller_persistence --test persistence_integration concurrent_writers_save_disjoint_records_and_exit`

Expected: PASS with both child processes exiting successfully and all 40 disjoint records present. A same-identity concurrent update remains last-writer-wins and must be called out in the final remainder.

Run: `source ~/.cargo/env && cargo test --manifest-path rust/Cargo.toml -p tiller_persistence`

Expected: PASS.

### Task 3: Claim the package and verify the surrounding persistence seam

**Files:**
- Modify: `docs/linux-rewrite/INVENTORY-STATUS.md`
- No source changes in `rust/crates/tiller_control/**` or agent crates

**Interfaces:**
- Consumes: package test output, current v5 schema, existing `tiller` session tests for debouncing and `flush_now`.
- Produces: an honest status entry naming `tiller_persistence` as claimed and separating package-proven behavior from absent Swift-only behavior.

- [ ] **Step 1: Run the session-layer persistence tests.**

Run: `source ~/.cargo/env && cargo test --manifest-path rust/Cargo.toml -p tiller session::tests::flush_now_writes_even_inside_the_debounce_window`

Run: `source ~/.cargo/env && cargo test --manifest-path rust/Cargo.toml -p tiller session::tests::flush_now_persists_a_new_snapshot_inside_a_later_debounce_window`

Expected: PASS, confirming the existing `flush_now` seam independently of the package's SQLite writer test.

- [ ] **Step 2: Update inventory status with the exact 13-entry count and verdict rationale.**

Record that the package claim covers the v5 Linux records, real-file migration/error/concurrency evidence, and the 256 KiB/64 MiB bounds. Mark Swift-only v18 workspace/browser/chat/account/quarantine requirements as honest remainder or unreachable in the Linux reduced scope, and do not tick them as passed from source inspection.

- [ ] **Step 3: Run the repository verification gate.**

Run: `./Scripts/ci-linux.sh`

Expected: the script prints `CI OK`. If unrelated pre-existing changes cause a failure, report the exact failing command and keep the persistence results separate.

- [ ] **Step 4: Review the final diff and report within 12 lines.**

Run: `git diff --check` and `git status --short`. Confirm only the intended persistence source/tests/status/plan/spec files changed, then report entry count and verdict counts, old/future schema behavior, corruption/truncation behavior, concurrent writer result, claimed crates, and honest remainder.
