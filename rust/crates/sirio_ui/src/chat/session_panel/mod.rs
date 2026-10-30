//! The session panel: a column docked beside the transcript and composer.
//!
//! It holds one card per section, stacked and scrolling as a whole; a card
//! opens and closes from its title. `Chat::session_panel_sections` is the one
//! list of sections, in the order they are drawn (Environment, then Context,
//! Connectors, Trace and Change request as they land), and both the drawing
//! and the control snapshot read it, so a section appears in each at once.
//! Open or closed is the chat's own and survives a relaunch
//! (`SessionTabState`); which cards are folded does not: every card but
//! Environment starts folded.

mod change_request;
mod connectors;
mod context;
mod environment;
mod trace;

use super::composer_row::RowLayout;
use super::*;
use ely_gpui_component::{
    primitives::{Icon, IconName},
    theme::{ActiveTheme as _, IconSize as ElyIconSize},
};
use crate::text_selection::selectable_text;

/// The column's width, fixed in this version.
pub(super) const COLUMN_WIDTH: f32 = 300.0;
/// Below this chat width the column is laid over the transcript instead of
/// docked beside it, so the transcript and the composer keep a readable width.
const MIN_CHAT_WIDTH: f32 = 640.0;
/// What an overlaid column leaves of the chat to its left, at the most.
const OVERLAY_MARGIN: f32 = 16.0;

/// The cards that start folded: Environment is the only one open. A key here
/// is a section's fold wherever its card is drawn yet or not, since some are
/// drawn only once the agent or the forge has answered.
const FOLDED_AT_START: [&str; 4] = ["context", "connectors", "trace", "change_request"];

/// What the debug control verb and the tests can do to the column.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SessionPanelAction {
    Open,
    Close,
    /// Folds or unfolds the section or node with this key.
    Toggle(String),
    /// Runs the action of the row with this key.
    Activate(String),
}

#[derive(Clone, Debug, Default, PartialEq, serde::Serialize)]
pub struct SessionPanelSnapshot {
    pub open: bool,
    /// Whether the column is on screen: open and the chat laid out.
    pub drawn: bool,
    /// Drawn over the transcript rather than docked beside it, because the
    /// chat is narrower than `MIN_CHAT_WIDTH`.
    pub overlay: bool,
    pub sections: Vec<SectionSnapshot>,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct SectionSnapshot {
    /// `environment`, `context`, `connectors`, `trace` or `change_request`.
    pub key: String,
    pub expanded: bool,
    /// The short line beside the title.
    pub summary: String,
    /// A line under the title that qualifies the card's numbers.
    pub note: String,
    pub rows: Vec<RowSnapshot>,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct RowSnapshot {
    pub key: String,
    pub depth: u8,
    pub label: String,
    pub detail: String,
    pub tag: String,
    pub expanded: Option<bool>,
    pub action: Option<String>,
    /// A line under the row that wraps in full: what a failed action said.
    pub note: String,
}

/// One line of a card, as it is drawn and as the snapshot reports it.
pub(super) struct Row {
    pub(super) key: String,
    pub(super) depth: u8,
    pub(super) icon: Option<IconName>,
    pub(super) label: String,
    pub(super) detail: String,
    /// A small word beside the label: a file's type, a plugin's counts.
    pub(super) tag: String,
    /// `Some` for a node that folds, with whether it is open.
    pub(super) expanded: Option<bool>,
    /// What activating the row does, named for the control verb.
    pub(super) action: Option<String>,
    /// The file activating the row opens.
    pub(super) path: Option<PathBuf>,
    /// Drawn dimmer: it takes no room in the window.
    pub(super) muted: bool,
    /// The chart hue of the dot that ties the row to its bar segment.
    pub(super) swatch: Option<usize>,
    /// A group title rather than a line of the card.
    pub(super) heading: bool,
    /// The colour of the status dot before the label.
    pub(super) tone: Option<Tone>,
    /// Draws the action's name at the row's end, as a row whose action is
    /// not obvious from its label needs. The dot already says the state, so
    /// the action takes the place of the state's word.
    pub(super) show_action: bool,
    /// The word drawn at the row's end in place of the action and of
    /// `detail`, which the snapshot keeps in full: a busy connector's
    /// `waiting…`.
    pub(super) short: Option<String>,
    /// Draws `detail` as a muted second line under the label, indented past
    /// the icon, instead of at the row's end.
    pub(super) detail_below: bool,
    /// A muted line under the row, wrapping rather than truncated.
    pub(super) note: String,
}

/// How a status dot reads, resolved to a theme colour when it is drawn.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Tone {
    Good,
    Warn,
    Bad,
    Quiet,
}

impl Row {
    pub(super) fn new(key: &str, label: String, detail: &str) -> Self {
        Self {
            key: key.to_string(),
            depth: 0,
            icon: None,
            label,
            detail: detail.to_string(),
            tag: String::new(),
            expanded: None,
            action: None,
            path: None,
            muted: false,
            swatch: None,
            heading: false,
            tone: None,
            show_action: false,
            short: None,
            detail_below: false,
            note: String::new(),
        }
    }

