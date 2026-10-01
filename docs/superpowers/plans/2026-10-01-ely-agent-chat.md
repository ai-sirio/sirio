# Ely Agent Chat Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the entire AI chat presentation with Ely components using Sirio's theme and white agent logos, preserving current chat interactions.

**Architecture:** Vendor a pinned Ely component crate and adapt it to Sirio's existing Bezel GPUI packages. `Chat` retains its connection, protocol, persistence, editing, selection and scrolling state; focused render modules use Ely through controlled component slots. A native compatibility milestone precedes the production UI changes.

**Tech Stack:** Rust 2024, Bezel GPUI 0.3.8, Bezel 0.1.4, pinned Ely 0.1.0, GPUI visual tests, real ACP/native-Claude fixture processes and Sirio's control socket.

**Spec:** [2026-09-30-ely-agent-chat-design.md](../specs/2026-09-30-ely-agent-chat-design.md), approved on 2026-10-01. Read it and `CLAUDE.md` before executing.

**Status:** proposed; execution waits for plan review and selection of execution method.

## Global Constraints

- Coverage is **the entire AI chat surface in Sirio**; the surrounding application retains its components.
- Ely is `ely-gpui-component` 0.1.0, revision `e17e31a6890c09ebcfa8b61133d7bc7c625edf69`, MIT OR Apache-2.0; record every local deviation.
- Upstream requires Rust 1.95 and Zed revision `1a28cff4b409169bac058bca40dfbfeb7621d19b`. Reuse `bezel-gpui` / `bezel-gpui-platform` `=0.3.8`; do not add a second GPUI type graph.
- Preserve Bezel `=0.1.4`, platform features including Linux Wayland/X11, and the Linux/Windows GPUI and Ghostty CPU baseline patches.
- The workspace is already in the 0.29.0 minor cycle; this feature does not bump it. Follow `CLAUDE.md`'s cycle rule if that baseline changes; `Scripts/set-workspace-version.sh` remains the only version writer.
- Use Sirio's resolved theme, UI/code fonts, scaling and motion preference; update Ely globals only when inputs change.
- Agent marks are monochrome white on a dark neutral plate in both appearances. Claude uses Claude Code's pixel mark; Codex/OpenCode/Pi/omp use their own marks. Resolve stable IDs and ACP aliases; a model change cannot change identity.
- Preserve `Chat`'s existing public API, `ChatEvent`, protocol messages, control methods and persistence schema. Additive identity information may pass from the host to the view.
- Preserve current disabled/submit rules, queue FIFO/clear/remove/send-now, attachment-only eligibility, request IDs, selection/copy, turn navigation, scrolling, history and rewind eligibility.
- Display only reported/computed catalogues, usage, durations and provider identities. Keep the application's existing strings; the HTML example does not specify translated production copy.
- `Scripts/ci.sh` and `Scripts/ci-linux.sh` run only on the user's explicit request. Use targeted commands below; never invoke either script from a new test script.
- macOS is the reference platform. Record unavailable platform checks explicitly. No claim that HTML captures establish native compatibility.
- No product dependency installation, scaffolding or implementation until this written plan is reviewed and the execution method is selected. Create isolation using `superpowers:using-git-worktrees` at execution time.

## Review Focus

1. **Enter during IME composition or an open completion popup:** composition/selection completes without sending; a later Enter queues exactly once while busy. Task 6 pins this.
2. **A theme/scale change during streaming:** the current draft, focus, selected text and scroll position survive; Ely does not alter another view's Bezel theme. Task 2 pins this.
3. **A reused tool ID after changing conversations:** expansion and scroll state belong to the current transcript; old state does not leak into the new one. Tasks 3–4 pin this.
4. **A request expires while its controls have focus:** stale controls cannot send a response, and a later request's draft/choice is independent. Task 5 pins this.
5. **Unicode selection across a virtualized tool/message boundary:** copying returns the expected transcript text after expansion and remeasurement, without new visual chrome in the clipboard. Tasks 3–4 pin this.

---

## File boundaries and test conventions

Paths below are relative to the repository root. Existing source anchors name
symbols because line numbers will move as rendering is extracted.

| File | Responsibility |
|---|---|
| `rust/vendor/ely-gpui-component/` | Pinned component source, required assets, local compatibility changes and licenses. |
| `rust/crates/sirio_ui/src/chat/ely.rs` | Ely initialization, cached theme synchronization and the composite asset source. |
| `rust/crates/sirio_ui/src/chat/identity.rs` | Stable agent identity and white mark presentation. |
| `rust/crates/sirio_ui/src/chat/transcript.rs` | Message surfaces, row identities, day headings and list composition. |
| Existing `chat/{tool_calls,thought,turn_rail,list_scroll}.rs` | Rich activity and existing scrolling/navigation behavior. |
| Existing `chat/question_dock.rs` | Active requests and response focus. |
| Existing `chat/composer_view.rs` | Editing/completion behavior and Ely composer shell. |
| New `chat/{controls,menus,history}.rs` | Agent catalogues/usage controls, context menus and history presentation. |
| Existing `chat/mod.rs` | Model, event processing and host API; remove migrated rendering bodies from this file. |
| New `chat/ely_tests.rs` | Only new behavioral regressions introduced by component seams. |
| Existing `sirio_ui/tests/fixtures/chat_fixture.py` | Deterministic real ACP process, extended only for required wire scenarios. |
| New `sirio_persistence/examples/ely_chat_seed.rs` | Creates a scratch chat session through existing persistence APIs for the real-app runner. |
| New `Scripts/Tests/test-ely-chat-ui-e2e.py` | Isolated real-app replay, control readback and native captures. |

