//! The non-modal update card (spec 2026-10-14-update-dialog-design §2–§4).
//!
//! [`update_card_view`] decides everything the card shows from the host's
//! facts; [`UpdateCard`] draws the result. The card holds no state of its own:
//! the host owns dismissal, the cross, the agent count and the button routing.

use std::rc::Rc;

use ely_gpui_component::primitives::{IconName, Severity};
use gpui::{App, IntoElement, RenderOnce, Window, div, prelude::*, px};
use sirio_theme::Theme;

use crate::ely_ui::{ButtonState, icon_button, message, text_button};
use crate::status_bar::{UpdateState, UpdateStatus};
use crate::text_selection::selectable_text;

/// What a button on the card asks the host to do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UpdateCardAction {
    /// Put the card away for this version, in memory only.
    Later,
    /// Apply and restart (or restart into an installed update).
    Restart,
    /// Install now, restart later.
    InstallOnly,
    /// Try an apply that failed again.
    Retry,
    /// Hide the card while work continues behind it.
    Hide,
}

/// The facts the host supplies. Borrowed, so the host keeps its own state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UpdateCardInput<'a> {
    pub state: &'a UpdateState,
    /// The version the user put off with *Later*, if any.
    pub dismissed: Option<&'a str>,
    /// The user hid the card with its cross.
    pub hidden: bool,
    /// Agents that a restart would stop.
    pub agents_working: usize,
    /// Whether the platform's installer restarts Sirio by itself (Windows).
    pub installer_restarts: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UpdateCardButton {
    pub action: UpdateCardAction,
    pub label: &'static str,
    pub enabled: bool,
    pub primary: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UpdateCardView {
    pub title: String,
    pub body: Option<String>,
    pub warning: Option<String>,
    pub progress: Option<String>,
    pub error: Option<String>,
    /// Whether the card has a cross: only while work continues behind it.
    pub hideable: bool,
    pub buttons: Vec<UpdateCardButton>,
}

const NOTES_CAP: usize = 4096;

impl UpdateCardAction {
    /// The name the debug socket uses.
    pub fn name(self) -> &'static str {
        match self {
            Self::Later => "later",
            Self::Restart => "restart",
            Self::InstallOnly => "install",
            Self::Retry => "retry",
            Self::Hide => "hide",
        }
    }
}

/// Everything the card shows, decided from the host's facts (spec §2). `None`
/// is "no card".
pub fn update_card_view(input: &UpdateCardInput<'_>) -> Option<UpdateCardView> {
    if !input.state.enabled || input.hidden {
        return None;
    }
    let status = &input.state.status;
    let version = status.version()?;
    if input.dismissed == Some(version) {
        return None;
    }
    let restart_label = if input.installer_restarts {
        "Update and restart"
    } else {
        "Restart to update"
    };
    let warning = (input.agents_working > 0).then(|| agents_warning(input.agents_working));
    let button = |action, label, enabled, primary| UpdateCardButton {
        action,
        label,
        enabled,
        primary,
    };
    let view = match status {
        UpdateStatus::Available { notes, .. } | UpdateStatus::Ready { notes, .. } => {
            let mut buttons = vec![button(UpdateCardAction::Later, "Later", true, false)];
            if warning.is_some() && !input.installer_restarts {
                buttons.push(button(
                    UpdateCardAction::InstallOnly,
                    "Install without restarting",
                    true,
                    false,
                ));
            }
            buttons.push(button(
                UpdateCardAction::Restart,
                restart_label,
                true,
                true,
            ));
            let here = if matches!(status, UpdateStatus::Ready { .. }) {
                "ready"
            } else {
                "available"
            };
            UpdateCardView {
                title: format!("Sirio {version} is {here}"),
                body: body(input.state, notes),
                warning,
                progress: None,
                error: None,
                hideable: false,
                buttons,
            }
        }
        UpdateStatus::Downloading { .. } | UpdateStatus::Installing { .. } => UpdateCardView {
            title: format!("Updating to Sirio {version}"),
            body: None,
            warning: None,
            progress: Some(
                if matches!(status, UpdateStatus::Downloading { .. }) {
                    format!("Downloading Sirio {version}…")
                } else {
                    "Installing…".to_string()
                },
            ),
            error: None,
            hideable: true,
            buttons: vec![
                button(UpdateCardAction::Later, "Later", false, false),
                button(UpdateCardAction::Restart, restart_label, false, true),
            ],
        },
        UpdateStatus::Installed { .. } if input.installer_restarts => UpdateCardView {
            title: format!("Sirio {version} is installed"),
            body: None,
            warning: None,
            progress: Some("Sirio restarts to finish the update…".to_string()),
            error: None,
            hideable: true,
            buttons: Vec::new(),
        },
        UpdateStatus::Installed { .. } => UpdateCardView {
            title: format!("Sirio {version} is installed — restart to use it"),
            body: None,
            warning,
            progress: None,
            error: None,
            hideable: false,
            buttons: vec![
                button(UpdateCardAction::Later, "Later", true, false),
                button(UpdateCardAction::Restart, "Restart now", true, true),
            ],
        },
        UpdateStatus::ApplyFailed {
            message, retryable, ..
        } => {
            let mut buttons = vec![button(UpdateCardAction::Later, "Later", true, false)];
            if *retryable {
                buttons.push(button(UpdateCardAction::Retry, "Retry", true, true));
            }
            UpdateCardView {
                title: "Update failed".to_string(),
                body: None,
                warning: None,
                progress: None,
                error: Some(message.clone()),
                hideable: false,
                buttons,
            }
        }
        UpdateStatus::Disabled
        | UpdateStatus::NotDue
        | UpdateStatus::Checking
        | UpdateStatus::UpToDate
        | UpdateStatus::Failed { .. } => return None,
    };
    Some(view)
}

