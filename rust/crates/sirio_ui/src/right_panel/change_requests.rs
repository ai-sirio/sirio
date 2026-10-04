//! The right panel's fifth view (spec §7.1, mockup C): the worktree's own
//! change request on a card, the four filters as underlined icon tabs, and
//! the list. It reads through `forge_source`. Nothing reaches the network
//! before the view is first shown, and nothing refreshes while it is hidden.

use std::path::PathBuf;
use std::time::Duration;

use bezel::motion::{Fade, Painter};
use bezel::ui::input::TextField;
use bezel::ui::popover;
use gpui::{
    AnyElement, App, ClipboardItem, Context, Entity, EventEmitter, Focusable, FontWeight, Global,
    IntoElement, KeyDownEvent, MouseButton, Pixels, Point, Render, Task, Window, div, prelude::*,
    px,
};
use sirio_forge::{
    ChangePage, ChangeRef, ChangeSummary, Filter, Forge, ForgeError, ListQuery, PageCursor,
};
use sirio_theme::Theme;

use crate::change_request_style as style;
use crate::forge_source::{self, Connection, ReadyConnection};
use crate::sidebar::icons::{Icon, IconElement, IconSize};
use crate::text_selection::selectable_text;

const REFRESH_EVERY: Duration = Duration::from_secs(60);
const SEARCH_DEBOUNCE: Duration = Duration::from_millis(300);

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ChangeRequestListEvent {
    Open { reference: ChangeRef, title: String },
}

/// The filter chosen last, kept for the session like `PanelView`: the
/// panel entity is rebuilt on every worktree switch.
struct FilterSetting(Filter);

impl Global for FilterSetting {}

pub(crate) fn current_filter(cx: &App) -> Filter {
    cx.try_global::<FilterSetting>()
        .map_or(Filter::AllOpen, |setting| setting.0)
}

pub fn filter_word(filter: Filter) -> &'static str {
    match filter {
        Filter::Mine => "mine",
        Filter::ToReview => "to-review",
        Filter::AllOpen => "all-open",
        Filter::ClosedAndMerged => "closed",
    }
}

pub fn parse_filter(word: &str) -> Option<Filter> {
    match word {
        "mine" => Some(Filter::Mine),
        "to-review" => Some(Filter::ToReview),
        "all-open" => Some(Filter::AllOpen),
        "closed" => Some(Filter::ClosedAndMerged),
        _ => None,
    }
}

pub(crate) enum Link {
    /// Not shown yet: nothing has been asked.
    Idle,
    Connecting,
    /// No `ChangeRequestSource` installed (a test host, or a build without it).
    NoSource,
    Settled(Connection),
}

pub(crate) enum Card {
    /// A detached HEAD, or not connected yet.
    Hidden,
    Loading,
    Found(ChangeSummary),
    Missing {
        branch: String,
        create_url: String,
    },
    Failed(ForgeError),
}

pub(crate) enum TokenState {
    Idle,
    Saving,
    Failed(String),
}

struct RowMenu {
    index: usize,
    position: Point<Pixels>,
    painter: Painter,
}

pub(crate) struct ChangeRequestList {
    worktree: PathBuf,
    pub(crate) link: Link,
    pub(crate) rows: Vec<ChangeSummary>,
    next: Option<PageCursor>,
    /// A first page has settled for the current filter and search. It gates
    /// the placeholder, not "a load is in flight": a refresh keeps the rows
    /// on screen (the Files view's `settled` rule).
    pub(crate) settled: bool,
    pub(crate) list_error: Option<ForgeError>,
    pub(crate) card: Card,
    /// The branch the card is about. `connect` reads it once and the panel
    /// stays open across `git switch`, so every card load reads it again.
    pub(crate) card_branch: Option<String>,
    to_review: Option<u32>,
    /// A rate limit's reset: no request before it (spec §9).
    paused_until: Option<i64>,
    search: Entity<TextField>,
    search_open: bool,
    applied_search: String,
    pub(crate) token: Entity<TextField>,
    pub(crate) token_state: TokenState,
    visible: bool,
    generation: u64,
    menu: Option<RowMenu>,
    connect_task: Option<Task<()>>,
    page_task: Option<Task<()>>,
    more_task: Option<Task<()>>,
    card_task: Option<Task<()>>,
    count_task: Option<Task<()>>,
    timer_task: Option<Task<()>>,
    search_task: Option<Task<()>>,
    token_task: Option<Task<()>>,
}

impl EventEmitter<ChangeRequestListEvent> for ChangeRequestList {}

impl ChangeRequestList {
    pub(crate) fn new(worktree: PathBuf, cx: &mut Context<Self>) -> Self {
        let search = cx.new(|cx| TextField::new(cx).with_placeholder("Search"));
        cx.observe(&search, |list, field, cx| {
            let text = field.read(cx).content().trim().to_string();
            if text != list.applied_search {
                list.schedule_search(text, cx);
            }
        })
        .detach();
        let token =
            cx.new(|cx| TextField::new(cx).with_placeholder("Paste a personal access token"));
        Self {
            worktree,
            link: Link::Idle,
            rows: Vec::new(),
            next: None,
            settled: false,
            list_error: None,
            card: Card::Hidden,
            card_branch: None,
            to_review: None,
            paused_until: None,
            search,
            search_open: false,
            applied_search: String::new(),
            token,
            token_state: TokenState::Idle,
            visible: false,
            generation: 0,
            menu: None,
            connect_task: None,
            page_task: None,
            more_task: None,
            card_task: None,
            count_task: None,
            timer_task: None,
            search_task: None,
            token_task: None,
        }
    }

