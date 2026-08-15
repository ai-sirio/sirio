# Wave E slice E-P3 — 2 rows

**Scheduling round 2 of 5.** Running at the same time as: E-C-2 — none of them owns any file you own, verified before launch.

Slices in **earlier** rounds have already landed their commits; slices in **later** rounds have not started. So every file below is yours exclusively right now — but **re-read each one from disk before editing**, because an earlier round has changed some of them since this brief was written. Do not trust quoted line numbers.

## Files you own

- `rust/crates/tiller_ui/src/project_forms.rs`
- `rust/crates/tiller_ui/src/project_identity.rs`
- `rust/crates/tiller_ui/src/sidebar.rs`

## Rows

### `F-PRJ-06` — ledger line 99, currently **FAILED — defective**

- **Files:** rust/crates/tiller_ui/src/project_forms.rs, rust/crates/tiller_ui/src/sidebar.rs
- **Wave-D critic evidence (2026-08-15, current tree):** Re-drove the real sidebar-confined popover at its correct on-screen position (per P123). Cancel closes reliably on first click every time -- not a click-routing bug. But the URL field drops nearly all typed characters across 5 independent attempts (best case 13/44 chars); a same-session positive control types 18 chars into the Projects filter field perfectly via the identical mechanism, ruling out a lane artifact and pointing at the Clone form's own per-keystroke Destination-recompute. can_submit() correctly stays false on the resulting empty/partial url, so status never leaves Ready to clone. sweep D-P1, 2026-08-15

### `F-PRJ-14` — ledger line 107, currently **half-proven**

- **Files:** rust/crates/tiller_ui/src/project_identity.rs, rust/crates/tiller_ui/src/sidebar.rs
- **Wave-D critic evidence (2026-08-15, current tree):** Favicon arm unchanged (already proven). Read render_avatar_mode: GitHub-avatar arm only ever shows a text caption, never fetches/renders an actual image (network fetch deliberately not built here) — same visible-image gap as the PNG arm, just for a different underlying reason. sweep D-U, 2026-08-15

