# Thinking on bezel `Takeover` Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Draw a reasoning ("Thought") entry the way the bezel gallery's Activity page draws it — orb-or-chevron header with `Thinking` / `Thought for Ns`, a body that follows its newest line while it streams — and let the section open itself while the thought streams, fold when it settles, and obey the reader once pressed (`widgets::Takeover`).

**Architecture:** A new module `sirio_ui/src/chat/thought.rs` holds the pure header label, the per-entry scroll state (`ThoughtScroll`) and the two renderers (`render_thought_header`, `render_thought_body`); `chat/mod.rs` keeps the entry, its persistence and the event handling. `Entry::Thought` trades `expanded: bool` for `open: Takeover` plus a start instant and a measured duration; `ChatEntry::Thought` gains `duration_ms` additively. The transient generating spinner (#239) is drawn with the same header so a run in progress and a settled thought share one shape.

**Tech Stack:** Rust, gpui, bezel `=0.1.4` (`ui::widgets::{Takeover, Layout}`, `ui::loaders::orb` through `loading::thinking_indicator`, `ui::scroll::{FollowState, ScrollbarState, follow, scrollbar}`, `motion::Painter`, `theme::ink`), serde.

**Spec:** `docs/superpowers/specs/2026-09-04-bezel-agent-parity-design.md`, section "Sub-project 3 — Thinking / Activity (`thought.rs`)" and "Cross-cutting constraints".

## Global Constraints

- bezel and gpui pins unchanged; the reference is tag `v0.1.4`, not `main`. No dependency changes.
- Every persistence change is additive with `#[serde(default)]`; a database written by today's build restores unchanged.
- Selectors used by `sirio`'s integration tests are preserved verbatim: `thought-toggle-N` stays; `chat-generating-spinner` stays.
- Workspace gates (`Scripts/ci.sh`, `Scripts/ci-linux.sh`) only on the user's explicit request; iteration is `cargo test -p sirio_ui chat::` and `cargo clippy -p sirio_ui --all-targets`.
- Build in the worktree's own `rust/target` (never a shared `CARGO_TARGET_DIR`; the repo's `.cargo/config.toml` routes dependency compilation through sccache).
- Baseline reds on Windows in `chat::` are exactly three (`auth_required_retry_survives_long_guidance_text_at_running_width`, `failed_launch_can_retry_and_complete`, `a_disconnected_agent_offers_restart_agent_not_retry`); judge failures as a list.
- F-CHAT-21's rule changes only for a *live* thought (auto-open while streaming, fold on settle); a restored thought still opens closed. Persistence stores nothing for the open state.

## File Structure

- Create: `rust/crates/sirio_ui/src/chat/thought.rs` — `thought_header_label` (pure), `ThoughtScroll` (view state), `impl Chat { render_thought_header, render_thought_body, thought_is_streaming, settle_open_thought, toggle_thought }`, `mod tests`.
- Modify: `rust/crates/sirio_ui/src/chat/mod.rs` — `Entry::Thought` fields, `persisted_entry`/`restored_entry`, `Chat.thought_scroll`, `ThoughtChunk`/`TurnEnded`/`push_entry` hooks, the `Entry::Thought` arm of `render_entry`, the #239 spinner row, tests.
- Modify: `rust/crates/sirio_persistence/src/model.rs` — `ChatEntry::Thought { text, duration_ms }`.
- Modify (compile-only): every `ChatEntry::Thought { text }` constructor outside `sirio_ui` — `rust/crates/sirio_acp/src/chat.rs` and `rust/crates/sirio_persistence/tests/persistence_integration.rs` (grep `ChatEntry::Thought`).
- Modify (Task 6 only): `docs/superpowers/specs/2026-09-04-bezel-agent-parity-design.md` — one Delivery line.

Reference sources (read before Task 3): the gallery's `Activity::header` and `Activity::reasoning` in `apps/gallery/src/patterns/agent.rs` (lines 140–245 of the sparse clone in the session scratchpad, `bezel/apps/gallery/src/patterns/agent.rs`); bezel 0.1.4 `widgets/mod.rs` (`Takeover`), `widgets/layout.rs` (`Layout::disclosure`), `scroll.rs` (`FollowState`, `ScrollbarState`, `follow`, `scrollbar`) under `C:\Users\enzop\.cargo\registry\src\index.crates.io-1949cf8c6b5b557f\bezel-ui-0.1.4\src\`. When a snippet below and the crate's real signature disagree, the crate wins.

---

### Task 1: Pure helpers and the scroll state in `chat/thought.rs`

**Files:**
- Create: `rust/crates/sirio_ui/src/chat/thought.rs`
- Modify: `rust/crates/sirio_ui/src/chat/mod.rs` (one `mod thought;` line next to `mod tool_calls;`)

**Interfaces:**
- Produces: `pub(crate) fn thought_header_label(streaming: bool, duration_ms: Option<u64>) -> String`; `pub(crate) struct ThoughtScroll { pub scroll: gpui::ScrollHandle, pub follow: bezel::ui::scroll::FollowState, pub bar: bezel::ui::scroll::ScrollbarState }` with `pub(crate) fn new(painter: bezel::motion::Painter) -> Self`.

