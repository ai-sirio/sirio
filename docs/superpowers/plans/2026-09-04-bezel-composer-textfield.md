# Bezel Composer on TextField — Implementation Plan (sub-project 1 of 4)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace Sirio's hand-rolled chip composer with bezel's `TextField` inside the gallery's Composer card, with skill/file mentions as text tokens, images as an attachment strip, and every popup on `bezel::ui::popover`.

**Architecture:** `Chat` (in `rust/crates/sirio_ui/src/chat/mod.rs` after Task 1) owns an `Entity<TextField>` and observes it; a cached `(draft, caret)` pair drives the `/` and `@` token popups through `popover::Filter`; the card, control row and popups are rebuilt in a new `chat/composer_view.rs` module from the gallery's `Composer::render`. The chip document model (`composer.rs`), the custom `ComposerText` element and the composer caret blink are deleted.

**Tech Stack:** Rust, gpui (`bezel-gpui =0.3.8`), bezel `=0.1.4` (`ui::input::TextField`, `ui::popover`, `ui::icons`, `theme::Theme`, `motion::{Fade, Painter}`), `#[gpui::test]` visual tests with `VisualTestContext`.

**Spec:** `docs/superpowers/specs/2026-09-04-bezel-agent-parity-design.md` — section "Sub-project 1 — Composer". Read it first; this plan argues from it.

## Global Constraints

- bezel pinned `=0.1.4`, gpui `=0.3.8`: no dependency or version changes. The reference is tag `v0.1.4` of `crabtalk/bezel`, `apps/gallery/src/patterns/agent.rs` lines 609–939 (the `Composer` section).
- Iterate with `cargo test -p sirio_ui` and `cargo clippy -p sirio_ui`. **Never** run `Scripts/ci.sh` or `Scripts/ci-linux.sh`.
- Windows toolchain: `export PATH="$HOME/.cargo/bin:/d/toolchains/zig/zig-x86_64-windows-0.15.2:$PATH"` before any cargo command (Zig exactly 0.15.2). Kill a running `sirio.exe` before `cargo build -p sirio`.
- Every `debug_selector` used by the `sirio` crate's tests is preserved: `composer`, `composer-input`, `send`, `attach-image`, `slash-popup`, `slash-option-<name>`, `mention-popup`, `mention-option-<path>`, `queued-item`, `queued-text-<text>`, `queued-remove`, `attach-error`, `chat-status`, `chat-connecting`, `model-chip`, `effort-chip`, `agent-badge`, `context-ring`, `composer-overflow`, `composer-overflow-menu`, `overflow-*`, `chat-history-*`, `model-picker`, `mode-picker`, `context-popover`, `model-search-input`.
- Commit messages: Conventional Commits, lower-case imperative subject, trailer lines
  `Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>` and
  `Claude-Session: https://claude.ai/code/session_015gJ251HyJzHR2P49ipCEZL`.
- Persistence unchanged in this sub-project: `draft_text()` still returns the plain draft text and `control_compose` still restores it.
- No user-visible behaviour change other than: chips become text tokens / an attachment strip; the card looks like the gallery; pickers are bezel menus.

---

## Precondition (orchestrator, before Task 1)

The working tree carries unrelated uncommitted work (`rust/crates/sirio/src/main.rs` tab redesign, `rust/crates/sirio_registry/src/{installer,store}.rs`, `rust/crates/sirio_ui/src/chat.rs` skill-popup tooltip). Commit or stash it, then:

```bash
git checkout -b feat/composer-bezel-textfield main
```

---

### Task 1: Move `chat.rs` into `chat/mod.rs`

**Files:**
- Move: `rust/crates/sirio_ui/src/chat.rs` → `rust/crates/sirio_ui/src/chat/mod.rs`

**Interfaces:**
- Produces: the module path `sirio_ui::chat` unchanged; new sibling files can be declared with `mod composer_view;` inside `chat/mod.rs`.

- [ ] **Step 1: Move the file with git**

```bash
cd rust/crates/sirio_ui/src && mkdir chat && git mv chat.rs chat/mod.rs
```

- [ ] **Step 2: Build and run the chat tests to prove nothing changed**

Run: `cd rust && cargo test -p sirio_ui chat:: 2>&1 | tail -5`
Expected: the same pass/fail list as on `main` (compare against a run on `main` first if unsure; two timing-sensitive tests may flake — rerun those alone).

- [ ] **Step 3: Commit**

```bash
git add -A rust/crates/sirio_ui/src/chat rust/crates/sirio_ui/src/chat.rs
git commit -m "refactor(chat): move chat.rs to chat/mod.rs for sibling modules"
```

---

### Task 2: Serve bezel's SVG icons from the app

bezel's `icons::icon(path)` is `svg().path("icons/<name>.svg")`, which gpui resolves through the application's `AssetSource`. Sirio's own icons embed their bytes (`svg().data(...)`) and never needed one, so `main.rs` registers none — every bezel SVG (tree chevrons, `changes.rs`'s `DOCUMENT`) paints nothing today. The composer's send arrow, stop square and paperclip are bezel SVGs, so this lands first.

**Files:**
- Modify: `rust/crates/sirio/src/main.rs` (the `application().run(|cx| { … })` call, ~line 16397)

**Interfaces:**
- Produces: bezel icons render in the app. Tests do not need it — `svg` still lays out (and `debug_bounds` works) when the asset is missing.

- [ ] **Step 1: Register bezel's asset source**

In `rust/crates/sirio/src/main.rs`, change

```rust
    application().run(|cx: &mut App| {
```

to

```rust
    application()
        // bezel's icons are `svg().path("icons/…")`; without an asset source
        // gpui finds nothing and paints nothing. Sirio's own icons embed their
        // bytes and never needed this.
        .with_assets(bezel::ui::icons::Assets)
        .run(|cx: &mut App| {
```

and re-indent the closure body by one level (or leave it and let `cargo fmt` do it).

- [ ] **Step 2: Build the app**

Run: `cd rust && cargo build -p sirio 2>&1 | tail -3`
Expected: `Finished` with no new warnings.

- [ ] **Step 3: Commit**

```bash
git add rust/crates/sirio/src/main.rs
git commit -m "fix(app): serve bezel svg icons through the asset source"
```

---

### Task 3: Token parsing and prompt assembly (pure functions)

**Files:**
- Create: `rust/crates/sirio_ui/src/chat/composer_view.rs`
- Modify: `rust/crates/sirio_ui/src/chat/mod.rs` (add `mod composer_view;` after the `use` block at the top)

**Interfaces:**
- Produces (all `pub(crate)`, in `chat::composer_view`):
  - `fn slash_token(text: &str) -> Option<&str>` — `Some("cr")` for `"/cr"`, `None` once the draft has whitespace or does not start with `/`.
  - `fn mention_token(text: &str, caret: usize) -> Option<(usize, &str)>` — the byte index of the `@` nearest behind `caret` and the token typed after it; `None` when there is whitespace between them or no `@`.
  - `fn assemble_prompt(text: &str, accepted: &[String]) -> (String, Vec<String>)` — the text with every accepted `@path` token removed and those paths as `mention_paths`, deduplicated, in text order.

- [ ] **Step 1: Write the failing tests**

Create `rust/crates/sirio_ui/src/chat/composer_view.rs`:

