//! The session panel: a column docked beside the transcript and composer.
//!
//! It holds one card per section, stacked and scrolling as a whole; a card
//! opens and closes from its title. `Chat::session_panel_sections` is the one
//! list of sections, in the order they are drawn (Environment, then Context,
//! Connectors and Change request as they land), and both the drawing and the
//! control snapshot read it, so a section appears in each at once. Open or
//! closed is the chat's own and survives a relaunch (`SessionTabState`); which
//! cards are folded does not.

mod environment;

use super::*;
use ely_gpui_component::{
    primitives::{Icon, IconName},
    theme::{ActiveTheme as _, IconSize as ElyIconSize},
};
use crate::text_selection::selectable_text;

/// The column's width, fixed in this version.
const COLUMN_WIDTH: f32 = 300.0;
/// Below this chat width the column is not drawn: the transcript and the
/// composer keep the whole width and the open state is kept for later.
const MIN_CHAT_WIDTH: f32 = 640.0;

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
    /// Whether the column is on screen: open and the chat wide enough.
    pub drawn: bool,
    pub sections: Vec<SectionSnapshot>,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct SectionSnapshot {
    /// `environment`, `context`, `connectors` or `change_request`.
    pub key: String,
    pub expanded: bool,
    pub rows: Vec<RowSnapshot>,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct RowSnapshot {
    pub key: String,
    pub depth: u8,
    pub label: String,
    pub detail: String,
    pub expanded: Option<bool>,
    pub action: Option<String>,
}

/// One line of a card, as it is drawn and as the snapshot reports it.
pub(super) struct Row {
    pub(super) key: String,
    pub(super) depth: u8,
    pub(super) icon: Option<IconName>,
    pub(super) label: String,
    pub(super) detail: String,
    /// `Some` for a node that folds, with whether it is open.
    pub(super) expanded: Option<bool>,
    /// What activating the row does, named for the control verb.
    pub(super) action: Option<String>,
}

impl Row {
    pub(super) fn new(key: &str, label: String, detail: &str) -> Self {
        Self {
            key: key.to_string(),
            depth: 0,
            icon: None,
            label,
            detail: detail.to_string(),
            expanded: None,
            action: None,
        }
    }
}

/// One card: its stable key, its title, the short summary beside the title
/// and its rows.
pub(super) struct Section {
    pub(super) key: &'static str,
    pub(super) title: &'static str,
    pub(super) summary: String,
    pub(super) rows: Vec<Row>,
}

/// The column's state on a `Chat`.
pub(super) struct SessionPanel {
    open: bool,
    /// Keys of the sections and nodes the reader folded.
    collapsed: HashSet<String>,
    /// Whether the chat is wide enough for the column, as last measured;
    /// `None` until the first frame has been laid out.
    wide: Rc<Cell<Option<bool>>>,
    environment: environment::Environment,
    environment_task: Option<gpui::Task<()>>,
}

impl SessionPanel {
    pub(super) fn new() -> Self {
        Self {
            open: false,
            collapsed: HashSet::new(),
            wide: Rc::new(Cell::new(None)),
            environment: environment::Environment::default(),
            environment_task: None,
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
                if !self.session_panel.collapsed.remove(&key) {
                    self.session_panel.collapsed.insert(key);
                }
                cx.notify();
            }
            SessionPanelAction::Activate(key) => {
                if !self.session_panel.open {
                    return Err("the session panel is closed".to_string());
                }
                let acts = self.session_panel_sections().iter().any(|section| {
                    section
                        .rows
                        .iter()
                        .any(|row| row.key == key && row.action.is_some())
                });
                if !acts {
                    return Err(format!("no row with an action has key {key}"));
                }
                // No row has a handler yet: the sections that add actions
                // run them here.
                return Err(format!("no handler for {key}"));
            }
        }
        Ok(())
    }

    /// The cards, in the order they are drawn. The sections that need the
    /// agent (Context, Connectors) and the forge (Change request) are added
    /// here as they are built. It runs every frame and for every snapshot, so
    /// it reads only what the panel has cached: no disk, git or environment
    /// work belongs in it.
    fn session_panel_sections(&self) -> Vec<Section> {
        vec![environment::section(
            &self.session_panel.environment,
            &self.agent_cwd,
        )]
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
        self.session_panel.environment_task = Some(cx.spawn(async move |this, cx| {
            let branch = cx
                .background_spawn(async move { environment::read_branch(&root) })
                .await;
            let _ = this.update(cx, |chat, cx| {
                chat.session_panel.environment.branch = branch;
                cx.notify();
            });
        }));
    }

    /// Whether the column is on screen: open, and the chat wide enough.
    pub(super) fn session_panel_drawn(&self) -> bool {
        self.session_panel.open && self.session_panel.wide.get() == Some(true)
    }

    pub(super) fn session_panel_snapshot(&self) -> SessionPanelSnapshot {
        let panel = &self.session_panel;
        let sections = if panel.open {
            self.session_panel_sections()
                .into_iter()
                .map(|section| SectionSnapshot {
                    expanded: !panel.collapsed.contains(section.key),
                    key: section.key.to_string(),
                    rows: section
                        .rows
                        .into_iter()
                        .map(|row| RowSnapshot {
                            key: row.key,
                            depth: row.depth,
                            label: row.label,
                            detail: row.detail,
                            expanded: row.expanded,
                            action: row.action,
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
            sections,
        }
    }

    /// An invisible probe that tells the chat how wide it is: the column
    /// steps aside under `MIN_CHAT_WIDTH`, and a change across it draws again.
    pub(super) fn render_session_panel_probe(&self, cx: &mut Context<Self>) -> AnyElement {
        let wide = self.session_panel.wide.clone();
        let entity = cx.entity();
        canvas(
            move |bounds, _, cx| {
                let now = bounds.size.width >= px(MIN_CHAT_WIDTH);
                if wide.replace(Some(now)) != Some(now) {
                    cx.defer(move |cx| entity.update(cx, |_, cx| cx.notify()));
                }
            },
            |_, _, _, _| {},
        )
        .absolute()
        .size_full()
        .into_any_element()
    }

    /// The column: every section's card, scrolling as a whole.
    pub(super) fn render_session_panel(&self, theme: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let cards: Vec<AnyElement> = self
            .session_panel_sections()
            .iter()
            .map(|section| self.render_session_card(section, theme, cx))
            .collect();
        div()
            .id("session-panel")
            .debug_selector(|| "session-panel".into())
            .flex_none()
            .w(px(COLUMN_WIDTH))
            .h_full()
            .border_l_1()
            .border_color(theme.ely.border)
            .bg(theme.ely.bg)
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
            )
            .into_any_element()
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
                entity.update(cx, |chat, cx| {
                    if !chat.session_panel.collapsed.remove(key) {
                        chat.session_panel.collapsed.insert(key.to_string());
                    }
                    cx.notify();
                });
            })
            .child(
                Icon::new(chevron)
                    .size(ElyIconSize::Sm)
                    .color(theme.ely.fg_muted),
            )
            .child(section.title)
            .child(div().flex_1())
            .when(!section.summary.is_empty(), |row| {
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
                card.children(
                    section
                        .rows
                        .iter()
                        .map(|row| Self::render_session_row(row, theme)),
                )
            })
            .into_any_element()
    }

    /// One row. Its text is read, not clicked, so it can be selected; a row
    /// that folds or acts will be drawn as a clickable one, without it.
    fn render_session_row(row: &Row, theme: &Theme) -> AnyElement {
        let caption = theme.typography.footnote;
        let key = SharedString::from(format!("session-row-{}", row.key));
        div()
            .id(key.clone())
            .debug_selector({
                let selector = key.to_string();
                move || selector.clone()
            })
            .flex()
            .items_center()
            .gap_2()
            .pl(px(f32::from(row.depth) * 12.0))
            .text_size(caption)
            .text_color(theme.ely.fg)
            .children(row.icon.map(|icon| {
                div().flex_none().child(
                    Icon::new(icon)
                        .size(ElyIconSize::Sm)
                        .color(theme.ely.fg_muted),
                )
            }))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .child(selectable_text(row.label.clone()).id(SharedString::from(
                        format!("session-row-text-{}", row.key),
                    ))),
            )
            .when(!row.detail.is_empty(), |line| {
                line.child(
                    div()
                        .flex_none()
                        .text_color(theme.ely.fg_muted)
                        .child(row.detail.clone()),
                )
            })
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
    async fn the_column_steps_aside_in_a_narrow_chat(cx: &mut TestAppContext) {
        let (chat, cx) = offline_chat_view(cx);
        cx.simulate_resize(size(px(600.0), px(700.0)));
        chat.update(cx, |chat, cx| chat.set_session_panel_open(true, cx));
        for _ in 0..3 {
            refresh_frame(cx);
        }
        assert!(chat.read_with(&cx.cx, |chat, _| chat.session_panel_open()));
        let snapshot = chat.read_with(&cx.cx, |chat, _| chat.control_snapshot());
        assert!(snapshot.session_panel.open);
        assert!(!snapshot.session_panel.drawn);
        assert!(cx.debug_bounds("session-panel").is_none());

        cx.simulate_resize(size(px(700.0), px(700.0)));
        for _ in 0..3 {
            refresh_frame(cx);
        }
        let snapshot = chat.read_with(&cx.cx, |chat, _| chat.control_snapshot());
        assert!(snapshot.session_panel.drawn, "wide again, the column returns");
        assert!(cx.debug_bounds("session-panel").is_some());
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
        cx.update(bezel::ui::input::init);
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
