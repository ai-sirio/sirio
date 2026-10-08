//! The hand-off dialog (change requests C2): one agent takes a change
//! request's worktree, for one purpose, from the tab's action bar, a review
//! thread, a failed check or the list's row menu. The dialog only asks; the
//! host answers `HandoffAsked` with `open_handoff`, and runs `Handoff` and
//! answers it with `set_handoff`.

use ely_gpui_component::forms::{Choice, Input, RadioGroup, TextInput};
use ely_gpui_component::overlays::Dialog;
use ely_gpui_component::primitives::Severity;
use sirio_forge::{CiState, Purpose, Scope};

use crate::ely_ui::{ButtonState, message, new_input, text_button};
use crate::text_selection::selectable_text;
use super::*;

/// Where the agent's work shows: its own terminal pane, or its chat.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Surface {
    Terminal,
    Chat,
}

impl Surface {
    pub fn word(self) -> &'static str {
        match self {
            Self::Terminal => "terminal",
            Self::Chat => "chat",
        }
    }

    pub fn parse(word: &str) -> Option<Self> {
        match word {
            "terminal" => Some(Self::Terminal),
            "chat" => Some(Self::Chat),
            _ => None,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Terminal => "Terminal",
            Self::Chat => "Chat",
        }
    }
}

/// One agent the dialog offers; the host fills these.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HandoffAgent {
    pub id: &'static str,
    pub name: &'static str,
    /// `Err(program)`: its CLI is not on PATH ("claude is not on PATH").
    pub terminal: Result<(), String>,
    /// It has a chat transport today.
    pub chat: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HandoffOptions {
    pub agents: Vec<HandoffAgent>,
    /// The project's last choice: agent id or `None` for no agent, and the surface.
    pub remembered: Option<(Option<&'static str>, Surface)>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HandoffRequest {
    pub purpose: Purpose,
    pub scope: Scope,
    pub agent: Option<&'static str>,
    pub surface: Surface,
    pub instructions: String,
}

/// What the dialog's *Worktree* line shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HandoffPreview {
    Loading,
    Ready { worktree: Result<String, String>, viewer_is_author: Option<bool> },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HandoffState {
    Idle,
    Running,
    Done(String),
    Failed(String),
}

impl HandoffState {
    pub fn word(&self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Running => "running",
            Self::Done(_) => "done",
            Self::Failed(_) => "failed",
        }
    }

    pub fn detail(&self) -> &str {
        match self {
            Self::Done(detail) | Self::Failed(detail) => detail,
            Self::Idle | Self::Running => "",
        }
    }
}

/// The purpose a dialog starts with (ruling 8): fixing failing CI when CI
/// failed, reviewing a change request someone else wrote, resuming one the
/// viewer wrote. An unknown author is not a stranger, so it resumes.
pub(crate) fn default_purpose(ci_failed: bool, viewer_is_author: Option<bool>) -> Purpose {
    match (ci_failed, viewer_is_author) {
        (true, _) => Purpose::Ci,
        (false, Some(false)) => Purpose::Review,
        (false, _) => Purpose::Resume,
    }
}

/// The purpose a thread or a job scope fixes, if it fixes one.
fn fixed_purpose(scope: &Scope) -> Option<Purpose> {
    match scope {
        Scope::Thread(_) => Some(Purpose::Comments),
        Scope::Job(_) => Some(Purpose::Ci),
        Scope::Whole => None,
    }
}

/// Why a purpose cannot be chosen for this scope, in the words the dialog shows.
fn purpose_refusal(scope: &Scope, ci_failed: bool, purpose: Purpose) -> Option<&'static str> {
    match scope {
        Scope::Thread(_) => (purpose != Purpose::Comments).then_some("fixed by the thread you chose"),
        Scope::Job(_) => (purpose != Purpose::Ci).then_some("fixed by the job you chose"),
        Scope::Whole => (purpose == Purpose::Ci && !ci_failed).then_some("CI has not failed"),
    }
}

/// `HH:MM` in the local zone, for a rate limit's reset.
fn reset_clock(until: i64) -> String {
    chrono::DateTime::from_timestamp(until, 0)
        .map(|time| time.with_timezone(&chrono::Local).format("%H:%M").to_string())
        .unwrap_or_else(|| until.to_string())
}