    /// What the line draws at its end, and whether that is the action to
    /// take: the short word of a call in flight, else the action's name when
    /// the row shows it and has one, else the detail. Nothing for a row with
    /// none of them or whose detail has its own line.
    pub(super) fn end(&self) -> Option<(&str, bool)> {
        if let Some(word) = &self.short {
            return Some((word, false));
        }
        if self.show_action
            && let Some(action) = &self.action
        {
            return Some((action, true));
        }
        if self.detail_below {
            return None;
        }
        (!self.detail.is_empty()).then_some((self.detail.as_str(), false))
    }
}

/// One card: its stable key, its title, the short summary beside the title
/// and its rows.
pub(super) struct Section {
    pub(super) key: &'static str,
    pub(super) title: &'static str,
    pub(super) summary: String,
    /// A muted line under the title, drawn with the card's rows.
    pub(super) note: String,
    /// The stacked bar under the title: one segment per entry.
    pub(super) bar: Vec<BarSegment>,
    pub(super) rows: Vec<Row>,
    /// Drawn by the card itself rather than from `rows`, which the snapshot
    /// still reports: the Trace card's waterfall.
    pub(super) custom_body: bool,
}

/// One segment of a card's bar: its share of the whole and its chart hue.
pub(super) struct BarSegment {
    pub(super) fraction: f32,
    pub(super) hue: usize,
}

/// What the probe, which paints with the chat and only then, leaves behind.
/// A chat in a background tab or on another worktree is not painted, so
/// what only a painted column should do reads it.
#[derive(Default)]
pub(super) struct Frame {
    /// The chat's width as last measured; `None` until the first frame has
    /// been laid out.
    width: Cell<Option<Pixels>>,
    /// Set by every paint; whoever acts on it takes it (`replace(false)`),
    /// so it says whether a frame came since.
    painted: Cell<bool>,
    /// A read the change request owes to the next frame that paints.
    read_on_paint: Cell<bool>,
    /// Set by every paint like `painted`, and cleared only by the trace's
    /// own once-a-second timer: the Change request's loop takes `painted`
    /// with `replace(false)`, and two readers of one flag would starve each
    /// other.
    trace_painted: Cell<bool>,
    /// A delayed frame of the trace's animation is on its way, which is how
    /// it stays at 30 frames a second or fewer. Shared with the loop's timer
    /// task, which clears it.
    trace_frame_pending: crate::trace_frame::Pending,
}

/// Where the column is drawn, which the chat's width decides.
pub(super) enum Placement {
    /// Beside the transcript and composer, which narrow to make room.
    Docked,
    /// Over the transcript's right edge, this wide; the composer stays free.
    Overlay(Pixels),
}

/// The column's state on a `Chat`.
pub(super) struct SessionPanel {
    open: bool,
    /// Keys of the sections the reader folded.
    collapsed: HashSet<String>,
    /// Keys of the nodes whose fold state is the opposite of their default
    /// (a top-level node starts open, a deeper one closed), so a toggle is a
    /// flip and a new report keeps what the reader opened: keys are stable.
    flipped: HashSet<String>,
    context: context::Context,
    connectors: connectors::Connectors,
    change_request: change_request::ChangeRequest,
    /// What the frames the chat is painted in have told it.
    frame: Rc<Frame>,
    environment: environment::Environment,
    environment_task: Option<gpui::Task<()>>,
    /// The frames the trace card's body has asked for, shared with the
    /// canvas that asks (a plain `Cell` cannot reach it).
    trace_frames: Rc<Cell<u32>>,
    /// The once-a-second tick of the folded title, alive while a turn runs.
    trace_title_task: Option<gpui::Task<()>>,
}

impl SessionPanel {
    /// Forgets the reported context and connectors: a new client is a new
    /// session.
    pub(super) fn clear_session(&mut self) {
        self.context.clear();
        self.connectors.clear();
    }

    pub(super) fn new() -> Self {
        Self {
            open: false,
            // Every card but Environment starts folded: the column opens as
            // a list of titles with their summaries, and the reader unfolds
            // what they want. `trace` is folded ahead of its card.
            collapsed: FOLDED_AT_START.into_iter().map(str::to_string).collect(),
            flipped: HashSet::new(),
            context: context::Context::default(),
            connectors: connectors::Connectors::default(),
            change_request: change_request::ChangeRequest::default(),
            frame: Rc::new(Frame::default()),
            environment: environment::Environment::default(),
            environment_task: None,
            trace_frames: Rc::new(Cell::new(0)),
            trace_title_task: None,
        }
    }
}

impl Chat {
    pub fn session_panel_open(&self) -> bool {
        self.session_panel.open
    }

