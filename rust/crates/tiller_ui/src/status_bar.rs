//! The fixed-height usage/status strip below the workspace columns.
//!
//! The usage segment is real data: the bar owns a Claude
//! usage fetch that runs on the background executor (never the render
//! thread), refreshes on a fixed interval, and can be triggered manually
//! from the refresh button. A provider that is missing, unreadable or
//! malformed renders its specific unavailable reason; a timed-out refresh
//! keeps the last good numbers visibly dimmed rather than showing them as
//! current.

use gpui::{Context, Render, Rgba, Window, div, prelude::*, px, text};
use std::rc::Rc;
use std::time::Duration;
use tiller_theme::Theme;
use tiller_usage::{
    ClaudeUsageFetcher, CodexUsageFetcher, OpenCodeGoUsageFetcher, ProviderUsageState,
    UsageFetchOutcome, UsageReason, reduce,
};

use crate::sidebar::icons::{Icon, IconElement};

pub(crate) const HEIGHT: f32 = 40.0;

/// The Swift default: refresh every five minutes (60..3600 allowed).
const REFRESH_INTERVAL: Duration = Duration::from_secs(300);

/// The settings the usage bar consumes from the settings surface
/// (F-SET-10): which provider segments are visible and how often the
/// fetches run. Before P58 these values changed in the settings screen and
/// nothing read them — the bar now derives its behaviour from the
/// persistence contract's snapshot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UsageBarPrefs {
    pub claude_visible: bool,
    pub codex_visible: bool,
    pub opencode_visible: bool,
    /// Refresh interval in minutes, the settings surface's unit (1..=60).
    pub refresh_interval_min: i32,
    /// OpenCode Go workspace-ID override (F-SET-12). A non-empty value is
    /// routed straight into the fetch, skipping `/_server` discovery;
    /// empty keeps discovery.
    pub opencode_workspace_id_override: String,
}

impl Default for UsageBarPrefs {
    fn default() -> Self {
        Self {
            claude_visible: true,
            codex_visible: true,
            opencode_visible: false,
            refresh_interval_min: 5,
            opencode_workspace_id_override: String::new(),
        }
    }
}

impl UsageBarPrefs {
    /// Derives the bar's preferences from the settings contract. This is
    /// the only mapping between the two surfaces — the host routes the
    /// snapshot here at construction and on every settings change.
    pub fn from_snapshot(snapshot: &crate::settings::SettingsSnapshot) -> Self {
        Self {
            claude_visible: snapshot.claude_show_in_bar,
            codex_visible: snapshot.codex_show_in_bar,
            opencode_visible: snapshot.opencode_show_in_bar,
            refresh_interval_min: snapshot.refresh_interval.clamp(1, 60),
            opencode_workspace_id_override: snapshot.opencode_workspace_id_override.clone(),
        }
    }
}

/// Plain data source for [`StatusBar`]: the worktree context shown at the
/// right edge. The usage numbers come from the real fetchers, not here.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UsageBarData {
    pub branch: String,
    pub path: String,
}

impl UsageBarData {
    /// Default context used when a host has not supplied a worktree yet.
    pub fn default_context() -> Self {
        Self {
            branch: "main".into(),
            path: "~/Desktop/Progetti/tiller".into(),
        }
    }
}

/// A self-refreshing status bar. The provider usage segments are fetched
/// from the real CLIs on the background executor, on an interval.
pub struct StatusBar {
    data: UsageBarData,
    claude: ProviderUsageState,
    codex: ProviderUsageState,
    opencode_go: ProviderUsageState,
    /// How often the usage segments re-fetch.
    refresh_interval: Duration,
    /// The settings the bar consumes: segment visibility and the refresh
    /// interval (F-SET-10). Replaced wholesale by
    /// [`StatusBar::apply_preferences`] whenever the settings surface
    /// changes.
    prefs: UsageBarPrefs,
    /// Lazily armed on first render (the constructor has no context to spawn
    /// with).
    refresh_task_started: bool,
    on_settings: Option<Rc<dyn Fn()>>,
    on_refresh: Option<Rc<dyn Fn()>>,
}