/// The dialog while it is open. `purpose_touched` keeps a preview from
/// overriding a purpose the user chose.
pub(crate) struct HandoffDialog {
    scope: Scope,
    purpose: Purpose,
    purpose_touched: bool,
    agents: Vec<HandoffAgent>,
    agent: Option<&'static str>,
    surface: Surface,
    instructions: Entity<TextInput>,
    preview: HandoffPreview,
    ci_failed: bool,
    /// The reason the last *Start* was refused, or the last attempt failed.
    refusal: Option<String>,
}

impl HandoffDialog {
    fn selected_agent(&self) -> Option<&HandoffAgent> {
        self.agent.and_then(|id| self.agents.iter().find(|agent| agent.id == id))
    }

    /// Why the chosen agent and surface cannot be used, if they cannot.
    fn agent_refusal(&self) -> Option<String> {
        let agent = self.selected_agent()?;
        if let Err(program) = &agent.terminal {
            return Some(format!("{program} is not on PATH"));
        }
        (self.surface == Surface::Chat && !agent.chat).then(|| format!("no chat for {}", agent.name))
    }

    fn worktree_refusal(&self) -> Option<String> {
        match &self.preview {
            HandoffPreview::Ready { worktree: Err(reason), .. } => Some(reason.clone()),
            HandoffPreview::Loading | HandoffPreview::Ready { worktree: Ok(_), .. } => None,
        }
    }

    fn warns(&self, cross_repository: bool) -> bool {
        cross_repository
            || matches!(self.preview, HandoffPreview::Ready { viewer_is_author: Some(false), .. })
    }
}

/// The radio for one agent; a CLI that is not on PATH cannot be chosen.
fn agent_choice(agent: &HandoffAgent) -> Choice {
    let choice = Choice::new(agent.id, agent.name);
    match &agent.terminal {
        Ok(()) => choice,
        Err(program) => choice.disabled().note(format!("{program} is not on PATH")),
    }
}

/// A field of the dialog: its label, its radios, and the notes of its
/// disabled radios. Ely's radio does not draw a `Choice`'s note, so the notes
/// sit beside the group, never inside a radio, and stay selectable.
fn field(
    id: &'static str,
    label: &'static str,
    group: RadioGroup,
    notes: Vec<String>,
    theme: &Theme,
) -> AnyElement {
    div()
        .id(id)
        .debug_selector(move || id.to_owned())
        .flex()
        .flex_col()
        .gap(px(6.0))
        .child(div().text_color(theme.ely.fg_muted).child(label))
        .child(group)
        .children(notes.into_iter().map(|note| {
            div().text_color(theme.ely.fg_subtle).child(selectable_text(note))
        }))
        .into_any_element()
}

impl ChangeRequestTab {
    /// The reason *Start* is refused now, if it is.
    fn start_blocker(&self) -> Option<String> {
        let Some(dialog) = &self.handoff_dialog else {
            return Some("the hand-off dialog is not open".to_string());
        };
        if self.handoff == HandoffState::Running {
            return Some("a hand-off is already running".to_string());
        }
        if self.checkout == CheckoutState::Running {
            return Some("a checkout is already running".to_string());
        }
        if self.action_busy() {
            return Some("an action is already in flight".to_string());
        }
        if let Some(until) = self.paused_reset() {
            return Some(format!("rate limited until {}", reset_clock(until)));
        }
        if let HandoffPreview::Loading = dialog.preview {
            return Some("the worktree is still being worked out".to_string());
        }
        dialog.worktree_refusal().or_else(|| dialog.agent_refusal())
    }

    /// The rate limit's reset, while it is in the future.
    fn paused_reset(&self) -> Option<i64> {
        self.paused_until.filter(|until| style::now() < *until)
    }

    /// The *Hand off to an agent* request from the tab's action bar, a
    /// thread, a failed check or the list. Nothing opens here: the host
    /// answers with `open_handoff`.
    pub fn ask_handoff(&mut self, scope: Scope, cx: &mut Context<Self>) {
        if self.handoff == HandoffState::Running || self.checkout == CheckoutState::Running {
            return;
        }
        cx.emit(ChangeRequestTabEvent::HandoffAsked(scope));
    }

