#!/usr/bin/env python3
"""FABLE-08 — the stale-FAILED census: which `FAILED — absent` rows are already built?

Mirror of FABLE-07 (adjudication-census.py). That pass asked, of rows marked built, "is this
reachable, or only a test?" — the false-PASSED direction. This one asks, of the 141 rows marked
`FAILED — absent`, "does it exist after all?" — the false-FAILED direction. A false FAILED sends
a builder to construct something that already exists; F-TAB-02 / F-TAB-27 were built by codex12
in P65 and still marked absent within the hour.

INPUT IS THE PIN, NOT THE LIVE LEDGER. The census parses
docs/linux-rewrite/pins/INVENTORY-LEDGER.FABLE-08.md — a verbatim copy of the working-tree
ledger at census time (2026-08-13T19:30:21Z, HEAD f0b44ef + 119 dirty files, snapshot
manifest-hash 6bdfb52eb0af0753). Five builders are editing; a moving input cannot be re-audited.
Statuses below describe that pinned tree; `pireview` adjudicates, this file only carries evidence.

WHAT THE STATUSES MEAN
- built:  the row's clause exists — file + needle cited, and this script RE-VERIFIES the needle
          on every run against `--tree`, printing the live line number. Needles, not line
          numbers, are the durable evidence: they survive builder drift, and a needle that stops
          matching makes the run fail loudly instead of citing a stale line.
- absent: searched in Linux/GPUI vocabulary (not the row's own words) and not found; the terms
          are disclosed per row. Absences are DISCLOSURE, not machine-verified assertions — many
          terms legitimately occur elsewhere (`auth` in a test fixture, `diff` in error-card
          styling), so "term found => row wrong" would itself be a false signal. Only built
          citations and the controls below are enforced. Rows proven absent by *exercising*
          (pass-13 display transcripts: F-PRJ-01/03, F-CHG-02) were re-read, not re-exercised —
          a static census cannot overturn behavioral evidence.

CONTROLS — the script refuses to print the table if any fails. When one fails, ask first
whether the census's claim is too broad, not whether the control is too strict.
1. Backend probe: the search backend must find a symbol known present
   (`agent_skill_install_command` in tiller_project/src/skill.rs) and must NOT find a fabricated
   one anywhere. Searches are pure-Python substring scans, deliberately independent of
   /usr/bin/grep (which on this machine is ugrep 7.5.0, not GNU grep).
2. Population: the pin must parse to exactly 141 `FAILED — absent` rows and the census table
   must cover exactly that set — no silent gaps, no strays.
3. Known-stale controls: the six findings known before this census (F-EDIT-04, F-EDIT-06,
   F-EDIT-08, F-SET-09 + F-AGENT-SAFE-01, F-TAB-02, F-TAB-27) must come out `built` with
   verifying citations. A census that loses a known-true positive is broken, not strict.
4. Citation verify: every `built` needle must be found in its cited file under `--tree`.

USAGE
  python3 Scripts/stale-failed-census.py                 # verify against rust/crates in this repo
  python3 Scripts/stale-failed-census.py --tree PATH     # verify against another tree (e.g. the pinned snapshot)
  python3 Scripts/stale-failed-census.py --write         # regenerate the table in docs/linux-rewrite/STALE-FAILED-CENSUS.md
"""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
PIN = REPO / "docs/linux-rewrite/pins/INVENTORY-LEDGER.FABLE-08.md"
DOC = REPO / "docs/linux-rewrite/STALE-FAILED-CENSUS.md"
TASKS = REPO / "docs/linux-rewrite/tasks"
EXPECTED_ROWS = 141

ROW = re.compile(r"^\|\s*`(F-[A-Z0-9-]+)`\s*\|\s*([^|]+?)\s*\|\s*(.*?)\s*\|\s*([^|]*?)\s*\|\s*$", re.M)

# Same exclusions as assigned-but-absent.py: critic/census briefs are not assignments,
# and the two meta briefs quote row ids without assigning them.
META_BRIEFS = {
    "P68-the-editor-cluster-and-three-rows-that-lie.md",
    "P64-the-diff-surface.md",
}

PROBE_PRESENT_FILE = "tiller_project/src/skill.rs"
PROBE_PRESENT_NEEDLE = "agent_skill_install_command"
PROBE_FABRICATED = "zzz_symbol_that_must_not_exist_zzz"

