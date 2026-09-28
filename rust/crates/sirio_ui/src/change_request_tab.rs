//! One change request, read-only, in the Secondary half of the centre split
//! (spec §7.2, mockup A): a fixed header, then Conversation, Commits, Checks
//! and Files. Its identity is a `ChangeRef`, which is what the host keys the
//! tab by and what the session store keeps.

use std::path::Path;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use gpui::{
    AnyElement, App, Context, Entity, EventEmitter, FontWeight, Hsla, IntoElement, Render, Task,
    Window, div, prelude::*, px,
};
use sirio_forge::{
    ChangeHeader, ChangeRef, Check, CheckStatus, CiState, CommitSummary, EventKind, FileChange,
    FileChangeKind, Forge, ForgeClient, ForgeError, Listing, ReviewOutcome, TimelineItem,
};
use sirio_theme::Theme;

use crate::change_request_style as style;
use crate::chat::{Chat, LinkClickOverride};
use crate::forge_source::{self, Connection};
use crate::sidebar::icons::{Icon, IconElement, IconSize};
use crate::text_selection::selectable_text;

/// The inner tab a change request shows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum InnerTab {
    #[default]
    Conversation,
    Commits,
    Checks,
    Files,
}

impl InnerTab {
    pub const ALL: [Self; 4] = [Self::Conversation, Self::Commits, Self::Checks, Self::Files];

