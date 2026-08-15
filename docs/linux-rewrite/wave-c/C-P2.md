# Wave C slice C-P2 — 7 rows

Runs in parallel with other slices. Every file your rows touch is listed below and no other slice owns any of them.

## Files you own

- `rust/crates/tiller_activity/src/model.rs`
- `rust/crates/tiller_ui/src/status_bar.rs`
- `rust/crates/tiller_usage/src/claude.rs`
- `rust/crates/tiller_usage/src/codex.rs`
- `rust/crates/tiller_usage/src/model.rs`

## Rows

### `F-CORE-USG-05` — ledger line 403, currently **half-proven**

- **Files triage named:** rust/crates/tiller_usage/src/codex.rs
- **Evidence on record:** Live: real tiller binary with synthetic backdated CODEX_HOME creds triggered the previously-zero-caller needs_refresh gate, logging 'proactive token refresh failed: Expired' within seconds; independent curl to auth.openai.com reproduced the same 401 token_expired response. Success-path merge-not-overwrite not independently re-driven (no real valid Codex creds available) — stays half-proven.

### `F-CORE-USG-06` — ledger line 404, currently **half-proven**

- **Files triage named:** rust/crates/tiller_usage/src/codex.rs, rust/crates/tiller_usage/src/model.rs, rust/crates/tiller_ui/src/status_bar.rs
- **Evidence on record:** Reconfirmed unchanged per instructions (stays half-proven, needs-foreign-file): same live run reached the reactive discard site via a real non-mocked HTTP failure, logging 'token refresh failed: Expired'; but status bar screenshot at the same time still shows only generic 'Codex logged out' — no Reused/Revoked/Expired-specific copy, model.rs/status_bar.rs untouched.

### `F-USE-02` — ledger line 265, currently **half-proven**

- **Files triage named:** rust/crates/tiller_ui/src/status_bar.rs
- **Evidence on record:** Hovering loaded Claude segment (140,901) for 1.6s now shows a tooltip reading 'Claude 25% 5h · 84% wk', matching segment text — this half flipped from previously-confirmed-absent to present. Unavailable-segment tooltip case still unreached: forcing a provider unavailable depends on F-USE-03 (separate row, out of scope), so row stays half-proven.

### `F-USE-03` — ledger line 266, currently **half-proven**

- **Files triage named:** rust/crates/tiller_usage/src/model.rs, rust/crates/tiller_usage/src/claude.rs
- **Evidence on record:** half-proven (unchanged): live loaded+logged-out states previously confirmed; Loading/Stale still untested. This pass captured a fresh-launch frame (02-use-immediate.png) but no OCR instrument exists on this lane to distinguish which text string the usage-bar region shows, so the capture is uninterpretable for this question.

### `F-CORE-ACT-06` — ledger line 342, currently **half-proven**

- **Files triage named:** rust/crates/tiller_activity/src/model.rs
- **Evidence on record:** half-proven: independent crop of act06's clean continuous run (clean-x0..x4) against tab_status_glyph source shows a genuine hollow->filled->hollow identify-then-clear cycle, correcting the driver's own mismatched-baseline pixel-diff read. Owed half: repeat on a process-owned pane and confirm unrelated title text does NOT clear it — not attempted this pass.

### `F-CORE-ACT-07` — ledger line 343, currently **NOT EXERCISED**

- **Files triage named:** rust/crates/tiller_activity/src/model.rs
- **Evidence on record:** NOT EXERCISED (unchanged): debounce mechanism re-confirmed real and wired (TITLE_DEBOUNCE=1500ms, title.rs:23; should_apply_title_signal wired into handle_title_change, model.rs:223) but no live race gesture attempted this pass — same blocker as before.

### `F-CORE-ACT-11` — ledger line 347, currently **NOT EXERCISED**

- **Files triage named:** rust/crates/tiller_activity/src/model.rs
- **Evidence on record:** NOT EXERCISED (unchanged): spawn-owned leg still blocked by unreachable + menu; process-owned leg still unproven (see ACT-10); title-owned leg individually has evidence now (see ACT-06) but was never combined with the other two on separate panes as this row's claim requires.

