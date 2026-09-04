# Transcript Work Zone and Day Headings Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Split every turn the way the bezel gallery's Transcript page does — the answer is the prose after the last tool call, everything before it is interim work folded behind a `Worked · N steps` header run by `widgets::Takeover` — and print a day heading where the calendar day changes between turns.

**Architecture:** A new module `sirio_ui/src/chat/transcript.rs` holds the pure split (`split_work`), the labels (`work_label`, `day_label`), the per-frame role table (`work_roles`) and the renderers for the Work header, the member frame, interim prose and the day heading. `chat/mod.rs` keeps the virtualised `list()`, the F-CHAT-22 turn fold and the entry types; it gains `Chat.work_open` and the `at` timestamp on `Entry::User`. The list still draws one row per entry: the turn's `User` row draws the heading, the bubble and the Work header; a member row draws nothing while the zone is folded and its own content inside the zone's left-border frame while it is open — the same trick the tool-call runs already use.

**Tech Stack:** Rust, gpui, bezel `=0.1.4` (`ui::widgets::{Takeover, Layout}`, `ui::popover::tracked_upper`, `theme::ink`), chrono (already a `sirio_ui` dependency), serde.

**Spec:** `docs/superpowers/specs/2026-09-04-bezel-agent-parity-design.md`, section "Sub-project 4 — Transcript (`transcript.rs`)" and "Cross-cutting constraints".

## Global Constraints

- bezel and gpui pins unchanged; the reference is tag `v0.1.4`. No dependency changes.
- Every persistence change is additive with `#[serde(default)]`; a database written by today's build restores unchanged.
- Selectors used by `sirio`'s integration tests are preserved verbatim: `user-bubble-N`, `thought-toggle-N`, `tool-call-toggle-N`, `tool-run-N`, `tool-fold-N`, `turn-fold-*`, `assistant-copy-N`, `chat-generating-spinner`.
- Kept from Sirio, untouched: the user bubble, `TurnFooter`, the F-CHAT-22 "Turn: …" fold for older turns (`turn_row_roles`, `unfolded_turns`, `OPEN_TURN_COUNT`), the copy affordance on answers, the pending-question bar, the virtualised list and its follow logic. `scroll::follow`/`scrollbar` are **not** adopted for the transcript.
- Workspace gates only on the user's explicit request; iterate with `cargo test -p sirio_ui chat::` and `cargo clippy -p sirio_ui --all-targets`, in the worktree's own `rust/target` (never a shared `CARGO_TARGET_DIR`).
- Baseline reds on Windows in `chat::` are exactly three (`auth_required_retry_survives_long_guidance_text_at_running_width`, `failed_launch_can_retry_and_complete`, `a_disconnected_agent_offers_restart_agent_not_retry`); judge failures as a list.

## File Structure

- Create: `rust/crates/sirio_ui/src/chat/transcript.rs` — `WorkSplit`, `split_work`, `work_label`, `day_label`, `WorkRole`, `work_roles`, `impl Chat { streaming_turn_start, toggle_work, remeasure_turn, render_work_header, render_work_member, render_interim_prose, render_day_heading }`, `mod tests`.
- Modify: `rust/crates/sirio_ui/src/chat/mod.rs` — `Entry::User { text, at }`, `persisted_entry`/`restored_entry`, the send path, `turn_label`, `Chat.work_open`, the `TurnEnded` arm, the list row processor, the `Entry::User` and `Entry::Assistant` arms of `render_entry`, tests.
- Modify: `rust/crates/sirio_persistence/src/model.rs` — `ChatEntry::UserMessage { text, at }`.
- Modify (compile-only): every other `ChatEntry::UserMessage { text }` constructor (`rust/crates/sirio_acp/src/chat.rs`, `rust/crates/sirio_persistence/tests/persistence_integration.rs`; grep) and every `Entry::User(` in `sirio_ui` (44 in `chat/mod.rs`; grep the crate).
- Modify (Task 7 only): `docs/superpowers/specs/2026-09-04-bezel-agent-parity-design.md` — one Delivery line.

Reference: the gallery's `apps/gallery/src/patterns/transcript.rs` (`split`, `work_header`, `work`, `Render`) in the session scratchpad's sparse clone; bezel 0.1.4 `widgets/mod.rs` (`Takeover`), `widgets/layout.rs` (`Layout::disclosure`), `popover.rs` (`tracked_upper`, line ~914) under `C:\Users\enzop\.cargo\registry\src\index.crates.io-1949cf8c6b5b557f\bezel-ui-0.1.4\src\`. Sirio's existing turn machinery: `segment_turns`, `TurnSegment`, `turn_row_roles`, `TurnRowRole` in `chat/mod.rs` (~line 7283 on). When a snippet below and the crate's real signature disagree, the crate wins.

---

### Task 1: Pure split, labels and the role table in `chat/transcript.rs`

**Files:**
- Create: `rust/crates/sirio_ui/src/chat/transcript.rs`
- Modify: `rust/crates/sirio_ui/src/chat/mod.rs` (`mod transcript;` next to `mod thought;`; make `TurnSegment`, `segment_turns` `pub(crate)` if they are private)

**Interfaces:**
- Produces:
  - `pub(crate) struct WorkSplit { pub answer_from: usize, pub steps: usize }`
  - `pub(crate) fn split_work(entries: &[Entry], turn: &TurnSegment) -> WorkSplit`
  - `pub(crate) fn work_label(steps: usize) -> String` (`Worked · 1 step` / `Worked · N steps`)
  - `pub(crate) fn day_label(at: chrono::DateTime<chrono::Local>, now: chrono::DateTime<chrono::Local>) -> String` (`Today`, `Yesterday`, else `%a %-d %b`)
  - `#[derive(Clone, Debug, PartialEq, Eq)] pub(crate) enum WorkRole { Outside, Header { turn: usize, steps: usize, open: bool }, Member { turn: usize, open: bool } }`
  - `pub(crate) fn work_roles(entries: &[Entry], work_open: &HashMap<usize, Takeover>, streaming_turn: Option<usize>) -> Vec<WorkRole>`