Extend the existing `chat::tests` harness instead of writing another executor
or subprocess pump. Expose only the needed helpers to sibling test modules
with `pub(super)`: `chat_view`, `offline_chat_view`, `pump_chat_until`,
`refresh_frame`, `TempDir`, `CHAT_FIXTURE` and the existing selection/queue
helpers used by a new test. Preserve their current signatures. Register
`#[cfg(test)] mod ely_tests;` in `chat/mod.rs`.

In the test assertion fragments below, `chat` is `Entity<Chat>` and `cx` is
`&mut VisualTestContext` from that harness. The preceding prose specifies the
actions/fixture setup. Implement each named test using those actions and
assertions; these are not separate production helper APIs.

All Cargo commands run with working directory `rust/`. Test commands must
report a nonzero number of executed tests and zero failures. Start each
behavioral change with its named regression failing for the intended reason,
then implement it and rerun. Reuse current tests for preserved behavior;
layout/assets/configuration changes use build and native visual evidence.
`TestAppContext` installs an empty asset source; these tests prove interaction
and state, not SVG painting. Keep chat initialization independent of eager
asset validation. The native probe uses `ChatAssets` and proves icon loading.

## Task 1: Prove Ely compatibility on the existing GPUI foundation

**Files:**

- Create: `rust/vendor/ely-gpui-component/` from the pinned source, with `LOCAL-CHANGES.md` and licenses.
- Modify: `rust/Cargo.toml`, `rust/Cargo.lock`, `rust/crates/sirio_ui/Cargo.toml`, `rust/vendor/README.md`.
- Create: `rust/crates/sirio_ui/examples/ely_chat_probe.rs`.
- Modify vendored: `Cargo.toml`, `src/lib.rs`, `src/assets.rs`, `src/primitives/icon.rs` and the required module export files.
- Document: `CLAUDE.md`'s UI-reference paragraph, narrowly recording the approved Ely exception for chat. Read `writing-for-agents` before this documentation edit.

**Interfaces:**

- Produces workspace dependency `ely-gpui-component` at the local vendor path, `default-features = false`, `features = ["sirio-chat"]`.
- Produces `ely_gpui_component::init_chat(cx: &mut gpui::App)`, idempotently initializing the needed theme/components without registering unrelated keys or fonts.
- Produces `ely_gpui_component::Assets: gpui::AssetSource` and Ely icon paths under `ely/`.
- Produces a native example containing a real `MessageBubble`, `ToolCallCard` and editable Ely `PromptInput`/`TextInput`.

- [ ] **Step 1: Add the native probe before the dependency.**

  Put a small GPUI `Render` view in `ely_chat_probe.rs`. Import the three
  components above, retain an `Entity<TextInput>`, and append its submitted
  text to a visible message list. Initialize its own asset source and the
  existing platform/font setup. The example must exercise editing and submission,
  not only create unused components.

- [ ] **Step 2: Check the expected initial failure.**

  Run `cargo check -p sirio_ui --example ely_chat_probe`.
  Expected: unresolved `ely_gpui_component` import, with no product changes yet.

- [ ] **Step 3: Vendor and adapt the required component closure.**

  Verify the checkout SHA before copying. Add the vendored crate as an explicit
  workspace member so targeted `-p ely-gpui-component` commands use Sirio's
  lockfile and workspace GPUI dependencies. Keep Ely's own package version
  0.1.0 and Rust 1.95 floor. Set its default feature to `sirio-chat`; remove
  upstream Gallery targets from this integration manifest.

  Retain upstream source provenance; the supported build exports chat/agent
  surfaces and their required buttons, forms, layout, menus, overlays,
  primitives, motion, theme and typography closure. Exclude unrelated finance,
  terminal, webview and document/editor stacks and their unused dependencies
  from this build. Resolve internal imports within the required closure, rather
  than adding unrelated product dependencies to make all upstream modules compile.

  Use workspace GPUI dependencies plus Linux-only Wayland/X11 features.
  Implement `init_chat` and the `ely/` asset namespace in both icon paths
  and `Assets::load/list`. Preserve icon geometry. Record all API adaptations,
  module/dependency exclusions and initialization differences in `LOCAL-CHANGES.md`.
  Move upstream's eager icon availability assertion into the native probe;
  initialization must also work in existing asset-free GPUI test contexts.

- [ ] **Step 4: Verify compilation and dependency provenance.**

  Run:

  ```bash
  cargo check -p ely-gpui-component --no-default-features --features sirio-chat
  cargo check -p sirio_ui --example ely_chat_probe
  cargo tree -p sirio_ui -i bezel-gpui
  cargo tree -p sirio --target x86_64-unknown-linux-gnu -i bezel-gpui-linux
  cargo tree -p sirio --target x86_64-pc-windows-msvc -i bezel-gpui-windows
  cargo tree -p sirio -i libghostty-vt-sys
  ```

  Expected: successful checks, one `bezel-gpui` version, the existing three
  vendor patches selected, and no `patch ... was not used` warnings. Inspect
  `Cargo.lock` for unintended GPUI git sources or unrelated platform stacks.

- [ ] **Step 5: Run and inspect the native probe.**

  Run `cargo run -p sirio_ui --example ely_chat_probe` on the available native
  display. Type, select/copy, enter a newline and send a message. Retain a PNG
  and command/toolchain record in an untracked artifact directory. If only
  a headless check is available, explicitly leave native rendering unverified.
  Do not proceed with production UI conversion until an available native
  rendering lane proves the compatibility milestone. A broad GPUI migration
  requires a design amendment before continuing.

- [ ] **Step 6: Commit the compatibility deliverable.**

  Stage only this task's files; commit `build: adapt Ely chat components to Sirio GPUI`.

## Task 2: Integrate theme, assets and white agent identity

**Files:**

