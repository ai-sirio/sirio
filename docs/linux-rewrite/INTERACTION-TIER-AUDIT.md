# Interaction-tier audit — GPUI headless view tests

Updated 13 August 2026 after the `VisualTestContext` discovery. This is a status correction,
not a claim that every listed entry has already been independently exercised.

The historical critic report is intentionally not rewritten: its display-blocked verdicts were
correct for the evidence available at that time. This audit supersedes only the current tracking
reason for the interaction halves.

## The boundary

`gpui::TestAppContext` plus `VisualTestContext` draws the real element tree without a display. A
drawn frame has a debug-bounds map, and `VisualTestContext` can dispatch real clicks, mouse
movement, mouse down/up sequences, and keystrokes through GPUI's normal event path. Therefore an
entry about an element being present, being hit-testable, or changing application state after a
click/key is **headlessly testable**. It is not display-blocked.

The harness does not establish pixel colour, font rendering, visual spacing, or equivalence to
waku. Those halves remain **display-blocked**. A mixed entry must be split: reclaim the behaviour
half and retain the appearance half as a debt. Reclassification is not a pass; each reclaimed
entry still needs an evidence test that draws the frame, locates the element, drives it, and
asserts the resulting state.

The harness has no drag convenience method, but a drag is expressible as a real
`simulate_mouse_down` → `simulate_mouse_move(..., Some(button), ...)` → `simulate_mouse_up`
sequence. A double-click is expressible through `simulate_event` with the GPUI mouse event's
`click_count`. Key chords are directly supported by `simulate_keystrokes` (for example,
`"ctrl-tab"`, `"cmd-1"`, and `"cmd-w"`). These are interaction evidence, not pixel evidence.

## Reclassification

| Entry | Current verdict | Evidence / boundary |
|---|---|---|
| `F-EDIT-01` | **BUILDER-CLAIMED behaviour; appearance owed** | Drawn-frame tests click Code and Preview, assert the corresponding source/rendered subtree, and cover the large-file manual-preview unlock. Typography inside either mode remains display-only. |
| `F-EDIT-09` | **BUILDER-CLAIMED interaction half** | `double_clicking_a_drawn_file_row_emits_open_file` finds the real row, sends a real double-click, and asserts the shell-facing path event. The editor-tab route is shell-owned and routed to codex12. |
| `F-EDIT-12` | **FAILED/ABSENT behaviour** | GPUI can express mouse down/move/up, but neither the explorer nor a pane implements the requested file drag/drop contract. There is no interaction evidence to claim. |
| `F-CHG-03` | **PARTIAL builder evidence** | A drawn directory row expands through the real dispatch path; refresh/loading/error presentation has no drawn-frame evidence here. |
| `F-CHG-04` | **PARTIAL builder evidence** | `clicking_a_drawn_directory_row_expands_and_collapses_it` proves directory toggling and child layout. File selection is not implemented/proven. |
| `F-CHG-05` | **FAILED/ABSENT behaviour** | The Files tree has no arrow/Space/Return key handlers. Key dispatch is possible, but there is no behavior to exercise. |
| `F-CHG-09` | **BUILDER-CLAIMED behaviour; appearance owed** | The existing drawn `.git` removal test finds the error and Retry bounds, clicks Retry, and proves recovery. Error typography/colour remains display-only. |
| `F-CHG-10` | **BUILDER-CLAIMED behaviour; appearance owed** | Drawn Stage and Unstage controls mutate a real checkout and move the row between sections. |
| `F-CHG-11` | **PARTIAL builder evidence** | Drawn Stage all and Discard all (including confirmation and untracked retention) are covered; Unstage all is not. |
| `F-CHG-12` | **BUILDER-CLAIMED behaviour; appearance owed** | A drawn file row, context band, Expand All, and Collapse All controls change the real expansion state through clicks; diff pixels are not asserted. |
| `F-CHG-14` | **BUILDER-CLAIMED behaviour; appearance owed** | Drawn Discard opens the real confirmation, Cancel preserves the checkout, and Discard restores it. |
| `F-CHG-18` | **FAILED/ABSENT behaviour** | No changed-file drag/drop handler exists; low-level drag synthesis alone is not evidence. |
| `F-TAB-03/04/05/06/07/08` | **PARTIAL builder evidence; shell routed** | The real drawn New Tab menu and every item dispatch a typed action and close. Actual terminal/chat/browser creation belongs to the shell and is routed to codex12. |
| `F-TAB-19/20/22` | **HARNESS CAPABILITY; shell routed** | `VisualTestContext::simulate_keystrokes` dispatches modifier chords through a real focused element. The actual workspace bindings live in `tiller/src/main.rs`/`panes.rs` and have no drawn shell test here. |
| `F-TAB-24` | **FAILED/ABSENT behaviour** | Escape can be sent, but there is no tab-drag implementation to cancel. |
| `F-TAB-28` | **FAILED/ABSENT behaviour; shell routed** | The shell binds `ctrl-alt-w` for pane close, not the inventory's `Cmd-W` tab close. This is an implementation gap, not a display block. |

