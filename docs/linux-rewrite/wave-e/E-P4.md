# Wave E slice E-P4 — 1 rows

**Scheduling round 3 of 5.** Running at the same time as: E-C-1 — none of them owns any file you own, verified before launch.

Slices in **earlier** rounds have already landed their commits; slices in **later** rounds have not started. So every file below is yours exclusively right now — but **re-read each one from disk before editing**, because an earlier round has changed some of them since this brief was written. Do not trust quoted line numbers.

## Files you own

- `rust/crates/tiller_theme/src/cosmic/mod.rs`
- `rust/crates/tiller_ui/src/browser.rs`

## Rows

### `F-BRW-01` — ledger line 250, currently **FAILED — defective**

- **Files:** rust/crates/tiller_theme/src/cosmic/mod.rs, rust/crates/tiller_ui/src/browser.rs
- **Wave-D critic evidence (2026-08-15, current tree):** Opened a real Browser tab (New Browser menu) on the X11 lane, loaded https://example.com with real rendered content. Two independent pixel-row scans (convert txt:-) show content spanning x=331..1059 against the pane's true chrome bounds x=386..1236 -- 55px sidebar bleed, 177px unpainted gap before Files panel, reproducing the prior critic's exact numbers fresh, live, today. Confirms the GPUI-vs-GTK scale mismatch the builder diagnosed is still present; no fix attempted (native_webview_rect already shown correct; bug is on wry/GTK's side). sweep D-P1, 2026-08-15

