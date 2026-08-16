# Wave G slice G1-browser — 3 rows

**Scheduling round 1 of 6.** You run alone this round.

Earlier rounds have landed; later rounds have not started. Every file below is yours exclusively right now — but **re-read each from disk before editing**; an earlier round may have changed it since this brief was written, so do not trust quoted line numbers.

## Files you own

- `rust/crates/tiller/src/main.rs`
- `rust/crates/tiller_terminal/src/lib.rs`
- `rust/crates/tiller_theme/src/cosmic/mod.rs`
- `rust/crates/tiller_ui/src/browser.rs`

## Rows

### `F-BRW-01` — ledger line 250, currently **FAILED — defective**

- **Files:** rust/crates/tiller_terminal/src/lib.rs, rust/crates/tiller_theme/src/cosmic/mod.rs, rust/crates/tiller_ui/src/browser.rs
- **Latest critic evidence (2026-08-16, current tree):** Live re-check on DISPLAY=:1, two independent fresh launches (restored tab + genuinely new tab via + -> New Browser): pixel scans at y=400/y=850 reproduce D-P1's exact numbers (content x=331..1060 vs chrome x=386..1236) byte-for-byte on current HEAD. The self-calibrating fix (browser.rs:1752-1805) is real code with a passing unit test, but that test never calls the real webview.bounds() — live it produces no change; a token-call-site with a passing test but no live behavior change.

### `F-CTRL-BROWSER-05` — ledger line 448, currently **half-proven**

- **Files:** rust/crates/tiller/src/main.rs, rust/crates/tiller_ui/src/browser.rs
- **Latest critic evidence (2026-08-16, current tree):** Found a pre-existing E-C-2-verdicts.md (commit c699ac3, predates this dispatch, never reached the ledger) claiming PASSED with clean content. I could not reproduce it: two independent fresh X11-lane (DISPLAY=:1) processes, non-blocking browser.get read loading:true/title:'' immediately and 15s later (curl to the same URL succeeds instantly, ruling out network), and browser.eval returned 'Browser child is unavailable'. Also found CONTROL_ACTION_TIMEOUT (main.rs:196, hardcoded 5s) truncates any browser.wait timeoutMs above ~5s with a misleading 'control action timed out', a real but separate defect. Recorded half-proven, matching the builder/wave-D, not the conflicting prior file.

### `F-CTRL-BROWSER-06` — ledger line 449, currently **half-proven**

- **Files:** rust/crates/tiller/src/main.rs, rust/crates/tiller_ui/src/browser.rs
- **Latest critic evidence (2026-08-16, current tree):** Same two X11-lane runs as BROWSER-05: browser.eval script=document.title consistently returned 'Browser child is unavailable' or the 5s dispatch-timeout artifact, never real content. Could not reproduce the prior (unlanded) file's claimed 'Example Domain' result despite two independent attempts.