    pub fn open_handoff(
        &mut self,
        scope: Scope,
        options: HandoffOptions,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let ci_failed = self
            .header
            .value()
            .is_some_and(|header| matches!(header.summary.ci, CiState::Failed));
        let first = options.agents.iter().find(|agent| agent.terminal.is_ok()).map(|agent| agent.id);
        let (agent, surface) = match options.remembered {
            Some((None, surface)) => (None, surface),
            Some((Some(id), surface))
                if options.agents.iter().any(|agent| agent.id == id && agent.terminal.is_ok()) =>
            {
                (Some(id), surface)
            }
            _ => (first, Surface::Terminal),
        };
        let chat_ok = agent.is_none_or(|id| options.agents.iter().any(|agent| agent.id == id && agent.chat));
        // With no agent the surface has nothing to show, so it is the terminal (the dialog and the socket agree).
        let surface = if agent.is_none() || (surface == Surface::Chat && !chat_ok) { Surface::Terminal } else { surface };
        let purpose = fixed_purpose(&scope).unwrap_or_else(|| default_purpose(ci_failed, None));
        let instructions = new_input(window, cx, "", Some((2, 6)), "Anything the agent should know…");
        // A reopened dialog starts clean: a previous hand-off's outcome is not this one's.
        if matches!(self.handoff, HandoffState::Done(_) | HandoffState::Failed(_)) {
            self.handoff = HandoffState::Idle;
        }
        self.handoff_dialog = Some(HandoffDialog {
            scope,
            purpose,
            purpose_touched: false,
            agents: options.agents,
            agent,
            surface,
            instructions,
            preview: HandoffPreview::Loading,
            ci_failed,
            refusal: None,
        });
        cx.notify();
    }

    /// The preview answers the *Worktree* line and the warning. Until the
    /// user picks a purpose, the viewer's authorship chooses it.
    pub fn set_handoff_preview(&mut self, preview: HandoffPreview, cx: &mut Context<Self>) {
        let Some(dialog) = self.handoff_dialog.as_mut() else {
            return;
        };
        if let HandoffPreview::Ready { viewer_is_author, .. } = &preview
            && !dialog.purpose_touched
            && fixed_purpose(&dialog.scope).is_none()
        {
            dialog.purpose = default_purpose(dialog.ci_failed, *viewer_is_author);
        }
        dialog.preview = preview;
        cx.notify();
    }

    /// Sets one field of the open dialog, as the user would, for the control
    /// socket. A choice the dialog would disable is refused with its reason.
    pub fn set_handoff_field(&mut self, field: &str, value: &str, cx: &mut Context<Self>) -> Result<(), String> {
        let dialog = self.handoff_dialog.as_mut().ok_or("the hand-off dialog is not open")?;
        match field {
            "purpose" => {
                let purpose = Purpose::parse(value).ok_or_else(|| format!("unknown purpose {value}"))?;
                if let Some(refusal) = purpose_refusal(&dialog.scope, dialog.ci_failed, purpose) {
                    return Err(refusal.to_string());
                }
                dialog.purpose = purpose;
                dialog.purpose_touched = true;
            }
            "agent" => {
                if value == "none" {
                    dialog.agent = None;
                    dialog.surface = Surface::Terminal;
                } else {
                    let agent = dialog
                        .agents
                        .iter()
                        .find(|agent| agent.id == value)
                        .ok_or_else(|| format!("unknown agent {value}"))?;
                    if let Err(program) = &agent.terminal {
                        return Err(format!("{program} is not on PATH"));
                    }
                    dialog.agent = Some(agent.id);
                    // The surface the new agent cannot show falls back to the terminal.
                    if dialog.surface == Surface::Chat && !agent.chat {
                        dialog.surface = Surface::Terminal;
                    }
                }
            }
            "surface" => {
                let surface = Surface::parse(value).ok_or_else(|| format!("unknown surface {value}"))?;
                if dialog.agent.is_none() {
                    return Err("choose an agent first".to_string());
                }
                let previous = dialog.surface;
                dialog.surface = surface;
                if let Some(refusal) = dialog.agent_refusal() {
                    dialog.surface = previous;
                    return Err(refusal);
                }
            }
            "instructions" => {
                let input = dialog.instructions.clone();
                input.update(cx, |input, cx| input.set_text(value, cx));
            }
            other => return Err(format!("unknown hand-off field {other}")),
        }
        cx.notify();
        Ok(())
    }

    /// *Start*: the request goes to the host, the dialog stays open while it
    /// runs. A refusal is returned and shown inside the dialog.
    pub fn start_handoff(&mut self, cx: &mut Context<Self>) -> Result<(), String> {
        if let Some(reason) = self.start_blocker() {
            if let Some(dialog) = self.handoff_dialog.as_mut() {
                dialog.refusal = Some(reason.clone());
            }
            cx.notify();
            return Err(reason);
        }
        let request = {
            let dialog = self.handoff_dialog.as_ref().expect("start_blocker refuses without a dialog");
            HandoffRequest {
                purpose: dialog.purpose,
                scope: dialog.scope.clone(),
                agent: dialog.agent,
                surface: dialog.surface,
                instructions: dialog.instructions.read(cx).text().to_string(),
            }
        };
        self.handoff = HandoffState::Running;
        if let Some(dialog) = self.handoff_dialog.as_mut() {
            dialog.refusal = None;
        }
        cx.emit(ChangeRequestTabEvent::Handoff(request));
        cx.notify();
        Ok(())
    }

