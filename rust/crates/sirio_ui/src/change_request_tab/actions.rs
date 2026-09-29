//! Acting on the change request (spec §5, §7.1): the one write in flight,
//! `perform` — the single function every button calls — the header's
//! buttons, and the words under them when a write fails.

use std::collections::BTreeMap;

use sirio_forge::{Action, ActionOutcome, ChangeState};

use super::*;

/// The one write in flight, or how the last one ended. One at a time per
/// tab: a second click while the first is in flight is refused, so a double
/// click cannot post twice.
pub(crate) enum ActionState {
    Idle,
    Working(&'static str),
    Failed {
        kind: &'static str,
        message: String,
    },
    /// The connection dropped after the request was sent: it may have gone
    /// through. Nothing is sent again by itself (spec §5).
    Unconfirmed {
        kind: &'static str,
    },
    /// It worked, but a second step of it did not.
    Warning(String),
}

impl ActionState {
    /// `idle`, `working`, `failed`, `unconfirmed` or `warning` — the words
    /// the control socket reports.
    pub(crate) fn word(&self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Working(_) => "working",
            Self::Failed { .. } => "failed",
            Self::Unconfirmed { .. } => "unconfirmed",
            Self::Warning(_) => "warning",
        }
    }

    pub(crate) fn kind(&self) -> &'static str {
        match self {
            Self::Working(kind) | Self::Failed { kind, .. } | Self::Unconfirmed { kind } => kind,
            Self::Idle | Self::Warning(_) => "",
        }
    }
}

pub(crate) struct ActionsState {
    pub(crate) state: ActionState,
    task: Option<Task<()>>,
}

impl ActionsState {
    pub(crate) fn new() -> Self {
        Self {
            state: ActionState::Idle,
            task: None,
        }
    }
}

/// What the user reads when a write fails: the forge's own reason where it
/// gave one, the remedy where there is one (spec §9).
pub(crate) fn action_error_text(error: &ForgeError, forge: Forge) -> String {
    match error {
        ForgeError::Forbidden {
            sso_url: Some(url), ..
        } => format!("This organisation requires its SSO: authorise the token at {url}"),
        ForgeError::Forbidden { detail, .. } => format!(
            "This token cannot write here ({detail}). It needs {} — see Settings → Git Hosting.",
            style::write_scopes(forge)
        ),
        ForgeError::Rejected { message, .. } => message.clone(),
        ForgeError::HeadMoved { .. } => {
            "The branch changed since you opened this. Reload to review the new commits.".to_string()
        }
        ForgeError::Unsupported { .. } => {
            format!("Not available on this version of {}.", forge.name())
        }
        ForgeError::NotAuthenticated { host } => format!(
            "Not signed in to {host}. Sign in with `{} auth login --hostname {host}`, or add a token in Settings → Git Hosting.",
            style::cli_name(forge)
        ),
        other => other.to_string(),
    }
}

pub(crate) fn action_button(
    id: &'static str,
    label: &'static str,
    theme: &Theme,
    enabled: bool,
    on_click: impl Fn(&mut App) + 'static,
) -> gpui::Stateful<gpui::Div> {
    let hover = theme.element_hover;
    div()
        .id(id)
        .debug_selector(move || id.to_owned())
        .flex()
        .flex_none()
        .items_center()
        .px(px(8.0))
        .py(px(4.0))
        .rounded(theme.radii.control)
        .text_size(theme.typography.footnote)
        .text_color(if enabled {
            theme.text_muted
        } else {
            theme.text_faint
        })
        .when(enabled, |this| {
            this.cursor_pointer()
                .hover(move |style| style.bg(hover))
                .on_click(move |_, _, cx| on_click(cx))
        })
        .child(label)
}

impl ChangeRequestTab {
    pub(crate) fn action_busy(&self) -> bool {
        matches!(self.actions.state, ActionState::Working(_))
    }

