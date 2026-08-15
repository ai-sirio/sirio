# Wave C slice C-MAIN-2 — 15 rows

**Serialised slice — you are link 2 of 4 in the `C-MAIN` chain.** The links before you have already landed and the links after you have not started, so **no sibling holds your files while you work.** Commit only the files listed below.

## Files you own

- `rust/crates/tiller/src/main.rs`

## Rows

### `F-CORE-ACT-25` — ledger line 361, currently **FAILED — absent**

- **Files triage named:** rust/crates/tiller_activity/tests/activity_domain_integration.rs, rust/crates/tiller/src/main.rs, rust/crates/tiller_activity/src/bootstrap.rs
- **Evidence on record:** FAILED — absent (reclassified): independently re-confirmed BootstrapRestoreOrder::partition has zero call sites outside its own module/test workspace-wide, and restore_tabs_in_workspace (main.rs:7703) never references it. Same shape as already-FAILED F-CORE-ACT-26's ids_to_evict; caller-count evidence resolves the prior single-snapshot ambiguity.

### `F-CORE-ACT-26` — ledger line 362, currently **FAILED — absent**

- **Files triage named:** rust/crates/tiller_activity/src/mount.rs, rust/crates/tiller_project/src/settings.rs, rust/crates/tiller/src/main.rs
- **Evidence on record:** FAILED — absent: ids_to_evict has 0 callers outside its own test (grep re-confirmed); live, 4 worktrees survive 2 full restarts with zero evictions. No eviction path exists to observe. P120, 2026-08-14.

### `F-CORE-DOM-03` — ledger line 371, currently **NOT EXERCISED**

- **Files triage named:** rust/crates/tiller/src/main.rs
- **Evidence on record:** Reconfirmed on current HEAD: swaymsg get_tree shows single con before/after click, no portal window in this lane's compositor. Across all 9 frames in both attempts, the add-project-menu popover itself is never seen open, so even the click-reaches-prompt_for_paths step is unconfirmed here. Host-desktop portal ambiguity (ENVIRONMENT.md:77) unresolved from this lane.

### `F-CORE-DOM-07` — ledger line 375, currently **NOT EXERCISED**

- **Files triage named:** rust/crates/tiller_project/src/domain.rs, rust/crates/tiller/src/main.rs
- **Evidence on record:** P116's wt-<seconds> branch-name evidence is main.rs:7829's git worktree fallback namer, not AutoNamingThrottle (domain.rs:92, MIN_INTERVAL=30s/MIN_GROWTH=200). AutoNamingThrottle/should_request/record_request have zero callers outside domain.rs and tests/p99_naming_throttle.rs (grep-confirmed). Clause untouched.

### `F-CORE-FILE-04` — ledger line 389, currently **FAILED — defective**

- **Files triage named:** rust/crates/tiller_project/src/file_link.rs, rust/crates/tiller_ui/src/editor.rs, rust/crates/tiller_terminal/src/link_router.rs, rust/crates/tiller_ui/src/file_view.rs, rust/crates/tiller/src/main.rs
- **Evidence on record:** Click detection/resolve/emit built & tested, but exhaustive grep confirms zero subscribers to FileViewEvent anywhere in the app (main.rs wires the sibling RightPanelEvent::OpenFile/ChatEvent::OpenFile but not this one) -- a real click can never open the file end to end.

### `F-CORE-SET-01` — ledger line 397, currently **half-proven**

- **Files triage named:** rust/crates/tiller/src/main.rs, rust/crates/tiller_ui/src/settings.rs, rust/crates/tiller_project/src/settings.rs, rust/crates/tiller_ui/src/sidebar.rs
- **Evidence on record:** half-proven: 5 malformed DB values (font sizes, theme, socket-enable, refresh interval) confirmed live via ctl read after a real restart to fall back/clamp correctly, no crash. Other named settings (mount cap, sidebar widths, TILLER_SOCKET_ENABLE env override, summarizerAgent) not independently re-read live.

### `F-CORE-WSP-04` — ledger line 380, currently **NOT EXERCISED**

- **Files triage named:** rust/crates/tiller_project/src/layout.rs, rust/crates/tiller/src/main.rs
- **Evidence on record:** P116's panel.split drive uses PaneRegistry::split (main.rs:1131, raw PTY pane split), not tiller_project::layout::LayoutCommand (layout.rs:282)/classify_layout_command (:333) - zero callers outside layout.rs (grep-confirmed). Insert/move/close/activate/divider/view-state/rename remain untouched; the split proven is a different subsystem.

### `F-CORE-WSP-08` — ledger line 384, currently **NOT EXERCISED**

- **Files triage named:** rust/crates/tiller_project/src/layout.rs, rust/crates/tiller/src/session.rs, rust/crates/tiller/src/main.rs, rust/crates/tiller_ui/src/editor.rs, rust/crates/tiller_ui/src/file_view.rs, rust/crates/tiller_ui/src/chat.rs
- **Evidence on record:** P116 honestly reports it did not expose WorkspaceTabViewState. Its finding (control panes gone after quit+relaunch against the same DB) is pane-restore, not per-tab caret/scroll/folds/draft/follows-tail state across a close/reopen. Worth flagging for session-restore rows, not this one.