    /// The panel says whether this view is on screen. Hidden, it drops its
    /// timer; shown, it connects the first time and refreshes every time.
    pub(crate) fn set_visible(&mut self, visible: bool, cx: &mut Context<Self>) {
        if visible == self.visible {
            return;
        }
        self.visible = visible;
        if !visible {
            self.timer_task = None;
            return;
        }
        match &self.link {
            Link::Idle | Link::NoSource => self.connect(cx),
            Link::Settled(Connection::Ready(_)) => self.refresh(cx),
            Link::Connecting | Link::Settled(_) => {}
        }
        self.timer_task = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(REFRESH_EVERY).await;
                if this.update(cx, |list, cx| list.refresh(cx)).is_err() {
                    break;
                }
            }
        }));
    }

    fn connect(&mut self, cx: &mut Context<Self>) {
        let Some(source) = forge_source::source(cx) else {
            self.link = Link::NoSource;
            cx.notify();
            return;
        };
        self.link = Link::Connecting;
        let worktree = self.worktree.clone();
        self.connect_task = Some(cx.spawn(async move |this, cx| {
            let connection = cx
                .background_spawn(async move { source.connect(&worktree) })
                .await;
            let _ = this.update(cx, |list, cx| list.settle(connection, cx));
        }));
        cx.notify();
    }

    fn settle(&mut self, connection: Connection, cx: &mut Context<Self>) {
        let ready = matches!(connection, Connection::Ready(_));
        if let Connection::Ready(connected) = &connection {
            self.card_branch = connected.branch.clone();
        }
        self.link = Link::Settled(connection);
        if ready {
            self.refresh(cx);
        }
        cx.notify();
    }

    /// Start over — after Settings changed a host, or a token was saved.
    pub(crate) fn reconnect(&mut self, cx: &mut Context<Self>) {
        self.rows.clear();
        self.settled = false;
        self.card = Card::Hidden;
        self.connect(cx);
    }

    fn ready(&self) -> Option<&ReadyConnection> {
        match &self.link {
            Link::Settled(Connection::Ready(ready)) => Some(ready),
            _ => None,
        }
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

    pub(crate) fn refresh(&mut self, cx: &mut Context<Self>) {
        if self.rate_paused() {
            return;
        }
        let Some(ready) = self.ready().cloned() else {
            return;
        };
        self.load_page(ready.clone(), cx);
        self.load_card(ready.clone(), cx);
        self.load_count(ready, cx);
    }

    fn query(&self, cx: &App) -> ListQuery {
        ListQuery {
            filter: current_filter(cx),
            search: (!self.applied_search.is_empty()).then(|| self.applied_search.clone()),
        }
    }

    fn load_page(&mut self, ready: ReadyConnection, cx: &mut Context<Self>) {
        if self.rate_paused() {
            return;
        }
        self.generation += 1;
        let generation = self.generation;
        self.more_task = None;
        let query = self.query(cx);
        let client = ready.client;
        self.page_task = Some(cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move { client.list(&query, None) })
                .await;
            let _ = this.update(cx, |list, cx| {
                if list.generation == generation {
                    list.apply_page(result, false, cx);
                }
            });
        }));
        cx.notify();
    }

    pub(crate) fn load_more(&mut self, cx: &mut Context<Self>) {
        if self.rate_paused() {
            return;
        }
        let (Some(ready), Some(cursor)) = (self.ready().cloned(), self.next.clone()) else {
            return;
        };
        if self.more_task.is_some() {
            return;
        }
        let query = self.query(cx);
        let generation = self.generation;
        let client = ready.client;
        self.more_task = Some(cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move { client.list(&query, Some(&cursor)) })
                .await;
            let _ = this.update(cx, |list, cx| {
                list.more_task = None;
                if list.generation == generation {
                    list.apply_page(result, true, cx);
                }
            });
        }));
    }

    fn apply_page(
        &mut self,
        result: Result<ChangePage, ForgeError>,
        append: bool,
        cx: &mut Context<Self>,
    ) {
        self.settled = true;
        match result {
            Ok(page) => {
                if append {
                    for item in page.items {
                        if !self
                            .rows
                            .iter()
                            .any(|row| row.reference.number == item.reference.number)
                        {
                            self.rows.push(item);
                        }
                    }
                } else {
                    self.rows = page.items;
                }
                self.next = page.next;
                self.list_error = None;
            }
            Err(error) => self.handle_error(error, cx),
        }
        cx.notify();
    }

    /// A refused sign-in turns the view back into the "not connected" card
    /// and makes the host ask again; a rate limit pauses refreshing until
    /// its reset; anything else shows where the rows are, and they stay.
    fn handle_error(&mut self, error: ForgeError, cx: &mut Context<Self>) {
        match &error {
            ForgeError::NotAuthenticated { host } => {
                let forge = self.ready().map(|ready| ready.client.forge());
                if let (Some(source), Some(forge)) = (forge_source::source(cx), forge) {
                    source.forget(host);
                    self.link = Link::Settled(Connection::NotConnected {
                        forge,
                        host: host.clone(),
                    });
                }
            }
            ForgeError::RateLimited { .. } => self.note_rate_limited(&error),
            _ => {}
        }
        self.list_error = Some(error);
    }

    fn load_card(&mut self, ready: ReadyConnection, cx: &mut Context<Self>) {
        if self.rate_paused() {
            return;
        }
        let Some(source) = forge_source::source(cx) else {
            return;
        };
        if !matches!(self.card, Card::Found(_)) {
            self.card = Card::Loading;
        }
        let client = ready.client;
        let owner = ready.source_owner;
        let worktree = self.worktree.clone();
        let generation = self.generation;
        self.card_task = Some(cx.spawn(async move |this, cx| {
            // The branch is read here, not taken from `ready`: `connect`
            // read it once, and the worktree may have switched since.
            let result = cx
                .background_spawn(async move {
                    let Some(branch) = source.current_branch(&worktree) else {
                        return Ok(None);
                    };
                    client
                        .for_branch(&branch, owner.as_deref())
                        .map(|found| Some((found, client.creation_url(&branch), branch)))
                })
                .await;
            let _ = this.update(cx, |list, cx| {
                if list.generation != generation {
                    return;
                }
                if let Err(error) = &result {
                    list.note_rate_limited(error);
                }
                list.card = match result {
                    Ok(None) => {
                        list.card_branch = None;
                        Card::Hidden
                    }
                    Ok(Some((found, create_url, branch))) => {
                        list.card_branch = Some(branch.clone());
                        match found {
                            Some(found) => Card::Found(found),
                            None => Card::Missing { branch, create_url },
                        }
                    }
                    Err(error) => Card::Failed(error),
                };
                cx.notify();
            });
        }));
    }

    fn load_count(&mut self, ready: ReadyConnection, cx: &mut Context<Self>) {
        if self.rate_paused() {
            return;
        }
        let client = ready.client;
        let generation = self.generation;
        self.count_task = Some(cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move { client.to_review_count() })
                .await;
            let _ = this.update(cx, |list, cx| {
                if list.generation != generation {
                    return;
                }
                if let Err(error) = &result {
                    list.note_rate_limited(error);
                }
                list.to_review = result.ok();
                cx.notify();
            });
        }));
    }

    pub(crate) fn set_filter(&mut self, filter: Filter, cx: &mut Context<Self>) {
        cx.set_global(FilterSetting(filter));
        self.rows.clear();
        self.next = None;
        self.settled = false;
        self.list_error = None;
        if let Some(ready) = self.ready().cloned() {
            self.load_page(ready, cx);
        }
        cx.notify();
    }

    fn schedule_search(&mut self, text: String, cx: &mut Context<Self>) {
        self.search_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(SEARCH_DEBOUNCE).await;
            let _ = this.update(cx, |list, cx| {
                list.applied_search = text;
                list.rows.clear();
                list.settled = false;
                if let Some(ready) = list.ready().cloned() {
                    list.load_page(ready, cx);
                }
            });
        }));
    }

    /// The socket's search: opens the field and types `text` into it, so the
    /// debounce and the read run as they do for a person.
    pub(crate) fn control_search(&mut self, text: &str, _window: &mut Window, cx: &mut Context<Self>) {
        self.search_open = true;
        self.search.update(cx, |field, cx| field.set_content(text, cx));
        cx.notify();
    }

    fn toggle_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.search_open = !self.search_open;
        if self.search_open {
            self.search.read(cx).focus_handle(cx).focus(window, cx);
        } else if !self.applied_search.is_empty() {
            self.search.update(cx, |field, cx| field.clear(cx));
        }
        cx.notify();
    }

    pub(crate) fn open_row(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(row) = self.rows.get(index) {
            cx.emit(ChangeRequestListEvent::Open {
                reference: row.reference.clone(),
                title: row.title.clone(),
            });
        }
    }

    fn open_card(&mut self, cx: &mut Context<Self>) {
        if let Card::Found(found) = &self.card {
            cx.emit(ChangeRequestListEvent::Open {
                reference: found.reference.clone(),
                title: found.title.clone(),
            });
        }
    }

    /// The user's answer to "which forge is this host?".
    pub(crate) fn answer_forge(&mut self, forge: Forge, cx: &mut Context<Self>) {
        let Link::Settled(Connection::UnknownForge { host }) = &self.link else {
            return;
        };
        let Some(source) = forge_source::source(cx) else {
            return;
        };
        let host = host.clone();
        let worktree = self.worktree.clone();
        self.link = Link::Connecting;
        self.connect_task = Some(cx.spawn(async move |this, cx| {
            let connection = cx
                .background_spawn(async move {
                    source.set_forge(&host, forge);
                    source.connect(&worktree)
                })
                .await;
            let _ = this.update(cx, |list, cx| list.settle(connection, cx));
        }));
        cx.notify();
    }

    /// Verifies and stores the pasted token through the host, then connects.
    pub(crate) fn save_token(&mut self, cx: &mut Context<Self>) {
        let Link::Settled(Connection::NotConnected { forge, host }) = &self.link else {
            return;
        };
        let Some(source) = forge_source::source(cx) else {
            return;
        };
        let (forge, host) = (*forge, host.clone());
        let token = self.token.read(cx).content().to_string();
        if token.trim().is_empty() {
            return;
        }
        let worktree = self.worktree.clone();
        self.token_state = TokenState::Saving;
        self.token_task = Some(cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    source
                        .save_token(&host, forge, &token)
                        .map(|_| source.connect(&worktree))
                })
                .await;
            let _ = this.update(cx, |list, cx| {
                match result {
                    Ok(connection) => {
                        list.token_state = TokenState::Idle;
                        list.token.update(cx, |field, cx| field.clear(cx));
                        list.settle(connection, cx);
                    }
                    Err(error) => list.token_state = TokenState::Failed(error.to_string()),
                }
                cx.notify();
            });
        }));
        cx.notify();
    }

    fn on_token_key(&mut self, event: &KeyDownEvent, _window: &mut Window, cx: &mut Context<Self>) {
        if matches!(event.keystroke.key.as_str(), "enter" | "return") {
            self.save_token(cx);
        }
    }

    /// The reference and title of change request `number` on the project
    /// this view reads — what the socket's `open` verb opens (Task 9).
    pub(crate) fn reference_for(&self, number: u64) -> Option<(ChangeRef, String)> {
        let ready = self.ready()?;
        let title = self
            .rows
            .iter()
            .chain(match &self.card {
                Card::Found(found) => Some(found),
                _ => None,
            })
            .find(|row| row.reference.number == number)
            .map(|row| row.title.clone())
            .unwrap_or_default();
        Some((ready.client.reference(number), title))
    }

    pub(crate) fn report(&self, cx: &App) -> Vec<(String, String)> {
        let (state, host) = match &self.link {
            Link::Idle => ("idle", String::new()),
            Link::Connecting => ("connecting", String::new()),
            Link::NoSource => ("unavailable", String::new()),
            Link::Settled(Connection::NoForgeRemote) => ("no-remote", String::new()),
            Link::Settled(Connection::UnknownForge { host }) => ("unknown-forge", host.clone()),
            Link::Settled(Connection::NotConnected { host, .. }) => ("not-connected", host.clone()),
            Link::Settled(Connection::Ready(ready)) => {
                let state = if !self.settled {
                    "loading"
                } else if self.rows.is_empty() && self.list_error.is_some() {
                    "error"
                } else {
                    "ready"
                };
                (state, ready.client.host().to_string())
            }
        };
        let card = match &self.card {
            Card::Found(found) => found.reference.label(),
            Card::Missing { .. } => "none".to_string(),
            Card::Loading => "loading".to_string(),
            Card::Failed(_) => "error".to_string(),
            Card::Hidden => "-".to_string(),
        };
        vec![
            ("state".to_string(), state.to_string()),
            ("host".to_string(), host),
            (
                "filter".to_string(),
                filter_word(current_filter(cx)).to_string(),
            ),
            ("rows".to_string(), self.rows.len().to_string()),
            (
                "labels".to_string(),
                self.rows
                    .iter()
                    .map(|row| row.reference.label())
                    .collect::<Vec<_>>()
                    .join(","),
            ),
            ("card".to_string(), card),
            (
                "toReview".to_string(),
                self.to_review
                    .map_or("-".to_string(), |count| count.to_string()),
            ),
            (
                "error".to_string(),
                self.list_error
                    .as_ref()
                    .map(ToString::to_string)
                    .unwrap_or_default(),
            ),
        ]
    }

    fn open_menu(&mut self, index: usize, position: Point<Pixels>, cx: &mut Context<Self>) {
        self.menu = Some(RowMenu {
            index,
            position,
            painter: Painter::of(cx),
        });
        cx.notify();
    }

    fn close_menu(&mut self, cx: &mut Context<Self>) {
        self.menu = None;
        cx.notify();
    }
}

