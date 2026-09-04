# Bezel Tool Calls on step_row — Implementation Plan (sub-project 2 of 4)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Render every tool call in the chat transcript as bezel's `step_row` inside the gallery's bordered run box, with same-verb folds, measured durations, and `step_output` for text output — the gallery's "Tool calls" page, over Sirio's real ACP data.

**Architecture:** A new `rust/crates/sirio_ui/src/chat/tool_calls.rs` holds the pure helpers (icon/verb per kind, `took`, verb folding by `chunk_by`) and the `impl Chat` rendering of rows, run boxes and fold headers; `chat/mod.rs` keeps ingestion and gains a `tool_started` clock and a `duration_ms` on tool-call entries, mirrored in `sirio_persistence`. The current card header, the "N steps" group renderer and the compact collapsed rows are deleted; the diff preview, file links and edit summary under a row stay Sirio's.

**Tech Stack:** Rust, gpui (`bezel-gpui =0.3.8`), bezel `=0.1.4` (`ui::widgets::{Status, Layout}`, `ui::widgets::step_row_hover`, `ui::icons`, `theme::Theme`), `#[gpui::test]` visual tests.

**Spec:** `docs/superpowers/specs/2026-09-04-bezel-agent-parity-design.md` — section "Sub-project 2 — Tool calls" (and "Cross-cutting constraints"). Read it first.

## Global Constraints

- Branch from the tip of `feat/composer-bezel-textfield` (sub-project 1, not yet merged) or from `main` once it is merged — whichever the orchestrator names in the brief. bezel pinned `=0.1.4`, gpui `=0.3.8`: no dependency changes. Reference: tag `v0.1.4` of `crabtalk/bezel`, `apps/gallery/src/patterns/agent.rs` lines 319–608 (the `ToolCalls` section) and the `tool` / `work` fns in `apps/gallery/src/patterns/transcript.rs`.
- Iterate with `cargo test -p sirio_ui chat::` and `cargo clippy -p sirio_ui --all-targets`. **Never** run `Scripts/ci.sh` or `Scripts/ci-linux.sh`. Never push.
- Windows toolchain: PowerShell `$env:PATH = "C:\Users\enzop\.cargo\bin;D:\toolchains\zig\zig-x86_64-windows-0.15.2;" + $env:PATH`, then `cd <worktree>\rust`. Kill `sirio.exe` before `cargo build -p sirio`.
- Selectors preserved verbatim: `tool-call-toggle-N`, `tool-call-location-N-M`, `subagent-task-toggle-N`, `subagent-tool-call-toggle-N-M`, `edit-summary-*`, `tool-diff-*`, `chat-entry`. New selectors introduced here: `tool-run-<first index>`, `tool-fold-<first index>`, `tool-output-<index>-<ordinal>`.
- Persistence changes are additive (`#[serde(default)]`); a database written by today's build restores unchanged.
- The three baseline red chat tests on Windows (`auth_required_retry_survives_long_guidance_text_at_running_width`, `failed_launch_can_retry_and_complete`, `a_disconnected_agent_offers_restart_agent_not_retry`) stay red; judge failures as a list against the untouched branch tip.
- Commit messages: Conventional Commits, lower-case imperative subject, no trailers.
- `rustfmt --edition 2024` on `chat/mod.rs` and `chat/tool_calls.rs` before every commit (never `cargo fmt` on the whole crate).

---

### Task 1: Pure helpers — icon, verb, duration, verb folds

**Files:**
- Create: `rust/crates/sirio_ui/src/chat/tool_calls.rs`
- Modify: `rust/crates/sirio_ui/src/chat/mod.rs` (add `mod tool_calls;` beside `mod composer_view;`)

**Interfaces:**
- Produces (`pub(crate)` in `chat::tool_calls`):
  - `fn tool_icon(kind: &str) -> &'static str` — a `bezel::ui::icons` asset path.
  - `fn tool_verb(kind: &str) -> String` — the kind word with its first letter upper-cased; `"Tool"` for empty or `"tool"`.
  - `fn took(ms: u64) -> String` — `412ms` under a second, `1.4s` from a second up.
  - `fn is_failed_status(status: &str) -> bool` — `failed`/`cancelled`/`canceled`, case-insensitive.
  - `fn verb_folds(kinds: &[&str]) -> Vec<(usize, usize)>` — `(start, len)` per consecutive same-kind run, in order (`chunk_by`).

- [ ] **Step 1: Write the module with its tests**

