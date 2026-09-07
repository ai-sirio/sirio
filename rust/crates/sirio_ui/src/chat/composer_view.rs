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

/// The colour the composer's status dot carries: the connection state,
/// kept apart from the pill's label so a known permission mode can name
/// itself while a turn streams — the Swift reference's `statusDotColor`
/// beside `mode.displayName` (`ComposerControlBar.swift`, `modePill`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PillDot {
    /// Startup in flight, or a turn streaming.
    Busy,
    /// A live agent waiting on the user.
    Ready,
    /// No live agent and no startup in flight.
    Offline,
}

/// The status pill's dot and label. The label is the session's permission
/// mode whenever the agent has advertised one: the pill is the mode
/// selector, and its word must not flip to "working" every time a turn
/// streams, or the mode the user picked is unreadable exactly while it
/// matters. Only a chat with no mode catalog names its state instead. A
/// catalog whose current id matches no offered mode keeps the "Ask"
/// fallback it always had.
pub(crate) fn status_pill_content(
    connecting: bool,
    streaming: bool,
    connected: bool,
    mode_catalog: Option<&ModeCatalog>,
) -> (PillDot, String) {
    let dot = if connecting || streaming {
        PillDot::Busy
    } else if connected {
        PillDot::Ready
    } else {
        PillDot::Offline
    };
    let label = match mode_catalog {
        Some(catalog) => catalog
            .options
            .iter()
            .find(|mode| mode.id == catalog.current_id)
            .map(|mode| mode.name.clone())
            .unwrap_or_else(|| "Ask".to_string()),
        None if connecting => "connecting".to_string(),
        None if streaming => "working".to_string(),
        None if connected => "idle".to_string(),
        None => "offline".to_string(),
    };
    (dot, label)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sirio_acp::AgentMode;

    fn catalog(current: &str) -> ModeCatalog {
        ModeCatalog {
            current_id: current.into(),
            options: vec![
                AgentMode {
                    id: "default".into(),
                    name: "Manual".into(),
                    description: None,
                },
                AgentMode {
                    id: "acceptEdits".into(),
                    name: "Accept edits".into(),
                    description: None,
                },
            ],
            config_option_id: None,
        }
    }

    #[test]
    fn a_known_mode_names_the_pill_while_a_turn_streams() {
        let catalog = catalog("acceptEdits");
        assert_eq!(
            status_pill_content(false, true, true, Some(&catalog)),
            (PillDot::Busy, "Accept edits".to_string()),
            "streaming moves to the dot; the label stays the selected permission"
        );
        assert_eq!(
            status_pill_content(false, false, true, Some(&catalog)),
            (PillDot::Ready, "Accept edits".to_string())
        );
    }

    #[test]
    fn a_known_mode_names_the_pill_while_reconnecting() {
        let catalog = catalog("default");
        assert_eq!(
            status_pill_content(true, false, false, Some(&catalog)),
            (PillDot::Busy, "Manual".to_string())
        );
        assert_eq!(
            status_pill_content(false, false, false, Some(&catalog)),
            (PillDot::Offline, "Manual".to_string()),
            "an agent that went away keeps its last known mode readable; the dot says offline"
        );
    }

    #[test]
    fn without_a_catalog_the_pill_names_the_state() {
        assert_eq!(
            status_pill_content(true, false, false, None),
            (PillDot::Busy, "connecting".to_string())
        );
        assert_eq!(
            status_pill_content(false, true, true, None),
            (PillDot::Busy, "working".to_string())
        );
        assert_eq!(
            status_pill_content(false, false, true, None),
            (PillDot::Ready, "idle".to_string())
        );
        assert_eq!(
            status_pill_content(false, false, false, None),
            (PillDot::Offline, "offline".to_string())
        );
    }

    #[test]
    fn an_unlisted_current_mode_falls_back_to_ask() {
        let catalog = catalog("bypassPermissions");
        assert_eq!(
            status_pill_content(false, false, true, Some(&catalog)),
            (PillDot::Ready, "Ask".to_string())
        );
    }

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
        assert_eq!(
            mention_token("@a", 99),
            Some((0, "a")),
            "caret past the end clamps"
        );
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
        assert_eq!(
            text, "mail me@example.com",
            "an @ inside a word is not a token"
        );
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

use gpui::{AnyElement, Context, Pixels, Point, SharedString, div, prelude::*, px};
use sirio_acp::ModeCatalog;

use super::{Chat, PopupAccept, PopupNext, PopupPrevious};

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
    let content = bezel::ui::surface::popover(bezel::theme::Theme::surface_radius(), content);
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
