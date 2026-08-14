# Wave B slice B3-sidebar — 5 rows to build

**sidebar and project identity**

## Files you own this wave

- `rust/crates/tiller_ui/src/sidebar.rs`
- `rust/crates/tiller_ui/src/project_identity.rs`
- `rust/crates/tiller_ui/src/row_reorder.rs`

No other agent will touch these. **You must not edit any file outside this list** —
every other source file belongs to a sibling and an edit there is silently lost or
silently overwrites theirs. If a fix genuinely needs a file you do not own, stop and
report it rather than making the change.

## Rows

### `F-SID-17` — ledger line 86, currently **FAILED — defective**

- **Triage:** build, size M
- **Files triage expects:** rust/crates/tiller_ui/src/sidebar.rs, rust/crates/tiller_ui/src/row_reorder.rs
- **Approach:** Hand-traced row_drag/reorder_rows/insertion_index against the exact P109 same-project two-worktree drag scenario -- the arithmetic and scoping are correct, and the machinery is identical to F-SID-16's project-row drag which works live. No code defect found by static reading; flagged as an interaction-tier hypothesis (nested-row hit-testing, or the adjacent hover-x control) that needs live debug instrumentation to localize, not a confident diagnosis.
- **Evidence on record:** Same real drag technique as F-SID-16 (15 interpolated steps) produced no reorder, immediate or delayed — a follow-up capture after a hover event (ruling out stale-repaint) still showed the original order. shots/120.

### `F-PRJ-01` — ledger line 94, currently **FAILED — defective**

- **Triage:** build, size S
- **Files triage expects:** rust/crates/tiller_ui/src/sidebar.rs
- **Approach:** Move the `.when(add_project_menu, ...)` block (sidebar.rs:2436) down to sit with the other three end-of-child-list overlay blocks (context_menu/project_settings/project_form, :2631-2638) so it paints last, on top -- matching the established pattern those three already use. The bg is already opaque (card_fill has a=1.0); this is a paint-order bug, not an alpha bug.
- **Evidence on record:** **pass 13 is superseded** — the `+` now opens a three-item menu (`sidebar.rs:1617/1626/1635`: Open Project… / Clone Repository… / Create Project…) and Clone really opens its form. **But the menu draws with no opaque background**: the sidebar Filter field and the `tiller` project path composite straight through it, and the topmost item — `Open Project…`, the primary path for adding an existing proj

### `F-PRJ-11` — ledger line 104, currently **FAILED — absent**

- **Triage:** build, size S
- **Files triage expects:** rust/crates/tiller_ui/src/sidebar.rs
- **Approach:** render_project_settings (sidebar.rs:1865-2022) has no removal control, confirmed by full enumeration. The removal logic already exists and is fully correct: request_remove_project (sidebar.rs:1055-1075) does the native confirm-prompt + files-untouched removal the clause asks for, just only reachable from the context menu today. Add a trash button to render_project_settings whose on_click calls the same request_remove_project.
- **Evidence on record:** Live-driven: full Project Settings sheet enumerated (path, repo type, display name, icon picker, Reset, Close, id) — no removal/trash control anywhere; Reset is scoped to icon/colour, not removal. Only Remove Project door is the context menu one level up. shots/29b,33.

### `F-PRJ-12` — ledger line 105, currently **half-proven**

- **Triage:** both, size S
- **Files triage expects:** rust/crates/tiller_ui/src/sidebar.rs
- **Approach:** Staleness bug is real: open_project_settings (sidebar.rs:889-940) snapshots card.is_git once; set_projects (sidebar.rs:499-509), called via refresh_sidebar after Initialize Git, rebuilds self.rows but never patches an already-open project_settings card. Fix: in set_projects, after rebuilding rows, patch card.is_git/card.path for a matching open card by id. Separately, display-name propagation to the sidebar row is already correct (on_display_name_key writes row.title live, sidebar.rs:852-886) -- just needs a post-Close screenshot, no code change.
- **Evidence on record:** Display-name field + live sheet-header update confirmed. Repo-type switch (Folder→Git) confirmed via filesystem + reopened sheet + context menu, though the OPEN sheet itself doesn't live-refresh (staleness bug). Sidebar-shows-new-name specifically not directly screenshotted post-close (sheet occludes sidebar while open). shots/29b,33.

### `F-PRJ-16` — ledger line 109, currently **half-proven**

- **Triage:** build, size M
- **Files triage expects:** rust/crates/tiller_ui/src/project_identity.rs, rust/crates/tiller_ui/src/sidebar.rs
- **Approach:** The Open-Emoji-Picker button is a real dead control, not just unwired: on_open_emoji_picker (project_identity.rs:259,310-312) is a host-callback hook whose only caller anywhere in rust/ is a unit test (:1306); the real mount site (sidebar.rs open_project_settings, :912-920) never chains it, AND no emoji-picker overlay/grid component exists anywhere in the codebase to open. Needs a real overlay UI built (searchable grid; no Linux equivalent of macOS's character palette to shell out to) plus wiring the callback at the mount site.
- **Evidence on record:** Entry/validation half proven live: invalid ab rejected with exact text Enter exactly one emoji.; valid single emoji accepted (swatch updates, no error) -- first non-ASCII typed input proven on this lane. Row's own VERIFY clause also requires Open Emoji Picker: clicked with field empty, no observable effect, picker overlay never reached -- that required step stays unexercised, so the row is not ful