```rust
//! Tool calls as the gallery draws them: one `step_row` per call, a bordered
//! box per run of consecutive calls, and a `Verb · N` fold for consecutive
//! calls of the same verb — `crabtalk/bezel` tag `v0.1.4`,
//! `apps/gallery/src/patterns/agent.rs` (the `ToolCalls` section).
//!
//! The library never learns what a tool is: `step_row` takes strings, and the
//! grouping is `slice::chunk_by`. What Sirio adds is the mapping from the
//! protocol's `kind` to an icon and a verb, and a clock for the duration.

use bezel::ui::icons;

/// The icon for a protocol tool kind (`Read`, `Edit`, `Execute`, …).
pub(crate) fn tool_icon(kind: &str) -> &'static str {
    match kind.to_ascii_lowercase().as_str() {
        "read" => icons::BOOK,
        "edit" => icons::PEN,
        "execute" => icons::TERMINAL,
        "search" => icons::MAGNIFER,
        "fetch" => icons::DOWNLOAD,
        "think" => icons::CPU,
        "delete" => icons::TRASH_BIN_MINIMALISTIC,
        "move" => icons::ARROW_RIGHT,
        _ => icons::WIDGET,
    }
}

/// The verb a row leads with: the kind word, capitalised; `Tool` when the
/// protocol gave none (an empty kind, or the generic `tool` a pre-#168
/// database restores).
pub(crate) fn tool_verb(kind: &str) -> String {
    let kind = kind.trim();
    if kind.is_empty() || kind.eq_ignore_ascii_case("tool") {
        return "Tool".to_string();
    }
    let mut chars = kind.chars();
    let first = chars.next().map(|c| c.to_uppercase().to_string()).unwrap_or_default();
    format!("{first}{}", chars.as_str())
}

/// `412ms` under a second, `1.4s` over it — a figure you read at a glance
/// rather than count digits in (gallery `took`).
pub(crate) fn took(ms: u64) -> String {
    if ms < 1000 {
        format!("{ms}ms")
    } else {
        format!("{:.1}s", ms as f32 / 1000.0)
    }
}

/// Whether a status reads as a failure — tinted red on the row.
pub(crate) fn is_failed_status(status: &str) -> bool {
    matches!(
        status.to_ascii_lowercase().as_str(),
        "failed" | "cancelled" | "canceled"
    )
}

/// Consecutive same-verb runs as `(start, len)` — the gallery's grouping,
/// straight out of std.
pub(crate) fn verb_folds(kinds: &[&str]) -> Vec<(usize, usize)> {
    let mut folds = Vec::new();
    let mut start = 0;
    for run in kinds.chunk_by(|a, b| a.eq_ignore_ascii_case(b)) {
        folds.push((start, run.len()));
        start += run.len();
    }
    folds
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icons_follow_the_kind_and_fall_back_to_widget() {
        assert_eq!(tool_icon("Read"), icons::BOOK);
        assert_eq!(tool_icon("edit"), icons::PEN);
        assert_eq!(tool_icon("Execute"), icons::TERMINAL);
        assert_eq!(tool_icon("Search"), icons::MAGNIFER);
        assert_eq!(tool_icon("Fetch"), icons::DOWNLOAD);
        assert_eq!(tool_icon("Think"), icons::CPU);
        assert_eq!(tool_icon("Delete"), icons::TRASH_BIN_MINIMALISTIC);
        assert_eq!(tool_icon("Move"), icons::ARROW_RIGHT);
        assert_eq!(tool_icon("Other"), icons::WIDGET);
        assert_eq!(tool_icon(""), icons::WIDGET);
    }

    #[test]
    fn verbs_are_the_kind_word_or_tool() {
        assert_eq!(tool_verb("Read"), "Read");
        assert_eq!(tool_verb("execute"), "Execute");
        assert_eq!(tool_verb("tool"), "Tool");
        assert_eq!(tool_verb(""), "Tool");
    }

    #[test]
    fn took_reads_at_a_glance() {
        assert_eq!(took(0), "0ms");
        assert_eq!(took(412), "412ms");
        assert_eq!(took(999), "999ms");
        assert_eq!(took(1000), "1.0s");
        assert_eq!(took(1412), "1.4s");
        assert_eq!(took(61_000), "61.0s");
    }

    #[test]
    fn failed_statuses_are_failed_and_cancelled() {
        assert!(is_failed_status("failed"));
        assert!(is_failed_status("Cancelled"));
        assert!(is_failed_status("canceled"));
        assert!(!is_failed_status("completed"));
        assert!(!is_failed_status("in_progress"));
    }

    #[test]
    fn verb_folds_are_consecutive_same_kind_runs() {
        assert_eq!(verb_folds(&[]), vec![]);
        assert_eq!(verb_folds(&["Read"]), vec![(0, 1)]);
        assert_eq!(
            verb_folds(&["Read", "Read", "read", "Execute", "Read"]),
            vec![(0, 3), (3, 1), (4, 1)]
        );
    }
}
```

Add `mod tool_calls;` to `chat/mod.rs` right after `mod composer_view;`.

- [ ] **Step 2: Run the tests**

Run: `cargo test -p sirio_ui chat::tool_calls 2>&1 | tail -8`
Expected: 5 passed.

- [ ] **Step 3: Commit**

```bash
git add rust/crates/sirio_ui/src/chat/tool_calls.rs rust/crates/sirio_ui/src/chat/mod.rs
git commit -m "feat(chat): tool call icon, verb, duration and fold helpers"
```

---

### Task 2: Measure and persist a tool call's duration

**Files:**
- Modify: `rust/crates/sirio_ui/src/chat/mod.rs` — `Entry::ToolCall` and `SubagentToolCall` (add `duration_ms: Option<u64>`), `struct Chat` (add `tool_started: HashMap<String, std::time::Instant>`), `Chat::new`, the `AcpEvent::ToolCallStarted`, `ToolCallUpdated`, `ToolCallCompleted` arms, `persisted_entry` and the restore arm (`ChatEntry::ToolCall { .. } => Entry::ToolCall { .. }`), every `Entry::ToolCall { … }` literal in tests (add `duration_ms: None`).
- Modify: `rust/crates/sirio_persistence/src/model.rs` — `ChatEntry::ToolCall` gains `#[serde(default)] duration_ms: Option<u64>`.

**Interfaces:**
- Produces: `Entry::ToolCall { …, duration_ms: Option<u64> }`, `SubagentToolCall { …, duration_ms: Option<u64> }`, `ChatEntry::ToolCall { …, duration_ms: Option<u64> }`, `Chat::note_tool_started(&mut self, id: &str)`, `Chat::tool_duration_on(&mut self, id: &str, status: &str) -> Option<u64>` (consumes the start on a terminal status).