fn agents_warning(count: usize) -> String {
    if count == 1 {
        "1 agent is working — restarting will stop it.".to_string()
    } else {
        format!("{count} agents are working — restarting will stop them.")
    }
}

/// Nightly says what it is, as Settings does; Stable shows the notes as plain
/// text with control characters dropped and the length capped (spec §4).
fn body(state: &UpdateState, notes: &str) -> Option<String> {
    if state.channel.trim().eq_ignore_ascii_case("nightly") {
        return Some("Nightly builds track main".to_string());
    }
    let clean: String = notes
        .chars()
        .filter(|c| *c == '\n' || !c.is_control())
        .collect();
    let clean = clean.trim();
    if clean.is_empty() {
        return None;
    }
    if clean.len() <= NOTES_CAP {
        return Some(clean.to_string());
    }
    let mut end = NOTES_CAP;
    while !clean.is_char_boundary(end) {
        end -= 1;
    }
    Some(format!("{}…", &clean[..end]))
}

/// Draws an [`UpdateCardView`] docked bottom-right above the status bar. It
/// holds no state, takes no focus and draws no scrim (spec §0, decision 3):
/// the user keeps typing wherever they were.
#[derive(IntoElement)]
pub struct UpdateCard {
    view: UpdateCardView,
    on_action: Rc<dyn Fn(UpdateCardAction, &mut Window, &mut App)>,
}

impl UpdateCard {
    pub fn new(
        view: UpdateCardView,
        on_action: impl Fn(UpdateCardAction, &mut Window, &mut App) + 'static,
    ) -> Self {
        Self {
            view,
            on_action: Rc::new(on_action),
        }
    }
}

fn button_id(action: UpdateCardAction) -> &'static str {
    match action {
        UpdateCardAction::Later => "update-card-later",
        UpdateCardAction::Restart => "update-card-restart",
        UpdateCardAction::InstallOnly => "update-card-install-only",
        UpdateCardAction::Retry => "update-card-retry",
        UpdateCardAction::Hide => "update-card-hide",
    }
}