impl StatusBar {
    pub fn new(data: UsageBarData) -> Self {
        Self {
            data,
            claude: ProviderUsageState::Loading,
            codex: ProviderUsageState::Loading,
            opencode_go: ProviderUsageState::Loading,
            refresh_interval: REFRESH_INTERVAL,
            prefs: UsageBarPrefs::default(),
            refresh_task_started: false,
            on_settings: None,
            on_refresh: None,
        }
    }

    pub fn new_with_default_context() -> Self {
        Self::new(UsageBarData::default_context())
    }

    /// Sets the preferences the bar starts with, derived from the settings
    /// contract by the host.
    pub fn with_preferences(mut self, prefs: UsageBarPrefs) -> Self {
        self.prefs = prefs;
        self.refresh_interval =
            Duration::from_secs(self.prefs.refresh_interval_min.clamp(1, 60) as u64 * 60);
        self
    }

    /// Applies the settings surface's current visibility and refresh
    /// interval. The interval is read live by the fetch loop, so a change
    /// takes effect at the next cycle without re-arming the task.
    pub fn apply_preferences(&mut self, prefs: UsageBarPrefs, cx: &mut Context<Self>) {
        self.prefs = prefs;
        self.refresh_interval =
            Duration::from_secs(self.prefs.refresh_interval_min.clamp(1, 60) as u64 * 60);
        cx.notify();
    }

    pub fn on_settings(mut self, callback: impl Fn() + 'static) -> Self {
        self.on_settings = Some(Rc::new(callback));
        self
    }

    pub fn on_refresh(mut self, callback: impl Fn() + 'static) -> Self {
        self.on_refresh = Some(Rc::new(callback));
        self
    }

    /// The current Claude usage state, for tests and the host.
    pub fn claude_state(&self) -> &ProviderUsageState {
        &self.claude
    }

    /// The current Codex usage state, for tests and the host.
    pub fn codex_state(&self) -> &ProviderUsageState {
        &self.codex
    }

    /// The current OpenCode Go usage state, for tests and the host.
    pub fn opencode_go_state(&self) -> &ProviderUsageState {
        &self.opencode_go
    }

    /// Applies one round of fetch outcomes; never panics on a bad provider.
    fn apply_outcomes(
        &mut self,
        claude: UsageFetchOutcome,
        codex: UsageFetchOutcome,
        opencode_go: UsageFetchOutcome,
        cx: &mut Context<Self>,
    ) {
        self.claude = reduce(claude, &self.claude);
        self.codex = reduce(codex, &self.codex);
        self.opencode_go = reduce(opencode_go, &self.opencode_go);
        cx.notify();
    }

    /// Arms the periodic refresh task once: fetches immediately, then on
    /// the interval. The loop awaits the background fetches, so the render
    /// thread never blocks. The interval is read from the bar's current
    /// preferences on every cycle, so a settings change takes effect
    /// without re-arming the task.
    fn ensure_refresh_task(&mut self, cx: &mut Context<Self>) {
        if self.refresh_task_started {
            return;
        }
        self.refresh_task_started = true;
        cx.spawn(async move |this, cx| {
            loop {
                // The workspace override is read fresh each cycle, so a
                // settings edit reaches the next fetch without re-arming
                // the task (F-SET-12).
                let workspace_override = match this.update(cx, |bar, _| {
                    let trimmed = bar.prefs.opencode_workspace_id_override.trim();
                    (!trimmed.is_empty()).then(|| trimmed.to_string())
                }) {
                    Ok(value) => value,
                    Err(_) => return,
                };
                let executor = cx.background_executor();
                let claude = executor.spawn(async move { ClaudeUsageFetcher::fetch() });
                let codex = executor.spawn(async move { CodexUsageFetcher::fetch() });
                let opencode_go = executor.spawn(async move {
                    OpenCodeGoUsageFetcher::fetch(workspace_override.as_deref())
                });
                let (claude, codex, opencode_go) = (claude.await, codex.await, opencode_go.await);
                let interval = match this.update(cx, |bar, cx| {
                    bar.apply_outcomes(claude, codex, opencode_go, cx);
                    bar.refresh_interval
                }) {
                    Ok(interval) => interval,
                    Err(_) => return,
                };
                cx.background_executor().timer(interval).await;
            }
        })
        .detach();
    }