```rust
//! The composer: bezel's `TextField` in the gallery's Composer card, with the
//! `/` and `@` tokens read off the text the way the gallery's `reread` does.
//!
//! Transcribed from `crabtalk/bezel` tag `v0.1.4`,
//! `apps/gallery/src/patterns/agent.rs` (the `Composer` section). A chip used
//! to be one atomic position in a hand-rolled document; now a skill is the
//! `/name ` prefix as text, a file mention is `@path ` as text with the path
//! remembered in `Chat::accepted_mentions`, and an image is an entry in
//! `Chat::attachments` drawn as a strip above the field.

/// The active `/`-token: the whole draft is one unbroken word starting with
/// a slash. Whitespace anywhere ends the token, exactly as the reference
/// `slashTokenRange` did.
pub(crate) fn slash_token(text: &str) -> Option<&str> {
    let rest = text.strip_prefix('/')?;
    (!rest.chars().any(char::is_whitespace)).then_some(rest)
}

/// The active `@`-token behind `caret`: the nearest `@` with nothing but
/// non-whitespace between it and the caret. Returns the byte index of the
/// `@` and the token after it. A read of the text rather than a key handler,
/// so typing, pasting, arrowing back into a word and deleting the `@` all
/// agree without special cases.
pub(crate) fn mention_token(text: &str, caret: usize) -> Option<(usize, &str)> {
    let caret = caret.min(text.len());
    let head = text.get(..caret)?;
    let at = head.rfind('@')?;
    let token = &head[at + 1..];
    (!token.chars().any(char::is_whitespace)).then_some((at, token))
}

/// What the composer hands to the ACP layer: the text with every accepted
/// `@path` token removed, and those paths as mention paths (deduplicated, in
/// the order they appear). A token the user edited no longer matches an
/// accepted path and stays as plain text — the same triple the old chip
/// document produced.
pub(crate) fn assemble_prompt(text: &str, accepted: &[String]) -> (String, Vec<String>) {
    let mut out = String::with_capacity(text.len());
    let mut mention_paths: Vec<String> = Vec::new();
    let mut rest = text;
    while let Some(at) = rest.find('@') {
        out.push_str(&rest[..at]);
        let after = &rest[at + 1..];
        let token_end = after.find(char::is_whitespace).unwrap_or(after.len());
        let token = &after[..token_end];
        let boundary_before = out.is_empty() || out.ends_with(char::is_whitespace);
        if boundary_before && !token.is_empty() && accepted.iter().any(|path| path == token) {
            if !mention_paths.iter().any(|path| path == token) {
                mention_paths.push(token.to_string());
            }
            let mut skip = token_end;
            if after[token_end..].starts_with(' ') {
                skip += 1;
            }
            rest = &after[skip..];
        } else {
            out.push('@');
            rest = after;
        }
    }
    out.push_str(rest);
    (out, mention_paths)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slash_token_is_the_unbroken_leading_word() {
        assert_eq!(slash_token("/"), Some(""));
        assert_eq!(slash_token("/cr"), Some("cr"));
        assert_eq!(slash_token("/cr "), None);
        assert_eq!(slash_token("hi /cr"), None);
        assert_eq!(slash_token(""), None);
    }

    #[test]
    fn mention_token_is_the_at_nearest_behind_the_caret() {
        assert_eq!(mention_token("see @src/ma", 11), Some((4, "src/ma")));
        assert_eq!(mention_token("see @", 5), Some((4, "")));
        assert_eq!(mention_token("see @src done", 13), None);
        assert_eq!(mention_token("see @src done", 8), Some((4, "src")));
        assert_eq!(mention_token("no at here", 10), None);
        assert_eq!(mention_token("@a", 99), Some((0, "a")), "caret past the end clamps");
    }

    #[test]
    fn assemble_prompt_lifts_accepted_tokens_into_mention_paths() {
        let accepted = vec!["src/main.rs".to_string()];
        let (text, paths) = assemble_prompt("fix @src/main.rs please", &accepted);
        assert_eq!(text, "fix please");
        assert_eq!(paths, vec!["src/main.rs".to_string()]);
    }

    #[test]
    fn assemble_prompt_leaves_edited_and_unaccepted_tokens_as_text() {
        let accepted = vec!["src/main.rs".to_string()];
        let (text, paths) = assemble_prompt("fix @src/main.rss and @other", &accepted);
        assert_eq!(text, "fix @src/main.rss and @other");
        assert!(paths.is_empty());
        let (text, paths) = assemble_prompt("mail me@example.com", &["example.com".to_string()]);
        assert_eq!(text, "mail me@example.com", "an @ inside a word is not a token");
        assert!(paths.is_empty());
    }

    #[test]
    fn assemble_prompt_deduplicates_and_keeps_text_order() {
        let accepted = vec!["b.rs".to_string(), "a.rs".to_string()];
        let (text, paths) = assemble_prompt("@a.rs then @b.rs then @a.rs end", &accepted);
        assert_eq!(text, "then then end");
        assert_eq!(paths, vec!["a.rs".to_string(), "b.rs".to_string()]);
    }
}
```

Add to `rust/crates/sirio_ui/src/chat/mod.rs`, right after the last top-level `use …;` line:

```rust
mod composer_view;
```

- [ ] **Step 2: Run the tests**

Run: `cd rust && cargo test -p sirio_ui chat::composer_view 2>&1 | tail -8`
Expected: 5 passed. (The functions are written with the tests because they are three pure functions; the tests are the contract the later tasks build on.)

- [ ] **Step 3: Commit**

```bash
git add rust/crates/sirio_ui/src/chat/composer_view.rs rust/crates/sirio_ui/src/chat/mod.rs
git commit -m "feat(chat): composer token parsing and prompt assembly"
```

---

### Task 4: Cut the composer over to bezel `TextField`

The one large task: the field, the cached draft, tokens instead of chips, keys, and the tests that drive them. The crate compiles only at the end of the task; commit once.

**Files:**
- Modify: `rust/crates/sirio_ui/src/chat/mod.rs` — struct fields (~1432–1566), `new` (~1684–1780), `bind_keys` (~1916), `actions!` (~401), `can_send` (~2409), `draft_text`/`control_compose` (~2486–2500), slash/mention/attach handlers (~3013–3160, 3260), `send`/`commit_queued_item`/`reset_composer_popups` (~3343–3450), `insert_text`…`cancel` (~3684–3775), `backspace`…`end` handlers (~3933–4050), `on_composer_key` (~4040–4160), `render_composer` (~5928–7600), root `render` (~7680–7710), `impl Focusable` (~7634), `ChatSessionState.composer_text` fill (~2561), tests (~8600–13800)
- Delete: `rust/crates/sirio_ui/src/composer.rs`; the `pub mod composer;` line in `rust/crates/sirio_ui/src/lib.rs`
- Modify: `rust/crates/sirio_ui/src/chat/composer_view.rs` (add the `impl Chat` block)

**Interfaces:**
- Consumes: `composer_view::{slash_token, mention_token, assemble_prompt}` (Task 3).
- Produces (on `Chat`, all `pub(crate)` unless noted):
  - fields `composer_field: Entity<bezel::ui::input::TextField>`, `draft: SharedString`, `draft_caret: usize`, `accepted_mentions: Vec<String>`, `attachments: Vec<ImageAttachment>`, `composer_placeholder_shown: String`, `slash_filter: bezel::ui::popover::Filter`, `mention_filter: bezel::ui::popover::Filter`
  - `pub fn draft_text(&self) -> String` (unchanged signature)
  - `fn set_composer_text(&mut self, text: impl Into<SharedString>, cx: &mut Context<Self>)`
  - `fn reread_composer(&mut self, cx: &mut Context<Self>)`
  - `fn composer_disabled(&self) -> bool`, `fn composer_placeholder(&self) -> String`
  - `fn open_token_popup(&self) -> TokenPopup` with `enum TokenPopup { None, Slash, Mention }`
  - `fn remove_attachment(&mut self, index: usize, cx: &mut Context<Self>)`
  - actions `chat_composer::{Send, Cancel, CopyTranscript, PopupPrevious, PopupNext, PopupAccept}`
  - selectors `attachment-strip`, `attachment-chip-N`, `attachment-remove-N`

- [ ] **Step 1: Replace the action set**

In `chat/mod.rs` replace the `actions!(chat_composer, [...])` block with:

```rust
actions!(
    chat_composer,
    [Send, Cancel, CopyTranscript, PopupPrevious, PopupNext, PopupAccept]
);
```

Delete every handler for the removed actions and their `.on_action(...)` lines on `chat-root`: `newline`, `backspace`, `delete`, `left`, `right`, `select_left`, `select_right`, `select_all`, `home`, `end`. Delete `insert_text` and `flip_composer_blink`.