impl RenderOnce for UpdateCard {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = *Theme::get(cx);
        let view = self.view;
        let on_action = self.on_action;
        let header = div()
            .flex()
            .items_start()
            .justify_between()
            .gap(px(8.0))
            .child(
                div()
                    .id("update-card-title")
                    .debug_selector(|| "update-card-title".to_owned())
                    .min_w_0()
                    .text_size(theme.typography.callout)
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(theme.ely.fg)
                    .child(selectable_text(view.title.clone())),
            )
            .when(view.hideable, |header| {
                let on_action = on_action.clone();
                header.child(icon_button(
                    "update-card-hide",
                    IconName::X,
                    "Hide",
                    true,
                    move |window, cx| on_action(UpdateCardAction::Hide, window, cx),
                ))
            });
        let buttons = view.buttons.iter().map(|button| {
            let on_action = on_action.clone();
            let action = button.action;
            let state = ButtonState::enabled(button.enabled);
            let state = if button.primary {
                state.primary()
            } else {
                state
            };
            text_button(
                button_id(action),
                button.label,
                None,
                state,
                move |window, cx| on_action(action, window, cx),
            )
        });
        div()
            .id("update-card")
            .debug_selector(|| "update-card".to_owned())
            .occlude()
            .absolute()
            .bottom(px(44.0))
            .right(px(16.0))
            .w(px(360.0))
            .flex()
            .flex_col()
            .gap(px(10.0))
            .p(px(14.0))
            .rounded(theme.radii.control)
            .border_1()
            .border_color(theme.ely.border)
            .bg(theme.sirio.floating_surface)
            .shadow_lg()
            .child(header)
            .when_some(view.body.clone(), |card, body| {
                card.child(
                    div()
                        .id("update-card-body")
                        .debug_selector(|| "update-card-body".to_owned())
                        .max_h(px(180.0))
                        .overflow_y_scroll()
                        .text_size(theme.typography.footnote)
                        .text_color(theme.ely.fg_muted)
                        .child(selectable_text(body)),
                )
            })
            .when_some(view.warning.clone(), |card, warning| {
                card.child(
                    div()
                        .debug_selector(|| "update-card-warning".to_owned())
                        .child(message(Severity::Warning, warning, &theme)),
                )
            })
            .when_some(view.error.clone(), |card, error| {
                card.child(
                    div()
                        .debug_selector(|| "update-card-error".to_owned())
                        .child(message(Severity::Danger, error, &theme)),
                )
            })
            .when_some(view.progress.clone(), |card, progress| {
                card.child(
                    div()
                        .debug_selector(|| "update-card-progress".to_owned())
                        .flex()
                        .flex_col()
                        .gap(px(6.0))
                        .text_size(theme.typography.footnote)
                        .text_color(theme.ely.fg_muted)
                        .child(selectable_text(progress))
                        .child(
                            div()
                                .h(px(3.0))
                                .w_full()
                                .rounded_full()
                                .bg(theme.ely.surface)
                                .child(
                                    div()
                                        .h_full()
                                        .w(px(120.0))
                                        .rounded_full()
                                        .bg(theme.ely.fg_muted),
                                ),
                        ),
                )
            })
            .when(!view.buttons.is_empty(), |card| {
                card.child(div().flex().justify_end().gap(px(8.0)).children(buttons))
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use UpdateCardAction::*;

    fn state(status: UpdateStatus) -> UpdateState {
        UpdateState { enabled: true, channel: "stable".into(), status, last_checked: None }
    }
    fn ready(version: &str) -> UpdateStatus {
        UpdateStatus::Ready { version: version.into(), notes: "Fixes.".into() }
    }
    fn input(state: &UpdateState) -> UpdateCardInput<'_> {
        UpdateCardInput { state, dismissed: None, hidden: false, agents_working: 0, installer_restarts: false }
    }
    fn actions(view: &UpdateCardView) -> Vec<(UpdateCardAction, bool)> {
        view.buttons.iter().map(|button| (button.action, button.enabled)).collect()
    }

    #[test]
    fn hidden_when_updates_are_off() {
        let mut off = state(ready("1.0.0"));
        off.enabled = false;
        assert_eq!(update_card_view(&input(&off)), None);
    }

    #[test]
    fn hidden_for_states_that_offer_nothing() {
        for status in [
            UpdateStatus::Disabled,
            UpdateStatus::NotDue,
            UpdateStatus::Checking,
            UpdateStatus::UpToDate,
            UpdateStatus::Failed { message: "network".into() },
        ] {
            let state = state(status.clone());
            assert_eq!(update_card_view(&input(&state)), None, "{status:?} raised the card");
        }
    }

    #[test]
    fn later_hides_only_the_version_put_off() {
        let same = state(ready("1.0.0"));
        let mut put_off = input(&same);
        put_off.dismissed = Some("1.0.0");
        assert_eq!(update_card_view(&put_off), None);

        let newer = state(ready("1.1.0"));
        let mut put_off = input(&newer);
        put_off.dismissed = Some("1.0.0");
        assert!(update_card_view(&put_off).is_some());
    }

    #[test]
    fn the_cross_hides_the_card_during_work_and_only_then_is_offered() {
        let working = state(UpdateStatus::Installing { version: "1.0.0".into() });
        assert!(update_card_view(&input(&working)).unwrap().hideable);
        let mut hidden = input(&working);
        hidden.hidden = true;
        assert_eq!(update_card_view(&hidden), None);

        let idle = state(ready("1.0.0"));
        assert!(!update_card_view(&input(&idle)).unwrap().hideable);
    }

    #[test]
    fn titles_say_whether_the_update_is_already_here() {
        let r = state(ready("1.0.0"));
        assert_eq!(update_card_view(&input(&r)).unwrap().title, "Sirio 1.0.0 is ready");
        let a = state(UpdateStatus::Available { version: "1.0.0".into(), notes: String::new() });
        assert_eq!(update_card_view(&input(&a)).unwrap().title, "Sirio 1.0.0 is available");
    }

    #[test]
    fn the_primary_action_says_what_the_platform_does() {
        let r = state(ready("1.0.0"));
        let here = update_card_view(&input(&r)).unwrap();
        assert_eq!(actions(&here), vec![(Later, true), (Restart, true)]);
        assert_eq!(here.buttons[1].label, "Restart to update");
        assert!(here.buttons[1].primary && !here.buttons[0].primary);

        let mut windows = input(&r);
        windows.installer_restarts = true;
        assert_eq!(update_card_view(&windows).unwrap().buttons[1].label, "Update and restart");
    }

    #[test]
    fn install_without_restarting_needs_an_agent_at_work_where_sirio_restarts_itself() {
        let r = state(ready("1.0.0"));
        let mut busy = input(&r);
        busy.agents_working = 2;
        assert_eq!(actions(&update_card_view(&busy).unwrap()), vec![(Later, true), (InstallOnly, true), (Restart, true)]);

        busy.installer_restarts = true;
        assert_eq!(actions(&update_card_view(&busy).unwrap()), vec![(Later, true), (Restart, true)]);
    }

    #[test]
    fn the_warning_counts_agents_in_words() {
        let r = state(ready("1.0.0"));
        let mut i = input(&r);
        assert_eq!(update_card_view(&i).unwrap().warning, None);
        i.agents_working = 1;
        assert_eq!(
            update_card_view(&i).unwrap().warning.as_deref(),
            Some("1 agent is working — restarting will stop it.")
        );
        i.agents_working = 3;
        assert_eq!(
            update_card_view(&i).unwrap().warning.as_deref(),
            Some("3 agents are working — restarting will stop them.")
        );
    }

    #[test]
    fn work_in_flight_disables_every_action() {
        for (status, line) in [
            (UpdateStatus::Downloading { version: "1.0.0".into() }, "Downloading Sirio 1.0.0…"),
            (UpdateStatus::Installing { version: "1.0.0".into() }, "Installing…"),
        ] {
            let s = state(status);
            let mut i = input(&s);
            i.agents_working = 2;
            let view = update_card_view(&i).unwrap();
            assert!(view.buttons.iter().all(|button| !button.enabled), "{view:?}");
            assert_eq!(view.progress.as_deref(), Some(line));
            assert_eq!(view.title, "Updating to Sirio 1.0.0");
        }
    }

    #[test]
    fn retry_only_where_trying_again_can_help() {
        let retryable = state(UpdateStatus::ApplyFailed {
            version: "1.0.0".into(),
            message: "network unreachable".into(),
            retryable: true,
        });
        let view = update_card_view(&input(&retryable)).unwrap();
        assert_eq!(view.title, "Update failed");
        assert_eq!(view.error.as_deref(), Some("network unreachable"));
        assert_eq!(actions(&view), vec![(Later, true), (Retry, true)]);

        let refused = state(UpdateStatus::ApplyFailed {
            version: "1.0.0".into(),
            message: "not an install".into(),
            retryable: false,
        });
        assert_eq!(actions(&update_card_view(&input(&refused)).unwrap()), vec![(Later, true)]);
    }

    #[test]
    fn installed_offers_restart_now_where_sirio_restarts_itself() {
        let s = state(UpdateStatus::Installed { version: "1.0.0".into() });
        let view = update_card_view(&input(&s)).unwrap();
        assert_eq!(view.title, "Sirio 1.0.0 is installed — restart to use it");
        assert_eq!(actions(&view), vec![(Later, true), (Restart, true)]);
        assert_eq!(view.buttons[1].label, "Restart now");

        let mut windows = input(&s);
        windows.installer_restarts = true;
        let view = update_card_view(&windows).unwrap();
        assert_eq!(view.title, "Sirio 1.0.0 is installed");
        assert!(view.buttons.is_empty());
        assert_eq!(view.progress.as_deref(), Some("Sirio restarts to finish the update…"));
    }

    #[test]
    fn the_body_is_the_channel_on_nightly_and_clean_notes_on_stable() {
        let mut nightly = state(ready("1.0.0"));
        nightly.channel = "nightly".into();
        assert_eq!(update_card_view(&input(&nightly)).unwrap().body.as_deref(), Some("Nightly builds track main"));

        let dirty = state(UpdateStatus::Ready {
            version: "1.0.0".into(),
            notes: format!("Line one\n\u{1b}[31mred\u{7}\n{}", "x".repeat(10_000)),
        });
        let body = update_card_view(&input(&dirty)).unwrap().body.unwrap();
        assert!(body.starts_with("Line one\n[31mred\n"), "{body:?}");
        assert!(!body.chars().any(|c| c != '\n' && c.is_control()));
        assert!(body.len() <= 4096 + '…'.len_utf8());
        assert!(body.ends_with('…'));

        let empty = state(UpdateStatus::Ready { version: "1.0.0".into(), notes: "  \n".into() });
        assert_eq!(update_card_view(&input(&empty)).unwrap().body, None);
    }
}
