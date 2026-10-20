//! The question asked before a chat switches to Bypass permissions: the one
//! mode in which the agent runs every tool, shell commands included, without
//! asking. Claude Code's own terminal asks the same before it applies it.

use ely_gpui_component::buttons::{Button, ButtonVariant};
use ely_gpui_component::overlays::Dialog;

use super::*;
use crate::text_selection::selectable_text;

const TITLE: &str = "Bypass permissions?";

const BODY: &str = "The agent will run every tool without asking, shell commands and file \
                    edits included, until you pick another mode. Use it only where a \
                    mistake costs nothing: a disposable worktree, a container, a VM.";

impl Chat {
    /// Holds `mode` until the dialog answers, instead of applying it.
    pub(super) fn ask_before_bypass(&mut self, mode: AgentMode, cx: &mut Context<Self>) {
        self.mode_picker_open = false;
        self.bypass_ask = Some(mode);
        cx.notify();
    }

    /// Cancel, Escape and the scrim: the mode stays what it was.
    fn close_bypass_ask(&mut self, cx: &mut Context<Self>) {
        if self.bypass_ask.take().is_some() {
            cx.notify();
        }
    }

    /// Applies the held mode. Nothing when no dialog was open, so a second
    /// press does nothing.
    fn confirm_bypass_ask(&mut self, cx: &mut Context<Self>) {
        if let Some(mode) = self.bypass_ask.take() {
            self.apply_mode(mode, cx);
        }
    }

    pub(super) fn render_bypass_dialog(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        self.bypass_ask.as_ref()?;
        let entity = cx.entity();
        let (close, confirm) = (entity.clone(), entity);
        let dialog = Dialog::new("chat-bypass-dialog", TITLE, move |_, cx| {
            close.update(cx, |chat, cx| chat.close_bypass_ask(cx))
        })
        .child(selectable_text(BODY))
        .action(|close| {
            dialog_button("chat-bypass-cancel", "Cancel", ButtonVariant::Ghost, move |window, cx| {
                close(window, cx)
            })
        })
        .action(move |close| {
            dialog_button(
                "chat-bypass-confirm",
                "Bypass permissions",
                ButtonVariant::Danger,
                move |window, cx| {
                    // Confirm first: closing runs `close_bypass_ask`, which
                    // would take the held mode before the confirm read it.
                    confirm.update(cx, |chat, cx| chat.confirm_bypass_ask(cx));
                    close(window, cx);
                },
            )
        });
        Some(
            div()
                .debug_selector(|| "chat-bypass-dialog".into())
                .child(dialog)
                .into_any_element(),
        )
    }
}

/// One of the dialog's buttons, findable by its id.
fn dialog_button(
    id: &'static str,
    label: &'static str,
    variant: ButtonVariant,
    on_click: impl Fn(&mut Window, &mut App) + 'static,
) -> AnyElement {
    div()
        .id(id)
        .debug_selector(move || id.to_owned())
        .flex_none()
        .child(
            Button::new(id, label)
                .variant(variant)
                .on_click(move |_, window, cx| on_click(window, cx)),
        )
        .into_any_element()
}