- Create: `rust/crates/sirio_ui/src/chat/{ely,identity,ely_tests}.rs`.
- Modify: `rust/crates/sirio_ui/src/chat/mod.rs` (`init`, constructors, `set_agent_name`).
- Modify: `rust/crates/sirio/src/main.rs` (app asset setup and all chat construction/restore paths).
- Create: `rust/assets/icons/chat/claude-code.svg`, `rust/assets/icons/chat/ATTRIBUTION.md`.
- Modify vendored: `src/chat/message.rs`, `src/theme/{mod,tokens}.rs`.
- Modify: `rust/crates/sirio_ui/examples/ely_chat_probe.rs` to use the integrated assets/theme.

**Interfaces:**

- `pub struct ChatAssets;` in `chat/ely.rs`, implementing `AssetSource`; re-export from `chat/mod.rs`.
- `pub(crate) fn init(cx: &mut App)` and `pub(crate) fn sync_theme_if_changed(cx: &mut App)` in `chat/ely.rs`.
- Add `agent_id: Option<String>` to `Chat` and additive `pub fn set_agent_identity(&mut self, agent_id: Option<String>, display_name: Option<String>)`; keep `set_agent_name` for existing consumers.
- `pub(crate) fn render_mark(id: ElementId, agent_id: Option<&str>, name: &str, theme: &Theme) -> AnyElement` in `identity.rs`.
- Vendored `MessageAvatar::content(self, content: impl IntoElement) -> Self` to accept the host's mark and plate.
- Vendored `ThemeMetrics { text: [Pixels; 8], radii: [Pixels; 4] }`, stored as an optional metric override on Ely's `Theme`. Existing `text_size/radius` return types remain `Rems`.

- [ ] **Step 1: Add the theme-change behavioral regression.**

  `ely_theme_change_preserves_chat_state_and_bezel_palette`: open the staged
  fixture, type an unsent Unicode draft, select transcript text and scroll
  away from the tail. Change Sirio's theme and interface size, draw two frames,
  and record the existing state before/after. Assertions:

  ```rust
  assert_eq!(chat.read_with(&cx.cx, |v, _| v.draft_text()), draft_before);
  assert_eq!(chat.read_with(&cx.cx, |v, _| v.selected_transcript_text()), selection_before);
  assert_eq!(focused_handle_after, focused_handle_before);
  assert!(!chat.read_with(&cx.cx, |v, _| v.list_state.is_following_tail()));
  let expected_bezel = resolved_sirio_theme.to_bezel_theme();
  assert_eq!(bezel_after.text, expected_bezel.text);
  assert_eq!(bezel_after.surface, expected_bezel.surface);
  assert_eq!(bezel_after.border, expected_bezel.border);
  assert_eq!(ely_after.font_family.as_ref(), resolved_sirio_theme.typography.ui_family);
  ```

  Use a test window hosting an Ely probe next to an existing Bezel control
  so the test actually observes the two theme consumers. On the initial
  implementation the Ely probe still uses its default appearance, so the
  font/appearance synchronization assertions fail.

- [ ] **Step 2: Run the focused test red.**

  Run `cargo test -p sirio_ui --lib ely_theme_change_preserves_chat_state_and_bezel_palette`.
  Expected: the intended missing theme integration assertion fails.

- [ ] **Step 3: Implement the adapter and identity inputs.**

  `ChatAssets` dispatches `ely/` to Ely's asset source, `sirio-chat/` to
  embedded agent-mark bytes, and all remaining paths to the currently used
  `bezel::ui::icons::Assets`. `list` follows the same namespaces. Change the
  app's `with_assets` source to `ChatAssets`; keep font registration before
  `Theme::init` and extend the existing `sirio_ui::chat::init` call.

  Cache the last `(sirio_theme::Theme, reduced_motion)` in one GPUI global.
  Initialize after Sirio's theme. Observe Sirio theme changes; also call
  `sync_theme_if_changed` at chat render entry to pick up GPUI's motion flag
  through `bezel::motion::AppExt::reduced_motion`. Compare before updating;
  there is no global theme mutation or new timer for an unchanged frame.
  Make the first synchronization initialize missing Ely globals idempotently,
  including offline/restored test views that bypass application initialization.

  Map palette `fg/fg_muted/fg_subtle`, surfaces, border/focus/selection/link
  and success/warning/danger from Sirio's corresponding tokens. Set UI/code
  font families from `theme.typography`. Use this metric order:

  | Ely metric | Sirio token |
  |---|---|
  | `Xs`, `Sm`, `Base`, `Md` | `footnote`, `ui_size`, `base_size`, `callout` |
  | `Lg`, `Xl`, `Xxl`, `Display` | `headline`, `title2`, `title`, `large_title` |
  | `Radius::Sm`, `Md`, `Lg`, `Xl` | `chip`, `control`, `code_block`, `composer` |

  Use Sirio's spacing tokens for chat wrappers; the existing rem/window scale
  continues to drive Ely's internal spacing. Convert metric pixels to rems
  once without applying font scale a second time.

  Pass the host's known agent ID and display name at every creation/restore
  site currently assigning `set_agent_name`, including unavailable and
  archived chats. Keep unknown IDs/names honest. Resolve aliases as the
  existing `Icon::for_agent_id` does. Embed Claude Code's pinned pixel SVG
  from LobeHub 1.95.0 and existing Codex/OpenCode/Pi/omp geometry under the chat
  asset namespace. Render as an SVG mask with white tint; use a dark neutral
  plate derived from the resolved Sirio palette. Preserve each SVG's fill
  rule, holes and viewBox; do not recolor the global icon catalog.

