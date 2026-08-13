# The stale-FAILED census (FABLE-08)

**Question.** Of the 141 ledger rows marked `FAILED — absent`, how many are already built? The
mirror of FABLE-07's false-PASSED audit: a false FAILED sends a builder to construct something
that already exists, and that price has been paid repeatedly (F-TAB-02 / F-TAB-27 were built by
codex12 in P65 and still marked absent within the hour).

**Evidence, not verdicts.** No ledger row changes here. Every claim below is an input to
`pireview`, which adjudicates.

## What this census is pinned to

- Ledger: `docs/linux-rewrite/pins/INVENTORY-LEDGER.FABLE-08.md` — a verbatim copy of the
  working-tree ledger at census time, sha256
  `85c7abf986d3f234cf8e6ba8974b33fa3145ba22f935c30583c02a52b72af37d`.
- Tree: an rsync snapshot of `rust/ docs/ Scripts/` taken 2026-08-13T19:30:21Z at HEAD
  `f0b44ef` + 119 dirty files (69 untracked), manifest-hash `6bdfb52eb0af0753`. Five builders
  were editing; every search in this census ran against the snapshot, not the moving tree.
  The snapshot itself is ephemeral (`/tmp`), so the durable evidence is the **needle**, not the
  line number: `Scripts/stale-failed-census.py` re-verifies every "already built" citation
  against whatever `--tree` it is given and prints live line numbers, failing loudly if a
  needle stops matching.

## Method

Rows were settled by reading the snapshot, searching in **Linux/GPUI vocabulary rather than the
row's own words** (⌘S → `ctrl-s` in `linux_window_shortcuts`, NSWindow → gpui Window,
WKWebView → webview, Keychain → keyring, SwiftUI menus → gpui overlays). Statuses:

- **already built** — file + needle cited, machine-re-verified on every script run.
- **still absent** — searched and not found; the terms are disclosed per row. Disclosed, not
  machine-asserted: many terms legitimately occur elsewhere (`auth` in a test fixture, `diff`
  in error-card styling), so "term found ⇒ row wrong" would itself be a false signal.
- Rows proven absent by *exercising* (pass-13 display transcripts: F-PRJ-01/03, F-CHG-02) were
  re-read, not re-exercised — a static census cannot overturn behavioral evidence.

## Three ways a row's wording defeats the search for it

1. **Old-platform vocabulary.** `F-EDIT-04` says ⌘S; the binding is `ctrl-s`. Same for ⌘T
   (F-SID-19) and ⌘, (F-WIN-01). F-TAB-28 is the class with a twist: the shipped Linux chord is
   *literally* `"cmd-w"` in shell code, so the row's ⌘W defeats a cmd-search in both directions.
2. **Wrong crate.** `F-SET-09`/`F-AGENT-SAFE-01` look for skill code in `tiller_agents`; it
   lives in `tiller_project/src/skill.rs`. `F-CORE-FILE-06`'s watcher lives in
   `tiller_markdown`.
3. **Symptom stated as cause.** `F-PRJ-04` reads "insertion failures are eprintln-only — no
   error surface". The surface **exists** (`Sidebar.notice`, `set_notice`, the `sidebar-notice`
   render — with three `set_notice` call sites already live in `main.rs`); only the insertion
   call site bypasses it. Searching for "error surface" and finding none confirms the theory
   vacuously — and the theory is false. Same shape in `F-SET-08` ("no install mechanism, so no
   real command to copy" — `agent_skill_install_command` provides exactly that command, with a
   test) and `F-TAB-09` (the "no other open-file surface" conjunct is now false: ctrl-o, the
   palette, and the tab menu all open files). **When a row asserts *why* something is missing,
   that clause is the least reliable part of the row: search for the capability, never for the
   row's diagnosis.** Related: `F-AUTO-06` and `F-PER-07` cite *other rows' findings* as their
   evidence — they inherit those rows' staleness automatically (both re-verified still true
   today, but the coupling is the risk).

## Reproducing

```
python3 Scripts/stale-failed-census.py            # verify all citations against rust/crates
python3 Scripts/stale-failed-census.py --write    # regenerate the section below
```

The script refuses to print if its backend probe fails (a known-present symbol must be found, a
fabricated one must not), if the pin does not parse to exactly 141 rows covered exactly by the
census, if any of the six known-stale control rows fails to come out built, or if any "already
built" citation stops verifying.