- [ ] **Step 1: Write the failing tests**

```rust
//! The transcript as the gallery's Transcript page draws it: a turn is a
//! question and everything up to the next one; **the answer is the prose
//! after the last tool call, everything before it is interim** — that
//! sentence is the whole rule (`apps/gallery/src/patterns/transcript.rs`,
//! `split`). The interim half folds behind `Worked · N steps`, run by
//! `widgets::Takeover`: open while the turn streams, folded once it ends,
//! the reader's press holding from then on. A day heading marks where the
//! calendar day changes between turns.

use std::collections::HashMap;

use bezel::ui::widgets::Takeover;
use chrono::{DateTime, Datelike, Local};

use super::{Entry, TurnSegment};

/// One turn's zones: entries before `answer_from` are interim work, prose
/// at or after it is the answer. `steps` is how much happened — each tool
/// call is one, a subagent task is one plus its nested calls.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct WorkSplit {
    pub answer_from: usize,
    pub steps: usize,
}

/// `rposition` of the last tool in the turn — `Transcript.svelte`'s whole
/// `splitTurn`. A turn without tools is all answer.
pub(crate) fn split_work(entries: &[Entry], turn: &TurnSegment) -> WorkSplit {
    todo!()
}

/// The Work header's word (the old `tool_group_label`).
pub(crate) fn work_label(steps: usize) -> String {
    todo!()
}

/// Sirio's words for a day — bezel carries no clock. `Today`, `Yesterday`,
/// otherwise `Mon 2 Sep`.
pub(crate) fn day_label(at: DateTime<Local>, now: DateTime<Local>) -> String {
    todo!()
}

/// What the list draws at one index once the Work zones are applied.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum WorkRole {
    /// Not part of a zone: the answer, footers, permissions, plans, errors,
    /// and the members of a turn that has no tool calls.
    Outside,
    /// The turn's first entry: draws the heading, the bubble and — when the
    /// turn has steps — the Work header.
    Header { turn: usize, steps: usize, open: bool },
    /// An interim entry of a turn with steps: an empty row while the zone is
    /// folded, its own content inside the zone's frame while open.
    Member { turn: usize, open: bool },
}

/// One role per entry, resolved once per frame. `streaming_turn` is the
/// start index of the turn still being answered, if any: its zone's `auto`
/// is `true`, every other turn's is `false`.
pub(crate) fn work_roles(
    entries: &[Entry],
    work_open: &HashMap<usize, Takeover>,
    streaming_turn: Option<usize>,
) -> Vec<WorkRole> {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat::{parse_chat_markdown, segment_turns};
    use chrono::TimeZone;

    fn user(text: &str) -> Entry {
        Entry::User { text: text.into(), at: None }
    }
    fn prose(text: &str) -> Entry {
        Entry::Assistant { text: text.into(), document: parse_chat_markdown(text) }
    }
    fn tool(id: &str) -> Entry {
        crate::chat::tests::test_tool_call(id)
    }
    fn thought() -> Entry {
        Entry::Thought { text: "…".into(), open: Default::default(), started: None, duration_ms: None }
    }

    #[test]
    fn the_last_tool_call_decides_where_the_answer_starts() {
        let entries = vec![user("q"), thought(), prose("interim"), tool("a"), tool("b"), prose("answer"), Entry::TurnFooter("12:00".into())];
        let turns = segment_turns(&entries);
        assert_eq!(split_work(&entries, &turns[0]), WorkSplit { answer_from: 5, steps: 2 });
    }

    #[test]
    fn a_turn_without_tools_is_all_answer() {
        let entries = vec![user("q"), thought(), prose("answer"), Entry::TurnFooter("12:00".into())];
        let turns = segment_turns(&entries);
        assert_eq!(split_work(&entries, &turns[0]), WorkSplit { answer_from: 0, steps: 0 });
    }

    #[test]
    fn a_subagent_task_counts_as_a_tool_with_its_members() {
        let mut task = tool("t");
        // Build a SubagentTask with two nested calls the way `handle_event`
        // does (see `is_subagent_tool_call`); mirror the fields in scope.
        task = Entry::SubagentTask { /* id, title, status, tool_calls: vec![two SubagentToolCall], expanded: false — copy the exact field list from the enum */ ..todo!() };
        let entries = vec![user("q"), task, prose("answer")];
        let turns = segment_turns(&entries);
        let split = split_work(&entries, &turns[0]);
        assert_eq!(split.answer_from, 2);
        assert_eq!(split.steps, 3, "the task and its two members");
    }

    #[test]
    fn work_labels_count_steps() {
        assert_eq!(work_label(1), "Worked · 1 step");
        assert_eq!(work_label(4), "Worked · 4 steps");
    }

    #[test]
    fn day_labels_are_sirios_words() {
        let now = Local.with_ymd_and_hms(2026, 9, 4, 22, 0, 0).unwrap();
        assert_eq!(day_label(now, now), "Today");
        assert_eq!(day_label(Local.with_ymd_and_hms(2026, 9, 3, 1, 0, 0).unwrap(), now), "Yesterday");
        assert_eq!(day_label(Local.with_ymd_and_hms(2026, 9, 1, 9, 0, 0).unwrap(), now), "Tue 1 Sep");
    }

    #[test]
    fn roles_follow_the_split_and_the_takeover() {
        let entries = vec![user("q"), thought(), tool("a"), prose("answer"), Entry::TurnFooter("t".into()), user("q2"), prose("plain")];
        let none = HashMap::new();
        let roles = work_roles(&entries, &none, Some(5));
        assert_eq!(roles[0], WorkRole::Header { turn: 0, steps: 1, open: false });
        assert_eq!(roles[1], WorkRole::Member { turn: 0, open: false });
        assert_eq!(roles[2], WorkRole::Member { turn: 0, open: false });
        assert_eq!(roles[3], WorkRole::Outside, "the answer is outside the zone");
        assert_eq!(roles[4], WorkRole::Outside);
        assert_eq!(roles[5], WorkRole::Header { turn: 5, steps: 0, open: false }, "no tools: a header row with no header");
        assert_eq!(roles[6], WorkRole::Outside);

        let streaming = work_roles(&entries, &none, Some(0));
        assert_eq!(streaming[1], WorkRole::Member { turn: 0, open: true }, "the streaming turn's zone is open by itself");

        let mut pressed = HashMap::new();
        let mut t = Takeover::default();
        t.toggle(false);
        pressed.insert(0, t);
        let held = work_roles(&entries, &pressed, None);
        assert_eq!(held[1], WorkRole::Member { turn: 0, open: true }, "a press holds");
    }
}
```

