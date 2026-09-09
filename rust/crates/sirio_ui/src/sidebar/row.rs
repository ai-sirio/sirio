//! The sidebar's worktree card: one row of the flattened tree, its status
//! glyph, its hover controls and (from Task 4 on) its tab pills.

//! Split out of `sidebar.rs` so the row's drawing can grow without the
//! entity's state, popups and drag handling growing with it — the same
//! `mod.rs` + one-file-per-surface arrangement `right_panel/` uses.

use super::*;

/// What the leading status column of a worktree (or collapsed project) row
/// draws — a port of `TillerCore/SidebarGlyph.swift`'s `SidebarGlyphKind`,
/// with the extra `Idle` case the Rust `ActivityStatus` carries folded onto
/// the same `None` the Swift `nil` status maps to.
///
/// The Swift table is the contract, and two of its rows had been inverted
/// here: `Idle` drew the amber needs-input dot, so a worktree with nothing
/// happening was pixel-identical to one waiting on an answer, and `Running`
/// drew nothing at all, so a busy worktree looked empty. Both now follow
/// `SidebarGlyphKind.forStatus`: nothing for no status, the loader for
/// running, a lifecycle dot for the rest.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum RowStatusGlyph {
    /// No glyph. The column keeps its width so rows stay aligned.
    None,
    /// The running indicator, tinted with the agent's **brand** — Swift's
    /// `RunningDots(color: AgentIcon.color(for: agentId))`, whose whole
    /// purpose is to say *whose* work is in progress.
    Running(Rgba),
    /// A static lifecycle dot: amber needs-input, green done, red error.
    Dot(Rgba),
}

impl RowStatusGlyph {
    pub(super) fn for_status(
        status: Option<ActivityStatus>,
        brand: Option<AgentBrandColor>,
        theme: Theme,
    ) -> Self {
        match status {
            None | Some(ActivityStatus::Idle) => Self::None,
            // The tint is the agent's brand, never a `Theme` status token.
            // Routed through the eight-token settings palette it used to be
            // one — Claude resolved to `Amber`, i.e. to `tab_needs_input` —
            // so a *running* Claude worktree and one that *needed input*
            // painted the same `#E0B36A` and differed only by dot geometry.
            // An unidentified agent gets the neutral fallback, matching
            // `AgentIcon.color(for: agentId ?? "")`'s `.gray`.
            Some(ActivityStatus::Running) => {
                Self::Running(brand.unwrap_or(AgentBrandColor::Unknown).color())
            }
            Some(ActivityStatus::NeedsInput) => Self::Dot(theme.warning),
            Some(ActivityStatus::Done) => Self::Dot(theme.success),
            Some(ActivityStatus::Error) => Self::Dot(theme.danger),
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
    /// Whether the row has rows under it in the full tree — a worktree's
    /// tab rows, which may be hidden by its own disclosure. Decides the
    /// tree shape (`Sidebar::tree_row`), so it is an input like the rest.
    pub(super) has_children: bool,
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
        self.render_count = self.render_count.wrapping_add(1);
        let theme = *Theme::get(cx);
        let bezel_theme = bezel::theme::Theme::of(cx).clone();
        let inputs = self.inputs.clone();
        let row_shape = Sidebar::tree_row(&inputs.row, inputs.has_children);
        // Boxed here: the opaque return type of `render_row` captures the
        // borrow of `bezel_theme`, which ends with this frame's render.
        Sidebar::render_row(
            inputs.row,
            inputs.index,
            row_shape,
            inputs.cursor,
            inputs.project_id,
            inputs.project_icon,
            inputs.drag,
            self.sidebar.clone(),
            theme,
            &bezel_theme,
            window,
            cx,
        )
        .into_any_element()
    }
}

impl Sidebar {
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
        matches!(row.kind, RowKind::Project | RowKind::Worktree)
            && (row.is_primary
                || row
                    .comment
                    .as_ref()
                    .is_some_and(|comment| !comment.is_empty()))
    }