/// What a failure means to the reader, and what to do about it.
fn error_text(error: &ForgeError) -> String {
    match error {
        ForgeError::RateLimited {
            reset_at: Some(reset),
            ..
        } => {
            let at = chrono::DateTime::from_timestamp(*reset, 0)
                .map(|time| {
                    time.with_timezone(&chrono::Local)
                        .format("%H:%M")
                        .to_string()
                })
                .unwrap_or_default();
            format!("Rate limited · retrying at {at}")
        }
        ForgeError::RateLimited { .. } => "Rate limited · retrying shortly".to_string(),
        ForgeError::NotInstalled { program } => {
            format!("{program} is not on PATH. Add a token in Settings → Git Hosting instead.")
        }
        ForgeError::Tls { host, .. } => format!(
            "The TLS handshake with {host} failed. If it uses a company certificate authority, install that CA in the system trust store."
        ),
        other => other.to_string(),
    }
}

fn icon_button(
    id: &'static str,
    icon: Icon,
    tooltip: &'static str,
    theme: &Theme,
    on_click: impl Fn(&mut Window, &mut App) + 'static,
) -> gpui::Stateful<gpui::Div> {
    let hover = theme.ely.hover;
    div()
        .id(id)
        .debug_selector(move || id.to_owned())
        .w(px(24.0))
        .h(px(24.0))
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .rounded(theme.radii.chip)
        .text_color(theme.ely.fg_muted)
        .cursor_pointer()
        .hover(move |style| style.bg(hover))
        .tooltip(move |window, cx| crate::controls::sidebar_tooltip(tooltip, window, cx))
        .on_click(move |_, window, cx| on_click(window, cx))
        .child(IconElement::new(icon, IconSize::Small))
}