`test_tool_call` lives in `chat/mod.rs`'s test module; make it `pub(crate)` (under `#[cfg(test)]`) or duplicate a minimal constructor here. Fill the `SubagentTask` literal from the enum's real field list (`chat/mod.rs`, `Entry::SubagentTask`, and `SubagentToolCall`).

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p sirio_ui chat::transcript::`
Expected: FAIL — `todo!()`.

- [ ] **Step 3: Implement**

```rust
pub(crate) fn split_work(entries: &[Entry], turn: &TurnSegment) -> WorkSplit {
    let range = turn.start..=turn.end;
    let slice = &entries[range.clone()];
    let is_tool = |entry: &Entry| matches!(entry, Entry::ToolCall { .. } | Entry::SubagentTask { .. });
    let answer_from = slice.iter().rposition(is_tool).map_or(turn.start, |i| turn.start + i + 1);
    let steps = slice[..answer_from - turn.start]
        .iter()
        .map(|entry| match entry {
            Entry::ToolCall { .. } => 1,
            Entry::SubagentTask { tool_calls, .. } => 1 + tool_calls.len(),
            _ => 0,
        })
        .sum();
    WorkSplit { answer_from, steps }
}

pub(crate) fn work_label(steps: usize) -> String {
    if steps == 1 { "Worked · 1 step".to_string() } else { format!("Worked · {steps} steps") }
}

pub(crate) fn day_label(at: DateTime<Local>, now: DateTime<Local>) -> String {
    let day = at.date_naive();
    let today = now.date_naive();
    if day == today {
        "Today".to_string()
    } else if today.pred_opt() == Some(day) {
        "Yesterday".to_string()
    } else {
        at.format("%a %-d %b").to_string()
    }
}

pub(crate) fn work_roles(
    entries: &[Entry],
    work_open: &HashMap<usize, Takeover>,
    streaming_turn: Option<usize>,
) -> Vec<WorkRole> {
    let mut roles = vec![WorkRole::Outside; entries.len()];
    for turn in segment_turns(entries) {
        let split = split_work(entries, &turn);
        let auto = streaming_turn == Some(turn.start);
        let open = work_open.get(&turn.start).copied().unwrap_or_default().get(auto);
        roles[turn.start] = WorkRole::Header { turn: turn.start, steps: split.steps, open };
        if split.steps == 0 {
            continue;
        }
        for role in &mut roles[turn.start + 1..split.answer_from] {
            *role = WorkRole::Member { turn: turn.start, open };
        }
    }
    roles
}
```

`Takeover` derives `Copy` in the crate (`.copied()` above); if it does not, clone. If `Datelike` is unused after this, drop the import.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p sirio_ui chat::transcript::`
Expected: PASS (6 tests).

- [ ] **Step 5: Commit**

```bash
git add rust/crates/sirio_ui/src/chat/transcript.rs rust/crates/sirio_ui/src/chat/mod.rs
git commit -m "feat(chat): transcript work split, labels and role table"
```

---

### Task 2: `Entry::User { text, at }` and the persisted timestamp

**Files:**
- Modify: `rust/crates/sirio_ui/src/chat/mod.rs` — `Entry::User` (~line 500), every `Entry::User(` pattern and constructor (44), `persisted_entry` (~644), `restored_entry` (~794), the send path (~3374: `self.push_entry(Entry::User(text.clone()))`), `turn_label` (~7361), the `Entry::User` render arm (~4495).
- Modify: `rust/crates/sirio_persistence/src/model.rs` (~264), `rust/crates/sirio_acp/src/chat.rs`, `rust/crates/sirio_persistence/tests/persistence_integration.rs` (add `at: None`).

**Interfaces:**
- Produces: `Entry::User { text: String, at: Option<chrono::DateTime<chrono::Local>> }`; `ChatEntry::UserMessage { text: String, #[serde(default)] at: Option<i64> }` (unix seconds).

- [ ] **Step 1: Write the failing tests** (in `chat/mod.rs` tests)

