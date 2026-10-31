//! The composer: Ely's `TextInput` in the gallery's Composer card, with the
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

impl PillDot {
    pub(crate) fn color(self) -> gpui::Rgba {
        match self {
            PillDot::Busy => gpui::rgb(0xf5a623),
            PillDot::Ready => gpui::rgb(0x53c653),
            PillDot::Offline => gpui::rgb(0x8a8d99),
        }
    }
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

/// The value the native transport spells the reset with. It is not a rung on
/// a magnitude axis — on most models "default" resolves to High, well above
/// Low — so it is not one of the levels the list steps through; it rides
/// below them as a row of its own. An agent that offers no such choice simply
/// gets no reset row.
pub(crate) const EFFORT_RESET: &str = "default";

/// The value the native transport spells Ultracode with: the same string as
/// `sirio_claude::catalog::EFFORT_ULTRACODE`, which `sirio_ui` does not
/// depend on. It is Extra high plus orchestration, not one step further up
/// the scale, so the list gives it a note in place of a step.
pub(crate) const EFFORT_ULTRACODE: &str = "ultracode";

/// The effort choices the list draws, split by what each row is.
#[derive(Clone, Debug)]
pub(crate) struct EffortRows {
    /// The levels, in the order the agent offered them, without the reset
    /// and Ultracode. Each is a step on the scale.
    pub levels: Vec<EffortChoice>,
    pub ultracode: Option<EffortChoice>,
    pub reset: Option<EffortChoice>,
}

impl EffortRows {
    /// Every row, in the order the list draws them: the levels, Ultracode,
    /// then the reset below its separator.
    pub(crate) fn in_order(&self) -> impl Iterator<Item = &EffortChoice> {
        self.levels.iter().chain(&self.ultracode).chain(&self.reset)
    }
}

/// Splits the session's choices into the list's rows.
pub(crate) fn effort_rows(choices: &[EffortChoice]) -> EffortRows {
    let find = |value: &str| choices.iter().find(|choice| choice.value == value).cloned();
    EffortRows {
        levels: choices
            .iter()
            .filter(|choice| choice.value != EFFORT_RESET && choice.value != EFFORT_ULTRACODE)
            .cloned()
            .collect(),
        ultracode: find(EFFORT_ULTRACODE),
        reset: find(EFFORT_RESET),
    }
}

/// The level every chat starts on, when the agent offers it.
pub(crate) const EFFORT_HIGH: &str = "high";

/// The level Sirio should ask the agent for, or `None` when the session is
/// where it should be.
///
/// A level the user chose wins when the agent offers it, and is asked for
/// only if it is not the current one. Without one (or when this model does
/// not offer it) every chat starts on High, when High is offered. An agent
/// that offers no High keeps whatever it reports.
pub(crate) fn effort_target(
    current: Option<&str>,
    choices: &[EffortChoice],
    user_choice: Option<&str>,
) -> Option<String> {
    let offered = |value: &str| choices.iter().any(|choice| choice.value == value);
    let wanted = match user_choice {
        Some(value) if offered(value) => value,
        _ if offered(EFFORT_HIGH) => EFFORT_HIGH,
        _ => return None,
    };
    (current != Some(wanted)).then(|| wanted.to_string())
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
        let catalog = catalog("someFutureMode");
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

    /// What Sirio asks a session's agent for, case by case. `L` is Claude's
    /// own ladder, `T` the three levels an ACP agent offers, `X` and `M`
    /// ladders with no `high` in them.
    #[test]
    fn effort_target_covers_every_case() {
        fn ladder(values: &[&str]) -> Vec<EffortChoice> {
            values
                .iter()
                .map(|value| EffortChoice {
                    value: (*value).to_string(),
                    name: (*value).to_string(),
                })
                .collect()
        }
        let l = ladder(&[
            "default",
            "low",
            "medium",
            "high",
            "xhigh",
            "max",
            "ultracode",
        ]);
        let t = ladder(&["low", "medium", "high"]);
        let x = ladder(&["xhigh"]);
        let m = ladder(&["low", "max"]);

        // (current, choices, user's choice, expected)
        let cases = [
            // a fresh session
            (Some("default"), &l, None, Some("high")),
            // already there
            (Some("high"), &l, None, None),
            // an ACP agent's own level
            (Some("medium"), &t, None, Some("high")),
            // no high offered
            (Some("xhigh"), &x, None, None),
            (Some("low"), &m, None, None),
            // the user's level, offered
            (Some("high"), &l, Some("low"), Some("low")),
            // the user's level, already current
            (Some("low"), &l, Some("low"), None),
            // the user's level, not offered
            (Some("default"), &t, Some("xhigh"), Some("high")),
            // the user chose Default
            (Some("high"), &l, Some("default"), Some("default")),
            // the agent reported no current
            (None, &t, None, Some("high")),
        ];
        for (current, choices, user_choice, expected) in cases {
            let offered: Vec<&str> = choices.iter().map(|c| c.value.as_str()).collect();
            assert_eq!(
                effort_target(current, choices, user_choice).as_deref(),
                expected,
                "current {current:?}, offered {offered:?}, user choice {user_choice:?}"
            );
        }
    }
}

use gpui::{AnyElement, Context, SharedString, canvas, div, px};
use sirio_acp::{EffortChoice, ModeCatalog};

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
        // The field holds line endings as `\n`; the cache must read what it will.
        let text: SharedString = crate::ely_ui::normalize(&text, true).into();
        self.draft = text.clone();
        self.draft_caret = text.len();
        self.composer_field
            .update(cx, |field, cx| field.reset_text(text.to_string(), cx));
        self.refresh_token_popups(cx);
    }

    /// The observer: runs after every content or caret change in the field.
    /// A disabled composer refuses edits by putting the last accepted draft
    /// back (`offline_enter_never_discards_the_typed_draft`).
    pub(crate) fn reread_composer(&mut self, cx: &mut Context<Self>) {
        // The blink and every caret move notify, so most calls end at this
        // comparison, with nothing read out of the field.
        let (content, caret) = {
            let field = self.composer_field.read(cx);
            if field.text() == self.draft.as_ref() && field.cursor() == self.draft_caret {
                return;
            }
            (SharedString::from(field.text().to_owned()), field.cursor())
        };
        if self.composer_disabled() && content != self.draft {
            let last = self.draft.to_string();
            self.composer_field
                .update(cx, |field, cx| field.reset_text(last, cx));
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

use super::composer_row::{BAND_COLUMN, BAND_TOP_PAD, RowLayout, row_layout, row_width};
use super::*;
use crate::popup::{hang_above_at, menu_card, menu_row};
use ely_gpui_component::{
    buttons::{Button, ButtonVariant, IconButton},
    chat::{Attachment, AttachmentChip, PromptInput},
    menus::Hang,
    primitives::{Icon, IconName},
    theme::{ActiveTheme as _, ControlSize, IconSize, Radius, TextSize},
    typography::EllipsisTooltip,
};

/// The room above and below the field's rows inside `composer-input`: what the
/// card's field always had between its edge and its text.
const FIELD_PAD_Y: f32 = 5.0;

/// The distance from the band's top edge to the field's first row: its 1px
/// rule and its top padding. The completion popups hang above the token that
/// opened them, and this keeps one opened on the first row above the band
/// instead of over its rule.
const SHELL_TOP_INSET: f32 = 1.0 + BAND_TOP_PAD;

/// What the chip calls an attached picture: the kind of file it will be sent as.
fn attachment_chip_name(mime_type: &str) -> String {
    match mime_type {
        "image/png" => "Image.png".to_string(),
        "image/jpeg" => "Image.jpg".to_string(),
        "image/gif" => "Image.gif".to_string(),
        "image/webp" => "Image.webp".to_string(),
        _ => "Image".to_string(),
    }
}

/// The size of the picture a base64 payload carries, without decoding it.
fn base64_decoded_len(data: &str) -> u64 {
    let padding = data.bytes().rev().take_while(|byte| *byte == b'=').count();
    (data.len() as u64 / 4 * 3).saturating_sub(padding as u64)
}

impl Chat {
    /// The queue block above the composer: a header with the count, a fold
    /// and "Clear all", then one row per queued prompt in send order — its
    /// text, "Send now" and a remove control. The front entry is the one the
    /// running turn's end sends; its dot is the only bright one. The list caps
    /// its height and scrolls, so a long queue never pushes the card off the
    /// pane. Callers draw it only while the queue is non-empty.
    pub(super) fn render_queue(&self, _theme: &Theme, cx: &Context<Self>) -> AnyElement {
        let ely = cx.theme();
        let (border, fg, fg_muted, fg_subtle, surface) = {
            let colors = &ely.colors;
            (
                colors.border,
                colors.fg,
                colors.fg_muted,
                colors.fg_subtle,
                colors.surface,
            )
        };
        let (radius, label_size) = (ely.radius(Radius::Md), ely.text_size(TextSize::Xs));
        let entity = cx.entity();
        let count = self.queue.len();
        let expanded = self.queue_expanded;
        let title = if count == 1 {
            "1 message queued".to_string()
        } else {
            format!("{count} messages queued")
        };
        let toggle_entity = entity.clone();
        let clear_entity = entity.clone();
        div()
            .id("queue")
            .debug_selector(|| "queue".into())
            .w_full()
            .max_w(px(TRANSCRIPT_WIDTH))
            .mb(px(8.0))
            .rounded(radius)
            .border_1()
            .border_color(border)
            .bg(surface)
            .overflow_hidden()
            .flex()
            .flex_col()
            .child(
                div()
                    .id("queue-header")
                    .flex()
                    .items_center()
                    .gap_1()
                    .pl_2()
                    .pr_1()
                    .py_0p5()
                    .child(
                        div()
                            .id("queue-toggle")
                            .debug_selector(|| "queue-toggle".into())
                            .flex()
                            .flex_1()
                            .items_center()
                            .gap_1p5()
                            .py_1()
                            .cursor_pointer()
                            .on_click(move |_, _, cx| {
                                toggle_entity.update(cx, |chat, cx| chat.toggle_queue_folded(cx));
                            })
                            .child(
                                Icon::new(if expanded {
                                    IconName::ChevronDown
                                } else {
                                    IconName::ChevronRight
                                })
                                .size(IconSize::Xs)
                                .color(fg_subtle),
                            )
                            .child(
                                div()
                                    .id("queue-count")
                                    .debug_selector(move || format!("queue-count-{count}"))
                                    .text_size(label_size)
                                    .text_color(fg_muted)
                                    .child(title),
                            ),
                    )
                    .child(
                        div()
                            .flex_none()
                            .debug_selector(|| "queue-clear".into())
                            .child(
                                Button::new("queue-clear-all", "Clear all")
                                    .variant(ButtonVariant::Ghost)
                                    .size(ControlSize::Sm)
                                    .on_click(move |_, _, cx| {
                                        clear_entity.update(cx, |chat, cx| chat.clear_queue(cx));
                                    }),
                            ),
                    ),
            )
            .when(expanded, |block| {
                block.child(
                    div()
                        .id("queue-entries")
                        .flex()
                        .flex_col()
                        .max_h(px(QUEUE_MAX_HEIGHT))
                        .overflow_y_scroll()
                        .border_t_1()
                        .border_color(border)
                        .children(self.queue.iter().enumerate().map(|(index, text)| {
                            let send_entity = entity.clone();
                            let remove_entity = entity.clone();
                            let text_for_id = text.clone();
                            let is_next = index == 0;
                            div()
                                .id(("queue-entry", index))
                                .debug_selector(move || format!("queue-entry-{index}"))
                                .flex()
                                .items_center()
                                .gap_2()
                                .px_2p5()
                                .py_1()
                                .when(index + 1 < count, |row| {
                                    row.border_b_1().border_color(border)
                                })
                                .child(
                                    div()
                                        .flex_none()
                                        .size(px(6.0))
                                        .rounded_full()
                                        .bg(if is_next { fg } else { fg_subtle }),
                                )
                                .child(
                                    div()
                                        .id(("queue-text", index))
                                        .debug_selector(move || format!("queue-text-{text_for_id}"))
                                        .flex_1()
                                        .min_w_0()
                                        .text_size(label_size)
                                        .text_color(fg)
                                        .child(EllipsisTooltip::new(
                                            ("queue-tooltip", index),
                                            text.clone(),
                                        )),
                                )
                                .child(
                                    div()
                                        .flex_none()
                                        .debug_selector(move || format!("queue-send-{index}"))
                                        .child(
                                            Button::new(("queue-send-button", index), "Send now")
                                                .variant(ButtonVariant::Ghost)
                                                .size(ControlSize::Sm)
                                                .on_click(move |_, _, cx| {
                                                    send_entity.update(cx, |chat, cx| {
                                                        chat.send_queued_entry_now(index, cx);
                                                    });
                                                }),
                                        ),
                                )
                                .child(
                                    div()
                                        .flex_none()
                                        .debug_selector(move || format!("queue-remove-{index}"))
                                        .child(
                                            IconButton::new(
                                                ("queue-remove-button", index),
                                                IconName::X,
                                            )
                                            .variant(ButtonVariant::Ghost)
                                            .size(ControlSize::Sm)
                                            .tooltip("Remove")
                                            .on_click(
                                                move |_, _, cx| {
                                                    remove_entity.update(cx, |chat, cx| {
                                                        chat.remove_queued_entry(index, cx);
                                                    });
                                                },
                                            ),
                                        ),
                                )
                        })),
                )
            })
            .into_any_element()
    }

    pub(super) fn render_composer(
        &mut self,
        theme: &Theme,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let _perf = sirio_perf::span("Chat.render_composer", cx.entity_id().as_u64());
        let typography = theme.typography;
        let bezel_theme = bezel_theme::Theme::of(cx).clone();
        let placeholder = self.composer_placeholder();
        if placeholder != self.composer_placeholder_shown {
            self.composer_placeholder_shown = placeholder.clone();
            self.composer_field
                .update(cx, |field, cx| field.set_placeholder(placeholder, cx));
        }
        let disabled = self.composer_disabled();
        let can_send = self.can_send();
        let entity = cx.entity();

        let send_entity = entity.clone();
        let stop_entity = entity.clone();
        let attach_entity = entity.clone();
        let panel_entity = entity.clone();
        let overflow_entity = entity.clone();

        // Slash-command popup (F-CHAT-09): a filtered list over the input,
        // opened by the leading `/token`, closed the moment the token is no
        // longer a single unbroken prefix. Keyboard selection comes from the
        // composer key path (up/down/tab, enter accepts via `Send`); click
        // accepts directly.
        //
        // Anchored to the composer card's top edge (`bottom: 100%`), not a
        // fixed distance up from its bottom: the card is taller than that
        // distance, so the list used to sit *inside* it — over the input
        // rows, in the card's own `surface` fill, where it read as a
        // transparent veil rather than a menu. The same token paints both
        // on purpose (they are the same step above the page); what makes
        // this a card of its own is that it floats over the page, with the
        // gap below it.
        let slash_popup = if self.slash_popup_visible() {
            let candidates = self.slash_candidates();
            let active = self.slash_filter.active();
            let anchor = self
                .composer_field
                .read(cx)
                .bounds_for(0)
                .map(|row| gpui::point(row.left(), row.top() - px(8.0 + SHELL_TOP_INSET)));
            anchor.map(|anchor| {
                let rows: Vec<AnyElement> = candidates
                    .into_iter()
                    .enumerate()
                    .map(|(position, command)| {
                        let name = command.name.clone();
                        let tooltip = slash_option_tooltip(&command.description);
                        let hint = command.argument_hint.clone();
                        let row_entity = entity.clone();
                        let accept_name = name.clone();
                        let name_for_id = name.clone();
                        let name_for_label_id = name.clone();
                        let name_for_hint_id = name.clone();
                        // One line per row: the name. The description is
                        // the row's tooltip, so ten rows stay ten lines
                        // and the list does not fill the pane.
                        menu_row(
                            SharedString::from(format!("slash-option-{name}")),
                            Some(position) == active,
                            false,
                            cx,
                        )
                        .debug_selector(move || format!("slash-option-{name_for_id}"))
                        .when_some(tooltip, |this, text| {
                            this.tooltip(crate::controls::tooltip(text))
                        })
                        .on_click(move |_, _, cx| {
                            row_entity.update(cx, |chat, cx| {
                                chat.accept_slash_command(&accept_name, cx);
                            });
                        })
                        .child(
                            div()
                                .debug_selector(move || {
                                    format!("slash-option-name-{name_for_label_id}")
                                })
                                .flex_none()
                                .text_size(typography.footnote)
                                .child(format!("/{name}")),
                        )
                        // The hint rides beside the name rather than under
                        // it: the row is one line by design (the
                        // description is the tooltip), and a second line
                        // per row would fill the pane. A command that
                        // takes no arguments draws nothing — the CLI says
                        // so with an empty `argumentHint`, and three in
                        // five of its commands do.
                        .when_some(hint, |row, hint| {
                            row.child(
                                div()
                                    .debug_selector(move || {
                                        format!("slash-option-hint-{name_for_hint_id}")
                                    })
                                    .min_w_0()
                                    .truncate()
                                    // The name's own size, not a smaller
                                    // one: a second size on a 12px row is
                                    // noise, colour already says which of
                                    // the two is secondary, and a taller
                                    // line box here would grow the row.
                                    .text_size(typography.footnote)
                                    .text_color(theme.ely.fg_subtle)
                                    .child(hint),
                            )
                        })
                        .into_any_element()
                    })
                    .collect();
                div()
                    .child(hang_above_at(
                        "slash-popup-menu",
                        anchor,
                        menu_card("slash-popup-card", cx)
                            .debug_selector(|| "slash-popup".into())
                            .w(px(280.0))
                            .child(div().flex().flex_col().children(rows)),
                        None,
                        cx,
                    ))
                    .into_any_element()
            })
        } else {
            None
        };

        // @ file-mention popup (F-CHAT-10): the bounded filesystem walk's
        // results for the trailing `@token`, anchored above the token. Hidden
        // when the token matches nothing.
        let mention_popup = if mention_token(&self.draft, self.draft_caret).is_some()
            && !self.mention_candidates.is_empty()
        {
            let (at, _) = mention_token(&self.draft, self.draft_caret).expect("token");
            let candidates = self.mention_candidates.clone();
            let active = self.mention_filter.active();
            let anchor = self
                .composer_field
                .read(cx)
                .bounds_for(at)
                .map(|row| gpui::point(row.left(), row.top() - px(8.0 + SHELL_TOP_INSET)));
            anchor.map(|anchor| {
                let rows: Vec<AnyElement> = candidates
                    .into_iter()
                    .enumerate()
                    .map(|(position, path)| {
                        let row_entity = entity.clone();
                        let path_for_id = path.clone();
                        let path_for_accept = path.clone();
                        menu_row(
                            SharedString::from(format!("mention-option-{path_for_id}")),
                            Some(position) == active,
                            false,
                            cx,
                        )
                        .debug_selector(move || format!("mention-option-{path_for_id}"))
                        // Pin the row to the card's inner width instead of
                        // trusting cross-axis stretch, so the path below has
                        // a definite box to ellipsize inside.
                        .w_full()
                        .min_w_0()
                        .on_click(move |_, _, cx| {
                            row_entity.update(cx, |chat, cx| {
                                chat.accept_mention(&path_for_accept, cx);
                            });
                        })
                        .child(
                            Icon::new(IconName::FileText)
                                .size(IconSize::Xs)
                                .color(theme.ely.fg_subtle),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .text_ellipsis()
                                .text_size(typography.footnote)
                                .child(path),
                        )
                        .into_any_element()
                    })
                    .collect();
                div()
                    .child(hang_above_at(
                        "mention-popup-menu",
                        anchor,
                        menu_card("mention-popup-card", cx)
                            .debug_selector(|| "mention-popup-card".into())
                            .w(px(360.0))
                            .child(div().flex().flex_col().children(rows)),
                        None,
                        cx,
                    ))
                    .into_any_element()
            })
        } else {
            None
        };

        let chat_history_menu = self.history_open.then(|| self.render_history(theme, cx));

        let attach_button = div()
            .flex_none()
            .debug_selector(|| "attach-image".into())
            .child(
                IconButton::new("attach-image-button", IconName::Plus)
                    .variant(ButtonVariant::Ghost)
                    .size(ControlSize::Sm)
                    .tooltip("Attach image")
                    .on_click(move |_, window, cx| {
                        attach_entity.update(cx, |chat, cx| chat.attach_image(window, cx));
                    }),
            );

        // Opens and closes the session panel's column; drawn "on" while open.
        let panel_open = self.session_panel_open();
        let panel_button = div()
            .flex_none()
            .debug_selector(|| "session-panel-toggle".into())
            .rounded(theme.radii.control)
            .when(panel_open, |button| button.bg(theme.ely.active))
            .child(
                IconButton::new("session-panel-button", IconName::PanelRight)
                    .variant(ButtonVariant::Ghost)
                    .size(ControlSize::Sm)
                    .tooltip("Session panel")
                    .on_click(move |_, _, cx| {
                        panel_entity.update(cx, |chat, cx| chat.toggle_session_panel(cx));
                    }),
            );

        let overflow_frame = self.composer_row.clone();
        let overflow_button = div()
            .relative()
            .flex_none()
            .debug_selector(|| "composer-overflow".into())
            // Measures the trigger; paints nothing.
            .child(
                canvas(
                    move |bounds, _, _| overflow_frame.overflow_button.set(Some(bounds)),
                    |_, _, _, _| {},
                )
                .absolute()
                .inset_0(),
            )
            .child(
                IconButton::new("composer-overflow-button", IconName::Ellipsis)
                    .variant(ButtonVariant::Ghost)
                    .size(ControlSize::Sm)
                    .tooltip("More")
                    .on_click(move |_, window, cx| {
                        overflow_entity.update(cx, |chat, cx| chat.toggle_overflow(window, cx));
                    }),
            );

        // With no agent configured the tool row holds only the send control.
        let has_agent = self.agent_launch.is_some();

        // Which layout the row takes follows the chat's width (the probe's),
        // never what the row itself measures, so the choice cannot move the
        // width it is made from. Before the first paint nothing is known and
        // the row is as wide as it can be.
        let layout = if has_agent {
            let docked = matches!(
                self.session_panel_placement(),
                Some(session_panel::Placement::Docked)
            );
            let chat_width = self.chat_width().map_or(f32::INFINITY, f32::from);
            let needed = self.full_row_needed(theme, window);
            let compact_needed = self.compact_row_needed(theme, window);
            let layout = row_layout(
                row_width(chat_width, docked),
                needed,
                compact_needed,
                self.composer_row.layout.get(),
            );
            self.composer_row.compact_needed.set(compact_needed);
            self.composer_row.layout.set(layout);
            self.composer_row.needed.set(needed);
            self.composer_row.docked.set(docked);
            layout
        } else {
            self.composer_row.layout.set(RowLayout::Full);
            // Nothing to lay out: the probe has nothing to ask about.
            self.composer_row.needed.set(0.0);
            self.composer_row.compact_needed.set(0.0);
            self.composer_row.docked.set(false);
            RowLayout::Full
        };

        // Overflow menu (F-CHAT-14) and the Chat History popover
        // (F-CHAT-34/35): the secondary composer actions the control row does
        // not carry inline. Built after the layout is known, because a Compact
        // row hands Fast, Thinking and the panel toggle to this menu.
        let overflow_menu = self.overflow_open.then(|| self.render_chat_menu(theme, cx));
        if overflow_menu.is_none() {
            *self.composer_row.menu.borrow_mut() = None;
        }
        // The thinking picker hangs from ⋯ in a Compact row, which has no chip
        // for it (the Full row's chip hangs its own). ⋯ is in the left group, so
        // the picker grows rightward into the chat.
        let compact_thinking_picker = (layout != RowLayout::Full)
            .then(|| self.render_thinking_picker(theme, Hang::Above, cx))
            .flatten();
        // A Narrow row has no ring for the context popover to hang from either,
        // so it hangs from ⋯ the same way.
        let narrow_context_popover = (layout == RowLayout::Narrow)
            .then(|| self.render_context_popover(theme, Hang::Above, cx))
            .flatten();

        let composer_context_menu = self
            .composer_context_menu
            .get()
            .map(|_| self.render_composer_context_menu(&bezel_theme, cx));

        // Pictures waiting to be sent, as Ely attachment chips above the
        // editor. The agent gets the base64 payload; the chip only needs a
        // name and a size, both read off what is attached.
        let attachment_chips = (!self.attachments.is_empty()).then(|| {
            div()
                .id("attachment-strip")
                .debug_selector(|| "attachment-strip".into())
                .flex()
                .flex_wrap()
                .gap_1p5()
                .children(self.attachments.iter().enumerate().map(|(index, image)| {
                    let remove_entity = entity.clone();
                    div()
                        .id(("attachment-chip", index))
                        .debug_selector(move || format!("attachment-chip-{index}"))
                        .min_w_0()
                        .child(
                            AttachmentChip::new(
                                ("attachment", index),
                                Attachment {
                                    key: format!("image-{index}").into(),
                                    name: attachment_chip_name(&image.mime_type).into(),
                                    bytes: base64_decoded_len(&image.base64_data),
                                    preview: None,
                                    progress: None,
                                },
                            )
                            .remove_selector(format!("attachment-remove-{index}"))
                            .on_remove(move |window, cx| {
                                remove_entity.update(cx, |chat, cx| {
                                    chat.remove_attachment(index, cx);
                                    chat.composer_field
                                        .read(cx)
                                        .focus_handle(cx)
                                        .focus(window, cx);
                                });
                            }),
                        )
                }))
        });

        // The field is Ely's raw `TextInput`: it paints text, selection and
        // caret and no frame or fill, so the shell's themed surface shows
        // through (`PromptInput::custom`) and the shell is the one frame the
        // composer has. The text lands on the content edge, lined up with the
        // chips and the tool row, in the 13px/18px the field has always had.
        let input_box = div()
            .id("composer-input")
            .debug_selector(|| "composer-input".into())
            .relative()
            .w_full()
            .py(px(FIELD_PAD_Y))
            .text_size(px(13.0))
            .line_height(px(18.0))
            .when(disabled, |input| input.opacity(0.6))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(|this, event: &MouseDownEvent, window, cx| {
                    cx.stop_propagation();
                    this.open_composer_context_menu(event.position, window, cx);
                }),
            )
            .child(self.composer_field.clone())
            .children(slash_popup)
            .children(mention_popup)
            .children(composer_context_menu);
        let editor = div()
            .w_full()
            .flex()
            .flex_col()
            .gap_1()
            .child(input_box)
            .when_some(self.attach_error.clone(), |column, message| {
                column.child(
                    div()
                        .id("attach-error")
                        .debug_selector(|| "attach-error".into())
                        .text_size(typography.caption2)
                        .text_color(theme.ely.danger)
                        .child(message),
                )
            });

        let row_frame = self.composer_row.clone();
        let send_frame = self.composer_row.clone();
        let streaming = self.streaming;
        let mut prompt = PromptInput::custom("composer-shell", editor, can_send, move |_, cx| {
            send_entity.update(cx, |chat, cx| chat.send(cx));
        })
        .band(px(BAND_COLUMN))
        .send_probe(move |bounds| send_frame.send.set(Some(bounds)));
        if streaming {
            // While a turn runs the send control is stop: the same path Escape
            // takes, and it leaves the draft alone.
            prompt = prompt.busy(move |_, cx| {
                stop_entity.update(cx, |chat, cx| chat.cancel_turn(cx));
            });
        }
        if let Some(chips) = attachment_chips {
            prompt = prompt.above(chips);
        }
        if has_agent {
            prompt = prompt.tool(attach_button);
            // Compact and Narrow move the panel's toggle into ⋯.
            if layout == RowLayout::Full {
                prompt = prompt.tool(panel_button);
            }
            prompt = prompt
                .tool(
                    div()
                        .relative()
                        .child(overflow_button)
                        .children(overflow_menu)
                        .children(compact_thinking_picker)
                        .children(narrow_context_popover)
                        .children(chat_history_menu),
                )
                .trailing(self.render_agent_controls(theme, layout, window, cx));
        }

        div()
            .w_full()
            .flex()
            .flex_col()
            .child(
                div()
                    .id("composer")
                    .debug_selector(|| "composer".into())
                    .relative()
                    .w_full()
                    .on_mouse_down(
                        gpui::MouseButton::Left,
                        cx.listener(|this, _, window, cx| {
                            this.composer_field
                                .read(cx)
                                .focus_handle(cx)
                                .focus(window, cx);
                        }),
                    )
                    .child(prompt)
                    // Measures the band; paints nothing.
                    .child(
                        canvas(
                            move |bounds, _, _| row_frame.band.set(Some(bounds)),
                            |_, _, _, _| {},
                        )
                        .absolute()
                        .inset_0(),
                    ),
            )
            .into_any_element()
    }
}