- [ ] **Step 1: Write the failing tests**

In `chat/mod.rs` `mod tests`:

```rust
    /// A call's duration is the wall clock between its start and its first
    /// terminal status, and it survives a restart; a call restored from a
    /// database written before the field has none.
    #[gpui::test]
    async fn a_tool_call_measures_its_duration_and_persists_it(cx: &mut TestAppContext) {
        cx.update(Theme::init);
        cx.update(bezel::ui::input::init);
        let (chat, cx) = cx.add_window_view(|_, cx| Chat::new(None, std::env::temp_dir(), cx));
        chat.update(cx, |chat, cx| {
            chat.handle_event(
                AcpEvent::ToolCallStarted {
                    id: "t1".into(),
                    title: "cargo test".into(),
                    status: "in_progress".into(),
                    kind: "Execute".into(),
                    content: vec![],
                    locations: vec![],
                    raw_input: None,
                    raw_output: None,
                },
                cx,
            );
            assert!(
                matches!(chat.entries.last(), Some(Entry::ToolCall { duration_ms: None, .. })),
                "no duration while the call runs"
            );
            chat.handle_event(
                AcpEvent::ToolCallCompleted {
                    id: "t1".into(),
                    status: "completed".into(),
                    kind: None,
                    content: None,
                    locations: None,
                    raw_input: None,
                    raw_output: None,
                },
                cx,
            );
        });
        let duration = chat.read_with(cx, |chat, _| match chat.entries.last() {
            Some(Entry::ToolCall { duration_ms, .. }) => *duration_ms,
            other => panic!("expected a tool call, got {other:?}"),
        });
        assert!(duration.is_some(), "a terminal status stores the elapsed time");
        assert!(
            chat.read_with(cx, |chat, _| chat.tool_started.is_empty()),
            "the start is consumed once measured"
        );

        let persisted = chat.read_with(cx, |chat, _| persisted_entry(chat.entries.last().unwrap()));
        assert!(
            matches!(persisted, Some(ChatEntry::ToolCall { duration_ms: Some(_), .. })),
            "the duration is written to the persisted entry"
        );
        let restored = restored_entry(ChatEntry::ToolCall {
            id: "old".into(),
            title: "Read".into(),
            status: "completed".into(),
            kind: Some("Read".into()),
            locations: vec![],
            duration_ms: None,
        });
        assert!(matches!(restored, Entry::ToolCall { duration_ms: None, .. }));
    }
```