- [ ] **Step 4: Verify state, assets and native logos.**

  Run the focused test, `cargo test -p sirio --bin sirio restored_chat`, and
  `cargo check -p sirio --bin sirio`. Capture Claude/Codex in light and dark
  from the native probe; inspect recognizable marks, white tint, contrast
  and typography. Test an unknown ID and a model change: identity stays
  tied to the agent. Asset absence is a visible test failure, not a blank
  image silently accepted. Record native IME/motion limitations if not exercised.

- [ ] **Step 5: Commit.**

  Commit `feat: integrate Ely theme and white chat agent marks`.

## Task 3: Convert messages, transcript layout and navigation

**Files:**

- Modify: `chat/{mod,transcript,list_scroll,turn_rail,ely_tests}.rs`.
- Modify vendored: `src/chat/message.rs` and `src/chat/list.rs` only if required for host slots.

**Interfaces:**

- In `Chat`, `transcript_generation: u64` and `pub(crate) fn row_id(&self, index: usize, cx: &Context<Self>) -> ElementId`; combine entity ID, generation and entry index.
- Increment generation on full transcript replacement/clear, including history reopen and new conversation. Ordinary append/chunk updates retain row identity.
- Move the existing static entry renderer into `transcript.rs`, preserving its separation from mutable `Chat` borrows inside list callbacks. Its signature becomes `fn render_entry(entry: Entry, row: TranscriptRowContext<'_>, theme: &Theme, window: &mut Window, cx: &mut App) -> AnyElement`.
- `TranscriptRowContext<'a>` packs the current arguments plus row/identity inputs: `index: usize`, `id: ElementId`, `chat: Entity<Chat>`, `focus: FocusHandle`, `source_start: usize`, `copied_target: Option<CopyTarget>`, `edit_summary: Option<EditSummaryState>`, `thought_streaming: bool`, `thought_scroll: &'a HashMap<usize, thought::ThoughtScroll>`, `tool_output_scroll: &'a HashMap<String, ScrollHandle>`, `day_heading: Option<&'a str>`, `agent_id: Option<&'a str>` and `agent_name: &'a str`. Keep existing plain-text and Markdown helpers.
- `pub(crate) fn render_header(&self, theme: &Theme, cx: &Context<Self>) -> AnyElement` in `transcript.rs` consumes Task 2's identity.
- Preserve `render_day_heading` and list/turn-rail handlers; use Ely surfaces with the existing `ListState`.

- [ ] **Step 1: Add identity and remeasurement regressions.**

  `ely_row_identity_changes_only_when_transcript_is_replaced`: append to a
  streamed assistant entry, then replace the transcript with a new conversation
  containing a reused tool ID. Assert the first identity is stable during the
  chunk and differs after replacement.

  ```rust
  assert_eq!(row_before_chunk, row_after_chunk);
  assert_ne!(row_before_replace, row_after_replace);
  ```

  `ely_unicode_selection_survives_row_remeasurement`: select across a Unicode
  user/assistant/tool sequence, resize the pane and expand a card, then copy
  through the real copy action.

  ```rust
  assert_eq!(chat.read_with(&cx.cx, |v, _| v.selected_transcript_text()), selected_before);
  assert_eq!(clipboard_text, selected_before.unwrap());
  assert!(!chat.read_with(&cx.cx, |v, _| v.list_state.is_following_tail()));
  ```

- [ ] **Step 2: Run the new identity test red.**

  Run `cargo test -p sirio_ui --lib ely_row_identity_changes_only_when_transcript_is_replaced`.
  Expected: missing row-generation seam, or an assertion failure on replacement.

- [ ] **Step 3: Implement Ely message layout on the existing list.**

  Frame the list and bottom region with `ChatContainer`; do not use Ely's
  private `MessageList` feed. Render right-aligned user `MessageBubble`s and
  left-aligned assistant prose with `MessageAvatar/MessageHeader/MessageFooter`.
  Use the original selectable bodies and parsed Markdown documents. Maintain
  global selection offsets; visual metadata/marks do not become new copied text.

  Keep chronological ordering and date separators, older-turn folds, turn
  previews, rail ticks, scrollbars and tail following. Replace jump-to-latest
  presentation with `ScrollToBottomButton`, delegating to the existing list.
  Render notices, turn footers, rewind previews/results and all error kinds
  through Ely surfaces with existing action handlers. Preserve authentication
  instructions and distinguish interruption from failure/completion.

  Reuse existing targeted remeasurement when entries change; preserve the
  currently visible anchor on width/fold changes. Set the reading column to
  the approximately 760 px reference at default scale with token padding.
  Move only the relevant rendering from `mod.rs`; shared Markdown helpers
  used by other surfaces remain unchanged.

- [ ] **Step 4: Verify transcript interactions.**

  Run both new tests and these existing filters individually:

  ```bash
  cargo test -p sirio_ui --lib transcript_selection_copies_across_entries_from_global_offsets
  cargo test -p sirio_ui --lib a_real_click_unfolds_an_older_turn_and_folds_it_back
  cargo test -p sirio_ui --lib the_transcript_scrollbar_appears_on_overflow_and_tracks_the_list
  cargo test -p sirio_ui --lib failed_launch_can_retry_and_complete
  cargo test -p sirio_ui --lib the_rewind_action_appears_only_where_it_can_work
  ```

  Existing selector names remain where their interaction still exists.
  Update only obsolete geometry assumptions after checking the new UI;
  retain behavioral assertions. Inspect wide/narrow native captures, including
  selectable Markdown, auth guidance, partial failure and restored messages.

- [ ] **Step 5: Commit.**

  Commit `feat: render the chat transcript with Ely message components`.

## Task 4: Convert reasoning, tools, plans and subagent activity

**Files:**