- [ ] **Step 1: Write the failing tests**

```rust
//! The reasoning entry as the gallery's Activity page draws it: an orb while
//! it streams and a disclosure chevron once it settles, `Thinking` /
//! `Thought for Ns`, and a body pinned to its newest line —
//! `crabtalk/bezel` tag `v0.1.4`, `apps/gallery/src/patterns/agent.rs`
//! (`Activity`). Whether the body is open is `widgets::Takeover`: it follows
//! the run until the reader presses the header, then obeys the reader.

use std::time::Duration;

use bezel::motion::Painter;
use bezel::ui::scroll::{FollowState, ScrollbarState};
use gpui::ScrollHandle;

use crate::loading;

/// The header's word: `Thinking` while the thought streams, then
/// `Thought for Ns` when this process measured it, or a bare `Thought` for a
/// thought restored from a database that never knew.
pub(crate) fn thought_header_label(streaming: bool, duration_ms: Option<u64>) -> String {
    todo!()
}

/// Per-entry scroll state for an open body: the handle the well tracks, the
/// follow pin, and the scrollbar's own state. Not persisted; created the
/// first time a thought's body is drawn.
pub(crate) struct ThoughtScroll {
    pub scroll: ScrollHandle,
    pub follow: FollowState,
    pub bar: ScrollbarState,
}

impl ThoughtScroll {
    pub(crate) fn new(painter: Painter) -> Self {
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_label_follows_the_state() {
        assert_eq!(thought_header_label(true, None), "Thinking");
        assert_eq!(thought_header_label(true, Some(4_000)), "Thinking");
        assert_eq!(thought_header_label(false, Some(4_000)), "Thought for 4s");
        assert_eq!(thought_header_label(false, Some(450)), "Thought for 0s");
        assert_eq!(thought_header_label(false, None), "Thought");
    }

    #[test]
    fn a_fresh_scroll_state_follows() {
        let state = FollowState::new();
        assert!(state.following(), "a new follow pin starts pinned to the newest line");
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p sirio_ui chat::thought::`
Expected: FAIL — `todo!()` panics in `the_label_follows_the_state`.

- [ ] **Step 3: Implement**

```rust
pub(crate) fn thought_header_label(streaming: bool, duration_ms: Option<u64>) -> String {
    if streaming {
        "Thinking".to_string()
    } else {
        loading::thought_label(duration_ms.map(Duration::from_millis))
    }
}

impl ThoughtScroll {
    pub(crate) fn new(painter: Painter) -> Self {
        Self {
            scroll: ScrollHandle::new(),
            follow: FollowState::new(),
            bar: ScrollbarState::new(painter),
        }
    }
}
```