    /// The segment text for one provider, mirroring the Swift bar:
    /// "Claude 12% 5h · 10% wk · 0% Fable", "Codex 51% 5h",
    /// "OpenCode Go 12% 5h · 34% wk · 71% mo"; "…" while loading; "—" when
    /// unavailable.
    fn segment_text(display_name: &str, state: &ProviderUsageState) -> String {
        match state {
            ProviderUsageState::Loading => format!("{display_name} …"),
            ProviderUsageState::Loaded(usage) | ProviderUsageState::Stale(usage) => {
                let parts: Vec<String> = [
                    usage.session.as_ref(),
                    usage.weekly.as_ref(),
                    usage.monthly.as_ref(),
                    usage.fable_weekly.as_ref(),
                ]
                .into_iter()
                .flatten()
                .map(|window| format!("{}% {}", window.used_percent, window.label))
                .collect();
                if parts.is_empty() {
                    format!("{display_name} —")
                } else {
                    format!("{display_name} {}", parts.join(" · "))
                }
            }
            // F-SET-11: the reason is the one piece of data that tells the
            // four unavailable states apart; a `_` wildcard here discarded
            // it and rendered every one of them as the same bare "—". A
            // A timed-out fetch with a previous value is represented by
            // `Stale`; without a previous value it remains
            // `Unavailable(TimedOut)` so the user can distinguish it from a
            // provider error.
            ProviderUsageState::Unavailable(reason) => {
                let reason_text = match reason {
                    UsageReason::NotInstalled => "not found",
                    UsageReason::LoggedOut => "logged out",
                    UsageReason::TimedOut => "timed out",
                    UsageReason::Error => "error",
                };
                format!("{display_name} {reason_text}")
            }
        }
    }

    /// Whether the segment is dimmed: stale data and unavailability must
    /// look different from fresh numbers.
    fn segment_dimmed(state: &ProviderUsageState) -> bool {
        !matches!(state, ProviderUsageState::Loaded(_))
    }
}

impl Render for StatusBar {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *Theme::get(cx);
        self.ensure_refresh_task(cx);
        let settings = self.on_settings.clone();

        let icon_button = |id: &'static str, icon: Icon| {
            div()
                .id(id)
                .w(px(22.0))
                .h(px(22.0))
                .flex()
                .items_center()
                .justify_center()
                .rounded(theme.radii.control)
                .text_size(px(11.5))
                .text_color(theme.meta)
                .hover(|style| style.bg(theme.row_hover))
                .child(IconElement::new(icon, px(12.0)).text_color(theme.meta))
        };

        // The three provider segments, in the reference order: Claude,
        // Codex, OpenCode Go. A stale or unavailable provider reads dimmer
        // than fresh numbers — never a plausible-looking fake.
        let claude_dimmed = Self::segment_dimmed(&self.claude);
        let codex_dimmed = Self::segment_dimmed(&self.codex);
        let opencode_go_dimmed = Self::segment_dimmed(&self.opencode_go);
        let claude_color = if claude_dimmed {
            dim(theme.meta)
        } else {
            theme.meta
        };
        let codex_color = if codex_dimmed {
            dim(theme.title)
        } else {
            theme.title
        };
        let opencode_go_color = if opencode_go_dimmed {
            dim(theme.title)
        } else {
            theme.title
        };

        let provider_segment =
            |display_name: &'static str, mark: Icon, text_color: gpui::Rgba, text: String| {
                // The `text!` macro derives its element id from its own source
                // location: inside this closure the location is shared by all
                // three segments, so the ids must be explicit or the duplicate
                // element ids make GPUI drop all but one segment.
                let text_id = format!("{display_name}-usage-text");
                div()
                    .debug_selector(move || format!("{display_name}-usage-text"))
                    .flex()
                    .items_center()
                    .gap(px(5.0))
                    .text_size(theme.typography.caption2)
                    .text_color(text_color)
                    .child(IconElement::new(mark, px(12.0)))
                    .child(text!(id = text_id, text))
            };

        // The segments follow the settings surface's "Show in usage bar"
        // toggles (F-SET-10): a provider hidden there does not render here.
        let mut left = div().flex().items_center().gap(px(12.0)).child(
            icon_button("status-settings", Icon::Settings).on_click(move |_, _, _| {
                if let Some(callback) = &settings {
                    callback();
                }
            }),
        );
        if self.prefs.claude_visible {
            left = left.child(provider_segment(
                "Claude",
                Icon::ClaudeCode,
                claude_color,
                Self::segment_text("Claude", &self.claude),
            ));
        }
        if self.prefs.codex_visible {
            left = left.child(provider_segment(
                "Codex",
                Icon::Codex,
                codex_color,
                Self::segment_text("Codex", &self.codex),
            ));
        }
        if self.prefs.opencode_visible {
            left = left.child(provider_segment(
                "OpenCode Go",
                Icon::OpenCode,
                opencode_go_color,
                Self::segment_text("OpenCode Go", &self.opencode_go),
            ));
        }

        div()
            .w_full()
            .h(px(HEIGHT))
            .px(px(12.0))
            .flex()
            .items_center()
            .bg(theme.canvas)
            .text_size(theme.typography.caption2)
            .text_color(theme.meta)
            .child(left)
            .child(div().flex_1())
            .child(text!(format!("{} · {}", self.data.branch, self.data.path)))
    }
}