- Modify: `chat/{tool_calls,thought,transcript,ely_tests}.rs`, affected renderers in `chat/mod.rs`.
- Modify vendored: `src/agent/{tools,progress}.rs`, `src/chat/status.rs`.

**Interfaces:**

- Add `.body(self, body: impl IntoElement) -> Self` and `.expanded(self, open: bool, on_toggle: impl Fn(bool, &mut Window, &mut App) + 'static) -> Self` to `ToolCallCard` and `ThinkingBlock`.
- Add the same `.expanded(...)` seam to `ToolCallGroup`. Host children remain supported.
- Consume Task 3's `row_id` for stable parent identities; nested children add their ID/position under that parent.
- Existing `render_tool_body`, output/thought scroll maps, `Takeover`, `toggle_verb_fold` and location handoffs remain the behavior owners.

- [ ] **Step 1: Add a controlled-expansion regression.**

  `ely_tool_and_thought_expansion_survives_virtualization`: expand a tool and
  manually pin a thought open, scroll them offscreen/back, append a stream
  chunk, settle the turn, and reopen a different transcript reusing the tool
  ID. Assert the first transcript retains explicit choices while the second
  starts from its own model state.

  ```rust
  assert!(tool_expanded_after_remount);
  assert!(thought_open_after_settle);
  assert!(!replacement_tool_expanded);
  assert_eq!(selection_after_tool_expand, selection_before_tool_expand);
  ```

  Obtain those observations from the existing entry fields/Takeover and
  actual drawn bodies, not Ely's private state entities.

- [ ] **Step 2: Run the component seam test red.**

  Build the test against the required `.body/.expanded` calls, then run
  `cargo test -p sirio_ui --lib ely_tool_and_thought_expansion_survives_virtualization`.
  Expected: missing controlled seam before implementation.

- [ ] **Step 3: Extend the actual Ely components and wire existing rich bodies.**

  Controlled mode bypasses `window.use_keyed_state` and calls the host toggle
  with the desired open value. Custom bodies count as expandable content.
  `ToolCallCard` keeps status/name/summary/duration and draws Sirio's existing
  selectable argument/output/diff/location bodies. Do not flatten them into
  Ely's string-only `arguments/result`. Failed/running status stays visible
  when the group is closed.

  Use `ThinkingBlock/ThinkingIndicator/ThinkingDuration` around the existing
  takeover and bounded scrolling logic. Use `AgentPlan` and agent progress
  primitives for existing plan rows/approval and `SubagentTask` children.
  Preserve event order and conservative grouping, timing and editor handoffs.
  Group only existing compatible runs; no new aggregate model or invented DAG.
  Collapsed content must not retain newly introduced animation claims/timers.

- [ ] **Step 4: Verify activity and rich content.**

  Run the new test and existing filters `a_live_thought_opens_while_streaming_folds_on_settle_and_obeys_a_press`,
  `an_expanded_execute_output_is_a_capped_well_with_scroll_state`,
  `a_run_is_one_box_and_same_verb_calls_fold`, `a_plan_renders_approval_attaches_and_the_plan_advances`,
  `a_tool_call_diffs_rows_are_addressable_in_the_transcript_text` and
  `following_edited_files_opens_the_tool_calls_reported_location`.
  Run `cargo test -p ely-gpui-component --lib --features test-support` for
  the included component tests; exclude tests belonging to excluded modules
  at their module boundary. Native capture must include expanded arguments,
  a selectable rich diff, failed/running grouped tools and nested edge scrolling.

- [ ] **Step 5: Commit.**

  Commit `feat: present agent activity with controlled Ely components`.

## Task 5: Convert protocol-driven questions and permissions

**Files:**

- Modify: `chat/{question_dock,transcript,ely_tests}.rs` and request renderers in `chat/mod.rs`.
- Modify vendored: `src/agent/{permission,control}.rs`.
- Modify: `rust/crates/sirio_ui/tests/fixtures/chat_fixture.py` only for opaque-option/expiry scenarios.

**Interfaces:**

- Add `PermissionPrompt::custom(id: impl Into<ElementId>, title: impl Into<SharedString>) -> Self`, with `.detail(...)`, `.body(impl IntoElement)` and `.action(impl IntoElement)` slots. The custom constructor emits only supplied actions; the original fixed-option constructor retains its upstream behavior.
- Add `.body(impl IntoElement)` / `.action(impl IntoElement)` to `HumanInputRequest` for the existing answer field and protocol options.
- Existing `pending_question`, `respond_permission`, `answer_question_text`, `cancel_question`, `dismiss_permission` and request-specific focus/draft state remain authoritative.

- [ ] **Step 1: Add opaque-option and expiry regressions.**

  `ely_request_sends_only_the_advertised_option_id`: a real fixture offers
  `allow:this-call` and `deny:this-call`, no session-wide permission. Click
  the rendered allow action and inspect the fixture's echoed response.

  ```rust
  assert_eq!(echoed_option_id, "allow:this-call");
  assert!(cx.debug_bounds("ely-request-always").is_none());
  assert_eq!(wire_response_count, 1);
  ```

  `ely_expired_request_cannot_answer_the_next_request`: keep focus on an
  expiring request, draw its expired state, press Enter, then receive a new
  request with a different ID. Assert:

  ```rust
  assert_eq!(expired_wire_response_count, 0);
  assert_eq!(new_request_answer_draft, "");
  assert_eq!(new_request_selected_option, 0);
  ```

  Counts come from fixture traffic, not a mocked UI callback.

- [ ] **Step 2: Run the new tests red.**

  Run `cargo test -p sirio_ui --lib ely_request_` and
  `cargo test -p sirio_ui --lib ely_expired_request_`.
  Expected: missing dynamic request surface or the stated regression.

