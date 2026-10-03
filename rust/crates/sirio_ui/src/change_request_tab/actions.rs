//! Acting on the change request (spec §5, §7.1): the one write in flight,
//! `perform` — the single function every button calls — the header's
//! buttons, and the words under them when a write fails.

use std::collections::BTreeMap;

use ely_gpui_component::forms::TextInput;
use ely_gpui_component::primitives::{IconName, Severity};
use sirio_forge::{Action, ActionOutcome, ChangeState, CommentRef};

use super::composer::ComposerSend;
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

/// The title, target branch and description of an edit in progress.
pub(crate) struct EditFields {
    pub(crate) title: Entity<TextInput>,
    pub(crate) target: Entity<TextInput>,
    pub(crate) body: Entity<TextInput>,
}

/// One of the viewer's own timeline entries, open for editing.
pub(crate) struct CommentEdit {
    pub(crate) comment: CommentRef,
    pub(crate) field: Entity<TextInput>,
}

pub(crate) struct ActionsState {
    pub(crate) state: ActionState,
    task: Option<Task<()>>,
    pub(crate) edit: Option<EditFields>,
    pub(crate) comment_edit: Option<CommentEdit>,
    /// The composer's field, and whether it holds only blanks (kept by an
    /// observer, so a render never has to read it).
    /// Built by the first render: Ely's input needs a window the constructors lack.
    pub(crate) composer: Option<Entity<TextInput>>,
    pub(crate) composer_blank: bool,
    /// What the composer held when it was sent: on success the field is
    /// cleared only if it still holds exactly that, so words typed while the
    /// request was in flight are never wiped.
    pub(crate) sent: Option<String>,
}

impl ActionsState {
    pub(crate) fn new() -> Self {
        Self {
            state: ActionState::Idle,
            task: None,
            edit: None,
            comment_edit: None,
            composer: None,
            composer_blank: true,
            sent: None,
        }
    }
}

/// What a header button does.
enum HeaderAction {
    Edit,
    Do(Action),
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
    _theme: &Theme,
    enabled: bool,
    on_click: impl Fn(&mut Window, &mut App) + 'static,
) -> AnyElement {
    super::ely_ui::text_button(
        id,
        label,
        None,
        super::ely_ui::ButtonState::enabled(enabled),
        on_click,
    )
}

