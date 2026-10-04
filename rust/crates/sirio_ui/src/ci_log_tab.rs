//! A CI job's log in a read-only Secondary tab (spec B2 §7.4, §8, §15.2):
//! Ely chrome over a gpui `uniform_list` of lines, never wrapped, whose
//! groups fold. The log lives only in this entity: never on disk, never in
//! a trace.

use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::Duration;

use ely_gpui_component::{
    feedback::Callout,
    motion::Skeleton,
    primitives::{Icon as EIcon, IconName, Severity},
    theme::IconSize as EIconSize,
};
use gpui::{
    AnyElement, ClipboardItem, Context, EventEmitter, FontStyle, FontWeight, HighlightStyle,
    Hsla, IntoElement, ListHorizontalSizingBehavior, ParentElement, Render, Rgba, ScrollStrategy,
    Styled, Task, UnderlineStyle, UniformListScrollHandle, Window, div, prelude::*, px, uniform_list,
};
use sirio_forge::{ChangeRef, CheckJob, Forge, ForgeClient, ForgeError, LOG_TAIL_BYTES};
use sirio_theme::Theme;

use crate::ansi_log::{self, LogColor, LogDoc, LogFlavor, LogStyle, Mark};
use crate::change_request_tab::Slot;
use crate::ely_ui;
use crate::forge_source;
use crate::horizontal_scroll::HorizontalBarState;
use crate::text_selection::selectable_text;

/// How often a running job's log is read again while the tab is drawn.
const RELOAD_EVERY: Duration = Duration::from_secs(5);

pub enum CiLogTabEvent {
    Close,
}

pub struct Loaded {
    doc: LogDoc,
    dropped: u64,
    complete: bool,
    published: bool,
}

pub struct CiLogTab {
    reference: ChangeRef,
    job: CheckJob,
    name: String,
    web_url: Option<String>,
    started: bool,
    client: Option<Arc<ForgeClient>>,
    unreachable: Option<String>,
    log: Slot<Loaded>,
    folded: BTreeSet<usize>,
    folds_chosen: bool,
    scroll: UniformListScrollHandle,
    vertical_bar: Option<bezel::ui::scroll::ScrollbarState>,
    horizontal_bar: HorizontalBarState,
    load_task: Option<Task<()>>,
    /// A read is on its way: a tick waits for it rather than starting a
    /// second download of the same log beside it.
    loading: bool,
    connect_task: Option<Task<()>>,
    generation: u64,
    paused_until: Option<i64>,
    renders: u64,
    renders_at_tick: u64,
    ticking: bool,
    copied: usize,
}

impl EventEmitter<CiLogTabEvent> for CiLogTab {}

impl CiLogTab {
    pub fn new(reference: ChangeRef, job: CheckJob, name: String, web_url: Option<String>) -> Self {
        Self {
            reference,
            job,
            name,
            web_url,
            started: false,
            client: None,
            unreachable: None,
            log: Slot::Idle,
            folded: BTreeSet::new(),
            folds_chosen: false,
            scroll: UniformListScrollHandle::new(),
            vertical_bar: None,
            horizontal_bar: Default::default(),
            load_task: None,
            loading: false,
            connect_task: None,
            generation: 0,
            paused_until: None,
            renders: 0,
            renders_at_tick: 0,
            ticking: false,
            copied: 0,
        }
    }

    pub fn reference(&self) -> &ChangeRef {
        &self.reference
    }