- [ ] **Step 2: Replace the fields and the constructor**

In `struct Chat`, delete `composer`, `composer_blink`, `composer_caret_sig`, `composer_paint`, `composer_focus`. Add:

```rust
    /// The draft, as bezel's field: IME, selection, undo, wrapping and scroll
    /// are its job. `Chat` observes it and reads `content()`/`cursor()` into
    /// `draft`/`draft_caret` on every change — the popups and Send read the
    /// cached pair rather than borrowing the entity mid-render.
    composer_field: Entity<TextField>,
    draft: SharedString,
    draft_caret: usize,
    /// File paths accepted from the `@` picker while their `@path` token is
    /// still in the draft. Lifted into the prompt's mention paths on send;
    /// cleared on send and on `control_compose`.
    accepted_mentions: Vec<String>,
    /// Images attached through the picker or a drop, drawn as a strip above
    /// the field. Not persisted, as before.
    attachments: Vec<ImageAttachment>,
    /// The placeholder last pushed into the field, so render pushes a new
    /// one only when the state it names changed.
    composer_placeholder_shown: String,
    /// Ranked views over `available_commands` and `mention_candidates`, with
    /// the keyboard's active row — bezel's own picker state.
    slash_filter: popover::Filter,
    mention_filter: popover::Filter,
```

