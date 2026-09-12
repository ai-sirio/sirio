//! The sidebar's worktree card: one row of the flattened tree, its status
//! glyph, its hover controls and (from Task 4 on) its tab pills.

//! Split out of `sidebar.rs` so the row's drawing can grow without the
//! entity's state, popups and drag handling growing with it — the same
//! `mod.rs` + one-file-per-surface arrangement `right_panel/` uses.

use super::*;

/// A tab rendered inside its worktree row instead of as a separate tree row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SidebarPill {
    pub tab_id: Option<usize>,
    pub parked_tab: Option<usize>,
    pub title: String,
    pub icon: Icon,
    pub brand: Option<AgentBrandColor>,
    pub status: Option<ActivityStatus>,
    pub selected: bool,
}

/// What a worktree (or collapsed project) row draws in place of a status
/// word — one Bezel bloom, either travelling or stopped.
///
/// It used to be two glyphs in two places: a loader in a leading 12px column
/// and the status spelled out in words at the end of the title line. The
/// bloom carries both facts on its own — motion says whether work is in
/// flight, tint says which state it settled into — so the column is gone and
/// the words with it. `Idle` and the parked-agent count are the exception:
/// neither is a state a bloom can draw, so they stay text (see
/// `Sidebar::status_text`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum RowStatusGlyph {
    /// No glyph at all.
    None,
    /// A bloom still travelling: work is in flight.
    Running(Rgba),
    /// A bloom stopped at full, tinted by which state it stopped in: green
    /// done, amber needs-input, red error.
    Settled(Rgba),
}

impl RowStatusGlyph {
    pub(super) fn for_status(status: Option<ActivityStatus>, theme: Theme) -> Self {
        match status {
            None | Some(ActivityStatus::Idle) => Self::None,
            // Running shares `success` with done deliberately: green is the
            // colour of a worktree that is fine, and what separates "working"
            // from "finished" is that one of them moves. They do not collapse
            // onto each other under reduced motion either — see
            // `loading::settled_bloom_rings` and the test that pins it.
            Some(ActivityStatus::Running) => Self::Running(theme.success),
            Some(ActivityStatus::Done) => Self::Settled(theme.success),
            Some(ActivityStatus::NeedsInput) => Self::Settled(theme.warning),
            Some(ActivityStatus::Error) => Self::Settled(theme.danger),
        }
    }
}

/// What one row renders from — a copy the sidebar pushes in, compared before
/// it notifies, so an unchanged row stays a replayed subtree.
#[derive(Clone, PartialEq)]
pub(super) struct RowInputs {
    pub(super) row: SidebarRow,
    pub(super) index: usize,
    pub(super) cursor: bool,
    pub(super) pill_cursor: Option<usize>,
    pub(super) project_id: Option<String>,
    pub(super) project_icon: Option<ProjectIcon>,
    pub(super) drag: Option<RowDrag>,
}

/// One sidebar row as its own view. It owns nothing but its inputs; every
/// handler still targets the sidebar entity it holds, exactly as the row did
/// when the sidebar rendered it inline. Its render is where the running
/// spinner's lease lands, so a running worktree re-renders one row.
pub(super) struct RowView {
    pub(super) sidebar: gpui::Entity<Sidebar>,
    pub(super) inputs: RowInputs,
    /// How many times gpui asked this row to render. Test-observable only.
    pub(super) render_count: u64,
}

impl Render for RowView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let _perf = sirio_perf::span("RowView.render", self.inputs.row.id as u64);
        self.render_count = self.render_count.wrapping_add(1);
        let theme = *Theme::get(cx);
        let inputs = self.inputs.clone();
        Sidebar::render_row(
            inputs.row,
            inputs.index,
            inputs.cursor,
            inputs.pill_cursor,
            inputs.project_id,
            inputs.project_icon,
            inputs.drag,
            self.sidebar.clone(),
            theme,
            window,
            cx,
        )
        .into_any_element()
    }
}

impl Sidebar {
    /// The colour a card's title is drawn in.
    ///
    /// It used to have none. The element named a weight, a line height and
    /// an ellipsis and left the colour to be inherited — but nothing in the
    /// sidebar's ancestry sets one: not the row, not `sidebar-tree`, not the
    /// panel, not the window root, which establishes only `font_family`. The
    /// title therefore fell through to gpui's default `TextStyle`, which is
    /// black, and a branch name was drawn very nearly invisible on the dark
    /// surface behind it (measured at peak luma 15 against a 13 background,
    /// while the sub-line right under it read 115). Every other string in
    /// the row — the sub-line, the count, the status, the star — names its
    /// own token, which is exactly why the defect showed up as "only the
    /// branch names are black".
    ///
    /// A parked tab keeps the faint tone it always had: it is a record of
    /// what the worktree holds, not a live surface.
    pub(crate) fn title_color(parked: bool, theme: Theme) -> Rgba {
        if parked { theme.text_faint } else { theme.text }
    }

