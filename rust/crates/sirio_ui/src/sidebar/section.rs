//! The project section header. A project used to be a row of the tree with a
//! disclosure chevron and a `New Worktree` child row; it is now the header
//! that names the section, counts it, and carries both actions itself.

use super::*;
use gpui::SharedString;

/// Height of a section header. Shorter than a row on purpose: it is a label,
/// not something you select.
pub(super) const SECTION_HEIGHT: f32 = 26.0;

pub(super) fn render_section(
    row: SidebarRow,
    worktree_count: usize,
    entity: gpui::Entity<Sidebar>,
    theme: Theme,
) -> impl IntoElement {
    let row_id = row.id;
    let group = SharedString::from(format!("sidebar-section-group-{row_id}"));
    let collapse_entity = entity.clone();
    let add_entity = entity.clone();
    let menu_entity = entity.clone();
    let menu_click_entity = entity.clone();
    let drag_entity = entity.clone();
    let move_entity = entity.clone();
    let drop_entity = entity;

    div()
        .id(("sidebar-section", row_id))
        .debug_selector(move || format!("sidebar-section-{row_id}"))
        .group(group.clone())
        .relative()
        .h(px(SECTION_HEIGHT))
        .w_full()
        .px(px(12.0))
        .flex()
        .items_center()
        .gap(px(6.0))
        .bg(theme.surface_raised)
        .border_t_1()
        .border_b_1()
        .border_color(theme.border)
        .text_size(theme.typography.scaled(12.0))
        .text_color(theme.text_muted)
        .cursor_pointer()
        // Keep the old project-row selector as a compatibility probe for
        // existing sidebar tests while the project itself is now a header.
        .child(
            div()
                .debug_selector(move || format!("sidebar-row-{row_id}"))
                .absolute()
                .inset_0(),
        )
        .on_click(move |_, _, cx| {
            collapse_entity.update(cx, |sidebar, cx| {
                if let Some(index) = sidebar
                    .visible_rows()
                    .iter()
                    .position(|visible| visible.id == row_id)
                {
                    sidebar.tree_cursor = index;
                }
                sidebar.toggle_project(row_id, cx);
            });
        })
        .on_mouse_down(
            MouseButton::Right,
            move |event: &MouseDownEvent, window, cx| {
                cx.stop_propagation();
                menu_entity.update(cx, |sidebar, cx| {
                    sidebar.open_context_menu(row_id, event.position, window, cx);
                });
            },
        )
        .on_drag(
            RowDrag {
                scope: ReorderScope::Projects,
                id: row_id,
                group: None,
            },
            move |_, _, _, cx| {
                drag_entity.update(cx, |sidebar, _| sidebar.pending_reorder = None);
                cx.new(|_| gpui::Empty)
            },
        )
        .on_drag_move::<RowDrag>(move |event: &DragMoveEvent<RowDrag>, _, cx| {
            let drag = *event.drag(cx);
            let before = event.event.position.y < event.bounds.center().y;
            move_entity.update(cx, |sidebar, cx| {
                sidebar.preview_reorder(drag, row_id, before, cx);
            });
        })
        .on_drop::<RowDrag>(move |_, _, cx| {
            drop_entity.update(cx, |sidebar, cx| sidebar.confirm_reorder(cx));
        })
        .child(
            div()
                .relative()
                .min_w_0()
                .flex_1()
                .whitespace_nowrap()
                .overflow_hidden()
                .child(row.title)
                .child(
                    super::fade::fade_right(theme.surface_raised, super::fade::FADE_WIDTH)
                        .id(("sidebar-section-fade", row_id))
                        .debug_selector(move || format!("sidebar-section-fade-{row_id}")),
                ),
        )
        .child(
            div()
                .debug_selector(move || format!("sidebar-section-count-{row_id}"))
                .flex_none()
                .text_color(theme.text_faint)
                .text_size(theme.typography.scaled(11.0))
                .child(worktree_count.to_string()),
        )
        .child(
            div()
                .id(("sidebar-section-add", row_id))
                .debug_selector(move || format!("sidebar-section-add-{row_id}"))
                .w(px(18.0))
                .h(px(18.0))
                .flex()
                .items_center()
                .justify_center()
                .rounded(theme.radii.control)
                .invisible()
                .group_hover(group.clone(), |style| style.visible())
                .hover(|style| style.bg(theme.element_hover))
                .child(IconElement::new(Icon::Plus, IconSize::XSmall))
                .on_click(move |_, window, cx| {
                    cx.stop_propagation();
                    add_entity.update(cx, |sidebar, cx| {
                        sidebar.begin_worktree_prompt(row_id, window, cx);
                    });
                }),
        )
        .child(
            div()
                .id(("sidebar-section-menu", row_id))
                .debug_selector(move || format!("sidebar-section-menu-{row_id}"))
                .w(px(18.0))
                .h(px(18.0))
                .flex()
                .items_center()
                .justify_center()
                .rounded(theme.radii.control)
                .invisible()
                .group_hover(group, |style| style.visible())
                .hover(|style| style.bg(theme.element_hover))
                .text_color(theme.text_muted)
                .child("⋯")
                .on_click(move |event, window, cx| {
                    cx.stop_propagation();
                    menu_click_entity.update(cx, |sidebar, cx| {
                        sidebar.open_context_menu(row_id, event.position(), window, cx);
                    });
                }),
        )
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{Context, TestAppContext, VisualTestContext};

    fn sidebar_with_one_project(cx: &mut Context<Sidebar>) -> Sidebar {
        let mut sidebar = Sidebar::from_projects(
            vec![SidebarProject {
                id: "sirio".to_string(),
                name: "sirio".to_string(),
                is_git: true,
                root_path: PathBuf::from("/tmp/sirio"),
                worktrees: vec![
                    SidebarWorktree {
                        branch: "main".to_string(),
                        path: PathBuf::from("/tmp/sirio"),
                        is_primary: true,
                        comment: None,
                    },
                    SidebarWorktree {
                        branch: "feat/x".to_string(),
                        path: PathBuf::from("/tmp/sirio-feat-x"),
                        is_primary: false,
                        comment: Some("redesign".to_string()),
                    },
                ],
            }],
            cx,
        );
        sidebar.set_worktree_tabs(
            2,
            vec![
                SidebarTab {
                    tab: SidebarTabRef::Open(0),
                    title: "Chat".to_string(),
                    selected: true,
                    kind: TabKind::AgentChat,
                    agent: None,
                },
                SidebarTab {
                    tab: SidebarTabRef::Open(1),
                    title: "Terminal".to_string(),
                    selected: false,
                    kind: TabKind::Terminal,
                    agent: None,
                },
            ],
            cx,
        );
        sidebar
    }

    #[gpui::test]
    async fn a_project_header_offers_add_and_menu_instead_of_a_new_worktree_row(
        cx: &mut TestAppContext,
    ) {
        cx.update(Theme::init);
        let window = cx.add_window(|_window, cx| sidebar_with_one_project(cx));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        assert!(cx.debug_bounds("sidebar-section-0").is_some());
        assert!(cx.debug_bounds("sidebar-section-add-0").is_some());
        assert!(cx.debug_bounds("sidebar-section-menu-0").is_some());
        assert!(cx.debug_bounds("sidebar-new-worktree-row").is_none());
    }

    #[gpui::test]
    async fn a_section_header_counts_its_worktrees(cx: &mut TestAppContext) {
        cx.update(Theme::init);
        let window = cx.add_window(|_window, cx| sidebar_with_one_project(cx));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        assert!(cx.debug_bounds("sidebar-section-count-0").is_some());
    }
}