    /// Opens or closes the column. Opening reads the environment again; the
    /// workspace hears of a change through `ChatEvent::SessionPanelToggled`
    /// so it can save the layout.
    pub fn set_session_panel_open(&mut self, open: bool, cx: &mut Context<Self>) {
        if self.session_panel.open == open {
            return;
        }
        self.session_panel.open = open;
        if open {
            self.refresh_session_environment(cx);
            self.ensure_session_context();
            self.reopen_session_connectors(cx);
            self.probe_session_change_request_forge(cx);
            self.start_session_change_request(cx);
        } else {
            self.stop_session_change_request();
            self.session_panel.connectors.stop_backoff();
        }
        cx.emit(ChatEvent::SessionPanelToggled);
        cx.notify();
    }

    /// Puts the column back as a restored tab had it. It tells nobody: the
    /// layout it was read from already says so.
    pub fn restore_session_panel_open(&mut self, open: bool, cx: &mut Context<Self>) {
        self.session_panel.open = open;
        if open {
            self.refresh_session_environment(cx);
            self.ensure_session_context();
            self.reopen_session_connectors(cx);
            self.probe_session_change_request_forge(cx);
            self.start_session_change_request(cx);
        } else {
            self.stop_session_change_request();
            self.session_panel.connectors.stop_backoff();
        }
        cx.notify();
    }

    pub(super) fn toggle_session_panel(&mut self, cx: &mut Context<Self>) {
        self.set_session_panel_open(!self.session_panel.open, cx);
    }

    /// What the debug control verb does to the column.
    pub fn session_panel_act(
        &mut self,
        action: SessionPanelAction,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        match action {
            SessionPanelAction::Open => self.set_session_panel_open(true, cx),
            SessionPanelAction::Close => self.set_session_panel_open(false, cx),
            SessionPanelAction::Toggle(key) => {
                if !self.session_panel.open {
                    return Err("the session panel is closed".to_string());
                }
                let foldable = self.session_panel_sections().iter().any(|section| {
                    section.key == key
                        || section
                            .rows
                            .iter()
                            .any(|row| row.key == key && row.expanded.is_some())
                });
                if !foldable {
                    return Err(format!("no section or node with key {key}"));
                }
                self.toggle_session_fold(&key, cx);
            }
            SessionPanelAction::Activate(key) => {
                if !self.session_panel.open {
                    return Err("the session panel is closed".to_string());
                }
                return self.activate_session_row(&key, cx);
            }
        }
        Ok(())
    }

    /// The cards, in the order they are drawn. The sections that need the
    /// agent (Context, Connectors, Trace) and the forge (Change request) are
    /// added here as they are built. It runs every frame and for every
    /// snapshot, so it reads only what the panel has cached: no disk, git or
    /// environment work belongs in it.
    fn session_panel_sections(&self) -> Vec<Section> {
        let panel = &self.session_panel;
        let mut sections = vec![environment::section(&panel.environment, &self.agent_cwd)];
        sections.extend(context::section(
            &panel.context,
            self.context_usage.as_ref(),
            self.client.as_ref().is_some_and(ChatClient::supports_connectors),
            self.streaming,
            &panel.flipped,
        ));
        sections.extend(connectors::section(
            &panel.connectors,
            self.client.as_ref().is_some_and(ChatClient::supports_connectors),
            &panel.flipped,
        ));
        sections.extend(trace::section(&self.trace, std::time::Instant::now()));
        sections.extend(change_request::section(&panel.change_request));
        sections
    }

    /// Folds or unfolds the section or node with this key. A section is
    /// folded when it is in `collapsed`; a node is flipped from its default.
    fn toggle_session_fold(&mut self, key: &str, cx: &mut Context<Self>) {
        let is_section = FOLDED_AT_START.contains(&key)
            || self
                .session_panel_sections()
                .iter()
                .any(|section| section.key == key);
        let set = if is_section {
            &mut self.session_panel.collapsed
        } else {
            &mut self.session_panel.flipped
        };
        if !set.remove(key) {
            set.insert(key.to_string());
        }
        // The change request is read only while its card is open.
        if key == "change_request" {
            if self.session_panel.collapsed.contains(key) {
                self.stop_session_change_request();
            } else {
                self.start_session_change_request(cx);
            }
        }
        cx.notify();
    }

    /// Runs the action of the row with this key: a row with a file opens it
    /// in a tab, a connector's row signs in or reconnects.
    fn activate_session_row(&mut self, key: &str, cx: &mut Context<Self>) -> Result<(), String> {
        if key.starts_with("connectors/") {
            return self.activate_connector(key, cx);
        }
        if key.starts_with("change_request/") {
            return self.activate_change_request(key, cx);
        }
        if key.starts_with("trace/") {
            return self.activate_trace(key, cx);
        }
        let sections = self.session_panel_sections();
        let Some(row) = sections
            .iter()
            .flat_map(|section| &section.rows)
            .find(|row| row.key == key && row.action.is_some())
        else {
            return Err(format!("no row with an action has key {key}"));
        };
        match &row.path {
            Some(path) => {
                cx.emit(ChatEvent::OpenFile(path.clone()));
                Ok(())
            }
            None => Err(format!("no handler for {key}")),
        }
    }