fn text_button(
    id: &'static str,
    label: String,
    theme: &Theme,
    on_click: impl Fn(&mut App) + 'static,
) -> gpui::Stateful<gpui::Div> {
    let hover = theme.ely.hover;
    div()
        .id(id)
        .debug_selector(move || id.to_owned())
        .flex_none()
        .px(px(10.0))
        .py(px(5.0))
        .rounded(theme.radii.control)
        .bg(theme.ely.hover)
        .text_size(theme.typography.footnote)
        .text_color(theme.ely.fg)
        .cursor_pointer()
        .hover(move |style| style.bg(hover))
        .on_click(move |_, _, cx| on_click(cx))
        .child(label)
}

fn notice(id: &'static str, title: String, hint: Option<String>, theme: &Theme) -> gpui::Div {
    div()
        .debug_selector(move || id.to_owned())
        .flex_1()
        .min_h(px(0.0))
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap(theme.spacing.card_gap)
        .p(theme.spacing.card_gap)
        .text_size(theme.typography.headline)
        .text_color(theme.ely.fg)
        .child(selectable_text(title))
        .when_some(hint, |this, hint| {
            this.child(
                div()
                    .text_size(theme.typography.footnote)
                    .text_color(theme.ely.fg_subtle)
                    .child(selectable_text(hint)),
            )
        })
}

impl ChangeRequestList {
    fn render_header(
        &self,
        ready: &ReadyConnection,
        theme: &Theme,
        entity: &Entity<Self>,
    ) -> impl IntoElement {
        let client = &ready.client;
        let project = match client.host() {
            "github.com" | "gitlab.com" => client.project().to_string(),
            host => format!("{host}/{}", client.project()),
        };
        let refresh = entity.clone();
        div()
            .flex()
            .items_center()
            .gap(px(7.0))
            .px(px(12.0))
            .pt(px(9.0))
            .pb(px(6.0))
            .text_size(theme.typography.footnote)
            .text_color(theme.ely.fg_muted)
            .child(
                IconElement::new(style::forge_mark(client.forge()), IconSize::Small)
                    .text_color(theme.ely.fg),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .text_ellipsis()
                    .whitespace_nowrap()
                    .child(project),
            )
            .child(icon_button(
                "change-requests-refresh",
                Icon::RefreshCw,
                "Refresh",
                theme,
                move |_, cx| refresh.update(cx, |list, cx| list.refresh(cx)),
            ))
    }