    /// The session store's spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Conversation => "conversation",
            Self::Commits => "commits",
            Self::Checks => "checks",
            Self::Files => "files",
        }
    }

    /// Anything unknown — a session written before, or by a later build —
    /// opens on the conversation.
    pub fn parse(value: &str) -> Self {
        match value {
            "commits" => Self::Commits,
            "checks" => Self::Checks,
            "files" => Self::Files,
            _ => Self::Conversation,
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Self::Conversation => "Conversation",
            Self::Commits => "Commits",
            Self::Checks => "Checks",
            Self::Files => "Files",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChangeRequestTabEvent {
    /// The title the forge now reports; the host's tab strip follows it.
    TitleChanged(String),
    /// A commit was clicked: the host opens it locally when the object is in
    /// the repository, otherwise on the forge.
    OpenCommit { sha: String, web_url: String },
    /// The user closed a tab that can no longer reach its change request.
    Close,
}

/// One piece of what the tab shows.
pub(crate) enum Slot<T> {
    Idle,
    Loading,
    /// Loaded. `stale` is a later refresh's failure, shown as a line above
    /// the data instead of in place of it (spec §10).
    Loaded {
        value: T,
        stale: Option<ForgeError>,
    },
    Failed(ForgeError),
}

impl<T> Slot<T> {
    pub(crate) fn value(&self) -> Option<&T> {
        match self {
            Self::Loaded { value, .. } => Some(value),
            _ => None,
        }
    }

    fn begin(&mut self) {
        if !matches!(self, Self::Loaded { .. }) {
            *self = Self::Loading;
        }
    }

    fn finish(&mut self, result: Result<T, ForgeError>) {
        *self = match (result, std::mem::replace(self, Self::Idle)) {
            (Ok(value), _) => Self::Loaded { value, stale: None },
            (Err(error), Self::Loaded { value, .. }) => Self::Loaded {
                value,
                stale: Some(error),
            },
            (Err(error), _) => Self::Failed(error),
        };
    }
}

/// A selected tab refreshes when chosen, unless it did just now.
const FRESH_FOR: Duration = Duration::from_secs(10);
/// While its CI runs, a tab refreshes on its own at this pace (spec §9).
const CI_REFRESH: Duration = Duration::from_secs(60);

pub struct ChangeRequestTab {
    reference: ChangeRef,
    title: String,
    inner: InnerTab,
    started: bool,
    client: Option<Arc<ForgeClient>>,
    /// Why there is no client — shown with Retry and Close; the tab is never
    /// dropped on its own (spec §8).
    pub(crate) unreachable: Option<String>,
    pub(crate) header: Slot<ChangeHeader>,
    pub(crate) description: Option<markdown::Doc>,
    /// The Markdown of each timeline entry that has a body, aligned with it.
    bodies: Vec<Option<markdown::Doc>>,
    pub(crate) commits: Slot<Listing<CommitSummary>>,
    pub(crate) checks: Slot<Listing<Check>>,
    pub(crate) files: Slot<Listing<FileChange>>,
    show_settled_checks: bool,
    generation: u64,
    /// A rate limit's reset: no request before it (spec §9).
    paused_until: Option<i64>,
    last_refresh: Option<Instant>,
    connect_task: Option<Task<()>>,
    header_task: Option<Task<()>>,
    commits_task: Option<Task<()>>,
    checks_task: Option<Task<()>>,
    files_task: Option<Task<()>>,
    ci_timer: Option<Task<()>>,
}

impl ChangeRequestTab {
    pub fn new(reference: ChangeRef, title: String, cx: &mut Context<Self>) -> Self {
        Self::restored(reference, title, InnerTab::Conversation, cx)
    }

    /// A tab brought back from the session: it shows its saved title at once
    /// and loads when it is shown (spec §8).
    pub fn restored(
        reference: ChangeRef,
        title: String,
        inner: InnerTab,
        cx: &mut Context<Self>,
    ) -> Self {
        let _ = cx;
        Self {
            reference,
            title,
            inner,
            started: false,
            client: None,
            unreachable: None,
            header: Slot::Idle,
            description: None,
            bodies: Vec::new(),
            commits: Slot::Idle,
            checks: Slot::Idle,
            files: Slot::Idle,
            show_settled_checks: false,
            generation: 0,
            paused_until: None,
            last_refresh: None,
            connect_task: None,
            header_task: None,
            commits_task: None,
            checks_task: None,
            files_task: None,
            ci_timer: None,
        }
    }

    pub fn reference(&self) -> &ChangeRef {
        &self.reference
    }

    pub fn inner_tab(&self) -> InnerTab {
        self.inner
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    /// The strip title: `#578 Fix the login redirect`, or the label alone.
    pub fn tab_title(reference: &ChangeRef, title: &str) -> String {
        if title.is_empty() {
            reference.label()
        } else {
            format!("{} {title}", reference.label())
        }
    }

    /// The bare title inside a saved strip title.
    pub fn title_from_tab(reference: &ChangeRef, tab_title: &str) -> String {
        tab_title
            .strip_prefix(&reference.label())
            .map(str::trim)
            .unwrap_or(tab_title)
            .to_string()
    }

    /// The host calls this whenever the tab is chosen.
    pub fn on_selected(&mut self, cx: &mut Context<Self>) {
        if !self.started {
            self.connect(cx);
        } else if self.last_refresh.is_none_or(|at| at.elapsed() > FRESH_FOR) {
            self.refresh(cx);
        }
    }

    fn connect(&mut self, cx: &mut Context<Self>) {
        self.started = true;
        let Some(source) = forge_source::source(cx) else {
            self.unreachable = Some("Change requests are unavailable in this build.".to_string());
            cx.notify();
            return;
        };
        self.unreachable = None;
        let reference = self.reference.clone();
        self.connect_task = Some(cx.spawn(async move |this, cx| {
            let answer = cx
                .background_spawn(async move { source.client_for(&reference) })
                .await;
            let _ = this.update(cx, |tab, cx| match answer {
                Ok(client) => {
                    tab.client = Some(client);
                    tab.refresh(cx);
                }
                Err(connection) => {
                    tab.unreachable = Some(unreachable_text(&connection));
                    cx.notify();
                }
            });
        }));
        cx.notify();
    }

    /// Forget what the host resolved for this forge, and try again.
    pub(crate) fn retry(&mut self, cx: &mut Context<Self>) {
        if let Some(source) = forge_source::source(cx) {
            source.forget(&self.reference.host);
        }
        self.client = None;
        self.connect(cx);
    }

    /// While a rate limit's reset is in the future, no path asks the forge.
    fn rate_paused(&mut self) -> bool {
        if let Some(until) = self.paused_until {
            if style::now() < until {
                return true;
            }
            self.paused_until = None;
        }
        false
    }

    fn note_rate_limited(&mut self, error: &ForgeError) {
        if let ForgeError::RateLimited {
            reset_at: Some(reset),
            ..
        } = error
        {
            self.paused_until = Some(*reset);
        }
    }

    pub fn refresh(&mut self, cx: &mut Context<Self>) {
        if self.rate_paused() {
            return;
        }
        let Some(client) = self.client.clone() else {
            self.connect(cx);
            return;
        };
        self.generation += 1;
        let generation = self.generation;
        self.last_refresh = Some(Instant::now());
        self.header.begin();
        let number = self.reference.number;
        let header_client = client.clone();
        self.header_task = Some(cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move { header_client.header(number) })
                .await;
            let _ = this.update(cx, |tab, cx| {
                if tab.generation == generation {
                    tab.apply_header(result, cx);
                }
            });
        }));
        if self.inner != InnerTab::Conversation {
            self.load_inner(self.inner, cx);
        }
        cx.notify();
    }

    fn apply_header(&mut self, result: Result<ChangeHeader, ForgeError>, cx: &mut Context<Self>) {
        if let Ok(header) = &result {
            if header.summary.title != self.title {
                self.title = header.summary.title.clone();
                cx.emit(ChangeRequestTabEvent::TitleChanged(self.title.clone()));
            }
            let theme = *Theme::get(cx);
            self.description = Some(markdown_doc(&header.body, &theme));
            self.bodies = header
                .timeline
                .iter()
                .map(|item| {
                    body_of(item)
                        .filter(|body| !body.trim().is_empty())
                        .map(|body| markdown_doc(body, &theme))
                })
                .collect();
            self.schedule_ci_refresh(header.summary.ci, cx);
        }
        if let Err(error) = &result {
            self.note_rate_limited(error);
        }
        self.header.finish(result);
        cx.notify();
    }

    /// While CI runs, look again in a minute; otherwise stop (spec §9).
    fn schedule_ci_refresh(&mut self, ci: CiState, cx: &mut Context<Self>) {
        if !matches!(ci, CiState::Running(_)) {
            self.ci_timer = None;
            return;
        }
        if self.ci_timer.is_some() {
            return;
        }
        self.ci_timer = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(CI_REFRESH).await;
            let _ = this.update(cx, |tab, cx| {
                tab.ci_timer = None;
                tab.refresh(cx);
            });
        }));
    }

    pub fn select_inner(&mut self, inner: InnerTab, cx: &mut Context<Self>) {
        self.inner = inner;
        let idle = match inner {
            InnerTab::Conversation => false,
            InnerTab::Commits => matches!(self.commits, Slot::Idle),
            InnerTab::Checks => matches!(self.checks, Slot::Idle),
            InnerTab::Files => matches!(self.files, Slot::Idle),
        };
        if idle {
            self.load_inner(inner, cx);
        }
        cx.notify();
    }

    fn load_inner(&mut self, inner: InnerTab, cx: &mut Context<Self>) {
        if self.rate_paused() {
            return;
        }
        let Some(client) = self.client.clone() else {
            return;
        };
        let number = self.reference.number;
        let generation = self.generation;
        match inner {
            InnerTab::Conversation => {}
            InnerTab::Commits => {
                self.commits.begin();
                self.commits_task = Some(cx.spawn(async move |this, cx| {
                    let result = cx
                        .background_spawn(async move { client.commits(number) })
                        .await;
                    let _ = this.update(cx, |tab, cx| {
                        if tab.generation == generation {
                            if let Err(error) = &result {
                                tab.note_rate_limited(error);
                            }
                            tab.commits.finish(result);
                            cx.notify();
                        }
                    });
                }));
            }
            InnerTab::Checks => {
                self.checks.begin();
                self.checks_task = Some(cx.spawn(async move |this, cx| {
                    let result = cx
                        .background_spawn(async move { client.checks(number) })
                        .await;
                    let _ = this.update(cx, |tab, cx| {
                        if tab.generation == generation {
                            if let Err(error) = &result {
                                tab.note_rate_limited(error);
                            }
                            tab.checks.finish(result);
                            cx.notify();
                        }
                    });
                }));
            }
            InnerTab::Files => {
                self.files.begin();
                self.files_task = Some(cx.spawn(async move |this, cx| {
                    let result = cx
                        .background_spawn(async move { client.files(number) })
                        .await;
                    let _ = this.update(cx, |tab, cx| {
                        if tab.generation == generation {
                            if let Err(error) = &result {
                                tab.note_rate_limited(error);
                            }
                            tab.files.finish(result);
                            cx.notify();
                        }
                    });
                }));
            }
        }
    }

    /// The strip's glyph colour, once the state is known.
    pub fn state_color(&self, theme: &Theme) -> Option<Hsla> {
        self.header
            .value()
            .map(|header| style::state_color(header.summary.state, theme))
    }

    /// What the control socket reports about this tab (Task 9).
    pub fn report(&self) -> Vec<(String, String)> {
        let (state, rows) = if self.unreachable.is_some() {
            ("unreachable", 0)
        } else {
            let rows = |slot_rows: Option<usize>| slot_rows.unwrap_or(0);
            match self.inner {
                InnerTab::Conversation => (
                    slot_state(&self.header),
                    rows(self.header.value().map(|h| h.timeline.len())),
                ),
                InnerTab::Commits => (
                    slot_state(&self.commits),
                    rows(self.commits.value().map(|l| l.items.len())),
                ),
                InnerTab::Checks => (
                    slot_state(&self.checks),
                    rows(self.checks.value().map(|l| l.items.len())),
                ),
                InnerTab::Files => (
                    slot_state(&self.files),
                    rows(self.files.value().map(|l| l.items.len())),
                ),
            }
        };
        vec![
            ("label".to_string(), self.reference.label()),
            ("title".to_string(), self.title.clone()),
            ("inner".to_string(), self.inner.as_str().to_string()),
            ("state".to_string(), state.to_string()),
            ("rows".to_string(), rows.to_string()),
        ]
    }
}

