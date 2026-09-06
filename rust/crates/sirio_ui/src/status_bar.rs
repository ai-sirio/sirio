//! The fixed-height usage/status strip below the workspace columns.
//!
//! The usage segment is real data: the bar owns a Claude
//! usage fetch that runs on the background executor (never the render
//! thread), refreshes on a fixed interval, and can be triggered manually
//! from the refresh button. The bar shows numbers and nothing else: a
//! provider that is missing, unreadable, unsupported here or still loading
//! has no segment at all rather than a reason; a timed-out refresh keeps
//! the last good numbers visibly dimmed rather than showing them as
//! current.

use gpui::{AnyView, Context, Render, Rgba, Window, div, prelude::*, px, text};
use sirio_theme::Theme;
use sirio_usage::{
    ClaudeUsageFetcher, CodexUsageFetcher, OllamaCloudUsageFetcher, OpenCodeGoUsageFetcher,
    ProviderUsage, ProviderUsageState, UsageFetchOutcome, UsageWindow, reduce, reset_countdown,
};
use std::rc::Rc;
use std::time::{Duration, SystemTime};

use crate::loading;
use crate::sidebar::icons::{Icon, IconElement, IconSize};

pub(crate) const HEIGHT: f32 = 40.0;

/// The Swift default: refresh every five minutes (60..3600 allowed).
const REFRESH_INTERVAL: Duration = Duration::from_secs(300);

/// The pill meter's track width beside each provider's numbers.
const METER_WIDTH: f32 = 18.0;

/// The opacity stale numbers (and their meter) drop to; matches [`dim`].
const DIM_OPACITY: f32 = 0.55;

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