    pub(super) fn row_min_height(row: &SidebarRow) -> f32 {
        if Self::has_sub_line(row) {
            CARD_TWO_LINE_HEIGHT
        } else {
            ROW_HEIGHT
        }
    }

    /// The structural row handed to bezel. Sirio keeps the data and content;
    /// bezel owns branch/leaf identity, indentation, disclosure and chrome.
    /// The bezel tree shape of one row. A project is a container even when
    /// empty and always carries a chevron; a worktree earns one only while
    /// it has tab rows to hide (`has_children`), so an idle worktree with
    /// nothing under it does not grow a disclosure that opens onto nothing.
    pub(super) fn tree_row(row: &SidebarRow, has_children: bool) -> tree::Row {
        match row.kind {
            RowKind::Project => tree::Row::branch(0, row.expanded),
            RowKind::Worktree if has_children => tree::Row::branch(1, row.expanded),
            RowKind::Worktree | RowKind::NewWorktree => tree::Row::leaf(1),
            RowKind::Tab => tree::Row::leaf(2),
        }
    }

    /// Apply one of bezel's standard tree directions to the currently
    /// visible, depth-annotated rows. Expansion remains Sirio state; bezel
    /// reports only the intent.
    /// Stable semantic debug/test names, independent of vendored filenames.
    fn icon_selector_name(icon: Icon) -> &'static str {
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
            RowKind::Tab => row.agent_icon.unwrap_or(match row.tab_kind {
                Some(TabKind::Terminal) => Icon::SquareTerminal,
                Some(TabKind::Editor | TabKind::Diff) => Icon::File,
                _ => Icon::MessageSquare,
            }),
            RowKind::NewWorktree => Icon::Plus,
        }
    }

    fn render_row(
        row: SidebarRow,
        row_index: usize,
        row_shape: tree::Row,
        cursor: bool,
        project_id: Option<String>,
        project_icon: Option<ProjectIcon>,
        drag: Option<RowDrag>,
        entity: gpui::Entity<Self>,
        theme: Theme,
        bezel_theme: &bezel::theme::Theme,
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
        let row_min_height = Self::row_min_height(&row);
        // F-CORE-ACT-18: the trailing running-agents badge is one 12px mark
        // per distinct running agent, 3px apart, 7px clear of the title. It
        // takes its width out of the title's, so a busy worktree truncates
        // its branch name instead of pushing the hover controls off the row.
        let running_agents: Vec<AgentMark> = if kind == RowKind::Worktree {
            row.running_agents.clone()
        } else {
            Vec::new()
        };
        // A glyph only appears for a notable status — matching the
        // reference. A collapsed project also gets one (F-SID-06): its
        // worktree rows are hidden, so `row.agent_status` was pre-aggregated
        // onto the project row itself in `visible_rows`.
        let status_glyph =
            if kind == RowKind::Worktree || (kind == RowKind::Project && !row.expanded) {
                RowStatusGlyph::for_status(row.agent_status, row.agent_brand, theme)
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
            RowKind::Tab => {
                Self::tab_row_icon_color(row.agent_brand, row.agent_icon.is_some(), theme)
            }
            RowKind::Worktree | RowKind::NewWorktree => theme.text_faint,
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

        let row_debug_selector = if kind == RowKind::NewWorktree {
            "new-worktree-row".to_string()
        } else {
            format!("sidebar-row-{row_id}")
        };
        let mut row_view = tree::tree_row(bezel_theme, &row_shape, selected, cursor)
            .text_size(theme.typography.scaled(12.5))
            .id(row_id)
            .debug_selector(move || row_debug_selector)
            .group(hover_group.clone())
            .relative()
            .min_h(px(row_min_height))
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
                        RowKind::NewWorktree => {
                            sidebar.begin_worktree_prompt(row_id, window, cx);
                        }
                        RowKind::Tab => {}
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

        // Sirio owns the row's content; bezel's tree row already supplied
        // disclosure, indentation, cursor/selection paint and hover chrome.
        let main_line = div()
            .flex()
            .items_center()
            .gap(px(7.0))
            .min_h(px(ROW_TITLE_LINE_HEIGHT))
            .child(
                div()
                    .w(px(12.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(px(12.0))
                    .text_color(theme.text_faint)
                    .child(match status_glyph {
                        // Swift's `RunningDots`, tinted by the agent: a
                        // different *shape* from a lifecycle dot, so a
                        // running worktree can never be mistaken for a
                        // finished one at a glance, and a different tint per
                        // agent, so the one glyph carries both facts.
                        RowStatusGlyph::Running(_color) => div()
                            .id(("sidebar-status-running", row_id))
                            .debug_selector(move || format!("sidebar-status-running-{row_id}"))
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(loading::compact("sidebar-running-spinner", window, cx))
                            .into_any_element(),
                        RowStatusGlyph::Dot(color) => div()
                            .id(("sidebar-status-dot", row_id))
                            .debug_selector(move || format!("sidebar-status-dot-{row_id}"))
                            .w(px(6.0))
                            .h(px(6.0))
                            .rounded(px(3.0))
                            .bg(color)
                            .into_any_element(),
                        RowStatusGlyph::None => div().into_any_element(),
                    }),
            )
            .child({
                let slot = div().w(px(16.0)).flex().items_center().justify_center();
                // F-CORE-ACT-17: a worktree row's mark is its agent's brand
                // when one owns the worktree, and the branch glyph
                // otherwise. The selector carries which, so the identity is
                // assertable from a drawn test.
                if is_worktree {
                    let name = Self::icon_selector_name(glyph);
                    slot.id(("sidebar-worktree-mark", row_id))
                        .debug_selector(move || format!("sidebar-worktree-mark-{row_id}-{name}"))
                        .child(project_mark)
                        .into_any_element()
                } else if kind == RowKind::Tab {
                    // A tab row names its glyph the same way, so a drawn test
                    // can assert that a pane identified after spawn actually
                    // changed the mark on screen rather than only in a field.
                    let name = Self::icon_selector_name(glyph);
                    slot.id(("sidebar-tab-mark", row_id))
                        .debug_selector(move || format!("sidebar-tab-mark-{row_id}-{name}"))
                        .child(project_mark)
                        .into_any_element()
                } else {
                    slot.child(project_mark).into_any_element()
                }
            })
            .child(
                div()
                    .min_w_0()
                    .flex_1()
                    .whitespace_nowrap()
                    .overflow_hidden()
                    .debug_selector(move || format!("sidebar-row-title-{row_id}"))
                    .text_ellipsis()
                    .line_height(px(ROW_TITLE_LINE_HEIGHT))
                    .font_weight(if is_project {
                        FontWeight::SEMIBOLD
                    } else {
                        FontWeight::NORMAL
                    })
                    // A parked tab is not live: its title reads as a record
                    // of what the worktree holds, not as an open surface.
                    .when(parked_tab.is_some(), |this| {
                        this.text_color(theme.text_faint)
                    })
                    .child(title),
            )
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
            // F-CORE-ACT-18: `AgentActivityModel::running_agent_ids` already
            // de-duplicated these and put them in `AgentCatalog` order, so
            // the badge draws them left to right exactly as handed over —
            // it never re-sorts and never de-duplicates again.
            .when(!running_agents.is_empty(), |this| {
                this.child(
                    div()
                        .id(("sidebar-running-agents", row_id))
                        .debug_selector(move || format!("sidebar-running-agents-{row_id}"))
                        .flex()
                        .flex_none()
                        .items_center()
                        .gap(px(3.0))
                        .children(running_agents.iter().enumerate().map(|(index, mark)| {
                            div()
                                .id(("sidebar-running-agent", row_id * 16 + index))
                                .debug_selector({
                                    let name = Self::icon_selector_name(mark.icon);
                                    move || format!("sidebar-running-agent-{row_id}-{name}")
                                })
                                .flex()
                                .flex_none()
                                .items_center()
                                // Each mark in its own brand. Every mark used
                                // to be tinted `theme.text`, a
                                // coral near enough to Claude's brand to read
                                // as it, so a Codex or Pi mark was drawn in
                                // Claude's colour. Shape carried identity;
                                // colour actively contradicted it. Codex is the
                                // one exception: its mark is drawn in
                                // `theme.text` (white) like everywhere else
                                // in the app — tab bar and status bar never
                                // use its blue brand hex, so the badge must
                                // not be the only blue Codex mark on screen.
                                .child(IconElement::new(mark.icon, IconSize::Small).text_color(
                                    if matches!(mark.icon, Icon::Codex) {
                                        theme.text
                                    } else {
                                        mark.brand.color()
                                    },
                                ))
                        })),
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
            });

        let content = div()
            .min_w_0()
            .flex_1()
            .flex()
            .flex_col()
            .justify_center()
            .gap(px(ROW_GAP))
            .child(main_line)
            .when(has_sub_line, |this| {
                this.child(
                    div()
                        // Aligned under the title: 12px leading slot + 7px gap
                        // + 16px glyph + 7px gap.
                        .pl(px(42.0))
                        .w_full()
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .text_size(px(12.5))
                        .line_height(px(ROW_SUB_LINE_HEIGHT))
                        .text_color(theme.text_faint)
                        .when(row.is_primary, |this| {
                            this.child(
                                div()
                                    .id(("sidebar-primary-pill", row_id))
                                    .debug_selector(move || {
                                        format!("sidebar-primary-pill-{row_id}")
                                    })
                                    .px(px(5.0))
                                    .rounded(theme.radii.chip)
                                    .bg(theme.surface_raised)
                                    .text_color(theme.text)
                                    .text_size(theme.typography.scaled(11.0))
                                    .child("Primary"),
                            )
                        })
                        // F-SID-11: the durable `worktree.comment` annotation
                        // (`worktree.set` over the control socket) was already
                        // persisted and read by the status bar; the worktree
                        // row itself never rendered it.
                        .when_some(
                            row.comment.filter(|comment| !comment.is_empty()),
                            |this, comment| {
                                this.child(
                                    div()
                                        .id(("sidebar-worktree-comment", row_id))
                                        .debug_selector(move || {
                                            format!("sidebar-worktree-comment-{row_id}")
                                        })
                                        .min_w_0()
                                        .truncate()
                                        .text_color(theme.text_faint)
                                        .child(comment),
                                )
                            },
                        ),
                )
            });

        // A worktree's disclosure is its own control, unlike a project's,
        // whose whole row toggles: the row body must keep meaning "select
        // this worktree". bezel draws the chevron with no handler of its
        // own, so a hit target the size of its column sits over it and
        // stops the click before the row's selection handler sees it.
        let chevron_entity = entity.clone();
        let worktree_chevron = is_worktree && row_shape.expanded.is_some();
        let chevron_left = tree::INDENT * row_shape.depth as f32;
        row_view.child(content).when(worktree_chevron, |this| {
            this.child(
                div()
                    .id(("sidebar-worktree-chevron", row_id))
                    .debug_selector(move || format!("sidebar-worktree-chevron-{row_id}"))
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .left(px(chevron_left))
                    .w(px(16.0))
                    .cursor_pointer()
                    .on_click(move |_, window, cx| {
                        cx.stop_propagation();
                        chevron_entity.update(cx, |sidebar, cx| {
                            sidebar.focus_tree_row(row_index, window, cx);
                            sidebar.toggle_worktree(row_id, cx);
                        });
                    }),
            )
        })
    }
}