fn slot_state<T>(slot: &Slot<T>) -> &'static str {
    match slot {
        Slot::Idle | Slot::Loading => "loading",
        Slot::Loaded { .. } => "loaded",
        Slot::Failed(_) => "error",
    }
}

fn unreachable_text(connection: &Connection) -> String {
    match connection {
        Connection::NotConnected { forge, host } => format!(
            "Not signed in to {host}. Sign in with `{} auth login --hostname {host}`, or add a token in Settings → Git Hosting.",
            style::cli_name(*forge)
        ),
        Connection::UnknownForge { host } => {
            format!(
                "Sirio does not know which forge {host} runs; say which in the right panel's change request view."
            )
        }
        Connection::NoForgeRemote | Connection::Ready(_) => {
            "This change request's project cannot be reached.".to_string()
        }
    }
}

fn body_of(item: &TimelineItem) -> Option<&str> {
    match item {
        TimelineItem::Comment { body, .. } | TimelineItem::Review { body, .. } => Some(body),
        TimelineItem::LineComment(comment) => Some(&comment.body),
        TimelineItem::Event { .. } => None,
    }
}

/// A forge body through the Preview's Markdown path, `expand_html` subset
/// included, with its images turned into text (see [`images_as_text`]).
fn markdown_doc(body: &str, theme: &Theme) -> markdown::Doc {
    let preview = crate::markdown_preview::build(
        sirio_markdown::parse(body),
        Path::new(""),
        &crate::markdown_preview::Diagrams::default(),
        &crate::markdown_preview::palette(theme),
    );
    let mut doc = preview.doc;
    images_as_text(&mut doc);
    doc
}

/// Sirio installs no HTTP client for GPUI, so a remote picture draws as an
/// empty box (checked 2026-09-27), and a forge body's relative picture has
/// no file behind it here. Each picture becomes its alt text (or "image"),
/// a link when it is remote.
pub(crate) fn images_as_text(doc: &mut markdown::Doc) {
    for block in &mut doc.blocks {
        let markdown::BlockKind::Image { url, alt, .. } = &block.kind else {
            continue;
        };
        let label = if alt.text.trim().is_empty() {
            "image".to_string()
        } else {
            alt.text.clone()
        };
        let mut text = markdown::Text::plain(label.clone());
        if url.contains("://") {
            text.marks.push(markdown::MarkSpan {
                range: 0..label.len(),
                mark: markdown::Mark::Link(url.clone()),
            });
        }
        block.kind = markdown::BlockKind::Paragraph(text);
    }
}

fn open_links() -> LinkClickOverride {
    Rc::new(|url, _window, cx| cx.open_url(url))
}

fn button(
    id: &'static str,
    icon: Icon,
    label: Option<&'static str>,
    theme: &Theme,
    on_click: impl Fn(&mut App) + 'static,
) -> gpui::Stateful<gpui::Div> {
    let hover = theme.element_hover;
    div()
        .id(id)
        .debug_selector(move || id.to_owned())
        .flex()
        .flex_none()
        .items_center()
        .gap(px(4.0))
        .px(px(8.0))
        .py(px(4.0))
        .rounded(theme.radii.control)
        .text_size(theme.typography.footnote)
        .text_color(theme.text_muted)
        .cursor_pointer()
        .hover(move |style| style.bg(hover))
        .on_click(move |_, _, cx| on_click(cx))
        .child(IconElement::new(icon, IconSize::Small))
        .when_some(label, |this, label| this.child(label))
}

fn badge(label: &'static str, color: Hsla, theme: &Theme) -> impl IntoElement {
    div()
        .flex_none()
        .px(px(8.0))
        .py(px(1.0))
        .rounded(px(999.0))
        .text_size(theme.typography.caption2)
        .font_weight(FontWeight::MEDIUM)
        .text_color(color)
        .bg(color.opacity(0.14))
        .child(label)
}

fn error_panel(
    message: String,
    theme: &Theme,
    retry: Rc<dyn Fn(&mut App)>,
    close: Option<Rc<dyn Fn(&mut App)>>,
) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .items_center()
        .gap(theme.spacing.card_gap)
        .p(theme.spacing.card_gap)
        .child(
            div()
                .text_color(theme.danger)
                .child(selectable_text(message)),
        )
        .child(
            div()
                .flex()
                .gap(px(8.0))
                .child(button(
                    "change-request-retry",
                    Icon::RefreshCw,
                    Some("Retry"),
                    theme,
                    move |cx| retry(cx),
                ))
                .when_some(close, |this, close| {
                    this.child(button(
                        "change-request-close",
                        Icon::Close,
                        Some("Close"),
                        theme,
                        move |cx| close(cx),
                    ))
                }),
        )
        .into_any_element()
}

fn stale_line(message: String, theme: &Theme, retry: Rc<dyn Fn(&mut App)>) -> AnyElement {
    div()
        .flex()
        .items_center()
        .gap(px(8.0))
        .text_size(theme.typography.footnote)
        .text_color(theme.danger)
        .child(
            div()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .text_ellipsis()
                .child(format!("Refresh failed · {message}")),
        )
        .child(button(
            "change-request-stale-retry",
            Icon::RefreshCw,
            Some("Retry"),
            theme,
            move |cx| retry(cx),
        ))
        .into_any_element()
}