- [ ] **Step 3: Implement the bottom request dock and historical outcomes.**

  Instantiate Ely request shells with the existing selectable description,
  wire-driven actions and existing answer editor. Preserve multi-question
  navigation, free-text availability, answer validation, cancel/dismiss and
  number/arrow/Enter/Escape semantics. Deactivate controls when the request
  resolves/expires and keep drafts/focus keyed to its request ID.
  Never map arbitrary protocol options onto Ely's Once/Always/Deny enum.

  Place the bounded dock above queue/composer; allow long command/option text
  to scroll/wrap within it. Keep pending plan approval in the same flow.
  Render resolved/expired/dismissed outcomes in the transcript; restored
  entries cannot recreate a live request.

- [ ] **Step 4: Verify existing request workflows.**

  Rerun the new tests, then existing filters `a_permission_prompt_answers_both_ways`,
  `an_unrenderable_permission_can_be_dismissed`, `permission_wait_disables_the_composer_and_shows_its_own_placeholder`,
  `long_permission_option_labels_stay_inside_the_dock`, `a_long_command_scrolls_inside_the_dock_instead_of_growing_it`,
  and `a_queued_question_is_not_armed_before_it_is_drawn`. Capture pending and
  answered requests in a narrow native pane; composer/request actions remain reachable.

- [ ] **Step 5: Commit.**

  Commit `feat: render chat requests through Ely protocol-driven slots`.

## Task 6: Convert the composer and FIFO queue

**Files:**

- Modify: `chat/{composer_view,ely_tests}.rs`, move `render_composer/render_queue` from `chat/mod.rs`.
- Modify vendored: `src/chat/{composer,attach}.rs`.

**Interfaces:**

- Add `PromptInput::custom(id: impl Into<ElementId>, editor: impl IntoElement, ready: bool, on_send: impl Fn(&mut Window, &mut App) + 'static) -> Self` and `.focused(self, focused: bool) -> Self`.
- Custom mode renders the supplied editor and existing `above/tool/on_drop/busy` slots; it does not capture Ely's `forms::Enter` or construct a second completion popup. Sirio's field handles keyboard/IME/completion.
- Existing `Chat::render_composer(&mut self, theme: &Theme, window: &mut Window, cx: &mut Context<Self>) -> AnyElement` and `render_queue(&self, theme: &Theme, cx: &Context<Self>) -> AnyElement` move to `composer_view.rs` without changing callers.
- Existing editor entity, `send`, `commit_queued_item`, queue controls, mentions/attachments and popup handlers retain their behavior.

- [ ] **Step 1: Add the input-seam regressions.**

  `ely_composer_enter_respects_completion_and_ime`: focus the real field,
  create a slash/mention choice, accept it with Enter and exercise a marked
  composition via the field's platform input path before a final Enter.

  ```rust
  assert_eq!(turns_sent_during_composition, 0);
  assert_eq!(turns_sent_while_accepting_popup, 0);
  assert_eq!(turns_sent_after_final_enter, 1);
  ```

  `ely_composer_sends_an_attachment_without_text`: attach one valid image
  to the plain fixture, click the Ely send action and inspect the fixture's
  received prompt content.

  ```rust
  assert!(prompt_text.trim().is_empty());
  assert_eq!(received_images, 1);
  assert!(chat.read_with(&cx.cx, |v, _| v.attachments.is_empty()));
  ```

  Where the GPUI test harness cannot synthesize the actual native IME
  marked-text event, test the field's composition-aware action path and
  separately record the native IME check; never substitute ordinary typing
  and call it an IME test.

- [ ] **Step 2: Run the input tests red.**

  Run `cargo test -p sirio_ui --lib ely_composer_`.
  Expected: missing custom input seam before implementation; make the
  attachment-only and completion assertions fail if Ely's default gates are used.

- [ ] **Step 3: Implement the Ely input shell around the existing field.**

  Render the existing Bezel editing controller without its redundant outer
  card inside `PromptInput::custom`, with Sirio typography and bounded grow
  behavior. Keep caret/content observers, `ChatComposer` key context, disabled
  state, image paste, context menu and completion anchoring. Use host `can_send`
  for the button and the current Enter action for idle/queue behavior.
  Stop invokes the existing cancel path and does not clear the draft.

  Place accepted context and attachment chips above the editor. Use Ely
  attachment/drop surfaces while preserving accepted file types, size/error
  checks and wait/offline restrictions. Preserve image-only eligibility.
  Render the queue above the shell with count, fold, remove, clear-all and
  send-now actions. Preserve queued-item order and the current payload semantics;
  this task does not redesign queue storage. Mode/model/effort controls become
  host tool slots; Task 7 converts their popup contents.

- [ ] **Step 4: Verify submission, editing and queue behavior.**

  Rerun the new tests and existing filters `enter_during_a_stream_queues_and_the_turn_end_sends_it_exactly_once`,
  `a_second_enter_appends_to_the_queue_and_turn_ends_drain_it_in_order`,
  `clear_all_empties_the_queue`, `the_queue_header_folds_the_entries_and_keeps_the_count`,
  `offline_enter_never_discards_the_typed_draft`, `pasting_an_image_into_the_composer_attaches_it_and_sends_it_to_the_agent`,
  and `dropping_external_files_is_refused_during_permission_wait`.
  Also run the existing send-now/removal/cancel cases in the chat test module.
  Inspect a native narrow composer with a long draft, several chips and a
  pending request; the input/stop controls remain usable.

- [ ] **Step 5: Commit.**

  Commit `feat: use Ely for the chat composer and queue presentation`.

## Task 7: Convert catalogue controls, chat menus and history

**Files:**

- Create: `chat/{controls,menus,history}.rs`.
- Modify: `chat/{mod,composer_view,transcript,ely_tests}.rs`.
- Use vendored: `src/menus/{menu,hosts}.rs`, required forms/overlay primitives.