    /// Resolves the chat's worktree and reads its branch afresh. The lookup
    /// touches the disk and the read spawns `git`, so this runs when the
    /// column opens and never from a render; the answers are cached on the
    /// panel. The branch is unread until the new read lands, so a stale one
    /// is not shown meanwhile.
    pub(super) fn refresh_session_environment(&mut self, cx: &mut Context<Self>) {
        let environment = &mut self.session_panel.environment;
        environment.worktree = crate::worktree_lookup::lookup(&self.agent_cwd, cx);
        environment.home = std::env::var_os("HOME")
            .or_else(|| std::env::var_os("USERPROFILE"))
            .map(PathBuf::from);
        environment.branch = environment::Branch::Unread;
        self.reread_session_branch(cx);
    }

    /// Reads the branch again, keeping the one shown until the answer
    /// arrives. A turn can switch it, so a turn's end asks while the column
    /// is open.
    pub(super) fn reread_session_branch(&mut self, cx: &mut Context<Self>) {
        let root = self
            .session_panel
            .environment
            .worktree
            .as_ref()
            .map_or_else(|| self.agent_cwd.clone(), |worktree| worktree.root.clone());
        self.session_panel.environment.read_seq += 1;
        self.session_panel.environment_task = Some(cx.spawn(async move |this, cx| {
            let branch = cx
                .background_spawn(async move { environment::read_branch(&root) })
                .await;
            let _ = this.update(cx, |chat, cx| {
                let environment = &mut chat.session_panel.environment;
                let moved = environment.branch != environment::Branch::Unread
                    && environment.branch != branch;
                environment.branch = branch;
                cx.notify();
                // The card follows the branch the line now shows.
                if moved {
                    chat.session_branch_moved(cx);
                }
            });
        }));
    }

    /// Where the column is drawn, or `None` while it is closed or the chat has
    /// not been laid out yet.
    pub(super) fn session_panel_placement(&self) -> Option<Placement> {
        let width = self.session_panel.frame.width.get()?;
        if !self.session_panel.open {
            return None;
        }
        Some(if width >= px(MIN_CHAT_WIDTH) {
            Placement::Docked
        } else {
            Placement::Overlay(px(COLUMN_WIDTH).min((width - px(OVERLAY_MARGIN)).max(px(0.0))))
        })
    }

    /// The chat's width as the probe last measured it; `None` until it has
    /// been laid out.
    pub(super) fn chat_width(&self) -> Option<Pixels> {
        self.session_panel.frame.width.get()
    }

    /// Whether the column is on screen: open, and the chat laid out.
    pub(super) fn session_panel_drawn(&self) -> bool {
        self.session_panel_placement().is_some()
    }

    pub(super) fn session_panel_snapshot(&self) -> SessionPanelSnapshot {
        let panel = &self.session_panel;
        let sections = if panel.open {
            self.session_panel_sections()
                .into_iter()
                .map(|section| SectionSnapshot {
                    expanded: !panel.collapsed.contains(section.key),
                    key: section.key.to_string(),
                    summary: section.summary,
                    note: section.note,
                    rows: section
                        .rows
                        .into_iter()
                        .map(|row| RowSnapshot {
                            key: row.key,
                            depth: row.depth,
                            label: row.label,
                            detail: row.detail,
                            tag: row.tag,
                            expanded: row.expanded,
                            action: row.action,
                            note: row.note,
                        })
                        .collect(),
                })
                .collect()
        } else {
            Vec::new()
        };
        SessionPanelSnapshot {
            open: panel.open,
            drawn: self.session_panel_drawn(),
            overlay: matches!(self.session_panel_placement(), Some(Placement::Overlay(_))),
            sections,
        }
    }

    /// An invisible probe that tells the chat how wide it is: the column
    /// docks from `MIN_CHAT_WIDTH` up and is laid over the transcript below
    /// it, and a change across that, or of an overlay's width, draws again.
    pub(super) fn render_session_panel_probe(&self, cx: &mut Context<Self>) -> AnyElement {
        let frame = self.session_panel.frame.clone();
        let entity = cx.entity();
        let row = self.composer_row.clone();
        canvas(
            move |bounds, _, cx| {
                frame.painted.set(true);
                frame.trace_painted.set(true);
                row.root.set(Some(bounds));
                let width = bounds.size.width;
                let before = frame.width.replace(Some(width));
                let docked = |width: Option<Pixels>| width.map(|w| w >= px(MIN_CHAT_WIDTH));
                // An overlay is as wide as the chat allows, so it follows it.
                let mut moved = docked(before) != docked(Some(width))
                    || (width < px(MIN_CHAT_WIDTH) && before != Some(width));
                // The control row's layout gives way at a chat width; the row
                // reads this width, and never its own, so the layout it picks
                // cannot move the width it was picked from.
                moved |= row.flip.get().is_some_and(|flip| match row.layout.get() {
                    RowLayout::Full => f32::from(width) < flip,
                    RowLayout::Compact => f32::from(width) >= flip,
                });
                if moved {
                    let entity = entity.clone();
                    cx.defer(move |cx| entity.update(cx, |_, cx| cx.notify()));
                }
                if frame.read_on_paint.get() {
                    let entity = entity.clone();
                    cx.defer(move |cx| entity.update(cx, Chat::session_panel_painted));
                }
            },
            |_, _, _, _| {},
        )
        .absolute()
        .size_full()
        .into_any_element()
    }