    /// The host's answer. `Done` closes the dialog; `Failed` keeps it open
    /// with the reason in it, and the reason also goes on the status line.
    pub fn set_handoff(&mut self, state: HandoffState, cx: &mut Context<Self>) {
        match &state {
            HandoffState::Done(_) => self.handoff_dialog = None,
            HandoffState::Failed(reason) => {
                if let Some(dialog) = self.handoff_dialog.as_mut() {
                    dialog.refusal = Some(reason.clone());
                }
            }
            HandoffState::Idle | HandoffState::Running => {}
        }
        self.handoff = state;
        cx.notify();
    }

    pub fn close_handoff(&mut self, cx: &mut Context<Self>) {
        self.handoff_dialog = None;
        cx.notify();
    }

    /// One line under the header: the hand-off running, or what it made or why it failed.
    pub(crate) fn render_handoff_status(&self, theme: &Theme) -> Option<AnyElement> {
        let (severity, text) = match &self.handoff {
            HandoffState::Idle => return None,
            HandoffState::Running => (Severity::Info, "Handing off to the agent…".to_string()),
            HandoffState::Done(detail) => (Severity::Info, detail.clone()),
            HandoffState::Failed(reason) => (Severity::Danger, reason.clone()),
        };
        Some(
            div()
                .id("change-request-handoff-status")
                .debug_selector(|| "change-request-handoff-status".into())
                .child(message(severity, text, theme))
                .into_any_element(),
        )
    }