/// The update facts the host supplies to the two UI surfaces. The updater
/// owns checking, downloading and applying; this crate only renders its
/// result and emits user intent back to the host.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UpdateStatus {
    Disabled,
    NotDue,
    Checking,
    UpToDate,
    Available { version: String, notes: String },
    Ready { version: String, notes: String },
    Failed { message: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UpdateState {
    /// A per-install opt-out. The host must use this to suppress polling and
    /// downloading as well as the indicator.
    pub enabled: bool,
    /// Host-supplied channel, kept as text so this crate stays independent of
    /// `sirio_control` and `sirio_update`.
    pub channel: String,
    pub status: UpdateStatus,
    /// Host-formatted local time, e.g. `14 minutes ago`.
    pub last_checked: Option<String>,
}

impl Default for UpdateState {
    fn default() -> Self {
        Self {
            enabled: true,
            channel: String::new(),
            status: UpdateStatus::NotDue,
            last_checked: None,
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
            path: "~/Desktop/Progetti/sirio".into(),
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
    /// Host-owned update state. The bar only renders the available/ready
    /// states and routes the click to the existing Settings callback.
    update_state: UpdateState,
    /// Lazily armed on first render (the constructor has no context to spawn
    /// with).
    refresh_task_started: bool,
    on_settings: Option<Rc<dyn Fn()>>,
    /// Host callback for opening the General update detail. Falls back to
    /// `on_settings` when a host does not need a distinct destination.
    on_update: Option<Rc<dyn Fn()>>,
    on_refresh: Option<Rc<dyn Fn()>>,
    /// How many times gpui asked this view to render. Test-observable only:
    /// the host caches the bar, and a still status bar must not render at
    /// all while a spinner elsewhere keeps the window drawing.
    render_count: u64,
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
            update_state: UpdateState::default(),
            refresh_task_started: false,
            on_settings: None,
            on_update: None,
            on_refresh: None,
            render_count: 0,
        }
    }

    /// How many times this view has rendered. Only a test should read it:
    /// it exists so the host can prove a cached, still bar is reused across
    /// frames rather than re-rendered.
    pub fn render_count(&self) -> u64 {
        self.render_count
    }

    pub fn new_with_default_context() -> Self {
        Self::new(UsageBarData::default_context())
    }

    /// The current host-owned update state, for the host and tests.
    pub fn update_state(&self) -> &UpdateState {
        &self.update_state
    }

    /// Supplies the host-owned update state shown in the bar.
    pub fn with_update_state(mut self, state: UpdateState) -> Self {
        self.update_state = state;
        self
    }

    /// Replaces the host-owned update state without starting any updater work.
    pub fn apply_update_state(&mut self, state: UpdateState, cx: &mut Context<Self>) {
        self.update_state = state;
        cx.notify();
    }

    /// The preferences the bar currently consumes (F-SET-10), as last
    /// routed by the host through [`Self::apply_preferences`] or set at
    /// construction.
    pub fn preferences(&self) -> &UsageBarPrefs {
        &self.prefs
    }

    /// Whether the quiet status-bar indicator should be drawn.
    fn update_indicator_visible(state: &UpdateState) -> bool {
        state.enabled
            && matches!(
                state.status,
                UpdateStatus::Available { .. } | UpdateStatus::Ready { .. }
            )
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

    /// Installs the host callback used by the update indicator. Hosts should
    /// route it to Settings → General rather than starting an update.
    pub fn on_update(mut self, callback: impl Fn() + 'static) -> Self {
        self.on_update = Some(Rc::new(callback));
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
    /// tick. Segments flip to `Loading` right away — the refresh control
    /// spins and the numbers leave the bar until the fetches return — so
    /// the click has a visible effect before any result is in.
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

    /// The segment text for one provider — "3% used 1h 40m · 24% used 5d
    /// 23h · 46% used Fable" — or `None` when there are no numbers to show.
    /// The bar renders numbers and nothing else: a provider that is still
    /// loading, could not be read (logged out, API key, not installed,
    /// unsupported here, timed out with nothing to keep), or succeeded
    /// without a single window gets no segment rather than a reason. The
    /// reason remains a distinct fact in [`sirio_usage::UsageReason`] for a
    /// surface that explains; this one only counts. Stale numbers are still
    /// numbers, dimmed by [`Self::segment_dimmed`].
    fn segment_text(state: &ProviderUsageState) -> Option<String> {
        Self::segment_text_at(state, SystemTime::now())
    }

    /// [`Self::segment_text`] with the clock injected, so the countdowns can
    /// be asserted against a fixed `now`.
    fn segment_text_at(state: &ProviderUsageState, now: SystemTime) -> Option<String> {
        match state {
            ProviderUsageState::Loaded(usage) | ProviderUsageState::Stale(usage) => {
                let parts = Self::window_texts(usage, now);
                (!parts.is_empty()).then(|| parts.join(" · "))
            }
            ProviderUsageState::Loading | ProviderUsageState::Unavailable(_) => None,
        }
    }

    /// One text per window read, in the bar's order: `3% used 1h 40m` — the
    /// percent, "used", and the time to that window's reset. A window whose
    /// reset is unknown keeps its label (`12% used 5h`) so the reader still
    /// knows which limit it is. The Fable window keeps its name always: its
    /// reset coincides with the weekly one, and a second `5d 23h` would leave
    /// the two indistinguishable.
    fn window_texts(usage: &ProviderUsage, now: SystemTime) -> Vec<String> {
        let counted = |window: &UsageWindow| {
            let tail = window
                .resets_at
                .map(|resets_at| reset_countdown(resets_at, now))
                .unwrap_or_else(|| window.label.clone());
            format!("{}% used {tail}", window.used_percent)
        };
        let named =
            |window: &UsageWindow| format!("{}% used {}", window.used_percent, window.label);
        [
            usage.session.as_ref().map(counted),
            usage.weekly.as_ref().map(counted),
            usage.monthly.as_ref().map(counted),
            usage.fable_weekly.as_ref().map(named),
        ]
        .into_iter()
        .flatten()
        .collect()
    }

    /// The pill meter's fill: the session window, or the first window the
    /// provider reports when it has no session (Ollama Cloud reads one
    /// "usage" window). `0.0` when nothing was read.
    fn meter_fraction(usage: &ProviderUsage) -> f32 {
        [
            usage.session.as_ref(),
            usage.weekly.as_ref(),
            usage.monthly.as_ref(),
            usage.fable_weekly.as_ref(),
        ]
        .into_iter()
        .flatten()
        .next()
        .map_or(0.0, |window| f32::from(window.used_percent) / 100.0)
    }

    /// The pill meter's fill for a segment. Only a segment with numbers is
    /// drawn (see [`Self::segment_text`]), so a state without them fills
    /// nothing: the value is never rendered.
    fn segment_meter(state: &ProviderUsageState) -> f32 {
        match state {
            ProviderUsageState::Loaded(usage) | ProviderUsageState::Stale(usage) => {
                Self::meter_fraction(usage)
            }
            ProviderUsageState::Loading | ProviderUsageState::Unavailable(_) => 0.0,
        }
    }

    /// Whether the segment is dimmed: stale data and unavailability must
    /// look different from fresh numbers.
    fn segment_dimmed(state: &ProviderUsageState) -> bool {
        !matches!(state, ProviderUsageState::Loaded(_))
    }
}

impl Render for StatusBar {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let _perf = sirio_perf::span("StatusBar.render", cx.entity_id().as_u64());
        self.render_count = self.render_count.wrapping_add(1);
        let theme = *Theme::get(cx);
        self.ensure_refresh_task(cx);
        let settings = self.on_settings.clone();
        let update_settings = self.on_update.clone().or(settings.clone());
        let refresh_entity = cx.entity();
        let refreshing = matches!(&self.claude, ProviderUsageState::Loading)
            || matches!(&self.codex, ProviderUsageState::Loading)
            || matches!(&self.opencode_go, ProviderUsageState::Loading)
            || matches!(&self.ollama_cloud, ProviderUsageState::Loading);

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
                .text_color(theme.text)
                .hover(|style| style.bg(theme.element_hover))
                .child(IconElement::new(icon, IconSize::Medium).text_color(theme.text))
        };

        // The four provider segments, in the reference order: Claude,
        // Codex, OpenCode Go, Ollama Cloud. A stale or unavailable provider
        // reads dimmer than fresh numbers — never a plausible-looking fake.
        let claude_dimmed = Self::segment_dimmed(&self.claude);
        let codex_dimmed = Self::segment_dimmed(&self.codex);
        let opencode_go_dimmed = Self::segment_dimmed(&self.opencode_go);
        let ollama_cloud_dimmed = Self::segment_dimmed(&self.ollama_cloud);
        let status_text_color = status_bar_foreground(theme);
        let claude_color = if claude_dimmed {
            dim(status_text_color)
        } else {
            status_text_color
        };
        let codex_color = if codex_dimmed {
            dim(theme.text)
        } else {
            theme.text
        };
        let opencode_go_color = if opencode_go_dimmed {
            dim(theme.text)
        } else {
            theme.text
        };
        let ollama_cloud_color = if ollama_cloud_dimmed {
            dim(theme.text)
        } else {
            theme.text
        };

        let provider_segment = move |display_name: &'static str,
                                     mark: Icon,
                                     text_color: gpui::Rgba,
                                     dimmed: bool,
                                     meter: f32,
                                     text: String| {
            // The `text!` macro derives its element id from its own source
            // location: inside this closure the location is shared by all
            // three segments, so the ids must be explicit or the duplicate
            // element ids make GPUI drop all but one segment.
            let text_id = format!("{display_name}-usage-text");
            // F-USE-02: the tooltip repeats the segment's own text rather
            // than inventing a second vocabulary. The text carries numbers
            // and no name (the brand mark names the provider in the bar), so
            // the tooltip is where the name lives: "Claude · 3% used 1h 40m".
            let segment_id = format!("{display_name}-usage-segment");
            let tooltip_text = format!("{display_name} · {text}");
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
                // The pill: bezel's determinate progress bar on a fixed
                // narrow track, so the row never reflows as the value
                // moves. Dimmed with the text when the numbers are stale.
                .child(
                    div()
                        .id(format!("{display_name}-usage-meter"))
                        .debug_selector(move || format!("{display_name}-usage-meter"))
                        .flex_none()
                        .w(px(METER_WIDTH))
                        .opacity(if dimmed { DIM_OPACITY } else { 1.0 })
                        .child(loading::progress(meter, &theme)),
                )
                .child(text!(id = text_id, text))
        };

        // The segments follow the settings surface's "Show in usage bar"
        // toggles (F-SET-10): a provider hidden there does not render here.
        let refresh_button = div()
            .id("status-refresh")
            .w(px(22.0))
            .h(px(22.0))
            .flex()
            .items_center()
            .justify_center()
            .rounded(theme.radii.control)
            .text_size(theme.typography.footnote)
            .text_color(theme.text)
            .hover(|style| style.bg(theme.element_hover))
            .on_click(move |_, _, cx| {
                refresh_entity.update(cx, |bar, cx| bar.on_refresh_clicked(cx));
            })
            .child(if refreshing {
                loading::compact("status-refresh-spinner", window, cx)
            } else {
                IconElement::new(Icon::RefreshCw, IconSize::Medium)
                    .text_color(theme.text)
                    .into_any_element()
            });

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
            .child(refresh_button);
        // A segment exists only for a provider with numbers to show:
        // loading, unavailable for any reason, or read without a window all
        // leave the bar untouched rather than narrating the error here.
        if self.prefs.claude_visible
            && let Some(text) = Self::segment_text(&self.claude)
        {
            left = left.child(provider_segment(
                "Claude",
                Icon::ClaudeCode,
                claude_color,
                claude_dimmed,
                Self::segment_meter(&self.claude),
                text,
            ));
        }
        if self.prefs.codex_visible
            && let Some(text) = Self::segment_text(&self.codex)
        {
            left = left.child(provider_segment(
                "Codex",
                Icon::Codex,
                codex_color,
                codex_dimmed,
                Self::segment_meter(&self.codex),
                text,
            ));
        }
        if self.prefs.opencode_visible
            && let Some(text) = Self::segment_text(&self.opencode_go)
        {
            left = left.child(provider_segment(
                "OpenCode Go",
                Icon::OpenCode,
                opencode_go_color,
                opencode_go_dimmed,
                Self::segment_meter(&self.opencode_go),
                text,
            ));
        }
        if self.prefs.ollama_visible
            && let Some(text) = Self::segment_text(&self.ollama_cloud)
        {
            // F-SET-13: no Ollama brand mark exists in the pinned Zed catalog —
            // the globe is a declared stand-in for a cloud service, not a
            // silent leftover.
            left = left.child(provider_segment(
                "Ollama Cloud",
                Icon::Globe,
                ollama_cloud_color,
                ollama_cloud_dimmed,
                Self::segment_meter(&self.ollama_cloud),
                text,
            ));
        }
        if Self::update_indicator_visible(&self.update_state) {
            left = left.child(
                div()
                    .id("status-update-indicator")
                    .debug_selector(|| "status-update-indicator".into())
                    .flex()
                    .items_center()
                    .gap(px(5.0))
                    .text_size(theme.typography.caption2)
                    .text_color(theme.text)
                    .hover(|style| style.opacity(0.9))
                    .on_click(move |_, _, _| {
                        if let Some(callback) = &update_settings {
                            callback();
                        }
                    })
                    .child(IconElement::new(Icon::Sparkles, IconSize::XSmall))
                    .child(text!(id = "status-update-label", "Update available")),
            );
        }

        div()
            .id("sirio-status-bar")
            .debug_selector(|| "sirio-status-bar".into())
            .w_full()
            .h(px(HEIGHT))
            .px(px(12.0))
            .flex()
            .items_center()
            .bg(gpui::transparent_black())
            .text_size(theme.typography.caption2)
            .text_color(status_text_color)
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
            .bg(self.theme.surface_raised)
            .border_1()
            .border_color(self.theme.border)
            .text_size(self.theme.typography.caption2)
            .text_color(self.theme.text)
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

/// Uses Bezel's secondary-text rung for the light status bar, where its
/// metadata rung is too faint against the light shell surface. Dark keeps the
/// existing metadata color unchanged.
fn status_bar_foreground(theme: Theme) -> Rgba {
    match theme.appearance {
        sirio_theme::Appearance::Light => theme.text_muted,
        sirio_theme::Appearance::Dark => theme.text_faint,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{Modifiers, VisualTestContext};
    use sirio_usage::UsageReason;

    #[test]
    fn light_status_bar_foreground_uses_muted_text_and_dark_stays_faint() {
        let light = Theme::light();
        let dark = Theme::dark();

        assert_eq!(status_bar_foreground(light), light.text_muted);
        assert_eq!(status_bar_foreground(dark), dark.text_faint);
        assert_ne!(light.text_muted, light.text_faint);
    }

    fn window_resetting_in(label: &str, percent: u8, now: SystemTime, secs: u64) -> UsageWindow {
        UsageWindow {
            resets_at: Some(now + Duration::from_secs(secs)),
            ..UsageWindow::new(label, percent)
        }
    }

    /// The reference bar reads `3% used 1h 40m · 24% used 5d 23h · 46% used
    /// Fable`: each window is its percent, the word "used", and the time
    /// to its reset — except Fable, which keeps its name because its reset
    /// coincides with the weekly one and a second `5d 23h` would leave the
    /// two windows indistinguishable. The provider name is not in the text:
    /// the brand mark carries it.
    #[test]
    fn loaded_windows_read_as_percent_used_and_the_time_to_reset() {
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000);
        let usage = ProviderUsage {
            session: Some(window_resetting_in("5h", 3, now, 3_600 + 40 * 60)),
            weekly: Some(window_resetting_in(
                "wk",
                24,
                now,
                5 * 86_400 + 23 * 3_600 + 5 * 60,
            )),
            monthly: None,
            fable_weekly: Some(window_resetting_in(
                "Fable",
                46,
                now,
                5 * 86_400 + 23 * 3_600 + 5 * 60,
            )),
        };
        assert_eq!(
            StatusBar::window_texts(&usage, now),
            vec!["3% used 1h 40m", "24% used 5d 23h", "46% used Fable"]
        );
        assert_eq!(
            StatusBar::segment_text_at(&ProviderUsageState::Loaded(usage.clone()), now),
            Some("3% used 1h 40m · 24% used 5d 23h · 46% used Fable".to_string())
        );
        assert_eq!(
            StatusBar::segment_text_at(&ProviderUsageState::Stale(usage), now),
            Some("3% used 1h 40m · 24% used 5d 23h · 46% used Fable".to_string()),
            "stale keeps the same text — dimming is what marks it stale"
        );
    }

    #[test]
    fn a_window_without_a_known_reset_falls_back_to_its_label() {
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000);
        let usage = ProviderUsage {
            session: Some(UsageWindow::new("5h", 12)),
            weekly: Some(UsageWindow::new("wk", 34)),
            monthly: Some(UsageWindow::new("mo", 71)),
            fable_weekly: None,
        };
        assert_eq!(
            StatusBar::window_texts(&usage, now),
            vec!["12% used 5h", "34% used wk", "71% used mo"]
        );
    }

    /// The pill meter is the session window; a provider without one (Ollama
    /// Cloud reads a single "usage" window) shows the first window it has.
    #[test]
    fn the_meter_tracks_the_session_window_or_the_first_window_read() {
        let session_and_weekly = ProviderUsage {
            session: Some(UsageWindow::new("5h", 3)),
            weekly: Some(UsageWindow::new("wk", 24)),
            monthly: None,
            fable_weekly: None,
        };
        assert!((StatusBar::meter_fraction(&session_and_weekly) - 0.03).abs() < 1e-6);

        let weekly_only = ProviderUsage {
            session: None,
            weekly: Some(UsageWindow::new("wk", 24)),
            monthly: None,
            fable_weekly: None,
        };
        assert!((StatusBar::meter_fraction(&weekly_only) - 0.24).abs() < 1e-6);

        assert_eq!(StatusBar::meter_fraction(&ProviderUsage::default()), 0.0);
    }

    /// The pill meter is drawn only beside real numbers: a provider that is
    /// loading or unavailable has nothing to fill it with, and an empty
    /// track would read as "0% used".
    #[gpui::test]
    async fn the_meter_renders_beside_numbers_and_not_beside_a_reason(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(Theme::init);
        let window = cx.add_window(|_window, _cx| StatusBar::new_with_default_context());
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        let bar = cx.update(|window, _cx| {
            window
                .root::<StatusBar>()
                .flatten()
                .expect("status bar root")
        });
        bar.update(&mut cx, |bar, cx| {
            let usage = ProviderUsage {
                session: Some(UsageWindow::new("5h", 3)),
                weekly: Some(UsageWindow::new("wk", 24)),
                monthly: None,
                fable_weekly: None,
            };
            bar.apply_outcomes(
                UsageFetchOutcome::Success(usage),
                UsageFetchOutcome::Unavailable(UsageReason::LoggedOut),
                UsageFetchOutcome::Unavailable(UsageReason::LoggedOut),
                UsageFetchOutcome::Unavailable(UsageReason::LoggedOut),
                cx,
            );
        });
        cx.run_until_parked();

        assert!(
            cx.debug_bounds("Claude-usage-meter").is_some(),
            "a loaded provider draws its pill meter"
        );
        assert!(cx.debug_bounds("Claude-usage-text").is_some());
        assert!(
            cx.debug_bounds("Codex-usage-meter").is_none(),
            "an unavailable provider draws no meter"
        );
        assert!(
            cx.debug_bounds("Codex-usage-text").is_none(),
            "an unavailable provider leaves no text in the bar either"
        );
    }

    /// The bar only ever shows numbers: a provider that had them and then
    /// stopped reporting (logged out, API refused, not installed) is
    /// removed from the bar, not replaced by a reason. Nothing about an
    /// error is rendered here.
    #[gpui::test]
    async fn a_provider_that_stops_reporting_is_removed_from_the_bar(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(Theme::init);
        let window = cx.add_window(|_window, _cx| StatusBar::new_with_default_context());
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        let bar = cx.update(|window, _cx| {
            window
                .root::<StatusBar>()
                .flatten()
                .expect("status bar root")
        });
        let usage = ProviderUsage {
            session: Some(UsageWindow::new("5h", 12)),
            weekly: None,
            monthly: None,
            fable_weekly: None,
        };
        bar.update(&mut cx, |bar, cx| {
            bar.apply_outcomes(
                UsageFetchOutcome::Success(usage.clone()),
                UsageFetchOutcome::Success(usage),
                UsageFetchOutcome::Unavailable(UsageReason::LoggedOut),
                UsageFetchOutcome::Unavailable(UsageReason::LoggedOut),
                cx,
            );
        });
        cx.run_until_parked();
        assert!(cx.debug_bounds("Claude-usage-text").is_some());
        assert!(cx.debug_bounds("Codex-usage-text").is_some());

        bar.update(&mut cx, |bar, cx| {
            bar.apply_outcomes(
                UsageFetchOutcome::Unavailable(UsageReason::LoggedOut),
                UsageFetchOutcome::Unavailable(UsageReason::ApiKey),
                UsageFetchOutcome::Unavailable(UsageReason::LoggedOut),
                UsageFetchOutcome::Unavailable(UsageReason::LoggedOut),
                cx,
            );
        });
        cx.run_until_parked();
        assert!(
            cx.debug_bounds("Claude-usage-text").is_none(),
            "a provider that stopped reporting is gone from the bar, not narrated"
        );
        assert!(
            cx.debug_bounds("Codex-usage-text").is_none(),
            "an API-key account has no usage to show and says nothing"
        );
        assert!(
            cx.debug_bounds("sirio-status-bar").is_some(),
            "the bar's own controls stay: only the usage segments are absent"
        );
    }

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

        // Only numbers render a segment, so every provider is given some
        // before the visibility toggles are exercised.
        let bar = cx.update(|window, _cx| {
            window
                .root::<StatusBar>()
                .flatten()
                .expect("status bar root")
        });
        bar.update(&mut cx, |bar, cx| {
            let usage = ProviderUsage {
                session: Some(UsageWindow::new("5h", 12)),
                weekly: None,
                monthly: None,
                fable_weekly: None,
            };
            bar.apply_outcomes(
                UsageFetchOutcome::Success(usage.clone()),
                UsageFetchOutcome::Success(usage.clone()),
                UsageFetchOutcome::Success(usage.clone()),
                UsageFetchOutcome::Success(usage),
                cx,
            );
        });
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

    /// A provider without numbers leaves no text in the bar: not while
    /// loading, and not for any unavailable reason. "logged out", "API
    /// key", "not supported here" and the rest were the bar's only error
    /// vocabulary, and the bar is not the place for it — the reason stays a
    /// distinct fact in `UsageReason` for a surface that explains; here the
    /// segment is simply absent. Only real numbers earn a segment, and
    /// stale numbers are still numbers (dimming marks them, not absence).
    #[test]
    fn a_state_without_numbers_renders_no_segment_text() {
        assert_eq!(StatusBar::segment_text(&ProviderUsageState::Loading), None);
        for reason in [
            UsageReason::NotInstalled,
            UsageReason::LoggedOut,
            UsageReason::ApiKey,
            UsageReason::TokenReused,
            UsageReason::TokenRevoked,
            UsageReason::TokenExpired,
            UsageReason::TimedOut,
            UsageReason::Unsupported,
            UsageReason::Error,
        ] {
            assert_eq!(
                StatusBar::segment_text(&ProviderUsageState::Unavailable(reason)),
                None,
                "{reason:?} must leave the bar empty"
            );
        }
        assert_eq!(
            StatusBar::segment_text(&ProviderUsageState::Loaded(ProviderUsage::default())),
            None,
            "a success that read no window has nothing to show either"
        );
        let usage = ProviderUsage {
            session: Some(UsageWindow::new("5h", 12)),
            weekly: None,
            monthly: None,
            fable_weekly: None,
        };
        assert_eq!(
            StatusBar::segment_text(&ProviderUsageState::Loaded(usage.clone())),
            Some("12% used 5h".to_string())
        );
        assert_eq!(
            StatusBar::segment_text(&ProviderUsageState::Stale(usage)),
            Some("12% used 5h".to_string()),
            "stale keeps showing the last good numbers — dimming is what marks it stale, not absence"
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
    #[test]
    fn update_indicator_requires_an_enabled_available_update() {
        let available = UpdateState {
            enabled: true,
            channel: "stable".into(),
            status: UpdateStatus::Available {
                version: "0.7.0".into(),
                notes: "notes".into(),
            },
            last_checked: Some("10 minutes ago".into()),
        };
        assert!(StatusBar::update_indicator_visible(&available));

        let disabled = UpdateState {
            enabled: false,
            ..available.clone()
        };
        assert!(!StatusBar::update_indicator_visible(&disabled));

        let checking = UpdateState {
            status: UpdateStatus::Checking,
            ..available
        };
        assert!(!StatusBar::update_indicator_visible(&checking));
    }

    /// Ticket #316: the indicator is a quiet status-bar affordance. Its
    /// click only reaches the host's Settings callback; it does not invoke
    /// an updater operation in this crate.
    #[gpui::test]
    async fn available_update_indicator_opens_host_settings_callback(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(Theme::init);
        let settings_calls = std::rc::Rc::new(std::cell::RefCell::new(0));
        let spy = settings_calls.clone();
        let update = UpdateState {
            enabled: true,
            channel: "stable".into(),
            status: UpdateStatus::Available {
                version: "0.7.0".into(),
                notes: String::new(),
            },
            last_checked: Some("now".into()),
        };
        let window = cx.add_window(|_window, _cx| {
            StatusBar::new_with_default_context()
                .with_update_state(update)
                .on_settings(move || *spy.borrow_mut() += 1)
        });
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        let indicator = cx
            .debug_bounds("status-update-indicator")
            .expect("available update draws the status indicator");
        cx.simulate_click(indicator.center(), Modifiers::none());
        cx.run_until_parked();
        assert_eq!(*settings_calls.borrow(), 1);
    }

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