    /// The words at the end of the title line — now only the states no bloom
    /// can draw.
    ///
    /// Running, done, needs-input and error each have a bloom of their own
    /// (`RowStatusGlyph`), and writing the word next to it said the same
    /// thing twice in the row's narrowest space. What is left is the quiet
    /// half: a worktree sitting idle, and how many agents are parked in one.
    pub(crate) fn status_text(row: &SidebarRow) -> Option<String> {
        match row.agent_status {
            Some(ActivityStatus::Idle) | None if row.pills.len() > 1 => {
                Some(format!("{} agents", row.pills.len()))
            }
            Some(ActivityStatus::Idle) => Some("idle".to_owned()),
            _ => None,
        }
    }

    /// The card's second line.
    ///
    /// It used to be the checkout path, which is the one thing on the card
    /// the reader already knows: every worktree under a project repeats the
    /// same prefix, and the tail that tells them apart is the branch name
    /// already spelled on the line above — so the line cost a row of height
    /// to say nothing, and said it truncated. What earns that line is the
    /// work: the name of the worktree's most recent task. The host already
    /// names a tab from the first thing the user typed and then lets the
    /// summarizer rewrite it (`title_from_prompt` and `apply_auto_title` in
    /// `sirio`'s `main.rs`), and that name arrives here on the pill, so the
    /// card reads as "what I was doing in this worktree".
    ///
    /// A worktree comment (F-SID-11) still wins — a person put it there on
    /// purpose. With neither a comment nor a task the line is left empty
    /// rather than falling back to the path.
    pub(crate) fn sub_line_text(row: &SidebarRow) -> String {
        row.comment
            .as_deref()
            .filter(|comment| !comment.is_empty())
            .map(str::to_owned)
            .or_else(|| Self::last_task_title(row))
            .unwrap_or_default()
    }

    /// Which task the card names: the selected pill when the worktree has
    /// one, because that is the tab the user is actually looking at, and
    /// otherwise the last of the strip, which is where a newly opened tab
    /// lands. Parked pills count — a worktree nobody has mounted this
    /// session still remembers what was last open in it.
    fn last_task_title(row: &SidebarRow) -> Option<String> {
        row.pills
            .iter()
            .find(|pill| pill.selected)
            .or_else(|| row.pills.last())
            .map(|pill| pill.title.clone())
            .filter(|title| !title.trim().is_empty())
    }

    /// The minimum row rhythm: a single-line row is 32px (13.5px title at
    /// an 18px line height plus 7px of vertical padding — the action-row
    /// math); a card with a context line is 51px (7 + 18 + 4 + 15 + 7 —
    /// the session-card math). Content-sized titles grow beyond this floor.
    /// Whether a row draws a second line at all.
    ///
    /// The sub-line used to lead with the checkout path, which every project
    /// and worktree row had, so "is a card" and "has a path" were the same
    /// question. #151 dropped the path — it was almost always truncated,
    /// repeated the project prefix on every child, and bought its second
    /// line for every row in the tree. What remains on that line is the
    /// `Primary` pill and the F-SID-11 worktree comment, either of which may
    /// be absent, so both the sub-line and the taller height that pays for
    /// it now follow whether there is anything left to put there.
    ///
    /// The render and the height read this one predicate, so they cannot
    /// drift into disagreeing about whether a row has two lines.
    fn has_sub_line(row: &SidebarRow) -> bool {
        matches!(row.kind, RowKind::Worktree)
    }

    pub(super) fn row_min_height(row: &SidebarRow) -> f32 {
        if row.kind == RowKind::Worktree {
            CARD_TWO_LINE_HEIGHT
        } else {
            ROW_HEIGHT
        }
    }

    /// The title's line height at the persisted interface font size: the
    /// card rhythm above, shifted by the same delta the title's own size
    /// takes, so a larger interface font gets a taller line instead of a
    /// clipped one.
    pub(super) fn title_line_height(typography: &Typography) -> Pixels {
        typography.scaled(ROW_TITLE_LINE_HEIGHT)
    }

    /// The height a row is actually drawn at: the kind's rhythm plus
    /// whatever its title line grew by. `row_min_height` stays the rhythm at
    /// the default interface size, which is what the conformance inventory
    /// pins; only the drawn height follows the setting.
    pub(super) fn row_drawn_height(row: &SidebarRow, typography: &Typography) -> f32 {
        Self::row_min_height(row) + f32::from(Self::title_line_height(typography))
            - ROW_TITLE_LINE_HEIGHT
    }

    /// The structural row handed to bezel. Sirio keeps the data and content;
    /// bezel owns branch/leaf identity, indentation, disclosure and chrome.
    /// The bezel tree shape of one row. Projects retain their disclosure;
    /// worktrees are leaves because their tabs are pills inside the row.
    pub(super) fn tree_row(row: &SidebarRow) -> tree::Row {
        match row.kind {
            RowKind::Project => tree::Row::branch(0, row.expanded),
            RowKind::Worktree => tree::Row::leaf(0),
        }
    }