fn slot_view<T>(
    slot: &Slot<T>,
    what: &'static str,
    theme: &Theme,
    retry: Rc<dyn Fn(&mut App)>,
    close: Option<Rc<dyn Fn(&mut App)>>,
    loaded: impl FnOnce(&T) -> AnyElement,
) -> AnyElement {
    match slot {
        Slot::Idle | Slot::Loading => div()
            .text_color(theme.text_faint)
            .child(format!("Loading {what}…"))
            .into_any_element(),
        Slot::Failed(error) => error_panel(error.to_string(), theme, retry, close),
        Slot::Loaded { value, stale } => div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .when_some(stale.as_ref(), |this, error| {
                this.child(stale_line(error.to_string(), theme, retry.clone()))
            })
            .child(loaded(value))
            .into_any_element(),
    }
}

impl EventEmitter<ChangeRequestTabEvent> for ChangeRequestTab {}

impl ChangeRequestTab {
    fn retry_handle(entity: &Entity<Self>) -> Rc<dyn Fn(&mut App)> {
        let entity = entity.clone();
        Rc::new(move |cx| entity.update(cx, |tab, cx| tab.refresh(cx)))
    }

    fn close_handle(entity: &Entity<Self>) -> Rc<dyn Fn(&mut App)> {
        let entity = entity.clone();
        Rc::new(move |cx| entity.update(cx, |_, cx| cx.emit(ChangeRequestTabEvent::Close)))
    }