    pub fn job(&self) -> &CheckJob {
        &self.job
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn web_url(&self) -> Option<&str> {
        self.web_url.as_deref()
    }

    /// `test · #101`.
    pub fn tab_title(&self) -> String {
        format!("{} · {}", self.name, self.reference.label())
    }

    /// The tab was shown: the first time, it connects and loads (a restored
    /// tab loads when shown, A §8).
    pub fn on_selected(&mut self, cx: &mut Context<Self>) {
        if !self.started {
            self.connect(cx);
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
            let answer = cx.background_spawn(async move { source.client_for(&reference) }).await;
            let _ = this.update(cx, |tab, cx| match answer {
                Ok(client) => {
                    tab.client = Some(client);
                    tab.load(cx);
                }
                Err(connection) => {
                    tab.unreachable = Some(crate::change_request_tab::unreachable_text(&connection));
                    cx.notify();
                }
            });
        }));
        cx.notify();
    }

    /// Reads the log again; the parse runs off the UI thread.
    pub fn load(&mut self, cx: &mut Context<Self>) {
        let Some(client) = self.client.clone() else {
            self.connect(cx);
            return;
        };
        if self.rate_paused() {
            return;
        }
        self.generation += 1;
        let generation = self.generation;
        let job = self.job.clone();
        let flavor = match self.reference.forge {
            Forge::GitHub => LogFlavor::GitHub,
            Forge::GitLab => LogFlavor::GitLab,
        };
        self.log.begin();
        self.loading = true;
        self.load_task = Some(cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    client.job_log(&job).map(|log| Loaded {
                        doc: ansi_log::parse(&log.bytes, flavor),
                        dropped: log.dropped,
                        complete: log.complete,
                        published: log.published,
                    })
                })
                .await;
            let _ = this.update(cx, |tab, cx| {
                if tab.generation != generation {
                    return;
                }
                tab.loading = false;
                if let Err(ForgeError::RateLimited { reset_at, .. }) = &result {
                    tab.paused_until = Some(reset_at.unwrap_or_else(|| now_unix() + 60));
                }
                tab.log.finish(result);
                if !tab.folds_chosen
                    && let Some(loaded) = tab.log.value()
                {
                    tab.folded = ansi_log::default_folds(&loaded.doc);
                    tab.folds_chosen = true;
                }
                cx.notify();
            });
        }));
        cx.notify();
    }

    fn rate_paused(&self) -> bool {
        self.paused_until.is_some_and(|until| now_unix() < until)
    }

    fn running(&self) -> bool {
        self.log.value().is_some_and(|loaded| !loaded.complete)
    }

    /// While the job runs, a reload every 5 s — only if the tab was drawn
    /// since the last tick (the Changes surface's rule): a hidden tab costs
    /// the forge nothing.
    fn ensure_ticking(&mut self, cx: &mut Context<Self>) {
        if self.ticking || !self.running() {
            return;
        }
        self.ticking = true;
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(RELOAD_EVERY).await;
                let carry_on = this.update(cx, |tab, cx| {
                    if !tab.running() {
                        tab.ticking = false;
                        return false;
                    }
                    // The drawn mark is spent only by a read that starts: a
                    // tick during a rate limit leaves it for the first tick
                    // after the reset.
                    if tab.renders != tab.renders_at_tick && !tab.loading && !tab.rate_paused() {
                        tab.renders_at_tick = tab.renders;
                        tab.load(cx);
                    }
                    true
                });
                if !matches!(carry_on, Ok(true)) {
                    return;
                }
            }
        })
        .detach();
    }

    pub fn toggle(&mut self, group: usize, cx: &mut Context<Self>) {
        if !self.folded.remove(&group) {
            self.folded.insert(group);
        }
        cx.notify();
    }

    /// Unfolds what hides the first error and scrolls it to the top.
    pub fn jump_to_first_error(&mut self, cx: &mut Context<Self>) {
        let Some(loaded) = self.log.value() else { return };
        let Some(line) = loaded.doc.first_error else { return };
        let mut holder = loaded.doc.lines[line].group;
        while let Some(group) = holder {
            self.folded.remove(&group);
            holder = loaded.doc.groups[group].parent;
        }
        let rows = ansi_log::visible_lines(&loaded.doc, &self.folded);
        if let Some(row) = rows.iter().position(|visible| *visible == line) {
            self.scroll.scroll_to_item(row, ScrollStrategy::Top);
        }
        cx.notify();
    }

    fn copy(&mut self, text: String, cx: &mut Context<Self>) {
        self.copied = text.len();
        cx.write_to_clipboard(ClipboardItem::new_string(text));
    }

    pub fn copy_all(&mut self, cx: &mut Context<Self>) {
        if let Some(loaded) = self.log.value() {
            let text = ansi_log::plain_text(&loaded.doc);
            self.copy(text, cx);
        }
    }

    pub fn copy_group(&mut self, group: usize, cx: &mut Context<Self>) {
        if let Some(loaded) = self.log.value() {
            let text = ansi_log::group_text(&loaded.doc, group);
            self.copy(text, cx);
        }
    }

    /// The socket's way in: the same handlers the toolbar and the chevrons call.
    pub fn control_view(
        &mut self,
        toggle: Option<usize>,
        jump_error: bool,
        refresh: bool,
        copy: Option<&str>,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        if refresh {
            self.load(cx);
        }
        if let Some(group) = toggle {
            let known = self.log.value().is_some_and(|loaded| group < loaded.doc.groups.len());
            if !known {
                return Err(format!("no group {group}"));
            }
            self.toggle(group, cx);
        }
        if jump_error {
            self.jump_to_first_error(cx);
        }
        match copy {
            None => {}
            Some("all") => self.copy_all(cx),
            Some(group) => {
                let group = group.parse().map_err(|_| format!("copy takes all or a group number, not {group}"))?;
                self.copy_group(group, cx);
            }
        }
        Ok(())
    }

    pub fn report(&self) -> Vec<(String, String)> {
        let state = if self.unreachable.is_some() {
            "unreachable"
        } else {
            match &self.log {
                Slot::Idle => "idle",
                Slot::Loading => "loading",
                Slot::Failed(_) | Slot::Loaded { stale: Some(_), .. } => "error",
                Slot::Loaded { value, .. } if !value.published => "not-published",
                Slot::Loaded { .. } => "loaded",
            }
        };
        let loaded = self.log.value();
        let doc = loaded.map(|loaded| &loaded.doc);
        let visible = doc.map(|doc| ansi_log::visible_lines(doc, &self.folded));
        let scroll = self.scroll.0.borrow();
        let top_index = scroll.deferred_scroll_to_item.as_ref().map_or_else(
            || scroll.base_handle.logical_scroll_top().0,
            |pending| pending.item_index,
        );
        // Lines that all fit leave nothing to scroll: a jump keeps row 0 on
        // top, and the error is on screen anyway.
        let scrollable = scroll.base_handle.max_offset().y > px(0.0);
        let top = visible
            .as_ref()
            .and_then(|rows| rows.get(top_index).copied())
            .map_or("-".to_string(), |line| line.to_string());
        let message = match &self.log {
            Slot::Failed(error) => error.to_string(),
            Slot::Loaded { stale: Some(error), .. } => error.to_string(),
            _ => self.unreachable.clone().unwrap_or_default(),
        };
        vec![
            ("title".into(), self.tab_title()),
            ("state".into(), state.into()),
            (
                "job".into(),
                if loaded.is_some_and(|loaded| loaded.complete) {
                    "complete"
                } else {
                    "running"
                }.into(),
            ),
            ("lines".into(), doc.map_or(0, |doc| doc.lines.len()).to_string()),
            ("visible".into(), visible.as_ref().map_or(0, Vec::len).to_string()),
            ("groups".into(), doc.map_or(0, |doc| doc.groups.len()).to_string()),
            (
                "folded".into(),
                if self.folded.is_empty() {
                    "-".into()
                } else {
                    self.folded.iter().map(usize::to_string).collect::<Vec<_>>().join(",")
                },
            ),
            (
                "first_error".into(),
                doc.and_then(|doc| doc.first_error)
                    .map_or("-".into(), |line| line.to_string()),
            ),
            ("top".into(), top),
            ("scrollable".into(), if scrollable { "yes" } else { "no" }.into()),
            (
                "error_shown".into(),
                match (doc.and_then(|doc| doc.first_error), visible.as_ref()) {
                    (Some(line), Some(rows)) if rows.contains(&line) => "yes",
                    _ => "no",
                }.into(),
            ),
            ("dropped".into(), loaded.map_or(0, |loaded| loaded.dropped).to_string()),
            (
                "truncated".into(),
                if loaded.is_some_and(|loaded| loaded.dropped > 0) {
                    "yes"
                } else {
                    "no"
                }.into(),
            ),
            ("message".into(), message),
            ("copied".into(), self.copied.to_string()),
        ]
    }
}

fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs() as i64)
}

/// Every row the same height: the list measures one and trusts it.
const ROW_HEIGHT: f32 = 18.0;

fn colour(colour: LogColor, theme: &Theme) -> Hsla {
    match colour {
        LogColor::Ansi(index) => theme.ely.ansi[usize::from(index.min(15))],
        LogColor::Indexed(index) => {
            let (red, green, blue) = ansi_log::indexed_rgb(index);
            rgb_hsla(red, green, blue)
        }
        LogColor::Rgb(red, green, blue) => rgb_hsla(red, green, blue),
    }
}

fn rgb_hsla(red: u8, green: u8, blue: u8) -> Hsla {
    Rgba {
        r: f32::from(red) / 255.0,
        g: f32::from(green) / 255.0,
        b: f32::from(blue) / 255.0,
        a: 1.0,
    }.into()
}

fn highlight(style: &LogStyle, theme: &Theme) -> HighlightStyle {
    HighlightStyle {
        color: style.fg.map(|fg| colour(fg, theme)),
        background_color: style.bg.map(|bg| colour(bg, theme)),
        font_weight: style.bold.then_some(FontWeight::BOLD),
        font_style: style.italic.then_some(FontStyle::Italic),
        underline: style.underline.then_some(UnderlineStyle { thickness: px(1.0), ..Default::default() }),
        fade_out: style.dim.then_some(0.4),
        ..Default::default()
    }
}