    /// Apply one of bezel's standard tree directions to the currently
    /// visible, depth-annotated rows. Expansion remains Sirio state; bezel
    /// reports only the intent.
    /// The tint of a pill's glyph.
    ///
    /// A branded agent mark is drawn in its brand. A pill with no agent — an
    /// unstarted chat, a plain terminal — takes the row grey. It must never
    /// fall back to `warning`: that amber means "answer me", and spending it
    /// as decoration made an idle terminal pixel-identical to an agent
    /// genuinely waiting on the reader. That bug survived one fix already,
    /// which is why the choice lives here instead of inline in the pill,
    /// where no test could reach it.
    pub(super) fn pill_icon_color(brand: Option<AgentBrandColor>, theme: Theme) -> Rgba {
        brand.map_or(theme.text_muted, AgentBrandColor::color)
    }

    /// Stable semantic debug/test names, independent of vendored filenames.
    pub(super) fn icon_selector_name(icon: Icon) -> &'static str {
        match icon {
            Icon::FolderFill => "folder",
            Icon::GitBranch => "git-branch",
            Icon::MessageSquare => "chat-round-line",
            Icon::SquareTerminal => "terminal",
            Icon::Close => "close",
            Icon::ChevronDown => "alt-arrow-down",
            Icon::ChevronUp => "alt-arrow-up",
            Icon::ChevronRight => "alt-arrow-right",
            Icon::ChevronLeft => "alt-arrow-left",
            Icon::Settings => "settings-minimalistic",
            Icon::RefreshCw => "refresh",
            Icon::Plus => "plus",
            Icon::File => "document",
            Icon::Sparkles => "sparkle-thin",
            Icon::Shield => "shield-thin",
            Icon::SunMoon => "sun-dim-thin",
            Icon::Globe => "global",
            Icon::ClaudeCode => "claude-mark",
            Icon::Codex => "openai-mark",
            Icon::OpenCode => "agent-opencode",
            Icon::Pi => "pi-mark",
            Icon::OhMyPi => "agent-omp",
            Icon::SidebarLeft => "sidebar-minimalistic-left",
            Icon::PanelRight => "sidebar-minimalistic",
            Icon::Archive => "archive-minimalistic",
            Icon::Lock => "key-minimalistic",
            Icon::FileTree => "file-tree",
            Icon::Thread => "thread",
            Icon::Diff => "diff",
            Icon::DiffUnified => "diff-unified",
            Icon::DiffSplit => "diff-split",
            Icon::ExpandVertical => "expand-vertical",
            Icon::FoldVertical => "fold-vertical",
            Icon::SquarePlus => "square-plus",
            Icon::SquareMinus => "square-minus",
            Icon::Undo => "undo",
            Icon::GitGraph => "git-graph",
            Icon::FileType(_) => "file-type",
        }
    }

    pub(super) fn row_icon(row: &SidebarRow) -> Icon {
        match row.kind {
            RowKind::Project => Icon::FolderFill,
            // A worktree row's mark says what the row *is*, not what is
            // running in it: `App/SidebarView.swift:361` draws
            // `arrow.triangle.branch` beside the branch name unconditionally
            // and never puts an agent mark there. The agent reaches this row
            // as the tint of the status indicator and as the trailing badge
            // — see `set_worktree_activity`.
            RowKind::Worktree => Icon::GitBranch,
        }
    }

    const PILL_SIZE: f32 = 20.0;

    fn render_pills(
        row: &SidebarRow,
        pill_cursor: Option<usize>,
        entity: gpui::Entity<Sidebar>,
        theme: Theme,
    ) -> impl IntoElement {
        let row_id = row.id;
        let worktree_path = row.path.clone();
        div()
            .flex()
            .flex_none()
            .ml_auto()
            .items_center()
            .gap(px(4.0))
            .children(row.pills.iter().enumerate().map(|(index, pill)| {
                let select_entity = entity.clone();
                let close_entity = entity.clone();
                let pill_group = format!("sidebar-pill-group-{row_id}-{index}");
                let tab_id = pill.tab_id;
                let parked = pill.parked_tab;
                let path = worktree_path.clone();
                div()
                    .id(("sidebar-pill", row_id * 32 + index))
                    .debug_selector(move || format!("sidebar-pill-{row_id}-{index}"))
                    .group(pill_group.clone())
                    .relative()
                    .w(px(Self::PILL_SIZE))
                    .h(px(Self::PILL_SIZE))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(theme.radii.chip)
                    .bg(theme.surface_raised)
                    .when(pill.selected || pill_cursor == Some(index), |this| {
                        this.border_1().border_color(theme.accent)
                    })
                    .when(parked.is_some(), |this| this.opacity(0.6))
                    .hover(|style| style.bg(theme.element_hover))
                    .child(
                        div()
                            .group_hover(pill_group.clone(), |style| style.invisible())
                            .debug_selector(move || {
                                format!(
                                    "sidebar-pill-mark-{row_id}-{index}-{}",
                                    Self::icon_selector_name(pill.icon)
                                )
                            })
                            .child(
                                IconElement::new(pill.icon, IconSize::XSmall)
                                    .text_color(Self::pill_icon_color(pill.brand, theme)),
                            ),
                    )
                    .when_some(pill.status, |this, status| {
                        this.when(status != ActivityStatus::Idle, |this| {
                            this.child(
                                div()
                                    .absolute()
                                    .top(px(-2.0))
                                    .right(px(-2.0))
                                    .w(px(6.0))
                                    .h(px(6.0))
                                    .rounded_full()
                                    .bg(crate::right_panel::status_color(status, theme)),
                            )
                        })
                    })
                    .when_some(tab_id, |this, tab_id| {
                        this.child(
                            div()
                                .id(("sidebar-pill-close", row_id * 32 + index))
                                .debug_selector(move || {
                                    format!("sidebar-pill-close-{row_id}-{index}")
                                })
                                .absolute()
                                .inset_0()
                                .flex()
                                .items_center()
                                .justify_center()
                                .invisible()
                                .group_hover(pill_group.clone(), |style| style.visible())
                                .child(IconElement::new(Icon::Close, IconSize::XSmall))
                                .on_click(move |_, _, cx| {
                                    cx.stop_propagation();
                                    close_entity.update(cx, |_, cx| {
                                        cx.emit(SidebarEvent::CloseTab(tab_id));
                                    });
                                }),
                        )
                    })
                    .on_click(move |_, _, cx| {
                        cx.stop_propagation();
                        select_entity.update(cx, |_, cx| match (tab_id, parked, path.clone()) {
                            (Some(id), _, _) => cx.emit(SidebarEvent::SelectTab(id)),
                            (None, Some(index), Some(path)) => {
                                cx.emit(SidebarEvent::SelectParkedTab { path, index })
                            }
                            _ => {}
                        });
                    })
            }))
            .when(row.selected, |this| {
                let add_entity = entity.clone();
                this.child(
                    div()
                        .id(("sidebar-pill-add", row_id))
                        .debug_selector(move || format!("sidebar-pill-add-{row_id}"))
                        .w(px(Self::PILL_SIZE))
                        .h(px(Self::PILL_SIZE))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(theme.radii.chip)
                        .border_1()
                        .border_color(theme.border)
                        .text_color(theme.text_faint)
                        .hover(|style| style.bg(theme.element_hover))
                        .child("+")
                        .on_click(move |event, window, cx| {
                            cx.stop_propagation();
                            add_entity.update(cx, |sidebar, cx| {
                                sidebar.open_context_menu(row_id, event.position(), window, cx);
                            });
                        }),
                )
            })
    }

    fn render_row(
        row: SidebarRow,
        row_index: usize,
        cursor: bool,
        pill_cursor: Option<usize>,
        project_id: Option<String>,
        project_icon: Option<ProjectIcon>,
        drag: Option<RowDrag>,
        entity: gpui::Entity<Self>,
        theme: Theme,
        window: &mut Window,
        cx: &mut App,
    ) -> impl IntoElement {
        let row_id = row.id;
        let selected = row.selected;
        let kind = row.kind;
        let title = row.title.clone();
        let path = row.path.clone();
        // The click closure reports the checkout path for worktree rows.
        let worktree_path = path.clone();
        let is_project = kind == RowKind::Project;
        let is_worktree = kind == RowKind::Worktree;
        // #372: the primary checkout offers no hover-x — like its disabled
        // context-menu item, it cannot be `git worktree remove`d.
        let is_removable_worktree = is_worktree && !row.is_primary;
        // waku's card rhythm: a project or worktree row becomes a two-line
        // card (13.5px title over an 11.5px context line) only when it has
        // something for that second line; every other row is single-line at
        // the 32px action-row height. See `has_sub_line`.
        let has_sub_line = Self::has_sub_line(&row);
        let row_height = Self::row_drawn_height(&row, &theme.typography);
        // F-CORE-ACT-18: the trailing running-agents badge is one 12px mark
        // per distinct running agent, 3px apart, 7px clear of the title. It
        // takes its width out of the title's, so a busy worktree truncates
        // its branch name instead of pushing the hover controls off the row.
        // A glyph only appears for a notable status — matching the
        // reference. A collapsed project also gets one (F-SID-06): its
        // worktree rows are hidden, so `row.agent_status` was pre-aggregated
        // onto the project row itself in `visible_rows`.
        let status_glyph =
            if kind == RowKind::Worktree || (kind == RowKind::Project && !row.expanded) {
                RowStatusGlyph::for_status(row.agent_status, theme)
            } else {
                RowStatusGlyph::None
            };
        let glyph = project_icon
            .as_ref()
            .and_then(|icon| match &icon.value {
                ProjectIconValue::Symbol(glyph) => Some(glyph.icon()),
                ProjectIconValue::Avatar(_) => Some(Icon::Globe),
                ProjectIconValue::Emoji(_) => None,
            })
            .unwrap_or_else(|| Self::row_icon(&row));
        let glyph_color = match kind {
            RowKind::Project => project_icon
                .as_ref()
                .map(|icon| icon.tint.resolve(theme))
                .unwrap_or_else(|| Self::project_color(&title)),
            RowKind::Worktree => theme.text_faint,
        };
        let entity = entity.clone();
        let remove_entity = entity.clone();
        let click_entity = entity.clone();
        let tab_close_entity = entity.clone();
        let context_entity = entity.clone();
        let hover_group = format!("sidebar-project-{row_id}");
        let tab_id = row.tab_id;
        let parked_tab = row.parked_tab;
        let mark_size = theme.typography.headline;
        let icon_size = IconSize::Small;
        let project_mark = match project_icon.as_ref().map(|icon| &icon.value) {
            Some(ProjectIconValue::Emoji(emoji)) => div()
                .text_size(px(15.0))
                .child(emoji.clone())
                .into_any_element(),
            // A locally chosen PNG is real file content already on disk — no
            // network fetch needed, so it can render as an actual image
            // instead of the generic globe glyph every other avatar source
            // still falls back to (F-PRJ-14: GitHub/Favicon need an HTTP
            // client this app doesn't have yet; see project_identity.rs).
            Some(ProjectIconValue::Avatar(AvatarSource::LocalPng(path))) => img(path.clone())
                .w(mark_size)
                .h(mark_size)
                .rounded(theme.radii.control)
                .into_any_element(),
            _ => IconElement::new(glyph, icon_size)
                .text_color(glyph_color)
                .into_any_element(),
        };

        let row_debug_selector = format!("sidebar-row-{row_id}");
        let mut row_view = div()
            .id(("sidebar-row", row_id))
            .debug_selector(move || row_debug_selector)
            .group(hover_group.clone())
            .relative()
            .h(px(row_height))
            .w_full()
            .px(px(12.0))
            .py(px(7.0))
            .flex()
            .gap(px(8.0))
            .when(selected, |this| this.bg(theme.element_active))
            .when(!selected, |this| {
                this.hover(|style| style.bg(theme.element_hover))
            })
            .on_click(move |_, window, cx| {
                click_entity.update(cx, |sidebar, cx| {
                    sidebar.focus_tree_row(row_index, window, cx);
                    if let Some(tab_id) = tab_id {
                        // A host-driven row: the host owns which tab is
                        // selected, so report the click rather than
                        // flipping `selected` locally.
                        cx.emit(SidebarEvent::SelectTab(tab_id));
                        return;
                    }
                    if let Some(index) = parked_tab
                        && let Some(path) = worktree_path.as_ref()
                    {
                        // A parked tab has no live id: ask the host to bring
                        // its worktree back with this strip position active.
                        cx.emit(SidebarEvent::SelectParkedTab {
                            path: path.clone(),
                            index,
                        });
                        return;
                    }
                    match kind {
                        RowKind::Project => sidebar.toggle_project(row_id, cx),
                        RowKind::Worktree => {
                            // Report the click to the host; the host decides
                            // what actually becomes selected and confirms by
                            // calling back `set_selected_worktree`.
                            if let Some(path) = worktree_path.as_ref() {
                                cx.emit(SidebarEvent::SelectWorktree(path.clone()));
                                sidebar.select_row(row_id, cx);
                            }
                        }
                    }
                });
            });

        row_view = row_view.on_mouse_down(
            MouseButton::Right,
            move |event: &MouseDownEvent, window, cx| {
                cx.stop_propagation();
                context_entity.update(cx, |sidebar, cx| {
                    sidebar.open_context_menu(row_id, event.position, window, cx)
                });
            },
        );

        if let Some(drag) = drag {
            let drag_entity = entity.clone();
            let move_entity = entity.clone();
            let drop_entity = entity.clone();
            row_view = row_view
                .on_drag(drag, move |_, _, _, cx| {
                    drag_entity.update(cx, |sidebar, _| sidebar.pending_reorder = None);
                    cx.new(|_| gpui::Empty)
                })
                .on_drag_move::<RowDrag>(move |event: &DragMoveEvent<RowDrag>, _, cx| {
                    let drag = *event.drag(cx);
                    let before = event.event.position.y < event.bounds.center().y;
                    move_entity.update(cx, |sidebar, cx| {
                        sidebar.preview_reorder(drag, row_id, before, cx);
                    });
                })
                .on_drop::<RowDrag>(move |_, _, cx| {
                    drop_entity.update(cx, |sidebar, cx| sidebar.confirm_reorder(cx));
                });
        }

        // The bloom that stands where the status word used to. Built here
        // rather than inline in `title_line` because it is the one part of
        // the row that needs `window` and `cx`: a travelling bloom takes a
        // lease on Bezel's shared clock, and that lease is what re-renders
        // this row — and only this row — on every spinner frame.
        let status_bloom = match status_glyph {
            RowStatusGlyph::Running(tint) => Some(
                div()
                    .id(("sidebar-status-running", row_id))
                    .debug_selector(move || format!("sidebar-status-running-{row_id}"))
                    .flex_none()
                    .child(loading::bloom(
                        "sidebar-running-bloom",
                        loading::BLOOM_GLYPH,
                        tint,
                        &theme,
                        window,
                        cx,
                    ))
                    .into_any_element(),
            ),
            RowStatusGlyph::Settled(tint) => Some(
                div()
                    .id(("sidebar-status-settled", row_id))
                    .debug_selector(move || format!("sidebar-status-settled-{row_id}"))
                    .flex_none()
                    .child(loading::settled_bloom(
                        "sidebar-settled-bloom",
                        loading::BLOOM_GLYPH,
                        tint,
                    ))
                    .into_any_element(),
            ),
            RowStatusGlyph::None => None,
        };
        let title_element = div()
            .min_w_0()
            .flex_1()
            .whitespace_nowrap()
            .overflow_hidden()
            .text_ellipsis()
            .debug_selector(move || format!("sidebar-row-title-{row_id}"))
            // Named, not inherited. The element used to leave both the size
            // and the colour to its ancestry, and nothing in the sidebar's
            // ancestry sets either -- the window root establishes only
            // `font_family` -- so the branch name fell through to gpui's
            // default `TextStyle`: a fixed 16px that Settings -> Appearance
            // -> Interface font size could not move. See `title_color` for
            // the same defect, and the same fix, on the colour.
            .text_size(theme.typography.scaled(ROW_TITLE_FONT_SIZE))
            .line_height(Self::title_line_height(&theme.typography))
            .font_weight(if is_project {
                FontWeight::SEMIBOLD
            } else {
                FontWeight::NORMAL
            })
            .text_color(Self::title_color(parked_tab.is_some(), theme))
            .child(title);
        let title_line = div()
            .w_full()
            .flex()
            .items_center()
            .gap(px(6.0))
            .child({
                let slot = div().w(px(16.0)).flex().items_center().justify_center();
                if is_worktree {
                    let name = Self::icon_selector_name(glyph);
                    slot.id(("sidebar-worktree-mark", row_id))
                        .debug_selector(move || format!("sidebar-worktree-mark-{row_id}-{name}"))
                        .child(project_mark)
                        .into_any_element()
                } else {
                    slot.child(project_mark).into_any_element()
                }
            })
            .child(title_element)
            .when(row.is_primary, |this| {
                this.child(
                    div()
                        .debug_selector(move || format!("sidebar-primary-star-{row_id}"))
                        .flex_none()
                        .text_size(px(10.0))
                        .text_color(theme.text_faint)
                        .child("★"),
                )
            })
            .when_some(Self::status_text(&row), |this, status| {
                this.child(
                    div()
                        .debug_selector(move || format!("sidebar-row-status-{row_id}"))
                        .flex_none()
                        .text_size(theme.typography.scaled(11.0))
                        .text_color(theme.text_faint)
                        .child(status),
                )
            });
        let sub_line = div()
            .w_full()
            .flex()
            .items_center()
            .gap(px(6.0))
            .line_height(px(ROW_SUB_LINE_HEIGHT))
            .text_size(px(12.5))
            .text_color(theme.text_faint)
            .child(
                div()
                    .debug_selector(move || format!("sidebar-row-subline-{row_id}"))
                    .min_w_0()
                    .flex_1()
                    .whitespace_nowrap()
                    .overflow_hidden()
                    .text_ellipsis()
                    .child(Self::sub_line_text(&row)),
            )
            .child(Self::render_pills(
                &row,
                cursor.then_some(pill_cursor).flatten(),
                entity.clone(),
                theme,
            ));
        let title_line = title_line
            .when(is_project, |this| {
                this.child(
                    div()
                        .id(("project-settings", row_id))
                        .debug_selector(move || format!("project-settings-{row_id}"))
                        .cursor(gpui::CursorStyle::PointingHand)
                        .w(px(16.0))
                        .flex_none()
                        .text_size(px(13.0))
                        .text_color(theme.text_faint)
                        .invisible()
                        .group_hover(hover_group.clone(), |style| style.visible())
                        .child(
                            IconElement::new(Icon::Settings, IconSize::XSmall)
                                .text_color(theme.text),
                        )
                        .on_click(move |_, _window, cx| {
                            if let Some(project_id) = project_id.clone() {
                                remove_entity.update(cx, |_, cx| {
                                    cx.emit(SidebarEvent::OpenProjectSettings(project_id));
                                });
                            }
                        }),
                )
            })
            .when(is_removable_worktree, |this| {
                let remove_entity = entity.clone();
                this.child(
                    div()
                        .id(("remove-worktree", row_id))
                        .debug_selector(move || format!("remove-worktree-{row_id}"))
                        .w(px(16.0))
                        .flex_none()
                        .text_size(px(12.0))
                        .text_color(theme.text_faint)
                        .rounded(theme.radii.chip)
                        .hover(|style| style.bg(theme.element_hover))
                        .invisible()
                        .group_hover(hover_group.clone(), |style| style.visible())
                        .on_click(move |event, window, cx| {
                            cx.stop_propagation();
                            remove_entity.update(cx, |sidebar, cx| {
                                sidebar.open_worktree_close_menu(
                                    row_id,
                                    event.position(),
                                    window,
                                    cx,
                                );
                            });
                        })
                        .child(
                            IconElement::new(Icon::Close, IconSize::XSmall)
                                .text_color(theme.text_faint),
                        ),
                )
            })
            .when_some(tab_id, |this, tab_id| {
                this.child(
                    div()
                        .id(("sidebar-tab-close", row_id))
                        .debug_selector(move || format!("sidebar-tab-close-{row_id}"))
                        .w(px(16.0))
                        .flex_none()
                        .text_size(px(14.0))
                        .text_color(theme.text_muted)
                        .rounded(theme.radii.chip)
                        .hover(|style| style.bg(theme.element_hover))
                        .invisible()
                        .group_hover(hover_group.clone(), |style| style.visible())
                        .on_click(move |_, _, cx| {
                            cx.stop_propagation();
                            tab_close_entity.update(cx, |_, cx| {
                                cx.emit(SidebarEvent::CloseTab(tab_id));
                            });
                        })
                        .child(
                            IconElement::new(Icon::Close, IconSize::XSmall).text_color(theme.text),
                        ),
                )
            })
            // Last, after every hover control on the line. Those controls are
            // `.invisible()` rather than unmounted, so they hold their 16px
            // whether or not the pointer is on the row -- but only some rows
            // have them (a primary checkout cannot be removed, a worktree has
            // no project settings). Anything placed before them therefore
            // sits at a different x per row, which is exactly the drift
            // `running_agent_badges_share_one_trailing_edge_across_worktrees`
            // exists to catch. Last is the only seat where the bloom lands on
            // one trailing edge for every row.
            .when_some(status_bloom, |this, bloom| this.child(bloom));
        let content = div()
            .min_w_0()
            .flex_1()
            .flex()
            .flex_col()
            .justify_center()
            .gap(px(ROW_GAP))
            .child(title_line)
            .when(has_sub_line, |this| this.child(sub_line));

        row_view.child(content)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sidebar::tests_support;
    use gpui::{Modifiers, TestAppContext, VisualTestContext};
    use std::path::PathBuf;

    fn worktree(id: usize, title: &str) -> SidebarRow {
        SidebarRow {
            id,
            kind: RowKind::Worktree,
            depth: 1,
            title: title.to_owned(),
            selected: false,
            expanded: true,
            is_primary: false,
            agent_status: None,
            is_git: true,
            path: None,
            tab_id: None,
            parked_tab: None,
            tab_kind: None,
            agent_icon: None,
            agent_brand: None,
            comment: None,
            pills: Vec::new(),
        }
    }

    fn pill(tab_id: usize) -> SidebarPill {
        SidebarPill {
            tab_id: Some(tab_id),
            parked_tab: None,
            title: "Claude".to_owned(),
            icon: Icon::MessageSquare,
            brand: None,
            status: Some(ActivityStatus::Idle),
            selected: false,
        }
    }

    /// The four notable statuses are drawn as a bloom now, not spelled out,
    /// so the status line is left to the two things a bloom cannot say: that
    /// a worktree is idle, and how many agents are parked in it.
    #[test]
    fn a_notable_status_is_a_bloom_and_the_words_are_left_to_the_quiet_states() {
        let mut row = worktree(3, "main");
        for status in [
            ActivityStatus::Running,
            ActivityStatus::NeedsInput,
            ActivityStatus::Done,
            ActivityStatus::Error,
        ] {
            row.agent_status = Some(status);
            assert_eq!(
                Sidebar::status_text(&row),
                None,
                "{status:?} is drawn, not written"
            );
        }
        row.agent_status = Some(ActivityStatus::Idle);
        row.pills = vec![pill(1), pill(2)];
        assert_eq!(Sidebar::status_text(&row).as_deref(), Some("2 agents"));
        row.pills = vec![pill(1)];
        assert_eq!(Sidebar::status_text(&row).as_deref(), Some("idle"));
    }

    /// A running worktree with several agents parked in it is still running:
    /// the bloom is the message, and the count would only compete with it.
    #[test]
    fn a_running_worktree_does_not_fall_back_to_counting_its_agents() {
        let mut row = worktree(3, "main");
        row.agent_status = Some(ActivityStatus::Running);
        row.pills = vec![pill(1), pill(2)];
        assert_eq!(Sidebar::status_text(&row), None);
    }

    /// The second line names the worktree's most recent task, not its
    /// checkout path. The path was the least informative thing the card
    /// could carry — every sibling repeats the project prefix and the part
    /// that differs is the branch name already on the line above — and it
    /// pushed the one fact worth reading, what the user was doing here, off
    /// the card entirely. A comment still wins, and with neither the line
    /// is empty rather than falling back to the path.
    #[test]
    fn the_second_line_names_the_last_task_and_never_the_path() {
        let mut row = worktree(4, "feat/x");
        row.path = Some(PathBuf::from("/tmp/projects/sirio-wt/feat-x"));
        assert_eq!(
            Sidebar::sub_line_text(&row),
            "",
            "a worktree with no task and no comment leaves the line empty"
        );

        let mut first = pill(1);
        first.title = "wire up the pill row".to_owned();
        let mut second = pill(2);
        second.title = "chase the ConPTY title".to_owned();
        row.pills = vec![first, second];
        assert_eq!(
            Sidebar::sub_line_text(&row),
            "chase the ConPTY title",
            "with nothing selected the newest tab of the strip names the card"
        );

        row.pills[0].selected = true;
        assert_eq!(
            Sidebar::sub_line_text(&row),
            "wire up the pill row",
            "the tab the user is actually on wins over strip order"
        );

        row.comment = Some("redesign the sidebar".to_owned());
        assert_eq!(
            Sidebar::sub_line_text(&row),
            "redesign the sidebar",
            "a comment a person left on purpose still outranks a derived name"
        );
    }

    /// A parked tab is the case that made the old path fallback look
    /// harmless: an unmounted worktree has no live tabs at all. It does
    /// still carry its persisted strip, so the card can name what was last
    /// open in it without mounting anything.
    #[test]
    fn a_parked_strip_still_names_the_card() {
        let mut row = worktree(5, "feat/y");
        row.pills = vec![SidebarPill {
            tab_id: None,
            parked_tab: Some(0),
            title: "review the release notes".to_owned(),
            icon: Icon::MessageSquare,
            brand: None,
            status: None,
            selected: false,
        }];
        assert_eq!(
            Sidebar::sub_line_text(&row),
            "review the release notes"
        );
    }

    #[gpui::test]
    async fn clicking_a_pill_selects_its_tab(cx: &mut TestAppContext) {
        cx.update(Theme::init);
        let window = cx.add_window(|_window, cx| tests_support::sidebar_with_one_project(cx));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        let sidebar =
            cx.update(|window, _| window.root::<Sidebar>().flatten().expect("sidebar root"));
        let events = tests_support::collect_events(&sidebar, &mut cx);
        let pill = cx
            .debug_bounds("sidebar-pill-1-0")
            .expect("the first pill is rendered");
        cx.simulate_click(pill.center(), Modifiers::none());
        cx.run_until_parked();

        assert!(matches!(
            events.borrow().last(),
            Some(SidebarEvent::SelectTab(1))
        ));
    }

    #[gpui::test]
    async fn the_pill_close_reports_close_tab(cx: &mut TestAppContext) {
        cx.update(Theme::init);
        let window = cx.add_window(|_window, cx| tests_support::sidebar_with_one_project(cx));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        let sidebar =
            cx.update(|window, _| window.root::<Sidebar>().flatten().expect("sidebar root"));
        let events = tests_support::collect_events(&sidebar, &mut cx);
        let close = cx
            .debug_bounds("sidebar-pill-close-1-0")
            .expect("the pill close control is rendered");
        cx.simulate_mouse_move(close.center(), None, Modifiers::none());
        cx.run_until_parked();
        cx.simulate_click(close.center(), Modifiers::none());
        cx.run_until_parked();

        assert!(matches!(
            events.borrow().last(),
            Some(SidebarEvent::CloseTab(1))
        ));
    }

    #[gpui::test]
    async fn a_parked_pill_restores_instead_of_selecting(cx: &mut TestAppContext) {
        cx.update(Theme::init);
        let window = cx.add_window(|_window, cx| tests_support::sidebar_with_parked_tab(cx));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        let sidebar =
            cx.update(|window, _| window.root::<Sidebar>().flatten().expect("sidebar root"));
        let events = tests_support::collect_events(&sidebar, &mut cx);
        let pill = cx
            .debug_bounds("sidebar-pill-1-0")
            .expect("the parked pill is rendered");
        cx.simulate_click(pill.center(), Modifiers::none());
        cx.run_until_parked();

        assert!(matches!(
            events.borrow().last(),
            Some(SidebarEvent::SelectParkedTab { .. })
        ));
    }
}