    /// Sends `action` to the forge on the background executor. `Err` is the
    /// reason nothing was sent: an action is already in flight, the forge
    /// asked Sirio to wait, or the tab is not connected.
    pub(crate) fn perform(&mut self, action: Action, cx: &mut Context<Self>) -> Result<(), String> {
        if self.action_busy() {
            return Err("an action is already in flight".to_string());
        }
        if self.rate_paused() {
            return Err("the forge asked Sirio to wait; try again after its reset".to_string());
        }
        let Some(client) = self.client.clone() else {
            return Err("the change request is not connected".to_string());
        };
        let kind = action.kind();
        let number = self.reference.number;
        self.actions.state = ActionState::Working(kind);
        self.actions.task = Some(cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move { client.act(number, &action) })
                .await;
            let _ = this.update(cx, |tab, cx| tab.finish_action(kind, result, cx));
        }));
        cx.notify();
        Ok(())
    }

    fn finish_action(
        &mut self,
        kind: &'static str,
        result: Result<ActionOutcome, ForgeError>,
        cx: &mut Context<Self>,
    ) {
        self.actions.task = None;
        match result {
            Ok(outcome) => {
                self.actions.state = outcome.warning.map_or(ActionState::Idle, ActionState::Warning);
                self.reread_after_write(cx);
            }
            // The request may have gone through: look before saying anything,
            // keep every typed word, and send nothing again.
            Err(ForgeError::Network { .. }) => {
                self.actions.state = ActionState::Unconfirmed { kind };
                self.reread_after_write(cx);
            }
            Err(error) => {
                self.note_rate_limited(&error);
                if matches!(error, ForgeError::HeadMoved { .. }) {
                    self.refresh(cx);
                }
                self.actions.state = ActionState::Failed {
                    kind,
                    message: action_error_text(&error, self.reference.forge),
                };
            }
        }
        cx.notify();
    }

    /// The forge is the truth: what it says now, not what the tab hoped.
    fn reread_after_write(&mut self, cx: &mut Context<Self>) {
        self.refresh(cx);
        cx.emit(ChangeRequestTabEvent::Changed);
    }

    /// The header's own buttons: whichever of draft ↔ ready and close ↔
    /// reopen the forge says the viewer may use on this state.
    pub(crate) fn render_action_bar(&self, theme: &Theme, entity: &Entity<Self>) -> Option<AnyElement> {
        let header = self.header.value()?;
        let caps = header.capabilities;
        let state = header.summary.state;
        let mut items: Vec<(&'static str, &'static str, Action)> = Vec::new();
        if caps.can_toggle_draft {
            match state {
                ChangeState::Draft => items.push((
                    "change-request-ready",
                    "Ready for review",
                    Action::MarkReady,
                )),
                ChangeState::Open => items.push((
                    "change-request-draft",
                    "Convert to draft",
                    Action::ConvertToDraft,
                )),
                ChangeState::Closed | ChangeState::Merged => {}
            }
        }
        if caps.can_change_state {
            match state {
                ChangeState::Open | ChangeState::Draft => items.push((
                    "change-request-close-request",
                    "Close",
                    Action::Close,
                )),
                ChangeState::Closed => items.push((
                    "change-request-reopen",
                    "Reopen",
                    Action::Reopen,
                )),
                ChangeState::Merged => {}
            }
        }
        if items.is_empty() {
            return None;
        }
        let enabled = !self.action_busy();
        Some(
            div()
                .flex()
                .items_center()
                .gap(px(4.0))
                .children(items.into_iter().map(|(id, label, action)| {
                    let entity = entity.clone();
                    action_button(id, label, theme, enabled, move |cx| {
                        entity.update(cx, |tab, cx| {
                            let _ = tab.perform(action.clone(), cx);
                        })
                    })
                }))
                .into_any_element(),
        )
    }

    /// One line under the header: what is being sent, or why it failed.
    pub(crate) fn render_action_status(&self, theme: &Theme) -> Option<AnyElement> {
        let (text, tone): (String, Hsla) = match &self.actions.state {
            ActionState::Idle => return None,
            ActionState::Working(kind) => (format!("Sending {kind}…"), theme.text_faint.into()),
            ActionState::Failed { message, .. } => (message.clone(), theme.danger.into()),
            ActionState::Unconfirmed { .. } => (
                "Could not confirm that it went through. Look at the conversation before sending it again."
                    .to_string(),
                theme.warning.into(),
            ),
            ActionState::Warning(text) => (text.clone(), theme.warning.into()),
        };
        Some(
            div()
                .id("change-request-action-status")
                .debug_selector(|| "change-request-action-status".into())
                .text_size(theme.typography.footnote)
                .text_color(tone)
                .child(selectable_text(text))
                .into_any_element(),
        )
    }

    /// The capabilities that are on, comma-separated, for the control
    /// socket's report; `-` when none is.
    pub(crate) fn caps_words(&self) -> String {
        let Some(header) = self.header.value() else {
            return String::new();
        };
        let caps = header.capabilities;
        let words: Vec<&str> = [
            (caps.can_comment, "comment"),
            (caps.can_approve, "approve"),
            (caps.can_request_changes, "request-changes"),
            (caps.can_edit, "edit"),
            (caps.can_change_state, "state"),
            (caps.can_toggle_draft, "draft"),
        ]
        .into_iter()
        .filter_map(|(on, word)| on.then_some(word))
        .collect();
        if words.is_empty() {
            "-".to_string()
        } else {
            words.join(",")
        }
    }

    /// What the status line says, for the control socket's report.
    pub(crate) fn action_message(&self) -> String {
        match &self.actions.state {
            ActionState::Failed { message, .. } | ActionState::Warning(message) => message.clone(),
            ActionState::Unconfirmed { .. } => "unconfirmed".to_string(),
            ActionState::Idle | ActionState::Working(_) => String::new(),
        }
    }

    /// The control socket's test hook: the buttons' own handlers, by name.
    /// The host serves it in debug builds only, so a release binary has no
    /// way to write to a forge over the socket (spec §10).
    pub fn control_act(
        &mut self,
        name: &str,
        _params: &BTreeMap<String, String>,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        match name {
            "close" => self.perform(Action::Close, cx),
            "reopen" => self.perform(Action::Reopen, cx),
            "ready" => self.perform(Action::MarkReady, cx),
            "draft" => self.perform(Action::ConvertToDraft, cx),
            other => Err(format!("unknown action {other}")),
        }
    }
}