impl Render for CiLogTab {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        crate::ely::sync_theme_if_changed(cx);
        if !self.started {
            cx.defer_in(window, |tab, _window, cx| tab.connect(cx));
        }
        self.renders = self.renders.wrapping_add(1);
        self.ensure_ticking(cx);
        self.vertical_bar.get_or_insert_with(|| {
            bezel::ui::scroll::ScrollbarState::new(bezel::motion::Painter::of(cx))
        });
        let theme = Theme::get(cx).clone();
        let entity = cx.entity();
        div()
            .id("ci-log")
            .debug_selector(|| "ci-log".to_owned())
            .size_full()
            .flex()
            .flex_col()
            .bg(theme.ely.bg)
            .text_color(theme.ely.fg)
            .child(self.render_toolbar(&theme, &entity))
            .child(self.render_body(&theme, &entity))
    }
}

impl CiLogTab {
    fn render_toolbar(&self, theme: &Theme, entity: &gpui::Entity<Self>) -> AnyElement {
        let loaded = self.log.value().is_some();
        let has_error = self.log.value().is_some_and(|log| log.doc.first_error.is_some());
        let complete = self.log.value().is_some_and(|log| log.complete);
        div().flex().items_center().px(px(12.0)).py(px(6.0)).gap(px(8.0))
            .border_b_1().border_color(theme.ely.border)
            .child(div().text_sm().font_weight(FontWeight::SEMIBOLD).child(self.name.clone()))
            .child(div().text_sm().text_color(if complete { theme.ely.fg_subtle } else { theme.ely.warning })
                .child(if complete { "complete" } else { "running" }))
            .child(div().flex_1())
            .child(ely_ui::icon_button("ci-log-refresh", IconName::RefreshCw, "Refresh", true, {
                let entity = entity.clone();
                move |_, cx| entity.update(cx, |tab, cx| tab.load(cx))
            }))
            .child(ely_ui::icon_button("ci-log-jump-error", IconName::CircleAlert, "Jump to first error", has_error, {
                let entity = entity.clone();
                move |_, cx| entity.update(cx, |tab, cx| tab.jump_to_first_error(cx))
            }))
            .child(ely_ui::icon_button("ci-log-copy", IconName::Copy, "Copy log", loaded, {
                let entity = entity.clone();
                move |_, cx| entity.update(cx, |tab, cx| tab.copy_all(cx))
            }))
            .child(ely_ui::icon_button("ci-log-browser", IconName::ExternalLink, "Open in browser", self.web_url.is_some(), {
                let url = self.web_url.clone();
                move |_, cx| if let Some(url) = &url { cx.open_url(url) }
            }))
            .into_any_element()
    }