    fn render_header(&self, theme: &Theme, entity: &Entity<Self>) -> impl IntoElement {
        let summary = self.header.value().map(|header| &header.summary);
        let web_url = summary.map(|summary| summary.web_url.clone());
        let refresh = entity.clone();
        div()
            .flex()
            .flex_col()
            .gap(px(6.0))
            .px(px(16.0))
            .pt(px(12.0))
            .pb(px(10.0))
            .border_b_1()
            .border_color(theme.border)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .items_baseline()
                            .gap(px(6.0))
                            .child(
                                div()
                                    .debug_selector(|| "change-request-title".into())
                                    .min_w_0()
                                    .overflow_hidden()
                                    .text_ellipsis()
                                    .whitespace_nowrap()
                                    .text_size(theme.typography.title3)
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(selectable_text(self.title.clone())),
                            )
                            .child(
                                div()
                                    .flex_none()
                                    .text_color(theme.text_faint)
                                    .child(selectable_text(self.reference.label())),
                            ),
                    )
                    .when_some(web_url, |this, url| {
                        let forge = self.reference.forge;
                        this.child(
                            button(
                                "change-request-open-browser",
                                style::forge_mark(forge),
                                Some("Open in browser"),
                                theme,
                                move |cx| cx.open_url(&url),
                            )
                            .child(IconElement::new(Icon::ArrowUpRight, IconSize::XSmall)),
                        )
                    })
                    .child(button(
                        "change-request-refresh",
                        Icon::RefreshCw,
                        None,
                        theme,
                        move |cx| refresh.update(cx, |tab, cx| tab.refresh(cx)),
                    )),
            )
            .when_some(summary, |this, summary| {
                let meta = format!(
                    "{} · {} → {} · updated {}",
                    summary.author,
                    summary.source_branch,
                    summary.target_branch,
                    style::age(style::now(), summary.updated_at)
                );
                this.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .text_size(theme.typography.footnote)
                        .text_color(theme.text_muted)
                        .child(badge(
                            style::state_label(summary.state),
                            style::state_color(summary.state, theme),
                            theme,
                        ))
                        .child(
                            div()
                                .debug_selector(|| "change-request-meta".into())
                                .min_w_0()
                                .overflow_hidden()
                                .text_ellipsis()
                                .whitespace_nowrap()
                                .child(selectable_text(meta)),
                        ),
                )
            })
    }

    fn inner_count(&self, inner: InnerTab) -> Option<String> {
        let header = self.header.value()?;
        match inner {
            InnerTab::Conversation => Some(header.summary.comments.to_string()),
            InnerTab::Commits => header.commit_count.map(|count| count.to_string()),
            InnerTab::Checks => match header.summary.ci {
                CiState::Running(Some(progress)) => {
                    Some(format!("{}/{}", progress.done, progress.total))
                }
                _ => self
                    .checks
                    .value()
                    .map(|listing| listing.items.len().to_string()),
            },
            InnerTab::Files => header.changed_files.map(|count| count.to_string()),
        }
    }

    fn render_inner_strip(&self, theme: &Theme, entity: &Entity<Self>) -> impl IntoElement {
        let ci = self.header.value().map(|header| header.summary.ci);
        let hover = theme.element_hover;
        div()
            .id("change-request-inner-tabs")
            .h(px(32.0))
            .px(px(10.0))
            .flex()
            .items_center()
            .gap(px(2.0))
            .border_b_1()
            .border_color(theme.border)
            .children(InnerTab::ALL.into_iter().map(|inner| {
                let active = inner == self.inner;
                let tone = if active { theme.text } else { theme.text_muted };
                let (icon, tint): (Icon, Hsla) = match inner {
                    InnerTab::Conversation => (Icon::MessageSquare, tone.into()),
                    InnerTab::Commits => (Icon::GitCommit, tone.into()),
                    InnerTab::Checks => ci
                        .and_then(|ci| style::ci_mark(ci, theme))
                        .unwrap_or((Icon::Circle, tone.into())),
                    InnerTab::Files => (Icon::File, tone.into()),
                };
                let id = match inner {
                    InnerTab::Conversation => "change-request-inner-conversation",
                    InnerTab::Commits => "change-request-inner-commits",
                    InnerTab::Checks => "change-request-inner-checks",
                    InnerTab::Files => "change-request-inner-files",
                };
                let entity = entity.clone();
                div()
                    .id(id)
                    .debug_selector(move || id.to_owned())
                    .relative()
                    .h_full()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .px(px(8.0))
                    .text_size(theme.typography.footnote)
                    .font_weight(if active {
                        FontWeight::MEDIUM
                    } else {
                        FontWeight::NORMAL
                    })
                    .text_color(tone)
                    .cursor_pointer()
                    .hover(move |style| style.bg(hover))
                    .on_click(move |_, _, cx| {
                        entity.update(cx, |tab, cx| tab.select_inner(inner, cx))
                    })
                    .child(IconElement::new(icon, IconSize::Small).text_color(tint))
                    .child(inner.title())
                    .when_some(self.inner_count(inner), |this, count| {
                        this.child(
                            div()
                                .px(px(6.0))
                                .rounded(px(999.0))
                                .bg(theme.element_hover)
                                .text_size(theme.typography.caption2)
                                .text_color(theme.text_muted)
                                .child(count),
                        )
                    })
                    .when(active, |this| {
                        this.child(
                            div()
                                .absolute()
                                .bottom(px(-1.0))
                                .left_0()
                                .right_0()
                                .h(px(2.0))
                                .bg(theme.text),
                        )
                    })
            }))
    }

    fn render_conversation(&self, theme: &Theme, entity: &Entity<Self>) -> AnyElement {
        slot_view(
            &self.header,
            "the conversation",
            theme,
            Self::retry_handle(entity),
            Some(Self::close_handle(entity)),
            |header| {
                let mut column = div().flex().flex_col().gap(px(12.0));
                if header.timeline_truncated {
                    let url = header.summary.web_url.clone();
                    column = column.child(button(
                        "change-request-earlier",
                        Icon::ArrowUpRight,
                        Some("Earlier activity is on the forge"),
                        theme,
                        move |cx| cx.open_url(&url),
                    ));
                }
                if let Some(doc) = self
                    .description
                    .clone()
                    .filter(|doc| !doc.blocks.is_empty())
                {
                    column = column.child(
                        div()
                            .debug_selector(|| "change-request-description".into())
                            .p(px(12.0))
                            .rounded(theme.radii.control)
                            .border_1()
                            .border_color(theme.border)
                            .bg(theme.surface)
                            .child(Chat::render_markdown_document_with_link_override(
                                doc,
                                theme,
                                open_links(),
                            )),
                    );
                }
                for (index, item) in header.timeline.iter().enumerate() {
                    column = column.child(self.render_timeline_item(index, item, theme));
                }
                column.into_any_element()
            },
        )
    }

    fn render_timeline_item(&self, index: usize, item: &TimelineItem, theme: &Theme) -> AnyElement {
        let now = style::now();
        let line = |who: String, what: String, at: Option<i64>| {
            div()
                .flex()
                .items_center()
                .gap(px(6.0))
                .text_size(theme.typography.footnote)
                .text_color(theme.text_muted)
                .child(
                    div()
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(theme.text)
                        .child(selectable_text(who)),
                )
                .child(selectable_text(what))
                .child(
                    div()
                        .text_color(theme.text_faint)
                        .child(selectable_text(style::age(now, at))),
                )
        };
        let body = self.bodies.get(index).cloned().flatten().map(|doc| {
            div()
                .pl(px(12.0))
                .border_l_2()
                .border_color(theme.border)
                .child(Chat::render_markdown_document_with_link_override(
                    doc,
                    theme,
                    open_links(),
                ))
        });
        match item {
            TimelineItem::Comment { author, at, .. } => div()
                .id(("change-request-timeline-item", index))
                .flex()
                .flex_col()
                .gap(px(4.0))
                .child(line(author.clone(), "commented".to_string(), *at))
                .when_some(body, |this, body| this.child(body))
                .into_any_element(),
            TimelineItem::Review {
                author,
                outcome,
                at,
                line_comments,
                ..
            } => {
                let verb = match outcome {
                    ReviewOutcome::Approved => "approved",
                    ReviewOutcome::ChangesRequested => "requested changes",
                    ReviewOutcome::Commented | ReviewOutcome::Other => "reviewed",
                    ReviewOutcome::Dismissed => "had a review dismissed",
                    ReviewOutcome::Requested => "was asked to review",
                };
                div()
                    .id(("change-request-timeline-item", index))
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .child(line(author.clone(), verb.to_string(), *at))
                    .when_some(body, |this, body| this.child(body))
                    .children(line_comments.iter().map(|comment| {
                        div()
                            .pl(px(12.0))
                            .text_size(theme.typography.footnote)
                            .text_color(theme.text_muted)
                            .child(selectable_text(format!(
                                "on {}:{} — {}",
                                comment.path,
                                comment
                                    .line
                                    .map_or("?".to_string(), |line| line.to_string()),
                                comment.body
                            )))
                    }))
                    .into_any_element()
            }
            TimelineItem::LineComment(comment) => div()
                .id(("change-request-timeline-item", index))
                .flex()
                .flex_col()
                .gap(px(4.0))
                .child(line(
                    comment.author.clone(),
                    format!(
                        "commented on {}:{}",
                        comment.path,
                        comment
                            .line
                            .map_or("?".to_string(), |line| line.to_string())
                    ),
                    comment.at,
                ))
                .when_some(body, |this, body| this.child(body))
                .into_any_element(),
            TimelineItem::Event { actor, kind, at } => {
                let what = match kind {
                    EventKind::CommitsPushed { count } => {
                        format!("added {count} commit{}", if *count == 1 { "" } else { "s" })
                    }
                    EventKind::ReviewRequested { reviewer } => {
                        format!("asked {reviewer} to review")
                    }
                    EventKind::Merged => "merged".to_string(),
                    EventKind::Closed => "closed".to_string(),
                    EventKind::Reopened => "reopened".to_string(),
                    EventKind::ReadyForReview => "marked it ready for review".to_string(),
                    EventKind::ConvertedToDraft => "marked it as a draft".to_string(),
                    EventKind::Other(text) => text.clone(),
                };
                line(actor.clone().unwrap_or_default(), what, *at)
                    .id(("change-request-timeline-item", index))
                    .into_any_element()
            }
        }
    }

    fn render_commits(
        &self,
        listing: &Listing<CommitSummary>,
        theme: &Theme,
        entity: &Entity<Self>,
    ) -> AnyElement {
        let now = style::now();
        let hover = theme.element_hover;
        div()
            .flex()
            .flex_col()
            .children(listing.items.iter().enumerate().map(|(index, commit)| {
                let entity = entity.clone();
                let sha = commit.sha.clone();
                let url = commit.web_url.clone();
                div()
                    .id(("change-request-commit", index))
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .px(px(6.0))
                    .py(px(5.0))
                    .rounded(theme.radii.control)
                    .cursor_pointer()
                    .hover(move |style| style.bg(hover))
                    .on_click(move |_, _, cx| {
                        entity.update(cx, |_, cx| {
                            cx.emit(ChangeRequestTabEvent::OpenCommit {
                                sha: sha.clone(),
                                web_url: url.clone(),
                            })
                        })
                    })
                    .child(
                        IconElement::new(Icon::GitCommit, IconSize::Small)
                            .text_color(theme.text_faint),
                    )
                    .child(
                        div()
                            .flex_none()
                            .font_family(theme.typography.code_family)
                            .text_color(theme.text_muted)
                            .child(commit.short_sha.clone()),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .text_ellipsis()
                            .whitespace_nowrap()
                            .child(commit.title.clone()),
                    )
                    .child(
                        div()
                            .flex_none()
                            .text_size(theme.typography.footnote)
                            .text_color(theme.text_faint)
                            .child(format!(
                                "{} · {}",
                                commit.author,
                                style::age(now, commit.at)
                            )),
                    )
            }))
            .when(listing.truncated, |this| {
                this.child(
                    div()
                        .text_color(theme.text_faint)
                        .child("More commits are on the forge."),
                )
            })
            .into_any_element()
    }

    fn render_checks(
        &self,
        listing: &Listing<Check>,
        theme: &Theme,
        entity: &Entity<Self>,
    ) -> AnyElement {
        let rank = |status: CheckStatus| match status {
            CheckStatus::Failed => 0,
            CheckStatus::Running => 1,
            CheckStatus::Queued => 2,
            _ => 3,
        };
        let mut ordered: Vec<(usize, &Check)> = listing.items.iter().enumerate().collect();
        ordered.sort_by_key(|(index, check)| (rank(check.status), *index));
        let (open, settled): (Vec<_>, Vec<_>) = ordered
            .into_iter()
            .partition(|(_, check)| rank(check.status) < 3);
        let passed = settled
            .iter()
            .filter(|(_, check)| check.status == CheckStatus::Passed)
            .count();
        let toggle = entity.clone();
        let hover = theme.element_hover;
        let row = |index: usize, check: &Check| {
            let (icon, tint) = style::check_mark(check.status, theme);
            let url = check.url.clone();
            div()
                .id(("change-request-check", index))
                .flex()
                .items_center()
                .gap(px(8.0))
                .px(px(6.0))
                .py(px(5.0))
                .rounded(theme.radii.control)
                .when(url.is_some(), |this| {
                    this.cursor_pointer().hover(move |style| style.bg(hover))
                })
                .on_click(move |_, _, cx| {
                    if let Some(url) = &url {
                        cx.open_url(url)
                    }
                })
                .child(IconElement::new(icon, IconSize::Small).text_color(tint))
                .when_some(check.group.clone(), |this, group| {
                    this.child(div().flex_none().text_color(theme.text_faint).child(group))
                })
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .overflow_hidden()
                        .text_ellipsis()
                        .whitespace_nowrap()
                        .child(check.name.clone()),
                )
                .when_some(check.duration_secs, |this, secs| {
                    this.child(
                        div()
                            .flex_none()
                            .text_color(theme.text_faint)
                            .child(style::duration_text(secs)),
                    )
                })
        };
        div()
            .flex()
            .flex_col()
            .children(open.iter().map(|(index, check)| row(*index, check)))
            .when(!settled.is_empty(), |this| {
                this.child(
                    div()
                        .id("change-request-settled-checks")
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .px(px(6.0))
                        .py(px(5.0))
                        .cursor_pointer()
                        .text_color(theme.text_muted)
                        .on_click(move |_, _, cx| {
                            toggle.update(cx, |tab, cx| {
                                tab.show_settled_checks = !tab.show_settled_checks;
                                cx.notify();
                            })
                        })
                        .child(IconElement::new(
                            if self.show_settled_checks {
                                Icon::ChevronDown
                            } else {
                                Icon::ChevronRight
                            },
                            IconSize::Small,
                        ))
                        .child(format!("{passed} passed, {} other", settled.len() - passed)),
                )
            })
            .when(self.show_settled_checks, |this| {
                this.children(settled.iter().map(|(index, check)| row(*index, check)))
            })
            .when(listing.truncated, |this| {
                this.child(
                    div()
                        .text_color(theme.text_faint)
                        .child("More checks are on the forge."),
                )
            })
            .into_any_element()
    }

    fn render_files(&self, listing: &Listing<FileChange>, theme: &Theme) -> AnyElement {
        let files_url = self
            .header
            .value()
            .map(|header| match self.reference.forge {
                Forge::GitHub => format!("{}/files", header.summary.web_url),
                Forge::GitLab => format!("{}/diffs", header.summary.web_url),
            });
        let hover = theme.element_hover;
        div()
            .flex()
            .flex_col()
            .children(listing.items.iter().enumerate().map(|(index, file)| {
                let (letter, tint): (&str, Hsla) = match file.kind {
                    Some(FileChangeKind::Added) => ("A", theme.success.into()),
                    Some(FileChangeKind::Deleted) => ("D", theme.danger.into()),
                    Some(FileChangeKind::Renamed) => ("R", theme.accent.into()),
                    Some(FileChangeKind::Copied) => ("C", theme.accent.into()),
                    Some(FileChangeKind::Modified) => ("M", theme.warning.into()),
                    None => ("·", theme.text_faint.into()),
                };
                let url = files_url.clone();
                div()
                    .id(("change-request-file", index))
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .px(px(6.0))
                    .py(px(5.0))
                    .rounded(theme.radii.control)
                    .cursor_pointer()
                    .hover(move |style| style.bg(hover))
                    .on_click(move |_, _, cx| {
                        if let Some(url) = &url {
                            cx.open_url(url)
                        }
                    })
                    .child(
                        div()
                            .w(px(14.0))
                            .flex_none()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(tint)
                            .child(letter),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .text_ellipsis()
                            .whitespace_nowrap()
                            .child(file.path.clone()),
                    )
                    .child(
                        div()
                            .flex_none()
                            .text_color(theme.diff_add)
                            .child(format!("+{}", file.additions)),
                    )
                    .child(
                        div()
                            .flex_none()
                            .text_color(theme.diff_del)
                            .child(format!("−{}", file.deletions)),
                    )
            }))
            .when(listing.truncated, |this| {
                this.child(
                    div()
                        .text_color(theme.text_faint)
                        .child("More files are on the forge."),
                )
            })
            .into_any_element()
    }

    fn render_unreachable(
        &self,
        message: String,
        theme: &Theme,
        entity: &Entity<Self>,
    ) -> AnyElement {
        let retry = entity.clone();
        let close = entity.clone();
        div()
            .flex_1()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(theme.spacing.card_gap)
            .p(px(24.0))
            .child(
                div()
                    .max_w(px(520.0))
                    .text_color(theme.text_muted)
                    .child(selectable_text(message)),
            )
            .child(
                div()
                    .flex()
                    .gap(px(8.0))
                    .child(button(
                        "change-request-reconnect",
                        Icon::RefreshCw,
                        Some("Retry"),
                        theme,
                        move |cx| retry.update(cx, |tab, cx| tab.retry(cx)),
                    ))
                    .child(button(
                        "change-request-close",
                        Icon::Close,
                        Some("Close"),
                        theme,
                        move |cx| close.update(cx, |_, cx| cx.emit(ChangeRequestTabEvent::Close)),
                    )),
            )
            .into_any_element()
    }
}