/// `action_button` for one of many: the timeline's *Edit* on entry `index`.
pub(crate) fn indexed_button(
    name: &'static str,
    index: usize,
    label: &'static str,
    _theme: &Theme,
    enabled: bool,
    on_click: impl Fn(&mut Window, &mut App) + 'static,
) -> AnyElement {
    use ely_gpui_component::buttons::{Button, ButtonVariant};
    div()
        .id((name, index))
        .debug_selector(move || format!("{name}-{index}"))
        .flex_none()
        .child(
            Button::new((name, index), label)
                .variant(ButtonVariant::Ghost)
                .disabled(!enabled)
                .on_click(move |_, window, cx| on_click(window, cx)),
        )
        .into_any_element()
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
                let warned = outcome.warning.is_some();
                self.actions.state = outcome.warning.map_or(ActionState::Idle, ActionState::Warning);
                self.action_succeeded(kind, warned, cx);
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

    /// Text is cleared only when its write succeeded; a failure keeps it,
    /// and so does a warning — the words never reached the forge (spec §7.1).
    /// A warning still forgets what was sent, so a later edit cannot clear
    /// the composer by mistake.
    fn action_succeeded(&mut self, kind: &'static str, warned: bool, cx: &mut Context<Self>) {
        match kind {
            "edit" => self.actions.edit = None,
            "edit-comment" => self.actions.comment_edit = None,
            "comment" | "approve" | "request-changes" => {
                let sent = self.actions.sent.take();
                if let (false, Some(sent), Some(composer)) =
                    (warned, sent, self.actions.composer.clone())
                {
                    if composer.read(cx).text() == sent {
                        composer.update(cx, |input, cx| input.set_text("", cx));
                    }
                }
            }
            _ => {}
        }
    }

    /// The header's own buttons: whichever of *Edit*, draft ↔ ready and
    /// close ↔ reopen the forge says the viewer may use on this state.
    pub(crate) fn render_action_bar(&self, _theme: &Theme, entity: &Entity<Self>) -> Option<AnyElement> {
        let header = self.header.value()?;
        let caps = header.capabilities;
        let state = header.summary.state;
        let mut items: Vec<(&'static str, IconName, &'static str, HeaderAction)> = Vec::new();
        if caps.can_edit {
            items.push((
                "change-request-edit",
                IconName::Pencil,
                "Edit",
                HeaderAction::Edit,
            ));
        }
        if caps.can_toggle_draft {
            match state {
                ChangeState::Draft => items.push((
                    "change-request-ready",
                    IconName::GitPullRequest,
                    "Ready for review",
                    HeaderAction::Do(Action::MarkReady),
                )),
                ChangeState::Open => items.push((
                    "change-request-draft",
                    IconName::GitPullRequestDraft,
                    "Convert to draft",
                    HeaderAction::Do(Action::ConvertToDraft),
                )),
                ChangeState::Closed | ChangeState::Merged => {}
            }
        }
        if caps.can_change_state {
            match state {
                ChangeState::Open | ChangeState::Draft => items.push((
                    "change-request-close-request",
                    IconName::GitPullRequestClosed,
                    "Close",
                    HeaderAction::Do(Action::Close),
                )),
                ChangeState::Closed => items.push((
                    "change-request-reopen",
                    IconName::RotateCcw,
                    "Reopen",
                    HeaderAction::Do(Action::Reopen),
                )),
                ChangeState::Merged => {}
            }
        }
        if items.is_empty() {
            return None;
        }
        let enabled = !self.action_busy() && self.actions.edit.is_none();
        Some(
            div()
                .flex()
                .items_center()
                .gap(px(4.0))
                .children(items.into_iter().map(|(id, icon, tooltip, what)| {
                    let entity = entity.clone();
                    super::ely_ui::icon_button(id, icon, tooltip, enabled, move |window, cx| {
                        entity.update(cx, |tab, cx| match &what {
                            HeaderAction::Edit => tab.start_edit(window, cx),
                            HeaderAction::Do(action) => {
                                let _ = tab.perform(action.clone(), cx);
                            }
                        })
                    })
                }))
                .into_any_element(),
        )
    }

    /// One line under the header: what is being sent, or why it failed.
    pub(crate) fn render_action_status(&self, _theme: &Theme) -> Option<AnyElement> {
        let severity = match &self.actions.state {
            ActionState::Idle => return None,
            ActionState::Working(_) => Severity::Info,
            ActionState::Failed { .. } => Severity::Danger,
            ActionState::Unconfirmed { .. } | ActionState::Warning(_) => Severity::Warning,
        };
        let text = match &self.actions.state {
            ActionState::Working(kind) => format!("Sending {kind}…"),
            ActionState::Failed { message, .. } | ActionState::Warning(message) => message.clone(),
            ActionState::Unconfirmed { .. } => {
                "Could not confirm that it went through. Look at the conversation before sending it again."
                    .to_string()
            }
            ActionState::Idle => return None,
        };
        Some(
            div()
                .id("change-request-action-status")
                .debug_selector(|| "change-request-action-status".into())
                .child(super::ely_ui::message(severity, text))
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
        params: &BTreeMap<String, String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let text = |key: &str| params.get(key).cloned();
        match name {
            "close" => self.perform(Action::Close, cx),
            "reopen" => self.perform(Action::Reopen, cx),
            "ready" => self.perform(Action::MarkReady, cx),
            "draft" => self.perform(Action::ConvertToDraft, cx),
            "compose" => {
                let words = text("text").ok_or("compose needs text")?;
                self.ensure_composer(window, cx);
                let composer = self.actions.composer.clone().ok_or("the composer is not built")?;
                composer.update(cx, |input, cx| input.set_text(words, cx));
                Ok(())
            }
            "send" => match text("how").as_deref() {
                Some("comment") => self.send_composer(ComposerSend::Comment, cx),
                Some("approve") => self.send_composer(ComposerSend::Approve, cx),
                Some("request-changes") => self.send_composer(ComposerSend::RequestChanges, cx),
                _ => Err("send needs how: comment, approve or request-changes".to_string()),
            },
            "edit" => {
                self.start_edit(window, cx);
                let fields = self.actions.edit.as_ref().ok_or("the change request is not loaded")?;
                for (key, field) in [
                    ("title", fields.title.clone()),
                    ("target", fields.target.clone()),
                    ("body", fields.body.clone()),
                ] {
                    if let Some(words) = text(key) {
                        field.update(cx, |input, cx| input.set_text(words, cx));
                    }
                }
                self.save_edit(cx)
            }
            "edit-comment" => {
                let index = text("index")
                    .and_then(|index| index.parse().ok())
                    .ok_or("edit-comment needs index")?;
                self.actions.comment_edit = None;
                self.start_comment_edit(index, window, cx);
                let edit = self
                    .actions
                    .comment_edit
                    .as_ref()
                    .ok_or("that timeline entry cannot be edited")?;
                let words = text("text").ok_or("edit-comment needs text")?;
                edit.field.update(cx, |input, cx| input.set_text(words, cx));
                self.save_comment_edit(cx)
            }
            other => Err(format!("unknown action {other}")),
        }
    }
}