    fn render_retry(&self, connect: bool, entity: &gpui::Entity<Self>) -> AnyElement {
        let view = entity.clone();
        ely_ui::text_button("ci-log-retry", "Retry", None, ely_ui::ButtonState::IDLE, move |_, cx| {
            view.update(cx, |tab, cx| {
                if connect { tab.connect(cx); } else { tab.load(cx); }
            });
        })
    }

    fn render_close(&self, entity: &gpui::Entity<Self>) -> AnyElement {
        let view = entity.clone();
        ely_ui::text_button("ci-log-close", "Close", None, ely_ui::ButtonState::IDLE, move |_, cx| {
            view.update(cx, |_, cx| cx.emit(CiLogTabEvent::Close));
        })
    }

    fn render_failure(&self, text: String, connect: bool, entity: &gpui::Entity<Self>) -> AnyElement {
        div().p(px(12.0)).flex().flex_col().gap(px(8.0))
            .child(Callout::new(Severity::Danger).title("Log unavailable")
                .child(selectable_text(text)))
            .child(self.render_retry(connect, entity))
            .child(self.render_close(entity))
            .into_any_element()
    }

    fn render_body(&self, theme: &Theme, entity: &gpui::Entity<Self>) -> AnyElement {
        let panel = || div().p(px(12.0)).flex().flex_col().gap(px(8.0));
        if let Some(text) = &self.unreachable {
            return self.render_failure(text.clone(), true, entity);
        }
        if let Slot::Failed(error) = &self.log {
            return self.render_failure(error.to_string(), false, entity);
        }
        let Some(loaded) = self.log.value() else {
            return panel().children((0..12usize).map(|row| {
                Skeleton::new(("ci-log-skeleton", row))
                    .h(px(12.0))
                    .w(gpui::relative([0.60, 0.85, 0.40][row % 3]))
            })).into_any_element();
        };
        if !loaded.published {
            if let Slot::Loaded { stale: Some(error), .. } = &self.log {
                return self.render_failure(error.to_string(), false, entity);
            }
            return panel()
                .child(ely_ui::message(Severity::Info,
                    "The log is not available until the job finishes.".into(), theme))
                .child(self.render_retry(false, entity))
                .into_any_element();
        }
        let mut body = div().flex_1().min_h(px(0.0)).flex().flex_col();
        if let Slot::Loaded { stale: Some(error), .. } = &self.log {
            body = body.child(self.render_failure(error.to_string(), false, entity));
        }
        if loaded.dropped > 0 {
            let url = self.web_url.clone();
            body = body.child(panel()
                .child(ely_ui::message(Severity::Info, format!(
                    "Showing the last {} MiB of the log; {} MiB before it are on the forge.",
                    LOG_TAIL_BYTES / (1024 * 1024), (loaded.dropped + 524_288) / 1_048_576,
                ), theme))
                .child(ely_ui::text_button("ci-log-truncated-browser", "Open in browser", Some(IconName::ExternalLink),
                    ely_ui::ButtonState::enabled(url.is_some()), move |_, cx| {
                        if let Some(url) = &url { cx.open_url(url); }
                    })));
        }
        let rows = ansi_log::visible_lines(&loaded.doc, &self.folded);
        let digits = loaded.doc.lines.len().max(1).to_string().len();
        let longest = rows.iter().enumerate()
            .max_by_key(|(_, line)| loaded.doc.lines[**line].text.len())
            .map(|(row, _)| row);
        let view = entity.clone();
        let list = uniform_list("ci-log-lines", rows.len(), move |range, _window, cx| {
            let tab = view.read(cx);
            let theme = Theme::get(cx).clone();
            let Some(loaded) = tab.log.value() else { return Vec::new() };
            range.filter_map(|row| rows.get(row).copied())
                .map(|line| tab.render_line(line, &loaded.doc, digits, &theme, &view))
                .collect()
        })
        .debug_selector(|| "ci-log-lines".to_owned())
        .with_horizontal_sizing_behavior(ListHorizontalSizingBehavior::Unconstrained)
        .with_width_from_item(longest)
        .track_scroll(&self.scroll)
        .size_full()
        .font_family(theme.typography.code_family)
        .text_size(px(12.0))
        .line_height(px(ROW_HEIGHT));
        let handle = self.scroll.0.borrow().base_handle.clone();
        body.child(div().flex_1().min_h(px(0.0)).relative()
            .child(list)
            .child(crate::file_view::scrollbar("ci-log-bar", &handle,
                self.vertical_bar.as_ref().expect("bar initialized by render")))
            .child(crate::file_view::horizontal_scrollbar("ci-log-horizontal-bar", &handle,
                &self.horizontal_bar, entity.clone())))
            .into_any_element()
    }