    fn meta_line(item: &ChangeSummary, theme: &Theme) -> gpui::Div {
        div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(9.0))
            .text_size(theme.typography.footnote)
            .text_color(theme.ely.fg_muted)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(3.0))
                    .text_color(style::state_color(item.state, theme))
                    .child(IconElement::new(Icon::PullRequest, IconSize::XSmall))
                    .child(style::state_label(item.state)),
            )
            .when_some(style::ci_mark(item.ci, theme), |this, (icon, tint)| {
                this.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(3.0))
                        .text_color(tint)
                        .child(IconElement::new(icon, IconSize::XSmall))
                        .child(style::ci_text(item.ci)),
                )
            })
            .when_some(
                style::review_mark(item.review, item.review_requested_from_me, theme),
                |this, (icon, tint)| {
                    this.child(IconElement::new(icon, IconSize::XSmall).text_color(tint))
                },
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(3.0))
                    .child(IconElement::new(Icon::MessageSquare, IconSize::XSmall))
                    .child(item.comments.to_string()),
            )
    }

    fn render_card(
        &self,
        ready: &ReadyConnection,
        theme: &Theme,
        entity: &Entity<Self>,
    ) -> Option<AnyElement> {
        let branch = self.card_branch.clone()?;
        let noun = ready.client.forge().change_noun();
        let body: AnyElement = match &self.card {
            Card::Found(found) => div()
                .flex()
                .flex_col()
                .gap(px(3.0))
                .child(
                    div()
                        .overflow_hidden()
                        .text_ellipsis()
                        .whitespace_nowrap()
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(theme.ely.fg)
                        .child(format!("{} {}", found.reference.label(), found.title)),
                )
                .child(Self::meta_line(found, theme))
                .into_any_element(),
            Card::Missing { branch, create_url } => {
                let url = create_url.clone();
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap(px(8.0))
                    .text_color(theme.ely.fg_muted)
                    .child(selectable_text(format!("No {noun} for {branch}")))
                    .child(icon_button(
                        "change-requests-create",
                        Icon::Plus,
                        "Create on the forge",
                        theme,
                        move |_, cx| cx.open_url(&url),
                    ))
                    .into_any_element()
            }
            Card::Loading | Card::Hidden => div()
                .text_color(theme.ely.fg_subtle)
                .child(format!("Looking for this branch's {noun}…"))
                .into_any_element(),
            Card::Failed(error) => div()
                .text_color(theme.ely.danger)
                .child(selectable_text(error_text(error)))
                .into_any_element(),
        };
        let found = matches!(self.card, Card::Found(_));
        let open = entity.clone();
        Some(
            div()
                .id("change-requests-card")
                .debug_selector(|| "change-requests-card".to_owned())
                .mx(px(10.0))
                .mt(px(4.0))
                .mb(px(10.0))
                .p(px(10.0))
                .flex()
                .flex_col()
                .gap(px(4.0))
                .rounded(theme.radii.control)
                .border_1()
                .border_color(theme.ely.border)
                .bg(theme.ely.surface)
                .when(found, |this| {
                    this.cursor_pointer()
                        .on_click(move |_, _, cx| open.update(cx, |list, cx| list.open_card(cx)))
                })
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .text_size(theme.typography.caption2)
                        .text_color(theme.ely.fg_subtle)
                        .child(IconElement::new(Icon::GitBranch, IconSize::XSmall))
                        .child(format!("THIS WORKTREE · {branch}")),
                )
                .child(body)
                .into_any_element(),
        )
    }

    fn render_filters(
        &self,
        filter: Filter,
        theme: &Theme,
        entity: &Entity<Self>,
    ) -> impl IntoElement {
        const FILTERS: [(Filter, Icon, &str, &str); 4] = [
            (
                Filter::Mine,
                Icon::Person,
                "Mine",
                "change-requests-filter-mine",
            ),
            (
                Filter::ToReview,
                Icon::Eye,
                "To review",
                "change-requests-filter-to-review",
            ),
            (
                Filter::AllOpen,
                Icon::PullRequest,
                "All open",
                "change-requests-filter-all-open",
            ),
            (
                Filter::ClosedAndMerged,
                Icon::Archive,
                "Closed & merged",
                "change-requests-filter-closed",
            ),
        ];
        let hover = theme.ely.hover;
        let to_review = self.to_review.filter(|count| *count > 0);
        let search = entity.clone();
        div()
            .h(px(30.0))
            .px(px(6.0))
            .flex()
            .items_center()
            .border_b_1()
            .border_color(theme.ely.border)
            .children(FILTERS.map(|(candidate, icon, label, id)| {
                let active = candidate == filter;
                let tone = if active { theme.ely.fg } else { theme.ely.fg_muted };
                let choose = entity.clone();
                div()
                    .id(id)
                    .debug_selector(move || id.to_owned())
                    .relative()
                    .h_full()
                    .flex()
                    .items_center()
                    .gap(px(5.0))
                    .px(px(7.0))
                    .text_size(theme.typography.footnote)
                    .font_weight(if active {
                        FontWeight::MEDIUM
                    } else {
                        FontWeight::NORMAL
                    })
                    .text_color(tone)
                    .cursor_pointer()
                    .hover(move |style| style.bg(hover))
                    .tooltip(move |window, cx| crate::controls::sidebar_tooltip(label, window, cx))
                    .on_click(move |_, _, cx| {
                        choose.update(cx, |list, cx| list.set_filter(candidate, cx))
                    })
                    .child(IconElement::new(icon, IconSize::Small).text_color(tone))
                    .when(active, |this| this.child(label))
                    .when(candidate == Filter::ToReview, |this| {
                        this.when_some(to_review, |this, count| {
                            this.child(
                                div()
                                    .text_size(theme.typography.caption2)
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(theme.sirio.quantity)
                                    .child(count.to_string()),
                            )
                        })
                    })
                    .when(active, |this| {
                        this.child(
                            div()
                                .absolute()
                                .bottom(px(-1.0))
                                .left_0()
                                .right_0()
                                .h(px(2.0))
                                .bg(theme.ely.fg),
                        )
                    })
            }))
            .child(div().flex_1())
            .child(icon_button(
                "change-requests-search",
                Icon::MagnifyingGlass,
                "Search",
                theme,
                move |window, cx| search.update(cx, |list, cx| list.toggle_search(window, cx)),
            ))
    }

    fn render_row(
        &self,
        index: usize,
        row: &ChangeSummary,
        now: i64,
        theme: &Theme,
        entity: &Entity<Self>,
    ) -> impl IntoElement {
        let hover = theme.ely.hover;
        let open = entity.clone();
        let menu = entity.clone();
        div()
            .id(("change-request-row", index))
            .debug_selector(move || format!("change-request-row-{index}"))
            .px(px(12.0))
            .py(px(7.0))
            .flex()
            .items_start()
            .gap(px(8.0))
            .border_b_1()
            .border_color(theme.ely.border)
            .cursor_pointer()
            .hover(move |style| style.bg(hover))
            .on_click(move |_, _, cx| open.update(cx, |list, cx| list.open_row(index, cx)))
            .on_mouse_down(MouseButton::Right, move |event, _, cx| {
                cx.stop_propagation();
                menu.update(cx, |list, cx| list.open_menu(index, event.position, cx));
            })
            .child(
                div().pt(px(2.0)).child(
                    IconElement::new(Icon::PullRequest, IconSize::Small)
                        .text_color(style::state_color(row.state, theme)),
                ),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(1.0))
                    .child(
                        div()
                            .overflow_hidden()
                            .text_ellipsis()
                            .whitespace_nowrap()
                            .text_color(theme.ely.fg)
                            .child(row.title.clone()),
                    )
                    .child(
                        div()
                            .text_size(theme.typography.footnote)
                            .text_color(theme.ely.fg_subtle)
                            .child(format!(
                                "{} · {} · {}",
                                row.reference.label(),
                                row.author,
                                style::age(now, row.updated_at)
                            )),
                    ),
            )
            .child(
                div()
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(px(7.0))
                    .text_size(theme.typography.footnote)
                    .text_color(theme.ely.fg_muted)
                    .when_some(style::ci_mark(row.ci, theme), |this, (icon, tint)| {
                        this.child(IconElement::new(icon, IconSize::Small).text_color(tint))
                    })
                    .when_some(
                        style::review_mark(row.review, row.review_requested_from_me, theme),
                        |this, (icon, tint)| {
                            this.child(IconElement::new(icon, IconSize::Small).text_color(tint))
                        },
                    )
                    .when(row.comments > 0, |this| {
                        this.child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(2.0))
                                .child(IconElement::new(Icon::MessageSquare, IconSize::XSmall))
                                .child(row.comments.to_string()),
                        )
                    }),
            )
    }

    fn render_rows(&self, theme: &Theme, entity: &Entity<Self>) -> AnyElement {
        if !self.settled {
            return notice(
                "change-requests-loading",
                "Loading…".to_string(),
                None,
                theme,
            )
            .into_any_element();
        }
        let retry = entity.clone();
        if self.rows.is_empty() {
            return match &self.list_error {
                Some(error) => notice("change-requests-error", error_text(error), None, theme)
                    .child(text_button(
                        "change-requests-retry",
                        "Retry".to_string(),
                        theme,
                        move |cx| retry.update(cx, |list, cx| list.refresh(cx)),
                    ))
                    .into_any_element(),
                None => notice(
                    "change-requests-empty",
                    "Nothing here.".to_string(),
                    None,
                    theme,
                )
                .into_any_element(),
            };
        }
        let now = style::now();
        let more = entity.clone();
        div()
            .id("change-requests-rows")
            .debug_selector(|| "change-requests-rows".to_owned())
            .flex_1()
            .min_h(px(0.0))
            .overflow_y_scroll()
            .when_some(self.list_error.as_ref(), |this, error| {
                this.child(
                    div()
                        .px(px(12.0))
                        .py(px(6.0))
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .text_size(theme.typography.footnote)
                        .text_color(theme.ely.danger)
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .overflow_hidden()
                                .text_ellipsis()
                                .child(selectable_text(format!(
                                    "Refresh failed · {}",
                                    error_text(error)
                                ))),
                        )
                        .child(text_button(
                            "change-requests-refresh-retry",
                            "Retry".to_string(),
                            theme,
                            move |cx| retry.update(cx, |list, cx| list.refresh(cx)),
                        )),
                )
            })
            .children(
                self.rows
                    .iter()
                    .enumerate()
                    .map(|(index, row)| self.render_row(index, row, now, theme, entity)),
            )
            .when(self.next.is_some(), |this| {
                this.child(div().p(px(10.0)).flex().justify_center().child(text_button(
                    "change-requests-more",
                    if self.more_task.is_some() {
                        "Loading…".to_string()
                    } else {
                        "Load more".to_string()
                    },
                    theme,
                    move |cx| more.update(cx, |list, cx| list.load_more(cx)),
                )))
            })
            .into_any_element()
    }

    fn render_unknown(&self, host: &str, theme: &Theme, entity: &Entity<Self>) -> AnyElement {
        let github = entity.clone();
        let gitlab = entity.clone();
        notice(
            "change-requests-unknown-forge",
            format!("Which forge is {host}?"),
            Some("Sirio cannot tell from the host alone. Your answer is kept.".to_string()),
            theme,
        )
        .child(
            div()
                .flex()
                .gap(px(8.0))
                .child(text_button(
                    "change-requests-forge-github",
                    "GitHub Enterprise".to_string(),
                    theme,
                    move |cx| github.update(cx, |list, cx| list.answer_forge(Forge::GitHub, cx)),
                ))
                .child(text_button(
                    "change-requests-forge-gitlab",
                    "GitLab".to_string(),
                    theme,
                    move |cx| gitlab.update(cx, |list, cx| list.answer_forge(Forge::GitLab, cx)),
                )),
        )
        .into_any_element()
    }

    fn render_not_connected(
        &self,
        forge: Forge,
        host: &str,
        theme: &Theme,
        entity: &Entity<Self>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let command = format!("{} auth login --hostname {host}", style::cli_name(forge));
        let scopes = style::token_scopes(forge);
        let copied = command.clone();
        let save = entity.clone();
        notice(
            "change-requests-not-connected",
            format!("Not signed in to {host}"),
            None,
            theme,
        )
        .child(
            div()
                .text_size(theme.typography.footnote)
                .text_color(theme.ely.fg_muted)
                .child(selectable_text(format!(
                    "Sign in with the {} CLI:",
                    forge.name()
                ))),
        )
        .child(
            div()
                .w_full()
                .flex()
                .items_center()
                .gap(px(6.0))
                .px(px(8.0))
                .py(px(4.0))
                .rounded(theme.radii.control)
                .bg(theme.sirio.code_wash)
                .text_size(theme.typography.footnote)
                .child(div().flex_1().min_w_0().child(selectable_text(command)))
                .child(icon_button(
                    "change-requests-copy-login",
                    Icon::Copy,
                    "Copy",
                    theme,
                    move |_, cx| cx.write_to_clipboard(ClipboardItem::new_string(copied.clone())),
                )),
        )
        .child(
            div()
                .text_size(theme.typography.footnote)
                .text_color(theme.ely.fg_muted)
                .child(selectable_text(format!(
                    "or paste a personal access token with {scopes}:"
                ))),
        )
        .child(
            div()
                .w_full()
                .max_w(px(360.0))
                .flex()
                .items_center()
                .gap(px(6.0))
                .child(
                    div()
                        .id("change-requests-token-field")
                        .debug_selector(|| "change-requests-token-field".to_owned())
                        .flex_1()
                        .min_w_0()
                        .on_key_down(cx.listener(Self::on_token_key))
                        .child(crate::controls::sidebar_text_field(self.token.clone())),
                )
                .child(text_button(
                    "change-requests-token-save",
                    "Save".to_string(),
                    theme,
                    move |cx| save.update(cx, |list, cx| list.save_token(cx)),
                )),
        )
        .when_some(
            match &self.token_state {
                TokenState::Idle => None,
                TokenState::Saving => Some(("Checking the token…".to_string(), theme.ely.fg_subtle)),
                TokenState::Failed(why) => Some((why.clone(), theme.ely.danger)),
            },
            |this, (text, tone)| {
                this.child(
                    div()
                        .text_size(theme.typography.footnote)
                        .text_color(tone)
                        .child(selectable_text(text)),
                )
            },
        )
        .into_any_element()
    }

    fn render_menu(
        &self,
        menu: &RowMenu,
        theme: &Theme,
        entity: &Entity<Self>,
    ) -> Option<AnyElement> {
        let row = self.rows.get(menu.index)?;
        let bezel_theme = theme.to_bezel_theme();
        let mut view = popover::popover_card(&bezel_theme)
            .id("change-request-menu")
            .debug_selector(|| "change-request-menu".to_owned())
            .text_size(theme.typography.ui_size)
            .w(theme.spacing.menu_width);
        for (selector, label) in [
            ("change-request-menu-open-browser", "Open in browser"),
            ("change-request-menu-copy-link", "Copy link"),
        ] {
            let url = row.web_url.clone();
            let close = entity.clone();
            let item = popover::menu_row(
                &bezel_theme,
                false,
                Fade::new(menu.painter.clone(), selector),
            )
            .id(selector)
            .debug_selector(move || selector.to_owned())
            .w_full()
            .min_h(theme.spacing.titlebar_control_frame.height)
            .text_size(theme.typography.ui_size)
            .text_color(bezel_theme.text)
            .on_click(move |_, _, cx| {
                if selector == "change-request-menu-copy-link" {
                    cx.write_to_clipboard(ClipboardItem::new_string(url.clone()));
                } else {
                    cx.open_url(&url);
                }
                close.update(cx, |list, cx| list.close_menu(cx));
            })
            .child(label);
            view = view.child(item);
        }
        let close = entity.clone();
        Some(popover::menu_at(
            "change-request-menu-layer",
            menu.position,
            view.on_mouse_down_out(move |_, _, cx| {
                close.update(cx, |list, cx| list.close_menu(cx))
            })
            .into_any_element(),
            None,
        ))
    }
}