KNOWN_STALE = {
    "F-EDIT-04", "F-EDIT-06", "F-EDIT-08",
    "F-SET-09", "F-AGENT-SAFE-01",   # one finding, two rows
    "F-TAB-02", "F-TAB-27",
}

# The three vocabulary classes that make a genuinely-built row look absent. Class 3 is the
# find of this pass: the row states a THEORY of why the thing is missing ("no error surface
# exists"), and searching in the theory's terms confirms the theory vacuously. Search for the
# capability, never for the row's diagnosis.
VOCAB_OLD_PLATFORM = {"F-EDIT-04", "F-SID-19", "F-TAB-28", "F-WIN-01"}          # CMD-S/T/W/, vs ctrl-*
VOCAB_WRONG_CRATE = {"F-SET-09", "F-AGENT-SAFE-01", "F-CORE-FILE-06"}           # code lives in another crate
VOCAB_SYMPTOM_AS_CAUSE = {"F-PRJ-04", "F-SET-08", "F-TAB-09"}                   # the row's causal theory is false
CROSS_ROW_EVIDENCE = {"F-AUTO-06", "F-PER-07"}  # evidence cites another row's finding; stales when it does

B, A = "built", "absent"

# id -> (status, cited file under --tree (built only), needle (built) / searched terms (absent), note)
CENSUS: dict[str, tuple[str, str, str, str]] = {
    # ---- window ----
    "F-WIN-01": (A, "", "'ctrl-,' / comma chord / a settings entry in linux_window_shortcuts", "the 5 shell chords have no Settings entry; row's CMD-, is class 1"),
    "F-WIN-07": (A, "", "History menu / menu_bar / history_menu", "a drawn test asserts the shell has NO in-window menu bar (resting_frame_has_context_menu_surfaces_but_no_in_window_menu_bar) — the clause presupposes a menu bar that is absent by design"),
    "F-WIN-10": (A, "", "toast", "only a theme radius token and a conformance comment"),
    # ---- sidebar ----
    "F-SID-06": (A, "", "status dot on project rows (render_row / RowKind)", "dot computed for RowKind::Worktree only — the project-badge half is still unbuilt"),
    "F-SID-07": (B, "tiller_ui/src/sidebar.rs", "render_project_settings", "Project Settings context-menu item opens the settings card"),
    "F-SID-08": (B, "tiller_ui/src/sidebar.rs", "Initialize Git repository", "menu item with AlreadyGitProject disabled reason, plus a test"),
    "F-SID-09": (B, "tiller_ui/src/sidebar.rs", "Show in File Manager", "menu item; the palette twin is labelled Reveal in File Manager"),
    "F-SID-11": (A, "", "the worktree-card comment line", "PARTIAL drift: branch-as-title, Primary pill and the path context line are built; the comment line is not"),
    "F-SID-15": (A, "", "Remove Worktree in the worktree context menu", "the menu ends at New Chat (drift: it now also carries Set/Unset Primary); removal is still only the hover x"),
    "F-SID-16": (A, "", "sidebar drag / reorder handlers", "a no-drag comment and a no-reorder test pin the design"),
    "F-SID-17": (A, "", "cross-project drag", "same evidence as F-SID-16"),
    "F-SID-18": (A, "", '"No Terminals" empty-state string', "zero matches"),
    "F-SID-19": (B, "tiller/src/main.rs", "WindowCommand::NewTerminalTab", "bound to ctrl-t; row says CMD-T (class 1)"),
    # ---- projects ----
    "F-PRJ-01": (A, "", "clone-from-URL / create-new forms behind the + control", "only the ashpd folder picker; pass-13 exercised on the display — not re-exercised here"),
    "F-PRJ-03": (A, "", "an Initialize-Git / Add-without-Git / Cancel prompt on non-git add", "pass-13 exercised; still no prompt code"),
    "F-PRJ-04": (A, "", "the insertion-failure route to the sidebar notice", "CLASS 3: the row says 'no error surface' but the surface EXISTS — Sidebar.notice + set_notice + the sidebar-notice render, and main.rs already has three set_notice call sites; only the insertion path (eprintln '[projects] ...') bypasses it. The fix is a call-site change, not a new surface"),
    "F-PRJ-05": (A, "", "clone form / clone_url", "no clone UI anywhere"),
    "F-PRJ-06": (A, "", "clone-form guards", "no form to carry them"),
    "F-PRJ-07": (A, "", "clone failure/retry surface", "no form"),
    "F-PRJ-08": (A, "", 'a create-new-project form / "New Project"', "the Add flow only browses folders"),
    "F-PRJ-09": (A, "", "create-form guards", "no form"),
    "F-PRJ-10": (A, "", "create-form failure surface", "no form"),
    "F-PRJ-11": (A, "", "a trash control in the project-settings sheet", "the sheet shows name/path/repository/Close/id only"),
    "F-PRJ-12": (A, "", "repository-type switch / display-name edit", "the sheet is explicitly read-only until a persistence contract exists"),
    "F-PRJ-13": (A, "", "icon colour/reset controls", "no icon UI in the sheet"),
    "F-PRJ-14": (A, "", "avatar / GitHub / PNG / favicon controls", "none"),
    "F-PRJ-15": (A, "", "an icon grid / picker", "the sfsymbol renderer exists for in-app glyphs; there is no picker"),
    "F-PRJ-16": (A, "", "emoji picker / emoji icon input", "none"),
    "F-PRJ-17": (A, "", "default-worktree-base options", "derive_worktree_path always derives the parent"),
    "F-PRJ-18": (A, "", "a custom worktree-location control", "the location is derived, not choosable"),
    # ---- tabs ----
    "F-TAB-01": (A, "", "a per-status tab cell (Running/Error/Idle)", "PARTIAL drift: the dirty dot IS built (workspace-tab-dirty-{id}); the status cell still renders Done only"),
    "F-TAB-02": (B, "tiller/src/tab_machinery.rs", "strip_overflows", "+ visible_tab_count + the strip overflow button; built by codex12 in P65"),
    "F-TAB-07": (B, "tiller_ui/src/tab_bar.rs", "New Chat", "ACP agent picker: availability-gated, emits the chosen agent id; drawn tests cover gating"),
    "F-TAB-08": (B, "tiller_ui/src/tab_bar.rs", "No supported agent found on PATH", "render_chat_empty: 'Other agents...' plus the none-found state; drawn test"),
    "F-TAB-09": (A, "", "Open File in the terminal-pane context menu", "CLASS 3 adjacent: the pane menu still lacks it, but the row's conjunct 'no other open-file surface' is now FALSE — ctrl-o, the palette Open File and the tab-menu Open File all exist"),
    "F-TAB-11": (A, "", "disabled/reason fields on terminal context items", "TerminalContextItem has none; split_disabled_reason callers are tests-only"),
    "F-TAB-12": (B, "tiller/src/main.rs", "Move Earlier", "+ Move Later, with handlers and palette entries"),
    "F-TAB-13": (B, "tiller/src/main.rs", "Move to This Pane", "+ Move to Other Pane / Move to Pane N, with disabled reasons"),
    "F-TAB-14": (B, "tiller/src/main.rs", "begin_tab_rename", "+ commit_tab_rename + the drawn tab-rename-field"),
    "F-TAB-15": (B, "tiller_ui/src/tab_bar.rs", "render_tab_context_menu", "right-click opens the tab menu; the Close item is dispatched in main.rs"),
    "F-TAB-16": (B, "tiller/src/main.rs", "Close dirty tab?", "window.prompt(Warning) on dirty close, plus bulk request_close_ids"),
    "F-TAB-17": (B, "tiller/src/main.rs", "Close Tabs to the Right", "+ Close Others, both with on_action handlers"),
    "F-TAB-18": (A, "", "a tab-row on_drag", "the only drag in the shell is DraggedPaneDivider"),
    "F-TAB-21": (B, "tiller/src/main.rs", "open_tab_menu", "the Tab menu exists and renders"),
    "F-TAB-23": (A, "", "SplitLeft / SplitUp / SplitPaneLeft / SplitPaneUp", "zero matches in any crate"),
    "F-TAB-24": (A, "", "a tab-drag cancel affordance", "vacuous while no tab drag exists"),
    "F-TAB-25": (A, "", "attach/detach pane", "one comment word only"),
    "F-TAB-27": (B, "tiller/src/main.rs", "fn resume_chat", "+ ResumeChat palette entry + tab-menu item gated on retained chats; built by codex12 in P65"),
    "F-TAB-28": (A, "", "a ctrl-w chord bound in the shell", "class 1 with a twist: CloseTab is bound to the literal chord 'cmd-w' in shell code; ctrl-w exists only in the tab_bar chord FIXTURE (strip bindings routed to codex12 per the fixture comment). The row's CMD-W defeats a cmd-search AND the shipped Linux chord is itself the old platform's"),
    # ---- chat ----
    "F-CHAT-02": (A, "", "auth (case-insensitive) in chat.rs and tiller_acp", "the only match is a fake-agent test script advertising authMethods:[] (line drifted from the row's 3974)"),
    "F-CHAT-08": (B, "tiller_ui/src/chat.rs", "stop-glyph", "while streaming the send control becomes stop (on_click -> cancel_turn), comment cites D-CHAT-02; the row said 'no loading state, no stop state'"),
    "F-CHAT-16": (A, "", "a search/filter input, 'Recommended', a no-match state in the model picker", "zero matches in chat.rs/composer.rs"),
    "F-CHAT-18": (A, "", "input/output/cache breakdown (cache_read, input_tokens, breakdown)", "the popover has percent + used/size + an optional Cost line — no breakdown rows"),
    "F-CHAT-21": (A, "", "an expand/collapse affordance on the Thought entry", "drift: Entry::Thought now RENDERS (static italic text) — the display exists, the affordance does not"),
    "F-CHAT-22": (A, "", "grouped / steps", "zero matches"),
    "F-CHAT-23": (A, "", "click/expand, output, diff/location links, Dismiss on ToolCall", "Entry::ToolCall still carries {id,title,status}; the render is a static title+status card (drifted 1861 -> 1949)"),
    "F-CHAT-24": (A, "", "a named Plan card", "a /create-plan slash option exists (different surface); the Permission card is generic"),
    "F-CHAT-25": (A, "", "text answer / cancel on the question card", "the Permission card offers option buttons only"),
    "F-CHAT-26": (A, "", "a pending-question bar", "only retry_pending_send matches 'pending' — a different concept"),
    "F-CHAT-27": (A, "", "an expired-question state", "zero matches for 'expired'"),
    "F-CHAT-28": (A, "", "subagent / task cards", "zero matches"),
    "F-CHAT-31": (A, "", "a chat diff preview", "the diff_* colour hits in chat.rs are the ERROR card's styling — a bare 'diff' search would false-positive here"),
    "F-CHAT-32": (A, "", "an edit summary / edited-files list", "Follow Edited Files is the F-CHAT-14 toggle, not a summary"),
    "F-CHAT-34": (A, "", "a chat history / past-chats menu", "zero matches in chat.rs and main.rs"),
    "F-CHAT-35": (A, "", 'a "No past chats" empty state', "zero matches"),
    # ---- changes / right panel ----
    "F-CHG-01": (A, "", "a right-panel tab set beyond Files+Activity", "Files and Activity only; Changes lives in a Diff tab, as the row records"),
    "F-CHG-02": (A, "", "a workspace-scoped changes source after close-workspace", "pass-13 EXERCISED evidence (socket transcript) — behavioral, outside static-census reach; re-read only"),
    "F-CHG-03": (B, "tiller_ui/src/right_panel.rs", "Loading files", "Refresh + 'Loading files...' + Retry states; refresh_started doc names both the single-flight guard and the rendered loading state"),
    "F-CHG-05": (B, "tiller_ui/src/right_panel.rs", "on_file_key", "down/up/space keyboard handling wired via .on_key_down"),
    "F-CHG-11": (B, "tiller_ui/src/changes.rs", "section_action_button", "'Unstage all' + per-section actions"),
    "F-CHG-13": (B, "tiller_ui/src/changes.rs", "Open diff", "OpenDiff action + label"),
    "F-CHG-16": (B, "tiller_ui/src/changes.rs", "Resolve in terminal", "ResolveInTerminal action + label"),
    "F-CHG-18": (B, "tiller_ui/src/changes.rs", "DiffDragPreview", "a product on_drag on changes-file-row with a drag payload and preview entity — the row said 'no drag handlers in changes.rs'"),
    "F-CHG-20": (A, "", "an Activity section / running-count element in changes.rs", "one comment word only; the row is already builder-claim-confirmed by reading"),
    "F-CHG-22": (B, "tiller_ui/src/right_panel.rs", "ActivityStatus::NeedsInput", "all five statuses modelled AND mapped in render (activity-status-needs-input debug id)"),
    # ---- editor / files ----
    "F-EDIT-01": (B, "tiller_ui/src/file_view.rs", "render_mode_switch", "Preview/Code toggle, rendered"),
    "F-EDIT-02": (B, "tiller_ui/src/file_view.rs", "render_markdown_toolbar", "file-format-toolbar with Bold/Italic, rendered"),
    "F-EDIT-03": (B, "tiller_ui/src/file_view.rs", "request_preview", "+ set_markdown_mode"),
    "F-EDIT-04": (B, "tiller/src/main.rs", "WindowCommand::SaveFile", "bound to ctrl-s in linux_window_shortcuts; row says CMD-S (class 1)"),
    "F-EDIT-05": (B, "tiller_ui/src/file_view.rs", "Reload", "Reload/Keep conflict banner with rendered conflict messages"),
    "F-EDIT-06": (B, "tiller_ui/src/file_view.rs", "pub fn save", "the save path; moved out of editor.rs since the row was written"),
    "F-EDIT-07": (B, "tiller_ui/src/editor.rs", "fn from_path", "Language::from_path applied on open; the doc comment names this row"),
    "F-EDIT-08": (B, "tiller/src/main.rs", "file_path_is_already_open", "add_file_tab dedupes via this check before pushing"),
    "F-EDIT-10": (B, "tiller_ui/src/right_panel.rs", "open_file_context_menu", "FileContextMenu state + render"),
    "F-EDIT-11": (B, "tiller_ui/src/right_panel.rs", "file-context-copy-path", "Copy Path item in the file context menu"),
    "F-EDIT-12": (A, "", "a file drag out of the right panel", "the on_drag/on_drop block in right_panel.rs is the drawn DragFixture (tests-only)"),
    # ---- persistence ----
    "F-PER-07": (A, "", "an icon/name persistence roundtrip", "cross-row dependency: rests on F-PRJ-12..16, all still absent — nothing to persist"),
    # ---- browser ----
    "F-BRW-01": (A, "", "a webview / browser surface", "TabKind::Browser and a palette 'New Browser' entry exist, but dispatch is the literal no-op `NewTabAction::NewBrowser => {}` and the palette explains 'Browser surfaces are unavailable on Linux' — a deliberate, now on-surface absence"),
    "F-BRW-02": (A, "", "a webview / browser surface", "same finding as F-BRW-01"),
    "F-BRW-03": (A, "", "a webview / browser surface", "same finding as F-BRW-01"),
    "F-BRW-04": (A, "", "a webview / browser surface", "same finding as F-BRW-01"),
    "F-BRW-05": (A, "", "a webview / browser surface", "same finding as F-BRW-01"),
    "F-BRW-06": (A, "", "a webview / browser surface", "same finding as F-BRW-01"),
    "F-BRW-07": (A, "", "a webview / browser surface", "same finding as F-BRW-01"),
    "F-BRW-08": (A, "", "a webview / browser surface", "same finding as F-BRW-01"),
    "F-BRW-09": (A, "", "a webview / browser surface", "same finding as F-BRW-01"),
    # ---- usage bar ----
    "F-USE-01": (A, "", "a rendered refresh control / app callers of on_refresh", "on_refresh callers: an example only; no refresh button renders — yet the module doc CLAIMS one (doc drift worth flagging)"),
    "F-USE-02": (A, "", "tooltip in status_bar.rs", "still zero case-insensitive matches"),
    "F-USE-03": (A, "", "distinct rendering of UsageReason variants", "segment_text maps every Unavailable(_) to the same em-dash (re-verified current)"),
    "F-USE-06": (A, "", "app callers of should_notify / build_payload", "re-counted: callers are tiller_activity itself and its tests; zero in the tiller crate (should_notify moved to tiller_activity/src/notification.rs — drift, not wiring)"),
    "F-AUTO-06": (A, "", "a notification delivery path", "cross-row dependency: cites F-USE-06's caller count as its evidence; re-verified, still zero — the inherited evidence happens to still be true"),
    # ---- settings ----
    "F-SET-02": (B, "tiller/src/main.rs", "CloseSettingsSurface", "action + escape binding + on_action handler all in tree; P58 builder claim confirmed at code level (runtime behavior stays pireview's)"),
    "F-SET-04": (B, "tiller_ui/src/settings.rs", "resume_agent_sessions", "field in the settings snapshot with default true; P58 claim confirmed at code level (DB half pending codex11)"),
    "F-SET-08": (A, "", "a Copy-install-command control in settings", "CLASS 3: the control is absent, but the row's theory — 'no install mechanism exists, so there was no real command to copy' — is FALSE: agent_skill_install_command (tiller_project/src/skill.rs) provides the exact npx command, with a test asserting it"),
    "F-SET-09": (B, "tiller_project/src/skill.rs", "agent_skill_install_command", "wrong-crate row (class 2): the skill code lives in tiller_project, not tiller_agents"),
    "F-SET-10": (B, "tiller_ui/src/settings.rs", "refresh_now_re_runs_provider_discovery", "refresh re-runs ProviderAccountStates::discovered(); prefs reach the usage bar via StatusBar::apply_preferences called from main.rs"),
    "F-SET-11": (A, "", "four distinct unavailable-reason renderings", "the same em-dash collapse as F-USE-03 (status_bar segment_text)"),
    "F-SET-12": (A, "", "cookie", "zero matches in settings.rs"),
    "F-SET-13": (A, "", "cookie", "zero matches in settings.rs"),
    "F-SET-14": (A, "", "functional add / re-auth / remove account actions", "drift: an 'Add Account' button NOW RENDERS but its handler is the literal no-op |_, _, _| {} — a drawn dead control; the surface changed, the capability is still absent"),
    "F-SET-15": (A, "", "a multi-account model", "a code comment states 'this app has no isolated accounts' — single account by construction"),
    "F-SET-16": (A, "", "a real search input + a live agents refresh", "'Search agents' is still a static text child and the Refresh handler still the literal no-op (both lines drifted: 1183 -> 1472, 1187 -> 1475)"),
    "F-SET-17": (A, "", "an agent registry", "zero matches"),
    "F-SET-18": (A, "", "install/update/retry actions on agent rows", "availability text only ('Not installed — install the {name} CLI to use it.')"),
    "F-SET-21": (A, "", "a second file-icon set on Linux", 'cfg(not(macos)) pins SEGMENTED_FILE_ICONS to ["Material"] (the row line drifted 30 -> 38)'),
    "F-SET-22": (A, "", "a clickable colour choice on agent-colour rows", "controls::color_swatch(id, color, theme) takes no click handler — display-only, re-verified"),
    # ---- terminal (app tier) ----
    "F-TERM-02": (A, "", "an empty-pane prompt", "zero matches"),
    "F-TERM-03": (A, "", "a surface rendering exit/signal status", "all four exit_status sites classified: socket mapping x2, socket snapshot x2, and the dirty-close gate — none renders"),
    "F-TERM-11": (A, "", "a no-worktree empty state", "zero matches"),
    # ---- package tier ----
    "F-CORE-ACT-24": (A, "", "app callers of bootstrap::partition", "re-counted zero in tiller/src (widened to bare `partition(`)"),
    "F-CORE-ACT-25": (A, "", "app callers of the partition order", "same re-count: zero"),
    "F-CORE-ACT-26": (A, "", "app callers of ids_to_evict", "re-counted zero (widened to `_evict`)"),
    "F-CORE-DOM-03": (A, "", "callers of default_project_base", "the re-export in tiller_project/src/lib.rs only"),
    "F-CORE-WSP-04": (A, "", "LayoutCommand in tiller/src", "zero — still a dead enum"),
    "F-CORE-WSP-08": (A, "", "view_state in tiller_persistence + tiller/src", "zero — never persisted"),
    "F-CORE-FILE-03": (A, "", "app callers of shell_quote_path / terminal_file_drop", "near-trap: shell_quote in tiller_agents is a DIFFERENT quoting with real callers; the write-a-path-to-a-pane quoting still has none"),
    "F-CORE-FILE-06": (A, "", "FileSystemEventMonitor callers outside its crate", "class-2 flavour: the monitor lives in tiller_markdown, not a file/editor crate; still zero callers outside it"),
    "F-CORE-FILE-08": (A, "", "a per-file icon lookup (extension / icon_for) in icons.rs", "zero"),
    "F-CORE-SET-01": (A, "", "persistence of SettingsPolicy keys", "'resumeAgentSessions' does appear in main.rs — but in the report-values BTreeMap, exactly the report-only path the row records; zero in tiller_persistence"),
    "F-CORE-USG-06": (A, "", "an injectable transport seam (a trait) in tiller_usage", "drift, not built: curl is now factored into tiller_usage/src/http.rs (pub fn get/post) — still no trait/seam"),
    "F-CORE-USG-07": (A, "", "an injectable transport for exercising the outcome mapping", "same http.rs finding — still unexercisable without a seam"),
    "F-CTRL-CLI-02": (A, "", "install / shim in tiller_control", "zero; bare tillerctl still relies on PATH"),
    "F-AGENT-API-01": (A, "", "summarizer / summarize in tiller_agents", "zero — the settings summarizer trigger (the F-SET-05 surface) does not add the trait method"),
    "F-AGENT-SAFE-01": (B, "tiller_project/src/skill.rs", "agent_skill_install_command", "same finding as F-SET-09 (class 2: wrong crate)"),
    "F-TERM-SCR-02": (A, "", "debounce / settle in tiller_terminal", "zero"),
    "F-TERM-PTY-05": (B, "tiller/src/main.rs", "subscribe_terminal_activity", "P50 wiring landed between the pass-11 snapshot (13:26Z) and this pin (19:30Z): activity events subscribed + start_process_signal_refresh, bound per terminal"),
    "F-TERM-PTY-06": (A, "", "terminal_file_drop callers outside tiller_project", "zero; the right_panel on_drop is the drawn DragFixture"),
    "F-TERM-PTY-07": (A, "", "a stable-host / generation abstraction in tiller_terminal", "zero"),
    "F-TERM-PTY-08": (A, "", "PaneCache / pane_cache", "zero"),
    "F-TERM-SPLIT-01": (A, "", "a 160px minimum (MIN_PANE / px(160)", "no minimum enforced; SEAM_WIDTH=1.0 unchanged"),
    "F-TERM-UI-02": (A, "", "open_url / OpenUrl / Hyperlink / OSC 8 in tiller_terminal", "zero — no URL router"),
}


