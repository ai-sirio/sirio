//! The non-modal update card (spec 2026-10-14-update-dialog-design §2–§4).
//!
//! [`update_card_view`] decides everything the card shows from the host's
//! facts; [`UpdateCard`] draws the result. The card holds no state of its own:
//! the host owns dismissal, the cross, the agent count and the button routing.

use std::rc::Rc;

use ely_gpui_component::primitives::{IconName, Severity};
use gpui::{
    Animation, AnimationExt, App, IntoElement, RenderOnce, Window, div, prelude::*, px, relative,
};
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
    /// Whether a restart follows the apply in flight (*Restart to update*, not
    /// *Install without restarting*).
    pub restart_after: bool,
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

/// The card is this wide. The three-button row (Later, Install without
/// restarting, Restart to update) is about 416 px at the default font, measured
/// from the update-ui capture, and the content box is the card less 30 px of
/// padding and border, so 420 px would still wrap it.
const CARD_WIDTH: f32 = 450.0;
/// The indeterminate segment that slides along the progress track.
const PROGRESS_SEGMENT: f32 = 120.0;
/// Where the segment's left edge sweeps, as a fraction of the track: from
/// before the track to its right end, so the segment is off the track at both
/// ends of every sweep and the track's clip hides it in between.
const PROGRESS_FROM: f32 = -0.4;
const PROGRESS_TO: f32 = 1.0;
const PROGRESS_PERIOD: std::time::Duration = std::time::Duration::from_millis(1500);
const PROGRESS_MAX_FPS: f32 = 30.0;

/// Each action's socket name, and from it the element id of its button: one
/// table both come from, so the socket's `act <name>` and the card's
/// `update-card-action-<name>` selectors (spec §3.1, §3.6) cannot drift apart.
macro_rules! update_card_actions {
    ($($variant:ident => $name:literal),+ $(,)?) => {
        impl UpdateCardAction {
            /// The name the debug socket uses.
            pub fn name(self) -> &'static str {
                match self {
                    $(Self::$variant => $name,)+
                }
            }

            /// The element id, and debug selector, of this action's button.
            fn element_id(self) -> &'static str {
                match self {
                    $(Self::$variant => concat!("update-card-action-", $name),)+
                }
            }
        }
    };
}

update_card_actions! {
    Later => "later",
    Restart => "restart",
    InstallOnly => "install",
    Retry => "retry",
    Hide => "hide",
}