impl Render for ChangeRequestList {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let _perf = sirio_perf::span("ChangeRequestList.render", cx.entity_id().as_u64());
        let theme = Theme::get(cx).with_sidebar_typography();
        let entity = cx.entity();
        let filter = current_filter(cx);
        let content: AnyElement = match &self.link {
            Link::Idle | Link::Connecting => notice(
                "change-requests-connecting",
                "Connecting to the forge…".to_string(),
                None,
                &theme,
            )
            .into_any_element(),
            Link::NoSource => notice(
                "change-requests-unavailable",
                "Change requests are unavailable in this build.".to_string(),
                None,
                &theme,
            )
            .into_any_element(),
            Link::Settled(Connection::NoForgeRemote) => notice(
                "change-requests-no-remote",
                "No forge remote".to_string(),
                Some("This repository has no remote on GitHub or GitLab.".to_string()),
                &theme,
            )
            .into_any_element(),
            Link::Settled(Connection::UnknownForge { host }) => {
                self.render_unknown(&host.clone(), &theme, &entity)
            }
            Link::Settled(Connection::NotConnected { forge, host }) => {
                let (forge, host) = (*forge, host.clone());
                self.render_not_connected(forge, &host, &theme, &entity, cx)
            }
            Link::Settled(Connection::Ready(ready)) => {
                let ready = ready.clone();
                div()
                    .flex_1()
                    .min_h(px(0.0))
                    .flex()
                    .flex_col()
                    .child(self.render_header(&ready, &theme, &entity))
                    .when_some(self.render_card(&ready, &theme, &entity), |this, card| {
                        this.child(card)
                    })
                    .child(self.render_filters(filter, &theme, &entity))
                    .when(self.search_open, |this| {
                        this.child(
                            div()
                                .px(px(10.0))
                                .py(px(6.0))
                                .border_b_1()
                                .border_color(theme.ely.border)
                                .child(crate::controls::sidebar_text_field(self.search.clone())),
                        )
                    })
                    .child(self.render_rows(&theme, &entity))
                    .into_any_element()
            }
        };
        let menu = self
            .menu
            .as_ref()
            .and_then(|menu| self.render_menu(menu, &theme, &entity));
        div()
            .id("change-requests")
            .debug_selector(|| "change-requests".to_owned())
            .size_full()
            .flex()
            .flex_col()
            .child(content)
            .when_some(menu, |this, menu| this.child(menu))
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::path::PathBuf;
    use std::rc::Rc;
    use std::sync::Arc;
    use std::time::Duration;

