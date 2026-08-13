//! The fixed-height usage/status strip below the workspace columns.
//!
//! The usage segment is real data: the bar owns a Claude
//! usage fetch that runs on the background executor (never the render
//! thread), refreshes on a fixed interval, and can be triggered manually
//! from the refresh button. A provider that is missing, unreadable or
//! malformed renders as "—"; a timed-out refresh keeps the last good
//! numbers visibly dimmed rather than showing them as current.

use gpui::{Context, Render, Rgba, Window, div, prelude::*, px, text};
use std::rc::Rc;
use std::time::Duration;
use tiller_theme::Theme;
use tiller_usage::{
    ClaudeUsageFetcher, CodexUsageFetcher, OpenCodeGoUsageFetcher, ProviderUsageState,
    UsageFetchOutcome, reduce,
};

use crate::sidebar::icons::{Icon, IconElement};

pub(crate) const HEIGHT: f32 = 40.0;

/// The Swift default: refresh every five minutes (60..3600 allowed).
const REFRESH_INTERVAL: Duration = Duration::from_secs(300);

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
            refresh_task_started: false,
            on_settings: None,
            on_refresh: None,
        }
    }

    pub fn new_with_default_context() -> Self {
        Self::new(UsageBarData::default_context())
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
    /// thread never blocks.
    fn ensure_refresh_task(&mut self, cx: &mut Context<Self>) {
        if self.refresh_task_started {
            return;
        }
        self.refresh_task_started = true;
        let interval = self.refresh_interval;
        cx.spawn(async move |this, cx| {
            loop {
                let executor = cx.background_executor();
                let claude = executor.spawn(async move { ClaudeUsageFetcher::fetch() });
                let codex = executor.spawn(async move { CodexUsageFetcher::fetch() });
                let opencode_go = executor.spawn(async move { OpenCodeGoUsageFetcher::fetch() });
                let (claude, codex, opencode_go) = (claude.await, codex.await, opencode_go.await);
                if this
                    .update(cx, |bar, cx| {
                        bar.apply_outcomes(claude, codex, opencode_go, cx);
                    })
                    .is_err()
                {
                    return;
                }
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
            ProviderUsageState::Unavailable(_) => format!("{display_name} —"),
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
                    .flex()
                    .items_center()
                    .gap(px(5.0))
                    .text_size(theme.typography.caption2)
                    .text_color(text_color)
                    .child(IconElement::new(mark, px(12.0)))
                    .child(text!(id = text_id, text))
            };

        let left = div()
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
            .child(provider_segment(
                "Claude",
                Icon::ClaudeCode,
                claude_color,
                Self::segment_text("Claude", &self.claude),
            ))
            .child(provider_segment(
                "Codex",
                Icon::Codex,
                codex_color,
                Self::segment_text("Codex", &self.codex),
            ))
            .child(provider_segment(
                "OpenCode Go",
                Icon::OpenCode,
                opencode_go_color,
                Self::segment_text("OpenCode Go", &self.opencode_go),
            ));

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
