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

/// The value the native transport spells the reset with. It is not a rung on
/// a magnitude axis — on most models "default" resolves to High, well above
/// Low — so it never becomes a stop on the track; it rides below it as its
/// own action. An agent that offers no such choice simply gets no reset row.
pub(crate) const EFFORT_RESET: &str = "default";

/// The slider's stops: the levels the session offers, in the order it
/// offered them, with the reset left out.
pub(crate) fn effort_stops(choices: &[EffortChoice]) -> Vec<&EffortChoice> {
    choices
        .iter()
        .filter(|choice| choice.value != EFFORT_RESET)
        .collect()
}

/// Where stop `index` sits on the track. The first stop is the left edge and
/// the last the right one, so a two-stop slider is a switch between its ends
/// rather than a knob that never reaches them.
pub(crate) fn effort_fraction_for_stop(stops: usize, index: usize) -> f32 {
    if stops < 2 {
        return 0.0;
    }
    (index.min(stops - 1) as f32) / ((stops - 1) as f32)
}

/// Which stop a point on the track belongs to: the nearest one. A drag that
/// comes to rest between two rungs picks the one it is closest to, rather
/// than the last one it passed — the levels are discrete, so every pixel of
/// the track has to answer with one of them.
pub(crate) fn effort_stop_for_fraction(stops: usize, fraction: f32) -> usize {
    if stops < 2 {
        return 0;
    }
    let steps = (stops - 1) as f32;
    let scaled = (fraction.clamp(0.0, 1.0) * steps).round();
    (scaled as usize).min(stops - 1)
}

