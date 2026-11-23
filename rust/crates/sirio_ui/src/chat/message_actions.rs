//! The action row under a chat message (spec §5.1): Ely's Copy, an Ely icon button for Quote and,
//! on the one row where Continue belongs (`Chat::continue_row`), Ely's Continue. The row is a
//! sibling of the message's selectable text, never inside it, and no text it holds is selectable
//! (`docs/agents/architecture/text-selection.md`).
//!
//! `ActionsFrame::row` is the one place that decides what a transcript row draws. The drawn row,
//! the control socket's readout and its `copy` and `quote` verbs all read it.

use ely_gpui_component::buttons::{ButtonVariant, IconButton};
use ely_gpui_component::chat::{MessageActions, continue_button};
use ely_gpui_component::primitives::IconName;
use ely_gpui_component::theme::ControlSize;
use gpui::{AnyElement, ElementId, Entity, SharedString, div, prelude::*};

use super::{Chat, Entry, TurnRowRole};

/// How visible a message's action row is. Either way the row has its place in the layout, so
/// hovering a message moves nothing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ActionsShown {
    /// Drawn: the last reply of the transcript, once its turn is over.
    Always,
    /// Not drawn until the pointer is over the message (`group_hover`).
    Hover,
}

/// What a transcript row draws as actions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum RowActions {
    /// Copy and Quote, and Continue when it sits on this row.
    Message {
        shown: ActionsShown,
        continue_here: bool,
    },
    /// Continue alone, on the footer of a turn that has no reply to carry it.
    ContinueOnly,
}

impl RowActions {
    /// The actions drawn, comma-joined, for the control socket's `actions`.
    #[cfg(debug_assertions)]
    pub(super) fn names(actions: Option<Self>) -> &'static str {
        match actions {
            Some(Self::Message {
                continue_here: false,
                ..
            }) => "copy,quote",
            Some(Self::Message {
                continue_here: true,
                ..
            }) => "copy,quote,continue",
            Some(Self::ContinueOnly) => "continue",
            None => "",
        }
    }

    /// `always`, `hover` or `none`, for the control socket's `actionsShown`.
    #[cfg(debug_assertions)]
    pub(super) fn shown_wire(actions: Option<Self>) -> &'static str {
        match actions {
            Some(Self::Message {
                shown: ActionsShown::Hover,
                ..
            }) => "hover",
            Some(Self::Message {
                shown: ActionsShown::Always,
                ..
            })
            | Some(Self::ContinueOnly) => "always",
            None => "none",
        }
    }
}

/// What a frame needs to know about the whole transcript to decide its rows' actions. Taken once
/// per frame, beside the turn roles: the row processor is called for every visible index.
#[derive(Clone, Copy, Debug)]
pub(super) struct ActionsFrame {
    /// The last reply of the transcript, while no turn streams.
    always_row: Option<usize>,
    /// The row Continue sits on (`Chat::continue_row`).
    continue_row: Option<usize>,
}

impl Chat {
    pub(super) fn actions_frame(&self) -> ActionsFrame {
        ActionsFrame {
            always_row: if self.streaming {
                None
            } else {
                self.entries
                    .iter()
                    .rposition(|entry| matches!(entry, Entry::Assistant { .. }))
            },
            continue_row: self.continue_row(),
        }
    }

    /// What the row at `index` draws now: what the drawn transcript and the control socket's verbs
    /// agree on. A row in a folded turn draws nothing.
    pub(super) fn row_actions_now(&self, index: usize) -> Option<RowActions> {
        let entry = self.entries.get(index)?;
        let roles = super::turn_row_roles(&self.entries, &self.folded_turns);
        if !roles_draw(roles.get(index)?) {
            return None;
        }
        self.actions_frame().row(index, entry)
    }