### `F-CTRL-BROWSER-02` — ledger line 445, currently **FAILED — defective**

- **Files triage named:** rust/crates/tiller/src/main.rs
- **Evidence on record:** FAILED — defective (narrower). Named defects (surface/url/title; missing-url) fixed, confirmed live + via source (main.rs:4476-4491). Driver's 'fresh instance' third-call claim was false (own screenshot shows prior project/worktree state); critic re-ran a genuinely fresh no-project.add probe — browser.open still succeeds with zero workspace context. Missing-workspace/adapter-unavailable rejection still absent (no check exists in source).

### `F-CTRL-BROWSER-03` — ledger line 446, currently **FAILED — defective**

- **Files triage named:** rust/crates/tiller/src/main.rs
- **Evidence on record:** `N/A — platform` void: the excuse's premise was "browser is out of scope", and the browser now exists and works (P83, verified live) with the F-BRW rows explicitly in scope. `browser.get` exercised live 03:02:56 in both clause variants — `{selector:h1, format:text}` and `{format:html}` — each returned `queued:true` and **no URL, no text, no HTML**; the handler is a no-op (main.rs:4638-4643, `let _ = surface.state();`). `browser.navigate` implements **none of the clause's three operations**: main.rs:4621-4629 handle

### `F-CTRL-BROWSER-04` — ledger line 447, currently **FAILED — defective**

- **Files triage named:** rust/crates/tiller/src/main.rs
- **Evidence on record:** `N/A — platform` void: the excuse's premise was "browser is out of scope", and the browser now exists and works (P83, verified live) with the F-BRW rows explicitly in scope. Both methods exercised live 03:02:56. `browser.screenshot` with an explicit `path=/tmp/probe-shot.png` returned `queued:true` and **wrote no file** (checked on disk immediately after: absent), returning no path. `browser.snapshot` returned `queued:true` and **no generation and no nodes**. Both sit in the no-op arm at main.rs:4638-4643

### `F-CTRL-BROWSER-05` — ledger line 448, currently **FAILED — defective**

- **Files triage named:** rust/crates/tiller/src/main.rs
- **Evidence on record:** `N/A — platform` void: the excuse's premise was "browser is out of scope", and the browser now exists and works (P83, verified live) with the F-BRW rows explicitly in scope. `browser.wait` exercised live 03:02:56 with `{selector:h1, timeout:1000}` — returned `queued:true` immediately, no elapsed time, no condition evaluated; it is a no-op (main.rs:4638-4643), and none of the clause's five conditions (selector/text/URL/load state/function) is parsed. `browser.act` implements **none** of click/fill/type/press/scroll:

### `F-CTRL-BROWSER-06` — ledger line 449, currently **FAILED — defective**

- **Files triage named:** rust/crates/tiller/src/main.rs
- **Evidence on record:** `N/A — platform` void: the excuse's premise was "browser is out of scope", and the browser now exists and works (P83, verified live) with the F-BRW rows explicitly in scope. Both exercised live 03:02:56. `browser.eval` with `{script:"1+1"}` returned `queued:true` and **no value** — the script is never read from params, let alone evaluated. `browser.console` with `{cursor:"0"}` returned `queued:true` and **no entries**; the cursor is ignored. Both are the no-op arm at main.rs:4638-4643

### `F-CTRL-CLI-02` — ledger line 451, currently **half-proven**

- **Files triage named:** rust/crates/tiller/src/main.rs
- **Evidence on record:** half-proven. Installed XDG symlink half re-confirmed directly by critic (ls -la ~/.local/share/TillerRust/bin/tillerctl). Real-agent-hook half still unexercised, but driver's 'only reachable via ctrl-shift-p' claim is false — tab-bar's plain-click '+' menu offers Claude Code/Codex/etc directly (tab_bar.rs:518-596), no chord, routing through adapter.prepare(). Next pass: click it.

### `F-CTRL-WORK-01` — ledger line 435, currently **FAILED — defective**

- **Files triage named:** rust/crates/tiller/src/session.rs, rust/crates/tiller_persistence/src/model.rs, rust/crates/tiller_project/src/worktree.rs, rust/crates/tiller/src/main.rs
- **Evidence on record:** verdict stands, **evidence corrected — the pass-6 "no comment column" is false and would have sent someone to build what exists**. The column is real: `migrations.rs:156` adds it, `model.rs:73` carries `comment: Option<String>`, and `db.rs:244` writes it in the worktree upsert (`comment = excluded.comment`). The actual cause is one line up the stack — `main.rs:328` declares the `worktree.set` annotation a runtime field and comments it **"intentionally not persisted"**. So the comment is lost by design, not for want