    /// The column: every section's card, scrolling as a whole. Docked it is
    /// a flex child of the row beside the transcript; as an overlay it is
    /// absolute over the transcript's right edge, on the elevated surface
    /// with a shadow, and takes the pointer so nothing under it is hit.
    pub(super) fn render_session_panel(
        &self,
        placement: &Placement,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let cards: Vec<AnyElement> = self
            .session_panel_sections()
            .iter()
            .map(|section| self.render_session_card(section, theme, cx))
            .collect();
        let column = div()
            .id("session-panel")
            .debug_selector(|| "session-panel".into())
            .h_full()
            .border_l_1()
            .border_color(theme.ely.border)
            .child(
                div()
                    .id("session-panel-scroll")
                    .size_full()
                    .overflow_y_scroll()
                    .p_2()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .children(cards),
            );
        match placement {
            Placement::Docked => column
                .flex_none()
                .w(px(COLUMN_WIDTH))
                .bg(theme.ely.bg)
                .into_any_element(),
            Placement::Overlay(width) => column
                .occlude()
                .absolute()
                .top_0()
                .right_0()
                .bottom_0()
                .w(*width)
                .bg(theme.sirio.floating_surface)
                .shadow_lg()
                .into_any_element(),
        }
    }

    /// One card: a title row that folds it, then its rows.
    fn render_session_card(
        &self,
        section: &Section,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let key = section.key;
        let collapsed = self.session_panel.collapsed.contains(key);
        let entity = cx.entity();
        let chevron = if collapsed {
            IconName::ChevronRight
        } else {
            IconName::ChevronDown
        };
        let caption = theme.typography.footnote;
        let title = div()
            .id(SharedString::from(format!("session-card-title-{key}")))
            .debug_selector(move || format!("session-card-title-{key}"))
            .flex()
            .items_center()
            .gap_1()
            .cursor(CursorStyle::PointingHand)
            .text_size(caption)
            .text_color(theme.ely.fg)
            .font_weight(FontWeight::SEMIBOLD)
            .on_click(move |_, _, cx| {
                entity.update(cx, |chat, cx| chat.toggle_session_fold(key, cx));
            })
            .child(
                Icon::new(chevron)
                    .size(ElyIconSize::Sm)
                    .color(theme.ely.fg_muted),
            )
            .child(section.title)
            .child(div().flex_1())
            .when(!section.summary.is_empty(), |row| {
                if section.custom_body {
                    return row.child(trace::title_summary(
                        self,
                        &section.summary,
                        collapsed,
                        theme,
                        cx,
                    ));
                }
                row.child(
                    div()
                        .font_weight(FontWeight::NORMAL)
                        .text_color(theme.ely.fg_muted)
                        .child(section.summary.clone()),
                )
            });
        div()
            .id(SharedString::from(format!("session-card-{key}")))
            .debug_selector(move || format!("session-card-{key}"))
            .flex()
            .flex_col()
            .gap_1()
            .px_2p5()
            .py_2()
            .rounded(theme.radii.row_card)
            .border_1()
            .border_color(theme.ely.border)
            .bg(theme.ely.surface)
            .child(title)
            .when(!collapsed, |card| {
                // The Trace card draws its own body; its rows are the
                // snapshot's.
                if section.custom_body {
                    return card.child(trace::render_body(self, theme, cx));
                }
                card.when(!section.bar.is_empty(), |card| {
                    card.child(Self::render_session_bar(&section.bar, theme))
                })
                .when(!section.note.is_empty(), |card| {
                    card.child(
                        div()
                            .text_size(caption)
                            .text_color(theme.ely.fg_subtle)
                            .child(section.note.clone()),
                    )
                })
                .children(
                    section
                        .rows
                        .iter()
                        .map(|row| Self::render_session_row(row, theme, cx)),
                )
            })
            .into_any_element()
    }

    /// The stacked bar: one segment per entry, in the chart's hues, on a
    /// track that shows the room left in the window.
    fn render_session_bar(bar: &[BarSegment], theme: &Theme) -> AnyElement {
        div()
            .flex()
            .h(px(6.0))
            .w_full()
            .rounded(px(3.0))
            .overflow_hidden()
            .bg(theme.ely.sunken)
            .children(bar.iter().map(|segment| {
                div()
                    .h_full()
                    .w(relative(segment.fraction))
                    .bg(theme.ely.chart[segment.hue % theme.ely.chart.len()])
            }))
            .into_any_element()
    }