impl Render for ChangeRequestTab {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let _perf = sirio_perf::span("ChangeRequestTab.render", cx.entity_id().as_u64());
        // A restored tab loads the first time it is drawn (spec §8).
        if !self.started {
            self.started = true;
            let this = cx.entity().downgrade();
            cx.defer(move |cx| {
                let _ = this.update(cx, |tab, cx| tab.connect(cx));
            });
        }
        let theme = *Theme::get(cx);
        let entity = cx.entity();
        let body = match self.unreachable.clone() {
            Some(message) => self.render_unreachable(message, &theme, &entity),
            None => {
                let retry = Self::retry_handle(&entity);
                let content = match self.inner {
                    InnerTab::Conversation => self.render_conversation(&theme, &entity),
                    InnerTab::Commits => {
                        slot_view(&self.commits, "commits", &theme, retry, None, |listing| {
                            self.render_commits(listing, &theme, &entity)
                        })
                    }
                    InnerTab::Checks => {
                        slot_view(&self.checks, "checks", &theme, retry, None, |listing| {
                            self.render_checks(listing, &theme, &entity)
                        })
                    }
                    InnerTab::Files => {
                        slot_view(&self.files, "files", &theme, retry, None, |listing| {
                            self.render_files(listing, &theme)
                        })
                    }
                };
                div()
                    .flex_1()
                    .min_h(px(0.0))
                    .flex()
                    .flex_col()
                    .child(self.render_inner_strip(&theme, &entity))
                    .child(
                        div()
                            .id("change-request-body")
                            .debug_selector(|| "change-request-body".to_owned())
                            .flex_1()
                            .min_h(px(0.0))
                            .overflow_y_scroll()
                            .p(px(16.0))
                            .child(content),
                    )
                    .into_any_element()
            }
        };
        div()
            .id("change-request-tab")
            .debug_selector(|| "change-request-tab".to_owned())
            .size_full()
            .flex()
            .flex_col()
            .bg(theme.bg)
            .text_color(theme.text)
            .child(self.render_header(&theme, &entity))
            .child(body)
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::sync::Arc;

