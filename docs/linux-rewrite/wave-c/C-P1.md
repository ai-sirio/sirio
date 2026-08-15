# Wave C slice C-P1 — 7 rows

Runs in parallel with other slices. Every file your rows touch is listed below and no other slice owns any of them.

## Files you own

- `rust/crates/tiller_git/src/remote.rs`
- `rust/crates/tiller_terminal/src/lib.rs`
- `rust/crates/tiller_ui/src/changes.rs`
- `rust/crates/tiller_ui/src/project_forms.rs`
- `rust/crates/tiller_ui/src/project_identity.rs`

## Rows

### `F-CORE-TERM-02` — ledger line 395, currently **half-proven**

- **Files triage named:** rust/crates/tiller_terminal/src/lib.rs
- **Evidence on record:** Independently reconfirmed: lib.rs:1446,1504 wire open_context_menu only to MouseButton::Right, no keyboard path in the crate; WAYLAND-LANE.md:24 documents right-click as not yet exercised on this lane. Half-proof unchanged; per-item effects still owed on DISPLAY=:1.

### `F-EDIT-12` — ledger line 230, currently **NOT EXERCISED**

- **Files triage named:** rust/crates/tiller_ui/src/changes.rs, rust/crates/tiller_terminal/src/lib.rs
- **Evidence on record:** NOT EXERCISED (unchanged), re-confirmed 2026-08-14: this lane's virtual-pointer tooling has no button-down-only/motion-while-held primitive; drag cannot be composed here by construction. X11/DISPLAY=:1 lane required.

### `F-TERM-PTY-06` — ledger line 531, currently **half-proven**

- **Files triage named:** rust/crates/tiller_terminal/src/lib.rs
- **Evidence on record:** Real gpui::ExternalPaths on_drop handler now exists (zero before this wave) and receive_file_drop takes Vec<PathBuf>; dedicated test drives the actual production handler with GPUI's real XDND payload type and passes. No live instrument on any lane (no drag primitive here; XDND unexercisable per ENVIRONMENT.md) can drive the gesture end to end.

### `F-TERM-UI-02` — ledger line 536, currently **NOT EXERCISED**

- **Files triage named:** rust/crates/tiller_terminal/src/lib.rs
- **Evidence on record:** Re-confirmed (no new drive), verified by critic: opens_terminal_link(event.modifiers.platform) at lib.rs:988 requires a held modifier; wayland-drive.sh's pointer_command has no modifier-click primitive (only plain left-button); keyboard-keeper stays unmodified after startup. X11 lane fallback off-limits per task constraints. Instrument-blocked, not platform-impossible; verdict unchanged.

### `F-GIT-REMOTE-01` — ledger line 489, currently **half-proven**

- **Files triage named:** rust/crates/tiller_ui/src/project_forms.rs, rust/crates/tiller_git/src/remote.rs, rust/crates/tiller_ui/src/project_identity.rs
- **Evidence on record:** Critic re-verified 2026-08-14 (independent grep + test run, not driver's word): grep -rn github_owner rust/ confirms zero callers outside tiller_git (re-export + internal wrapper + tests only); reran remote_parsing_supports_github_ssh_https_and_project_suffixes, passed (already built, 0.25s). project_name's app-reachability at project_forms.rs:704 reconfirmed untouched. Verdict unchanged: half-proven.

### `F-PRJ-06` — ledger line 99, currently **half-proven**

- **Files triage named:** rust/crates/tiller_ui/src/project_forms.rs
- **Evidence on record:** half-proven (unchanged). Guard half still unreachable, now root-caused more precisely: this Wayland lane drops clicks/keystrokes into the anchored/floating popover class generally (Clone-repository, Create-project), not just text entry -- a Cancel-button-click control failed 3/3 despite visible hover/press highlight, while the sidebar Filter field and Avatar favicon field (non-popover) accepted identical input in the same session. Slow-git PATH shim harness validated and reusable. Empty-URL-disablement half remains

### `F-PRJ-09` — ledger line 102, currently **half-proven**

- **Files triage named:** rust/crates/tiller_ui/src/project_forms.rs
- **Evidence on record:** half-proven (unchanged). Guard half still unreachable, same structural reason as F-PRJ-06 -- anchored-popover input-delivery gap on this lane, not sub-frame git-init timing as previously assumed. Slow-git shim confirmed active (stuck Loading Files...). Empty-name-disablement half remains proven from prior evidence.