(`restored_entry` is whatever the existing restore function is called — find the arm `ChatEntry::ToolCall { .. } => Entry::ToolCall { .. }` around line 795 and use its enclosing function's name; if it is inlined in `restore_persisted_transcript`, extract it into `fn restored_entry(entry: ChatEntry) -> Entry` first.) The `ToolCallCompleted` field names must match the enum in `sirio_acp` — read `AcpEvent::ToolCallCompleted` there and adjust.

- [ ] **Step 2: Run it**

Run: `cargo test -p sirio_ui a_tool_call_measures_its_duration 2>&1 | tail -5`
Expected: FAIL to compile (`duration_ms` and `tool_started` do not exist).

- [ ] **Step 3: Implement**

`Entry::ToolCall` and `SubagentToolCall` gain `duration_ms: Option<u64>`. `Chat` gains:

```rust
    /// When each live tool call started, by protocol id, so its first
    /// terminal status can store the elapsed time on the entry. Consumed on
    /// that status; a call that never settles simply leaves its start here
    /// until the chat is dropped.
    tool_started: HashMap<String, std::time::Instant>,
```

(initialised `HashMap::new()` in `Chat::new`), and:

```rust
    fn note_tool_started(&mut self, id: &str) {
        self.tool_started
            .insert(id.to_string(), std::time::Instant::now());
    }

    /// The elapsed time for `id` if `status` is terminal and the call's start
    /// is known; `None` otherwise. Consumes the start.
    fn tool_duration_on(&mut self, id: &str, status: &str) -> Option<u64> {
        if !is_terminal_tool_status(status) {
            return None;
        }
        self.tool_started
            .remove(id)
            .map(|started| started.elapsed().as_millis() as u64)
    }
```

In `ToolCallStarted`: call `self.note_tool_started(&id)` first; every pushed `Entry::ToolCall`/`SubagentToolCall` gets `duration_ms: None` — unless the started status is already terminal, in which case `duration_ms: self.tool_duration_on(&id, &status)` (a `Some(0)`-ish value; still truthful). In `ToolCallUpdated` and `ToolCallCompleted`, when a `status` arrives: `let measured = status.as_deref().and_then(|s| self.tool_duration_on(&id, s));` before the entry lookup (borrow rules), and inside the `Entry::ToolCall`/nested arms `if let Some(ms) = measured { *duration_ms = Some(ms); }` (add `duration_ms` to the destructuring). `ToolCallCompleted` always carries a status — treat it the same way.

`sirio_persistence::ChatEntry::ToolCall`: add

```rust
        /// Wall-clock milliseconds from the call's start to its terminal
        /// status, when this process measured it. `None` predates the field
        /// or means the call was restored before it settled.
        #[serde(default)]
        duration_ms: Option<u64>,
```

`persisted_entry` writes `duration_ms: *duration_ms` for `Entry::ToolCall` and `duration_ms: None` for `SubagentTask`; the restore arm copies it into the entry.

- [ ] **Step 4: Run the tests**

Run: `cargo test -p sirio_ui chat:: 2>&1 | grep -E 'FAILED|test result'` and `cargo test -p sirio_persistence 2>&1 | tail -3`
Expected: the new test passes; only the three baseline reds fail; persistence tests pass (its round-trip fixtures may need the new field — add `duration_ms: None` where a `ChatEntry::ToolCall` literal is built).

- [ ] **Step 5: Commit**

```bash
git add rust/crates/sirio_ui/src/chat/mod.rs rust/crates/sirio_persistence/src/model.rs
git commit -m "feat(chat): measure and persist a tool call's duration"
```

---

### Task 3: One call as a `step_row` with its body under it

**Files:**
- Modify: `rust/crates/sirio_ui/src/chat/tool_calls.rs` (add the `impl Chat` rendering block)
- Modify: `rust/crates/sirio_ui/src/chat/mod.rs` — `render_tool_call_card` (delete the header; the body moves), `render_entry` / the list processor (pass a `bezel::theme::Theme`), `render_subagent_tool_call_card`

**Interfaces:**
- Consumes: `tool_icon`, `tool_verb`, `took`, `is_failed_status` (Task 1); `duration_ms` (Task 2); `bezel::ui::widgets::{Status, Layout}` traits on `bezel::theme::Theme`, `bezel::ui::widgets::step_row_hover`.
- Produces (`impl Chat` in `tool_calls.rs`):
  - `fn tool_row_meta(status: &str, duration_ms: Option<u64>) -> SharedString` — `took(ms)` when measured, else the status word in lower case.
  - `fn tool_has_body(content: &[ToolCallContentInfo], locations: &[ToolCallLocationInfo]) -> bool`.
  - `fn render_tool_row(entry_index: usize, first: bool, title: &str, status: &str, kind: &str, duration_ms: Option<u64>, content: Vec<ToolCallContentInfo>, locations: Vec<ToolCallLocationInfo>, expanded: bool, edit_summary: Option<EditSummaryState>, source_start: usize, interaction: TranscriptInteraction, theme: &Theme, bezel_theme: &bezel::theme::Theme, entity: Entity<Chat>) -> AnyElement` — the row (hairline above unless `first`) plus its open body.

- [ ] **Step 1: Write the failing tests**

```rust
    /// A call is one `step_row`: icon and verb from its kind, the title as
    /// the truncating detail, the duration (or the status word) pinned right,
    /// and a chevron only when there is something to open.
    #[gpui::test]
    async fn a_tool_call_is_a_step_row_with_a_chevron_only_when_it_has_a_body(
        cx: &mut TestAppContext,
    ) {
        cx.update(Theme::init);
        cx.update(bezel::ui::input::init);
        let (_chat, cx) = cx.add_window_view(|_, cx| {
            let mut chat = Chat::new(None, std::env::temp_dir(), cx);
            let mut bare = test_tool_call("bare");
            if let Entry::ToolCall { content, locations, duration_ms, .. } = &mut bare {
                content.clear();
                locations.clear();
                *duration_ms = Some(1412);
            }
            chat.push_entry(bare);
            chat.push_entry(Entry::Assistant { text: "x".into(), document: parse_chat_markdown("x") });
            let mut full = test_tool_call("full");
            if let Entry::ToolCall { content, status, duration_ms, .. } = &mut full {
                content.push(ToolCallContentInfo::Text("hello from the tool".into()));
                *status = "failed".into();
                *duration_ms = None;
            }
            chat.push_entry(full);
            chat
        });
        refresh_frame(cx);
        assert!(cx.debug_bounds("tool-call-toggle-0").is_some());
        assert!(cx.debug_bounds("tool-call-chevron-0").is_none(), "nothing to open, no chevron");
        assert!(cx.debug_bounds("tool-call-meta-0-1.4s").is_some(), "a measured call shows its duration");
        assert!(cx.debug_bounds("tool-call-chevron-2").is_some(), "text output opens");
        assert!(cx.debug_bounds("tool-call-meta-2-failed").is_some(), "unmeasured shows the status word");
        assert!(cx.debug_bounds("tool-call-failed-2").is_some(), "a failed row is flagged");
        assert!(cx.debug_bounds("tool-output-2-0").is_none(), "closed until clicked");
        let row = cx.debug_bounds("tool-call-toggle-2").expect("row");
        cx.simulate_click(row.center(), Modifiers::none());
        refresh_frame(cx);
        assert!(cx.debug_bounds("tool-output-2-0").is_some(), "the row opens onto its output");
    }
```

(`test_tool_call(id)` already exists in the tests; extend it to set `duration_ms: None`.)

- [ ] **Step 2: Run it**

Run: `cargo test -p sirio_ui a_tool_call_is_a_step_row 2>&1 | tail -5`
Expected: FAIL (no `tool-call-chevron-*`/`tool-call-meta-*` selectors).

- [ ] **Step 3: Implement the row**

Append to `tool_calls.rs`:

```rust
use gpui::{AnyElement, Entity, SharedString, div, prelude::*, px};
use bezel::ui::widgets::{Status as _, step_row_hover};
use sirio_acp::{ToolCallContentInfo, ToolCallLocationInfo};

use super::{Chat, EditSummaryState, TranscriptInteraction, DiffPreviewContext, DiffPreviewSelection, diff_preview_lines, tool_call_plain_text, DIFF_PREVIEW_MAX_LINES};
use crate::Theme;

impl Chat {
    /// The right-aligned figure: how long the call took when this process
    /// measured it, otherwise the status word (`running`, `failed`, …) — a
    /// restored transcript has no clock to offer.
    pub(crate) fn tool_row_meta(status: &str, duration_ms: Option<u64>) -> SharedString {
        match duration_ms {
            Some(ms) => took(ms).into(),
            None => status.to_ascii_lowercase().replace('_', " ").into(),
        }
    }

    pub(crate) fn tool_has_body(
        content: &[ToolCallContentInfo],
        locations: &[ToolCallLocationInfo],
    ) -> bool {
        content
            .iter()
            .any(|item| !matches!(item, ToolCallContentInfo::Other))
            || !locations.is_empty()
    }

    /// One call: the row, then — when open — its body. `first` skips the
    /// hairline above, so a run box needs no divider of its own.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn render_tool_row(
        entry_index: usize,
        first: bool,
        title: &str,
        status: &str,
        kind: &str,
        duration_ms: Option<u64>,
        content: Vec<ToolCallContentInfo>,
        locations: Vec<ToolCallLocationInfo>,
        expanded: bool,
        edit_summary: Option<EditSummaryState>,
        source_start: usize,
        interaction: TranscriptInteraction,
        theme: &Theme,
        bezel_theme: &bezel::theme::Theme,
        entity: Entity<Chat>,
    ) -> AnyElement {
        let failed = is_failed_status(status);
        let has_body = Self::tool_has_body(&content, &locations);
        let meta = Self::tool_row_meta(status, duration_ms);
        let meta_for_id = meta.clone();
        let toggle_entity = entity.clone();
        let row = bezel_theme
            .step_row(
                tool_icon(kind),
                tool_verb(kind),
                Some(SharedString::from(title.to_string())),
                Some(meta),
                failed,
                has_body.then_some(expanded),
            )
            .id(("tool-call-toggle", entry_index))
            .debug_selector(move || format!("tool-call-toggle-{entry_index}"))
            .hover(step_row_hover)
            .on_click(move |_, _, cx| {
                toggle_entity.update(cx, |chat, cx| chat.toggle_tool_call_expanded(entry_index, cx));
            })
            // Zero-size markers: what the row *means* is testable without
            // reading pixels.
            .child(
                div()
                    .size_0()
                    .debug_selector(move || format!("tool-call-meta-{entry_index}-{meta_for_id}")),
            )
            .when(has_body, |row| {
                row.child(div().size_0().debug_selector(move || format!("tool-call-chevron-{entry_index}")))
            })
            .when(failed, |row| {
                row.child(div().size_0().debug_selector(move || format!("tool-call-failed-{entry_index}")))
            });

        let mut item = div()
            .w_full()
            .flex()
            .flex_col()
            .when(!first, |item| item.border_t_1().border_color(bezel_theme.border))
            .child(row);
        if expanded && has_body {
            item = item.child(Self::render_tool_body(
                entry_index, content, locations, edit_summary, source_start, interaction, title, status, theme, bezel_theme, entity,
            ));
        }
        item.into_any_element()
    }
}
```

`render_tool_body` is the old `render_tool_call_card` body (the `if expanded { … }` block and the edit-summary tail) moved into `tool_calls.rs`, with two changes: text content renders through `bezel_theme.step_output(("tool-output", entry_index, ordinal), text)` — give it `.debug_selector(move || format!("tool-output-{entry_index}-{ordinal}"))` — instead of `render_tool_output_text` (which is then deleted along with `truncate_tool_output` **only if** nothing else uses it; `step_output` caps by height and scrolls, so the character cap is no longer needed); diffs, locations and the edit summary keep their renderers, stacked in `div().flex().flex_col().gap(px(6.0)).px(px(12.0)).pb(px(8.0))`. `tool_call_plain_text` keeps feeding the selection offsets exactly as today.

`render_tool_call_card` is deleted; its two callers (the lone-call branch of `render_entry`, and `render_tool_call_group`) call `render_tool_row(…, first = true, …)` for now — Task 4 rebuilds the run. `render_subagent_tool_call_card` becomes a `render_tool_row` call with the `subagent-tool-call-toggle-{task}-{child}` selector kept (add a `selector: SharedString` parameter if the id format needs to differ, or key the nested rows by a distinct id tuple).

`bezel_theme`: in `render` (the list processor) add `let bezel_theme = bezel::theme::Theme::of(cx).clone();` next to `transcript_theme` and thread it through `render_entry` → the tool-call arms. The list processor closure receives `cx`; clone once per frame outside it.

- [ ] **Step 4: Run the tests**

Run: `cargo test -p sirio_ui chat:: 2>&1 | grep -E 'FAILED|test result'`
Expected: the new test passes; `tool_call_starts_collapsed_and_toggles_on_click`, `a_real_click_on_a_tool_call_location_opens_the_file`, `a_diff_preview_opens_its_file_and_its_rows_are_numbered_and_selectable`, `edit_summary_opens_and_reports_revert_success_or_error`, `following_edited_files_opens_the_tool_calls_reported_location` still pass (fix any that asserted the old header's text); baseline reds only.

- [ ] **Step 5: Commit**

```bash
git add rust/crates/sirio_ui/src/chat
git commit -m "feat(chat): tool calls as bezel step rows with step_output bodies"
```

---

### Task 4: The run box and the same-verb folds

**Files:**
- Modify: `rust/crates/sirio_ui/src/chat/tool_calls.rs` (add `render_tool_run`)
- Modify: `rust/crates/sirio_ui/src/chat/mod.rs` — `struct Chat` (add `open_verb_folds: HashSet<usize>`), the list processor's run branch, `render_tool_call_group`, `collapsed_tool_row_text`, `TOOL_CALL_GROUP_*`, `toggle_tool_call_group_expanded`, `Entry::ToolCall.group_expanded`

**Interfaces:**
- Produces: `Chat::render_tool_run(&self, members: Vec<(usize, usize, Entry)>, transcript_focus, theme, bezel_theme, entity) -> AnyElement` (the box for `start..=end`, one row per call, folds for same-verb runs of two or more), `Chat::toggle_verb_fold(&mut self, start: usize, cx)`, selectors `tool-run-<start>`, `tool-fold-<start>`.
- Removes: `Entry::ToolCall.group_expanded` (and every literal that sets it), `toggle_tool_call_group_expanded`, `render_tool_call_group`, `collapsed_tool_row_text`, `TOOL_CALL_GROUP_GAP`, `TOOL_CALL_GROUP_CHEVRON_WIDTH`, `TOOL_CALL_GROUP_MEMBER_INDENT`, `tool_group_label` stays (sub-project 4 uses it).

- [ ] **Step 1: Write the failing tests**

Replace `consecutive_tool_calls_group_under_one_toggle_and_expand_to_full_cards` and `collapsed_tool_call_rows_stay_single_line_with_long_paths` with:

```rust
    /// A run of consecutive calls is one bordered box; inside it, consecutive
    /// calls of the same verb fold under a `Verb · N` header that opens on
    /// click — `chunk_by` twice, the gallery's own finding.
    #[gpui::test]
    async fn a_run_is_one_box_and_same_verb_calls_fold(cx: &mut TestAppContext) {
        cx.update(Theme::init);
        cx.update(bezel::ui::input::init);
        let (_chat, cx) = cx.add_window_view(|_, cx| {
            let mut chat = Chat::new(None, std::env::temp_dir(), cx);
            for (id, kind) in [("a", "Read"), ("b", "Read"), ("c", "Read"), ("d", "Execute")] {
                let mut call = test_tool_call(id);
                if let Entry::ToolCall { kind: k, .. } = &mut call {
                    *k = kind.into();
                }
                chat.push_entry(call);
            }
            chat.push_entry(Entry::Assistant { text: "done".into(), document: parse_chat_markdown("done") });
            chat.push_entry(test_tool_call("e"));
            chat
        });
        refresh_frame(cx);
        let run = cx.debug_bounds("tool-run-0").expect("the four calls share one box");
        assert!(cx.debug_bounds("tool-run-5").is_some(), "the lone call after the prose is its own box");
        assert!(cx.debug_bounds("tool-run-1").is_none());
        let fold = cx.debug_bounds("tool-fold-0").expect("three Reads fold under one header");
        assert!(cx.debug_bounds("tool-call-toggle-0").is_none(), "folded members are not drawn");
        assert!(cx.debug_bounds("tool-call-toggle-3").is_some(), "the Execute row stands on its own");
        assert!(cx.debug_bounds("tool-fold-3").is_none(), "a run of one has no fold header");
        assert!(fold.top() >= run.top() && fold.bottom() <= run.bottom());

        cx.simulate_click(fold.center(), Modifiers::none());
        refresh_frame(cx);
        let member = cx.debug_bounds("tool-call-toggle-1").expect("opening the fold draws its members");
        assert!(member.left() > fold.left(), "members are indented under the header");
        cx.simulate_click(cx.debug_bounds("tool-fold-0").unwrap().center(), Modifiers::none());
        refresh_frame(cx);
        assert!(cx.debug_bounds("tool-call-toggle-1").is_none(), "clicking again folds them back");
    }

    /// A long title truncates in the row's detail slot instead of wrapping:
    /// the row stays one line tall.
    #[gpui::test]
    async fn a_tool_row_with_a_long_title_stays_one_line(cx: &mut TestAppContext) {
        cx.update(Theme::init);
        cx.update(bezel::ui::input::init);
        let (_chat, cx) = cx.add_window_view(|_, cx| {
            let mut chat = Chat::new(None, std::env::temp_dir(), cx);
            chat.push_entry(test_tool_call("short"));
            let mut long = test_tool_call("long");
            if let Entry::ToolCall { title, kind, .. } = &mut long {
                *title = format!("{}file.rs", "directory ".repeat(80));
                *kind = "Execute".into();
            }
            chat.push_entry(long);
            chat
        });
        refresh_frame(cx);
        let short = cx.debug_bounds("tool-call-toggle-0").expect("short row");
        let long = cx.debug_bounds("tool-call-toggle-1").expect("long row");
        assert_eq!(short.size.height, long.size.height, "the detail truncates, the row does not grow");
        assert!(long.right() <= cx.debug_bounds("tool-run-0").unwrap().right());
    }
```

- [ ] **Step 2: Run them**

Run: `cargo test -p sirio_ui -- a_run_is_one_box a_tool_row_with_a_long_title 2>&1 | tail -6`
Expected: FAIL (no `tool-run-*`/`tool-fold-*`).

- [ ] **Step 3: Implement the run**

`Chat` gains `open_verb_folds: HashSet<usize>` (view state; `HashSet::new()` in `new`) and

```rust
    pub(crate) fn toggle_verb_fold(&mut self, start: usize, cx: &mut Context<Self>) {
        if !self.open_verb_folds.insert(start) {
            self.open_verb_folds.remove(&start);
        }
        // The run's row is measured off its tail entry; the fold changed its height.
        if let Some((_, end)) = tool_call_run_bounds_inclusive(&self.entries, start) {
            self.remeasure_entry(end);
        }
        cx.notify();
    }
```

where `tool_call_run_bounds_inclusive` is `tool_call_run_bounds` relaxed to return `Some((index, index))` for a lone call (add it beside the old one, or change the old one and its three unit tests to the inclusive contract — the list branch then draws *every* tool call through the run path, lone or not).

In `tool_calls.rs`:

```rust
impl Chat {
    /// The box a run shares: rounded, bordered, clipping whatever it holds.
    fn run_box(bezel_theme: &bezel::theme::Theme) -> gpui::Div {
        div()
            .w_full()
            .rounded(px(bezel::theme::Theme::panel_radius()))
            .border_1()
            .border_color(bezel_theme.border)
            .overflow_hidden()
            .flex()
            .flex_col()
    }

    /// A run of consecutive calls, `members` in transcript order as
    /// `(entry index, source start, entry)`: one row per call, and one
    /// `Verb · N` header for each consecutive run of the same verb with two
    /// or more calls, its members drawn under it only while the fold is open.
    pub(crate) fn render_tool_run(
        &self,
        members: Vec<(usize, usize, Entry)>,
        transcript_focus: FocusHandle,
        theme: &Theme,
        bezel_theme: &bezel::theme::Theme,
        entity: Entity<Chat>,
    ) -> AnyElement {
        let Some((run_start, _, _)) = members.first() else {
            return div().into_any_element();
        };
        let run_start = *run_start;
        let kinds: Vec<&str> = members
            .iter()
            .map(|(_, _, entry)| match entry {
                Entry::ToolCall { kind, .. } => kind.as_str(),
                _ => "",
            })
            .collect();
        let mut children: Vec<AnyElement> = Vec::new();
        let mut first_in_box = true;
        for (offset, len) in verb_folds(&kinds) {
            let slice = &members[offset..offset + len];
            if len == 1 {
                let (index, source_start, entry) = &slice[0];
                children.push(self.tool_row_from_entry(*index, first_in_box, *source_start, entry, transcript_focus.clone(), theme, bezel_theme, entity.clone()));
                first_in_box = false;
                continue;
            }
            let fold_start = slice[0].0;
            let open = self.open_verb_folds.contains(&fold_start);
            let any_failed = slice.iter().any(|(_, _, entry)| matches!(entry, Entry::ToolCall { status, .. } if is_failed_status(status)));
            let kind = kinds[offset];
            let toggle_entity = entity.clone();
            let header = bezel_theme
                .step_row(tool_icon(kind), tool_verb(kind), Some(SharedString::from(format!("· {len}"))), None, any_failed, Some(open))
                .id(("tool-fold", fold_start))
                .debug_selector(move || format!("tool-fold-{fold_start}"))
                .hover(step_row_hover)
                .on_click(move |_, _, cx| {
                    toggle_entity.update(cx, |chat, cx| chat.toggle_verb_fold(fold_start, cx));
                });
            let mut fold = div()
                .w_full()
                .flex()
                .flex_col()
                .when(!first_in_box, |fold| fold.border_t_1().border_color(bezel_theme.border))
                .child(header);
            if open {
                let mut inner = div().w_full().flex().flex_col().border_t_1().border_color(bezel_theme.border).pl(px(16.0));
                for (position, (index, source_start, entry)) in slice.iter().enumerate() {
                    inner = inner.child(self.tool_row_from_entry(*index, position == 0, *source_start, entry, transcript_focus.clone(), theme, bezel_theme, entity.clone()));
                }
                fold = fold.child(inner);
            }
            children.push(fold.into_any_element());
            first_in_box = false;
        }
        Self::run_box(bezel_theme)
            .id(("tool-run", run_start))
            .debug_selector(move || format!("tool-run-{run_start}"))
            .children(children)
            .into_any_element()
    }
}
```

`tool_row_from_entry` destructures an `Entry::ToolCall` and calls `render_tool_row` with `self.edit_summaries.get(&index).cloned()` and a `TranscriptInteraction { chat: entity.clone(), focus: transcript_focus }` (as the old group renderer did).

The list processor's run branch: with the inclusive bounds every `Entry::ToolCall` index inside a run other than the tail returns the empty `chat-entry` row, and the tail returns `div().id(("chat-entry", entry_index)).w_full().max_w(px(TRANSCRIPT_WIDTH)).pb(px(8.0)).child(this.render_tool_run(members, …))`. Delete `render_tool_call_group`, `collapsed_tool_row_text`, the three `TOOL_CALL_GROUP_*` constants, `toggle_tool_call_group_expanded`, and `group_expanded` on `Entry::ToolCall` (every literal, including tests and `control_entry_row`). The `Entry::ToolCall` arm inside `render_entry` (lone call) is no longer reached from the list — keep it delegating to `render_tool_run` with a single member so nothing else that calls `render_entry` breaks.

- [ ] **Step 4: Run the tests**

Run: `cargo test -p sirio_ui chat:: 2>&1 | grep -E 'FAILED|test result'` and `cargo test -p sirio_ui tool_call_run_bounds 2>&1 | tail -4`
Expected: the two new tests pass; the run-bounds unit tests reflect the inclusive contract; baseline reds only. Then `cargo test -p sirio 2>&1 | grep -E 'FAILED|test result' | head` — the app crate's integration tests that read `tool-call-toggle-N` still pass (compare against the untouched tip; Windows-environment reds are a known list).

- [ ] **Step 5: Commit**

```bash
git add rust/crates/sirio_ui/src/chat
git commit -m "feat(chat): tool runs in a bordered box with same-verb folds"
```

---

### Task 5: The subagent task as a fold

**Files:**
- Modify: `rust/crates/sirio_ui/src/chat/tool_calls.rs`, `rust/crates/sirio_ui/src/chat/mod.rs` (`render_subagent_task_card`, `render_subagent_tool_call_card`)

**Interfaces:**
- Produces: `Chat::render_subagent_task(task_index, title, status, tool_calls, expanded, theme, bezel_theme, entity) -> AnyElement` — a run box whose header row is `step_row(CPU, "Task", Some(title), Some(meta), failed, Some(expanded))` with the `subagent-task-toggle-N` selector, and whose members are `render_tool_row`s keyed `subagent-tool-call-toggle-N-M` under a hairline at `pl 16`.

- [ ] **Step 1: Adjust the existing test**

`a_subagent_task_card_expands_nested_tool_calls` keeps its selectors; add two assertions: `cx.debug_bounds("subagent-task-toggle-0")` sits inside `cx.debug_bounds("tool-run-0")`, and once expanded, `subagent-tool-call-toggle-0-0` has `left()` greater than the header's `left()`.

- [ ] **Step 2: Run it**

Run: `cargo test -p sirio_ui a_subagent_task_card_expands 2>&1 | tail -4`
Expected: FAIL on the `tool-run-0` assertion.

- [ ] **Step 3: Implement**

Rewrite `render_subagent_task_card` in `tool_calls.rs` as `render_subagent_task` using `run_box`, the `Task` header row (`toggle_subagent_task_expanded` on click, `Self::tool_row_meta(&status, None)` as meta, `is_failed_status(&status)`), and the member rows through `render_tool_row` with a `selector` override for the nested id (extend `render_tool_row` with `id: gpui::ElementId` and `selector: SharedString` parameters if you have not already — the nested call keeps `("subagent-tool-call-toggle", task, child)` and its click toggling `toggle_subagent_tool_call_expanded`). Delete `render_subagent_tool_call_card`.

- [ ] **Step 4: Run the tests and commit**

Run: `cargo test -p sirio_ui chat:: 2>&1 | grep -E 'FAILED|test result'`
Expected: baseline reds only.

```bash
git add rust/crates/sirio_ui/src/chat
git commit -m "feat(chat): subagent tasks as a folded run of step rows"
```

---

### Task 6: Sweep, build, hand over

**Files:**
- Modify: `rust/crates/sirio_ui/src/chat/mod.rs`, `rust/crates/sirio_ui/src/chat/tool_calls.rs`, `docs/superpowers/specs/2026-09-04-bezel-agent-parity-design.md`

- [ ] **Step 1: Delete what nothing uses**

Run: `cargo clippy -p sirio_ui --all-targets 2>&1 | grep -E 'warning: (unused|dead_code|never used|never read)' -A3 | head -60`
Remove every item reported inside `chat/` that this sub-project orphaned (`render_tool_output_text`, `truncate_tool_output`, `TOOL_OUTPUT_MAX_CHARS` and their unit tests if nothing else uses them; `CARD_H_PADDING`/`CARD_V_PADDING` stay while the permission/plan cards use them).

- [ ] **Step 2: rustfmt the two files, run the crate, build the app**

```powershell
rustfmt --edition 2024 crates\sirio_ui\src\chat\mod.rs crates\sirio_ui\src\chat\tool_calls.rs
cargo test -p sirio_ui 2>&1 | Select-String -Pattern 'FAILED|test result'
cargo clippy -p sirio_ui --all-targets 2>&1 | Select-String -Pattern '^warning|^error' | Group-Object | Select-Object Count, Name
cargo build -p sirio 2>&1 | Select-Object -Last 2
```

Expected: baseline reds only; no new clippy warnings versus the branch tip you started from; `Finished`.

- [ ] **Step 3: Record the deviation and commit**

In the spec's "Sub-project 2 — Tool calls / Deletions" paragraph, replace the sentence about `group_expanded` staying through sub-project 2 with: `group_expanded` was removed here (nothing read it once the run box replaced the "N steps" header); sub-project 4 adds the Work zone's own `work_open`. Append to "Delivery": `Sub-project 2 landed on branch <branch> (<date>).`

```bash
git add rust/crates/sirio_ui docs/superpowers/specs/2026-09-04-bezel-agent-parity-design.md
git commit -m "chore(chat): sweep the tool-call rows and record sub-project 2"
```

Print `ALL DONE` and `git log --oneline <base>..HEAD`, then stop — the orchestrator runs the app and compares against `refs/gallery-toolcalls.jpg`.

---

## Self-review against the spec

- Row (`step_row`, icon per kind, verb, detail, meta = duration or status word, failed tint, chevron only with a body) → Tasks 1 and 3. Duration measured on the terminal status and persisted additively → Task 2. Run box with hairlines, single call boxed, same-verb folds keyed by first index (view state) → Task 4. `SubagentTask` as a fold → Task 5. Body: `step_output` for text, Sirio's diff/locations/edit summary kept → Task 3. Deletions (`render_tool_call_card` header, `render_tool_call_group`, `collapsed_tool_row_text`, `TOOL_CALL_GROUP_*`) → Tasks 3–4; `group_expanded` removed here rather than in sub-project 4 (recorded in Task 6).
- Names used across tasks: `tool_icon`, `tool_verb`, `took`, `is_failed_status`, `verb_folds`, `duration_ms`, `tool_started`, `note_tool_started`, `tool_duration_on`, `tool_row_meta`, `tool_has_body`, `render_tool_row`, `render_tool_body`, `render_tool_run`, `run_box`, `open_verb_folds`, `toggle_verb_fold`, `tool_call_run_bounds_inclusive`, `render_subagent_task`; selectors `tool-run-N`, `tool-fold-N`, `tool-output-N-M`, `tool-call-chevron-N`, `tool-call-meta-N-<meta>`, `tool-call-failed-N`.