Entries that were already **FAILED**, **UNREACHABLE**, **N/A**, or absent are not rescued by this
audit. For example, the lack of an implementation for an editor action remains a structural
failure; it is not converted into a display debt merely because a test could click it if it
existed. Likewise, a native Finder, browser, or permission result needs its external seam or
platform fixture; that is not evidence that a display is required.

## Evidence already in the tree

- `file_view.rs` contains drawn-frame tests for Code/Preview switching, including the large-file
  manual-preview unlock. These reclaim F-EDIT-01's behavior half; source and Markdown typography
  remain appearance debt.
- `changes.rs` contains drawn-frame tests for the Retry/error recovery path, directory-like
  section/file/context-band expansion, Expand All/Collapse All, Stage/Unstage, Stage all, Discard all, and individual
  Discard confirmation. Together with F-EDIT-01 and F-EDIT-09, these are nine reclaimed
  interaction entries when the partial F-CHG-03/04 and F-CHG-11 verdicts are counted once each.
- `right_panel.rs` contains drawn-frame tests for explorer double-click routing (F-EDIT-09) and
  directory expand/collapse (F-CHG-03/04), plus a harness-only payload-drag fixture proving the
  real mouse down/move/up sequence reaches a drop target. The product file-row/pane drag contract
  remains absent, so F-EDIT-12 is not promoted.
- `tab_bar.rs` contains drawn-frame tests for every New Tab menu item and a real Escape dismissal,
  plus a focused fixture proving `ctrl-tab`, `ctrl-1`, and `ctrl-w` reach GPUI action handlers.
  These prove the UI seam/capability; shell-owned results remain routed, and the fixture is not
  being misreported as a workspace tab test.
- The full `tiller_ui` library suite passes headlessly (`cargo test --manifest-path
  rust/Cargo.toml -p tiller_ui --lib`). `sidebar.rs`, `settings.rs`, `chat.rs`, and `file_view.rs`
  also use this harness pattern for their existing tests.

## What remains display-only

The visual freeze, comparison against waku, exact colours and fonts, pixel-level icon/status
treatment, spacing/layout, and the appearance half of every mixed entry remain
`NOT EXERCISED — blocked on display`. A drawn frame proves that something is laid out and
interactive; it does not prove what its pixels look like.

## P39 — entries reclaimed with drawn-frame evidence

Pass 7's rule is applied here: **reachable and present are different claims.** An entry is
recorded as behaviour-passed only where the feature exists in the tree AND a drawn-frame test
drives it (find element by debug bounds, dispatch a real click / mouse sequence / key chord
through GPUI, assert the resulting state). Where only the harness capability is proven, that is
said plainly; where no code exists, the entry stays FAILED (absent), not "reachable".

Every drawn test uses the hardened pump (600-iteration budget, `allow_parking`, virtual-clock
advance where a timer is involved, `cx.cx.run_until_parked()` before reading bounds) so a pass
is not timing luck.