    /// The control socket's read of the dialog and the hand-off state.
    pub(crate) fn handoff_fields(&self) -> Vec<(String, String)> {
        let dialog = self.handoff_dialog.as_ref();
        let cross = self.header.value().and_then(|header| header.head.as_ref()).is_some_and(|head| head.cross_repository);
        let worktree = match dialog.map(|dialog| &dialog.preview) {
            None => String::new(),
            Some(HandoffPreview::Loading) => "loading".to_string(),
            Some(HandoffPreview::Ready { worktree: Ok(line), .. }) => line.clone(),
            Some(HandoffPreview::Ready { worktree: Err(reason), .. }) => format!("refused: {reason}"),
        };
        let agents = dialog
            .map(|dialog| {
                dialog
                    .agents
                    .iter()
                    .map(|agent| {
                        format!(
                            "{}:{}{}",
                            agent.id,
                            if agent.terminal.is_ok() { "ok" } else { "missing" },
                            if agent.chat { "+chat" } else { "" }
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(",")
            })
            .unwrap_or_default();
        vec![
            ("handoff".to_string(), self.handoff.word().to_string()),
            ("handoff_detail".to_string(), self.handoff.detail().to_string()),
            ("handoff_dialog".to_string(), if dialog.is_some() { "open" } else { "" }.to_string()),
            ("handoff_purpose".to_string(), dialog.map_or("", |dialog| dialog.purpose.word()).to_string()),
            ("handoff_scope".to_string(), dialog.map_or(String::new(), |dialog| dialog.scope.word())),
            (
                "handoff_agent".to_string(),
                dialog.map_or("", |dialog| dialog.agent.unwrap_or("none")).to_string(),
            ),
            ("handoff_surface".to_string(), dialog.map_or("", |dialog| dialog.surface.word()).to_string()),
            ("handoff_worktree".to_string(), worktree),
            (
                "handoff_warning".to_string(),
                if dialog.is_some_and(|dialog| dialog.warns(cross)) { "yes" } else { "no" }.to_string(),
            ),
            ("handoff_agents".to_string(), agents),
            (
                "handoff_refusal".to_string(),
                dialog.and_then(|dialog| dialog.refusal.clone()).unwrap_or_default(),
            ),
        ]
    }

    pub(crate) fn render_handoff_dialog(&self, theme: &Theme, entity: &Entity<Self>) -> Option<AnyElement> {
        let state = self.handoff_dialog.as_ref()?;
        let running = self.handoff == HandoffState::Running;
        let can_start = self.start_blocker().is_none();
        let cross = self.header.value().and_then(|header| header.head.as_ref()).is_some_and(|head| head.cross_repository);
        let selected = state.selected_agent();

        let purposes: Vec<Choice> = Purpose::ALL
            .into_iter()
            .map(|purpose| {
                let choice = Choice::new(purpose.word(), purpose.label());
                match purpose_refusal(&state.scope, state.ci_failed, purpose) {
                    Some(note) => choice.disabled().note(note),
                    None => choice,
                }
            })
            .collect();
        let mut agents: Vec<Choice> = state.agents.iter().map(agent_choice).collect();
        agents.push(Choice::new("none", "No agent"));
        let surfaces: Vec<Choice> = [Surface::Terminal, Surface::Chat]
            .into_iter()
            .map(|surface| {
                let choice = Choice::new(surface.word(), surface.label());
                match selected {
                    _ if state.agent.is_none() => choice.disabled(),
                    Some(agent) if surface == Surface::Chat && !agent.chat => {
                        choice.disabled().note(format!("no chat for {}", agent.name))
                    }
                    _ => choice,
                }
            })
            .collect();

        let group = |id: &'static str, field_name: &'static str, choices: &[Choice], selected: &str, horizontal: bool| {
            let notes: Vec<String> = choices
                .iter()
                .filter_map(|choice| choice.note.as_ref().map(|note| format!("{}: {note}", choice.label)))
                .collect();
            let entity = entity.clone();
            let mut radios = RadioGroup::new(id, choices.to_vec())
                .selected(selected.to_string())
                .on_change(move |value, _, cx| {
                    let value = value.to_string();
                    entity.update(cx, |tab, cx| {
                        let _ = tab.set_handoff_field(field_name, &value, cx);
                    });
                });
            if horizontal {
                radios = radios.horizontal();
            }
            (radios, notes)
        };
        let (purpose_group, purpose_notes) =
            group("change-request-handoff-purpose", "purpose", &purposes, state.purpose.word(), false);
        let (agent_group, agent_notes) =
            group("change-request-handoff-agent", "agent", &agents, state.agent.unwrap_or("none"), false);
        let (surface_group, surface_notes) =
            group("change-request-handoff-surface", "surface", &surfaces, state.surface.word(), true);

        let worktree = match &state.preview {
            HandoffPreview::Loading => message(Severity::Info, "Worktree: working it out…".to_string(), theme),
            HandoffPreview::Ready { worktree: Ok(line), .. } => {
                message(Severity::Info, format!("Worktree: {line}"), theme)
            }
            HandoffPreview::Ready { worktree: Err(reason), .. } => {
                message(Severity::Danger, format!("Worktree: {reason}"), theme)
            }
        };
        let close = entity.clone();
        let start = entity.clone();
        let dialog = Dialog::new("change-request-handoff-dialog", "Hand off to an agent", move |_, cx| {
            close.update(cx, |tab, cx| tab.close_handoff(cx))
        })
        .child(field("change-request-handoff-purpose-field", "Purpose", purpose_group, purpose_notes, theme))
        .child(field("change-request-handoff-agent-field", "Agent", agent_group, agent_notes, theme))
        .child(field("change-request-handoff-surface-field", "Surface", surface_group, surface_notes, theme))
        .child(Input::new(&state.instructions))
        .child(worktree)
        .children(state.warns(cross).then(|| {
            message(
                Severity::Warning,
                "This change request's text reaches the agent as data, and the agent runs with your permissions.".to_string(),
                theme,
            )
        }))
        .children(state.refusal.clone().map(|reason| message(Severity::Danger, reason, theme)))
        .action(|close| {
            text_button("change-request-handoff-cancel", "Cancel", None, ButtonState::IDLE, move |window, cx| {
                close(window, cx)
            })
        })
        .action(move |_| {
            text_button(
                "change-request-handoff-start",
                "Start",
                None,
                ButtonState::enabled(can_start).loading(running).primary(),
                move |_, cx| {
                    start.update(cx, |tab, cx| {
                        let _ = tab.start_handoff(cx);
                    })
                },
            )
        });
        Some(div().debug_selector(|| "change-request-handoff-dialog-frame".to_owned()).child(dialog).into_any_element())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_purpose_follows_ci_then_authorship() {
        assert_eq!(default_purpose(true, Some(true)), Purpose::Ci);
        assert_eq!(default_purpose(false, Some(false)), Purpose::Review);
        assert_eq!(default_purpose(false, Some(true)), Purpose::Resume);
        assert_eq!(default_purpose(false, None), Purpose::Resume, "unknown authorship is not a stranger");
    }
}
