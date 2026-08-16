# Wave F slice F3-browser — 3 rows, re-verification only

Every row below is currently **PASSED** and counted in the ledger's headline total, but its
`judged` stamp names the orchestrator itself or a source whose independence is not structurally
guaranteed. See `../tasks/P125-rows-without-independent-provenance.md`.

**You are re-judging these from scratch. Do not edit any code.**

## Rows

### `F-TAB-06` — ledger line 122, currently **PASSED**

- **Judged by:** orchestrator drive, 2026-08-14
- **Evidence on record:** **pass 8 superseded** — the browser exists now. Live: tab-bar `+` -> **New Browser** created a Browser tab with a globe icon in both the tab bar and the sidebar tree, loading `https://example.com/` (`orch21-base.png`, `orch24-restore.png`). The `NewBrowser typed no-op` finding is stale

### `F-CTRL-BROWSER-01` — ledger line 444, currently **PASSED**

- **Judged by:** orchestrator headless-lane probe, 2026-08-14 05:05
- **Evidence on record:** **both defects of the 03:02 probe are fixed by `P90` (commit `988d9e9`), re-exercised live on the headless lane at 05:05 against a binary built after it.** All three contract points now hold: (a) the dispatcher **accepts all ten** — each returns a method-specific response, never "unknown method"; (b) **`browser.errors` is no longer advertised in `system.capabilities`** — 0 occurrences, where the 03:02 probe found it at `main.rs:1042`, so the clause's documented discrepancy now reproduces exactly; (c) **no tillerctl builder command for browser** — zero `browser` hits in `tiller_control`, unchanged and matching. Acceptance is **no longer hollow**: the old `queued:true`-with-no-data is gone, replaced by explicit honest refusals — `"browser.get is unsupported on Linux: browser automation is no

### `F-GIT-CLONE-01` — ledger line 488, currently **PASSED**

- **Judged by:** orchestrator audit, grep evidence, 2026-08-14
- **Evidence on record:** pass 12: `clone_from_local_repository_reports_receiving_progress` green (p41_git_behaviors.rs:122) — incremental Receiving objects % + invalid-source CommandFailed through the runner. **The pass-12 "zero app callers" is now stale and is removed**: `clone_repository` is imported at `tiller_ui/src/project_forms.rs:11` and the clone form was driven live at pass 17. Verdict unchanged; the note was the thing that was wrong