<!-- census:generated:begin -->
- Rows: 141 — **already built: 39**, still absent: 102, cannot tell: 0.
- Never-assigned rows (no builder brief names them, same exclusions as assigned-but-absent.py): 74 of 141 — all settled; 3 of them are already built: `F-SID-19`, `F-AGENT-SAFE-01`, `F-TERM-PTY-05`.
- Wording traps: class 1 old-platform vocabulary ['F-EDIT-04', 'F-SID-19', 'F-TAB-28', 'F-WIN-01']; class 2 wrong-crate ['F-AGENT-SAFE-01', 'F-CORE-FILE-06', 'F-SET-09']; class 3 symptom-as-cause (the row's causal theory is false) ['F-PRJ-04', 'F-SET-08', 'F-TAB-09']; cross-row evidence (inherits another row's staleness) ['F-AUTO-06', 'F-PER-07'].
- Controls: backend probe found `agent_skill_install_command` at tiller_project/src/skill.rs:9 and did not find the fabricated symbol; population 141/141; all 7 known-stale control rows (six findings) came out built; all 39 citations verified.

| row | census | evidence (verified against the tree at run time) |
|---|---|---|
| `F-WIN-01` | still absent | searched: 'ctrl-,' / comma chord / a settings entry in linux_window_shortcuts — the 5 shell chords have no Settings entry; row's CMD-, is class 1 |
| `F-WIN-07` | still absent | searched: History menu / menu_bar / history_menu — a drawn test asserts the shell has NO in-window menu bar (resting_frame_has_context_menu_surfaces_but_no_in_window_menu_bar) — the clause presupposes a menu bar that is absent by design |
| `F-WIN-10` | still absent | searched: toast — only a theme radius token and a conformance comment |
| `F-SID-06` | still absent | searched: status dot on project rows (render_row / RowKind) — dot computed for RowKind::Worktree only — the project-badge half is still unbuilt |
| `F-SID-07` | **already built** | `tiller_ui/src/sidebar.rs:1358` (`render_project_settings`) — Project Settings context-menu item opens the settings card |
| `F-SID-08` | **already built** | `tiller_ui/src/sidebar.rs:475` (`Initialize Git repository`) — menu item with AlreadyGitProject disabled reason, plus a test |
| `F-SID-09` | **already built** | `tiller_ui/src/sidebar.rs:481` (`Show in File Manager`) — menu item; the palette twin is labelled Reveal in File Manager |
| `F-SID-11` | still absent | searched: the worktree-card comment line — PARTIAL drift: branch-as-title, Primary pill and the path context line are built; the comment line is not |
| `F-SID-15` | still absent | searched: Remove Worktree in the worktree context menu — the menu ends at New Chat (drift: it now also carries Set/Unset Primary); removal is still only the hover x |
| `F-SID-16` | still absent | searched: sidebar drag / reorder handlers — a no-drag comment and a no-reorder test pin the design |
| `F-SID-17` | still absent | searched: cross-project drag — same evidence as F-SID-16 |
| `F-SID-18` | still absent | searched: "No Terminals" empty-state string — zero matches |
| `F-SID-19` | **already built** | `tiller/src/main.rs:116` (`WindowCommand::NewTerminalTab`) — bound to ctrl-t; row says CMD-T (class 1) |
| `F-PRJ-01` | still absent | searched: clone-from-URL / create-new forms behind the + control — only the ashpd folder picker; pass-13 exercised on the display — not re-exercised here |
| `F-PRJ-03` | still absent | searched: an Initialize-Git / Add-without-Git / Cancel prompt on non-git add — pass-13 exercised; still no prompt code |
| `F-PRJ-04` | still absent | searched: the insertion-failure route to the sidebar notice — CLASS 3: the row says 'no error surface' but the surface EXISTS — Sidebar.notice + set_notice + the sidebar-notice render, and main.rs already has three set_notice call sites; only the insertion path (eprintln '[projects] ...') bypasses it. The fix is a call-site change, not a new surface |
| `F-PRJ-05` | still absent | searched: clone form / clone_url — no clone UI anywhere |
| `F-PRJ-06` | still absent | searched: clone-form guards — no form to carry them |
| `F-PRJ-07` | still absent | searched: clone failure/retry surface — no form |
| `F-PRJ-08` | still absent | searched: a create-new-project form / "New Project" — the Add flow only browses folders |
| `F-PRJ-09` | still absent | searched: create-form guards — no form |
| `F-PRJ-10` | still absent | searched: create-form failure surface — no form |
| `F-PRJ-11` | still absent | searched: a trash control in the project-settings sheet — the sheet shows name/path/repository/Close/id only |
| `F-PRJ-12` | still absent | searched: repository-type switch / display-name edit — the sheet is explicitly read-only until a persistence contract exists |
| `F-PRJ-13` | still absent | searched: icon colour/reset controls — no icon UI in the sheet |
| `F-PRJ-14` | still absent | searched: avatar / GitHub / PNG / favicon controls — none |
| `F-PRJ-15` | still absent | searched: an icon grid / picker — the sfsymbol renderer exists for in-app glyphs; there is no picker |
| `F-PRJ-16` | still absent | searched: emoji picker / emoji icon input — none |
| `F-PRJ-17` | still absent | searched: default-worktree-base options — derive_worktree_path always derives the parent |
| `F-PRJ-18` | still absent | searched: a custom worktree-location control — the location is derived, not choosable |
| `F-TAB-01` | still absent | searched: a per-status tab cell (Running/Error/Idle) — PARTIAL drift: the dirty dot IS built (workspace-tab-dirty-{id}); the status cell still renders Done only |
| `F-TAB-02` | **already built** | `tiller/src/tab_machinery.rs:48` (`strip_overflows`) — + visible_tab_count + the strip overflow button; built by codex12 in P65 |
| `F-TAB-07` | **already built** | `tiller_ui/src/tab_bar.rs:309` (`New Chat`) — ACP agent picker: availability-gated, emits the chosen agent id; drawn tests cover gating |
| `F-TAB-08` | **already built** | `tiller_ui/src/tab_bar.rs:451` (`No supported agent found on PATH`) — render_chat_empty: 'Other agents...' plus the none-found state; drawn test |
| `F-TAB-09` | still absent | searched: Open File in the terminal-pane context menu — CLASS 3 adjacent: the pane menu still lacks it, but the row's conjunct 'no other open-file surface' is now FALSE — ctrl-o, the palette Open File and the tab-menu Open File all exist |
| `F-TAB-11` | still absent | searched: disabled/reason fields on terminal context items — TerminalContextItem has none; split_disabled_reason callers are tests-only |
| `F-TAB-12` | **already built** | `tiller/src/main.rs:5091` (`Move Earlier`) — + Move Later, with handlers and palette entries |
| `F-TAB-13` | **already built** | `tiller/src/main.rs:5123` (`Move to This Pane`) — + Move to Other Pane / Move to Pane N, with disabled reasons |
| `F-TAB-14` | **already built** | `tiller/src/main.rs:5226` (`begin_tab_rename`) — + commit_tab_rename + the drawn tab-rename-field |
| `F-TAB-15` | **already built** | `tiller_ui/src/tab_bar.rs:106` (`render_tab_context_menu`) — right-click opens the tab menu; the Close item is dispatched in main.rs |
| `F-TAB-16` | **already built** | `tiller/src/main.rs:5015` (`Close dirty tab?`) — window.prompt(Warning) on dirty close, plus bulk request_close_ids |
| `F-TAB-17` | **already built** | `tiller/src/main.rs:5076` (`Close Tabs to the Right`) — + Close Others, both with on_action handlers |
| `F-TAB-18` | still absent | searched: a tab-row on_drag — the only drag in the shell is DraggedPaneDivider |
| `F-TAB-21` | **already built** | `tiller/src/main.rs:4879` (`open_tab_menu`) — the Tab menu exists and renders |
| `F-TAB-23` | still absent | searched: SplitLeft / SplitUp / SplitPaneLeft / SplitPaneUp — zero matches in any crate |
| `F-TAB-24` | still absent | searched: a tab-drag cancel affordance — vacuous while no tab drag exists |
| `F-TAB-25` | still absent | searched: attach/detach pane — one comment word only |
| `F-TAB-27` | **already built** | `tiller/src/main.rs:3952` (`fn resume_chat`) — + ResumeChat palette entry + tab-menu item gated on retained chats; built by codex12 in P65 |
| `F-TAB-28` | still absent | searched: a ctrl-w chord bound in the shell — class 1 with a twist: CloseTab is bound to the literal chord 'cmd-w' in shell code; ctrl-w exists only in the tab_bar chord FIXTURE (strip bindings routed to codex12 per the fixture comment). The row's CMD-W defeats a cmd-search AND the shipped Linux chord is itself the old platform's |
| `F-CHAT-02` | still absent | searched: auth (case-insensitive) in chat.rs and tiller_acp — the only match is a fake-agent test script advertising authMethods:[] (line drifted from the row's 3974) |
| `F-CHAT-08` | **already built** | `tiller_ui/src/chat.rs:2996` (`stop-glyph`) — while streaming the send control becomes stop (on_click -> cancel_turn), comment cites D-CHAT-02; the row said 'no loading state, no stop state' |
| `F-CHAT-16` | still absent | searched: a search/filter input, 'Recommended', a no-match state in the model picker — zero matches in chat.rs/composer.rs |
| `F-CHAT-18` | still absent | searched: input/output/cache breakdown (cache_read, input_tokens, breakdown) — the popover has percent + used/size + an optional Cost line — no breakdown rows |
| `F-CHAT-21` | still absent | searched: an expand/collapse affordance on the Thought entry — drift: Entry::Thought now RENDERS (static italic text) — the display exists, the affordance does not |
| `F-CHAT-22` | still absent | searched: grouped / steps — zero matches |
| `F-CHAT-23` | still absent | searched: click/expand, output, diff/location links, Dismiss on ToolCall — Entry::ToolCall still carries {id,title,status}; the render is a static title+status card (drifted 1861 -> 1949) |
| `F-CHAT-24` | still absent | searched: a named Plan card — a /create-plan slash option exists (different surface); the Permission card is generic |
| `F-CHAT-25` | still absent | searched: text answer / cancel on the question card — the Permission card offers option buttons only |
| `F-CHAT-26` | still absent | searched: a pending-question bar — only retry_pending_send matches 'pending' — a different concept |
| `F-CHAT-27` | still absent | searched: an expired-question state — zero matches for 'expired' |
| `F-CHAT-28` | still absent | searched: subagent / task cards — zero matches |
| `F-CHAT-31` | still absent | searched: a chat diff preview — the diff_* colour hits in chat.rs are the ERROR card's styling — a bare 'diff' search would false-positive here |
| `F-CHAT-32` | still absent | searched: an edit summary / edited-files list — Follow Edited Files is the F-CHAT-14 toggle, not a summary |
| `F-CHAT-34` | still absent | searched: a chat history / past-chats menu — zero matches in chat.rs and main.rs |
| `F-CHAT-35` | still absent | searched: a "No past chats" empty state — zero matches |
| `F-CHG-01` | still absent | searched: a right-panel tab set beyond Files+Activity — Files and Activity only; Changes lives in a Diff tab, as the row records |
| `F-CHG-02` | still absent | searched: a workspace-scoped changes source after close-workspace — pass-13 EXERCISED evidence (socket transcript) — behavioral, outside static-census reach; re-read only |
| `F-CHG-03` | **already built** | `tiller_ui/src/right_panel.rs:679` (`Loading files`) — Refresh + 'Loading files...' + Retry states; refresh_started doc names both the single-flight guard and the rendered loading state |
| `F-CHG-05` | **already built** | `tiller_ui/src/right_panel.rs:555` (`on_file_key`) — down/up/space keyboard handling wired via .on_key_down |
| `F-CHG-11` | **already built** | `tiller_ui/src/changes.rs:930` (`section_action_button`) — 'Unstage all' + per-section actions |
| `F-CHG-13` | **already built** | `tiller_ui/src/changes.rs:1069` (`Open diff`) — OpenDiff action + label |
| `F-CHG-16` | **already built** | `tiller_ui/src/changes.rs:1082` (`Resolve in terminal`) — ResolveInTerminal action + label |
| `F-CHG-18` | **already built** | `tiller_ui/src/changes.rs:994` (`DiffDragPreview`) — a product on_drag on changes-file-row with a drag payload and preview entity — the row said 'no drag handlers in changes.rs' |
| `F-CHG-20` | still absent | searched: an Activity section / running-count element in changes.rs — one comment word only; the row is already builder-claim-confirmed by reading |
| `F-CHG-22` | **already built** | `tiller_ui/src/right_panel.rs:855` (`ActivityStatus::NeedsInput`) — all five statuses modelled AND mapped in render (activity-status-needs-input debug id) |
| `F-EDIT-01` | **already built** | `tiller_ui/src/file_view.rs:303` (`render_mode_switch`) — Preview/Code toggle, rendered |
| `F-EDIT-02` | **already built** | `tiller_ui/src/file_view.rs:326` (`render_markdown_toolbar`) — file-format-toolbar with Bold/Italic, rendered |
| `F-EDIT-03` | **already built** | `tiller_ui/src/file_view.rs:172` (`request_preview`) — + set_markdown_mode |
| `F-EDIT-04` | **already built** | `tiller/src/main.rs:118` (`WindowCommand::SaveFile`) — bound to ctrl-s in linux_window_shortcuts; row says CMD-S (class 1) |
| `F-EDIT-05` | **already built** | `tiller_ui/src/file_view.rs:18` (`Reload`) — Reload/Keep conflict banner with rendered conflict messages |
| `F-EDIT-06` | **already built** | `tiller_ui/src/file_view.rs:144` (`pub fn save`) — the save path; moved out of editor.rs since the row was written |
| `F-EDIT-07` | **already built** | `tiller_ui/src/editor.rs:138` (`fn from_path`) — Language::from_path applied on open; the doc comment names this row |
| `F-EDIT-08` | **already built** | `tiller/src/main.rs:2075` (`file_path_is_already_open`) — add_file_tab dedupes via this check before pushing |
| `F-EDIT-10` | **already built** | `tiller_ui/src/right_panel.rs:315` (`open_file_context_menu`) — FileContextMenu state + render |
| `F-EDIT-11` | **already built** | `tiller_ui/src/right_panel.rs:350` (`file-context-copy-path`) — Copy Path item in the file context menu |
| `F-EDIT-12` | still absent | searched: a file drag out of the right panel — the on_drag/on_drop block in right_panel.rs is the drawn DragFixture (tests-only) |
| `F-PER-07` | still absent | searched: an icon/name persistence roundtrip — cross-row dependency: rests on F-PRJ-12..16, all still absent — nothing to persist |
| `F-BRW-01` | still absent | searched: a webview / browser surface — TabKind::Browser and a palette 'New Browser' entry exist, but dispatch is the literal no-op `NewTabAction::NewBrowser => {}` and the palette explains 'Browser surfaces are unavailable on Linux' — a deliberate, now on-surface absence |
| `F-BRW-02` | still absent | searched: a webview / browser surface — same finding as F-BRW-01 |
| `F-BRW-03` | still absent | searched: a webview / browser surface — same finding as F-BRW-01 |
| `F-BRW-04` | still absent | searched: a webview / browser surface — same finding as F-BRW-01 |
| `F-BRW-05` | still absent | searched: a webview / browser surface — same finding as F-BRW-01 |
| `F-BRW-06` | still absent | searched: a webview / browser surface — same finding as F-BRW-01 |
| `F-BRW-07` | still absent | searched: a webview / browser surface — same finding as F-BRW-01 |
| `F-BRW-08` | still absent | searched: a webview / browser surface — same finding as F-BRW-01 |
| `F-BRW-09` | still absent | searched: a webview / browser surface — same finding as F-BRW-01 |
| `F-USE-01` | still absent | searched: a rendered refresh control / app callers of on_refresh — on_refresh callers: an example only; no refresh button renders — yet the module doc CLAIMS one (doc drift worth flagging) |
| `F-USE-02` | still absent | searched: tooltip in status_bar.rs — still zero case-insensitive matches |
| `F-USE-03` | still absent | searched: distinct rendering of UsageReason variants — segment_text maps every Unavailable(_) to the same em-dash (re-verified current) |
| `F-USE-06` | still absent | searched: app callers of should_notify / build_payload — re-counted: callers are tiller_activity itself and its tests; zero in the tiller crate (should_notify moved to tiller_activity/src/notification.rs — drift, not wiring) |
| `F-AUTO-06` | still absent | searched: a notification delivery path — cross-row dependency: cites F-USE-06's caller count as its evidence; re-verified, still zero — the inherited evidence happens to still be true |
| `F-SET-02` | **already built** | `tiller/src/main.rs:92` (`CloseSettingsSurface`) — action + escape binding + on_action handler all in tree; P58 builder claim confirmed at code level (runtime behavior stays pireview's) |
| `F-SET-04` | **already built** | `tiller_ui/src/settings.rs:277` (`resume_agent_sessions`) — field in the settings snapshot with default true; P58 claim confirmed at code level (DB half pending codex11) |
| `F-SET-08` | still absent | searched: a Copy-install-command control in settings — CLASS 3: the control is absent, but the row's theory — 'no install mechanism exists, so there was no real command to copy' — is FALSE: agent_skill_install_command (tiller_project/src/skill.rs) provides the exact npx command, with a test asserting it |
| `F-SET-09` | **already built** | `tiller_project/src/skill.rs:9` (`agent_skill_install_command`) — wrong-crate row (class 2): the skill code lives in tiller_project, not tiller_agents |
| `F-SET-10` | **already built** | `tiller_ui/src/settings.rs:2898` (`refresh_now_re_runs_provider_discovery`) — refresh re-runs ProviderAccountStates::discovered(); prefs reach the usage bar via StatusBar::apply_preferences called from main.rs |
| `F-SET-11` | still absent | searched: four distinct unavailable-reason renderings — the same em-dash collapse as F-USE-03 (status_bar segment_text) |
| `F-SET-12` | still absent | searched: cookie — zero matches in settings.rs |
| `F-SET-13` | still absent | searched: cookie — zero matches in settings.rs |
| `F-SET-14` | still absent | searched: functional add / re-auth / remove account actions — drift: an 'Add Account' button NOW RENDERS but its handler is the literal no-op |_, _, _| {} — a drawn dead control; the surface changed, the capability is still absent |
| `F-SET-15` | still absent | searched: a multi-account model — a code comment states 'this app has no isolated accounts' — single account by construction |
| `F-SET-16` | still absent | searched: a real search input + a live agents refresh — 'Search agents' is still a static text child and the Refresh handler still the literal no-op (both lines drifted: 1183 -> 1472, 1187 -> 1475) |
| `F-SET-17` | still absent | searched: an agent registry — zero matches |
| `F-SET-18` | still absent | searched: install/update/retry actions on agent rows — availability text only ('Not installed — install the {name} CLI to use it.') |
| `F-SET-21` | still absent | searched: a second file-icon set on Linux — cfg(not(macos)) pins SEGMENTED_FILE_ICONS to ["Material"] (the row line drifted 30 -> 38) |
| `F-SET-22` | still absent | searched: a clickable colour choice on agent-colour rows — controls::color_swatch(id, color, theme) takes no click handler — display-only, re-verified |
| `F-TERM-02` | still absent | searched: an empty-pane prompt — zero matches |
| `F-TERM-03` | still absent | searched: a surface rendering exit/signal status — all four exit_status sites classified: socket mapping x2, socket snapshot x2, and the dirty-close gate — none renders |
| `F-TERM-11` | still absent | searched: a no-worktree empty state — zero matches |
| `F-CORE-ACT-24` | still absent | searched: app callers of bootstrap::partition — re-counted zero in tiller/src (widened to bare `partition(`) |
| `F-CORE-ACT-25` | still absent | searched: app callers of the partition order — same re-count: zero |
| `F-CORE-ACT-26` | still absent | searched: app callers of ids_to_evict — re-counted zero (widened to `_evict`) |
| `F-CORE-DOM-03` | still absent | searched: callers of default_project_base — the re-export in tiller_project/src/lib.rs only |
| `F-CORE-WSP-04` | still absent | searched: LayoutCommand in tiller/src — zero — still a dead enum |
| `F-CORE-WSP-08` | still absent | searched: view_state in tiller_persistence + tiller/src — zero — never persisted |
| `F-CORE-FILE-03` | still absent | searched: app callers of shell_quote_path / terminal_file_drop — near-trap: shell_quote in tiller_agents is a DIFFERENT quoting with real callers; the write-a-path-to-a-pane quoting still has none |
| `F-CORE-FILE-06` | still absent | searched: FileSystemEventMonitor callers outside its crate — class-2 flavour: the monitor lives in tiller_markdown, not a file/editor crate; still zero callers outside it |
| `F-CORE-FILE-08` | still absent | searched: a per-file icon lookup (extension / icon_for) in icons.rs — zero |
| `F-CORE-SET-01` | still absent | searched: persistence of SettingsPolicy keys — 'resumeAgentSessions' does appear in main.rs — but in the report-values BTreeMap, exactly the report-only path the row records; zero in tiller_persistence |
| `F-CORE-USG-06` | still absent | searched: an injectable transport seam (a trait) in tiller_usage — drift, not built: curl is now factored into tiller_usage/src/http.rs (pub fn get/post) — still no trait/seam |
| `F-CORE-USG-07` | still absent | searched: an injectable transport for exercising the outcome mapping — same http.rs finding — still unexercisable without a seam |
| `F-CTRL-CLI-02` | still absent | searched: install / shim in tiller_control — zero; bare tillerctl still relies on PATH |
| `F-AGENT-API-01` | still absent | searched: summarizer / summarize in tiller_agents — zero — the settings summarizer trigger (the F-SET-05 surface) does not add the trait method |
| `F-AGENT-SAFE-01` | **already built** | `tiller_project/src/skill.rs:9` (`agent_skill_install_command`) — same finding as F-SET-09 (class 2: wrong crate) |
| `F-TERM-SCR-02` | still absent | searched: debounce / settle in tiller_terminal — zero |
| `F-TERM-PTY-05` | **already built** | `tiller/src/main.rs:2780` (`subscribe_terminal_activity`) — P50 wiring landed between the pass-11 snapshot (13:26Z) and this pin (19:30Z): activity events subscribed + start_process_signal_refresh, bound per terminal |
| `F-TERM-PTY-06` | still absent | searched: terminal_file_drop callers outside tiller_project — zero; the right_panel on_drop is the drawn DragFixture |
| `F-TERM-PTY-07` | still absent | searched: a stable-host / generation abstraction in tiller_terminal — zero |
| `F-TERM-PTY-08` | still absent | searched: PaneCache / pane_cache — zero |
| `F-TERM-SPLIT-01` | still absent | searched: a 160px minimum (MIN_PANE / px(160) — no minimum enforced; SEAM_WIDTH=1.0 unchanged |
| `F-TERM-UI-02` | still absent | searched: open_url / OpenUrl / Hyperlink / OSC 8 in tiller_terminal — zero — no URL router |
<!-- census:generated:end -->

## Honest remainder

- Everything here is **static reading** of the pinned snapshot — routes and renders were read at
  their dispatch/render sites, never exercised on a display. Behavioral rows (F-PRJ-01, F-PRJ-03,
  F-CHG-02) therefore keep their exercised evidence unchallenged.
- Four P58 rows (F-SET-02/04/10 built-confirmed, F-SET-08 class-3) carry builder claims whose
  *runtime* halves (focus traversal, DB round-trips) remain unverified — the code-level
  confirmation here does not stand in for the independent critic verification those rows demand.
- The partial rows (F-TAB-01 dirty-dot-only, F-SID-11 pill-without-comment, F-SET-14's no-op
  "Add Account", F-CHAT-21's rendered-but-uncollapsible Thought) are counted **still absent**
  on their clauses; the drift is recorded in their notes so pireview sees the surface moved.
- The tree moved while this census ran: HEAD advanced from `f0b44ef` to `c27c2c7` before the
  artefacts landed, and the **live** ledger is already down to 134 `FAILED — absent` rows —
  pireview retired rows mid-census. The pin is the input; a re-run against the live tree
  re-verifies every citation at its current line.
- Never-assigned counts drift with their inputs: the FABLE-08 brief said 81 (of 141, at
  brief-writing time), `assigned-but-absent.py` now says 73 (of the live 134), this census says
  74 (its brief corpus against the pinned 141). Same measure, three snapshots of a moving pair.
- The blind spot held less than feared: only 3 of the 39 already-built rows are never-assigned
  (`F-SID-19`, `F-AGENT-SAFE-01`, `F-TERM-PTY-05`). The staleness concentrates in **assigned**
  rows — 36 of the ~67 assigned `FAILED — absent` rows are already built. Assignment predicts
  staleness: assigned rows are the ones builders act on, and nothing re-reads the ledger after
  they ship. That is the process finding of this census.