/// How wide stop `index`'s share of the track is, as a fraction of the whole.
///
/// Not `1/stops`: a stop owns the pixels nearer to it than to its
/// neighbours, and the two ends have a neighbour on one side only, so they
/// own half as much track as the stops between them. Equal shares would make
/// the ends twice as easy to hit as they should be and put every boundary
/// half a step off.
pub(crate) fn effort_stop_share(stops: usize, index: usize) -> f32 {
    if stops < 2 {
        return 1.0;
    }
    let step = 1.0 / ((stops - 1) as f32);
    if index == 0 || index == stops - 1 {
        step / 2.0
    } else {
        step
    }
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

    fn choices(values: &[&str]) -> Vec<EffortChoice> {
        values
            .iter()
            .map(|value| EffortChoice {
                value: (*value).into(),
                name: (*value).into(),
            })
            .collect()
    }

    #[test]
    fn the_reset_is_not_a_stop_on_the_track() {
        let offered = choices(&[
            "default",
            "low",
            "medium",
            "high",
            "xhigh",
            "max",
            "ultracode",
        ]);
        let stops = effort_stops(&offered);
        assert_eq!(
            stops
                .iter()
                .map(|choice| choice.value.as_str())
                .collect::<Vec<_>>(),
            ["low", "medium", "high", "xhigh", "max", "ultracode"]
        );
    }

    #[test]
    fn an_agent_that_offers_no_reset_keeps_every_choice_as_a_stop() {
        // Only Claude's own transport spells a reset; an ACP agent may offer
        // nothing but levels, and none of them is silently eaten.
        let offered = choices(&["low", "high"]);
        assert_eq!(effort_stops(&offered).len(), 2);
    }

    #[test]
    fn every_stop_survives_the_round_trip_through_the_track() {
        for stops in 2..9 {
            for index in 0..stops {
                let fraction = effort_fraction_for_stop(stops, index);
                assert_eq!(
                    effort_stop_for_fraction(stops, fraction),
                    index,
                    "stop {index} of {stops} came back as something else"
                );
            }
        }
    }

    #[test]
    fn a_drag_that_rests_between_two_rungs_takes_the_nearer_one() {
        // Six stops: the rungs sit at 0.0, 0.2, 0.4, 0.6, 0.8, 1.0.
        assert_eq!(effort_stop_for_fraction(6, 0.09), 0);
        assert_eq!(effort_stop_for_fraction(6, 0.11), 1);
        assert_eq!(effort_stop_for_fraction(6, 0.71), 4);
        assert_eq!(effort_stop_for_fraction(6, 0.79), 4);
    }

    #[test]
    fn the_end_stops_own_half_the_track_the_middle_ones_do() {
        let stops = 5;
        let middle = effort_stop_share(stops, 2);
        assert!((middle - 0.25).abs() < f32::EPSILON);
        assert!((effort_stop_share(stops, 0) - middle / 2.0).abs() < f32::EPSILON);
        assert!((effort_stop_share(stops, 4) - middle / 2.0).abs() < f32::EPSILON);
        // The shares cover the track exactly once.
        let total: f32 = (0..stops)
            .map(|index| effort_stop_share(stops, index))
            .sum();
        assert!((total - 1.0).abs() < 1e-5, "shares summed to {total}");
    }

    #[test]
    fn a_single_stop_is_not_a_slider() {
        // One level to offer is not a range; the caller draws no track, and
        // the maths still answers rather than dividing by zero.
        assert_eq!(effort_fraction_for_stop(1, 0), 0.0);
        assert_eq!(effort_stop_for_fraction(1, 0.9), 0);
        assert_eq!(effort_stop_for_fraction(0, 0.5), 0);
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

use gpui::{AnyElement, Context, Pixels, Point, SharedString, div, px};
use sirio_acp::{EffortChoice, ModeCatalog};

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

use super::*;
use ely_gpui_component::{
    buttons::{Button, ButtonVariant, IconButton},
    chat::{Attachment, AttachmentChip, InputHint, PromptInput},
    primitives::{Icon, IconName},
    theme::{ActiveTheme as _, ControlSize, IconSize, Radius, TextSize},
    typography::EllipsisTooltip,
};

/// How much of bezel's text field is clipped away on every side. The field
/// draws a 1px border on an 8px corner radius; clipping 3px clears the arc
/// where it bends inward, so no trace of the border or the focus ring shows.
const FIELD_CROP: f32 = 3.0;

/// How far the clip box sits outside the composer's content: the field's 1px
/// border and 10px inset, less what is clipped. The text lands on the content
/// edge, lined up with the chips and the tool row.
const FIELD_INSET: f32 = 11.0 - FIELD_CROP;

/// The distance from the shell's top edge to the field's first row: its 1px
/// border and 12px padding. The completion popups hang above the token that
/// opened them, and this keeps one opened on the first row above the card
/// instead of over its border.
const SHELL_TOP_INSET: f32 = 13.0;

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
        let focused = self
            .composer_field
            .read(cx)
            .focus_handle(cx)
            .is_focused(window);
        let bezel_theme = bezel::theme::Theme::of(cx).clone();
        let placeholder = self.composer_placeholder();
        if placeholder != self.composer_placeholder_shown {
            self.composer_placeholder_shown = placeholder.clone();
            self.composer_field
                .update(cx, |field, cx| field.set_placeholder(placeholder, cx));
        }
        let disabled = self.composer_disabled();
        let can_send = self.can_send();
        let entity = cx.entity();

        // Swift's `modePill` (ComposerControlBar.swift) pairs a status dot
        // with the permission mode's name: the dot carries the connection
        // state, the label the mode, and a raw state word ("idle"/"working")
        // only stands in while no mode is known. The port used to let
        // "working" and "connecting" take the label over from the mode, so
        // the permission the user picked was unreadable exactly while a
        // turn ran (`status_pill_content`, `composer_view.rs`).
        let connecting = self.connecting;
        // F-CHAT-15 / #136: a known mode names itself from the first frame,
        // never waiting on `has_completed_turn`.
        let (dot, label) = composer_view::status_pill_content(
            connecting,
            self.streaming,
            self.client.is_some(),
            self.mode_catalog.as_ref(),
        );
        let dot = match dot {
            composer_view::PillDot::Busy => rgb(0xf5a623),
            composer_view::PillDot::Ready => rgb(0x53c653),
            composer_view::PillDot::Offline => rgb(0x8a8d99),
        };
        let mode_selectable = self.mode_selectable();
        let status_pill = div()
            .flex()
            .flex_none()
            .items_center()
            .gap(px(6.0))
            .h(px(24.0))
            .px(px(7.0))
            .rounded(theme.radii.control)
            .text_size(typography.ui_size);
        let status_pill = if connecting {
            status_pill
                .id("chat-connecting")
                .debug_selector(|| "chat-connecting".into())
        } else {
            status_pill
                .id("chat-status")
                .debug_selector(|| "chat-status".into())
        };
        let status_pill = status_pill
            .when(mode_selectable, |this| {
                this.hover(|style| style.bg(bezel_theme.element_hover))
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.toggle_mode_picker(window, cx);
                    }))
            })
            .child(div().w(px(6.0)).h(px(6.0)).rounded(px(3.0)).bg(dot))
            .child(div().text_color(theme.text).child(label))
            .when(mode_selectable, |this| this.child(picker_chevron(theme)));

        let selected_model_name = self
            .selected_model
            .as_deref()
            .and_then(|selected| {
                self.available_models
                    .iter()
                    .find(|option| option.id == selected)
                    .map(|option| option.name.clone())
            })
            .or_else(|| self.selected_model.clone())
            .unwrap_or_else(|| "Claude Code".into());
        let model_entity = entity.clone();
        let effort_label = self.effort.as_ref().and_then(|effort| {
            effort.current_value.as_ref().map(|current| {
                let name = effort
                    .choices
                    .iter()
                    .find(|choice| choice.value == *current)
                    .map(|choice| choice.name.clone())
                    .unwrap_or_else(|| current.clone());
                // Not uppercased any more: it used to be a bare caption that
                // needed to read as chrome, and now it sits beside its own
                // "Effort" label exactly the way the model value sits beside
                // "Model".
                name
            })
        });
        // The picker chip — "Model" in muted chrome text, the value in
        // title text, the effort level when one is selected — opens only
        // once a turn has completed and the agent reported models. Without
        // models the pill degrades to a plain agent badge (F-CHAT-36): no
        // label, no chevron, no picker.
        let model_control = if self.model_control_visible() {
            let model_selection_id = self
                .selected_model
                .as_deref()
                .map(|id| format!("model-selection-{id}"))
                .unwrap_or_else(|| "model-selection-none".into());
            div()
                .id("model-chip")
                .debug_selector(|| "model-chip".into())
                .flex()
                .items_center()
                .gap(px(6.0))
                .h(px(24.0))
                .px(px(7.0))
                .rounded(theme.radii.control)
                .text_size(typography.ui_size)
                // Sized to its content, not to the row. It used to carry
                // `flex_1`, which stretched the pill the whole width of the
                // control row and stranded its own chevron ~200px from the
                // model name, next to the overflow button -- so the chevron
                // read as belonging to nothing and the model value read as a
                // caption rather than a picker. It still shrinks on a tight
                // row -- that is what keeps the name's ellipsis working --
                // but a 56px floor stops it collapsing to nothing: the name
                // ellipsizes while the row wraps around it, never erasing
                // the picker.
                .min_w(px(56.0))
                .hover(|style| style.bg(bezel_theme.element_hover))
                .on_click(cx.listener(|this, _, window, cx| {
                    this.toggle_model_picker(window, cx);
                }))
                .child(div().text_color(theme.text_faint).child("Model"))
                .child(
                    div()
                        .id(model_selection_id.clone())
                        .debug_selector(move || model_selection_id)
                        // No `flex_1`. Inert on its own now that the pill
                        // hugs its content -- there is no slack left inside
                        // to absorb, and removing it alone does not move the
                        // chevron, which the test below confirms. It goes
                        // because the two together are what stranded the
                        // chevron: restore `flex_1` on the pill and this
                        // would push it to the far edge again.
                        // `min_w_0` + `text_ellipsis` do the real work,
                        // truncating a long name when the row is tight.
                        .min_w_0()
                        .text_ellipsis()
                        .text_color(theme.text)
                        .child(selected_model_name.clone()),
                )
                .child(
                    div()
                        .id("model-chip-chevron")
                        .debug_selector(|| "model-chip-chevron".into())
                        .flex()
                        .flex_none()
                        .items_center()
                        .justify_center()
                        .child(picker_chevron(theme)),
                )
        } else {
            // #206: this badge names the *agent*, so it reads the agent.
            // It used to render `selected_model_name`, a model variable
            // whose fallback is the literal "Claude Code" -- so it named
            // the wrong agent for every other one until models arrived,
            // and named an agent at all for a chat whose banner two
            // inches above says the agent is unknowable. `agent_name` is
            // `None` in exactly that case, which is the case the banner
            // is about.
            let agent_badge_name = self.agent_badge_name();
            div()
                .id("agent-badge")
                .debug_selector(|| "agent-badge".into())
                .flex()
                .items_center()
                .gap(px(6.0))
                .h(px(24.0))
                .px(px(7.0))
                .rounded(theme.radii.control)
                .text_size(typography.ui_size)
                // Same rule as the chip above: hug the content.
                .min_w_0()
                .child(
                    div()
                        .min_w_0()
                        .text_ellipsis()
                        .text_color(theme.text)
                        .child(agent_badge_name),
                )
        };

        // The effort level is a peer of the model, not a caption inside it:
        // it is changed about as often, so it belongs at the same depth and
        // carries its own label. Drawn only when the agent reports a value
        // AND the picker can actually open, so this is never a click target
        // that leads nowhere. `flex_none` keeps it intact while the model
        // chip beside it absorbs the squeeze on a narrow pane — past that
        // the chip wraps whole to the next line, never over the send disc.
        let effort_control = effort_label
            .filter(|_| self.model_control_visible())
            .map(|label| {
                let effort_entity = entity.clone();
                div()
                    .id("effort-chip")
                    .debug_selector(|| "effort-chip".into())
                    .flex()
                    .flex_none()
                    .items_center()
                    .gap(px(6.0))
                    .h(px(24.0))
                    .px(px(7.0))
                    .rounded(theme.radii.control)
                    .text_size(typography.ui_size)
                    .hover(|style| style.bg(bezel_theme.element_hover))
                    .on_click(move |_, window, cx| {
                        effort_entity.update(cx, |chat, cx| chat.toggle_effort_picker(window, cx));
                    })
                    .child(div().text_color(theme.text_faint).child("Effort"))
                    .child(
                        div()
                            .id("model-effort-label")
                            .debug_selector(|| "model-effort-label".into())
                            .text_color(theme.text)
                            .child(label),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_none()
                            .items_center()
                            .justify_center()
                            .child(picker_chevron(theme)),
                    )
            });

        // Fast mode: a toggle, not a picker, so it is a chip that shows its
        // own state rather than one that opens a list. Drawn only when the
        // session reported the feature at all — an older CLI says nothing
        // and gets no chip, rather than a dead one. When the CLI has
        // refused it, the chip stays and carries the CLI's own sentence,
        // the way a missing language server names the program.
        let fast_control = self
            .fast_mode
            .clone()
            .filter(|_| self.model_control_visible())
            .map(|fast| {
                let blocked = fast.blocked_by.clone();
                let fast_entity = entity.clone();
                let selector = if blocked.is_some() {
                    "fast-mode-chip-blocked"
                } else {
                    "fast-mode-chip"
                };
                let label_colour: gpui::Hsla = match (&blocked, fast.enabled) {
                    (Some(_), _) => theme.text_faint.into(),
                    // On, the chip is filled with the theme's own accent
                    // pair rather than a colour of Sirio's: the palette is
                    // bezel's, all of it.
                    (None, true) => bezel_theme.on_solid,
                    (None, false) => theme.text.into(),
                };
                div()
                    .id("fast-mode-chip")
                    .debug_selector(move || selector.into())
                    .flex()
                    .flex_none()
                    .items_center()
                    .gap(px(6.0))
                    .h(px(24.0))
                    .px(px(7.0))
                    .rounded(theme.radii.control)
                    .text_size(typography.ui_size)
                    .when(fast.enabled && blocked.is_none(), |chip| {
                        chip.bg(bezel_theme.solid)
                    })
                    .when(blocked.is_none(), |chip| {
                        chip.hover(|style| style.bg(bezel_theme.element_hover))
                            .on_click(move |_, _, cx| {
                                fast_entity.update(cx, |chat, cx| chat.toggle_fast_mode(cx));
                            })
                    })
                    .when_some(blocked, |chip, reason| {
                        let reason = SharedString::from(reason);
                        chip.tooltip(move |window, cx| Tooltip::text(reason.clone(), window, cx))
                    })
                    .child(div().text_color(label_colour).child("Fast"))
            });

        // Background tasks: a count, not a list. The CLI sends the whole
        // running set on every change and drains it as each task ends, so
        // this is only ever "how many are still going" — what *finished*
        // arrives separately, as a notice in the transcript, because the
        // list is already empty by the time that is known.
        let background_tasks = (self.background_tasks > 0).then(|| {
            let running = self.background_tasks;
            div()
                .id("background-tasks-chip")
                .debug_selector(|| "background-tasks-chip".into())
                .flex()
                .flex_none()
                .items_center()
                .gap(px(6.0))
                .h(px(24.0))
                .px(px(7.0))
                .rounded(theme.radii.control)
                .text_size(typography.ui_size)
                .text_color(theme.text_faint)
                .tooltip(move |window, cx| {
                    Tooltip::text(
                        SharedString::from(if running == 1 {
                            "1 background task running".to_string()
                        } else {
                            format!("{running} background tasks running")
                        }),
                        window,
                        cx,
                    )
                })
                .child(format!("⌁ {running}"))
        });

        // The thinking display. Drawn only while the transport has it and
        // the CLI has not refused the verb; unset shows no value at all,
        // because nothing reports the session's own and a word here would
        // be one nobody read (F-CHAT-18's rule, as the effort track already
        // applies it).
        let thinking = self
            .thinking_display
            .clone()
            .filter(|state| !state.unsupported)
            .filter(|_| self.model_control_visible());
        let thinking_control = thinking.clone().map(|state| {
            let thinking_entity = entity.clone();
            let chosen = state.chosen.clone();
            div()
                .id("thinking-chip")
                .debug_selector(|| "thinking-chip".into())
                .flex()
                .flex_none()
                .items_center()
                .gap(px(6.0))
                .h(px(24.0))
                .px(px(7.0))
                .rounded(theme.radii.control)
                .text_size(typography.ui_size)
                .hover(|style| style.bg(bezel_theme.element_hover))
                .on_click(move |_, window, cx| {
                    thinking_entity.update(cx, |chat, cx| chat.toggle_thinking_picker(window, cx));
                })
                .child(div().text_color(theme.text_faint).child("Thinking"))
                .when_some(chosen, |chip, chosen| {
                    chip.child(
                        div()
                            .debug_selector(|| "thinking-chip-value".into())
                            .text_color(theme.text)
                            .child(thinking_display_name(&chosen).to_string()),
                    )
                })
        });

        let thinking_picker = self.thinking_picker_open.then(|| {
            // The shared painter is bound further down, past this chip.
            let view = bezel::motion::Painter::of(cx);
            let chosen = thinking.and_then(|state| state.chosen);
            popover::anchored_menu_above(
                "thinking-picker-menu",
                div()
                    .id("thinking-picker")
                    .debug_selector(|| "thinking-picker".into())
                    .key_context("ChatThinkingPicker")
                    .track_focus(&self.thinking_picker_focus)
                    .on_action(cx.listener(Self::cancel))
                    .w(px(200.0))
                    .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                        this.thinking_picker_open = false;
                        cx.notify();
                    }))
                    .child(
                        popover::popover_card(&bezel_theme).child(
                            div()
                                .flex()
                                .flex_col()
                                // The agent's own answer is a row, not the
                                // absence of one: choosing it is how a user
                                // undoes a choice they made.
                                .children(thinking_rows().map(|value| {
                                    let id = value.unwrap_or("default");
                                    let row_entity = entity.clone();
                                    let is_selected = chosen.as_deref() == value;
                                    popover::menu_row_nav(
                                        &bezel_theme,
                                        is_selected,
                                        false,
                                        bezel::motion::Fade::new(
                                            view,
                                            format!("thinking-option-{id}"),
                                        ),
                                    )
                                    .id(format!("thinking-option-{id}"))
                                    .debug_selector(move || format!("thinking-option-{id}"))
                                    .on_click(move |_, _, cx| {
                                        row_entity.update(cx, |chat, cx| {
                                            chat.select_thinking_display(
                                                value.map(str::to_string),
                                                cx,
                                            );
                                        });
                                    })
                                    .child(
                                        value
                                            .map_or("Agent's own", thinking_display_name)
                                            .to_string(),
                                    )
                                })),
                        ),
                    )
                    .into_any_element(),
                None,
            )
        });

        // The effort selector, anchored to the chip above. A slider rather
        // than a row per level: effort is a scale, and the levels are the
        // session's own — `supportedEffortLevels` differs per model, so the
        // track grows and shrinks with the model instead of naming rungs the
        // model does not have.
        let effort_picker = self
            .effort_picker_open
            .then(|| self.effort.clone())
            .flatten()
            .map(|effort| {
                let stops: Vec<EffortChoice> =
                    effort_stops(&effort.choices).into_iter().cloned().collect();
                let count = stops.len();
                let current = effort
                    .current_value
                    .clone()
                    .unwrap_or_else(|| EFFORT_RESET.to_string());
                // `None` while the session is on its model's own default:
                // that is not a point on the track, so nothing is filled and
                // the value reads in the muted tone the rest of the row uses
                // for "not chosen".
                let selected = stops.iter().position(|stop| stop.value == current);
                let fraction = selected.map_or(0.0, |index| effort_fraction_for_stop(count, index));
                let current_name = effort
                    .choices
                    .iter()
                    .find(|choice| choice.value == current)
                    .map(|choice| choice.name.clone())
                    .unwrap_or_else(|| current.clone());
                let reset = effort
                    .choices
                    .iter()
                    .find(|choice| choice.value == EFFORT_RESET)
                    .cloned();
                let heading = effort.name.clone().unwrap_or_else(|| "Effort".to_string());

                let zones: Vec<AnyElement> = stops
                    .iter()
                    .enumerate()
                    .map(|(index, stop)| {
                        let value = stop.value.clone();
                        let selector_value = value.clone();
                        let zone_entity = entity.clone();
                        div()
                            .id(format!("effort-stop-{value}"))
                            .debug_selector(move || format!("effort-stop-{selector_value}"))
                            // Not an equal share each: a stop owns the track
                            // that is nearer to it than to its neighbours, and
                            // the two ends have a neighbour on one side only.
                            // Equal shares would put every boundary half a
                            // step away from where the drag snaps.
                            .w(relative(effort_stop_share(count, index)))
                            .h_full()
                            .cursor_pointer()
                            // The zones sit above the track, so they are what
                            // the pointer actually lands on: gpui only arms a
                            // drag on the element whose hitbox is topmost at
                            // mouse-down. Each one therefore starts the
                            // track's drag, under the track's own id — the
                            // move is read against the whole track's bounds
                            // by the listener below, not against one zone's.
                            .on_drag(SliderDrag("effort-slider".into()), |_, _, _, cx| {
                                cx.new(|_| gpui::Empty)
                            })
                            .on_click(move |_, _, cx| {
                                zone_entity.update(cx, |chat, cx| {
                                    chat.select_effort(value.clone(), cx);
                                });
                            })
                            .into_any_element()
                    })
                    .collect();

                let drag_stops = stops.clone();
                let track = div()
                    .relative()
                    .w_full()
                    .h(px(16.0))
                    .child(bezel_theme.slider(fraction))
                    .child(
                        div()
                            .id("effort-slider")
                            .debug_selector(|| "effort-slider".into())
                            .absolute()
                            .inset_0()
                            .flex()
                            .items_stretch()
                            .on_drag_move(cx.listener(
                                move |chat, event: &DragMoveEvent<SliderDrag>, _, cx| {
                                    let Some(fraction) =
                                        slider_fraction(event, "effort-slider", cx)
                                    else {
                                        return;
                                    };
                                    let Some(stop) =
                                        drag_stops.get(effort_stop_for_fraction(count, fraction))
                                    else {
                                        return;
                                    };
                                    // Every stop reached is a control request
                                    // to the agent, and a drag crosses the
                                    // track in dozens of moves. Only a stop
                                    // the session is not already on is worth
                                    // asking for.
                                    let standing = chat
                                        .effort
                                        .as_ref()
                                        .and_then(|effort| effort.current_value.as_deref());
                                    if standing != Some(stop.value.as_str()) {
                                        chat.select_effort(stop.value.clone(), cx);
                                    }
                                },
                            ))
                            .children(zones),
                    );

                let ends = div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .text_size(typography.caption2)
                    .text_color(theme.text_faint)
                    .child(
                        stops
                            .first()
                            .map(|stop| stop.name.clone())
                            .unwrap_or_default(),
                    )
                    .child(
                        stops
                            .last()
                            .map(|stop| stop.name.clone())
                            .unwrap_or_default(),
                    );

                let reset_row = reset.map(|reset| {
                    let reset_entity = entity.clone();
                    let reset_value = reset.value.clone();
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(6.0))
                        .child(div().h(px(1.0)).w_full().bg(theme.border))
                        .child(
                            div()
                                .id("effort-reset")
                                .debug_selector(|| "effort-reset".into())
                                .h(px(22.0))
                                .px(px(6.0))
                                .flex()
                                .items_center()
                                .rounded(theme.radii.control)
                                .text_size(typography.caption2)
                                .text_color(theme.text)
                                .when(selected.is_none(), |this| this.bg(theme.element_active))
                                .hover(|style| style.bg(theme.overlay))
                                .on_click(move |_, _, cx| {
                                    reset_entity.update(cx, |chat, cx| {
                                        chat.select_effort(reset_value.clone(), cx);
                                        // A reset is a command, not an
                                        // adjustment: it answers and closes,
                                        // where the track stays open because
                                        // moving a knob is something you do
                                        // more than once.
                                        chat.effort_picker_open = false;
                                        cx.notify();
                                    });
                                })
                                .child("Use the model's default"),
                        )
                });

                // A scale needs two ends. One level is a choice between it
                // and the model's default, which is a row — a knob with
                // nowhere to slide would be a control that cannot be worked.
                let scale = (count > 1).then(|| {
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(6.0))
                        // The knob is 14px centred on the track ends, so it
                        // overhangs 7px each side: without this inset it runs
                        // into the card's 4px padding and clips.
                        .px(px(8.0))
                        .child(track)
                        .child(ends)
                });
                let lone_stop = stops.first().filter(|_| count == 1).map(|stop| {
                    let value = stop.value.clone();
                    let selector_value = value.clone();
                    let lone_entity = entity.clone();
                    div()
                        .id(format!("effort-stop-{value}"))
                        .debug_selector(move || format!("effort-stop-{selector_value}"))
                        .h(px(22.0))
                        .px(px(6.0))
                        .flex()
                        .items_center()
                        .rounded(theme.radii.control)
                        .text_size(typography.caption2)
                        .text_color(theme.text)
                        .when(selected.is_some(), |this| this.bg(theme.element_active))
                        .hover(|style| style.bg(theme.overlay))
                        .on_click(move |_, _, cx| {
                            lone_entity
                                .update(cx, |chat, cx| chat.select_effort(value.clone(), cx));
                        })
                        .child(stop.name.clone())
                });

                popover::anchored_menu_above(
                    "effort-picker-menu",
                    div()
                        .id("effort-picker")
                        .debug_selector(|| "effort-picker".into())
                        .key_context("ChatEffortPicker")
                        .track_focus(&self.effort_picker_focus)
                        .on_action(cx.listener(Self::cancel))
                        .w(px(220.0))
                        .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                            this.effort_picker_open = false;
                            cx.notify();
                        }))
                        .child(
                            popover::popover_card(&bezel_theme).child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap(px(8.0))
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .justify_between()
                                            .gap(px(8.0))
                                            .text_size(typography.caption2)
                                            .child(
                                                div().text_color(theme.text_faint).child(heading),
                                            )
                                            .child(
                                                div()
                                                    .id("effort-current")
                                                    .debug_selector(move || {
                                                        if selected.is_some() {
                                                            "effort-current".into()
                                                        } else {
                                                            "effort-current-unset".into()
                                                        }
                                                    })
                                                    .text_color(if selected.is_some() {
                                                        theme.text
                                                    } else {
                                                        theme.text_faint
                                                    })
                                                    .child(current_name),
                                            ),
                                    )
                                    .children(scale)
                                    .children(lone_stop)
                                    .children(reset_row),
                            ),
                        )
                        .into_any_element(),
                    None,
                )
            });

        let view = bezel::motion::Painter::of(cx);
        let model_picker = self.model_picker_open.then(|| {
            let picker_entity = model_entity.clone();
            // F-CHAT-16: "Recommended" is not a protocol flag — `ModelOption`
            // has none, and the ACP layer never carries one — it is purely
            // the driver's own first-listed choice, matching Swift's
            // `recommendedId = models.first?.modelId` exactly. The search
            // filter runs over the full, unfiltered list order (`filter`,
            // not `sort`) so a query never reorders results.
            let recommended_id = self
                .available_models
                .first()
                .map(|option| option.id.clone());
            let query = self.model_search_field.read(cx).content().to_string();
            let filtered_models: Vec<ModelOption> = self
                .available_models
                .iter()
                .filter(|option| model_query_matches(option, &query))
                .cloned()
                .collect();
            let selected_id = self.selected_model.clone();
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
                    .child(
                        popover::popover_card(&bezel_theme).child(
                            div()
                                .flex()
                                .flex_col()
                                .when(!self.available_models.is_empty(), |this| {
                                    this.child(
                                        div()
                                            .id("model-search-input")
                                            .debug_selector(|| "model-search-input".into())
                                            .w_full()
                                            .mb(px(6.0))
                                            .child(self.model_search_field.clone()),
                                    )
                                })
                                .child(
                                    div()
                                        .id("model-picker-list")
                                        .debug_selector(|| "model-picker-list".into())
                                        .max_h(px(MODEL_PICKER_LIST_MAX_H))
                                        .overflow_y_scroll()
                                        .track_scroll(&self.model_picker_scroll)
                                        .flex()
                                        .flex_col()
                                        .when(self.available_models.is_empty(), |this| {
                                            this.child(
                                                div()
                                                    .p(px(8.0))
                                                    .text_size(typography.footnote)
                                                    .text_color(theme.text_faint)
                                                    .child(
                                                        "The connected agent did not report any models.",
                                                    ),
                                            )
                                        })
                                        .when(
                                            !self.available_models.is_empty()
                                                && filtered_models.is_empty(),
                                            |this| {
                                                this.child(
                                                    div()
                                                        .id("model-picker-no-match")
                                                        .debug_selector(|| {
                                                            "model-picker-no-match".into()
                                                        })
                                                        .p(px(8.0))
                                                        .text_size(typography.footnote)
                                                        .text_color(theme.text_faint)
                                                        .child("No models match"),
                                                )
                                            },
                                        )
                                        .children(filtered_models.iter().cloned().map(|option| {
                                            let option_id = option.id.clone();
                                            let option_name = option.name.clone();
                                            let option_entity = picker_entity.clone();
                                            let is_recommended = recommended_id.as_deref()
                                                == Some(option_id.as_str());
                                            let is_selected =
                                                selected_id.as_deref() == Some(option_id.as_str());
                                            popover::menu_row_nav(
                                                &bezel_theme,
                                                is_selected,
                                                false,
                                                bezel::motion::Fade::new(
                                                    view,
                                                    format!("model-option-{option_id}"),
                                                ),
                                            )
                                            .id(format!("model-option-{option_id}"))
                                            .debug_selector(move || {
                                                format!("model-option-{option_id}")
                                            })
                                            .on_click(move |_, _, cx| {
                                                option_entity.update(cx, |chat, cx| {
                                                    chat.select_model(option.clone(), cx);
                                                });
                                            })
                                            .child(
                                                div()
                                                    .flex_1()
                                                    .min_w_0()
                                                    .text_ellipsis()
                                                    .child(option_name),
                                            )
                                            .when(is_recommended, |this| {
                                                this.child(
                                                    div()
                                                        .id("model-option-recommended")
                                                        .debug_selector(|| {
                                                            "model-option-recommended".into()
                                                        })
                                                        .flex_shrink_0()
                                                        .px(px(5.0))
                                                        .rounded(px(4.0))
                                                        .text_size(typography.caption2)
                                                        .text_color(theme.text)
                                                        .bg(theme.overlay_strong)
                                                        .child("Recommended"),
                                                )
                                            })
                                        })),
                                )
                        ),
                    )
                    .into_any_element(),
                None,
            )
        });

        // F-CHAT-15: the session-mode picker, anchored above the status
        // pill the same way `model_picker` anchors above the model chip.
        // No search field — mode lists are small and entirely agent-defined
        // (ask/plan/auto today), so a flat list of rows is enough.
        let mode_picker = self.mode_picker_open.then(|| {
            let mode_entity = entity.clone();
            let current_id = self
                .mode_catalog
                .as_ref()
                .map(|catalog| catalog.current_id.clone())
                .unwrap_or_default();
            let options: Vec<AgentMode> = self
                .mode_catalog
                .as_ref()
                .map(|catalog| catalog.options.clone())
                .unwrap_or_default();
            popover::anchored_menu_above(
                "mode-picker-menu",
                div()
                    .id("mode-picker")
                    .debug_selector(|| "mode-picker".into())
                    .key_context("ChatModelPicker")
                    .track_focus(&self.mode_picker_focus)
                    .on_action(cx.listener(Self::cancel))
                    .w(px(200.0))
                    .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                        this.mode_picker_open = false;
                        cx.notify();
                    }))
                    .child(
                        popover::popover_card(&bezel_theme).child(
                            div()
                                .flex()
                                .flex_col()
                                .when(options.is_empty(), |this| {
                                    this.child(
                                        div()
                                            .p(px(8.0))
                                            .text_size(typography.footnote)
                                            .text_color(theme.text_faint)
                                            .child("No modes offered"),
                                    )
                                })
                                .children(options.into_iter().map(|mode| {
                                    let mode_id = mode.id.clone();
                                    let mode_name = mode.name.clone();
                                    let row_entity = mode_entity.clone();
                                    let is_selected = mode.id == current_id;
                                    popover::menu_row_nav(
                                        &bezel_theme,
                                        is_selected,
                                        false,
                                        bezel::motion::Fade::new(
                                            view,
                                            format!("mode-option-{mode_id}"),
                                        ),
                                    )
                                    .id(format!("mode-option-{mode_id}"))
                                    .debug_selector(move || format!("mode-option-{mode_id}"))
                                    .on_click(move |_, _, cx| {
                                        row_entity.update(cx, |chat, cx| {
                                            chat.select_mode(mode.clone(), cx)
                                        });
                                    })
                                    .child(mode_name)
                                })),
                        ),
                    )
                    .into_any_element(),
                None,
            )
        });

        let context_usage = self.context_usage.clone();
        let context_amount = context_usage
            .as_ref()
            .filter(|usage| usage.size > 0)
            .map(|usage| (usage.used as f32 / usage.size as f32).clamp(0.0, 1.0))
            .unwrap_or(0.0);
        let context_ring_entity = entity.clone();
        // Above the 80% warning threshold the arc switches from the gauge
        // blue to the shared danger token, the same rule as the reference
        // `contextRingColor`. The warning wrapper element exists only in
        // that state, so drawn tests can see the colour decision without
        // reading pixels.
        let context_warning = context_amount > 0.8;
        let warning_element = div()
            .id("context-ring-warning")
            .debug_selector(|| "context-ring-warning".into());
        let context_ring = div()
            .id("context-ring")
            .debug_selector(|| "context-ring".into())
            .relative()
            .w(px(16.0))
            .h(px(16.0))
            .rounded(px(8.0))
            .border_1()
            .border_color(theme.border)
            .hover(|style| style.bg(theme.overlay))
            .on_click(move |_, window, cx| {
                context_ring_entity.update(cx, |chat, cx| chat.toggle_context_popover(window, cx));
            })
            .when(context_warning, |this| this.child(warning_element))
            .child(
                div()
                    .id("context-ring-progress")
                    .debug_selector(|| "context-ring-progress".into())
                    .absolute()
                    .inset_0()
                    .child({
                        // The paint closure is `'static`, so it cannot borrow
                        // `theme`; copy out the two colours it draws with.
                        let ring_danger = theme.danger;
                        let ring_accent = theme.accent;
                        canvas(
                            move |_, _, _| {},
                            move |bounds, _, window, _| {
                                if context_amount <= 0.0 {
                                    return;
                                }
                                let origin_x: f32 = bounds.origin.x.into();
                                let origin_y: f32 = bounds.origin.y.into();
                                let width: f32 = bounds.size.width.into();
                                let center =
                                    point(px(origin_x + width / 2.0), px(origin_y + width / 2.0));
                                let radius = width / 2.0 - 2.0;
                                let start = point(center.x, px(origin_y + 1.0));
                                let angle = -std::f32::consts::FRAC_PI_2
                                    + context_amount * std::f32::consts::TAU;
                                let end = point(
                                    px(origin_x + width / 2.0 + radius * angle.cos()),
                                    px(origin_y + width / 2.0 + radius * angle.sin()),
                                );
                                let mut path = PathBuilder::stroke(px(2.0));
                                path.move_to(start);
                                if context_amount >= 1.0 {
                                    path.arc_to(
                                        point(px(radius), px(radius)),
                                        px(0.0),
                                        false,
                                        true,
                                        point(center.x, px(origin_y + width - 1.0)),
                                    );
                                    path.arc_to(
                                        point(px(radius), px(radius)),
                                        px(0.0),
                                        false,
                                        true,
                                        start,
                                    );
                                } else if context_amount > 0.0 {
                                    path.arc_to(
                                        point(px(radius), px(radius)),
                                        px(0.0),
                                        context_amount > 0.5,
                                        true,
                                        end,
                                    );
                                }
                                if let Ok(path) = path.build() {
                                    window.paint_path(
                                        path,
                                        if context_warning {
                                            ring_danger
                                        } else {
                                            ring_accent
                                        },
                                    );
                                }
                            },
                        )
                        .absolute()
                        .size_full()
                    }),
            );

        let context_popover = if self.context_popover_open {
            let usage = context_usage.clone();
            Some(popover::anchored_menu_above_end(
                "context-popover-menu",
                div()
                    .id("context-popover")
                    .debug_selector(|| "context-popover".into())
                    .key_context("ChatContextPopover")
                    .track_focus(&self.context_popover_focus)
                    .on_action(cx.listener(Self::cancel))
                    .w(px(285.0))
                    // `anchored_menu_*` mounts this straight on the popover
                    // surface, which paints the fill and the hairline but no
                    // inset -- unlike the pickers above, which go through
                    // `popover::popover_card`. Without it the usage lines sat
                    // flush against the border.
                    .p(px(4.0))
                    .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                        this.context_popover_open = false;
                        cx.notify();
                    }))
                    .when_some(usage, |this, usage| {
                        // `None` is a state of its own, not a zero: a
                        // session whose `usage_update` named no window size
                        // has nothing to report, and folding that into "0%"
                        // would claim an empty context nobody measured.
                        // Agents that never send a usage update (Pi's ACP
                        // adapter among them) are exactly the ones a
                        // fabricated zero would libel.
                        let percent = if usage.size == 0 {
                            None
                        } else {
                            Some(((usage.used as f64 / usage.size as f64) * 100.0).round() as u64)
                        };
                        let cost = usage
                            .cost
                            .map(|cost| format!("Cost: {:.2} {}", cost.amount, cost.currency));
                        let this = match percent {
                            Some(percent) => this
                                .child(
                                    div()
                                        .id(format!(
                                            "context-usage-{}-of-{}",
                                            usage.used, usage.size
                                        ))
                                        .debug_selector(move || {
                                            format!(
                                                "context-usage-{}-of-{}",
                                                usage.used, usage.size
                                            )
                                        })
                                        .text_size(typography.footnote)
                                        .text_color(theme.text)
                                        .child(format!("{percent}% of context used")),
                                )
                                .child(
                                    div()
                                        .mt(px(4.0))
                                        .text_size(typography.caption2)
                                        .text_color(theme.text_faint)
                                        .child(format!("{} / {} tokens", usage.used, usage.size)),
                                ),
                            // A partial update — the breakdown merged in
                            // before any used/size pair, or one that named
                            // no window — must not read as "0 of 0 tokens":
                            // the breakdown rows below are real, the
                            // fraction simply is not known.
                            None => this.child(
                                div()
                                    .id("context-usage-unreported")
                                    .debug_selector(|| "context-usage-unreported".into())
                                    .text_size(typography.footnote)
                                    .text_color(theme.text_faint)
                                    .child("The agent has not reported its context window size."),
                            ),
                        };
                        this.when_some(cost, |this, cost| {
                            this.child(
                                div()
                                    .mt(px(4.0))
                                    .text_size(typography.caption2)
                                    .text_color(theme.text_faint)
                                    .child(cost),
                            )
                        })
                        // F-CHAT-18: input/output/cache breakdown, present
                        // only for agents that report end-of-turn usage
                        // (`unstable_end_turn_token_usage`) -- absent for
                        // every other agent, so the rows are opt-in rather
                        // than showing zeros.
                        .when(
                            usage.input_tokens.is_some()
                                || usage.output_tokens.is_some()
                                || usage.cached_read_tokens.is_some(),
                            |this| {
                                this.child(
                                    div()
                                        .id("context-usage-breakdown")
                                        .debug_selector(|| "context-usage-breakdown".into())
                                        .mt(px(6.0))
                                        .pt(px(6.0))
                                        .border_t_1()
                                        .border_color(theme.border)
                                        .flex()
                                        .flex_col()
                                        .gap(px(2.0))
                                        .when_some(usage.input_tokens, |this, tokens| {
                                            this.child(
                                                div()
                                                    .text_size(typography.caption2)
                                                    .text_color(theme.text_faint)
                                                    .child(format!("Input: {tokens} tokens")),
                                            )
                                        })
                                        .when_some(usage.output_tokens, |this, tokens| {
                                            this.child(
                                                div()
                                                    .text_size(typography.caption2)
                                                    .text_color(theme.text_faint)
                                                    .child(format!("Output: {tokens} tokens")),
                                            )
                                        })
                                        .when_some(usage.cached_read_tokens, |this, tokens| {
                                            this.child(
                                                div()
                                                    .text_size(typography.caption2)
                                                    .text_color(theme.text_faint)
                                                    .child(format!("Cache read: {tokens} tokens")),
                                            )
                                        }),
                                )
                            },
                        )
                    })
                    .when(self.context_usage.is_none(), |this| {
                        this.child(
                            div()
                                .id("context-usage-never-reported")
                                .debug_selector(|| "context-usage-never-reported".into())
                                .text_size(typography.footnote)
                                .text_color(theme.text_faint)
                                .child("The agent has not reported context usage yet."),
                        )
                    })
                    .into_any_element(),
                None,
            ))
        } else {
            None
        };

        let send_entity = entity.clone();
        let stop_entity = entity.clone();
        let attach_entity = entity.clone();
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
        // rows, in the card's own `surface_raised` fill, where it read as a
        // transparent veil rather than a menu. The same token paints both
        // on purpose (they are the same step above the page); what makes
        // this a card of its own is that it floats over the page, with the
        // gap below it.
        let slash_popup = if self.slash_popup_visible() {
            let candidates = self.slash_candidates();
            let active = self.slash_filter.active();
            let view = bezel::motion::Painter::of(cx);
            let anchor = self
                .composer_field
                .read(cx)
                .offset_bounds(0, window)
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
                                .debug_selector(move || {
                                    format!("slash-option-name-{name_for_label_id}")
                                })
                                .flex_none()
                                .text_size(typography.footnote)
                                .text_color(bezel_theme.text)
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
                                    .text_color(bezel_theme.text_faint)
                                    .child(hint),
                            )
                        })
                        .into_any_element()
                    })
                    .collect();
                div()
                    .child(composer_view::menu_above_at(
                        "slash-popup-menu",
                        anchor,
                        popover::popover_card(&bezel_theme)
                            .debug_selector(|| "slash-popup".into())
                            .w(px(280.0))
                            .child(div().flex().flex_col().children(rows))
                            .into_any_element(),
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
            let view = bezel::motion::Painter::of(cx);
            let anchor = self
                .composer_field
                .read(cx)
                .offset_bounds(at, window)
                .map(|row| gpui::point(row.left(), row.top() - px(8.0 + SHELL_TOP_INSET)));
            anchor.map(|anchor| {
                let rows: Vec<AnyElement> = candidates
                    .into_iter()
                    .enumerate()
                    .map(|(position, path)| {
                        let row_entity = entity.clone();
                        let path_for_id = path.clone();
                        let path_for_accept = path.clone();
                        popover::menu_row(
                            &bezel_theme,
                            Some(position) == active,
                            bezel::motion::Fade::new(view, format!("mention-option-{path}")),
                        )
                        .id(SharedString::from(format!("mention-option-{path_for_id}")))
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
                            bezel::ui::icons::icon(bezel::ui::icons::DOCUMENT)
                                .size(px(12.0))
                                .text_color(bezel_theme.text_faint),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .text_ellipsis()
                                .text_size(typography.footnote)
                                .text_color(bezel_theme.text)
                                .child(path),
                        )
                        .into_any_element()
                    })
                    .collect();
                div()
                    .child(composer_view::menu_above_at(
                        "mention-popup-menu",
                        anchor,
                        popover::popover_card(&bezel_theme)
                            .id("mention-popup-card")
                            .debug_selector(|| "mention-popup-card".into())
                            .w(px(360.0))
                            .child(div().flex().flex_col().children(rows))
                            .into_any_element(),
                    ))
                    .into_any_element()
            })
        } else {
            None
        };

        // Overflow menu (F-CHAT-14): Follow Edited Files toggle and New
        // Conversation, the two secondary composer actions the control row
        // does not carry inline.
        let overflow_menu = if self.overflow_open {
            let follow_label = if self.following_edited_files {
                "Stop Following"
            } else {
                "Follow Edited Files"
            };
            Some(popover::anchored_menu_above_end(
                "composer-overflow-menu-menu",
                div()
                    .id("composer-overflow-menu")
                    .debug_selector(|| "composer-overflow-menu".into())
                    .key_context("ChatOverflowMenu")
                    .track_focus(&self.overflow_focus)
                    .on_action(cx.listener(Self::cancel))
                    .w(px(200.0))
                    // The card inset `popover_card` would have carried: this
                    // menu mounts on the bare popover surface, so its rows'
                    // hover wash ran into the border.
                    .p(px(4.0))
                    .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                        this.overflow_open = false;
                        cx.notify();
                    }))
                    .child(
                        div()
                            .id("overflow-follow")
                            .debug_selector(|| "overflow-follow".into())
                            .w_full()
                            .px(px(8.0))
                            .py(px(6.0))
                            .rounded(theme.radii.control)
                            .text_size(typography.footnote)
                            .text_color(theme.text)
                            .hover(|style| style.bg(theme.overlay))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.following_edited_files = !this.following_edited_files;
                                cx.notify();
                            }))
                            .child(follow_label),
                    )
                    .child(
                        div()
                            .id("overflow-new-conversation")
                            .debug_selector(|| "overflow-new-conversation".into())
                            .w_full()
                            .px(px(8.0))
                            .py(px(6.0))
                            .rounded(theme.radii.control)
                            .text_size(typography.footnote)
                            .text_color(theme.text)
                            .hover(|style| style.bg(theme.overlay))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.new_conversation(cx);
                            }))
                            .child("New Conversation"),
                    )
                    .child(
                        div()
                            .id("overflow-chat-history")
                            .debug_selector(|| "overflow-chat-history".into())
                            .w_full()
                            .px(px(8.0))
                            .py(px(6.0))
                            .rounded(theme.radii.control)
                            .text_size(typography.footnote)
                            .text_color(theme.text)
                            .hover(|style| style.bg(theme.overlay))
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.toggle_chat_history(window, cx);
                            }))
                            .child("Chat History"),
                    )
                    .into_any_element(),
                None,
            ))
        } else {
            None
        };

        // F-CHAT-34/35: the Chat History popover — a session list with
        // Open/Delete per row, or the "No past chats" empty state.
        let chat_history_menu = if self.history_open {
            let rows: Vec<AnyElement> = if self.history_sessions.is_empty() {
                vec![
                    div()
                        .id("chat-history-empty")
                        .debug_selector(|| "chat-history-empty".into())
                        .px(px(8.0))
                        .py(px(10.0))
                        .text_size(typography.footnote)
                        .text_color(theme.text_muted)
                        .child("No past chats")
                        .into_any_element(),
                ]
            } else {
                self.history_sessions
                    .iter()
                    .map(|session| {
                        let tab_id = session.tab_id.clone();
                        let confirming = self.history_delete_confirm.as_deref() == Some(&tab_id);
                        let open_tab_id = tab_id.clone();
                        let row_id = SharedString::from(format!("chat-history-row-{tab_id}"));
                        let row_selector = format!("chat-history-row-{tab_id}");
                        let title = if session.title.is_empty() {
                            "Untitled chat".to_string()
                        } else {
                            session.title.clone()
                        };
                        div()
                            .id(row_id)
                            .debug_selector(move || row_selector.clone())
                            .w_full()
                            .flex()
                            .items_center()
                            .justify_between()
                            .gap(px(6.0))
                            .px(px(8.0))
                            .py(px(6.0))
                            .rounded(theme.radii.control)
                            .hover(|style| style.bg(theme.overlay))
                            .child(
                                div()
                                    .id(SharedString::from(format!("chat-history-open-{tab_id}")))
                                    .flex_1()
                                    .min_w_0()
                                    .overflow_hidden()
                                    .whitespace_nowrap()
                                    .text_ellipsis()
                                    .text_size(typography.footnote)
                                    .text_color(theme.text)
                                    .child(title)
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.open_chat_history_session(open_tab_id.clone(), cx);
                                    })),
                            )
                            .child(if confirming {
                                let confirm_tab_id = tab_id.clone();
                                div()
                                    .id(SharedString::from(format!(
                                        "chat-history-confirm-{tab_id}"
                                    )))
                                    .flex()
                                    .flex_shrink_0()
                                    .gap(px(6.0))
                                    .child(
                                        div()
                                            .id(SharedString::from(format!(
                                                "chat-history-confirm-delete-{tab_id}"
                                            )))
                                            .flex_shrink_0()
                                            .text_size(typography.footnote)
                                            .text_color(theme.danger)
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                this.confirm_delete_chat_session(
                                                    confirm_tab_id.clone(),
                                                    cx,
                                                );
                                            }))
                                            .child("Confirm"),
                                    )
                                    .child(
                                        div()
                                            .id(SharedString::from(format!(
                                                "chat-history-cancel-delete-{tab_id}"
                                            )))
                                            .flex_shrink_0()
                                            .text_size(typography.footnote)
                                            .text_color(theme.text_muted)
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.cancel_delete_chat_session(cx);
                                            }))
                                            .child("Cancel"),
                                    )
                                    .into_any_element()
                            } else {
                                let delete_tab_id = tab_id.clone();
                                let delete_selector = format!("chat-history-delete-{tab_id}");
                                div()
                                    .id(SharedString::from(format!("chat-history-delete-{tab_id}")))
                                    .debug_selector(move || delete_selector.clone())
                                    .flex_shrink_0()
                                    .text_size(typography.footnote)
                                    .text_color(theme.text_muted)
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.request_delete_chat_session(delete_tab_id.clone(), cx);
                                    }))
                                    .child("Delete")
                                    .into_any_element()
                            })
                            .into_any_element()
                    })
                    .collect()
            };
            Some(popover::anchored_menu_above_end(
                "chat-history-menu-menu",
                div()
                    .id("chat-history-menu")
                    .debug_selector(|| "chat-history-menu".into())
                    .key_context("ChatHistoryMenu")
                    .track_focus(&self.history_focus)
                    .on_action(cx.listener(Self::cancel))
                    .w(px(260.0))
                    .max_h(px(320.0))
                    // Same card inset as the overflow menu above; the rows
                    // scroll inside it rather than against the border.
                    .p(px(4.0))
                    .overflow_y_scroll()
                    .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                        this.history_open = false;
                        cx.notify();
                    }))
                    .children(rows)
                    .into_any_element(),
                None,
            ))
        } else {
            None
        };

        let attach_button = div()
            .flex_none()
            .debug_selector(|| "attach-image".into())
            .child(
                IconButton::new("attach-image-button", IconName::Paperclip)
                    .variant(ButtonVariant::Ghost)
                    .size(ControlSize::Sm)
                    .tooltip("Attach image")
                    .on_click(move |_, window, cx| {
                        attach_entity.update(cx, |chat, cx| chat.attach_image(window, cx));
                    }),
            );

        let overflow_button = div()
            .flex_none()
            .debug_selector(|| "composer-overflow".into())
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

        // Bezel's field draws a frame of its own (fill, border, a 10px inset).
        // The shell is the one frame the composer has, so the field is laid
        // partly outside a clip box (`FIELD_CROP`, `FIELD_INSET`): its border
        // and the focus ring are never drawn, and its fill is the shell's own
        // (`PromptInput::custom`).
        let input_box = div()
            .id("composer-input")
            .debug_selector(|| "composer-input".into())
            .relative()
            .w_full()
            .when(disabled, |input| input.opacity(0.6))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(|this, event: &MouseDownEvent, window, cx| {
                    cx.stop_propagation();
                    this.open_composer_context_menu(event.position, window, cx);
                }),
            )
            .child(
                div()
                    .mx(px(-FIELD_INSET))
                    .overflow_hidden()
                    .child(div().m(px(-FIELD_CROP)).child(self.composer_field.clone())),
            )
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
                        .text_color(theme.danger)
                        .child(message),
                )
            });

        let streaming = self.streaming;
        let mut prompt = PromptInput::custom("composer-shell", editor, can_send, move |_, cx| {
            send_entity.update(cx, |chat, cx| chat.send(cx));
        })
        .focused(focused);
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
            prompt = prompt
                .tool(
                    div()
                        .relative()
                        .flex_none()
                        .child(status_pill)
                        .children(mode_picker),
                )
                // The one shrinkable tool: on a tight line the chip ellipsizes
                // its name down to its 56px floor, never to nothing.
                .tool(div().relative().child(model_control).children(model_picker))
                .tool(
                    div()
                        .relative()
                        .flex_none()
                        .children(effort_control)
                        .children(effort_picker),
                )
                .tool(div().relative().flex_none().children(fast_control))
                .tool(div().relative().flex_none().children(background_tasks))
                .tool(
                    div()
                        .relative()
                        .flex_none()
                        .children(thinking_control)
                        .children(thinking_picker),
                )
                .tool(
                    div()
                        .relative()
                        .flex()
                        .flex_none()
                        .items_center()
                        .gap(px(6.0))
                        .h(px(24.0))
                        .px(px(7.0))
                        .rounded(theme.radii.control)
                        .text_size(typography.ui_size)
                        .child(context_ring)
                        .child(
                            div()
                                .id("context-label")
                                .debug_selector(|| "context-label".into())
                                .text_color(theme.text_faint)
                                .child("Context"),
                        )
                        .children(context_popover),
                )
                .trailing(attach_button)
                .trailing(
                    div()
                        .relative()
                        .child(overflow_button)
                        .children(overflow_menu)
                        .children(chat_history_menu),
                );
        }

        // Enter queues while a turn runs and sends otherwise; the hint says
        // which, under the card rather than inside it.
        let hint = div()
            .id("composer-input-hint")
            .debug_selector(|| "composer-input-hint".into())
            .w_full()
            .flex()
            .justify_center()
            .pt(px(4.0))
            .child(InputHint::new().enter(if streaming {
                "to queue ·"
            } else {
                "to send ·"
            }));

        div()
            .w_full()
            .max_w(px(TRANSCRIPT_WIDTH))
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
                    .child(prompt),
            )
            .child(hint)
            .into_any_element()
    }
}