    /// One row. Its text is read, not clicked, so it can be selected; a row
    /// that folds or acts is a clickable one, drawn without selectable text
    /// because a press on it would fire the click after a drag.
    fn render_session_row(row: &Row, theme: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let caption = theme.typography.footnote;
        let key = SharedString::from(format!("session-row-{}", row.key));
        if row.heading {
            return div()
                .id(key.clone())
                .debug_selector({
                    let selector = key.to_string();
                    move || selector.clone()
                })
                .mt_1()
                .text_size(caption)
                .text_color(theme.ely.fg_muted)
                .font_weight(FontWeight::SEMIBOLD)
                .child(row.label.clone())
                .into_any_element();
        }
        let clickable = row.expanded.is_some() || row.action.is_some();
        let text_color = if row.muted {
            theme.ely.fg_subtle
        } else {
            theme.ely.fg
        };
        let is_link = row.path.is_some();
        let label = if clickable {
            let mut label = div().child(row.label.clone());
            if is_link {
                label = label.text_color(theme.ely.link).underline();
            }
            label.into_any_element()
        } else {
            selectable_text(row.label.clone())
                .id(SharedString::from(format!(
                    "session-row-text-{}",
                    row.key
                )))
                .into_any_element()
        };
        // The chevron's room is kept on a context row without one, so the
        // labels of one level line up.
        let chevron = match row.expanded {
            Some(true) => Some(IconName::ChevronDown),
            Some(false) => Some(IconName::ChevronRight),
            None => None,
        };
        let reserves_chevron = row.swatch.is_some() || row.depth > 0;
        let entity = cx.entity();
        let row_key = row.key.clone();
        let folds = row.expanded.is_some();
        let indent = f32::from(row.depth) * 12.0;
        let end = row.end().map(|(word, is_action)| (word.to_string(), is_action));
        let main = div()
            .flex()
            .items_center()
            .gap_2()
            .pl(px(f32::from(row.depth) * 12.0))
            .when(chevron.is_some() || reserves_chevron, |line| {
                line.child(
                    div()
                        .flex_none()
                        .w(px(12.0))
                        .children(chevron.map(|icon| {
                            Icon::new(icon)
                                .size(ElyIconSize::Sm)
                                .color(theme.ely.fg_muted)
                        })),
                )
            })
            .children(row.tone.map(|tone| {
                let color = match tone {
                    Tone::Good => theme.ely.success,
                    Tone::Warn => theme.ely.warning,
                    Tone::Bad => theme.ely.danger,
                    Tone::Quiet => theme.ely.fg_subtle,
                };
                div().flex_none().size(px(8.0)).rounded_full().bg(color)
            }))
            .children(row.swatch.map(|hue| {
                div()
                    .flex_none()
                    .size(px(8.0))
                    .rounded(px(2.0))
                    .bg(theme.ely.chart[hue % theme.ely.chart.len()])
            }))
            .children(row.icon.map(|icon| {
                div().flex_none().child(
                    Icon::new(icon)
                        .size(ElyIconSize::Sm)
                        .color(theme.ely.fg_muted),
                )
            }))
            .child(
                div()
                    .debug_selector({
                        let selector = format!("session-row-label-{}", row.key);
                        move || selector.clone()
                    })
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .child(label),
            )
            .when(!row.tag.is_empty(), |line| {
                line.child(
                    div()
                        .flex_none()
                        .text_color(theme.ely.fg_subtle)
                        .child(row.tag.clone()),
                )
            })
            .children(end.map(|(word, is_action)| {
                div()
                    .flex_none()
                    .text_color(if is_action {
                        theme.ely.link
                    } else {
                        theme.ely.fg_muted
                    })
                    .child(word)
            }));
        let line = div()
            .id(key.clone())
            .debug_selector({
                let selector = key.to_string();
                move || selector.clone()
            })
            .flex()
            .flex_col()
            .text_size(caption)
            .text_color(text_color)
            .when(clickable, |line| {
                line.cursor(CursorStyle::PointingHand)
                    .on_click(move |_, _, cx| {
                        entity.update(cx, |chat, cx| {
                            if folds {
                                chat.toggle_session_fold(&row_key, cx);
                            } else {
                                // A file that cannot be opened has nothing
                                // to say here; the workspace reports it.
                                let _ = chat.activate_session_row(&row_key, cx);
                            }
                        });
                    })
            })
            .child(main)
            // The detail of a card that wants its own line: under the
            // title, past the icon, as muted as a note.
            .when(row.detail_below && !row.detail.is_empty(), |line| {
                line.child(
                    div()
                        .debug_selector({
                            let selector = format!("session-row-detail-{}", row.key);
                            move || selector.clone()
                        })
                        .ml(px(indent + 20.0))
                        .truncate()
                        .text_color(theme.ely.fg_muted)
                        .child(row.detail.clone()),
                )
            });
        if row.note.is_empty() {
            return line.into_any_element();
        }
        // The line's own chevron, dot and gaps are not repeated: the note
        // starts under the label's depth and wraps across the card.
        div()
            .flex()
            .flex_col()
            .gap_0p5()
            .child(line)
            .child(
                div()
                    .pl(px(indent + 20.0))
                    .text_size(caption)
                    .text_color(theme.ely.fg_muted)
                    .child(
                        selectable_text(row.note.clone()).id(SharedString::from(format!(
                            "session-row-note-{}",
                            row.key
                        ))),
                    ),
            )
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::{TempDir, offline_chat_view, refresh_frame};
    use super::*;
    use crate::worktree_lookup::{self, WorktreeInfo};
    use gpui::{Modifiers, TestAppContext, VisualTestContext, size};
    use std::process::Command;

    fn git(dir: &Path, args: &[&str]) {
        let status = Command::new("git")
            .args(["-c", "user.name=t", "-c", "user.email=t@t", "-c", "commit.gpgsign=false"])
            .args(args)
            .current_dir(dir)
            .status()
            .expect("git runs");
        assert!(status.success(), "git {args:?} failed");
    }

    fn click(cx: &mut VisualTestContext, selector: &'static str) {
        let bounds = cx
            .debug_bounds(selector)
            .unwrap_or_else(|| panic!("{selector} is drawn"));
        cx.simulate_click(bounds.center(), Modifiers::none());
        cx.run_until_parked();
        for _ in 0..3 {
            refresh_frame(cx);
        }
    }

    #[gpui::test]
    async fn the_button_opens_and_closes_the_column(cx: &mut TestAppContext) {
        let (chat, cx) = offline_chat_view(cx);
        cx.simulate_resize(size(px(1100.0), px(700.0)));
        for _ in 0..3 {
            refresh_frame(cx);
        }
        assert!(cx.debug_bounds("session-panel").is_none());
        click(cx, "session-panel-toggle");
        assert!(chat.read_with(&cx.cx, |chat, _| chat.session_panel_open()));
        assert!(
            cx.debug_bounds("session-panel").is_some(),
            "the button opens the column"
        );
        click(cx, "session-panel-toggle");
        assert!(!chat.read_with(&cx.cx, |chat, _| chat.session_panel_open()));
        assert!(
            cx.debug_bounds("session-panel").is_none(),
            "the same button closes it"
        );
    }

    #[gpui::test]
    async fn a_narrow_chat_gets_the_column_over_its_transcript_not_its_composer(
        cx: &mut TestAppContext,
    ) {
        let (chat, cx) = offline_chat_view(cx);
        cx.simulate_resize(size(px(600.0), px(700.0)));
        chat.update(cx, |chat, cx| chat.set_session_panel_open(true, cx));
        for _ in 0..3 {
            refresh_frame(cx);
        }
        let snapshot = chat.read_with(&cx.cx, |chat, _| chat.control_snapshot());
        assert!(snapshot.session_panel.open);
        assert!(snapshot.session_panel.drawn, "open and painted is drawn, wide or not");
        assert!(snapshot.session_panel.overlay, "narrow: over the transcript");
        let root = cx.debug_bounds("chat-root").expect("the chat is drawn");
        let panel = cx.debug_bounds("session-panel").expect("the column is drawn");
        assert_eq!(panel.size.width, px(300.0));
        assert_eq!(panel.right(), root.right(), "pinned to the chat's right edge");
        assert!(
            panel.top() >= root.top() && panel.bottom() <= root.bottom(),
            "{panel:?} within {root:?}"
        );
        // The composer and its toggle stay reachable: the column ends above them.
        let composer = cx.debug_bounds("composer-input").expect("the composer is drawn");
        assert!(panel.bottom() <= composer.top(), "{panel:?} over the composer {composer:?}");
        let toggle = cx.debug_bounds("session-panel-toggle").expect("the toggle is drawn");
        assert!(!panel.intersects(&toggle), "{panel:?} over the toggle {toggle:?}");

        // A chat narrower than the column leaves a margin on the left.
        cx.simulate_resize(size(px(280.0), px(700.0)));
        for _ in 0..3 {
            refresh_frame(cx);
        }
        let panel = cx.debug_bounds("session-panel").expect("still drawn");
        assert_eq!(panel.size.width, px(264.0), "the chat's width less 16");

        // The toggle still closes it.
        click(cx, "session-panel-toggle");
        assert!(cx.debug_bounds("session-panel").is_none());

        // Wide again, it docks and the transcript gives it room.
        chat.update(cx, |chat, cx| chat.set_session_panel_open(true, cx));
        cx.simulate_resize(size(px(700.0), px(700.0)));
        for _ in 0..3 {
            refresh_frame(cx);
        }
        let snapshot = chat.read_with(&cx.cx, |chat, _| chat.control_snapshot());
        assert!(snapshot.session_panel.drawn);
        assert!(!snapshot.session_panel.overlay, "wide enough to dock");
        let docked = cx.debug_bounds("session-panel").expect("docked");
        let composer = cx.debug_bounds("composer-input").expect("the composer is drawn");
        assert!(
            composer.right() <= docked.left(),
            "docked, the composer gives up the column's width"
        );
    }

    const CLAUDE_FIXTURE: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../sirio_acp/tests/fixtures/claude_fixture.py"
    );

