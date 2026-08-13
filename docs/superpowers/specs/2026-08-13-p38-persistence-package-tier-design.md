# P38 Persistence Package Tier Design

## Goal

Make the Linux persistence boundary fail explicitly for damaged or unsupported SQLite files, keep concurrent stores bounded and safe, and close the concrete session-reference lifecycle gap without expanding the reduced Linux schema into absent Swift-only surfaces.

## Scope

The implementation owns `rust/crates/tiller_persistence/**`, its real-file integration tests, and the persistence status entry in `docs/linux-rewrite/INVENTORY-STATUS.md`. The control and agent crates remain untouched.

The inventory section contains 13 entries. The current Linux rewrite intentionally implements a smaller v5 schema: projects, worktrees, tabs, settings, sidebar state, opaque tab state, and session references. The larger Swift v18 entities that have no Linux model will be recorded as honest remainder rather than represented by empty placeholder tables.

## Design

`AppDatabase::open` will distinguish a missing path (new store) from an existing file. An existing file must have a complete SQLite header and page-aligned length; read-only preflight will reject a newer `PRAGMA user_version`, failed `quick_check`, and a logical database larger than the package limit before the read-write connection can migrate it. The normal connection will retain WAL, a five-second busy timeout, one immediate migration transaction, and a post-migration integrity check. The existing file is never replaced or reset on these failures.

The package will install SQLite `max_page_count` on every package connection, derived from a 64 MiB logical database limit, and preflight existing files against the same bound. Per-pane scrollback remains bounded at 256 KiB in the session layer; the package-level cap prevents package-owned writes from growing the SQLite file without limit. `PersistenceError` will expose a dedicated oversized-store error alongside corruption and newer-schema errors.

Session references will gain a public delete operation so the package supports the inventory's upsert/load/delete contract. Integration tests will use temporary files and real child processes: one v1 migration fixture, non-SQLite and truncated files, a future schema, the installed page limit, deletion, and two concurrent writer processes that each save disjoint records and exit. A same-key last-writer-wins policy will remain documented as an honest limitation; SQLite integrity and disjoint records must survive concurrent quit.

## Error handling

Corrupt, truncated, non-SQLite, and failed-integrity files return `PersistenceError::Corrupt` with the path and reason. A future schema returns `PersistenceError::NewerSchema` without migration. A file over 64 MiB returns `PersistenceError::DatabaseTooLarge`. SQLite busy, permission, and other I/O failures remain explicit `Sqlite`/`Io` errors rather than becoming an empty model.

## Verification

Run `source ~/.cargo/env && cargo test --manifest-path rust/Cargo.toml -p tiller_persistence`, then `source ~/.cargo/env && cargo test --manifest-path rust/Cargo.toml -p tiller` for the debounce and `flush_now` seam, and finally `./Scripts/ci-linux.sh` from the repository root. The final report will stay within the task's 12-line limit and include the 13-entry verdict counts, old/future schema behavior, corruption/truncation behavior, concurrent-writer result, claimed crates, and honest remainder.

## Self-review

- The scope names every file area changed and excludes control/agent work.
- The design never silently converts a damaged store into an empty database.
- The reduced v5 Linux schema is not misreported as the Swift v18 schema.
- The concurrency test distinguishes SQLite safety from same-key merge semantics.
- No placeholder tables, speculative agent records, or browser/webview behavior are introduced.