```rust
    /// A sent message carries the moment it was sent, the moment survives a
    /// restart as unix seconds, and a row written before the field restores
    /// with none.
    #[gpui::test]
    async fn a_user_message_is_stamped_and_the_stamp_persists(cx: &mut TestAppContext) {
        cx.update(Theme::init);
        cx.update(bezel::ui::input::init);
        let (chat, cx) = cx.add_window_view(|_, cx| Chat::new(None, std::env::temp_dir(), cx));
        chat.update(cx, |chat, cx| {
            chat.set_composer_text("hello", cx);
            chat.send(cx);
        });
        chat.read_with(cx, |chat, _| {
            let user = chat.entries.iter().find(|e| matches!(e, Entry::User { .. })).expect("the user entry");
            let Entry::User { at, .. } = user else { unreachable!() };
            let at = at.expect("a sent message is stamped");
            assert!((chrono::Local::now() - at).num_seconds().abs() < 60);
            let persisted = persisted_entry(user).expect("persisted");
            assert!(matches!(persisted, ChatEntry::UserMessage { at: Some(_), .. }));
        });
        let restored = restored_entry(ChatEntry::UserMessage { text: "old".into(), at: None });
        assert!(matches!(restored, Entry::User { at: None, .. }));
        let stamped = restored_entry(ChatEntry::UserMessage { text: "old".into(), at: Some(1_757_000_000) });
        assert!(matches!(stamped, Entry::User { at: Some(_), .. }));
    }

    #[test]
    fn a_user_row_written_before_the_stamp_restores() {
        let entry: ChatEntry = serde_json::from_str(r#"{"UserMessage":{"text":"hi"}}"#).expect("deserializes");
        assert!(matches!(entry, ChatEntry::UserMessage { at: None, .. }));
    }
```

