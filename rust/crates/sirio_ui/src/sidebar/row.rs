//! The sidebar's worktree card: one row of the flattened tree, its status
//! glyph, its hover controls and (from Task 4 on) its tab pills.

//! Split out of `sidebar.rs` so the row's drawing can grow without the
//! entity's state, popups and drag handling growing with it — the same
//! `mod.rs` + one-file-per-surface arrangement `right_panel/` uses.

use super::*;
use gpui::Hsla;

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
    /// The tab's kind — the Sessions view keeps agent chats, and terminals
    /// only once an agent has been identified in them.
    pub kind: TabKind,
    pub persistence_id: String,
    pub last_event_at: Option<i64>,
    /// The tab's session ended a turn out of sight and has not been looked at
    /// since. Drawn on the pill (ring and dot) and, aggregated, on its worktree
    /// row and a folded project header.
    pub unread: bool,
}

/// What a worktree (or collapsed project) row draws in place of a status
/// word — one Bezel bloom, either travelling or stopped.
///
/// It used to be two glyphs in two places: a loader in a leading 12px column
/// and the status spelled out in words at the end of the title line. The
/// bloom carries both facts on its own — motion says whether work is in
/// flight, tint says which state it settled into — so the column is gone and
/// the words with it. An idle worktree and the parked-agent count used to stay
/// text; the pills on the line now show each agent, so those words went too.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RowStatusGlyph {
    /// No glyph at all.
    None,
    /// A bloom still travelling: work is in flight.
    Running(Hsla),
    /// A bloom stopped at full, tinted by which state it stopped in: green
    /// done, amber needs-input, red error.
    Settled(Hsla),
}