    fn render_line(
        &self,
        line: usize,
        doc: &LogDoc,
        digits: usize,
        theme: &Theme,
        view: &gpui::Entity<Self>,
    ) -> AnyElement {
        let row = &doc.lines[line];
        let mut rendered = div().h(px(ROW_HEIGHT)).flex().items_center().gap(px(12.0)).whitespace_nowrap()
            .child(div().w(px(digits as f32 * 8.0 + 12.0)).flex_none().text_right()
                .text_color(theme.ely.fg_subtle).child((line + 1).to_string()));
        if let Some(group) = row.opens {
            let folded = self.folded.contains(&group);
            let toggle_view = view.clone();
            let copy_view = view.clone();
            rendered = rendered.child(div().id(("ci-log-group", group))
                .debug_selector(move || format!("ci-log-group-{group}"))
                .flex().items_center().gap(px(6.0)).cursor_pointer()
                .on_click(move |_, _, cx| toggle_view.update(cx, |tab, cx| tab.toggle(group, cx)))
                .child(EIcon::new(if folded { IconName::ChevronRight } else { IconName::ChevronDown }).size(EIconSize::Sm))
                .child(div().text_color(theme.ely.fg).font_weight(FontWeight::SEMIBOLD).child(row.text.clone())))
                .child(div().id(("ci-log-group-copy", group)).child(
                    ely_ui::icon_button("ci-log-group-copy", IconName::Copy, "Copy group", true,
                        move |_, cx| copy_view.update(cx, |tab, cx| tab.copy_group(group, cx)))));
        } else {
            let tone = match row.mark {
                Some(Mark::Error) => theme.ely.danger,
                Some(Mark::Warning) => theme.ely.warning,
                Some(Mark::Notice) => theme.ely.info,
                None => theme.ely.fg,
            };
            rendered = rendered.child(div().text_color(tone).child(
                selectable_text(row.text.clone()).id(("ci-log-line", line))
                    .highlights(row.spans.iter().map(|(range, style)| {
                        (range.clone(), highlight(style, theme))
                    }).collect())));
        }
        rendered.into_any_element()
    }
}