Delete `slash_selected` (the filter's `active()` replaces it). Add the imports at the top of `chat/mod.rs`:

```rust
use bezel::ui::input::TextField;
use bezel::ui::popover;
```

In `Chat::new`, before `Self { … }`:

```rust
        let composer_field = cx.new(|cx| {
            TextField::new(cx)
                .with_shape(bezel::ui::input::Shape::Grow { min: 3, max: 12 })
                .with_key_context("ChatComposer")
        });
        // Both content and caret changes notify, and the mention behind the
        // caret changes when either does.
        cx.observe(&composer_field, |chat: &mut Self, _, cx| chat.reread_composer(cx))
            .detach();
```

and in the struct literal replace the deleted fields with:

```rust
            composer_field,
            draft: SharedString::default(),
            draft_caret: 0,
            accepted_mentions: Vec::new(),
            attachments: Vec::new(),
            composer_placeholder_shown: String::new(),
            slash_filter: popover::Filter::new(Vec::new()),
            mention_filter: popover::Filter::new(Vec::new()),
```

Replace `impl Focusable for Chat`:

```rust
impl Focusable for Chat {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.composer_field.read(cx).focus_handle(cx)
    }
}
```

Every other `self.composer_focus` use becomes `self.composer_field.read(cx).focus_handle(cx)` (the card's `on_mouse_down`, the answer-field return-focus paths, `clear_question_answer_focus` callers). `bezel::ui::input::TextField` implements `gpui::Focusable`, so `focus_handle(cx)` needs `use gpui::Focusable as _` in scope — it already is (`impl Focusable for Chat`).

- [ ] **Step 3: The draft plumbing in `composer_view.rs`**

Append to `rust/crates/sirio_ui/src/chat/composer_view.rs`:

```rust
use gpui::{Context, SharedString};

use super::{Chat, PopupAccept, PopupNext, PopupPrevious};

/// Which token picker is on screen, if any. Enter/up/down/tab belong to it
/// while it is; otherwise they fall through to the field.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TokenPopup {
    None,
    Slash,
    Mention,
}

impl Chat {
    /// F-CHAT-05: the composer is out of service while a permission/plan
    /// question is unanswered or the agent is disconnected — the whole
    /// editor, not just Send, mirroring the Swift `.disabled(!canInteract)`.
    pub(crate) fn composer_disabled(&self) -> bool {
        self.pending_question().is_some() || self.is_offline()
    }

    /// The field's placeholder names the state the composer is in.
    pub(crate) fn composer_placeholder(&self) -> String {
        if self.pending_question().is_some() {
            "Waiting for permission response…".to_string()
        } else if self.streaming {
            "Type to queue for the next turn…".to_string()
        } else if self.is_offline() {
            "Agent offline — reconnecting when you send…".to_string()
        } else {
            self.default_placeholder()
        }
    }

    /// Replace the draft programmatically: the cache first, so the observer
    /// that fires next sees nothing to revert.
    pub(crate) fn set_composer_text(
        &mut self,
        text: impl Into<SharedString>,
        cx: &mut Context<Self>,
    ) {
        let text: SharedString = text.into();
        self.draft = text.clone();
        self.draft_caret = text.len();
        self.composer_field
            .update(cx, |field, cx| field.set_content(text, cx));
        self.refresh_token_popups(cx);
    }

    /// The observer: runs after every content or caret change in the field.
    /// A disabled composer refuses edits by putting the last accepted draft
    /// back (`offline_enter_never_discards_the_typed_draft`).
    pub(crate) fn reread_composer(&mut self, cx: &mut Context<Self>) {
        let (content, caret) = {
            let field = self.composer_field.read(cx);
            (field.content().clone(), field.cursor())
        };
        if content == self.draft && caret == self.draft_caret {
            return;
        }
        if self.composer_disabled() && content != self.draft {
            let last = self.draft.clone();
            self.composer_field
                .update(cx, |field, cx| field.set_content(last, cx));
            return;
        }
        self.draft = content;
        self.draft_caret = caret;
        self.refresh_token_popups(cx);
        cx.notify();
    }

    pub(crate) fn open_token_popup(&self) -> TokenPopup {
        if self.slash_popup_visible() {
            TokenPopup::Slash
        } else if self.mention_popup_visible() {
            TokenPopup::Mention
        } else {
            TokenPopup::None
        }
    }

    pub(crate) fn mention_popup_visible(&self) -> bool {
        mention_token(&self.draft, self.draft_caret).is_some()
            && !self.mention_filter.filtered().is_empty()
    }

    /// `up`: the popup's row when one is open, otherwise the field's own
    /// vertical motion — `propagate` lets gpui try the field's binding next.
    pub(crate) fn popup_previous(
        &mut self,
        _: &PopupPrevious,
        _: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) {
        match self.open_token_popup() {
            TokenPopup::Slash => self.slash_filter.step(-1),
            TokenPopup::Mention => self.mention_filter.step(-1),
            TokenPopup::None => return cx.propagate(),
        }
        cx.notify();
    }

    pub(crate) fn popup_next(
        &mut self,
        _: &PopupNext,
        _: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) {
        match self.open_token_popup() {
            TokenPopup::Slash => self.slash_filter.step(1),
            TokenPopup::Mention => self.mention_filter.step(1),
            TokenPopup::None => return cx.propagate(),
        }
        cx.notify();
    }

    /// `tab`: accept the active row; with no popup, ordinary focus traversal.
    pub(crate) fn popup_accept(
        &mut self,
        _: &PopupAccept,
        _: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) {
        if !self.accept_active_popup_row(cx) {
            cx.propagate();
        }
    }

    /// Accept whichever picker is open. `true` when one was.
    pub(crate) fn accept_active_popup_row(&mut self, cx: &mut Context<Self>) -> bool {
        match self.open_token_popup() {
            TokenPopup::Slash => {
                if let Some(item) = self.slash_filter.active_item() {
                    let name = self.slash_filter.items()[item].to_string();
                    self.accept_slash_command(&name, cx);
                }
                true
            }
            TokenPopup::Mention => {
                if let Some(item) = self.mention_filter.active_item() {
                    let path = self.mention_filter.items()[item].to_string();
                    self.accept_mention(&path, cx);
                }
                true
            }
            TokenPopup::None => false,
        }
    }

    pub(crate) fn remove_attachment(&mut self, index: usize, cx: &mut Context<Self>) {
        if index < self.attachments.len() {
            self.attachments.remove(index);
            cx.notify();
        }
    }
}
```

- [ ] **Step 4: Rewire the existing handlers in `chat/mod.rs`**

Replace these methods (keep their doc comments where still true):

```rust
    fn can_send(&self) -> bool {
        !self.streaming
            && !self.connecting
            && self.pending_question().is_none()
            && !self.is_offline()
            && (!self.draft.trim().is_empty() || !self.attachments.is_empty())
    }

    pub fn draft_text(&self) -> String {
        self.draft.to_string()
    }

    pub fn control_compose(&mut self, text: &str, cx: &mut Context<Self>) {
        self.accepted_mentions.clear();
        self.attachments.clear();
        self.reset_composer_popups();
        self.set_composer_text(text.to_string(), cx);
        cx.notify();
    }

    fn slash_candidates(&self) -> Vec<&AvailableCommandInfo> {
        if slash_token(&self.draft).is_none() {
            return Vec::new();
        }
        self.slash_filter
            .filtered()
            .iter()
            .take(10)
            .filter_map(|&item| {
                let name = self.slash_filter.items()[item].as_ref();
                self.available_commands.iter().find(|command| command.name == name)
            })
            .collect()
    }

    fn slash_popup_visible(&self) -> bool {
        !self.slash_candidates().is_empty() && !self.slash_dismissed
    }

    fn accept_slash_command(&mut self, name: &str, cx: &mut Context<Self>) {
        self.slash_dismissed = true;
        self.set_composer_text(format!("/{name} "), cx);
    }

    fn accept_mention(&mut self, path: &str, cx: &mut Context<Self>) {
        let Some((at, _)) = mention_token(&self.draft, self.draft_caret) else {
            return;
        };
        let caret = self.draft_caret.min(self.draft.len());
        let text = format!("{}@{path} {}", &self.draft[..at], &self.draft[caret..]);
        if !self.accepted_mentions.iter().any(|known| known == path) {
            self.accepted_mentions.push(path.to_string());
        }
        self.mention_candidates.clear();
        self.mention_filter = popover::Filter::new(Vec::new());
        self.set_composer_text(text, cx);
    }

    fn refresh_token_popups(&mut self, cx: &mut Context<Self>) {
        let slash = slash_token(&self.draft).map(str::to_string);
        if slash != self.last_slash_token {
            self.slash_dismissed = false;
            if let Some(query) = &slash {
                self.slash_filter.refilter(query);
            }
            self.last_slash_token = slash;
        }
        let mention = mention_token(&self.draft, self.draft_caret).map(|(_, token)| token.to_string());
        if mention != self.mention_query {
            self.mention_candidates.clear();
            self.mention_filter = popover::Filter::new(Vec::new());
            self.mention_query = mention;
            self.schedule_mention_walk(cx);
        }
    }
```

Add `use composer_view::{assemble_prompt, mention_token, slash_token, TokenPopup};` next to `mod composer_view;`. Delete `accept_slash_selection` (its two callers now use `accept_active_popup_row`).

Where `available_commands` is assigned from the ACP event (search `available_commands =`), rebuild the filter right after:

```rust
                    self.slash_filter = popover::Filter::new(
                        self.available_commands
                            .iter()
                            .map(|command| SharedString::from(command.name.clone()))
                            .collect(),
                    );
                    if let Some(query) = slash_token(&self.draft) {
                        self.slash_filter.refilter(query);
                    }
```

In `schedule_mention_walk`'s completion closure, after `chat.mention_candidates = hits;` add:

```rust
                    chat.mention_filter = popover::Filter::new(
                        chat.mention_candidates
                            .iter()
                            .map(|path| SharedString::from(path.clone()))
                            .collect(),
                    );
```

`apply_attached_paths`: replace the `insert_chip_at_cursor` call with

```rust
        self.attachments.push(ImageAttachment {
            mime_type: mime.to_string(),
            base64_data: base64,
        });
        cx.notify();
```

(and drop the `refresh_token_popups` call there). Do the same in `drop_external_paths` where it inserts an image chip. Delete `remove_composer_chip`.

`send` and `commit_queued_item`: replace the `let draft = self.composer.draft(); self.composer = Composer::new(); self.reset_composer_popups();` pairs with

```rust
        let (text, mention_paths) = assemble_prompt(&self.draft, &self.accepted_mentions);
        let images = std::mem::take(&mut self.attachments);
        self.accepted_mentions.clear();
        self.reset_composer_popups();
        self.set_composer_text("", cx);
```

then `self.submit_turn(text, mention_paths, images, cx);` (in `send`) or `self.queued_item = Some(text);` (in `commit_queued_item`, whose emptiness check becomes `if self.draft.trim().is_empty() && self.attachments.is_empty() { return; }`).

`send_action`:

```rust
    fn send_action(&mut self, _: &Send, _: &mut Window, cx: &mut Context<Self>) {
        if self.model_picker_open {
            return;
        }
        if self.accept_active_popup_row(cx) {
            cx.notify();
            return;
        }
        self.send(cx);
    }
```

`cancel`: replace the `slash_popup_visible` arm with

```rust
        } else if self.open_token_popup() != TokenPopup::None {
            self.slash_dismissed = true;
            self.mention_candidates.clear();
            self.mention_filter = popover::Filter::new(Vec::new());
            cx.notify();
        } else {
```

`reset_composer_popups`: drop the `slash_selected = 0` line; add `self.mention_filter = popover::Filter::new(Vec::new());`.

`ChatSessionState` fill (`composer_text: self.composer.text()`) → `composer_text: self.draft.to_string()`.

`on_composer_key`: delete everything from the `if event.keystroke.key == "c" && …` check down to the end **except** the answer-field branch at the top and the model-picker branch — the keymap is always installed by `Chat::new`, so the fallbacks it carried are dead. Keep the `ctrl-c` transcript-copy branch.

`bind_keys`: the `ChatComposer` lines become exactly

```rust
            KeyBinding::new("enter", Send, Some("ChatComposer")),
            KeyBinding::new("return", Send, Some("ChatComposer")),
            KeyBinding::new("shift-enter", bezel::ui::input::InsertNewline, Some("ChatComposer")),
            KeyBinding::new("shift-return", bezel::ui::input::InsertNewline, Some("ChatComposer")),
            KeyBinding::new("escape", Cancel, Some("ChatComposer")),
            KeyBinding::new("up", PopupPrevious, Some("ChatComposer")),
            KeyBinding::new("down", PopupNext, Some("ChatComposer")),
            KeyBinding::new("tab", PopupAccept, Some("ChatComposer")),
            KeyBinding::new("ctrl-c", CopyTranscript, Some("ChatTranscript")),
```

(the picker and answer-field bindings below them stay). On `chat-root` in `render`, delete `.key_context("ChatComposer")` and `.track_focus(&self.composer_focus)`; the `on_action` list becomes `send_action`, `cancel`, `copy_transcript`, `popup_previous`, `popup_next`, `popup_accept`, `send_answer_action`, `cancel_answer_action`.

- [ ] **Step 5: Replace the input area in `render_composer`**

Delete: the caret block at the top of `render_composer` (from `let caret_sig = (` through `let (caret_part, caret_offset) = …;`), the whole `let mut composer_parts` block and the two `if self.composer.is_empty()` caret insertions after it, and the `ComposerText`/`ComposerPaintTrace` types and their `impl`s. Delete `caret_visible`, `composer_paint`, `caret_bar` locals and the `focused` local (recompute below).

At the top of `render_composer` add:

```rust
        let focused = self.composer_field.read(cx).focus_handle(cx).is_focused(window);
        let placeholder = self.composer_placeholder();
        if placeholder != self.composer_placeholder_shown {
            self.composer_placeholder_shown = placeholder.clone();
            self.composer_field
                .update(cx, |field, cx| field.set_placeholder(placeholder, cx));
        }
        let disabled = self.composer_disabled();
```

Replace the `composer-input` child with the strip and the field:

```rust
            .when(!self.attachments.is_empty(), |card| {
                let remove_entity = entity.clone();
                card.child(
                    div()
                        .id("attachment-strip")
                        .debug_selector(|| "attachment-strip".into())
                        .flex()
                        .flex_wrap()
                        .gap(px(6.0))
                        .px(px(4.0))
                        .children(self.attachments.iter().enumerate().map(|(index, _)| {
                            let remove_entity = remove_entity.clone();
                            div()
                                .id(("attachment-chip", index))
                                .debug_selector(move || format!("attachment-chip-{index}"))
                                .h(px(24.0))
                                .px(px(8.0))
                                .rounded(theme.radii.control)
                                .bg(theme.surface_raised)
                                .border_1()
                                .border_color(theme.border)
                                .flex()
                                .items_center()
                                .gap(px(6.0))
                                .text_size(typography.caption2)
                                .child(div().text_color(theme.text_faint).child("▣"))
                                .child(div().text_color(theme.text).child("Image"))
                                .child(
                                    div()
                                        .id(("attachment-remove", index))
                                        .debug_selector(move || format!("attachment-remove-{index}"))
                                        .px(px(2.0))
                                        .rounded(px(2.0))
                                        .text_color(theme.text_faint)
                                        .hover(|style| style.bg(theme.overlay))
                                        .on_click(move |_, window, cx| {
                                            remove_entity.update(cx, |chat, cx| {
                                                chat.remove_attachment(index, cx);
                                                chat.composer_field
                                                    .read(cx)
                                                    .focus_handle(cx)
                                                    .focus(window, cx);
                                            });
                                        })
                                        .child("×"),
                                )
                        })),
                )
            })
            .child(
                div()
                    .id("composer-input")
                    .debug_selector(|| "composer-input".into())
                    .w_full()
                    .when(disabled, |input| input.opacity(0.6))
                    .child(self.composer_field.clone()),
            )
```

(The glyph and the card's own look are replaced by bezel icons and the glass card in Task 5; this step only moves the field in.) The card's `on_mouse_down` becomes

```rust
                cx.listener(|this, _, window, cx| {
                    this.composer_field.read(cx).focus_handle(cx).focus(window, cx);
                }),
```

and drop `entity_for_focus`.

- [ ] **Step 6: Delete the old document model**

```bash
git rm rust/crates/sirio_ui/src/composer.rs
```

Remove `pub mod composer;` from `rust/crates/sirio_ui/src/lib.rs` and the `use crate::composer::{Composer, ComposerChip, ComposerPart};` line from `chat/mod.rs`. Remove `use crate::caret;` only if nothing else in `chat/mod.rs` uses it (the question-answer field and the model search still do until Task 7 — keep it).

- [ ] **Step 7: Make the crate compile**

Run: `cd rust && cargo build -p sirio_ui 2>&1 | grep -E '^(error|warning: unused)' | sort | uniq -c | head -40`
Fix every error by the rules above (every `self.composer.` read becomes `self.draft` / `self.attachments`; every removed handler's caller goes). Repeat until the build is clean. Do not touch tests yet — they fail to compile next.

- [ ] **Step 8: Rewrite the composer tests**

In `chat/mod.rs`'s `mod tests`:

1. `chat_view`: after `cx.update(Theme::init);` add `cx.update(bezel::ui::input::init);`. Do the same in every test that builds a `Chat` without `chat_view` (search `add_window_view(|_, cx|` and `Chat::from_test_command` in the tests; each needs the `input::init` line before it, after `Theme::init`).
2. Every `chat.composer.text()` → `chat.draft_text()`; every `chat.composer.draft().text` → `chat.draft_text()`; every `chat.composer.draft().images` → `chat.attachments`; `chat.composer.draft().mention_paths` → `assemble_prompt(&chat.draft, &chat.accepted_mentions).1`; `chat.composer.insert_text(s)` inside `chat.update(cx, |chat, cx| …)` → `chat.set_composer_text(s, cx)` (the offline test's closure gains `cx`: `chat.update(cx, |chat, cx| chat.set_composer_text("hello offline test", cx))`).
3. `slash_popup_filters_and_inserts_a_skill_token`: `chat.slash_selected` → `chat.slash_filter.active().unwrap_or(0)`; the expected drafts become `"/create-plan "` and `"/cr "`.
4. `at_mention_popup_lists_files_and_inserts_a_file_chip` → rename `…_inserts_a_mention_token`: after the click assert `chat.draft_text() == "@src/main.rs "` instead of `composer-chip-file`; after `simulate_input(" check")` assert `draft_text() == "@src/main.rs  check"` and `assemble_prompt(...)` gives `("check", ["src/main.rs"])`; the `Entry::User` sent must be `"check"` (was `" check"`).
5. `attach_control_accepts_one_image_and_rejects_the_rest` and `dropping_external_files_attaches_chips_and_rejects_the_oversized_one`: `composer-chip-image` → `attachment-chip-0`, `chip-remove-0` → `attachment-remove-0`, `draft().images` → `attachments`.
6. Delete: `the_caret_sits_flush_against_the_character_it_follows`, `the_caret_of_a_wrapped_draft_stays_on_the_draft`, `the_composer_paints_the_run_it_has_selected`, `the_composer_arms_no_repaint_timer_while_streaming` (bezel owns the caret now). Keep `a_long_draft_wraps_inside_the_composer_instead_of_overflowing_it` but assert on `composer-input` bounds: its right edge stays inside `composer` and its height grows after typing 200 characters.
7. `narrow_composer_placeholder_stays_inside_composer_card`: replace the `composer-placeholder` bounds assertion with `assert_eq!(chat.read_with(&cx.cx, |chat, _| chat.composer_placeholder()), chat.read_with(&cx.cx, |chat, _| chat.default_placeholder()))` and keep the card-width assertions on `composer-input`.
8. `permission_wait_disables_the_composer_and_shows_its_own_placeholder` and `offline_composer_shows_its_own_placeholder`: replace `debug_bounds("permission-wait-placeholder")` / `debug_bounds("offline-placeholder")` with `chat.read_with(&cx.cx, |chat, _| chat.composer_placeholder())` equal to `"Waiting for permission response…"` / `"Agent offline — reconnecting when you send…"`; keep the typing-is-refused assertions on `draft_text()`.
9. Add one new test for the key routing:

```rust
    /// `up`/`down` drive a picker while one is open and are the field's own
    /// vertical motion otherwise — the handlers propagate when no popup is
    /// on screen, so gpui reaches the TextField's binding next.
    #[gpui::test]
    async fn arrows_move_the_caret_when_no_popup_is_open(cx: &mut TestAppContext) {
        let (chat, cx) = chat_view(cx, &["plain"]);
        pump_chat_until(cx, &chat, |chat| chat.client.is_some());
        refresh_frame(cx);
        focus_and_type(cx, "one");
        cx.simulate_keystrokes("shift-enter");
        cx.simulate_input("two");
        cx.run_until_parked();
        assert_eq!(chat.read_with(&cx.cx, |chat, _| chat.draft_text()), "one\ntwo");
        let end = chat.read_with(&cx.cx, |chat, _| chat.draft_caret);
        cx.simulate_keystrokes("up");
        cx.run_until_parked();
        let moved = chat.read_with(&cx.cx, |chat, _| chat.draft_caret);
        assert!(moved < end, "up without a popup moves the caret to the first row ({moved} < {end})");
        assert!(chat.read_with(&cx.cx, |chat, _| chat.entries.is_empty()));
    }
```

- [ ] **Step 9: Run the chat tests**

Run: `cd rust && cargo test -p sirio_ui chat:: 2>&1 | tail -15`
Expected: all pass except tests already red on `main` (compare lists). Then `cargo clippy -p sirio_ui 2>&1 | grep -c warning` — no new warnings versus `main`.

- [ ] **Step 10: Commit**

```bash
git add -A rust/crates/sirio_ui/src
git commit -m "feat(chat): composer on bezel TextField with text tokens and an attachment strip"
```

---

### Task 5: The gallery card and control row

**Files:**
- Modify: `rust/crates/sirio_ui/src/chat/mod.rs` — the `composer_card` block and the `attach_button`, `overflow_button`, send control in `render_composer`; the `status_pill`, `model_control`, `effort_control` sizes are already 24 px.

**Interfaces:**
- Consumes: `bezel::theme::Theme::of(cx)` (`card_glass_bg`, `solid`, `on_solid`, `border`, `text_faint`, `element_hover`), `bezel::ui::icons::{icon, ARROW_UP, STOP, PAPERCLIP}`, `bezel::theme::ink`.
- Produces: selectors unchanged; `send` is now 24×24; `stop-glyph` selector kept on the STOP icon wrapper.

- [ ] **Step 1: Write the failing test**

```rust
    /// The card is the gallery's `Composer` card: a glass surface at
    /// `surface_radius`, the field on top, one row of controls under it, and
    /// a 24px send disc at the row's end — inert until there is something to
    /// send.
    #[gpui::test]
    async fn composer_card_carries_the_control_row_and_a_send_disc(cx: &mut TestAppContext) {
        let (chat, cx) = chat_view(cx, &["plain"]);
        pump_chat_until(cx, &chat, |chat| chat.client.is_some());
        refresh_frame(cx);
        let card = cx.debug_bounds("composer").expect("card");
        let input = cx.debug_bounds("composer-input").expect("field");
        let send = cx.debug_bounds("send").expect("send disc");
        let attach = cx.debug_bounds("attach-image").expect("attach");
        assert_eq!(send.size.width, px(24.0));
        assert_eq!(send.size.height, px(24.0));
        assert!(input.bottom() <= send.top(), "the control row sits under the field");
        assert!(attach.left() < send.left(), "attach is in the left cluster, send at the right end");
        assert!(send.right() <= card.right() && card.left() <= attach.left());
        assert!(cx.debug_bounds("send-ready").is_none(), "an empty draft leaves the disc inert");
        focus_and_type(cx, "go");
        refresh_frame(cx);
        assert!(cx.debug_bounds("send-ready").is_some(), "a draft arms the disc");
    }
```

- [ ] **Step 2: Run it**

Run: `cd rust && cargo test -p sirio_ui composer_card_carries 2>&1 | tail -5`
Expected: FAIL (`send` is 26 px; no `send-ready`).

- [ ] **Step 3: Rebuild the card**

In `render_composer`, add near the top:

```rust
        let bezel_theme = bezel::theme::Theme::of(cx).clone();
```

Replace the `composer_card` builder's styling and the control row. The card:

```rust
        let composer_card = div()
            .id("composer")
            .debug_selector(|| "composer".into())
            .relative()
            .w_full()
            .max_w(px(TRANSCRIPT_WIDTH))
            // `Card variant="input"`: the field, then a row of controls under
            // it, both on one frosted surface (gallery `Composer::render`).
            .rounded(px(bezel::theme::Theme::surface_radius()))
            .border_1()
            .border_color(if focused { theme.text } else { bezel_theme.border })
            .bg(bezel_theme.card_glass_bg())
            .px(px(4.0))
            .pt(px(4.0))
            .pb(px(6.0))
            .flex()
            .flex_col()
            .gap(px(4.0))
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(|this, _, window, cx| {
                    this.composer_field.read(cx).focus_handle(cx).focus(window, cx);
                }),
            )
```

The attach button's child becomes

```rust
            .hover(|style| style.bg(bezel_theme.element_hover))
            .child(
                bezel::ui::icons::icon(bezel::ui::icons::PAPERCLIP)
                    .size(px(14.0))
                    .text_color(bezel_theme.text_faint),
            )
```

The control row (replacing the `div().flex().items_center().gap(px(6.0)).child(attach_button)…` child) becomes:

```rust
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .px(px(6.0))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.0))
                            .min_w_0()
                            .child(status_pill)
                            .child(model_control)
                            .children(effort_control)
                            .child(attach_button),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_none()
                            .items_center()
                            .gap(px(6.0))
                            .child(context_cluster)
                            .child(overflow_button)
                            .child(send_disc),
                    ),
            )
```

where `context_cluster` is the existing `div().flex().flex_none().items_center().gap(px(6.0)).h(px(24.0))…child(context_ring).child(context-label).child(context-percent)` element pulled out into a local, and `send_disc` is:

```rust
        let ready = can_send;
        let send_disc = {
            let disc = div()
                .size(px(24.0))
                .rounded_full()
                .flex()
                .items_center()
                .justify_center();
            // Three looks, one `AnyElement`: the ready arm is `Stateful`
            // (it carries an id), the other two are plain `Div`s.
            let disc: AnyElement = if self.streaming {
                // D-CHAT-02: the same control is Stop while a turn runs.
                disc.bg(bezel_theme.solid)
                    .cursor_pointer()
                    .child(
                        div()
                            .id("stop-glyph")
                            .debug_selector(|| "stop-glyph".into())
                            .child(
                                bezel::ui::icons::icon(bezel::ui::icons::STOP)
                                    .size(px(12.0))
                                    .text_color(bezel_theme.on_solid),
                            ),
                    )
                    .into_any_element()
            } else if ready {
                disc.id("send-ready")
                    .debug_selector(|| "send-ready".into())
                    .bg(bezel_theme.solid)
                    .cursor_pointer()
                    .hover(|s| s.opacity(0.9))
                    .child(
                        bezel::ui::icons::icon(bezel::ui::icons::ARROW_UP)
                            .size(px(14.0))
                            .text_color(bezel_theme.on_solid),
                    )
                    .into_any_element()
            } else {
                // Present but not pressable: the shape keeps its place, the
                // glyph goes faint, no hover and no pointer (gallery
                // `send_button`, after `ui::pagination::step`).
                disc.bg(bezel::theme::ink(0.06))
                    .child(
                        bezel::ui::icons::icon(bezel::ui::icons::ARROW_UP)
                            .size(px(14.0))
                            .text_color(bezel_theme.text_faint),
                    )
                    .into_any_element()
            };
            div()
                .id("send")
                .debug_selector(|| "send".into())
                .flex_none()
                .when(self.streaming, |this| {
                    this.on_click(move |_, _, cx| {
                        stop_entity.update(cx, |chat, cx| chat.cancel_turn(cx));
                    })
                })
                .when(!self.streaming && ready, |this| {
                    this.on_click(move |_, _, cx| {
                        send_entity.update(cx, |chat, cx| chat.send(cx));
                    })
                })
                .child(disc)
        };
```

`stop_entity` and `send_entity` are the existing clones of `entity` made just above the old send control; keep those two lines.

- [ ] **Step 4: Run the composer tests**

Run: `cd rust && cargo test -p sirio_ui chat:: 2>&1 | tail -10`
Expected: the new test passes; `narrow_composer_stays_inside_chat_pane_and_keeps_send_reachable`, `wide_composer_remains_capped_at_transcript_maximum`, `the_spinner_does_not_move_or_resize_the_composer` still pass (adjust only literal pixel expectations that named the old 26 px disc or the old 10 px padding).

- [ ] **Step 5: Commit**

```bash
git add rust/crates/sirio_ui/src/chat/mod.rs
git commit -m "feat(chat): gallery composer card with a control row and send disc"
```

---

### Task 6: `/` and `@` pickers on bezel popover, anchored at the token

**Files:**
- Modify: `rust/crates/sirio_ui/src/chat/composer_view.rs` (add `fn menu_above_at`)
- Modify: `rust/crates/sirio_ui/src/chat/mod.rs` (`slash_popup`, `mention_popup` in `render_composer`)

**Interfaces:**
- Consumes: `popover::{popover_card, menu_row}`, `bezel::motion::{Fade, Painter, menu_in}`, `bezel::ui::surface::popover`, `TextField::offset_bounds`.
- Produces: `composer_view::menu_above_at(id, point, content) -> AnyElement` — an upward menu at a **window** point (bezel's `anchored_menu_above_at` takes a point relative to a positioned ancestor, and the caret position is a window measurement).

- [ ] **Step 1: Write the failing test**

```rust
    /// The pickers hang above the token that opened them, not above the
    /// card: as the field grows a row, the menu follows the caret.
    #[gpui::test]
    async fn slash_popup_hangs_above_the_slash_and_steps_with_the_arrows(cx: &mut TestAppContext) {
        let (chat, cx) = chat_view(cx, &["composer"]);
        pump_chat_until(cx, &chat, |chat| chat.client.is_some());
        refresh_frame(cx);
        focus_and_type(cx, "/");
        refresh_frame(cx);
        let popup = cx.debug_bounds("slash-popup").expect("popup");
        let input = cx.debug_bounds("composer-input").expect("field");
        assert!(popup.bottom() <= input.top() + px(4.0), "the menu opens upward from the token row");
        assert!(popup.left() >= input.left() - px(8.0), "and starts at the token's column");
        assert_eq!(chat.read_with(&cx.cx, |chat, _| chat.slash_filter.active()), Some(0));
        cx.simulate_keystrokes("down");
        cx.run_until_parked();
        assert_eq!(chat.read_with(&cx.cx, |chat, _| chat.slash_filter.active()), Some(1));
        cx.simulate_keystrokes("up");
        cx.run_until_parked();
        assert_eq!(chat.read_with(&cx.cx, |chat, _| chat.slash_filter.active()), Some(0));
    }
```

- [ ] **Step 2: Run it**

Run: `cd rust && cargo test -p sirio_ui slash_popup_hangs_above 2>&1 | tail -5`
Expected: FAIL on the column assertion (the old popup is at `left(16px)` of the card).

- [ ] **Step 3: The anchored helper**

Append to `composer_view.rs`:

```rust
/// An upward menu at a window point — `popover::anchored_menu_above` with an
/// explicit position, for a token/caret anchor that is measured in window
/// coordinates (`TextField::offset_bounds`). Same material
/// (`ui::surface::popover`), entrance motion, occlusion and window snapping
/// as bezel's own; bezel's `anchored_menu_above_at` takes an ancestor-relative
/// point instead, which a caret measurement is not.
pub(crate) fn menu_above_at(
    id: impl Into<SharedString>,
    position: Point<Pixels>,
    content: AnyElement,
) -> AnyElement {
    let content =
        bezel::ui::surface::popover(bezel::theme::Theme::surface_radius(), content);
    gpui::deferred(
        gpui::anchored()
            .position(position)
            .anchor(gpui::Anchor::BottomLeft)
            .snap_to_window_with_margin(px(8.0))
            .child(bezel::motion::menu_in(
                id.into(),
                div().occlude().pb(px(6.0)).child(content),
            )),
    )
    .priority(1)
    .into_any_element()
}
```

and merge the module's imports into one line: `use gpui::{AnyElement, Context, Pixels, Point, SharedString, div, prelude::*, px};` (`bezel::ui::surface::popover(radius, child) -> Surface` and `bezel::motion::menu_in(id: impl Into<ElementId>, element)` are both public in 0.1.4; `SharedString: Into<ElementId>`).

- [ ] **Step 4: Rebuild the two popups**

In `render_composer`, replace the `slash_popup` block with:

```rust
        let slash_popup = if self.slash_popup_visible() {
            let candidates = self.slash_candidates();
            let active = self.slash_filter.active();
            let view = bezel::motion::Painter::of(cx);
            let anchor = self
                .composer_field
                .read(cx)
                .offset_bounds(0, window)
                .map(|row| gpui::point(row.left(), row.top() - px(4.0)));
            anchor.map(|anchor| {
                let rows: Vec<AnyElement> = candidates
                    .into_iter()
                    .enumerate()
                    .map(|(position, command)| {
                        let name = command.name.clone();
                        let tooltip = slash_option_tooltip(&command.description);
                        let row_entity = entity.clone();
                        let accept_name = name.clone();
                        let name_for_id = name.clone();
                        let name_for_label_id = name.clone();
                        popover::menu_row(
                            &bezel_theme,
                            Some(position) == active,
                            bezel::motion::Fade::new(view, format!("slash-option-{name}")),
                        )
                        .id(SharedString::from(format!("slash-option-{name}")))
                        .debug_selector(move || format!("slash-option-{name_for_id}"))
                        .when_some(tooltip, |this, text| {
                            this.tooltip(move |window, cx| Tooltip::text(text.clone(), window, cx))
                        })
                        .on_click(move |_, _, cx| {
                            row_entity.update(cx, |chat, cx| {
                                chat.accept_slash_command(&accept_name, cx);
                            });
                        })
                        .child(
                            div()
                                .debug_selector(move || format!("slash-option-name-{name_for_label_id}"))
                                .child(format!("/{name}")),
                        )
                        .into_any_element()
                    })
                    .collect();
                div()
                    .id("slash-popup")
                    .debug_selector(|| "slash-popup".into())
                    .child(composer_view::menu_above_at(
                        "slash-popup-menu",
                        anchor,
                        popover::popover_card(&bezel_theme)
                            .w(px(280.0))
                            .child(div().flex().flex_col().children(rows))
                            .into_any_element(),
                    ))
            })
        } else {
            None
        };
```

and the `mention_popup` block with the same shape over `self.mention_filter` (anchor at `offset_bounds(at, window)` where `at` comes from `mention_token(&self.draft, self.draft_caret)`, rows `mention-option-<path>` calling `accept_mention`, id `mention-popup`, active row `self.mention_filter.active()`, no tooltip, the `▤` glyph replaced by `bezel::ui::icons::icon(bezel::ui::icons::DOCUMENT).size(px(12.0)).text_color(bezel_theme.text_faint)` before the path text). `offset_bounds` is `None` before the field's first paint; the popup simply waits a frame.

- [ ] **Step 5: Run the picker tests**

Run: `cd rust && cargo test -p sirio_ui popup 2>&1 | tail -10`
Expected: `slash_popup_hangs_above…`, `slash_popup_filters_and_inserts_a_skill_token`, `slash_popup_floats_above_the_composer_with_single_line_rows`, `at_mention_popup_lists_files_and_inserts_a_mention_token` pass. (`debug_bounds` of an element inside a `deferred` layer is recorded on paint; call `refresh_frame` before reading it.)

- [ ] **Step 6: Commit**

```bash
git add rust/crates/sirio_ui/src/chat
git commit -m "feat(chat): token pickers on bezel popover anchored at the caret"
```

---

### Task 7: Model, mode, context, overflow and history menus on bezel popover; model search on `TextField`

**Files:**
- Modify: `rust/crates/sirio_ui/src/chat/mod.rs` — `model_picker`, `mode_picker`, `context_popover`, `overflow_menu`, `chat_history_menu` blocks; `model_search` field and its users (`toggle_model_picker`, `model_query_matches` call, `on_composer_key` model branch, `flip_model_search_blink`, `model_search_blink`, `model_search_caret_visible`).

**Interfaces:**
- Produces: `model_search_field: Entity<TextField>` (`Shape::Line`, placeholder `"Search models…"`, key context `"ChatModelSearch"`); menus built with `popover::anchored_menu_above` (pill, model chip) and `popover::anchored_menu_above_end` (context ring, overflow, history).

- [ ] **Step 1: Write the failing test**

```rust
    /// The model picker's search is a real field: typing filters, Backspace
    /// edits, Escape closes — and none of it reaches the composer's draft.
    #[gpui::test]
    async fn model_search_is_a_text_field_that_never_touches_the_draft(cx: &mut TestAppContext) {
        let (chat, cx) = chat_view(cx, &["plain"]);
        pump_chat_until(cx, &chat, |chat| chat.client.is_some());
        chat.update(cx, |chat, _| {
            configure_test_chat(chat);
            chat.available_models.push(ModelOption {
                id: "sonnet".into(),
                name: "Sonnet".into(),
                description: None,
            });
        });
        refresh_frame(cx);
        focus_and_type(cx, "draft stays");
        let chip = cx.debug_bounds("model-chip").expect("model chip");
        cx.simulate_click(chip.center(), Modifiers::none());
        refresh_frame(cx);
        assert!(cx.debug_bounds("model-picker").is_some());
        cx.simulate_input("son");
        refresh_frame(cx);
        assert!(cx.debug_bounds("model-option-sonnet").is_some());
        assert!(cx.debug_bounds("model-option-opus").is_none(), "the search narrows the list");
        cx.simulate_keystrokes("backspace backspace backspace");
        refresh_frame(cx);
        assert!(cx.debug_bounds("model-option-opus").is_some());
        cx.simulate_keystrokes("escape");
        refresh_frame(cx);
        assert!(cx.debug_bounds("model-picker").is_none());
        assert_eq!(chat.read_with(&cx.cx, |chat, _| chat.draft_text()), "draft stays");
    }
```

- [ ] **Step 2: Run it**

Run: `cd rust && cargo test -p sirio_ui model_search_is_a_text_field 2>&1 | tail -5`
Expected: FAIL (typing after the click still goes through `on_composer_key`'s `model_search` string; `backspace` is no longer bound).

- [ ] **Step 3: The search field**

Replace `model_search: String`, `model_search_blink`, `model_search_caret_visible` with `model_search_field: Entity<TextField>`, built in `Chat::new`:

```rust
        let model_search_field = cx.new(|cx| {
            TextField::new(cx)
                .with_placeholder("Search models…")
                .with_key_context("ChatModelSearch")
        });
        cx.observe(&model_search_field, |_, _, cx| cx.notify()).detach();
```

`toggle_model_picker`: on open, `self.model_search_field.update(cx, |f, cx| f.clear(cx));` then focus it: `self.model_search_field.read(cx).focus_handle(cx).focus(window, cx);`. Delete `model_picker_focus`'s `track_focus` on the picker card (the field is the focus target) but keep the `key_context("ChatModelPicker")` element wrapping it so `escape` → `Cancel` still resolves: bind `escape` to `Cancel` for `"ChatModelSearch"` in `bind_keys` as well. Remove the model-picker branch from `on_composer_key` and from `send_action` (Enter in a `Shape::Line` field is unbound, so it can no longer reach `Send`). Read the query with `let query = self.model_search_field.read(cx).content().to_string();` where `self.model_search` was read; delete `flip_model_search_blink` and the `caret::` uses in the picker.

- [ ] **Step 4: The menus**

For each of the five menus, keep its rows and selectors, and replace the `absolute().right(..).bottom(..).p(..).rounded(..).bg(..).border_1()…shadow_lg()` shell with a bezel layer:

```rust
        // The pill and the model chip open upward, left-aligned; the ring,
        // overflow and history are right-side triggers and open leftward.
        let model_picker = self.model_picker_open.then(|| {
            popover::anchored_menu_above(
                "model-picker-menu",
                div()
                    .id("model-picker")
                    .debug_selector(|| "model-picker".into())
                    .key_context("ChatModelPicker")
                    .on_action(cx.listener(Self::cancel))
                    .w(px(245.0))
                    .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                        this.model_picker_open = false;
                        cx.notify();
                    }))
                    .child(popover::popover_card(&bezel_theme).child(/* the existing children: search field row, empty/no-match rows, model rows, effort section */))
                    .into_any_element(),
                None,
            )
        });
```

The search row becomes `div().id("model-search-input").debug_selector(|| "model-search-input".into()).w_full().mb(px(6.0)).child(self.model_search_field.clone())`. Model rows use `popover::menu_row_nav(&bezel_theme, is_selected, false, Fade::new(view, format!("model-option-{id}")))` keeping their ids; effort chips and the "Recommended" tag keep their current markup inside the row.

`mode_picker` → `anchored_menu_above("mode-picker-menu", …)`, rows `menu_row_nav(is_selected, false, …)`. `context_popover`, `overflow_menu`, `chat_history_menu` → `anchored_menu_above_end(...)`, overflow/history rows through `menu_row(&bezel_theme, false, Fade::new(view, id))`.

Because `anchored_menu_above` anchors at the trigger's top-left, each menu must be a child of the element that triggers it: move `.children(model_picker)` into the model chip's builder (the chip gets `.relative()`), `mode_picker` into the status pill, `context_popover` into the context cluster, `overflow_menu` and `chat_history_menu` into the overflow button — instead of the card's trailing `.children(...)` list, which is deleted.

- [ ] **Step 5: Run the picker tests**

Run: `cd rust && cargo test -p sirio_ui -- picker chip effort context overflow history 2>&1 | tail -12`
Expected: the new test and `every_effort_chip_stays_inside_the_picker_border`, `the_model_chips_chevron_stays_beside_the_model_name`, the overflow/history tests pass (update any literal `right(42px)`/`bottom(43px)` expectations to relative assertions: the menu's bottom is at or above the trigger's top, and its right edge is at or left of the trigger's right edge for the `_end` menus).

- [ ] **Step 6: Commit**

```bash
git add rust/crates/sirio_ui/src/chat
git commit -m "feat(chat): composer menus on bezel popover and a TextField model search"
```

---

### Task 8: Sweep, docs and the whole-crate gate

**Files:**
- Modify: `rust/crates/sirio_ui/src/chat/mod.rs`, `rust/crates/sirio_ui/src/chat/composer_view.rs`, `docs/superpowers/specs/2026-09-04-bezel-agent-parity-design.md` (status line for sub-project 1)

- [ ] **Step 1: Delete what nothing uses**

Run: `cd rust && cargo clippy -p sirio_ui --all-targets 2>&1 | grep -E 'warning: (unused|dead_code|never used)' -A3 | head -60`
Remove every item reported inside `chat/`: leftover imports (`Rc`, `Cell`, `StyledText`, `ElementId` if unused), `caret::Blink` fields no longer read, constants for the old card (`CARD_*` only if unused), `ComposerPaintTrace` remnants. `caret` stays for the question-answer field.

- [ ] **Step 2: Run the whole crate**

Run: `cd rust && cargo test -p sirio_ui 2>&1 | tail -20 && cargo clippy -p sirio_ui --all-targets 2>&1 | tail -3`
Expected: pass list equals `main`'s minus the deleted tests plus the new ones; clippy clean.

- [ ] **Step 3: Build the app and hand over for the visual review**

Run: `cd rust && cargo build -p sirio 2>&1 | tail -2`
Expected: `Finished`. Stop here: the orchestrator launches the binary in isolation (`SIRIO_DB=<scratch>/session.db SIRIO_SOCKET_ENABLE=0`) and compares the composer against `refs/gallery-composer.jpg`; fixes come back as ordinary follow-up tasks.

- [ ] **Step 4: Note the status in the spec and commit**

In the spec's "Delivery" section append one line: `Sub-project 1 landed on branch feat/composer-bezel-textfield (2026-09-0x).` Then:

```bash
git add rust/crates/sirio_ui docs/superpowers/specs/2026-09-04-bezel-agent-parity-design.md
git commit -m "chore(chat): sweep the composer cut-over and record sub-project 1"
```

---

## Self-review against the spec (done while writing)

- Field, focus, observe → Task 4 Steps 2–3. Keys → Task 4 Step 4 (`bind_keys`, `PopupPrevious/Next/Accept` with `propagate`). Tokens replace chips, send serialisation, draft persistence, queued item → Task 3 + Task 4 Step 4. Images strip → Task 4 Step 5. Card + control row + send disc + attach → Task 5. Command/file pickers on `Filter` + upward caret-anchored menu → Task 6 (`menu_above_at` replaces `anchored_menu_above_at`, whose point is ancestor-relative — recorded in the task). Model/mode/context/overflow menus + `TextField` search → Task 7. Deletions → Task 4 Step 6 and Task 8. Tests → each task; `input::init` in every test constructor → Task 4 Step 8.
- Out of scope, as the spec says: the question-answer field's caret.
- Names used across tasks: `composer_field`, `draft`, `draft_caret`, `accepted_mentions`, `attachments`, `slash_filter`, `mention_filter`, `set_composer_text`, `reread_composer`, `open_token_popup`, `accept_active_popup_row`, `remove_attachment`, `composer_placeholder`, `composer_disabled`, `menu_above_at`, `model_search_field` — consistent between Tasks 4–7.