| Entry | Verdict | Evidence |
|---|---|---|
| `F-EDIT-01` Code/Preview modes | **PRESENT + BEHAVIOUR PASSED** (built in P39; the pass-7 snapshot predates it) | `file_view.rs`: `MarkdownMode` + a drawn Code/Preview switcher; tests `markdown_preview_and_code_modes_switch_in_the_drawn_frame`, `a_code_file_has_no_mode_switch_and_always_renders_source`, `a_large_markdown_file_opens_in_code_with_manual_preview_and_unlocks` — the switcher is found and clicked, each mode renders the content that belongs to it, and a large Markdown file opens in Code with the manual-preview notice that clicking Preview unlocks. Typography inside each mode remains a display debt. |
| `F-EDIT-09` double-click to open | **PRESENT + BEHAVIOUR PASSED (event-level)** | `right_panel.rs`: file rows open on a real `click_count=2` mouse-down; test `double_clicking_a_drawn_file_row_emits_open_file` asserts the emitted `OpenFile(path)` (the resulting editor tab is the shell's job — routed). |
| `F-EDIT-12` drag a file row into a pane | **HARNESS PROVEN; product ABSENT — routed** | `right_panel.rs` test `a_payload_drag_reaches_the_drop_target_through_real_mouse_events` proves an `on_drag` source delivers a `PathBuf` payload to an `on_drop` target through the real mouse-down/move/up path. No file row is a drag source and no pane is a drop target — both live in the shell (`tiller/src/main.rs`), routed to codex12. |
| `F-CHG-03` browse/refresh, loading/error/Retry | **PARTIAL** | Tree browse and refresh exist; the "Files unavailable + Retry" loading/error state does not — that half is absent. |
| `F-CHG-04` expand/collapse/select with the mouse | **BEHAVIOUR PASSED (drawn)** | `right_panel.rs` test `clicking_a_drawn_directory_row_expands_and_collapses_it` — the drawn directory row is clicked at its bounds and the tree expands/collapses. (Pass 7 saw only the handler-direct model tests; the drawn test landed after its snapshot.) |
| `F-CHG-09` error + Retry | **BEHAVIOUR VERIFIED** | `changes.rs` `the_error_state_renders_and_retry_is_clickable` (P34): a real `.git` removal, error/Retry bounds in a drawn frame, a real click on Retry, recovery. |
| `F-CHG-10` stage/unstage a file | **PRESENT + BEHAVIOUR PASSED** | `changes.rs` `drawn_stage_and_unstage_buttons_mutate_the_real_checkout` — drawn row controls mutate a real checkout; assertions read the index back with git. |
| `F-CHG-11` stage-all / unstage-all / discard-all | **PARTIAL** | Toolbar-level `Stage all` (`drawn_stage_all_button_stages_every_changed_file`) and `Discard all` (`drawn_discard_all_button_requires_confirmation_and_clears_worktree`, which asserts the documented boundary: tracked changes cleared, untracked deliberately retained) pass as drawn tests. The documented **section-level** actions — and any **Unstage all** — do not exist. |
| `F-CHG-12` expand a file / collapsed-context band | **BEHAVIOUR PASSED** | `changes.rs` `drawn_changes_rows_expand_sections_and_context_bands` — section header, file row and band are found in the drawn frame and clicked; expansion state changes through dispatch. |
| `F-CHG-14` discard after confirmation | **BEHAVIOUR PASSED** | `changes.rs` `drawn_discard_button_requires_confirmation_then_mutates_git` — the real prompt appears, Cancel leaves the checkout unchanged, a confirmed Discard restores it. |
| `F-CHG` Collapse All / Expand All | **PRESENT + BEHAVIOUR PASSED (new affordance)** | `changes.rs` `drawn_expand_all_and_collapse_all_drive_the_whole_list` — orca's diff-header affordance (`04-ux-patterns`); both toolbar buttons drive every file row and every context band, and the choice is view state, not a git mutation. |
| `F-TAB-19/20/28` chords | **HARNESS PROVEN; shell tests routed** | `tab_bar.rs` `modifier_chords_dispatch_actions_through_the_real_key_path` — `ctrl-tab`, `ctrl-1`, `ctrl-w` reach `on_action` handlers through a scoped `KeyBinding` + key context + real keystroke dispatch (the pass-7 "no chord-dispatch test exists" note is superseded). The workspace that binds the real chords and owns the tab strip is the shell — routed to codex12; `ctrl-w` is **not yet bound** (only `ctrl-alt-w` ClosePane exists), a gap to close. |
| `F-TAB-03` + menu | **MENU HALF PASSED; tab creation routed** | `tab_bar.rs` `drawn_new_tab_menu_dispatches_every_item_action` + `drawn_new_tab_menu_escape_dispatches_through_the_real_key_path` — the + control draws the menu, every item emits its typed action, Escape closes it. Creating the tab from the action is the shell's. |
| `F-TAB-15` close control | **PRESENT + BEHAVIOUR PASSED (sidebar view); shell strip routed** | `sidebar.rs` `the_drawn_tab_close_control_reports_closeta_tab` — a host-pushed tab row's hover-revealed ✕ is found in the drawn frame, clicked, and emits `CloseTab(real tab id)` without also selecting. The centre strip's ✕ (`workspace-tab-close-{id}` in `tiller/src/main.rs`) remains shell-owned — routed to codex12. |
| `F-SET-03` version + Check for Updates | **PARTIAL — click proven, result states absent** | `settings.rs` `general_settings_state_the_version_and_the_updates_control_is_clickable` — the drawn General card states the version and the Check for Updates button is found and clicked through real dispatch. The button's handler is **empty**, so no checking/up-to-date/update/error result states exist; that half stays absent. |
| `F-SET-21` Files icon theme | **BEHAVIOUR PASSED (pre-existing drawn test)** | `settings.rs` `selecting_the_listed_file_icon_set_changes_the_snapshot` already drives the drawn choice; the file-tree icons' pixels remain display-blocked. |
| `F-CHG-20` No-activity / running count | **PRESENT + BEHAVIOUR PASSED (built in P44)** | The pass-7 finding was correct: neither state existed. P44 built them — the expanded empty section states `No activity`, and the header states `N running` beside the Activity label while rows are active — and `right_panel.rs` `activity_section_states_no_activity_when_empty` + `activity_section_states_the_running_count` click the drawn header and assert both elements plus row layout. Appearance stays display-blocked. |

Appearance-only halves (typography, glyphs, colours, spacing) of the mixed entries above remain
`NOT EXERCISED — blocked on display`; the interaction halves above are no longer display-blocked
where the feature exists. Entries recorded as PARTIAL stay partial until the absent half is built.

## P44 — the chat surface, exercised per entry (13 August, ~16:00)

The chat surface had not been exercised since ~01:00. Every `F-CHAT-*` entry was judged with the
three questions in order — does it exist at all; can it be exercised here; does it do what the
VERIFY clause says — and every drawn test below uses the hardened pump (600-iteration budget,
`allow_parking`, virtual-clock advance, real sleep, `cx.cx.run_until_parked()` before reading
bounds; the suite passed three consecutive runs, so a pass is not timing luck). The ACP side of
every behavioural test is a real subprocess fixture (`tests/fixtures/chat_fixture.py`) speaking
the v1 JSON-RPC wire protocol, so the transcript events are the protocol tier's own events.

| Entry | Verdict | Evidence |
|---|---|---|
| `F-CHAT-01` transcript, user bubbles, markdown, turn timing | **BEHAVIOUR PASSED (drawn); duration half absent** | `a_streamed_reply_grows_one_entry_with_thought_tool_and_usage` — typed prompt → user entry; streamed reply grows **one** entry in place (go-file-gated fixture makes the mid-growth assertion deterministic); markdown parses; the turn footer (a timestamp, not a duration) closes the turn. |
| `F-CHAT-02` auth-required + Retry | **FAILED — absent** | No authentication banner or CLI guidance exists; a failed launch surfaces as a generic error card. |
| `F-CHAT-03` disconnected + Restart agent | **BEHAVIOUR PASSED (drawn)** | `a_stream_that_dies_mid_reply_states_the_error_and_retry_recovers` — transport death states an error card in the transcript; its Retry control is drawn, clicked, relaunches the agent, clears the error, and a later turn completes. The label is "Retry", not "Restart agent" (naming is appearance). |
| `F-CHAT-04` Return sends / Shift+Return newline | **BEHAVIOUR PASSED (drawn)** | `enter_sends_and_shift_return_inserts_a_newline` — both keys through the real key-dispatch path; Shift+Return inserts `\n` and sends nothing. |
| `F-CHAT-05` composer disabled while permission-wait | **PARTIAL** | The composer cannot send while a permission is pending (asserted in `a_permission_prompt_answers_both_ways`); the "editor disabled with placeholder" UI does not exist. |
| `F-CHAT-06` queue next-turn while prompting | **FAILED — absent** | No queue concept. |
| `F-CHAT-07` Stop control or Escape | **PARTIAL — Escape half PASSED** | `escape_cancels_the_stream_and_the_transcript_states_it` — Escape stops the streaming turn immediately and the transcript footer states `· cancelled`; a Stop control never exists (see F-CHAT-08). |
| `F-CHAT-08` connecting/send/stop primary states | **PARTIAL** | Connecting pill (`connecting_state_renders_while_startup_is_in_flight`) and the send control (`empty_chat_renders_composer_and_typing_does_not_send`) are drawn; the send control never morphs into Stop — that state is absent. |
| `F-CHAT-09` slash commands | **FAILED — absent** | |
| `F-CHAT-10` `@` file mentions | **FAILED — absent** | |
| `F-CHAT-11` image attach | **FAILED — absent** | |
| `F-CHAT-12` remove attachment chip | **FAILED — absent** | |
| `F-CHAT-13` drop files to attach | **FAILED — absent** | The drag harness capability is proven elsewhere; no chat drop surface exists. |
| `F-CHAT-14` overflow menu | **FAILED — absent** | |
| `F-CHAT-15` permission-mode pill | **FAILED — absent** | The status pill exists but is not a mode selector. |
| `F-CHAT-16` model picker search/Recommended/select | **PARTIAL** | Open, select, Escape-dismiss, click-away: `model_picker_selects_an_agent_advertised_model_and_escape_dismisses`. Search field and Recommended labels do not exist. |
| `F-CHAT-17` effort level | **FAILED — absent** | |
| `F-CHAT-18` context ring breakdown | **PARTIAL** | Popover shows fraction, tokens, cost (`context_ring_shows_reported_usage_and_escape_dismisses_popover`); the staged test asserts a live `usage_update` from the wire lands in `context_usage`. Input/output/cache breakdown absent. |
| `F-CHAT-19` ring warning colour >80% | **FAILED — absent** | The ring always paints `gauge`; no threshold logic (and it is appearance anyway). |
| `F-CHAT-20` follow streaming / scroll ownership | **PARTIAL** | `FollowMode::Tail` follows the stream (the staged test asserts the growing tail lays out); manual-scroll ownership is absent. |
| `F-CHAT-21` thinking expand/collapse | **FAILED — absent** | Thought chunks render as plain italic text with no toggle. |
| `F-CHAT-22` tool-group / older-turn folding | **FAILED — absent** | |
| `F-CHAT-23` tool call expand/output/dismiss | **PARTIAL** | A tool call card renders with title and terminal status (staged test, `Pending`→`Completed` over the wire); expand, output inspection, and Dismiss are absent. |
| `F-CHAT-24` plan approve/reject | **FAILED — absent** | |
| `F-CHAT-25` question text/option/cancel | **PARTIAL — both options PASSED** | `a_permission_prompt_answers_both_ways` — the permission card's Allow and Deny buttons are both drawn, clicked, and each records `Answered: …` on the card; text answers and a Cancel affordance are absent. |
| `F-CHAT-26` pending-bar jump | **FAILED — absent** | |
| `F-CHAT-27` expired-question state | **FAILED — absent** | An unresolved card stays unresolved when the turn ends. |
| `F-CHAT-28` subagent task cards | **FAILED — absent** | |
| `F-CHAT-29` copy response + confirmation | **PARTIAL** | Selection + `cmd-c` copy exists (`transcript_selection_copies_across_entries_from_global_offsets`, model-level); the per-message hover Copy control with checkmark is absent. |
| `F-CHAT-30` copy from code block | **FAILED — absent** | |
| `F-CHAT-31` diff preview open file | **FAILED — absent** | |
| `F-CHAT-32` edit summary open/revert | **FAILED — absent** | |
| `F-CHAT-33` turn errors + MCP warnings + OK | **PARTIAL** | Turn errors surface as stated transcript cards (launch failure and transport death, drawn-tested); the MCP-warning banner and an OK acknowledgement do not exist. |
| `F-CHAT-34` chat history browse/open/delete | **FAILED — absent (UI)** | Transcript retention/restore for resume exists (`a_retained_transcript_can_be_restored_as_visible_chat_history`), but there is no Chat History menu. |
| `F-CHAT-35` no-past-chats empty state | **FAILED — absent** | |
| `F-CHAT-36` no-models agent control | **PARTIAL** | The picker states "The connected agent did not report any models"; the separate agent control does not exist. |
| `F-CHAT-37` empty transcript + usable composer | **BEHAVIOUR PASSED (drawn)** | `empty_chat_renders_composer_and_typing_does_not_send` — zero transcript items before any message; composer, send, status pill, transcript surface all drawn; typing fills the composer and sends nothing. |

**The seam — abnormal ends must be stated, not silent.** The protocol tier already expresses
everything the surface needs; nothing was requested from it. The surface was missing one state: a
`TurnEnded` with a non-`EndTurn` stop reason (cancelled, refusal, token limits) was discarded and
the turn closed with a plain timestamp — exactly the "ordinary-looking completion hiding an
abnormal end" failure. `chat.rs` now states the reason in the turn footer (`HH:MM · cancelled`,
`· refused to continue`, `· stopped at the token limit`, `· stopped at the turn-request limit`),
driven by the wire-level `stop_reason`; `escape_cancels_the_stream_and_the_transcript_states_it`
proves the cancelled state appears and that a later normal turn stays plain. A stream that dies
mid-reply already stated itself (error card + Retry) and is now drawn-proven.

Appearance halves (bubble geometry, coral accent, type scale, the waku comparison, footer styling)
remain `NOT EXERCISED — blocked on display`.