    use gpui::TestAppContext;
    use sirio_forge::{Filter, Forge, ForgeError};
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
            cx.executor().advance_clock(Duration::from_millis(100));
            std::thread::sleep(Duration::from_millis(5));
            cx.run_until_parked();
        }
        panic!("condition never became true within the pump budget");
    }

    fn forge() -> Arc<CannedForge> {
        let forge = Arc::new(CannedForge::default());
        forge.answer("Viewer", testing::viewer());
        forge.answer(
            "ChangeRequestList",
            testing::list(
                vec![
                    testing::summary(101, "Fix the login"),
                    testing::summary(102, "Draft the README"),
                ],
                None,
            ),
        );
        forge.answer(
            "ChangeRequestSearch",
            testing::search(vec![testing::summary(104, "Bump the parser")]),
        );
        forge.answer("ChangeRequestCount", testing::count(1));
        forge.answer(
            "ChangeRequestForBranch",
            testing::branch(vec![testing::summary(103, "This branch")]),
        );
        forge
    }

    fn labels(list: &Entity<ChangeRequestList>, cx: &TestAppContext) -> Vec<String> {
        list.read_with(cx, |list, _| {
            list.rows.iter().map(|row| row.reference.label()).collect()
        })
    }

    fn shown(cx: &mut TestAppContext, source: Arc<FakeSource>) -> Entity<ChangeRequestList> {
        cx.update(Theme::init);
        cx.update(|cx| forge_source::set_source(source, cx));
        let list = cx.new(|cx| ChangeRequestList::new(PathBuf::from("/tmp/checkout"), cx));
        list.update(cx, |list, cx| list.set_visible(true, cx));
        list
    }

    #[gpui::test]
    fn the_login_command_and_copy_button_fit_inside_a_compact_sidebar(cx: &mut TestAppContext) {
        struct LoginPanel(Entity<ChangeRequestList>);

        impl Render for LoginPanel {
            fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
                div()
                    .w(px(320.0))
                    .h_full()
                    .overflow_hidden()
                    .child(self.0.clone())
            }
        }

        cx.update(Theme::init);
        let list = cx.new(|cx| {
            let mut list = ChangeRequestList::new(PathBuf::from("/tmp/checkout"), cx);
            list.link = Link::Settled(Connection::NotConnected {
                forge: Forge::GitLab,
                host: "git.compact-sidebar.invalid".to_owned(),
            });
            list
        });
        let window = cx.add_window(|_, _| LoginPanel(list));
        let mut view = gpui::VisualTestContext::from_window(window.into(), cx);
        view.run_until_parked();
        let panel = view.debug_bounds("change-requests").expect("panel drawn");
        let copy = view
            .debug_bounds("change-requests-copy-login")
            .expect("copy button drawn");
        assert!(
            copy.left() >= panel.left() && copy.right() <= panel.right(),
            "the copy button must stay inside the sidebar: copy={copy:?}, panel={panel:?}"
        );
        view.simulate_click(copy.center(), gpui::Modifiers::none());
        let copied = view.update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text()));
        assert_eq!(
            copied.as_deref(),
            Some("glab auth login --hostname git.compact-sidebar.invalid")
        );
    }

    #[gpui::test]
    fn nothing_is_asked_before_the_view_is_shown(cx: &mut TestAppContext) {
        cx.update(Theme::init);
        let source = FakeSource::ready(testing::github_client(forge()), Some("feat/103"));
        cx.update(|cx| forge_source::set_source(source.clone(), cx));
        let list = cx.new(|cx| ChangeRequestList::new(PathBuf::from("/tmp/checkout"), cx));
        cx.run_until_parked();
        assert_eq!(
            *source.connects.lock().unwrap(),
            0,
            "no call until the view is shown"
        );
        list.update(cx, |list, cx| list.set_visible(true, cx));
        pump_until(cx, || list.read_with(cx, |list, _| list.settled));
        assert_eq!(labels(&list, cx), vec!["#101", "#102"]);
        list.read_with(cx, |list, _| assert!(matches!(list.card, Card::Found(_))));
    }

    #[gpui::test]
    fn the_card_follows_the_branch_after_a_switch(cx: &mut TestAppContext) {
        let canned = forge();
        canned.answer("ChangeRequestForBranch", testing::branch(vec![]));
        let source = FakeSource::ready(testing::github_client(canned.clone()), Some("main"));
        let list = shown(cx, source.clone());
        pump_until(cx, || {
            list.read_with(cx, |list, _| matches!(list.card, Card::Missing { .. }))
        });
        list.read_with(cx, |list, _| {
            assert_eq!(list.card_branch.as_deref(), Some("main"));
        });

        source.switch_to("feat/103");
        canned.answer(
            "ChangeRequestForBranch",
            testing::branch(vec![testing::summary(103, "This branch")]),
        );
        list.update(cx, |list, cx| list.refresh(cx));
        pump_until(cx, || {
            list.read_with(cx, |list, _| matches!(list.card, Card::Found(_)))
        });

        list.read_with(cx, |list, _| {
            assert_eq!(list.card_branch.as_deref(), Some("feat/103"));
        });
    }

    #[gpui::test]
    fn choosing_a_filter_lists_its_change_requests(cx: &mut TestAppContext) {
        let list = shown(cx, FakeSource::ready(testing::github_client(forge()), None));
        pump_until(cx, || list.read_with(cx, |list, _| list.settled));
        list.update(cx, |list, cx| list.set_filter(Filter::ToReview, cx));
        pump_until(cx, || list.read_with(cx, |list, _| list.settled));
        assert_eq!(labels(&list, cx), vec!["#104"]);
        list.update(cx, |list, cx| list.set_filter(Filter::AllOpen, cx));
    }

    #[gpui::test]
    fn a_late_page_for_another_filter_is_dropped(cx: &mut TestAppContext) {
        let list = shown(cx, FakeSource::ready(testing::github_client(forge()), None));
        pump_until(cx, || list.read_with(cx, |list, _| list.settled));
        // Queue an All-open reload and switch filter before either runs:
        // whichever answer lands last, the older one must be dropped.
        list.update(cx, |list, cx| {
            list.refresh(cx);
            list.set_filter(Filter::ToReview, cx);
        });
        pump_until(cx, || list.read_with(cx, |list, _| list.settled));
        cx.run_until_parked();
        assert_eq!(labels(&list, cx), vec!["#104"]);
        list.update(cx, |list, cx| list.set_filter(Filter::AllOpen, cx));
    }

    #[gpui::test]
    fn a_failed_refresh_keeps_the_rows(cx: &mut TestAppContext) {
        let forge = forge();
        let list = shown(
            cx,
            FakeSource::ready(testing::github_client(forge.clone()), None),
        );
        pump_until(cx, || list.read_with(cx, |list, _| list.settled));
        forge.fail("ChangeRequestList", 500);
        list.update(cx, |list, cx| list.refresh(cx));
        pump_until(cx, || {
            list.read_with(cx, |list, _| list.list_error.is_some())
        });
        assert_eq!(
            labels(&list, cx),
            vec!["#101", "#102"],
            "a failed refresh keeps what was shown"
        );
    }

    #[gpui::test]
    fn a_hidden_list_stops_refreshing(cx: &mut TestAppContext) {
        let forge = forge();
        let list = shown(
            cx,
            FakeSource::ready(testing::github_client(forge.clone()), None),
        );
        pump_until(cx, || list.read_with(cx, |list, _| list.settled));
        let asked = forge.count("ChangeRequestList");
        list.update(cx, |list, cx| list.set_visible(false, cx));
        cx.executor().advance_clock(Duration::from_secs(180));
        cx.run_until_parked();
        assert_eq!(
            forge.count("ChangeRequestList"),
            asked,
            "nothing is asked while hidden"
        );
        list.update(cx, |list, cx| list.set_visible(true, cx));
        pump_until(cx, || forge.count("ChangeRequestList") > asked);
    }

    #[gpui::test]
    fn a_rate_limited_host_is_not_asked_until_it_resets(cx: &mut TestAppContext) {
        let forge = Arc::new(CannedForge::default());
        forge.answer("Viewer", testing::viewer());
        forge.answer(
            "ChangeRequestList",
            testing::list(
                vec![
                    testing::summary(101, "Fix the login"),
                    testing::summary(102, "Draft the README"),
                ],
                Some("cursor1"),
            ),
        );
        forge.answer(
            "ChangeRequestSearch",
            testing::search(vec![testing::summary(104, "Bump the parser")]),
        );
        forge.answer("ChangeRequestCount", testing::count(1));
        forge.answer(
            "ChangeRequestForBranch",
            testing::branch(vec![testing::summary(103, "This branch")]),
        );
        let list = shown(
            cx,
            FakeSource::ready(testing::github_client(forge.clone()), None),
        );
        pump_until(cx, || list.read_with(cx, |list, _| list.settled));
        let reset_at = crate::change_request_style::now() + 3600;
        forge.rate_limited("ChangeRequestList", reset_at);
        forge.rate_limited("ChangeRequestSearch", reset_at);
        forge.rate_limited("ChangeRequestMine", reset_at);
        list.update(cx, |list, cx| list.refresh(cx));
        pump_until(cx, || {
            list.read_with(cx, |list, _| {
                matches!(list.list_error, Some(ForgeError::RateLimited { .. }))
            })
        });
        let listed = forge.count("ChangeRequestList");
        let searched = forge.count("ChangeRequestSearch");
        list.update(cx, |list, cx| list.load_more(cx));
        list.update(cx, |list, cx| list.set_filter(Filter::ClosedAndMerged, cx));
        cx.executor().advance_clock(Duration::from_millis(600));
        cx.run_until_parked();
        cx.executor().advance_clock(Duration::from_millis(600));
        cx.run_until_parked();
        assert_eq!(
            forge.count("ChangeRequestList"),
            listed,
            "no page is asked while paused"
        );
        assert_eq!(
            forge.count("ChangeRequestSearch"),
            searched,
            "no search is asked while paused"
        );
    }

    #[gpui::test]
    fn opening_a_row_asks_the_host_for_its_tab(cx: &mut TestAppContext) {
        let list = shown(cx, FakeSource::ready(testing::github_client(forge()), None));
        pump_until(cx, || list.read_with(cx, |list, _| list.settled));
        let opened = Rc::new(RefCell::new(Vec::new()));
        let seen = opened.clone();
        cx.update(|cx| {
            cx.subscribe(&list, move |_, event: &ChangeRequestListEvent, _| {
                seen.borrow_mut().push(event.clone())
            })
            .detach()
        });
        list.update(cx, |list, cx| list.open_row(0, cx));
        assert_eq!(
            *opened.borrow(),
            vec![ChangeRequestListEvent::Open {
                reference: testing::reference(101),
                title: "Fix the login".into()
            }]
        );
    }

    #[gpui::test]
    fn an_unknown_forge_is_asked_and_the_answer_kept(cx: &mut TestAppContext) {
        let source = FakeSource::with(Connection::UnknownForge {
            host: "git.corp".into(),
        });
        let list = shown(cx, source.clone());
        pump_until(cx, || {
            list.read_with(cx, |list, _| matches!(list.link, Link::Settled(_)))
        });
        source.set(Connection::NotConnected {
            forge: Forge::GitLab,
            host: "git.corp".into(),
        });
        list.update(cx, |list, cx| list.answer_forge(Forge::GitLab, cx));
        pump_until(cx, || {
            list.read_with(cx, |list, _| {
                matches!(list.link, Link::Settled(Connection::NotConnected { .. }))
            })
        });
        assert_eq!(
            *source.forges.lock().unwrap(),
            vec![("git.corp".to_string(), Forge::GitLab)]
        );
    }

    #[gpui::test]
    fn a_rejected_token_says_why_and_stays_disconnected(cx: &mut TestAppContext) {
        let source = FakeSource::with(Connection::NotConnected {
            forge: Forge::GitHub,
            host: "ghe.test".into(),
        });
        *source.token_answer.lock().unwrap() = Err(ForgeError::NotAuthenticated {
            host: "ghe.test".into(),
        });
        let list = shown(cx, source.clone());
        pump_until(cx, || {
            list.read_with(cx, |list, _| matches!(list.link, Link::Settled(_)))
        });
        list.update(cx, |list, cx| {
            list.token
                .update(cx, |field, cx| field.set_content("bad-token", cx));
            list.save_token(cx);
        });
        pump_until(cx, || {
            list.read_with(cx, |list, _| {
                matches!(list.token_state, TokenState::Failed(_))
            })
        });
        list.read_with(cx, |list, _| {
            let TokenState::Failed(why) = &list.token_state else {
                unreachable!()
            };
            assert!(why.contains("not signed in to ghe.test"), "{why}");
            assert!(matches!(
                list.link,
                Link::Settled(Connection::NotConnected { .. })
            ));
        });
        assert_eq!(source.tokens.lock().unwrap()[0].2, "bad-token");
    }
}