    /// The socket's `copy` and `quote` press a button the row has: refused, with the reason, when
    /// the row at `index` draws no Copy and Quote.
    pub(super) fn require_message_row(&self, verb: &str, index: usize) -> Result<(), String> {
        match self.row_actions_now(index) {
            Some(RowActions::Message { .. }) => Ok(()),
            _ => Err(format!(
                "surface.chat.transcript {verb}: entry {index} draws no {verb} action"
            )),
        }
    }

    /// The text Copy takes and Quote quotes: what the entry holds, as sent for a user message and
    /// as its Markdown source for a reply.
    pub(super) fn message_text(&self, index: usize) -> Option<&str> {
        match self.entries.get(index)? {
            Entry::User { text, .. } | Entry::Assistant { text, .. } => Some(text.as_str()),
            _ => None,
        }
    }

    /// Debug builds only, the control socket's readouts of a row: `actions` (what it draws),
    /// `actionsShown` (`always`, `hover` or `none`) and, for each button the last paint drew,
    /// `actionBounds.<copy|quote|continue|row>` as `[x, y, width, height]` in window pixels. A hidden
    /// row paints none of its buttons, so a hover row reports no button until the pointer is over
    /// its message; `actionBounds.row` is the row's own place and is reported either way.
    #[cfg(debug_assertions)]
    pub(super) fn action_columns(
        &self,
        index: usize,
        frame: &ActionsFrame,
        roles: &[TurnRowRole],
        row: &mut std::collections::BTreeMap<String, String>,
    ) {
        let actions = self
            .entries
            .get(index)
            .filter(|_| roles.get(index).is_some_and(roles_draw))
            .and_then(|entry| frame.row(index, entry));
        row.insert("actions".into(), RowActions::names(actions).into());
        row.insert("actionsShown".into(), RowActions::shown_wire(actions).into());
        for ((entry, name), bounds) in self.action_bounds.borrow().iter() {
            if *entry == index {
                let value = serde_json::to_string(bounds).unwrap_or_default();
                row.insert(format!("actionBounds.{name}"), value);
            }
        }
    }

    /// Quote: holds the message at `index` as the quote of the next prompt, replacing any held,
    /// and focuses the composer. The author is only for the composer's card: `You` for a user
    /// message, the agent's name for a reply.
    pub(super) fn quote_message(&mut self, index: usize, cx: &mut gpui::Context<Self>) {
        let (author, text) = match self.entries.get(index) {
            Some(Entry::User { text, .. }) => ("You".to_string(), text.clone()),
            Some(Entry::Assistant { text, .. }) => (self.agent_badge_name(), text.clone()),
            _ => return,
        };
        self.quote = Some(super::quote::Quote { author, text });
        // The click and the socket's verb both come through here, and only a render has a window
        // to focus in.
        self.composer_focus_wanted = true;
        cx.notify();
    }
}

impl ActionsFrame {
    /// What `entry`, at `index`, draws as actions. Read from the transcript's own entry: a row is
    /// drawn from a copy whose stream is gone.
    pub(super) fn row(&self, index: usize, entry: &Entry) -> Option<RowActions> {
        let continue_here = self.continue_row == Some(index);
        match entry {
            Entry::User { .. } => Some(RowActions::Message {
                shown: ActionsShown::Hover,
                continue_here: false,
            }),
            // A reply still streaming has nothing final to copy.
            Entry::Assistant { stream: None, .. } => Some(RowActions::Message {
                shown: if self.always_row == Some(index) {
                    ActionsShown::Always
                } else {
                    ActionsShown::Hover
                },
                continue_here,
            }),
            Entry::TurnFooter(_) if continue_here => Some(RowActions::ContinueOnly),
            _ => None,
        }
    }
}

/// Whether a row with this role is drawn at all: a folded turn stands in for itself with one row.
pub(super) fn roles_draw(role: &TurnRowRole) -> bool {
    matches!(role, TurnRowRole::Normal | TurnRowRole::Refoldable { .. })
}