def fail(message: str) -> None:
    print(f"CONTROL FAILED: {message}", file=sys.stderr)
    print("Refusing to print the census.", file=sys.stderr)
    sys.exit(1)


def find_line(tree: Path, rel: str, needle: str) -> int | None:
    path = tree / rel
    if not path.is_file():
        return None
    for number, line in enumerate(path.read_text(errors="replace").splitlines(), 1):
        if needle in line:
            return number
    return None


def needle_anywhere(tree: Path, needle: str) -> bool:
    for path in tree.rglob("*.rs"):
        try:
            if needle in path.read_text(errors="replace"):
                return True
        except OSError:
            continue
    return False


def pin_rows() -> list[str]:
    if not PIN.is_file():
        fail(f"pin not found: {PIN}")
    return [m.group(1) for m in ROW.finditer(PIN.read_text()) if m.group(2) == "FAILED — absent"]


def assigned_rows() -> set[str]:
    # Same corpus as assigned-but-absent.py: builder briefs are P*.md only (D*/COSMIC-* are
    # design briefs, CRITIC-*/FABLE-* are not assignments), minus the two meta briefs that
    # quote row ids without assigning them. Note that script reads the LIVE ledger (134
    # FAILED — absent rows at the time of this census) while this one reads the PIN (141):
    # pireview retired rows while the census ran — the staleness this census measures is
    # bidirectional.
    mentioned: set[str] = set()
    if not TASKS.is_dir():
        return mentioned
    for brief in TASKS.glob("P*.md"):
        if brief.name in META_BRIEFS:
            continue
        mentioned.update(re.findall(r"\bF-[A-Z0-9]+(?:-[A-Z0-9]+)*\b", brief.read_text()))
    return mentioned