    use gpui::TestAppContext;
    use sirio_forge::Forge;
    use sirio_theme::Theme;

    use super::*;
    use crate::forge_source::testing::{self, CannedForge, FakeSource};
    use crate::forge_source::{self, Connection};

    fn pump_until(cx: &TestAppContext, mut condition: impl FnMut() -> bool) {
        cx.executor().allow_parking();
        for _ in 0..600 {
            if condition() {
                return;
            }
            cx.executor()
                .advance_clock(std::time::Duration::from_millis(100));
            std::thread::sleep(std::time::Duration::from_millis(5));
            cx.run_until_parked();
        }
        panic!("condition never became true within the pump budget");
    }

    fn forge_with_header() -> Arc<CannedForge> {
        let forge = Arc::new(CannedForge::default());
        forge.answer("Viewer", testing::viewer());
        forge.answer(
            "ChangeRequestHeader",
            testing::header(101, "Fix the login redirect", "## What\n\nFixes it."),
        );
        forge.answer("ChangeRequestCommits", testing::commits());
        forge
    }

    #[test]
    fn images_become_text_and_remote_ones_links() {
        let mut doc = markdown::parse(
            "![after](https://ghe.test/a.png)\n\n![](https://ghe.test/b.png)\n\n![local](docs/c.png)\n",
        );
        images_as_text(&mut doc);
        let paragraphs: Vec<(String, Option<String>)> = doc
            .blocks
            .iter()
            .map(|block| match &block.kind {
                markdown::BlockKind::Paragraph(text) => (
                    text.text.clone(),
                    text.marks.iter().find_map(|span| match &span.mark {
                        markdown::Mark::Link(url) => Some(url.clone()),
                        _ => None,
                    }),
                ),
                other => panic!("an image survived as {other:?}"),
            })
            .collect();
        assert_eq!(
            paragraphs,
            vec![
                (
                    "after".to_string(),
                    Some("https://ghe.test/a.png".to_string())
                ),
                (
                    "image".to_string(),
                    Some("https://ghe.test/b.png".to_string())
                ),
                ("local".to_string(), None),
            ]
        );
    }

    #[gpui::test]
    fn a_selected_tab_loads_its_header_and_follows_the_forges_title(cx: &mut TestAppContext) {
        cx.update(Theme::init);
        let forge = forge_with_header();
        let source = FakeSource::ready(testing::github_client(forge.clone()), None);
        cx.update(|cx| forge_source::set_source(source, cx));
        let tab =
            cx.new(|cx| ChangeRequestTab::new(testing::reference(101), "old title".into(), cx));
        let titles = Rc::new(RefCell::new(Vec::new()));
        let seen = titles.clone();
        cx.update(|cx| {
            cx.subscribe(&tab, move |_, event: &ChangeRequestTabEvent, _| {
                if let ChangeRequestTabEvent::TitleChanged(title) = event {
                    seen.borrow_mut().push(title.clone());
                }
            })
            .detach()
        });
        tab.update(cx, |tab, cx| tab.on_selected(cx));
        pump_until(cx, || {
            tab.read_with(cx, |tab, _| tab.header.value().is_some())
        });
        tab.read_with(cx, |tab, _| {
            assert_eq!(tab.title(), "Fix the login redirect");
            assert!(
                tab.description.is_some(),
                "the body is rendered once, when it arrives"
            );
        });
        assert_eq!(*titles.borrow(), vec!["Fix the login redirect".to_string()]);
    }