/// Everything the card shows, decided from the host's facts (spec §2). `None`
/// is "no card".
pub fn update_card_view(input: &UpdateCardInput<'_>) -> Option<UpdateCardView> {
    if !input.state.enabled {
        return None;
    }
    let status = &input.state.status;
    let version = status.version()?;
    if input.dismissed == Some(version) {
        return None;
    }
    // The cross puts the card away only while work goes on behind it (§2.1):
    // an apply in flight, or on Windows the installer closing Sirio after
    // `Installed` (the user waits for it, so the cross must still work there).
    // Once that work is over the outcome is shown whatever the cross said.
    let in_flight = matches!(
        status,
        UpdateStatus::Downloading { .. } | UpdateStatus::Installing { .. }
    );
    let installer_closing = input.installer_restarts
        && matches!(status, UpdateStatus::Installed { .. });
    if input.hidden && (in_flight || installer_closing) {
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
            // Work in flight keeps the warning (§2.2) where a restart follows
            // it. After *Install without restarting* none does, so no agent is
            // stopped by it. Windows' installer closes Sirio on any apply, so
            // there the warning stays whichever button started it.
            warning: if input.restart_after || input.installer_restarts {
                warning
            } else {
                None
            },
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
            // The warning has no exemption (§2.2): the installer still stops
            // the agents as it closes Sirio.
            warning,
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
                // No exemption (§2.2): a retry restarts Sirio, which stops them.
                warning,
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
/// text, tabs made spaces, other control characters dropped and the length
/// capped (spec §4).
fn body(state: &UpdateState, notes: &str) -> Option<String> {
    if state.channel.trim().eq_ignore_ascii_case("nightly") {
        return Some("Nightly builds track main".to_string());
    }
    // A tab keeps its word apart from the next one; any other control
    // character is dropped, so no escape sequence reaches the card.
    let clean: String = notes
        .chars()
        .filter_map(|c| match c {
            '\n' => Some('\n'),
            '\t' => Some(' '),
            c if c.is_control() => None,
            c => Some(c),
        })
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

/// Where the progress segment's left edge is, as a fraction of the track, at
/// `t` (0 to 1) through one sweep.
fn segment_left(t: f32) -> f32 {
    PROGRESS_FROM + (PROGRESS_TO - PROGRESS_FROM) * t.clamp(0.0, 1.0)
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
                    UpdateCardAction::Hide.element_id(),
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
                action.element_id(),
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
            .w(px(CARD_WIDTH))
            .flex()
            .flex_col()
            .gap(px(10.0))
            .p(px(14.0))
            .rounded(theme.radii.control)
            .border_1()
            .border_color(theme.ely.border)
            .bg(theme.sirio.floating_surface)
            // The shadow Sirio's other floating surfaces draw (outline,
            // history toolbar, settings). Ely's own Floating layers are too faint
            // on the light window for the card's edge to show.
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
                        .child(message(Severity::Warning, warning, "update-card-warning", &theme)),
                )
            })
            .when_some(view.error.clone(), |card, error| {
                card.child(
                    div()
                        .debug_selector(|| "update-card-error".to_owned())
                        .child(message(Severity::Danger, error, "update-card-error", &theme)),
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
                                .overflow_hidden()
                                .child(
                                    div()
                                        .id("update-card-progress-segment")
                                        .absolute()
                                        .h_full()
                                        .w(px(PROGRESS_SEGMENT))
                                        .rounded_full()
                                        .bg(theme.ely.fg_muted)
                                        .with_animation(
                                            "update-card-progress-segment",
                                            Animation::new(PROGRESS_PERIOD)
                                                .repeat()
                                                .with_max_fps(PROGRESS_MAX_FPS),
                                            |segment, t| segment.left(relative(segment_left(t))),
                                        ),
                                ),
                        ),
                )
            })
            .when(!view.buttons.is_empty(), |card| {
                // A row that is too wide wraps inside the card, not past its edge.
                card.child(
                    div()
                        .flex()
                        .flex_wrap()
                        .justify_end()
                        .gap(px(8.0))
                        .children(buttons),
                )
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use UpdateCardAction::*;
    use gpui::{Context, Render, TestAppContext, VisualTestContext, Window, div};

    /// The progress track: the card less its 14 px padding and the 1 px
    /// border on each side. The sweep must clear it at this width.
    const TRACK_WIDTH: f32 = CARD_WIDTH - 2.0 * (14.0 + 1.0);

    fn state(status: UpdateStatus) -> UpdateState {
        UpdateState { enabled: true, channel: "stable".into(), status, last_checked: None }
    }
    fn ready(version: &str) -> UpdateStatus {
        UpdateStatus::Ready { version: version.into(), notes: "Fixes.".into() }
    }
    fn input(state: &UpdateState) -> UpdateCardInput<'_> {
        UpdateCardInput {
            state,
            dismissed: None,
            hidden: false,
            agents_working: 0,
            installer_restarts: false,
            restart_after: true,
        }
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
        for status in [
            UpdateStatus::Downloading { version: "1.0.0".into() },
            UpdateStatus::Installing { version: "1.0.0".into() },
        ] {
            let working = state(status);
            assert!(update_card_view(&input(&working)).unwrap().hideable);
            let mut hidden = input(&working);
            hidden.hidden = true;
            assert_eq!(update_card_view(&hidden), None, "{:?}", working.status);
        }

        let idle = state(ready("1.0.0"));
        assert!(!update_card_view(&input(&idle)).unwrap().hideable);
    }

    #[test]
    fn the_cross_puts_away_only_work_in_flight() {
        for status in [
            UpdateStatus::ApplyFailed {
                version: "1.0.0".into(),
                message: "network unreachable".into(),
                retryable: true,
            },
            UpdateStatus::Installed { version: "1.0.0".into() },
        ] {
            let s = state(status.clone());
            let mut hidden = input(&s);
            hidden.hidden = true;
            assert!(update_card_view(&hidden).is_some(), "{status:?} was hidden behind the cross");
        }
    }

    #[test]
    fn the_progress_segment_enters_and_leaves_the_track() {
        assert!(segment_left(0.0) * TRACK_WIDTH + PROGRESS_SEGMENT <= 0.0);
        assert!(segment_left(1.0) >= 1.0);
        let samples: Vec<f32> = (0..=10).map(|step| segment_left(step as f32 / 10.0)).collect();
        assert!(samples.windows(2).all(|pair| pair[0] < pair[1]), "{samples:?}");
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
            assert_eq!(
                view.warning.as_deref(),
                Some("2 agents are working — restarting will stop them.")
            );
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
    fn the_cross_puts_an_installed_update_away_only_where_the_installer_restarts() {
        let s = state(UpdateStatus::Installed { version: "1.0.0".into() });
        let mut hidden = input(&s);
        hidden.hidden = true;
        hidden.installer_restarts = true;
        assert_eq!(
            update_card_view(&hidden),
            None,
            "Windows: the installer closes Sirio behind the cross, and the cross must still work"
        );

        hidden.installer_restarts = false;
        assert!(
            update_card_view(&hidden).is_some(),
            "macOS and Linux: an installed update waits for the user to restart"
        );
    }

    #[test]
    fn the_agents_warning_is_drawn_in_every_status_the_card_shows() {
        for (status, installer_restarts) in [
            (
                UpdateStatus::ApplyFailed {
                    version: "1.0.0".into(),
                    message: "network unreachable".into(),
                    retryable: true,
                },
                false,
            ),
            (
                UpdateStatus::ApplyFailed {
                    version: "1.0.0".into(),
                    message: "not an install".into(),
                    retryable: false,
                },
                false,
            ),
            (UpdateStatus::Installed { version: "1.0.0".into() }, true),
        ] {
            let s = state(status.clone());
            let mut i = input(&s);
            i.agents_working = 1;
            i.installer_restarts = installer_restarts;
            let view = update_card_view(&i).unwrap();
            assert_eq!(
                view.warning.as_deref(),
                Some("1 agent is working — restarting will stop it."),
                "{status:?} (installer_restarts: {installer_restarts}) drew no warning"
            );
        }
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

        let tabbed = state(UpdateStatus::Ready {
            version: "1.0.0".into(),
            notes: "fix\tthe\u{7} bug".into(),
        });
        assert_eq!(
            update_card_view(&input(&tabbed)).unwrap().body.as_deref(),
            Some("fix the bug")
        );
    }

    /// The card alone, docked the way the app docks it.
    struct CardHarness {
        view: UpdateCardView,
    }

    impl Render for CardHarness {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div().size_full().child(UpdateCard::new(self.view.clone(), |_, _, _| {}))
        }
    }

    fn drawn(cx: &mut TestAppContext, view: UpdateCardView) -> VisualTestContext {
        cx.update(Theme::init);
        cx.update(crate::ely::init);
        let window = cx.add_window(move |_, _| CardHarness { view });
        let cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();
        cx
    }

    /// Every button of the view is drawn inside the card's bounds. A row that
    /// overflows the card's edge is what this catches; whether the row fits on
    /// one line depends on the font, which this harness does not measure, so
    /// the captures carry that proof.
    fn assert_buttons_inside_the_card(view: &UpdateCardView, cx: &mut VisualTestContext) {
        let card = cx.debug_bounds("update-card").expect("the card is drawn");
        for button in &view.buttons {
            let id = button.action.element_id();
            let bounds = cx.debug_bounds(id).unwrap_or_else(|| panic!("{id} is drawn"));
            assert!(
                bounds.left() >= card.left()
                    && bounds.right() <= card.right()
                    && bounds.top() >= card.top()
                    && bounds.bottom() <= card.bottom(),
                "{id} at {bounds:?} lies outside the card at {card:?}"
            );
        }
    }

    #[gpui::test]
    async fn the_three_button_row_fits_inside_the_card(cx: &mut TestAppContext) {
        let s = state(ready("1.0.0"));
        let mut i = input(&s);
        i.agents_working = 1;
        let view = update_card_view(&i).unwrap();
        assert_eq!(view.buttons.len(), 3, "later, install without restarting, restart");
        let mut cx = drawn(cx, view.clone());
        assert_buttons_inside_the_card(&view, &mut cx);
    }

    #[gpui::test]
    async fn the_windows_two_button_row_fits_inside_the_card(cx: &mut TestAppContext) {
        let s = state(ready("1.0.0"));
        let mut i = input(&s);
        i.agents_working = 1;
        i.installer_restarts = true;
        let view = update_card_view(&i).unwrap();
        assert_eq!(view.buttons.len(), 2, "later, update and restart");
        let mut cx = drawn(cx, view.clone());
        assert_buttons_inside_the_card(&view, &mut cx);
    }

    #[gpui::test]
    async fn the_retry_row_fits_inside_the_card(cx: &mut TestAppContext) {
        let s = state(UpdateStatus::ApplyFailed {
            version: "1.0.0".into(),
            message: "network unreachable".into(),
            retryable: true,
        });
        let mut i = input(&s);
        i.agents_working = 1;
        let view = update_card_view(&i).unwrap();
        assert_eq!(view.buttons.len(), 2, "later, retry");
        let mut cx = drawn(cx, view.clone());
        assert_buttons_inside_the_card(&view, &mut cx);
    }

    #[test]
    fn an_install_without_a_restart_draws_no_agents_warning_while_it_works() {
        for status in [
            UpdateStatus::Downloading { version: "1.0.0".into() },
            UpdateStatus::Installing { version: "1.0.0".into() },
        ] {
            let s = state(status.clone());
            let mut i = input(&s);
            i.agents_working = 2;
            i.restart_after = false;
            assert_eq!(update_card_view(&i).unwrap().warning, None, "{status:?}");
        }
    }

    #[test]
    fn the_warning_stays_where_a_restart_follows_or_the_installer_restarts() {
        let s = state(UpdateStatus::Downloading { version: "1.0.0".into() });
        let mut restarting = input(&s);
        restarting.agents_working = 2;
        assert!(update_card_view(&restarting).unwrap().warning.is_some());

        // Windows: Inno closes Sirio on any apply, Settings' Stay included, so
        // a restart comes whichever button started the apply.
        let mut windows_stay = restarting.clone();
        windows_stay.restart_after = false;
        windows_stay.installer_restarts = true;
        assert!(update_card_view(&windows_stay).unwrap().warning.is_some());
    }

    #[test]
    fn restart_after_changes_nothing_outside_the_work_in_flight() {
        let s = state(ready("1.0.0"));
        let mut i = input(&s);
        i.agents_working = 1;
        i.restart_after = false;
        assert!(update_card_view(&i).unwrap().warning.is_some());
    }
}
