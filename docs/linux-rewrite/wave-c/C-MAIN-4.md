# Wave C slice C-MAIN-4 — 15 rows

**Serialised slice — you are link 4 of 4 in the `C-MAIN` chain.** The links before you have already landed and the links after you have not started, so **no sibling holds your files while you work.** Commit only the files listed below.

## Files you own

- `rust/crates/tiller/src/main.rs`

## Rows

### `F-TAB-01` — ledger line 117, currently **FAILED — defective**

- **Files triage named:** rust/crates/tiller_ui/src/right_panel.rs, rust/crates/tiller/src/main.rs
- **Evidence on record:** Live-driven: every click on an expanded folder's child row (file or subfolder), 3 separate isolated attempts, collapses the parent instead of acting on the child. No document tab ever opened — both the document-icon and dirty-route conjuncts unreachable as a direct consequence. shots/135,142.

### `F-TAB-08` — ledger line 124, currently **half-proven**

- **Files triage named:** rust/crates/tiller_ui/src/tab_bar.rs, rust/crates/tiller/src/main.rs, rust/crates/tiller_ui/src/settings.rs
- **Evidence on record:** Critic verified live click 2026-08-14: pre/post-click menu content is byte-identical (same fallback text, no Settings surface) -- corroborates no-op. Chased an apparent frame-resolution mismatch (04@1715x972 vs 05@1400x900) to wayland-drive.sh's shot() deliberately toggling output size to force a repaint (script lines 285-312); confirmed the click was dispatched at 1715x972, matching the frame the row's bounds were read from, so it genuinely landed inside the fallback row, not a miss. Source-read confirms render_ch

### `F-TAB-11` — ledger line 127, currently **FAILED — absent**

- **Files triage named:** rust/crates/tiller/src/panes.rs, rust/crates/tiller_terminal/src/context_menu.rs, rust/crates/tiller/src/main.rs, rust/crates/tiller_terminal/src/lib.rs
- **Evidence on record:** Live-driven, sole-tab state: right-click menu shows only a plain 'Copy' entry — no Split Left/Right/Above/Down items appear at all, disabled or otherwise, no reason text. Clause's premise (a disabled split item with a reason) does not exist. shots/146.

### `F-TAB-13` — ledger line 129, currently **half-proven**

- **Files triage named:** rust/crates/tiller/src/main.rs
- **Evidence on record:** Z-order fix confirmed live (17-move-to-pane-menu.png, 21-attach-self-disabled.png: correct disabled-reason text). But headline capability structurally unreachable: after live Split Right (15/16-split.png) re-right-click still shows 'no other pane available' (17...png, taken after split); add_group is #[cfg(test)]-only (tab_machinery.rs:150), every live tab-creation site in main.rs hard-codes active_group() -- dead code in production, only reached by a test that forges tab_machinery directly.

### `F-TAB-16` — ledger line 132, currently **FAILED — defective**

- **Files triage named:** rust/crates/gpui/src/window.rs, rust/crates/tiller/src/main.rs, rust/crates/tiller_ui/src/right_panel.rs, rust/crates/tiller_ui/src/editor.rs
- **Evidence on record:** P104 §Group 2: closing the dirty file-hosting tab silently discarded the unsaved y with no confirm/discard prompt. The two-dirty-tabs precondition was also unreachable.

### `F-TAB-23` — ledger line 139, currently **FAILED — defective**

- **Files triage named:** rust/crates/tiller/src/main.rs, rust/crates/tiller/src/panes.rs
- **Evidence on record:** P106 fable §F-TAB-23: live pane.split created all directions, but direction=left placed the new terminal on the right exactly like right; the required corresponding left placement is wrong. Menu-route exercise remains owed.

### `F-TAB-28` — ledger line 144, currently **FAILED — defective**

- **Files triage named:** rust/crates/tiller/src/panes.rs, rust/crates/tiller/src/main.rs, rust/crates/tiller_terminal/src/lib.rs, rust/crates/tiller_ui/src/chat.rs
- **Evidence on record:** Live-driven: ctrl-w on clean tab (Chat) — no change across 3 isolated tries incl. explicit refocus. ctrl-w on dirty tab (Terminal, tab_is_dirty) — byte-identical capture, no confirm dialog, no close. Zero observable effect on either path. shots/164,165,168,169.

### `F-TERM-08` — ledger line 326, currently **FAILED — defective**

