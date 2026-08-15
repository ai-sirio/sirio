# Wave D slice D-MAIN-3 — 7 rows

**Serialised slice — link 3 of 8 in the `D-MAIN` chain.** Earlier links have landed; later links have not started. Nobody else holds `main.rs`. **Re-read the file from disk before editing** — an earlier link changed it since this brief was written.

## Files you own

- `rust/crates/tiller/src/main.rs`

## Rows

### `F-CORE-SET-01` — ledger line 397, currently **half-proven**

- **Files:** rust/crates/tiller/src/main.rs, rust/crates/tiller_project/src/settings.rs, rust/crates/tiller_ui/src/settings.rs, rust/crates/tiller_ui/src/sidebar.rs
- **Wave-C critic evidence (fresh, 2026-08-15):** No wave-C commit touched settings load/clamp code. Did not re-drive the 5 already-proven fields (unchanged) or attempt the remaining untested ones (TILLER_SOCKET_ENABLE override is unreachable through wayland-drive.sh's own readiness gate without script changes). Coverage unchanged from prior pass. sweep C-MAIN-2, 2026-08-15

### `F-CORE-WSP-04` — ledger line 380, currently **NOT EXERCISED**

- **Files:** rust/crates/tiller/src/main.rs, rust/crates/tiller_project/src/layout.rs
- **Wave-C critic evidence (fresh, 2026-08-15):** Re-grepped today: LayoutCommand/classify_layout_command have zero callers outside layout.rs/lib.rs re-export; main.rs never references them. Unchanged; no wave-C commit touched layout.rs. sweep C-MAIN-2, 2026-08-15

### `F-CORE-WSP-08` — ledger line 384, currently **NOT EXERCISED**

- **Files:** rust/crates/tiller/src/main.rs, rust/crates/tiller/src/session.rs, rust/crates/tiller_project/src/layout.rs, rust/crates/tiller_ui/src/chat.rs, rust/crates/tiller_ui/src/editor.rs, rust/crates/tiller_ui/src/file_view.rs
- **Wave-C critic evidence (fresh, 2026-08-15):** Re-grepped today: WorkspaceTabViewState has zero callers outside layout.rs/lib.rs re-export. Unchanged; no wave-C commit touched layout.rs or session.rs. sweep C-MAIN-2, 2026-08-15

### `F-CTRL-BROWSER-02` — ledger line 445, currently **FAILED — defective**

- **Files:** rust/crates/tiller/src/main.rs
- **Wave-C critic evidence (fresh, 2026-08-15):** Live today on a genuinely fresh instance (cmain2fresh, no prior project.add): workspace.current -> 'no current workspace' error, then browser.open -> ok:true with a real surface returned. Confirms the recorded defect exactly, freshly re-driven; code unchanged. sweep C-MAIN-2, 2026-08-15

### `F-CTRL-BROWSER-03` — ledger line 446, currently **FAILED — absent**

- **Files:** rust/crates/tiller/src/main.rs
- **Wave-C critic evidence (fresh, 2026-08-15):** Live today (cmain2a): browser.get returned explicit ok:false 'unsupported on Linux: not implemented', not the recorded queued:true no-op. Traced the fix (988d9e9) to before wave C's start boundary via git merge-base — recorded evidence is stale. Reclassified defective->absent: capability still doesn't exist. sweep C-MAIN-2, 2026-08-15

### `F-CTRL-BROWSER-04` — ledger line 447, currently **FAILED — absent**

- **Files:** rust/crates/tiller/src/main.rs
- **Wave-C critic evidence (fresh, 2026-08-15):** Live today (cmain2a): browser.screenshot and browser.snapshot both returned explicit ok:false 'unsupported on Linux: not implemented' — not the recorded queued:true no-op. Same 988d9e9 pre-wave-C fix as BROWSER-03. Reclassified defective->absent. sweep C-MAIN-2, 2026-08-15

### `F-CTRL-BROWSER-05` — ledger line 448, currently **FAILED — absent**

- **Files:** rust/crates/tiller/src/main.rs
- **Wave-C critic evidence (fresh, 2026-08-15):** Live today (cmain2a): browser.wait -> explicit ok:false 'unsupported: not implemented'; browser.act -> explicit ok:false 'only the driving flag is implemented', none of click/fill/type/press/scroll exist (source-confirmed, single match-arm handles only 'driving'). Reclassified defective->absent for the not-implemented half. sweep C-MAIN-2, 2026-08-15