/// A part of the row in a box of its own, so the box is the part's. In a debug build it records
/// where it is painted (`Chat::action_bounds`): a part that is not painted records nothing.
fn measured(
    part: impl IntoElement,
    chat: &Entity<Chat>,
    entry: usize,
    name: &'static str,
    selector: Option<&'static str>,
) -> AnyElement {
    let wrapper = div()
        .relative()
        .flex_none()
        .when_some(selector, |wrapper, selector| {
            wrapper.debug_selector(move || format!("{selector}-{entry}"))
        })
        .child(part);
    #[cfg(debug_assertions)]
    let wrapper = wrapper.child(probe(chat, entry, name));
    #[cfg(not(debug_assertions))]
    let _ = (chat, name);
    wrapper.into_any_element()
}

/// Covers its parent and records, when painted, the parent's window bounds.
#[cfg(debug_assertions)]
fn probe(chat: &Entity<Chat>, entry: usize, name: &'static str) -> AnyElement {
    let chat = chat.clone();
    gpui::canvas(
        |bounds, _, _| bounds,
        move |_, bounds, _, cx| {
            let drawn = chat.read(cx).action_bounds.clone();
            drawn.borrow_mut().insert(
                (entry, name),
                [
                    bounds.origin.x.into(),
                    bounds.origin.y.into(),
                    bounds.size.width.into(),
                    bounds.size.height.into(),
                ],
            );
        },
    )
    .absolute()
    .top_0()
    .left_0()
    .size_full()
    .into_any_element()
}

/// Ely's Continue, running `Chat::continue_turn`. The press stops there: on a turn's footer the
/// row sits inside the click that folds the turn.
fn continue_control(id: &ElementId, entry: usize, chat: &Entity<Chat>) -> AnyElement {
    let run = chat.clone();
    measured(
        continue_button((id.clone(), "continue")).on_click(move |_, _, cx| {
            cx.stop_propagation();
            run.update(cx, |chat, cx| chat.continue_turn(cx));
        }),
        chat,
        entry,
        "continue",
        Some("message-continue"),
    )
}

/// Continue alone, for the footer of a turn that has no reply to carry it.
pub(super) fn render_lone_continue(id: ElementId, entry: usize, chat: Entity<Chat>) -> AnyElement {
    continue_control(&id, entry, &chat)
}

/// The action row of message `entry`: Ely's `MessageActions` for Copy (no handler set, so Copy
/// alone), then Quote, then Continue when `continue_here`. `text` is what Copy takes. `Hover`
/// leaves the row invisible, and it keeps its place, until the pointer is over `hover_group`,
/// which the caller sets on an element holding the message and this row.
pub(super) fn render_message_actions(
    id: ElementId,
    entry: usize,
    text: SharedString,
    shown: ActionsShown,
    continue_here: bool,
    chat: Entity<Chat>,
    hover_group: SharedString,
) -> AnyElement {
    let quote_chat = chat.clone();
    let copy = measured(
        MessageActions::new((id.clone(), "actions"), text),
        &chat,
        entry,
        "copy",
        Some("message-actions"),
    );
    let quote = measured(
        IconButton::new((id.clone(), "quote"), IconName::Quote)
            .variant(ButtonVariant::Ghost)
            .size(ControlSize::Sm)
            .tooltip("Quote")
            .on_click(move |_, _, cx| {
                cx.stop_propagation();
                quote_chat.update(cx, |chat, cx| chat.quote_message(entry, cx));
            }),
        &chat,
        entry,
        "quote",
        Some("message-quote"),
    );
    let row = div()
        .id((id.clone(), "row"))
        .flex()
        .items_center()
        .gap_0p5()
        .child(copy)
        .child(quote)
        .when(continue_here, |row| {
            row.child(continue_control(&id, entry, &chat))
        })
        .when(shown == ActionsShown::Hover, |row| {
            row.invisible()
                .group_hover(hover_group, |style| style.visible())
        });
    // The row itself is measured from a wrapper that is always painted, so a hover row that is
    // hidden still says where its place is: the spot to put a pointer on to show it.
    measured(row, &chat, entry, "row", None)
}