- **Files triage named:** rust/crates/tiller/src/main.rs, rust/crates/tiller_activity/src/activity.rs
- **Evidence on record:** Disagree with driver's exercised-working/PASSED claim. Critic-verified requires_close_confirmation (activity.rs:30) has zero app-crate callers (only its own def + 4 assertions in one test); grepped main.rs confirmation-dialog sites -- all gate CloseTab (3959-4053, 5722-5723), none gate ClosePane. Confirmation half is proven absent, not gesture-unproven. Critic re-traced both call paths (menu delegation, socket pane.close) to the identical close_terminal_at, corroborating driver's live captures (3->2 panels on close

### `F-TERM-09` — ledger line 327, currently **FAILED — defective**

- **Files triage named:** rust/crates/tiller_activity/src/title.rs, rust/crates/tiller_activity/src/model.rs, rust/crates/tiller/src/main.rs
- **Evidence on record:** pass 17, exercised across four live agent launches: indicators EXIST but do not TRACK activity. Observed state catalog — a fresh terminal-agent tab shows a `?` badge while the TUI sits at its prompt (p17-ao1/aq1) and an amber ● appears on the worktree's sidebar row at first agent launch; but during a real working turn (`✳ Orchestrating…` live in the TUI) the tab shows NO working indication (badge simply gone, p17-as2-working), after the turn no idle/done state returns (p17-ar3/as4 — though a 1s turn left `?` frozen

### `F-TERM-11` — ledger line 329, currently **FAILED — absent**

- **Files triage named:** rust/crates/tiller/src/main.rs, rust/crates/tiller_terminal/src/lib.rs
- **Evidence on record:** no no-worktree empty state

### `F-TERM-SPLIT-01` — ledger line 534, currently **FAILED — defective**

- **Files triage named:** rust/crates/tiller/src/panes.rs, rust/crates/tiller_terminal/src/lifecycle.rs, rust/crates/tiller/src/main.rs
- **Evidence on record:** P106 §F-TERM-SPLIT-01 plus fable §F-TAB-23: live split routes exist, but left direction placed the new pane on the right and TerminalPaneCache remains unintegrated; full recursive/cache lifecycle clause is defective.

### `F-TERM-UI-01` — ledger line 535, currently **half-proven**

- **Files triage named:** rust/crates/tiller/src/main.rs
- **Evidence on record:** Re-confirmed (no new drive), verified by critic: same right-click-only binding blocks menu render; F-TERM-08's socket ClosePane proves only the state half (no confirmation dialog), not this row's menu-render-plus-click clause. Z-order defect (Files panel over open menu) independently visible in p17-rclick-term.png, confirmed by critic; not re-photographed here since the menu cannot be opened from this lane. Owed half unchanged.

### `F-USE-06` — ledger line 269, currently **half-proven**

- **Files triage named:** rust/crates/tiller/src/main.rs
- **Evidence on record:** half-proven (changed from FAILED-defective) 2026-08-14: cited root cause is stale -- register_restored_agent (main.rs:7567-7574) is now wired into both restore_tabs (:7608) and restore_tabs_in_workspace (:7734), confirmed by direct read. Live notify session=pane-0 status=running/needs-input via the real code path produced correctly zero D-Bus traffic, but only because the tested pane (surface.chat.open, no agent param) was never agent-identified -- a correct no-op, not a delivery test. Owed: live positive fire for 

### `F-WIN-07` — ledger line 59, currently **half-proven**

- **Files triage named:** rust/crates/tiller/src/main.rs, rust/crates/tiller_ui/src/titlebar.rs
- **Evidence on record:** Critic independently re-grepped 2026-08-14: no History menu/previous-launch string anywhere in tiller/tiller_ui src; all history hits unrelated (browser back/forward, chat-history setting, pane event-sourcing, unrelated test var). Did not re-hit the live socket (a sibling agent's drive was active on the shared instance at review time); relies on driver's session.restore reproduction plus the already-independently-established P106/P108 restoredCount:0 finding. Verdict unchanged: half-proven.

### `F-WIN-10` — ledger line 62, currently **FAILED — absent**

- **Files triage named:** rust/crates/tiller/src/main.rs
- **Evidence on record:** Live-driven: 2 real errors triggered (duplicate-path OS error, invalid-name validation error) via sidebar + → Create Project — both render as persistent inline red text inside the dialog; no toast anywhere on screen (bottom-right or elsewhere) in either full-screen capture, neither auto-dismissed. shots/208,210.