    /// The title of a change request is the first thing a reviewer wants to
    /// paste into a message; it and the meta line under it were painted but
    /// not selectable. Drives the real tab, loaded from the fake forge.
    #[gpui::test]
    fn the_title_and_meta_line_of_a_change_request_can_be_copied(cx: &mut TestAppContext) {
        let forge = forge_with_header();
        let source = FakeSource::ready(testing::github_client(forge), None);
        cx.update(|cx| forge_source::set_source(source, cx));
        let executor = cx.executor();
        let (tab, _, cx) = crate::text_selection::testing::host(cx, |_, cx| {
            ChangeRequestTab::new(testing::reference(101), String::new(), cx)
        });
        tab.update(cx, |tab, cx| tab.on_selected(cx));
        executor.allow_parking();
        for _ in 0..600 {
            if tab.read_with(cx, |tab, _| tab.header.value().is_some()) {
                break;
            }
            executor.advance_clock(std::time::Duration::from_millis(100));
            std::thread::sleep(std::time::Duration::from_millis(5));
            cx.run_until_parked();
        }
        cx.run_until_parked();

        assert_eq!(
            crate::text_selection::testing::copy_line(cx, "change-request-title").as_deref(),
            Some("Fix the login redirect")
        );
        let meta = crate::text_selection::testing::copy_line(cx, "change-request-meta")
            .expect("the clipboard is readable");
        assert!(
            meta.contains(" → ") && meta.contains("updated"),
            "the meta line copied whole: {meta:?}"
        );
    }

    /// A pull request's description is prose a reviewer quotes. It is
    /// rendered Markdown, which had no selection at all outside the chat.
    #[gpui::test]
    fn the_description_of_a_change_request_can_be_selected_and_copied(cx: &mut TestAppContext) {
        let forge = forge_with_header();
        let source = FakeSource::ready(testing::github_client(forge), None);
        cx.update(|cx| forge_source::set_source(source, cx));
        let executor = cx.executor();
        let (tab, _, cx) = crate::text_selection::testing::host(cx, |_, cx| {
            ChangeRequestTab::new(testing::reference(101), String::new(), cx)
        });
        tab.update(cx, |tab, cx| tab.on_selected(cx));
        executor.allow_parking();
        for _ in 0..600 {
            if tab.read_with(cx, |tab, _| tab.header.value().is_some()) {
                break;
            }
            executor.advance_clock(std::time::Duration::from_millis(100));
            std::thread::sleep(std::time::Duration::from_millis(5));
            cx.run_until_parked();
        }
        cx.run_until_parked();

        // The heading and the paragraph, joined the way the page reads.
        assert_eq!(
            crate::text_selection::testing::copy_span(cx, "change-request-description", px(13.0))
                .as_deref(),
            Some("What\n\nFixes it.")
        );
    }

    #[gpui::test]
    fn an_inner_tab_loads_the_first_time_it_is_shown(cx: &mut TestAppContext) {
        cx.update(Theme::init);
        let forge = forge_with_header();
        let source = FakeSource::ready(testing::github_client(forge.clone()), None);
        cx.update(|cx| forge_source::set_source(source, cx));
        let tab = cx.new(|cx| ChangeRequestTab::new(testing::reference(101), String::new(), cx));
        tab.update(cx, |tab, cx| tab.on_selected(cx));
        pump_until(cx, || {
            tab.read_with(cx, |tab, _| tab.header.value().is_some())
        });
        assert_eq!(
            forge.count("ChangeRequestCommits"),
            0,
            "nothing is asked for a tab never shown"
        );
        tab.update(cx, |tab, cx| tab.select_inner(InnerTab::Commits, cx));
        pump_until(cx, || {
            tab.read_with(cx, |tab, _| tab.commits.value().is_some())
        });
        tab.update(cx, |tab, cx| tab.select_inner(InnerTab::Conversation, cx));
        tab.update(cx, |tab, cx| tab.select_inner(InnerTab::Commits, cx));
        cx.run_until_parked();
        assert_eq!(
            forge.count("ChangeRequestCommits"),
            1,
            "a loaded tab is not asked again on return"
        );
    }

    #[gpui::test]
    fn a_failed_inner_tab_keeps_the_conversation(cx: &mut TestAppContext) {
        cx.update(Theme::init);
        let forge = forge_with_header();
        forge.fail("ChangeRequestChecks", 500);
        let source = FakeSource::ready(testing::github_client(forge), None);
        cx.update(|cx| forge_source::set_source(source, cx));
        let tab = cx.new(|cx| ChangeRequestTab::new(testing::reference(101), String::new(), cx));
        tab.update(cx, |tab, cx| tab.on_selected(cx));
        pump_until(cx, || {
            tab.read_with(cx, |tab, _| tab.header.value().is_some())
        });
        tab.update(cx, |tab, cx| tab.select_inner(InnerTab::Checks, cx));
        pump_until(cx, || {
            tab.read_with(cx, |tab, _| matches!(tab.checks, Slot::Failed(_)))
        });
        tab.read_with(cx, |tab, _| {
            assert!(
                tab.header.value().is_some(),
                "an inner tab's failure is its own"
            );
        });
    }

    #[gpui::test]
    fn a_tab_that_cannot_connect_keeps_its_title_and_offers_retry(cx: &mut TestAppContext) {
        cx.update(Theme::init);
        let source = FakeSource::with(Connection::NotConnected {
            forge: Forge::GitHub,
            host: "ghe.test".into(),
        });
        cx.update(|cx| forge_source::set_source(source.clone(), cx));
        let tab = cx.new(|cx| {
            ChangeRequestTab::restored(
                testing::reference(101),
                "Fix the login redirect".into(),
                InnerTab::Checks,
                cx,
            )
        });
        tab.update(cx, |tab, cx| tab.on_selected(cx));
        pump_until(cx, || tab.read_with(cx, |tab, _| tab.unreachable.is_some()));
        tab.read_with(cx, |tab, _| {
            assert_eq!(
                tab.title(),
                "Fix the login redirect",
                "the saved title stays"
            );
            assert!(
                tab.unreachable
                    .as_deref()
                    .is_some_and(|why| why.contains("gh auth login --hostname ghe.test"))
            );
        });
        tab.update(cx, |tab, cx| tab.retry(cx));
        assert_eq!(
            *source.forgotten.lock().unwrap(),
            vec!["ghe.test".to_string()],
            "Retry asks the host again"
        );
    }
}