def run(tree: Path, write: bool) -> None:
    # Control 1 — backend probe.
    present_line = find_line(tree, PROBE_PRESENT_FILE, PROBE_PRESENT_NEEDLE)
    if present_line is None:
        fail(f"backend probe: {PROBE_PRESENT_NEEDLE!r} not found in {PROBE_PRESENT_FILE} under {tree}")
    if needle_anywhere(tree, PROBE_FABRICATED):
        fail("backend probe: the fabricated symbol was 'found' — the backend cannot be trusted")

    # Control 2 — population.
    rows = pin_rows()
    if len(rows) != EXPECTED_ROWS:
        fail(f"pin parsed to {len(rows)} FAILED — absent rows, expected {EXPECTED_ROWS}")
    missing = [r for r in rows if r not in CENSUS]
    strays = [r for r in CENSUS if r not in rows]
    if missing or strays:
        fail(f"census/pin mismatch — missing: {missing[:5]} strays: {strays[:5]}")

    # Controls 3 + 4 — known-stale rows and every built citation must verify.
    lines: dict[str, int] = {}
    for row_id, (status, rel, needle, _) in sorted(CENSUS.items()):
        if status != B:
            continue
        line = find_line(tree, rel, needle)
        if line is None:
            fail(f"citation for {row_id} no longer verifies: {needle!r} not in {rel}")
        lines[row_id] = line
    for row_id in KNOWN_STALE:
        if CENSUS[row_id][0] != B:
            fail(f"known-stale control {row_id} did not come out built")

    built = [r for r in rows if CENSUS[r][0] == B]
    absent = [r for r in rows if CENSUS[r][0] == A]
    assigned = assigned_rows()
    never = [r for r in rows if r not in assigned]
    built_never = [r for r in built if r not in assigned]

    def evidence(row_id: str) -> str:
        status, rel, needle_or_terms, note = CENSUS[row_id]
        if status == B:
            return f"`{rel}:{lines[row_id]}` (`{needle_or_terms}`) — {note}"
        return f"searched: {needle_or_terms} — {note}"

    def status_label(row_id: str) -> str:
        return "**already built**" if CENSUS[row_id][0] == B else "still absent"

    table = ["| row | census | evidence (verified against the tree at run time) |", "|---|---|---|"]
    table += [f"| `{r}` | {status_label(r)} | {evidence(r)} |" for r in rows]

    summary = [
        f"- Rows: {len(rows)} — **already built: {len(built)}**, still absent: {len(absent)}, cannot tell: 0.",
        f"- Never-assigned rows (no builder brief names them, same exclusions as assigned-but-absent.py): "
        f"{len(never)} of {len(rows)} — all settled; {len(built_never)} of them are already built: "
        + (", ".join(f"`{r}`" for r in built_never) if built_never else "none")
        + ".",
        f"- Wording traps: class 1 old-platform vocabulary {sorted(VOCAB_OLD_PLATFORM)}; "
        f"class 2 wrong-crate {sorted(VOCAB_WRONG_CRATE)}; "
        f"class 3 symptom-as-cause (the row's causal theory is false) {sorted(VOCAB_SYMPTOM_AS_CAUSE)}; "
        f"cross-row evidence (inherits another row's staleness) {sorted(CROSS_ROW_EVIDENCE)}.",
        f"- Controls: backend probe found `{PROBE_PRESENT_NEEDLE}` at {PROBE_PRESENT_FILE}:{present_line} "
        f"and did not find the fabricated symbol; population {len(rows)}/{EXPECTED_ROWS}; "
        f"all {len(KNOWN_STALE)} known-stale control rows (six findings) came out built; all {len(built)} citations verified.",
    ]

    print("\n".join(summary))
    print()
    print("\n".join(table))

    if write:
        begin, end = "<!-- census:generated:begin -->", "<!-- census:generated:end -->"
        doc = DOC.read_text()
        if begin not in doc or end not in doc:
            fail(f"{DOC} is missing the generated-section markers")
        generated = begin + "\n" + "\n".join(summary) + "\n\n" + "\n".join(table) + "\n" + end
        DOC.write_text(doc[: doc.index(begin)] + generated + doc[doc.index(end) + len(end):])
        print(f"\nwrote {DOC}")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--tree", type=Path, default=REPO / "rust/crates", help="crates tree to verify citations against")
    parser.add_argument("--write", action="store_true", help="regenerate the table in STALE-FAILED-CENSUS.md")
    args = parser.parse_args()
    run(args.tree.resolve(), args.write)


if __name__ == "__main__":
    main()