    /// Every card that needs the agent or the forge is drawn, and all of them
    /// folded but Environment: a new chat's column is a list of titles with
    /// the summaries beside them.
    #[gpui::test]
    async fn cards_start_folded_except_environment(cx: &mut TestAppContext) {
        use crate::forge_source::testing::{self, CannedForge, FakeSource};
        // The CLI's worker thread wakes the chat's event task, which the
        // test scheduler refuses unless it is told real threads are in play.
        cx.executor().allow_parking();
        cx.update(Theme::init);
        cx.update(crate::ely::init);
        cx.update(init);
        let forge = std::sync::Arc::new(CannedForge::default());
        let source = FakeSource::ready(testing::github_client(forge), Some("feat/y"));
        cx.update(|cx| crate::forge_source::set_source(source, cx));
        let (chat, cx) = cx.add_window_view(|_, cx| {
            let launch = sirio_acp::ClaudeLaunch::fixture(
                "python3",
                vec![CLAUDE_FIXTURE.to_string(), "session_panel".to_string()],
            );
            Chat::from_test_command(LaunchSpec::Claude(launch), std::env::temp_dir(), cx)
        });
        chat.update(cx, |chat, cx| chat.set_session_panel_open(true, cx));
        let keys = ["environment", "context", "connectors", "change_request"];
        super::super::tests::pump_chat_until(cx, &chat, |chat| {
            let drawn = chat.control_snapshot().session_panel.sections;
            keys.iter().all(|key| drawn.iter().any(|section| section.key == *key))
        });
        let sections = chat.read_with(&cx.cx, |chat, _| chat.control_snapshot()).session_panel.sections;
        let expanded = |key: &str| sections.iter().find(|section| section.key == key).map(|section| section.expanded);
        assert_eq!(expanded("environment"), Some(true));
        for key in ["context", "connectors", "change_request"] {
            assert_eq!(expanded(key), Some(false), "{key} starts folded");
        }
    }