impl RowStatusGlyph {
    pub fn for_status(status: Option<ActivityStatus>, theme: Theme) -> Self {
        match status {
            None | Some(ActivityStatus::Idle) => Self::None,
            // Running shares `success` with done deliberately: green is the
            // colour of a worktree that is fine, and what separates "working"
            // from "finished" is that one of them moves. They do not collapse
            // onto each other under reduced motion either — see
            // `loading::settled_bloom_rings` and the test that pins it.
            Some(ActivityStatus::Running) => Self::Running(theme.ely.success),
            Some(ActivityStatus::Done) => Self::Settled(theme.ely.success),
            Some(ActivityStatus::NeedsInput) => Self::Settled(theme.ely.warning),
            Some(ActivityStatus::Error) => Self::Settled(theme.ely.danger),
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
    /// Whether this is the first worktree card, the one the debug layout read
    /// measures. An input so a row that becomes the first re-renders, even
    /// when it is mounted cached, and paints its probe.
    pub(super) first_card: bool,
    /// Whether the card may be removed: neither the Primary nor git's main
    /// checkout. Drawn as a cross under the pointer, never otherwise.
    pub(super) removable: bool,
    /// Whether this is the first removable card, whose cross the debug layout
    /// read measures.
    pub(super) first_removable: bool,
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
        let theme = Theme::get(cx).with_sidebar_typography();
        let inputs = self.inputs.clone();
        // "(missing)" is measured so the name's slot can keep it inside, whatever the width.
        let missing_width = if inputs.row.missing {
            let text = "(missing)";
            window
                .text_system()
                .layout_line(
                    text,
                    theme.typography.scaled(ROW_TITLE_FONT_SIZE),
                    &[gpui::TextRun {
                        len: text.len(),
                        font: gpui::font(theme.typography.ui_family),
                        color: theme.ely.fg_subtle,
                        ..Default::default()
                    }],
                    None,
                )
                .width
        } else {
            px(0.0)
        };
        Sidebar::render_row(
            inputs.row,
            inputs.index,
            inputs.cursor,
            inputs.pill_cursor,
            inputs.project_id,
            inputs.project_icon,
            inputs.drag,
            inputs.first_card,
            inputs.removable,
            inputs.first_removable,
            missing_width,
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
    /// the row — "(missing)", the pills, the star — names its own token,
    /// which is exactly why the defect showed up as "only the branch names
    /// are black".
    ///
    /// A parked tab keeps the faint tone it always had: it is a record of
    /// what the worktree holds, not a live surface.
    pub(crate) fn title_color(parked: bool, theme: Theme) -> Hsla {
        if parked { theme.ely.fg_subtle } else { theme.ely.fg }
    }

    /// What a worktree card is called, and whether that is the fallback. A
    /// plain folder has no branch (its one synthesized row carries an empty
    /// one), so its card names the folder instead of drawing nothing.
    pub(crate) fn card_name(row: &SidebarRow) -> (String, bool) {
        if !row.title.trim().is_empty() {
            return (row.title.clone(), false);
        }
        let name = row.path.as_ref().map_or_else(String::new, |path| {
            path.file_name().map_or_else(
                || path.to_string_lossy().into_owned(),
                |name| name.to_string_lossy().into_owned(),
            )
        });
        (name, true)
    }

    /// The card's name as one string, "(missing)" included, for the places
    /// that read it as text (the Sessions view, the bell and the filter).
    pub(crate) fn branch_text(row: &SidebarRow) -> String {
        let mut text = Self::card_name(row).0;
        if row.missing {
            text.push_str(" (missing)");
        }
        text
    }

    /// The minimum row rhythm: a worktree card is one 31px line (name, pills
    /// and the trailing seat), a single-line row is 34px (an 18px title line
    /// with 8px of padding above and 8px below). The drawn height adds only
    /// the interface-font delta (`row_drawn_height`); titles never wrap.
    pub(super) fn row_min_height(row: &SidebarRow) -> f32 {
        if row.kind == RowKind::Worktree {
            WORKTREE_CARD_HEIGHT
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

    /// The tint of a pill's glyph.
    ///
    /// A branded agent mark is drawn in its brand. A pill with no agent — an
    /// unstarted chat, a plain terminal — takes the row grey. It must never
    /// fall back to `warning`: that amber means "answer me", and spending it
    /// as decoration made an idle terminal pixel-identical to an agent
    /// genuinely waiting on the reader. That bug survived one fix already,
    /// which is why the choice lives here instead of inline in the pill,
    /// where no test could reach it.
    pub(super) fn pill_icon_color(brand: Option<AgentBrandColor>, theme: Theme) -> Hsla {
        brand.map_or(theme.ely.fg_muted, |brand| AgentBrandColor::color(brand).into())
    }

    /// Stable semantic debug/test names, independent of vendored filenames.
    pub(super) fn icon_selector_name(icon: Icon) -> &'static str {
        match icon {
            Icon::FolderFill => "folder",
            Icon::FolderOpen => "folder-open",
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
            Icon::Browser => "screen",
            Icon::Globe => "global",
            Icon::ClaudeCode => "claude-mark",
            Icon::ClaudeAccount => "claude-account-mark",
            Icon::Codex => "openai-mark",
            Icon::OpenCode => "agent-opencode",
            Icon::Pi => "pi-mark",
            Icon::Gemini => "gemini-mark",
            Icon::Grok => "grok-mark",
            Icon::OhMyPi => "agent-omp",
            Icon::SidebarLeft => "sidebar-minimalistic-left",
            Icon::PanelRight => "sidebar-minimalistic",
            Icon::PanelLeftClose => "panel-left-close",
            Icon::PanelLeftOpen => "panel-left-open",
            Icon::Archive => "archive-minimalistic",
            Icon::Lock => "key-minimalistic",
            Icon::StarFilled => "star-filled",
            Icon::FileTree => "file-tree",
            Icon::Diff => "diff",
            Icon::DiffUnified => "diff-unified",
            Icon::DiffSplit => "diff-split",
            Icon::ExpandVertical => "expand-vertical",
            Icon::FoldVertical => "fold-vertical",
            Icon::SquarePlus => "square-plus",
            Icon::SquareMinus => "square-minus",
            Icon::Undo => "undo",
            Icon::GitGraph => "git-graph",
            Icon::MagnifyingGlass => "magnifying-glass",
            Icon::Eye => "eye",
            Icon::EyeOff => "eye-off",
            Icon::Code => "code",
            Icon::PullRequest => "pull-request",
            Icon::Person => "person",
            Icon::Check => "check",
            Icon::XCircle => "x-circle",
            Icon::Clock => "clock",
            Icon::Circle => "circle",
            Icon::Dash => "dash",
            Icon::UserCheck => "user-check",
            Icon::Warning => "warning",
            Icon::GitCommit => "git-commit",
            Icon::ArrowUpRight => "arrow-up-right",
            Icon::Copy => "copy",
            Icon::TextSnippet => "text-snippet",
            Icon::GitHubMark => "github-mark",
            Icon::GitLabMark => "gitlab-mark",
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

    const UNREAD_DOT_SIZE: f32 = 6.0;

    /// The 6px info dot, centred on its title line and wholly inside the
    /// leading gutter the row's horizontal padding leaves, so it never
    /// overlaps the glyph. It is a child of the (relative) title line: it
    /// spans that line's height and sits left of it by half of the gutter
    /// plus half the dot, so it is centred in the gutter (row x 0 while the
    /// gutter and the dot are both 6px). This holds only while the title
    /// line's column is the row's first in-flow child, at `ROW_PAD_X`.
    /// Drawn by a worktree row and a Sessions card alike.
    pub(super) fn unread_dot(selector: String, theme: &Theme) -> impl IntoElement + use<> {
        let colour = theme.ely.info;
        div()
            .absolute()
            .top_0()
            .bottom_0()
            .left(px(-(ROW_PAD_X + Self::UNREAD_DOT_SIZE) / 2.0))
            .flex()
            .items_center()
            .child(
                div()
                    .debug_selector(move || selector.clone())
                    .size(px(Self::UNREAD_DOT_SIZE))
                    .rounded_full()
                    .bg(colour),
            )
    }

    fn render_pills(
        row: &SidebarRow,
        pill_cursor: Option<usize>,
        entity: gpui::Entity<Sidebar>,
        theme: Theme,
    ) -> impl IntoElement {
        let row_id = row.id;
        let worktree_path = row.path.clone();
        // The "+" sits outside the pills' clip, so the pills give way before it (only the card's
        // own clip, at the narrowest sidebar, can cut it).
        div()
            .flex()
            .min_w_0()
            .items_center()
            .gap(px(2.0))
            .child(
                // Clipped at its right edge when the name leaves it too little, so the
                // trailing seat never leaves the card. The padding keeps a pill's
                // status dot, which sits 2px outside the pill, from being cut.
                div()
                    .flex()
                    .min_w_0()
                    .overflow_hidden()
                    .py(px(2.0))
                    .pr(px(2.0))
                    .items_center()
                    .gap(px(4.0))
                    .children(row.pills.iter().enumerate().map(|(index, pill)| {
                        let select_entity = entity.clone();
                        let tab_id = pill.tab_id;
                        let parked = pill.parked_tab;
                        let path = worktree_path.clone();
                        div()
                            .id(("sidebar-pill", row_id * 32 + index))
                            .debug_selector(move || format!("sidebar-pill-{row_id}-{index}"))
                            .relative()
                            .flex_none()
                            .w(px(Self::PILL_SIZE))
                            .h(px(Self::PILL_SIZE))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(theme.radii.chip)
                            .bg(theme.ely.surface)
                            .when(pill.selected || pill_cursor == Some(index), |this| {
                                this.border_1().border_color(theme.sirio.quantity)
                            })
                            // Unread outranks the selection ring (the colour is the news), but
                            // never the keyboard cursor's: that one must stay findable.
                            .when(pill.unread && pill_cursor != Some(index), |this| {
                                this.border_1().border_color(theme.ely.info)
                            })
                            .when(parked.is_some(), |this| this.opacity(0.6))
                            .hover(|style| style.bg(theme.ely.hover))
                            .child(
                                div()
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
                                            .bg(if pill.unread {
                                                theme.ely.info
                                            } else {
                                                crate::status::status_color(status, theme)
                                            }),
                                    )
                                })
                            })
                            .on_click(move |_, _, cx| {
                                cx.stop_propagation();
                                select_entity.update(cx, |_, cx| {
                                    match (tab_id, parked, path.clone()) {
                                        (Some(id), _, _) => cx.emit(SidebarEvent::SelectTab(id)),
                                        (None, Some(index), Some(path)) => {
                                            cx.emit(SidebarEvent::SelectParkedTab { path, index })
                                        }
                                        _ => {}
                                    }
                                });
                            })
                    })),
            )
            .when(row.selected, |this| {
                let add_entity = entity.clone();
                this.child(
                    div()
                        .id(("sidebar-pill-add", row_id))
                        .debug_selector(move || format!("sidebar-pill-add-{row_id}"))
                        .flex_none()
                        .w(px(Self::PILL_SIZE))
                        .h(px(Self::PILL_SIZE))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(theme.radii.chip)
                        .border_1()
                        .border_color(theme.ely.border)
                        .text_color(theme.ely.fg_subtle)
                        .hover(|style| style.bg(theme.ely.hover))
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
        first_card: bool,
        removable: bool,
        first_removable: bool,
        missing_width: Pixels,
        entity: gpui::Entity<Self>,
        theme: Theme,
        window: &mut Window,
        cx: &mut App,
    ) -> impl IntoElement {
        let row_id = row.id;
        let selected = row.selected;
        let kind = row.kind;
        // The name the card draws: a plain folder has no branch, so its card
        // names the folder, muted.
        let (title, title_fallback) = if row.kind == RowKind::Worktree {
            Self::card_name(&row)
        } else {
            (row.title.clone(), false)
        };
        let path = row.path.clone();
        // The click closure reports the checkout path for worktree rows.
        let worktree_path = path.clone();
        let is_project = kind == RowKind::Project;
        let is_worktree = kind == RowKind::Worktree;
        // #372: the primary checkout offers no hover-x — like its disabled
        // context-menu item, it cannot be `git worktree remove`d. Nor does
        // git's main checkout, whatever the Primary marker says (`removable`).
        let is_removable_worktree = is_worktree && !row.is_primary && removable;
        // A worktree row is one 31px line: name, pills, trailing seat. Project
        // rows draw through `render_section` at `SECTION_HEIGHT`, so no other
        // row reaches this renderer today.
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
                .unwrap_or_else(|| Self::project_color(&title).into()),
            RowKind::Worktree => theme.ely.fg_subtle,
        };
        let entity = entity.clone();
        let remove_entity = entity.clone();
        let click_entity = entity.clone();
        let tab_close_entity = entity.clone();
        let context_entity = entity.clone();
        let hover_group = format!("sidebar-project-{row_id}");
        let tab_id = row.tab_id;
        let parked_tab = row.parked_tab;
        let mark_size = theme.typography.scaled(14.0);
        let icon_size = IconSize::Small;
        let project_mark = match project_icon.as_ref().map(|icon| &icon.value) {
            Some(ProjectIconValue::Emoji(emoji)) => div()
                .text_size(theme.typography.scaled(14.0))
                .child(emoji.clone())
                .into_any_element(),
            // A locally chosen PNG is real file content already on disk — no
            // network fetch needed, so it can render as an actual image
            // instead of the generic globe glyph every other avatar source
            // still falls back to (F-PRJ-14: GitHub/Favicon need an avatar
            // download, which Sirio does not do yet; see project_identity.rs).
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
            .rounded(px(6.0))
            .px(px(ROW_PAD_X))
            // The card's one line is centred in its 31px; the name lines up
            // with the project's name, and the box (so the hover wash and the
            // probe) stays full width.
            .when(!is_worktree, |this| this.py(px(ROW_PAD_Y)))
            .when(is_worktree, |this| this.pl(px(ROW_PAD_X + CARD_CONTENT_INDENT)))
            .flex()
            .gap(px(8.0))
            .when(selected, |this| this.bg(theme.ely.active))
            .when(!selected, |this| {
                this.hover(|style| style.bg(theme.ely.hover))
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
        // lease on `sirio_ui::motion`'s shared clock, and that lease is what re-renders
        // this row — and only this row — on every spinner frame.
        let status_bloom = match status_glyph {
            RowStatusGlyph::Running(tint) => Some(
                div()
                    .id(("sidebar-status-running", row_id))
                    .debug_selector(move || format!("sidebar-status-running-{row_id}"))
                    .flex_none()
                    .group_hover(hover_group.clone(), |style| style.invisible())
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
                    .group_hover(hover_group.clone(), |style| style.invisible())
                    .child(loading::settled_bloom(
                        "sidebar-settled-bloom",
                        loading::BLOOM_GLYPH,
                        tint,
                    ))
                    .into_any_element(),
            ),
            RowStatusGlyph::None => None,
        };
        // A worktree card's trailing seat. The bloom and the hover buttons
        // share it: under the pointer the buttons stand where the bloom was
        // (it goes invisible, keeping its box), so they sit at one x on every
        // card whatever its status, and the seat is the same width on all of
        // them -- the primary holds an empty place where the cross would be.
        // The first card's star records where it is drawn for the debug
        // layout read, hidden or not: a canvas prepaints either way.
        let star_probe = (is_worktree && first_card).then(|| entity.read(cx).probe.card_star.clone());
        let remove_probe =
            (is_worktree && first_removable).then(|| entity.read(cx).probe.card_remove.clone());
        let (slot_bloom, tail_bloom) = if is_worktree {
            (status_bloom, None)
        } else {
            (None, status_bloom)
        };
        let trailing_slot = {
            let star_entity = entity.clone();
            let remove_entity = entity.clone();
            let is_primary = row.is_primary;
            let star_color = if is_primary { theme.ely.fg } else { theme.ely.fg_subtle };
            div()
                .relative()
                .flex_none()
                .w(px(TRAILING_SLOT_WIDTH))
                .self_stretch()
                .child(
                    div()
                        .absolute()
                        .top_0()
                        .bottom_0()
                        .right_0()
                        .flex()
                        .items_center()
                        .children(slot_bloom),
                )
                // The primary's rest-state star stands in the ★ seat, where
                // the ★ button appears under the pointer, so it does not jump
                // when the pointer arrives.
                .when(is_primary, |this| {
                    this.child(
                        div()
                            .debug_selector(move || format!("sidebar-primary-star-{row_id}"))
                            .absolute()
                            .top_0()
                            .bottom_0()
                            .left_0()
                            .w(px(16.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .group_hover(hover_group.clone(), |style| style.invisible())
                            .child(
                                IconElement::new(Icon::StarFilled, IconSize::XSmall)
                                    .text_color(theme.ely.fg_subtle),
                            ),
                    )
                })
                .child(
                    div()
                        .absolute()
                        .top_0()
                        .bottom_0()
                        .left_0()
                        .right_0()
                        .flex()
                        .items_center()
                        .justify_end()
                        .gap(px(6.0))
                        .child(
                            div()
                                .id(("sidebar-card-star", row_id))
                                .debug_selector(move || format!("sidebar-card-star-{row_id}"))
                                .cursor(gpui::CursorStyle::PointingHand)
                                .w(px(16.0))
                                .flex_none()
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded(theme.radii.chip)
                                .hover(|style| style.bg(theme.ely.hover))
                                .invisible()
                                .group_hover(hover_group.clone(), |style| style.visible())
                                .when_some(star_probe, |this, cell| {
                                    this.child(
                                        gpui::canvas(
                                            move |bounds, _, _| cell.set(Some(bounds)),
                                            |_, _, _, _| {},
                                        )
                                        .absolute()
                                        .top_0()
                                        .left_0()
                                        .size_full(),
                                    )
                                })
                                .on_click(move |_, window, cx| {
                                    cx.stop_propagation();
                                    star_entity.update(cx, |sidebar, cx| {
                                        let Some(target) = sidebar.context_target(row_id) else {
                                            return;
                                        };
                                        let action = if is_primary {
                                            SidebarContextAction::UnsetPrimary
                                        } else {
                                            SidebarContextAction::SetPrimary
                                        };
                                        sidebar.dispatch_context_action(target, action, window, cx);
                                    });
                                })
                                // The primary is the filled star, the same one the
                                // rest-state marker uses; a linked worktree is outline.
                                .child(if is_primary {
                                    IconElement::new(Icon::StarFilled, IconSize::XSmall)
                                        .selector(format!("sidebar-card-star-filled-{row_id}"))
                                        .text_color(star_color)
                                        .into_any_element()
                                } else {
                                    EIcon::new(IconName::Star)
                                        .size(EIconSize::Xs)
                                        .color(star_color)
                                        .into_any_element()
                                }),
                        )
                        .child(if is_removable_worktree {
                            div()
                                .id(("remove-worktree", row_id))
                                .debug_selector(move || format!("remove-worktree-{row_id}"))
                                .w(px(16.0))
                                .flex_none()
                                .flex()
                                .items_center()
                                .justify_center()
                                .text_size(theme.typography.scaled(12.0))
                                .text_color(theme.ely.fg_subtle)
                                .rounded(theme.radii.chip)
                                .hover(|style| style.bg(theme.ely.hover))
                                .invisible()
                                .group_hover(hover_group.clone(), |style| style.visible())
                                .when_some(remove_probe, |this, cell| {
                                    this.child(
                                        gpui::canvas(
                                            move |bounds, _, _| cell.set(Some(bounds)),
                                            |_, _, _, _| {},
                                        )
                                        .absolute()
                                        .top_0()
                                        .left_0()
                                        .size_full(),
                                    )
                                })
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
                                        .text_color(theme.ely.fg_subtle),
                                )
                                .into_any_element()
                        } else {
                            div().w(px(16.0)).flex_none().into_any_element()
                        }),
                )
        };
        // Any unread pill makes the whole worktree row read as news: a dot at its
        // leading edge and a heavier title. A project row is already semibold.
        let row_unread = is_worktree && row.pills.iter().any(|pill| pill.unread);
        let title_element = div()
            .min_w_0()
            // A card's name sizes to its text (so "(missing)" follows it) and
            // shrinks first; any other title takes the line.
            .when(!is_worktree, |this| this.flex_1())
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
            .font_weight(if is_project || row_unread {
                FontWeight::SEMIBOLD
            } else {
                FontWeight::NORMAL
            })
            .text_color(if title_fallback {
                theme.ely.fg_muted
            } else {
                Self::title_color(parked_tab.is_some(), theme)
            })
            .child(title);
        // The name and, after it, "(missing)": the state never ellipsises, the
        // name is the detail and gives way. The slot's floor is the name's 40 px plus
        // "(missing)", measured, so the label never runs onto the pills and the name keeps 40 px
        // (only the card's own clip, at the narrowest sidebar, can cut the label's end).
        let missing_reserve = if row.missing {
            px(MISSING_GAP) + missing_width
        } else {
            px(0.0)
        };
        let name_slot = if !is_worktree {
            title_element.into_any_element()
        } else {
            div()
                .flex_1()
                .min_w(px(NAME_MIN_WIDTH) + missing_reserve)
                .flex()
                .items_center()
                .gap(px(MISSING_GAP))
                .child(title_element)
                .when(row.missing, |this| {
                    this.child(
                        div()
                            .debug_selector(move || format!("sidebar-row-missing-{row_id}"))
                            .flex_none()
                            .whitespace_nowrap()
                            .text_size(theme.typography.scaled(ROW_TITLE_FONT_SIZE))
                            .line_height(Self::title_line_height(&theme.typography))
                            .text_color(theme.ely.fg_subtle)
                            .child("(missing)"),
                    )
                })
                .into_any_element()
        };
        let mark = {
            let slot = div().w(px(16.0)).flex_none().flex().items_center().justify_center();
            if is_worktree {
                let name = Self::icon_selector_name(glyph);
                slot.id(("sidebar-worktree-mark", row_id))
                    .debug_selector(move || format!("sidebar-worktree-mark-{row_id}-{name}"))
                    .child(project_mark)
                    .into_any_element()
            } else {
                slot.child(project_mark).into_any_element()
            }
        };
        let title_line = div()
            .relative()
            .w_full()
            .flex()
            .items_center()
            .gap(px(6.0))
            .when(row_unread, |this| {
                this.child(Self::unread_dot(format!("sidebar-unread-dot-{row_id}"), &theme))
            });
        let title_line = if is_worktree {
            // A card's content is clipped at its right edge, before the trailing seat, so the seat
            // never moves or gets covered at the narrowest sidebar; the content's right end gives.
            title_line.child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .child(mark)
                    .child(name_slot)
                    // The tab pills, before the trailing seat (below).
                    .child(Self::render_pills(
                        &row,
                        cursor.then_some(pill_cursor).flatten(),
                        entity.clone(),
                        theme,
                    )),
            )
        } else {
            title_line.child(mark).child(name_slot)
        };
        let title_line = title_line
            .when(is_project, |this| {
                this.child(
                    div()
                        .id(("project-settings", row_id))
                        .debug_selector(move || format!("project-settings-{row_id}"))
                        .cursor(gpui::CursorStyle::PointingHand)
                        .w(px(16.0))
                        .flex_none()
                        .text_size(theme.typography.scaled(13.0))
                        .text_color(theme.ely.fg_subtle)
                        .invisible()
                        .group_hover(hover_group.clone(), |style| style.visible())
                        .child(
                            IconElement::new(Icon::Settings, IconSize::XSmall)
                                .text_color(theme.ely.fg),
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
            .when_some(tab_id, |this, tab_id| {
                this.child(
                    div()
                        .id(("sidebar-tab-close", row_id))
                        .debug_selector(move || format!("sidebar-tab-close-{row_id}"))
                        .w(px(16.0))
                        .flex_none()
                        .text_size(theme.typography.scaled(13.0))
                        .text_color(theme.ely.fg_muted)
                        .rounded(theme.radii.chip)
                        .hover(|style| style.bg(theme.ely.hover))
                        .invisible()
                        .group_hover(hover_group.clone(), |style| style.visible())
                        .on_click(move |_, _, cx| {
                            cx.stop_propagation();
                            tab_close_entity.update(cx, |_, cx| {
                                cx.emit(SidebarEvent::CloseTab(tab_id));
                            });
                        })
                        .child(
                            IconElement::new(Icon::Close, IconSize::XSmall).text_color(theme.ely.fg),
                        ),
                )
            })
            // Last. A collapsed project's bloom lands here; a worktree's rides
            // in `trailing_slot`, which is the same width on every card, so
            // the bloom keeps one trailing edge across rows
            // (`running_agent_badges_share_one_trailing_edge_across_worktrees`).
            .when_some(tail_bloom, |this, bloom| this.child(bloom))
            .when(is_worktree, |this| this.child(trailing_slot));
        let content = div()
            .min_w_0()
            .flex_1()
            .flex()
            .flex_col()
            .justify_center()
            .child(title_line);

        // The first worktree card records where it is drawn for the debug
        // layout read. The card has no border, so a full-size absolute child
        // spans its whole box.
        let card_probe = (is_worktree && first_card).then(|| entity.read(cx).probe.card.clone());
        let guide_probe = (is_worktree && first_card).then(|| entity.read(cx).probe.guide.clone());
        // A worktree card reports where it is and whether the pointer is on it,
        // for the hover card beside the sidebar.
        let hover_bounds = std::rc::Rc::new(std::cell::Cell::new(None::<Bounds<Pixels>>));
        let hover_probe = is_worktree.then(|| hover_bounds.clone());
        let row_view = if is_worktree {
            let hover_entity = entity.clone();
            row_view.on_hover(move |hovered, _, cx| {
                let bounds = hover_bounds.get();
                hover_entity.update(cx, |sidebar, cx| {
                    sidebar.hover_card_hover(row_id, bounds, *hovered, cx)
                });
            })
        } else {
            row_view
        };
        row_view
            // The guide line down an open project: Ely Tree's `guides()`
            // recipe (a 1px line in the border colour), one segment per card.
            // It starts `ROW_V_GAP` above the card, across the seam, so the
            // segments join and the first one starts at the project row's
            // bottom. It is drawn over the card's hover and selection wash,
            // as Ely draws its guides over its rows. A 1px fill on the segment itself, the
            // same pixels as Ely's `border_l`: drawn as a bordered full-size child here, the
            // line did not paint (the E2E's pixel column stayed the ground).
            .when(is_worktree, |this| {
                this.child(
                    div()
                        .debug_selector(move || format!("sidebar-guide-{row_id}"))
                        .absolute()
                        .top(px(-ROW_V_GAP))
                        .bottom_0()
                        .left(px(GUIDE_X))
                        .w(px(1.0))
                        .bg(theme.ely.border)
                        .when_some(guide_probe, |this, cell| this.child(probe_canvas(&cell))),
                )
            })
            .when_some(hover_probe, |this, cell| {
                this.child(
                    gpui::canvas(move |bounds, _, _| cell.set(Some(bounds)), |_, _, _, _| {})
                        .absolute()
                        .top_0()
                        .left_0()
                        .size_full(),
                )
            })
            .child(content)
            .when_some(card_probe, |this, cell| {
            this.child(
                gpui::canvas(move |bounds, _, _| cell.set(Some(bounds)), |_, _, _, _| {})
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full(),
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sidebar::tests_support;
    use gpui::{Modifiers, TestAppContext, VisualTestContext};

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

    /// A mouse always hovers the pill before it clicks it, and the old
    /// design used that hover to swap the mark for a full-size ✕ -- so the
    /// click that selects was unreachable with a real pointer, and the
    /// gesture the user meant as "show me this tab" closed it instead. The
    /// mark is the whole control now, and a hovered click selects like any
    /// other; closing lives on the tab strip's own ✕.
    #[gpui::test]
    async fn a_hovered_pill_still_selects_its_tab(cx: &mut TestAppContext) {
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
        cx.simulate_mouse_move(pill.center(), None, Modifiers::none());
        cx.run_until_parked();
        assert!(
            cx.debug_bounds("sidebar-pill-close-1-0").is_none(),
            "the pill carries no close control, hovered or not"
        );
        cx.simulate_click(pill.center(), Modifiers::none());
        cx.run_until_parked();

        assert!(matches!(
            events.borrow().last(),
            Some(SidebarEvent::SelectTab(1))
        ));
    }

}