**Interfaces:**

- `Chat::render_agent_controls(&self, theme: &Theme, window: &mut Window, cx: &Context<Self>) -> AnyElement` in `controls.rs`, consumed by Task 6's tool slots.
- Move existing `render_composer_context_menu` / `render_transcript_context_menu` to `menus.rs` with their current signatures and handlers.
- `Chat::render_chat_menu(&self, theme: &Theme, cx: &Context<Self>) -> AnyElement` and `render_history(&self, theme: &Theme, cx: &Context<Self>) -> AnyElement` frame the existing overflow/history state.
- Keep model-search field, filters, focus handles, catalogue operations, usage data and persisted history operations.

- [ ] **Step 1: Extend the existing behavior tests for responsive popup rendering.**

  Use long agent-reported model/mode/effort labels at 430 px pane width;
  open a popup, navigate/select with the keyboard, then Escape to the composer.
  Add `ely_narrow_catalogue_popup_keeps_send_and_focus_reachable`:

  ```rust
  assert!(pane_bounds.contains_bounds(&send_bounds));
  assert!(pane_bounds.contains_bounds(&selected_row_bounds));
  assert_eq!(selected_model_id, advertised_model_id);
  assert!(composer_focus_after_escape);
  ```

  Extend the existing history delete-confirmation test to exercise the Ely
  row after truncating a long title; opening a session must still bind its
  existing persistence target. Avoid new tests that merely enumerate colors/components.

- [ ] **Step 2: Run the responsive behavior test red.**

  Run `cargo test -p sirio_ui --lib ely_narrow_catalogue_popup_`.
  Expected: the missing Ely popup integration or overflow/focus assertion fails.

- [ ] **Step 3: Implement Ely controls, popup contents and history surfaces.**

  Use reported mode/model/effort names and values, including optimistic
  selections and current error rollback. Keep model search separate from
  the prompt draft. Preserve supported fast/thinking/background options.
  Keep unknown context usage distinct from zero and preserve the existing
  usage breakdown popover and warning threshold.

  Wrap control rows at narrow widths; truncate labels with full-label
  tooltips while retaining accessible names. Use Ely menu/popover primitives
  around existing filters/actions; preserve anchoring, occlusion, focus
  restoration and terminal shortcut propagation. Migrate chat right-click
  cut/copy/paste/select-all surfaces as well as the header's overflow menu.

  Preserve file following, new conversation, history open/delete confirmation,
  and the current history persistence rebind. Use stable session identities
  in history rows. Empty/unavailable catalogues show the honest agent badge;
  no chevron/action claims a capability not reported by the agent.

- [ ] **Step 4: Verify controls and historical state.**

  Run the new test and existing filters `model_picker_search_filters_and_badges_the_recommended_model`,
  `model_search_is_a_text_field_that_never_touches_the_draft`,
  `context_chip_says_unknown_rather_than_zero_when_nothing_was_reported`,
  `partial_context_usage_without_a_window_size_is_not_zero`,
  `a_long_chat_history_title_does_not_push_delete_out_of_the_row`,
  `overflow_menu_toggles_follow_and_resets_to_a_new_conversation`,
  `composer_right_click_menu_offers_cut_copy_paste_on_the_field` and
  `transcript_right_click_menu_copies_and_selects_all`.
  Inspect native light/dark, small/large UI scale and narrow captures; verify
  advertised controls, history confirmation and keyboard focus visually.

- [ ] **Step 5: Commit.**

  Commit `feat: adopt Ely chat controls menus and history`.

## Task 8: Verify the complete native chat migration

**Files:**

- Create: `Scripts/Tests/test-ely-chat-ui-e2e.py` and `docs/testing/ely-agent-chat.md`.
- Create: `rust/crates/sirio_persistence/examples/ely_chat_seed.rs`.
- Extend: `rust/crates/sirio_ui/tests/fixtures/chat_fixture.py` only for capture scenarios not already represented.
- Modify: the affected UI modules only to fix failures found by this task.
- Update: `rust/vendor/ely-gpui-component/LOCAL-CHANGES.md` with final compatibility evidence.

**Interfaces:**

- `test-ely-chat-ui-e2e.py --out-dir PATH [--state-only] [--display DISPLAY]` runs isolated actual Sirio instances and retains readbacks/logs/frames. It never touches the user's normal DB, credentials or process.
- Script helper `request(socket_path: Path, method: str, params: dict[str, str]) -> dict` uses the existing newline JSON control protocol; every parameter is a string.
- Seed example CLI: `ely_chat_seed --database PATH --worktree PATH --appearance light|dark --ui-size SIZE`. Its `fn seed_chat(database: &Path, worktree: &Path, appearance: AppearanceMode, ui_size: i64) -> Result<(), PersistenceError>` uses public persistence APIs. The CLI rejects an existing database before calling it.
- Existing methods: `surface.chat.open` (select/read an already rendered chat), `.compose/.send` with `surfaceId,text`, `.permission` with `surfaceId,requestId,optionId`, and `.stop/.read` with `surfaceId`. There are no new product control methods or invented `sirioctl surface chat` subcommands.

