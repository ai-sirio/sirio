//! The fixed-height usage/status strip below the workspace columns.
//!
//! The usage segment is real data: the bar owns a Claude
//! usage fetch that runs on the background executor (never the render
//! thread), refreshes on a fixed interval, and can be triggered manually
//! from the refresh button. A provider that is missing, unreadable or
//! malformed renders its specific unavailable reason; a timed-out refresh
//! keeps the last good numbers visibly dimmed rather than showing them as
//! current.

use gpui::{AnyView, Context, Render, Rgba, Window, div, prelude::*, px, text};
use std::rc::Rc;
use std::time::Duration;
use tiller_theme::Theme;
use tiller_usage::{
    ClaudeUsageFetcher, CodexUsageFetcher, OllamaCloudUsageFetcher, OpenCodeGoUsageFetcher,
    ProviderUsageState, UsageFetchOutcome, UsageReason, reduce,
};

use crate::sidebar::icons::{Icon, IconElement, IconSize};

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
    /// Ollama Cloud segment visibility (F-SET-13).
    pub ollama_visible: bool,
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
            ollama_visible: false,
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
            ollama_visible: snapshot.ollama_show_in_bar,
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
    ollama_cloud: ProviderUsageState,
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
            ollama_cloud: ProviderUsageState::Loading,
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

    /// The current Ollama Cloud usage state, for tests and the host.
    pub fn ollama_cloud_state(&self) -> &ProviderUsageState {
        &self.ollama_cloud
    }

    /// Applies one round of fetch outcomes; never panics on a bad provider.
    fn apply_outcomes(
        &mut self,
        claude: UsageFetchOutcome,
        codex: UsageFetchOutcome,
        opencode_go: UsageFetchOutcome,
        ollama_cloud: UsageFetchOutcome,
        cx: &mut Context<Self>,
    ) {
        self.claude = reduce(claude, &self.claude);
        self.codex = reduce(codex, &self.codex);
        self.opencode_go = reduce(opencode_go, &self.opencode_go);
        self.ollama_cloud = reduce(ollama_cloud, &self.ollama_cloud);
        cx.notify();
    }

    /// Handles the manual refresh control (F-USE-01): tells the host, if
    /// one is listening via [`Self::on_refresh`], and — independent of
    /// whether a host is wired up at all — forces every provider segment
    /// to refetch immediately instead of waiting for the next interval
    /// tick. Segments flip to their "…" loading text right away so the
    /// click has a visible effect even before the fetches return.
    fn on_refresh_clicked(&mut self, cx: &mut Context<Self>) {
        if let Some(callback) = &self.on_refresh {
            callback();
        }
        self.claude = ProviderUsageState::Loading;
        self.codex = ProviderUsageState::Loading;
        self.opencode_go = ProviderUsageState::Loading;
        self.ollama_cloud = ProviderUsageState::Loading;
        cx.notify();

        let workspace_override = {
            let trimmed = self.prefs.opencode_workspace_id_override.trim();
            (!trimmed.is_empty()).then(|| trimmed.to_string())
        };
        cx.spawn(async move |this, cx| {
            let executor = cx.background_executor();
            let claude = executor.spawn(async move { ClaudeUsageFetcher::fetch() });
            let codex = executor.spawn(async move { CodexUsageFetcher::fetch() });
            let opencode_go = executor
                .spawn(async move { OpenCodeGoUsageFetcher::fetch(workspace_override.as_deref()) });
            let ollama_cloud = executor.spawn(async move { OllamaCloudUsageFetcher::fetch() });
            let (claude, codex, opencode_go, ollama_cloud) = (
                claude.await,
                codex.await,
                opencode_go.await,
                ollama_cloud.await,
            );
            let _ = this.update(cx, |bar, cx| {
                bar.apply_outcomes(claude, codex, opencode_go, ollama_cloud, cx);
            });
        })
        .detach();
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
                let ollama_cloud = executor.spawn(async move { OllamaCloudUsageFetcher::fetch() });
                let (claude, codex, opencode_go, ollama_cloud) = (
                    claude.await,
                    codex.await,
                    opencode_go.await,
                    ollama_cloud.await,
                );
                let interval = match this.update(cx, |bar, cx| {
                    bar.apply_outcomes(claude, codex, opencode_go, ollama_cloud, cx);
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
                    // F-CORE-USG-06: a Codex refresh-token failure classified
                    // as reused/revoked/expired gets its own copy instead of
                    // collapsing into the generic "logged out".
                    UsageReason::TokenReused => "token reused",
                    UsageReason::TokenRevoked => "token revoked",
                    UsageReason::TokenExpired => "token expired",
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
        let refresh_entity = cx.entity();

        let icon_button = |id: &'static str, icon: Icon| {
            div()
                .id(id)
                .w(px(22.0))
                .h(px(22.0))
                .flex()
                .items_center()
                .justify_center()
                .rounded(theme.radii.control)
                .text_size(theme.typography.footnote)
                .text_color(theme.title)
                .hover(|style| style.bg(theme.row_hover))
                .child(IconElement::new(icon, IconSize::XSmall).text_color(theme.title))
        };

        // The four provider segments, in the reference order: Claude,
        // Codex, OpenCode Go, Ollama Cloud. A stale or unavailable provider
        // reads dimmer than fresh numbers — never a plausible-looking fake.
        let claude_dimmed = Self::segment_dimmed(&self.claude);
        let codex_dimmed = Self::segment_dimmed(&self.codex);
        let opencode_go_dimmed = Self::segment_dimmed(&self.opencode_go);
        let ollama_cloud_dimmed = Self::segment_dimmed(&self.ollama_cloud);
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
        let ollama_cloud_color = if ollama_cloud_dimmed {
            dim(theme.title)
        } else {
            theme.title
        };

        let provider_segment =
            move |display_name: &'static str, mark: Icon, text_color: gpui::Rgba, text: String| {
                // The `text!` macro derives its element id from its own source
                // location: inside this closure the location is shared by all
                // three segments, so the ids must be explicit or the duplicate
                // element ids make GPUI drop all but one segment.
                let text_id = format!("{display_name}-usage-text");
                // F-USE-02: the segment's own text is the unavailable reason
                // (or the abbreviated numbers) already — the tooltip repeats it
                // rather than inventing a second vocabulary, so it stays
                // correct for every state (Loading/Loaded/Stale/Unavailable)
                // for free.
                let segment_id = format!("{display_name}-usage-segment");
                let tooltip_text = text.clone();
                div()
                    .id(segment_id)
                    .debug_selector(move || format!("{display_name}-usage-text"))
                    .flex()
                    .items_center()
                    .gap(px(5.0))
                    .text_size(theme.typography.caption2)
                    .text_color(text_color)
                    .tooltip(move |_, cx| -> AnyView {
                        let tooltip_text = tooltip_text.clone();
                        cx.new(|_| StatusBarTooltip {
                            theme,
                            text: tooltip_text,
                        })
                        .into()
                    })
                    .child(IconElement::new(mark, IconSize::XSmall))
                    .child(text!(id = text_id, text))
            };

        // The segments follow the settings surface's "Show in usage bar"
        // toggles (F-SET-10): a provider hidden there does not render here.
        let mut left = div()
            .flex()
            .items_center()
            .gap(px(12.0))
            .child(
                icon_button("status-settings", Icon::Settings).on_click(move |_, _, _| {
                    if let Some(callback) = &settings {
                        callback();
                    }
                }),
            )
            .child(
                // F-USE-01: `on_refresh` had a real field and builder but
                // no control in the render tree ever invoked it.
                icon_button("status-refresh", Icon::RefreshCw).on_click(move |_, _, cx| {
                    refresh_entity.update(cx, |bar, cx| bar.on_refresh_clicked(cx));
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
        if self.prefs.ollama_visible {
            // F-SET-13: no Ollama brand mark exists in the pinned Zed catalog —
            // the globe is a declared stand-in for a cloud service, not a
            // silent leftover.
            left = left.child(provider_segment(
                "Ollama Cloud",
                Icon::Globe,
                ollama_cloud_color,
                Self::segment_text("Ollama Cloud", &self.ollama_cloud),
            ));
        }

        div()
            .id("tiller-status-bar")
            .debug_selector(|| "tiller-status-bar".into())
            .w_full()
            .h(px(HEIGHT))
            .px(px(12.0))
            .flex()
            .items_center()
            .bg(gpui::transparent_black())
            .text_size(theme.typography.caption2)
            .text_color(theme.meta)
            .child(left)
            .child(div().flex_1())
            .child(text!(format!("{} · {}", self.data.branch, self.data.path)))
    }
}

/// F-USE-02: the hover tooltip for one usage-bar segment. Repeats the
/// segment's own visible text at full opacity — useful when the bar's
/// caption-size type or a long "not found"/"logged out" reason is easy to
/// misread at a glance.
struct StatusBarTooltip {
    theme: Theme,
    text: String,
}

impl Render for StatusBarTooltip {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .px(px(8.0))
            .py(px(4.0))
            .rounded(self.theme.radii.control)
            .bg(self.theme.raised)
            .border_1()
            .border_color(self.theme.hairline)
            .text_size(self.theme.typography.caption2)
            .text_color(self.theme.title)
            .child(self.text.clone())
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

        // Defaults: Claude and Codex visible, OpenCode Go and Ollama
        // Cloud hidden.
        assert!(
            cx.debug_bounds("Claude-usage-text").is_some(),
            "a visible provider renders its segment"
        );
        assert!(cx.debug_bounds("Codex-usage-text").is_some());
        assert!(
            cx.debug_bounds("OpenCode Go-usage-text").is_none(),
            "a provider hidden in settings renders no segment"
        );
        assert!(
            cx.debug_bounds("Ollama Cloud-usage-text").is_none(),
            "Ollama Cloud defaults to hidden (F-SET-13)"
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
                    ollama_visible: true,
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
        assert!(
            cx.debug_bounds("Ollama Cloud-usage-text").is_some(),
            "the Ollama Cloud toggle adds its segment (F-SET-13)"
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

    /// F-SET-10: the wave-D critic confirmed `on_refresh_clicked` sets all
    /// four provider states to `Loading` and `cx.notify()`s synchronously,
    /// before the background fetches are even spawned -- but could not
    /// catch the transient pixels live under this session's shared-machine
    /// contention (every capture round trip outlasted the fetch). This
    /// proves the synchronous half directly: read the entity's state right
    /// after calling the click handler and before `run_until_parked` lets
    /// the spawned fetch task run at all, so there is no race to lose.
    #[gpui::test]
    async fn refresh_click_flips_every_provider_to_loading_before_the_fetch_runs(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(Theme::init);
        let window = cx.add_window(|_window, _cx| StatusBar::new_with_default_context());
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        // Let the constructor's own fetch loop settle so every segment
        // starts from something other than Loading -- otherwise the test
        // could pass by accident (never having left the constructor's
        // initial state).
        cx.run_until_parked();

        let bar = cx.update(|window, _cx| {
            window
                .root::<StatusBar>()
                .flatten()
                .expect("status bar root")
        });
        let settled = cx.update(|_window, cx| bar.read(cx).claude.clone());
        assert!(
            !matches!(settled, ProviderUsageState::Loading),
            "the fetch loop must have settled to something other than \
             Loading before the click, or this test cannot tell the click \
             apart from the constructor's own initial state"
        );

        bar.update(&mut cx, |bar, cx| bar.on_refresh_clicked(cx));

        // No `run_until_parked` here: the spawned fetch task has not been
        // polled yet, so this reads exactly the synchronous state the
        // click handler left behind.
        let (claude, codex, opencode_go, ollama_cloud) = cx.update(|_window, cx| {
            let bar = bar.read(cx);
            (
                bar.claude.clone(),
                bar.codex.clone(),
                bar.opencode_go.clone(),
                bar.ollama_cloud.clone(),
            )
        });
        assert!(matches!(claude, ProviderUsageState::Loading));
        assert!(matches!(codex, ProviderUsageState::Loading));
        assert!(matches!(opencode_go, ProviderUsageState::Loading));
        assert!(matches!(ollama_cloud, ProviderUsageState::Loading));

        // Let the detached fetch task finish so it does not outlive the
        // test's executor.
        cx.run_until_parked();
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
            ollama_show_in_bar: true,
            refresh_interval: 999,
            opencode_workspace_id_override: "wrk_prefs".into(),
            ..Default::default()
        };
        let prefs = UsageBarPrefs::from_snapshot(&snapshot);
        assert!(!prefs.claude_visible);
        assert!(prefs.codex_visible);
        assert!(prefs.opencode_visible);
        assert!(prefs.ollama_visible, "F-SET-13 rides the same mapping");
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

    /// F-CORE-USG-06: a Codex refresh-token failure classified as
    /// reused/revoked/expired must render its own copy, not collapse to
    /// the generic "logged out" text every other login failure uses.
    #[test]
    fn token_refresh_reasons_render_distinct_text() {
        assert_eq!(
            StatusBar::segment_text(
                "Codex",
                &ProviderUsageState::Unavailable(UsageReason::TokenReused)
            ),
            "Codex token reused"
        );
        assert_eq!(
            StatusBar::segment_text(
                "Codex",
                &ProviderUsageState::Unavailable(UsageReason::TokenRevoked)
            ),
            "Codex token revoked"
        );
        assert_eq!(
            StatusBar::segment_text(
                "Codex",
                &ProviderUsageState::Unavailable(UsageReason::TokenExpired)
            ),
            "Codex token expired"
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

    /// F-USE-03: the reducer (`model::tests::success_replaces_the_previous_state`)
    /// and `segment_dimmed`/`segment_text` as pure functions were already
    /// proven, but nothing exercised the wiring that actually carries a
    /// real Success->TimedOut transition into a live `StatusBar` entity --
    /// the exact path `apply_outcomes` feeds from both `on_refresh_clicked`
    /// and the periodic `ensure_refresh_task` loop. Driving a real 25s
    /// `ClaudeUsageFetcher::TIMEOUT` hang through the Wayland lane (twice,
    /// to first get a `Loaded` baseline and then a real timeout) risked
    /// this dispatch's own silence-kill for no more proof than this: call
    /// the production entity method directly with synthetic outcomes and
    /// confirm the entity's live `claude` field -- the same field `render`
    /// reads via `segment_dimmed` -- really does land on `Stale` and stays
    /// dimmed, without a panic or a stuck `cx.notify()`, across two calls on
    /// a real `Context<StatusBar>` (not a bare `reduce()` call with no
    /// entity behind it).
    #[gpui::test]
    async fn a_real_timeout_after_a_real_success_dims_the_live_entity(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(Theme::init);
        let window = cx.add_window(|_window, _cx| StatusBar::new_with_default_context());
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        // Let the constructor's own fetch loop settle first so the
        // transition below is unambiguously caused by the two
        // `apply_outcomes` calls, not leftover constructor state.
        cx.run_until_parked();

        let bar = cx.update(|window, _cx| {
            window
                .root::<StatusBar>()
                .flatten()
                .expect("status bar root")
        });

        let usage = ProviderUsage {
            session: Some(UsageWindow::new("5h", 42)),
            weekly: None,
            monthly: None,
            fable_weekly: None,
        };
        bar.update(&mut cx, |bar, cx| {
            bar.apply_outcomes(
                UsageFetchOutcome::Success(usage.clone()),
                UsageFetchOutcome::Unavailable(UsageReason::NotInstalled),
                UsageFetchOutcome::Unavailable(UsageReason::NotInstalled),
                UsageFetchOutcome::Unavailable(UsageReason::NotInstalled),
                cx,
            );
        });
        cx.run_until_parked();
        let loaded = cx.update(|_window, cx| bar.read(cx).claude.clone());
        assert_eq!(
            loaded,
            ProviderUsageState::Loaded(usage.clone()),
            "the entity's live field must reflect a real Success outcome \
             before a timeout can meaningfully go stale"
        );
        assert!(
            !StatusBar::segment_dimmed(&loaded),
            "a freshly loaded segment must not render dimmed"
        );
        assert!(
            cx.debug_bounds("Claude-usage-text").is_some(),
            "the Loaded segment renders without panicking"
        );

        bar.update(&mut cx, |bar, cx| {
            bar.apply_outcomes(
                UsageFetchOutcome::TimedOut,
                UsageFetchOutcome::Unavailable(UsageReason::NotInstalled),
                UsageFetchOutcome::Unavailable(UsageReason::NotInstalled),
                UsageFetchOutcome::Unavailable(UsageReason::NotInstalled),
                cx,
            );
        });
        cx.run_until_parked();
        let stale = cx.update(|_window, cx| bar.read(cx).claude.clone());
        assert_eq!(
            stale,
            ProviderUsageState::Stale(usage),
            "a real TimedOut outcome fed through the production entity \
             method must keep the last good numbers, not drop them"
        );
        assert!(
            StatusBar::segment_dimmed(&stale),
            "the same field render() reads must now report dimmed"
        );
        assert!(
            cx.debug_bounds("Claude-usage-text").is_some(),
            "the Stale segment still renders (dimmed, not hidden) without panicking"
        );
    }
}