If `send` needs a connected agent to push the user entry, build the entry the way the send path does instead (`chat.push_entry(Entry::User { text, at: Some(Local::now()) })` is not the point — assert on the send path; read `send`/`submit_prompt` first and drive whichever fn pushes the `User` entry).

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p sirio_ui chat::tests::a_user_message_is_stamped`
Expected: FAIL to compile (`Entry::User` is a tuple variant).

- [ ] **Step 3: Implement**

`model.rs`: `UserMessage { text: String, /// Unix seconds of the moment the message was sent, when known. #[serde(default)] at: Option<i64> }`. `chat/mod.rs`: `User { text: String, at: Option<chrono::DateTime<chrono::Local>> }`; persisted: `at.map(|at| at.timestamp())`; restored: `at.and_then(|secs| chrono::DateTime::from_timestamp(secs, 0)).map(|utc| utc.with_timezone(&chrono::Local))`; the send path stamps `Some(chrono::Local::now())`; every other constructor (tests, fixtures) uses `at: None`; every `Entry::User(text)` pattern becomes `Entry::User { text, .. }`. Add `at: None` to `ChatEntry::UserMessage` constructors in `sirio_acp` and the persistence fixture.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p sirio_ui chat::` and `cargo test -p sirio_persistence` and `cargo build -p sirio_acp`
Expected: the two new tests pass; the `chat::` failing list is the 3 baseline reds; the other two crates compile and pass.

- [ ] **Step 5: Commit**

```bash
git add rust/crates/sirio_ui/src/chat/mod.rs rust/crates/sirio_persistence rust/crates/sirio_acp/src/chat.rs
git commit -m "feat(chat): stamp user messages and persist the moment additively"
```

---

### Task 3: `Chat.work_open`, the streaming turn, toggling and remeasuring

**Files:**
- Modify: `rust/crates/sirio_ui/src/chat/mod.rs` — the `Chat` struct and `Chat::new`, every place `unfolded_turns.clear()` runs (~2636, ~2861, ~3292), the `TurnEnded` arm (~2269).
- Modify: `rust/crates/sirio_ui/src/chat/transcript.rs` — `impl Chat { streaming_turn_start, toggle_work, remeasure_turn }`.

**Interfaces:**
- Produces: `Chat.work_open: HashMap<usize, Takeover>`; `Chat::streaming_turn_start(&self) -> Option<usize>`; `Chat::toggle_work(&mut self, turn: usize, cx: &mut Context<Self>)`; `Chat::remeasure_turn(&mut self, turn: usize)`.

- [ ] **Step 1: Write the failing test** (in `chat/mod.rs` tests)

```rust
    /// The streaming turn is the trailing one without a footer, only while
    /// the chat streams; pressing a zone flips what is on screen and holds.
    #[gpui::test]
    async fn the_work_zone_follows_the_stream_until_pressed(cx: &mut TestAppContext) {
        cx.update(Theme::init);
        cx.update(bezel::ui::input::init);
        let (chat, cx) = cx.add_window_view(|_, cx| {
            let mut chat = Chat::new(None, std::env::temp_dir(), cx);
            chat.push_entry(Entry::User { text: "q".into(), at: None });
            chat.push_entry(test_tool_call("a"));
            chat.streaming = true;
            chat
        });
        chat.read_with(cx, |chat, _| {
            assert_eq!(chat.streaming_turn_start(), Some(0));
            let roles = transcript::work_roles(&chat.entries, &chat.work_open, chat.streaming_turn_start());
            assert_eq!(roles[1], transcript::WorkRole::Member { turn: 0, open: true });
        });
        chat.update(cx, |chat, cx| chat.toggle_work(0, cx));
        chat.read_with(cx, |chat, _| {
            let roles = transcript::work_roles(&chat.entries, &chat.work_open, chat.streaming_turn_start());
            assert_eq!(roles[1], transcript::WorkRole::Member { turn: 0, open: false }, "pressed while auto-open: closes and holds");
        });
        chat.update(cx, |chat, cx| {
            chat.handle_event(AcpEvent::TurnEnded { stop_reason: "end_turn".into() }, cx);
        });
        chat.read_with(cx, |chat, _| {
            assert_eq!(chat.streaming_turn_start(), None);
            let roles = transcript::work_roles(&chat.entries, &chat.work_open, None);
            assert_eq!(roles[1], transcript::WorkRole::Member { turn: 0, open: false });
        });
    }
```

Check `TurnEnded`'s payload shape against `sirio_acp` (as sub-project 3 did) and adjust.

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p sirio_ui chat::tests::the_work_zone_follows`
Expected: FAIL to compile — no `work_open`, no `streaming_turn_start`.

- [ ] **Step 3: Implement**

`chat/mod.rs`: field `work_open: HashMap<usize, bezel::ui::widgets::Takeover>` with doc "Each turn's say over its Work zone (`widgets::Takeover`), keyed by the turn's first entry index. Not persisted; cleared with the entries."; `HashMap::new()` in `Chat::new`; `self.work_open.clear()` next to every `unfolded_turns.clear()`. In the `TurnEnded` arm, after the footer is pushed: `if let Some(turn) = segment_turns(&self.entries).last().map(|t| t.start) { self.remeasure_turn(turn); }` (the zone's `auto` just flipped; every member row changes height).

`chat/transcript.rs`:

```rust
impl Chat {
    /// The trailing turn without a footer, while the chat streams.
    pub(crate) fn streaming_turn_start(&self) -> Option<usize> {
        if !self.streaming {
            return None;
        }
        segment_turns(&self.entries).last().filter(|turn| turn.footer.is_none()).map(|turn| turn.start)
    }

    /// The reader presses a Work header: flip what is on screen and hold it.
    pub(crate) fn toggle_work(&mut self, turn: usize, cx: &mut Context<Self>) {
        let auto = self.streaming_turn_start() == Some(turn);
        self.work_open.entry(turn).or_default().toggle(auto);
        self.remeasure_turn(turn);
        cx.notify();
    }

    /// Every row of the turn starting at `turn` changes height when its zone
    /// opens or folds; remeasure them all.
    pub(crate) fn remeasure_turn(&mut self, turn: usize) {
        if let Some(segment) = segment_turns(&self.entries).into_iter().find(|t| t.start == turn) {
            self.list_state.remeasure_items(segment.start..segment.end + 1);
        }
    }
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p sirio_ui chat::`
Expected: the new test passes; failing list = the 3 baseline reds.

- [ ] **Step 5: Commit**

```bash
git add rust/crates/sirio_ui/src/chat
git commit -m "feat(chat): work zone open state on bezel takeover per turn"
```

---

### Task 4: Draw it — header, member frame, interim prose, folded members

**Files:**
- Modify: `rust/crates/sirio_ui/src/chat/transcript.rs` — `impl Chat { render_work_header, render_work_member, render_interim_prose }`
- Modify: `rust/crates/sirio_ui/src/chat/mod.rs` — the list row processor (`Render for Chat`, ~6760–7010: compute `let work = transcript::work_roles(&self.entries, &self.work_open, self.streaming_turn_start());` once per frame next to `turn_roles`, and use it per row), the `Entry::User` and `Entry::Assistant` arms of `render_entry` (they receive the role).

**Interfaces:**
- Produces: `pub(crate) fn render_work_header(turn: usize, steps: usize, open: bool, theme: &Theme, bezel_theme: &bezel::theme::Theme, entity: Entity<Chat>) -> AnyElement` (selector `work-toggle-<turn>`, zero-size marker `work-open-<turn>` when open); `pub(crate) fn render_work_member(entry_index: usize, content: AnyElement, bezel_theme: &bezel::theme::Theme) -> AnyElement` (the frame `ml 10 pl 12 border_l_1 border_color(border) pb 8`, selector `work-member-<entry_index>`); `pub(crate) fn render_interim_prose(entry_index: usize, text: &str, source_start: usize, interaction: &TranscriptInteraction, theme: &Theme) -> AnyElement` (`Callout`, `text_muted`, `render_plain_text`, selector `interim-<entry_index>`).

Row rules in the processor, after the F-CHAT-22 `turn_roles` match and before the tool-run grouping:

- `WorkRole::Member { open: false, .. }` → return the empty row `div().id(("chat-entry", entry_index))` (a folded zone's members contribute no rows).
- `WorkRole::Member { open: true, .. }` → render as today (a thought → its header/body; a tool run → the run box at the run's tail with the other run indices swallowed as now; an `Assistant` → `render_interim_prose`), then wrap the produced element with `render_work_member` instead of the row's own `pb(10)`.
- `WorkRole::Header { steps, open, .. }` → the `Entry::User` arm draws the day heading (Task 5), the bubble, and — when `steps > 0` — `render_work_header` under it with `gap 10`.
- `WorkRole::Outside` → as today; an `Assistant` outside the zone is the answer and gets a zero-size marker `answer-<entry_index>`.

Tool runs inside an open zone: `tool_call_run_bounds_inclusive` must not cross the zone boundary (a run is by construction interim — every tool is before `answer_from` — so no change is needed; note it in a comment).

- [ ] **Step 1: Write the failing test** (in `chat/mod.rs` tests)

```rust
    /// A finished turn folds its interim work — thought, interim prose, tool
    /// run — behind `Worked · N steps`; the answer stays; a press opens the
    /// zone and its members draw inside the frame.
    #[gpui::test]
    async fn a_finished_turn_folds_its_work_behind_a_header(cx: &mut TestAppContext) {
        cx.update(Theme::init);
        cx.update(bezel::ui::input::init);
        let (_chat, cx) = cx.add_window_view(|_, cx| {
            let mut chat = Chat::new(None, std::env::temp_dir(), cx);
            chat.push_entry(Entry::User { text: "q".into(), at: None });
            chat.push_entry(Entry::Thought { text: "hmm".into(), open: Default::default(), started: None, duration_ms: Some(1000) });
            chat.push_entry(Entry::Assistant { text: "looking".into(), document: parse_chat_markdown("looking") });
            chat.push_entry(test_tool_call("a"));
            chat.push_entry(test_tool_call("b"));
            chat.push_entry(Entry::Assistant { text: "the answer".into(), document: parse_chat_markdown("the answer") });
            chat.push_entry(Entry::TurnFooter("12:00".into()));
            chat
        });
        cx.update(|_window, cx| init(cx));
        refresh_frame(cx);
        let header = cx.debug_bounds("work-toggle-0").expect("Worked · 2 steps header");
        let bubble = cx.debug_bounds("user-bubble-0").expect("bubble");
        assert!(header.top() >= bubble.bottom(), "the header sits under the question");
        assert!(cx.debug_bounds("thought-toggle-1").is_none(), "folded: no thought row");
        assert!(cx.debug_bounds("interim-2").is_none(), "folded: no interim prose");
        assert!(cx.debug_bounds("tool-run-3").is_none(), "folded: no tool run");
        assert!(cx.debug_bounds("answer-5").is_some(), "the answer is outside the zone");

        cx.simulate_click(header.center(), Modifiers::none());
        refresh_frame(cx);
        assert!(cx.debug_bounds("work-open-0").is_some());
        let thought = cx.debug_bounds("thought-toggle-1").expect("open: thought header");
        let interim = cx.debug_bounds("interim-2").expect("open: interim prose");
        let run = cx.debug_bounds("tool-run-3").expect("open: the run box");
        let frame = cx.debug_bounds("work-member-1").expect("the frame around a member");
        assert!(thought.left() > bubble.left() || thought.left() > frame.left(), "members are inset by the frame");
        assert!(interim.top() >= thought.bottom() && run.top() >= interim.bottom(), "members keep transcript order");
    }

    /// The streaming turn's zone is open by itself and folds when the turn
    /// ends; a turn without tools has no header at all.
    #[gpui::test]
    async fn the_streaming_turns_zone_is_open_and_folds_on_turn_end(cx: &mut TestAppContext) {
        cx.update(Theme::init);
        cx.update(bezel::ui::input::init);
        let (chat, cx) = cx.add_window_view(|_, cx| {
            let mut chat = Chat::new(None, std::env::temp_dir(), cx);
            chat.push_entry(Entry::User { text: "q".into(), at: None });
            chat.push_entry(test_tool_call("a"));
            chat.streaming = true;
            chat
        });
        cx.update(|_window, cx| init(cx));
        refresh_frame(cx);
        assert!(cx.debug_bounds("work-open-0").is_some(), "open while streaming");
        assert!(cx.debug_bounds("tool-run-1").is_some());
        chat.update(cx, |chat, cx| {
            chat.handle_event(AcpEvent::AgentMessageChunk("done".into()), cx);
            chat.handle_event(AcpEvent::TurnEnded { stop_reason: "end_turn".into() }, cx);
        });
        refresh_frame(cx);
        assert!(cx.debug_bounds("work-open-0").is_none(), "folded on turn end");
        assert!(cx.debug_bounds("tool-run-1").is_none());
        assert!(cx.debug_bounds("answer-2").is_some());

        chat.update(cx, |chat, _| {
            chat.push_entry(Entry::User { text: "q2".into(), at: None });
            chat.push_entry(Entry::Assistant { text: "plain".into(), document: parse_chat_markdown("plain") });
        });
        refresh_frame(cx);
        assert!(cx.debug_bounds("work-toggle-4").is_none(), "no tools, no header");
        assert!(cx.debug_bounds("answer-5").is_some());
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p sirio_ui chat::tests::a_finished_turn_folds`
Expected: FAIL — `work-toggle-0` not found.

- [ ] **Step 3: Implement**

`chat/transcript.rs`:

```rust
use bezel::ui::widgets::Layout as _;
use gpui::{AnyElement, Entity, div, prelude::*, px};
use sirio_theme::Theme;

use super::{Chat, TranscriptInteraction};

impl Chat {
    /// The gallery's `work_header`: chevron + `Worked · N steps`, clickable.
    pub(crate) fn render_work_header(
        turn: usize,
        steps: usize,
        open: bool,
        theme: &Theme,
        bezel_theme: &bezel::theme::Theme,
        entity: Entity<Chat>,
    ) -> AnyElement {
        div()
            .id(("work-toggle", turn))
            .debug_selector(move || format!("work-toggle-{turn}"))
            .self_start()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(6.0))
            .px(px(4.0))
            .py(px(5.0))
            .rounded(px(bezel::theme::Theme::control_radius()))
            .cursor_pointer()
            .hover(|s| s.bg(bezel::theme::ink(0.03)))
            .on_click(move |_, _, cx| {
                entity.update(cx, |chat, cx| chat.toggle_work(turn, cx));
            })
            .child(bezel_theme.disclosure(open))
            .child(
                div()
                    .text_size(theme.typography.callout)
                    .line_height(px(19.0))
                    .text_color(theme.text_muted)
                    .child(work_label(steps)),
            )
            .when(open, |row| row.child(div().size_0().debug_selector(move || format!("work-open-{turn}"))))
            .into_any_element()
    }

    /// The zone's frame, one row at a time: the border line down the left
    /// and the zone's `gap 8` as bottom padding, so adjacent member rows read
    /// as one bordered column.
    pub(crate) fn render_work_member(
        entry_index: usize,
        content: AnyElement,
        bezel_theme: &bezel::theme::Theme,
    ) -> AnyElement {
        div()
            .debug_selector(move || format!("work-member-{entry_index}"))
            .ml(px(10.0))
            .pl(px(12.0))
            .border_l_1()
            .border_color(bezel_theme.border)
            .pb(px(8.0))
            .child(content)
            .into_any_element()
    }

    /// Interim prose: what the model said while working, `Callout` in
    /// `text_muted`, plain text with the transcript's selection.
    pub(crate) fn render_interim_prose(
        entry_index: usize,
        text: &str,
        source_start: usize,
        interaction: &TranscriptInteraction,
        theme: &Theme,
    ) -> AnyElement {
        div()
            .debug_selector(move || format!("interim-{entry_index}"))
            .text_size(theme.typography.callout)
            .line_height(px(19.0))
            .text_color(theme.text_muted)
            .child(Self::render_plain_text(
                text.to_string(),
                theme,
                format!("interim-entry-{entry_index}"),
                source_start,
                Some(interaction),
            ))
            .into_any_element()
    }
}
```

`chat/mod.rs`: compute `work` once per frame; pass `work.get(entry_index)` into the processor's logic as described in the row rules; give `render_entry` a `role: &transcript::WorkRole` parameter used by the `User` arm (bubble + header) and the `Assistant` arm (interim vs answer marker). Keep the bubble's own selector and layout. The `Header` case with `steps > 0` wraps bubble + header in `div().flex().flex_col().gap(px(10.0))`.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p sirio_ui chat::`
Expected: the two new tests pass; every existing test that pushed tool calls after a `User` entry still finds its selectors — where an old test now needs the zone open (its tool calls follow a `User` entry and the chat is not streaming), open it by clicking `work-toggle-<turn>` first or set `chat.streaming = true`; say which in the commit body. Failing list = the 3 baseline reds.

- [ ] **Step 5: Commit**

```bash
git add rust/crates/sirio_ui/src/chat
git commit -m "feat(chat): fold a turn's interim work behind the gallery's worked header"
```

---

### Task 5: Day headings

**Files:**
- Modify: `rust/crates/sirio_ui/src/chat/transcript.rs` — `pub(crate) fn heading_for(entries: &[Entry], index: usize, now: DateTime<Local>) -> Option<String>` and `impl Chat { render_day_heading }`
- Modify: `rust/crates/sirio_ui/src/chat/mod.rs` — the `Entry::User` arm draws the heading above the bubble.

**Interfaces:**
- Produces: `heading_for` (the label when `entries[index]` is a stamped `User` whose calendar day differs from the previous stamped `User`'s, or is the first stamped one; `None` for undated); `render_day_heading(entry_index: usize, label: &str, theme: &Theme, bezel_theme: &bezel::theme::Theme) -> AnyElement` (`py 8`, `Subheadline`/`typography.footnote`, `FontWeight::MEDIUM`, `text_faint`, `popover::tracked_upper(label)`, selector `day-heading-<entry_index>`).

- [ ] **Step 1: Write the failing tests**

Unit (`transcript.rs`):

```rust
    #[test]
    fn a_heading_appears_where_the_day_changes_and_never_for_undated_turns() {
        let now = Local.with_ymd_and_hms(2026, 9, 4, 22, 0, 0).unwrap();
        let y = Local.with_ymd_and_hms(2026, 9, 3, 9, 0, 0).unwrap();
        let entries = vec![
            Entry::User { text: "a".into(), at: Some(y) },
            Entry::User { text: "b".into(), at: Some(y) },
            Entry::User { text: "c".into(), at: None },
            Entry::User { text: "d".into(), at: Some(now) },
        ];
        assert_eq!(heading_for(&entries, 0, now).as_deref(), Some("Yesterday"));
        assert_eq!(heading_for(&entries, 1, now), None);
        assert_eq!(heading_for(&entries, 2, now), None, "undated: no heading, no break");
        assert_eq!(heading_for(&entries, 3, now).as_deref(), Some("Today"));
    }
```

gpui (`chat/mod.rs`):

```rust
    #[gpui::test]
    async fn day_headings_are_drawn_above_the_first_question_of_a_day(cx: &mut TestAppContext) {
        cx.update(Theme::init);
        cx.update(bezel::ui::input::init);
        let now = chrono::Local::now();
        let yesterday = now - chrono::Duration::days(1);
        let (_chat, cx) = cx.add_window_view(|_, cx| {
            let mut chat = Chat::new(None, std::env::temp_dir(), cx);
            chat.push_entry(Entry::User { text: "a".into(), at: Some(yesterday) });
            chat.push_entry(Entry::Assistant { text: "x".into(), document: parse_chat_markdown("x") });
            chat.push_entry(Entry::TurnFooter("t".into()));
            chat.push_entry(Entry::User { text: "b".into(), at: Some(now) });
            chat
        });
        cx.update(|_window, cx| init(cx));
        refresh_frame(cx);
        let heading = cx.debug_bounds("day-heading-0").expect("Yesterday");
        let bubble = cx.debug_bounds("user-bubble-0").expect("bubble");
        assert!(heading.bottom() <= bubble.top(), "the heading sits above the question");
        assert!(cx.debug_bounds("day-heading-3").is_some(), "Today");
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p sirio_ui chat::` (filter `heading`)
Expected: FAIL — `heading_for` undefined / `day-heading-0` not found.

- [ ] **Step 3: Implement**

```rust
pub(crate) fn heading_for(entries: &[Entry], index: usize, now: DateTime<Local>) -> Option<String> {
    let Some(Entry::User { at: Some(at), .. }) = entries.get(index) else {
        return None;
    };
    let previous = entries[..index].iter().rev().find_map(|entry| match entry {
        Entry::User { at: Some(at), .. } => Some(*at),
        _ => None,
    });
    if previous.is_some_and(|previous| previous.date_naive() == at.date_naive()) {
        return None;
    }
    Some(day_label(*at, now))
}

impl Chat {
    pub(crate) fn render_day_heading(
        entry_index: usize,
        label: &str,
        theme: &Theme,
        bezel_theme: &bezel::theme::Theme,
    ) -> AnyElement {
        div()
            .debug_selector(move || format!("day-heading-{entry_index}"))
            .py(px(8.0))
            .text_size(theme.typography.footnote)
            .font_weight(gpui::FontWeight::MEDIUM)
            .text_color(bezel_theme.text_faint)
            .child(bezel::ui::popover::tracked_upper(label))
            .into_any_element()
    }
}
```

The `User` arm draws `heading_for(&entries, entry_index, Local::now())` above the bubble — pass the label in from the processor (it holds `&this.entries`); the heading row belongs to the `User` entry's row, so no extra list index is needed.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p sirio_ui chat::`
Expected: both pass; failing list = the 3 baseline reds.

- [ ] **Step 5: Commit**

```bash
git add rust/crates/sirio_ui/src/chat
git commit -m "feat(chat): day headings where the calendar day changes between turns"
```

---

### Task 6: The turn fold and the Work zone agree; existing transcript tests

**Files:**
- Modify: `rust/crates/sirio_ui/src/chat/mod.rs` — tests only, unless a rule below needs a code change.

Rules to verify and encode:

- An F-CHAT-22 folded turn (`TurnRowRole::Hidden`) hides its Work header too (the turn's first row is the fold stand-in): assert in an existing fold test (`the_third_newest_turn_folds…` or whichever asserts `turn-fold-*`) that `work-toggle-<turn>` is absent while folded and present once unfolded.
- The generating spinner and the pending-question bar are outside every zone (they are not entries).
- `unfolded_turns` and `work_open` are both cleared on `clear_transcript`/new conversation (one assertion in the existing clearing test).

- [ ] **Step 1–4:** add the assertions, run red where they fail on the current tree, fix, run green (`cargo test -p sirio_ui chat::` = 3 baseline reds).

- [ ] **Step 5: Commit**

```bash
git add rust/crates/sirio_ui/src/chat/mod.rs
git commit -m "test(chat): the turn fold and the work zone agree"
```

---

### Task 7: Sweep, format, build, record

- [ ] **Step 1:** `cargo clippy -p sirio_ui --all-targets` — no warning in `chat/transcript.rs`, no new warning in `chat/mod.rs`; delete anything the cut-over left dead (an old `tool_group_label` if any survived, unused imports).
- [ ] **Step 2:** `rustfmt --edition 2024 rust/crates/sirio_ui/src/chat/mod.rs rust/crates/sirio_ui/src/chat/transcript.rs`.
- [ ] **Step 3:** `cargo build -p sirio` (own target; `taskkill /IM sirio.exe /F` first if locked), then `cargo test -p sirio_ui chat::` — failing list = the 3 baseline reds.
- [ ] **Step 4:** Under "Delivery" in the spec add `- Sub-project 4 landed on branch feat/transcript-work-zone (2026-09-04).` after the sub-project 3 line. Touch nothing else in the spec.
- [ ] **Step 5: Commit**

```bash
git add rust/crates/sirio_ui/src/chat docs/superpowers/specs/2026-09-04-bezel-agent-parity-design.md
git commit -m "chore(chat): sweep the transcript cut-over and record sub-project 4"
```

---

## Self-review

- **Spec coverage:** turn split (Task 1), Work zone header/open state/body/list trick (Tasks 3–4), day headings with `Entry::User { text, at }` and persisted `at` (Tasks 2, 5), kept-from-Sirio list respected (Task 6), deletions (nothing left of `group_expanded`/`render_tool_call_group` after sub-project 2; Task 7 sweeps stragglers), tests listed in the spec (split — Task 1; zone open/fold/press — Tasks 3–4; step count — Task 1; day heading placement — Task 5; existing transcript tests — Tasks 4, 6).
- **Type consistency:** `Entry::User { text, at }` from Task 2 on; `WorkRole::{Outside, Header{turn, steps, open}, Member{turn, open}}` from Task 1; `work_open: HashMap<usize, Takeover>` keyed by `TurnSegment.start`; `render_work_header(turn, steps, open, theme, bezel_theme, entity)`, `render_work_member(entry_index, content, bezel_theme)`, `render_interim_prose(entry_index, text, source_start, interaction, theme)`, `render_day_heading(entry_index, label, theme, bezel_theme)`.
- **Known unknowns, resolved in-task:** `TurnEnded`'s payload; whether `send` pushes the `User` entry without an agent (Task 2 gives the alternative); which existing tests need the zone opened (Task 4, recorded in the commit body).