`loading::thought_label` already formats `Thought for {:.0}s` / `Thought` (`sirio_ui/src/loading.rs:42`). If `FollowState::new()` does not start in the following state in the pinned crate, change the second test to call `state.follow()` first and keep the assertion.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p sirio_ui chat::thought::`
Expected: PASS (2 tests).

- [ ] **Step 5: Commit**

```bash
git add rust/crates/sirio_ui/src/chat/thought.rs rust/crates/sirio_ui/src/chat/mod.rs
git commit -m "feat(chat): thought header label and scroll state helpers"
```

---

### Task 2: `Entry::Thought` gains `open: Takeover`, `started` and `duration_ms`; persistence and settling

**Files:**
- Modify: `rust/crates/sirio_ui/src/chat/mod.rs` — `Entry::Thought` (~line 509), `persisted_entry` (~637), `restored_entry` (~785), `toggle_thought_expanded` (~1770), `push_entry` (~1690), the `ThoughtChunk` arm (~1883), the `TurnEnded` arm (~2269), the `render_entry` arm (~4548, minimal patch so it compiles — Task 3 rewrites it), every test that builds `Entry::Thought { .. expanded .. }` (grep `expanded: false` near `Entry::Thought`, e.g. ~11870).
- Modify: `rust/crates/sirio_persistence/src/model.rs` (~268), `rust/crates/sirio_acp/src/chat.rs`, `rust/crates/sirio_persistence/tests/persistence_integration.rs` (add `duration_ms: None` where `ChatEntry::Thought { text }` is built).
- Create/extend: `rust/crates/sirio_ui/src/chat/thought.rs` — `impl Chat { thought_is_streaming, settle_open_thought, toggle_thought }`.

**Interfaces:**
- Consumes: nothing from Task 1 beyond the module.
- Produces: `Entry::Thought { text: String, open: bezel::ui::widgets::Takeover, started: Option<std::time::Instant>, duration_ms: Option<u64> }`; `ChatEntry::Thought { text: String, #[serde(default)] duration_ms: Option<u64> }`; `Chat::thought_is_streaming(&self, index: usize) -> bool`; `Chat::settle_open_thought(&mut self)`; `Chat::toggle_thought(&mut self, index: usize, cx: &mut Context<Self>)`.

- [ ] **Step 1: Write the failing tests** (in `chat/mod.rs`'s `mod tests`)

```rust
    /// A thought measures the wall clock from its first chunk to the entry
    /// that settles it, and the duration survives a restart; a thought
    /// restored from a database written before the field has none.
    #[gpui::test]
    async fn a_thought_measures_its_duration_and_persists_it(cx: &mut TestAppContext) {
        cx.update(Theme::init);
        cx.update(bezel::ui::input::init);
        let (chat, cx) = cx.add_window_view(|_, cx| Chat::new(None, std::env::temp_dir(), cx));
        chat.update(cx, |chat, cx| {
            chat.streaming = true;
            chat.handle_event(AcpEvent::ThoughtChunk("weigh the options".into()), cx);
            assert!(
                matches!(
                    chat.entries.last(),
                    Some(Entry::Thought { started: Some(_), duration_ms: None, .. })
                ),
                "the first chunk starts the clock"
            );
            assert!(chat.thought_is_streaming(chat.entries.len() - 1));
            chat.handle_event(AcpEvent::AgentMessageChunk("Here is the answer.".into()), cx);
        });
        chat.read_with(cx, |chat, _| {
            let thought = chat
                .entries
                .iter()
                .find(|entry| matches!(entry, Entry::Thought { .. }))
                .expect("the thought is still there");
            assert!(
                matches!(thought, Entry::Thought { duration_ms: Some(_), .. }),
                "the answer's first chunk settles the thought: {thought:?}"
            );
            assert!(!chat.thought_is_streaming(0));
            assert!(
                matches!(
                    persisted_entry(thought),
                    Some(ChatEntry::Thought { duration_ms: Some(_), .. })
                ),
                "the duration is written to the persisted entry"
            );
        });
        let restored = restored_entry(ChatEntry::Thought {
            text: "old".into(),
            duration_ms: None,
        });
        assert!(matches!(
            restored,
            Entry::Thought { started: None, duration_ms: None, .. }
        ));
    }

    /// The turn's end settles a thought that no answer followed.
    #[gpui::test]
    async fn a_turn_end_settles_a_trailing_thought(cx: &mut TestAppContext) {
        cx.update(Theme::init);
        cx.update(bezel::ui::input::init);
        let (chat, cx) = cx.add_window_view(|_, cx| Chat::new(None, std::env::temp_dir(), cx));
        chat.update(cx, |chat, cx| {
            chat.streaming = true;
            chat.handle_event(AcpEvent::ThoughtChunk("…".into()), cx);
            chat.handle_event(
                AcpEvent::TurnEnded {
                    stop_reason: "end_turn".into(),
                },
                cx,
            );
        });
        chat.read_with(cx, |chat, _| {
            assert!(
                chat.entries
                    .iter()
                    .any(|entry| matches!(entry, Entry::Thought { duration_ms: Some(_), .. })),
                "the trailing thought settled on turn end"
            );
        });
    }

    /// A pre-field row restores with no duration rather than failing to
    /// deserialize.
    #[test]
    fn a_thought_row_written_before_the_duration_field_restores() {
        let json = r#"{"kind":"thought","text":"hmm"}"#;
        // Use the same serde shape `ChatEntry` derives; if the enum is tagged
        // differently, mirror the existing pre-#168 tool-call test next to
        // this one (`a_row_written_before_168_restores`).
        let entry: ChatEntry = serde_json::from_str(json).expect("deserializes");
        assert!(matches!(entry, ChatEntry::Thought { duration_ms: None, .. }));
    }
```

Check the `TurnEnded` payload shape (`stop_reason`'s type) against the enum in `sirio_acp` and adjust the literal; check how the existing pre-#168 tool-call test serialises `ChatEntry` and copy its JSON shape for the thought row.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p sirio_ui chat::tests::a_thought_measures -- --nocapture`
Expected: FAIL to compile — `Entry::Thought` has no field `started`/`duration_ms`, `thought_is_streaming` undefined.

- [ ] **Step 3: Implement**

`sirio_persistence/src/model.rs`:

```rust
    Thought {
        text: String,
        /// Wall-clock milliseconds from the first chunk to the entry that
        /// settled the thought, when this process measured it.
        #[serde(default)]
        duration_ms: Option<u64>,
    },
```

Add `duration_ms: None` to every other `ChatEntry::Thought { text }` constructor in the workspace (`sirio_acp/src/chat.rs`, `persistence_integration.rs`, and any in `sirio_ui`).

`chat/mod.rs`, the entry:

```rust
    /// A streamed reasoning chunk, visually distinct from the reply.
    ///
    /// `open` is the reader's say over the body (`widgets::Takeover`): until
    /// they press the header it follows the run — open while the thought
    /// streams, folded once it settles. `started` is view state (the first
    /// chunk's instant); `duration_ms` is what the settling entry stored and
    /// what persists. A restored thought has neither `started` nor a live
    /// run, so it opens closed as before (F-CHAT-21).
    Thought {
        text: String,
        open: bezel::ui::widgets::Takeover,
        started: Option<std::time::Instant>,
        duration_ms: Option<u64>,
    },
```

`persisted_entry`: `Entry::Thought { text, duration_ms, .. } => Some(ChatEntry::Thought { text: text.clone(), duration_ms: *duration_ms })`. `restored_entry`: `ChatEntry::Thought { text, duration_ms } => Entry::Thought { text, open: Default::default(), started: None, duration_ms }`.

`ThoughtChunk` arm: on the *new* entry set `started: Some(Instant::now())`, `open: Default::default()`, `duration_ms: None`. Appending to an existing thought is unchanged.

`chat/thought.rs`:

```rust
impl Chat {
    /// A thought streams while the turn does and nothing has settled it.
    pub(crate) fn thought_is_streaming(&self, index: usize) -> bool {
        self.streaming
            && matches!(
                self.entries.get(index),
                Some(Entry::Thought { started: Some(_), duration_ms: None, .. })
            )
    }

    /// Settles the thought at the tail, if one is still open: stores its
    /// elapsed time and remeasures the row (the header's word and the body's
    /// auto-open both change). Called before any non-thought entry is pushed
    /// and when the turn ends.
    pub(crate) fn settle_open_thought(&mut self) {
        let index = self.entries.len().checked_sub(1);
        if let Some(index) = index
            && let Some(Entry::Thought { started: Some(started), duration_ms, .. }) =
                self.entries.get_mut(index)
            && duration_ms.is_none()
        {
            *duration_ms = Some(started.elapsed().as_millis() as u64);
            self.remeasure_entry(index);
        }
    }

    /// The reader presses the header: flip what is on screen and hold it
    /// (`Takeover::toggle` against the current auto state).
    pub(crate) fn toggle_thought(&mut self, index: usize, cx: &mut Context<Self>) {
        let auto = self.thought_is_streaming(index);
        if let Some(Entry::Thought { open, .. }) = self.entries.get_mut(index) {
            open.toggle(auto);
            self.remeasure_entry(index);
        }
        cx.notify();
    }
}
```

If the crate's edition rejects `let … && let …` chains, nest the `if let`s. In `push_entry`, before the push: `if !matches!(entry, Entry::Thought { .. }) { self.settle_open_thought(); }`. In the `TurnEnded` arm, first line: `self.settle_open_thought();`. Delete `toggle_thought_expanded`; the header's click (Task 3) calls `toggle_thought`. Until Task 3, patch the existing `render_entry` arm minimally: destructure `open`, compute `let streaming = …` from `entry_index` via a `is_streaming: bool` you pass in (or compute `open.get(false)`) — the only goal is to compile; Task 3 replaces it. Update every test that built `Entry::Thought { text, expanded }` to `Entry::Thought { text, open: Default::default(), started: None, duration_ms: None }` and `thought_starts_collapsed_and_toggles_on_click` to read `open.get(false)` instead of `expanded`.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p sirio_ui chat::`
Expected: the three new tests pass; failing list = the 3 baseline reds.

- [ ] **Step 5: Commit**

```bash
git add rust/crates/sirio_ui/src/chat rust/crates/sirio_persistence rust/crates/sirio_acp/src/chat.rs
git commit -m "feat(chat): time a thought and let bezel takeover own its open state"
```

---

### Task 3: The header — orb while streaming, disclosure once settled, Takeover-driven open

**Files:**
- Modify: `rust/crates/sirio_ui/src/chat/thought.rs` — `impl Chat { render_thought_header }`
- Modify: `rust/crates/sirio_ui/src/chat/mod.rs` — the `Entry::Thought` arm of `render_entry` (~4548) and the row renderer that calls it (it must know `entry_index` and `streaming`).

**Interfaces:**
- Consumes: `thought_header_label`, `thought_is_streaming`, `toggle_thought` (Tasks 1–2).
- Produces: `pub(crate) fn render_thought_header(entry_index: usize, streaming: bool, open: bool, duration_ms: Option<u64>, clickable: bool, theme: &Theme, bezel_theme: &bezel::theme::Theme, window: &mut Window, cx: &mut App, entity: Option<Entity<Chat>>) -> AnyElement`.

Selectors: header `thought-toggle-N` (kept); zero-size markers `thought-streaming-N` (while streaming) / `thought-settled-N` (otherwise), `thought-took-N` when `duration_ms` is known; the body keeps `thought-body-N` (Task 4).

- [ ] **Step 1: Write the failing test** (in `chat/mod.rs` tests)

```rust
    /// The body follows the run until the reader presses the header: open
    /// while the thought streams, folded once it settles, and the reader's
    /// press holds from then on — `widgets::Takeover`.
    #[gpui::test]
    async fn a_live_thought_opens_while_streaming_folds_on_settle_and_obeys_a_press(
        cx: &mut TestAppContext,
    ) {
        cx.update(Theme::init);
        cx.update(bezel::ui::input::init);
        let (chat, cx) = cx.add_window_view(|_, cx| Chat::new(None, std::env::temp_dir(), cx));
        cx.update(|_window, cx| init(cx));
        chat.update(cx, |chat, cx| {
            chat.streaming = true;
            chat.handle_event(AcpEvent::ThoughtChunk("first, look at the tests".into()), cx);
        });
        refresh_frame(cx);
        assert!(cx.debug_bounds("thought-streaming-0").is_some(), "the header shows the orb");
        assert!(cx.debug_bounds("thought-body-0").is_some(), "a live thought is open");

        chat.update(cx, |chat, cx| {
            chat.handle_event(AcpEvent::AgentMessageChunk("Done.".into()), cx);
        });
        refresh_frame(cx);
        assert!(cx.debug_bounds("thought-settled-0").is_some(), "the header shows the chevron");
        assert!(cx.debug_bounds("thought-took-0").is_some(), "a measured thought says how long");
        assert!(cx.debug_bounds("thought-body-0").is_none(), "a settled thought folds");

        let header = cx.debug_bounds("thought-toggle-0").expect("header");
        cx.simulate_click(header.center(), Modifiers::none());
        refresh_frame(cx);
        assert!(cx.debug_bounds("thought-body-0").is_some(), "a press opens it");

        chat.update(cx, |chat, cx| {
            chat.handle_event(
                AcpEvent::TurnEnded {
                    stop_reason: "end_turn".into(),
                },
                cx,
            );
        });
        refresh_frame(cx);
        assert!(
            cx.debug_bounds("thought-body-0").is_some(),
            "the reader's choice holds across later events"
        );
    }

    /// A restored thought opens closed and its header carries no clock.
    #[gpui::test]
    async fn a_restored_thought_opens_closed(cx: &mut TestAppContext) {
        cx.update(Theme::init);
        cx.update(bezel::ui::input::init);
        let (_chat, cx) = cx.add_window_view(|_, cx| {
            let mut chat = Chat::new(None, std::env::temp_dir(), cx);
            chat.push_entry(restored_entry(ChatEntry::Thought {
                text: "old reasoning".into(),
                duration_ms: None,
            }));
            chat
        });
        cx.update(|_window, cx| init(cx));
        refresh_frame(cx);
        assert!(cx.debug_bounds("thought-settled-0").is_some());
        assert!(cx.debug_bounds("thought-took-0").is_none(), "no clock to offer");
        assert!(cx.debug_bounds("thought-body-0").is_none());
    }
```

`refresh_frame` and `init` are the existing test helpers in `chat/mod.rs` (see `a_tool_call_is_a_step_row_with_a_chevron_only_when_it_has_a_body`).

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p sirio_ui chat::tests::a_live_thought`
Expected: FAIL — `thought-streaming-0` not found (the old header draws Sirio's chevron and no markers).

- [ ] **Step 3: Implement**

`chat/thought.rs`:

```rust
use bezel::ui::widgets::Layout as _;
use gpui::{AnyElement, App, Entity, Window, div, prelude::*, px};

impl Chat {
    /// The gallery's `Activity::header`: a 14 px glyph slot — the orb
    /// cluster while the thought streams, the disclosure chevron once it
    /// settles — and the state's word. Clickable when it belongs to an entry;
    /// the generating spinner (#239) draws the same row without a click.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn render_thought_header(
        entry_index: usize,
        streaming: bool,
        open: bool,
        duration_ms: Option<u64>,
        theme: &Theme,
        bezel_theme: &bezel::theme::Theme,
        window: &mut Window,
        cx: &mut App,
        entity: Option<Entity<Chat>>,
    ) -> AnyElement {
        // Built outside the row: gpui reads an svg's colour off its own
        // element, so a chevron tinted by its parent paints nothing.
        let glyph: AnyElement = if streaming {
            loading::thinking_indicator("thought-orb", theme, window, cx)
        } else {
            bezel_theme.disclosure(open).into_any_element()
        };
        let label = thought_header_label(streaming, duration_ms);
        let mut row = div()
            .id(("thought-toggle", entry_index))
            .debug_selector(move || format!("thought-toggle-{entry_index}"))
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
            .child(div().flex().w(px(loading::THINKING_GLYPH)).justify_center().child(glyph))
            .child(
                div()
                    .text_size(theme.typography.callout)
                    .line_height(px(19.0))
                    .text_color(theme.text_muted)
                    .child(label),
            )
            .child(div().size_0().debug_selector(move || {
                if streaming {
                    format!("thought-streaming-{entry_index}")
                } else {
                    format!("thought-settled-{entry_index}")
                }
            }))
            .when(duration_ms.is_some() && !streaming, |row| {
                row.child(
                    div()
                        .size_0()
                        .debug_selector(move || format!("thought-took-{entry_index}")),
                )
            });
        if let Some(entity) = entity {
            row = row.on_click(move |_, _, cx| {
                entity.update(cx, |chat, cx| chat.toggle_thought(entry_index, cx));
            });
        }
        row.into_any_element()
    }
}
```

`loading::THINKING_GLYPH` is 14 px already (the slot the old header used). If `thinking_indicator`'s `&'static str` id must be unique per row, keep the single `"thought-orb"`: only one thought streams at a time, and the #239 spinner uses `"chat-thinking"`.

In `render_entry`'s `Entry::Thought` arm: destructure `text, open, duration_ms`, take `streaming: bool` as computed by the caller (`this.thought_is_streaming(entry_index)` — the list's row processor has `&mut Chat` through `cx.processor`; if `render_entry` is a static fn, add a `thought_streaming: bool` parameter the processor fills), compute `let is_open = open.get(streaming);`, build the column `div().w_full().flex().flex_col().gap(px(4.0))` with the header (`entity: Some(entity.clone())`), and — until Task 4 — the existing body block guarded by `is_open` (keep its `thought-body-N` id). Delete the old header code and the `Icon::ChevronDown/Right` use if nothing else uses them in this file.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p sirio_ui chat::`
Expected: the two new tests pass; `thought_starts_collapsed_and_toggles_on_click` still passes (a pushed thought with `started: None` is not streaming → closed; click → open; click → closed); failing list = the 3 baseline reds.

- [ ] **Step 5: Commit**

```bash
git add rust/crates/sirio_ui/src/chat
git commit -m "feat(chat): thought header on the gallery's activity row with takeover open state"
```

---

### Task 4: The body — scrolling well, fade strip, follow pin and scrollbar

**Files:**
- Modify: `rust/crates/sirio_ui/src/chat/thought.rs` — `impl Chat { render_thought_body }`
- Modify: `rust/crates/sirio_ui/src/chat/mod.rs` — `Chat.thought_scroll: HashMap<usize, ThoughtScroll>` (+ `Chat::new`, cleared wherever `self.entries` is cleared), the `Entry::Thought` arm uses `render_thought_body`.

**Interfaces:**
- Consumes: `ThoughtScroll` (Task 1), `render_plain_text` (existing, `chat/mod.rs:4045`: `(text: String, theme: &Theme, id: String, source_start: usize, interaction: Option<&TranscriptInteraction>) -> AnyElement`).
- Produces: `pub(crate) fn render_thought_body(entry_index: usize, text: &str, source_start: usize, interaction: &TranscriptInteraction, scroll: &ThoughtScroll, theme: &Theme, bezel_theme: &bezel::theme::Theme) -> AnyElement`.

Selectors: `thought-body-N` (kept, on the outer `ml 10 pl 12 border_l_1` frame), new `thought-fade-N` on the gradient strip, `thought-well-N` on the scrolling well; the scrollbar's id is `thought-bar-N`.

- [ ] **Step 1: Write the failing test**

```rust
    /// An open body is the gallery's reasoning box: a capped scrolling well
    /// with the fade strip along its top and the follow pin + scrollbar laid
    /// over it. A closed thought draws none of it.
    #[gpui::test]
    async fn an_open_thought_body_is_a_capped_well_with_a_fade_strip(cx: &mut TestAppContext) {
        cx.update(Theme::init);
        cx.update(bezel::ui::input::init);
        let long: String = (0..60).map(|i| format!("line {i}\n")).collect();
        let (chat, cx) = cx.add_window_view(|_, cx| {
            let mut chat = Chat::new(None, std::env::temp_dir(), cx);
            chat.push_entry(Entry::Thought {
                text: long,
                open: Default::default(),
                started: None,
                duration_ms: Some(2_000),
            });
            chat
        });
        cx.update(|_window, cx| init(cx));
        refresh_frame(cx);
        assert!(cx.debug_bounds("thought-well-0").is_none(), "closed: no well");
        assert!(cx.debug_bounds("thought-fade-0").is_none(), "closed: no fade");

        let header = cx.debug_bounds("thought-toggle-0").expect("header");
        cx.simulate_click(header.center(), Modifiers::none());
        refresh_frame(cx);
        let body = cx.debug_bounds("thought-body-0").expect("open: the body");
        let well = cx.debug_bounds("thought-well-0").expect("open: the well");
        let fade = cx.debug_bounds("thought-fade-0").expect("open: the fade strip");
        assert!(well.size.height <= px(160.0), "the well is capped at 160: {well:?}");
        assert_eq!(fade.size.height, px(20.0));
        assert_eq!(fade.top(), well.top(), "the strip sits on the well's top edge");
        assert!(body.left() < well.left(), "the well is inset from the border line");
        chat.read_with(cx, |chat, _| {
            assert!(chat.thought_scroll.contains_key(&0), "scroll state was created on first draw");
        });
    }
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p sirio_ui chat::tests::an_open_thought_body`
Expected: FAIL — `thought-well-0` not found.

- [ ] **Step 3: Implement**

`chat/mod.rs`: field `thought_scroll: HashMap<usize, ThoughtScroll>` (doc: "Per-entry scroll state for open thought bodies, keyed by entry index; created on first draw, cleared with the entries."), `HashMap::new()` in `Chat::new`, `self.thought_scroll.clear()` next to every `self.entries.clear()` / `self.entries = Vec::new()` (grep). In the row renderer (`&mut Chat` through `cx.processor` — verify; otherwise create the state in the `ThoughtChunk` arm and in the restore loop instead, both of which have `cx`), before rendering an open thought:

```rust
let painter = bezel::motion::Painter::of(cx);
let scroll = this
    .thought_scroll
    .entry(entry_index)
    .or_insert_with(|| ThoughtScroll::new(painter));
```

`chat/thought.rs`:

```rust
use bezel::ui::scroll;
use gpui::{linear_color_stop, linear_gradient};

/// The gallery's `LiveActivity.svelte` cap and mask.
const BOX_MAX: f32 = 160.0;
const FADE: f32 = 20.0;

impl Chat {
    /// The gallery's `Activity::reasoning`: a border line down the left, a
    /// well capped at 160 px that scrolls past that, the 20 px fade painted
    /// over its top edge, the follow pin and the scrollbar laid over the
    /// same container.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn render_thought_body(
        entry_index: usize,
        text: &str,
        source_start: usize,
        interaction: &TranscriptInteraction,
        scroll: &ThoughtScroll,
        theme: &Theme,
        bezel_theme: &bezel::theme::Theme,
    ) -> AnyElement {
        let typography = theme.typography;
        div()
            .id(("thought-body", entry_index))
            .debug_selector(move || format!("thought-body-{entry_index}"))
            .ml(px(10.0))
            .pl(px(12.0))
            .border_l_1()
            .border_color(bezel_theme.border)
            .child(
                div()
                    .relative()
                    .max_h(px(BOX_MAX))
                    .child(
                        div()
                            .id(("thought-well", entry_index))
                            .debug_selector(move || format!("thought-well-{entry_index}"))
                            .max_h(px(BOX_MAX))
                            .overflow_y_scroll()
                            .track_scroll(&scroll.scroll)
                            .pr(px(14.0))
                            .text_size(typography.callout)
                            .line_height(px(19.0))
                            .text_color(theme.text_muted.opacity(0.7))
                            .child(Self::render_plain_text(
                                text.to_string(),
                                theme,
                                format!("thought-entry-{entry_index}"),
                                source_start,
                                Some(interaction),
                            )),
                    )
                    .child(
                        div()
                            .debug_selector(move || format!("thought-fade-{entry_index}"))
                            .absolute()
                            .top_0()
                            .left_0()
                            .right_0()
                            .h(px(FADE))
                            .bg(linear_gradient(
                                180.0,
                                linear_color_stop(bezel_theme.bg, 0.0),
                                linear_color_stop(bezel_theme.bg.opacity(0.0), 1.0),
                            )),
                    )
                    .child(scroll::follow(&scroll.scroll, &scroll.follow))
                    .child(scroll::scrollbar(
                        format!("thought-bar-{entry_index}"),
                        &scroll.scroll,
                        &scroll.bar,
                    )),
            )
            .into_any_element()
    }
}
```

Keep `render_plain_text`'s selection wiring exactly as the old body had it (F-CHAT-31). The old body was italic; the gallery's is not — drop `.italic()` (the spec transcribes the gallery). A new thought re-follows: when the `ThoughtChunk` arm *creates* a thought, remove any stale `thought_scroll` entry for that index (`self.thought_scroll.remove(&index)`), so the next draw starts with a fresh `FollowState`.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p sirio_ui chat::`
Expected: the new test passes; failing list = the 3 baseline reds. If `fade.top() == well.top()` is off by the border, compare against the `relative` container instead and say so in the commit body.

- [ ] **Step 5: Commit**

```bash
git add rust/crates/sirio_ui/src/chat
git commit -m "feat(chat): thought body as the gallery's reasoning well with follow pin and scrollbar"
```

---

### Task 5: The generating spinner (#239) adopts the header row

**Files:**
- Modify: `rust/crates/sirio_ui/src/chat/mod.rs` — the `chat-generating-spinner` block in `Render for Chat` (~line 7020–7050).

**Interfaces:**
- Consumes: `render_thought_header` with `entity: None` (Task 3).

- [ ] **Step 1: Write the failing test**

```rust
    /// The transient generating spinner is the same row a live thought's
    /// header is: orb, `Thinking`, same paddings — one shape for a run in
    /// progress whether or not a thought has arrived.
    #[gpui::test]
    async fn the_generating_spinner_shares_the_thought_header_shape(cx: &mut TestAppContext) {
        cx.update(Theme::init);
        cx.update(bezel::ui::input::init);
        let (chat, cx) = cx.add_window_view(|_, cx| Chat::new(None, std::env::temp_dir(), cx));
        cx.update(|_window, cx| init(cx));
        chat.update(cx, |chat, cx| {
            chat.streaming = true;
            chat.handle_event(AcpEvent::ThoughtChunk("a".into()), cx);
        });
        refresh_frame(cx);
        let header = cx.debug_bounds("thought-toggle-0").expect("live thought header");
        chat.update(cx, |chat, cx| {
            chat.handle_event(AcpEvent::AgentMessageChunk("b".into()), cx);
        });
        refresh_frame(cx);
        let spinner = cx
            .debug_bounds("chat-generating-spinner")
            .expect("the spinner row while the turn streams");
        assert_eq!(
            spinner.size.height, header.size.height,
            "one row shape: spinner={spinner:?} header={header:?}"
        );
        assert!(
            cx.debug_bounds("thought-streaming-usize::MAX").is_none(),
            "the spinner carries no entry markers"
        );
    }
```

Read `the_generating_spinner_appears_while_a_turn_streams` first and reuse its way of getting the spinner drawn (it may need `has_completed_turn`/`streaming` set differently); keep the height comparison as the assertion that matters.

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p sirio_ui chat::tests::the_generating_spinner_shares`
Expected: FAIL — the old spinner row (`text_size scaled(12.5)`, no `line_height 19`) is a different height.

- [ ] **Step 3: Implement**

Replace the spinner's inner row with `Self::render_thought_header(usize::MAX, true, false, None, &theme, &bezel_theme, window, cx, None)` wrapped in the existing `chat-generating-spinner` container (keep that id + selector and its `pb`/margins). The `usize::MAX` index only feeds the marker ids and the row id; nothing reads them. Drop the second marker assertion from the test if `debug_selector` ids cannot contain `usize::MAX` textually — replace it with `cx.debug_bounds("thought-toggle-18446744073709551615").is_some()` or simply remove it.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p sirio_ui chat::`
Expected: new test passes; `the_generating_spinner_appears_while_a_turn_streams`, `the_generating_spinner_disappears_when_the_turn_ends`, `the_running_reasoning_header_shows_the_thinking_indicator` unchanged and green; failing list = the 3 baseline reds.

- [ ] **Step 5: Commit**

```bash
git add rust/crates/sirio_ui/src/chat/mod.rs
git commit -m "feat(chat): draw the generating spinner as the thought header row"
```

---

### Task 6: Sweep, format, build, record

**Files:**
- Modify: `rust/crates/sirio_ui/src/chat/mod.rs`, `rust/crates/sirio_ui/src/chat/thought.rs`
- Modify: `docs/superpowers/specs/2026-09-04-bezel-agent-parity-design.md` (Delivery list only)

- [ ] **Step 1: Sweep**

`cargo clippy -p sirio_ui --all-targets`: no warning in `chat/thought.rs`; no new warning in `chat/mod.rs`. Delete anything the cut-over left dead: `toggle_thought_expanded`, `Icon::ChevronDown/ChevronRight` imports if unused, the old italic/label branches, `a_settled_reasoning_header_never_invents_an_elapsed_time` only if it now duplicates `the_label_follows_the_state` (otherwise keep it). Doc comments that name `expanded` on a thought are rewritten to name `open`.

- [ ] **Step 2: Format**

`rustfmt --edition 2024 rust/crates/sirio_ui/src/chat/mod.rs rust/crates/sirio_ui/src/chat/thought.rs`. Nothing else.

- [ ] **Step 3: Build and full chat run**

`cargo build -p sirio` (own target dir; `taskkill /IM sirio.exe /F` first if the exe is locked), then `cargo test -p sirio_ui chat::` — failing list = the 3 baseline reds.

- [ ] **Step 4: Record**

Under "Delivery" in the spec add `- Sub-project 3 landed on branch feat/thought-takeover (2026-09-04).` after the sub-project 2 line. Touch nothing else in the spec.

- [ ] **Step 5: Commit**

```bash
git add rust/crates/sirio_ui/src/chat docs/superpowers/specs/2026-09-04-bezel-agent-parity-design.md
git commit -m "chore(chat): sweep the thought cut-over and record sub-project 3"
```

---

## Self-review

- **Spec coverage:** header (Task 3), duration (Task 2), open state / Takeover (Tasks 2–3), body with well/fade/follow/scrollbar and `thought_scroll` (Task 4), spinner #239 (Task 5), tests listed in the spec (label by state — Task 1; Takeover semantics — Task 3; duration measured/persisted/absent on restore — Task 2; body only when open — Task 4).
- **Type consistency:** `Entry::Thought { text, open, started, duration_ms }` everywhere from Task 2 on; `ThoughtScroll { scroll, follow, bar }` from Task 1; `render_thought_header(entry_index, streaming, open, duration_ms, theme, bezel_theme, window, cx, entity)`; `render_thought_body(entry_index, text, source_start, interaction, scroll, theme, bezel_theme)`.
- **Known unknowns, resolved in-task:** whether the list row processor hands out `&mut Chat` (Task 4 gives the fallback); `FollowState::new()`'s initial state (Task 1); the `TurnEnded` payload shape and the `ChatEntry` JSON tag (Task 2).