/// Halves the alpha of a color so stale/unavailable text reads as dimmed.
fn dim(color: Rgba) -> Rgba {
    Rgba {
        r: color.r,
        g: color.g,
        b: color.b,
        a: color.a * 0.55,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::VisualTestContext;
    use tiller_usage::{ProviderUsage, UsageWindow};

    /// P58, F-SET-10: the usage bar consumes the settings surface's
    /// visibility toggles and refresh interval. A provider hidden in
    /// settings does not render a segment, and the interval the fetch loop
    /// uses follows the setting (minutes -> seconds).
    #[gpui::test]
    async fn usage_bar_consumes_visibility_and_interval_preferences(cx: &mut gpui::TestAppContext) {
        cx.update(Theme::init);
        let window = cx.add_window(|_window, _cx| StatusBar::new_with_default_context());
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        // Defaults: Claude and Codex visible, OpenCode Go hidden.
        assert!(
            cx.debug_bounds("Claude-usage-text").is_some(),
            "a visible provider renders its segment"
        );
        assert!(cx.debug_bounds("Codex-usage-text").is_some());
        assert!(
            cx.debug_bounds("OpenCode Go-usage-text").is_none(),
            "a provider hidden in settings renders no segment"
        );

        let bar = cx.update(|window, _cx| {
            window
                .root::<StatusBar>()
                .flatten()
                .expect("status bar root")
        });
        bar.update(&mut cx, |bar, cx| {
            bar.apply_preferences(
                UsageBarPrefs {
                    claude_visible: false,
                    codex_visible: true,
                    opencode_visible: true,
                    refresh_interval_min: 7,
                    ..Default::default()
                },
                cx,
            );
        });
        cx.run_until_parked();

        assert!(
            cx.debug_bounds("Claude-usage-text").is_none(),
            "hiding a provider in settings removes its segment"
        );
        assert!(cx.debug_bounds("Codex-usage-text").is_some());
        assert!(
            cx.debug_bounds("OpenCode Go-usage-text").is_some(),
            "showing a provider in settings adds its segment"
        );

        let interval = cx.update(|window, cx| {
            window
                .root::<StatusBar>()
                .flatten()
                .expect("status bar root")
                .read(cx)
                .refresh_interval
        });
        assert_eq!(
            interval,
            Duration::from_secs(7 * 60),
            "the fetch interval follows the settings stepper (minutes)"
        );
    }

    /// P58, F-SET-10: `UsageBarPrefs::from_snapshot` is the single mapping
    /// from the persistence contract to the bar; it clamps the interval
    /// into the stepper's range so a stored out-of-range value cannot arm
    /// a pathological timer.
    #[test]
    fn prefs_derive_from_the_settings_snapshot_and_clamp() {
        let snapshot = crate::settings::SettingsSnapshot {
            claude_show_in_bar: false,
            codex_show_in_bar: true,
            opencode_show_in_bar: true,
            refresh_interval: 999,
            opencode_workspace_id_override: "wrk_prefs".into(),
            ..Default::default()
        };
        let prefs = UsageBarPrefs::from_snapshot(&snapshot);
        assert!(!prefs.claude_visible);
        assert!(prefs.codex_visible);
        assert!(prefs.opencode_visible);
        assert_eq!(
            prefs.refresh_interval_min, 60,
            "clamped to the stepper range"
        );
        assert_eq!(
            prefs.opencode_workspace_id_override, "wrk_prefs",
            "the override rides the same snapshot mapping (F-SET-12)"
        );
    }

    /// F-SET-11: the seven states this file can distinguish (Loading and
    /// Loaded are exercised by the drawn test above via the real fetch
    /// loop) each render their own text. `Stale` is functionally identical
    /// to `Loaded` here except for dimming (covered separately below), so
    /// this proves the three reasons stay apart instead of collapsing to
    /// one wildcard "—". The four unavailable reasons must each retain their
    /// own text: not found, logged out, timed out, and error.
    #[test]
    fn unavailable_reasons_render_distinct_text() {
        assert_eq!(
            StatusBar::segment_text("Claude", &ProviderUsageState::Loading),
            "Claude …"
        );
        assert_eq!(
            StatusBar::segment_text(
                "Claude",
                &ProviderUsageState::Unavailable(UsageReason::NotInstalled)
            ),
            "Claude not found"
        );
        assert_eq!(
            StatusBar::segment_text(
                "Claude",
                &ProviderUsageState::Unavailable(UsageReason::LoggedOut)
            ),
            "Claude logged out"
        );
        assert_eq!(
            StatusBar::segment_text(
                "Claude",
                &ProviderUsageState::Unavailable(UsageReason::TimedOut)
            ),
            "Claude timed out"
        );
        assert_eq!(
            StatusBar::segment_text(
                "Claude",
                &ProviderUsageState::Unavailable(UsageReason::Error)
            ),
            "Claude error"
        );
        let usage = ProviderUsage {
            session: Some(UsageWindow::new("5h", 12)),
            weekly: None,
            monthly: None,
            fable_weekly: None,
        };
        assert_eq!(
            StatusBar::segment_text("Claude", &ProviderUsageState::Loaded(usage.clone())),
            "Claude 12% 5h",
            "a loaded state still renders real numbers, not a reason"
        );
        assert_eq!(
            StatusBar::segment_text("Claude", &ProviderUsageState::Stale(usage)),
            "Claude 12% 5h",
            "stale keeps showing the last good numbers — dimming is what marks it stale, not the text"
        );
    }

    /// F-SET-11: only a fresh `Loaded` reads at full opacity — `Loading`,
    /// `Stale` and every `Unavailable` reason must all look dimmed, or a
    /// user cannot tell live numbers from a provider that has gone quiet.
    #[test]
    fn only_loaded_state_renders_undimmed() {
        assert!(!StatusBar::segment_dimmed(&ProviderUsageState::Loaded(
            ProviderUsage::default()
        )));
        assert!(StatusBar::segment_dimmed(&ProviderUsageState::Loading));
        assert!(StatusBar::segment_dimmed(&ProviderUsageState::Stale(
            ProviderUsage::default()
        )));
        assert!(StatusBar::segment_dimmed(&ProviderUsageState::Unavailable(
            UsageReason::NotInstalled
        )));
        assert!(StatusBar::segment_dimmed(&ProviderUsageState::Unavailable(
            UsageReason::LoggedOut
        )));
        assert!(StatusBar::segment_dimmed(&ProviderUsageState::Unavailable(
            UsageReason::TimedOut
        )));
        assert!(StatusBar::segment_dimmed(&ProviderUsageState::Unavailable(
            UsageReason::Error
        )));
    }
}