    #[gpui::test]
    async fn environment_names_the_chats_worktree_not_the_selection(cx: &mut TestAppContext) {
        let dir = TempDir::new();
        let repo = dir.0.join("p");
        let linked = repo.join(".wt").join("a");
        std::fs::create_dir_all(&repo).unwrap();
        git(&repo, &["init", "-q", "-b", "main"]);
        git(&repo, &["commit", "-q", "--allow-empty", "-m", "first"]);
        git(
            &repo,
            &["worktree", "add", "-q", "-b", "feat/x", linked.to_str().unwrap()],
        );
        let cwd = linked.join("src");
        std::fs::create_dir_all(&cwd).unwrap();

        cx.update(Theme::init);
        cx.update(crate::ely::init);
        cx.update(init);
        cx.update(|cx| {
            worktree_lookup::set_worktrees(
                vec![
                    WorktreeInfo {
                        project_name: "p".into(),
                        root: repo.clone(),
                        is_primary: true,
                    },
                    WorktreeInfo {
                        project_name: "p".into(),
                        root: linked.clone(),
                        is_primary: false,
                    },
                ],
                cx,
            )
        });
        let (chat, cx) = cx.add_window_view(|_, cx| {
            Chat::new(
                Some(LaunchSpec::Acp(AgentCommand::new(
                    "/definitely/missing/sirio-acp-agent",
                ))),
                cwd.clone(),
                cx,
            )
        });
        chat.update(cx, |chat, cx| chat.set_session_panel_open(true, cx));
        super::super::tests::pump_chat_until(cx, &chat, |chat| {
            chat.control_snapshot()
                .session_panel
                .sections
                .iter()
                .any(|section| section.rows.iter().any(|row| row.label == "feat/x"))
        });
        let snapshot = chat.read_with(&cx.cx, |chat, _| chat.control_snapshot());
        let environment = &snapshot.session_panel.sections[0];
        assert_eq!(environment.key, "environment");
        let rows: Vec<(&str, &str)> = environment
            .rows
            .iter()
            .map(|row| (row.label.as_str(), row.detail.as_str()))
            .collect();
        assert_eq!(rows[0].0, "p");
        assert!(
            rows[1].0.ends_with("/.wt/a/src"),
            "the directory is the chat's own cwd, not the worktree root: {rows:?}"
        );
        assert_eq!(rows[2], ("feat/x", "worktree"));
        assert_eq!(rows.len(), 3);
    }
}