- [ ] **Step 1: Add the reproducible real-app runner.**

  Model isolation and PID-matched X11 capture after `test-forge-ui-e2e.sh`.
  Use scratch `SIRIO_DB`, `SIRIO_SOCKET`, `SIRIO_CREDENTIALS` and an executable
  wrapper pointing `SIRIO_ACP_PROGRAM` to the existing Python fixture/mode.
  Initialize a scratch Git repository and invoke `ely_chat_seed` before
  launch. The Rust example uses `AppDatabase::open`, the existing
  `ProjectRecord/WorktreeRecord/TabRecord` constructors, `stable_worktree_id`,
  and `save_project/save_worktree/save_tabs/save_sidebar_state/save_settings`.
  Persist one selected worktree and active `kind = "chat"` tab. Leave its
  `agent_id` unset so restore uses the explicit `SIRIO_ACP_PROGRAM` fixture
  rather than a provider adapter. Set appearance, `ui_font_size` and disable
  updates through `AppSettings`; no schema or production API changes.
  Document that `surface.chat.open` selects a rendered chat and does not
  create one. The fixture run does not establish provider-logo identity;
  explicit-identity native probes below supply that evidence.

  Exercise staged streaming/queue, permission/question/plan, cancellation,
  death/recovery, authentication and restore. Store the actual control
  responses, fixture traffic, process exit status and PNGs under `--out-dir`.
  On X11, use the existing PID-matched `xwininfo/xprop/import` capture pattern;
  `--state-only` explicitly omits frames. For other native hosts use the
  documented manual capture lane, without claiming the X11 script covers them.

- [ ] **Step 2: Check the runner's failure discrimination.**

  Against the isolated fixture, intentionally request an absent surface and
  assert a failed control response. Verify a missing display fails the
  capture lane with a clear reason while `--state-only` still exercises
  state. Corrupt the expected fixture answer in a scratch run and confirm
  the runner exits nonzero; restore it before the accepted run.

- [ ] **Step 3: Run the focused complete build/test gate.**

  ```bash
  cargo build -p sirio --bin sirio -p sirio_control --bin sirioctl
  cargo build -p sirio_persistence --example ely_chat_seed
  cargo test -p sirio_ui --lib chat::
  cargo test -p sirio_acp --test chat_integration
  cargo test -p sirio_control --test control_integration
  cargo test -p sirio --bin sirio restored_chat
  cargo fmt --check -p sirio_ui -p sirio -p sirio_persistence -p ely-gpui-component
  cargo clippy -p sirio_ui -p sirio --all-targets -- -D warnings
  ```

  Building `sirio` requires Zig exactly 0.15.2 and the documented Linux
  GTK/WebKit development prerequisites. Treat a prerequisite failure as
  missing evidence and resolve it within the authorized environment; do not
  change the GPUI/Ghostty pins or suppress failing checks to obtain a green result.
  If the branch has a relevant pre-existing lint failure, record its exact
  location and separate it from migration failures.

- [ ] **Step 4: Run native state and visual checks.**

  From the repository root:

  ```bash
  python3 Scripts/Tests/test-ely-chat-ui-e2e.py --state-only --out-dir /tmp/sirio-ely-chat-state
  python3 Scripts/Tests/test-ely-chat-ui-e2e.py --out-dir /tmp/sirio-ely-chat-native
  ```

  Accepted output states that all requested scenarios passed and lists
  actual artifact paths; capture checks reject blank/wrong-PID images.
  Review the spec's entire §9 matrix. Supplement fixture-driven frames with
  actual pointer/keyboard interaction for selection, IME, menus, nested
  scrolling, turn rail and rewind. Check all five supported agent marks
  using native views with explicitly supplied identities; capability controls
  still come from the fixture/agent data. Record the source of each frame.

  Use a long transcript to compare streaming redraws/selection/scroll anchoring
  with the baseline. If a concrete regression remains, use existing static,
  content-free performance traces to localize it; no speculative benchmark suite.

- [ ] **Step 5: Record platform evidence and resolve findings.**

  On available native macOS/Windows hosts run `cargo check -p sirio --bin sirio`
  and the relevant chat tests, then capture light/dark and narrow/wide input
  states. List host/toolchain/commands and explicit unverified states for
  unavailable platforms in `docs/testing/ely-agent-chat.md`. A cross-target
  source check is not a native rendering/input result. Resolve every concrete
  behavioral/visual regression found before branch completion; request an
  architectural amendment only if a fix exceeds the approved compatibility scope.

- [ ] **Step 6: Review and commit the verified result.**

  Use `superpowers:verification-before-completion` and the chosen execution
  method's required fresh review. Audit all spec sections against the task
  coverage below, the dependency tree and the final diff. Update vendor
  deviations and record the exact checks that ran. Commit
  `test: verify the native Ely agent chat migration`.
  Do not push, publish or run the two protected CI scripts as part of this gate.

## Coverage and handoff

| Spec requirement | Tasks |
|---|---|
| §0–§2 scope, single GPUI, native compatibility, provenance | 1, 8 |
| §2 initialization/assets and §3 theme/identity | 1, 2 |
| §4 layout, responsive input and keyboard behavior | 3, 5, 6, 7, 8 |
| §5 all entry variants, rich activity, selection, navigation and rewind | 3, 4, 8 |
| §6 requests, queue, composer, advertised controls and usage | 5, 6, 7 |
| §7 focused modules, controlled seams and state ownership | 2–7 |
| §8 failure, authentication, historical/restored state | 3, 5, 7, 8 |
| §9 tests, native captures, performance and honest platform evidence | 1–8 |
| §10 review boundaries and execution gate | This handoff and Task 1's compatibility stop condition |

Self-review this plan for spec coverage, complete interfaces, executable
commands, meaningful test outcomes and the five Review Focus cases before
presenting it. No product code was written while preparing the plan.

**Recommended execution method:** Native. The tasks share the same `Chat`
entity and rendering interfaces; one implementer can carry those contracts
through the migration with less repeated context, followed by the required
fresh whole-branch review. Subagent-driven execution remains an option if
the user prefers a fresh implementation/review context for every task.

The user reviews this document and selects the execution method before Task 1
starts. On approval, preserve that selection and follow the corresponding
execution skill; do not request it again during routine implementation.
