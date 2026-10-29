//! The right panel's Open view (spec 2026-10-15 §2): every surface the
//! Secondary pane can open, as a vertical list of compact rows. What a row
//! *does* stays with the host, the only place that can create a tab: a click
//! reports an [`OpenSurface`] and the host opens it. What only the host knows
//! — the agents a chat can start with, why a row is unavailable — arrives as
//! [`OpenListFacts`], pushed every render.

use gpui::{AnyElement, App, Entity, SharedString, Window, div, prelude::*, px};
use sirio_theme::Theme;

use super::files::ROW_HEIGHT;
use super::{RightPanel, RightPanelActionEvent};
use crate::sidebar::icons::{Icon, IconElement, IconSize};

/// What a row of the Open list asks the host to open in the Secondary pane.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpenSurface {
    Terminal,
    /// An adapter id from `sirio_agents::ALL`.
    Chat(&'static str),
    /// Settings → Agents, where an agent that is not on PATH is set up.
    OtherAgents,
    Browser,
    Changes,
    OpenFile,
    ProjectSettings,
}

/// What only the host knows, pushed in every render.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct OpenListFacts {
    /// Agents a chat can open with today, as (adapter id, display name).
    pub agents: Vec<(&'static str, SharedString)>,
    pub changes_disabled: Option<SharedString>,
    pub project_settings_disabled: Option<SharedString>,
}

/// Left padding of a top-level row; an agent row adds `AGENT_INDENT`.
const ROW_INSET: f32 = 10.0;
const AGENT_INDENT: f32 = 22.0;

impl RightPanel {
    /// The host's facts. A no-op when they are unchanged, because the host
    /// calls this from its own render: notifying on equal facts would make
    /// every frame schedule the next.
    pub fn set_open_list_facts(&mut self, facts: OpenListFacts, cx: &mut gpui::Context<Self>) {
        if self.open_list_facts == facts {
            return;
        }
        self.open_list_facts = facts;
        cx.notify();
    }

    /// Expands or collapses Chat's agent rows, as a click on Chat does.
    pub fn toggle_open_list_chat(&mut self, cx: &mut gpui::Context<Self>) {
        self.open_list_chat_expanded = !self.open_list_chat_expanded;
        cx.notify();
    }

    pub fn open_list_chat_expanded(&self) -> bool {
        self.open_list_chat_expanded
    }

    pub(super) fn render_open_list(&self, entity: Entity<Self>, theme: Theme) -> AnyElement {
        let facts = &self.open_list_facts;
        let emit = |surface: OpenSurface| {
            let entity = entity.clone();
            move |_: &mut Window, cx: &mut App| {
                entity.update(cx, |_, cx| cx.emit(RightPanelActionEvent::OpenSurface(surface)));
            }
        };
        let mut list = div()
            .id("right-panel-open-list")
            .debug_selector(|| "right-panel-open-list".to_owned())
            .flex_1()
            .min_h(px(0.0))
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .py(px(6.0))
            .child(
                div()
                    .px(px(ROW_INSET))
                    .pb(px(4.0))
                    .text_size(theme.typography.caption2)
                    .text_color(theme.ely.fg_subtle)
                    .child("OPEN IN RIGHT PANE"),
            )
            .child(row(
                "open-list-terminal".into(),
                Some(Icon::SquareTerminal),
                "Terminal".into(),
                0.0,
                None,
                None,
                theme,
                emit(OpenSurface::Terminal),
            ));
        let chat_entity = entity.clone();
        list = list.child(row(
            "open-list-chat".into(),
            Some(Icon::MessageSquare),
            "Chat".into(),
            0.0,
            None,
            Some(if self.open_list_chat_expanded {
                Icon::ChevronDown
            } else {
                Icon::ChevronRight
            }),
            theme,
            move |_, cx| chat_entity.update(cx, |panel, cx| panel.toggle_open_list_chat(cx)),
        ));
        if self.open_list_chat_expanded {
            for (id, name) in &facts.agents {
                list = list.child(row(
                    format!("open-list-agent-{id}").into(),
                    Some(Icon::for_agent_id(id).unwrap_or(Icon::MessageSquare)),
                    name.clone(),
                    AGENT_INDENT,
                    None,
                    None,
                    theme,
                    emit(OpenSurface::Chat(*id)),
                ));
            }
            list = list.child(row(
                "open-list-other-agents".into(),
                None,
                "Other agents…".into(),
                AGENT_INDENT,
                None,
                None,
                theme,
                emit(OpenSurface::OtherAgents),
            ));
            if facts.agents.is_empty() {
                list = list.child(
                    div()
                        .id("open-list-no-agents")
                        .debug_selector(|| "open-list-no-agents".to_owned())
                        .pl(px(ROW_INSET + AGENT_INDENT))
                        .pb(px(4.0))
                        .text_size(theme.typography.caption2)
                        .text_color(theme.ely.fg_subtle)
                        .child("No supported agent found on PATH"),
                );
            }
        }
        for (id, icon, label, disabled, surface) in [
            ("open-list-browser", Icon::Browser, "Browser", None, OpenSurface::Browser),
            (
                "open-list-changes",
                Icon::Diff,
                "Changes",
                facts.changes_disabled.clone(),
                OpenSurface::Changes,
            ),
            ("open-list-open-file", Icon::File, "Open File", None, OpenSurface::OpenFile),
            (
                "open-list-project-settings",
                Icon::Settings,
                "Project Settings",
                facts.project_settings_disabled.clone(),
                OpenSurface::ProjectSettings,
            ),
        ] {
            list = list.child(row(
                id.into(),
                Some(icon),
                label.into(),
                0.0,
                disabled,
                None,
                theme,
                emit(surface),
            ));
        }
        list.into_any_element()
    }
}

/// One full-width row. `disabled` dims it, drops its click and explains why
/// in a tooltip, as the Secondary launcher's tiles did.
#[allow(clippy::too_many_arguments)]
fn row(
    id: SharedString,
    icon: Option<Icon>,
    label: SharedString,
    indent: f32,
    disabled: Option<SharedString>,
    trailing: Option<Icon>,
    theme: Theme,
    on_click: impl Fn(&mut Window, &mut App) + 'static,
) -> AnyElement {
    let selector = id.to_string();
    let element = div()
        .id(id)
        .debug_selector(move || selector.clone())
        .h(px(ROW_HEIGHT))
        .w_full()
        .flex_none()
        .pl(px(ROW_INSET + indent))
        .pr(px(ROW_INSET))
        .flex()
        .items_center()
        .gap(theme.spacing.titlebar_control_spacing)
        .text_size(theme.typography.scaled(13.5))
        .text_color(theme.ely.fg)
        .when_some(icon, |this, icon| {
            this.child(IconElement::new(icon, IconSize::Small).text_color(theme.ely.fg_muted))
        })
        .child(div().flex_1().min_w_0().truncate().child(label))
        .when_some(trailing, |this, icon| {
            this.child(IconElement::new(icon, IconSize::Small).text_color(theme.ely.fg_subtle))
        });
    match disabled {
        Some(reason) => element
            .opacity(0.5)
            .tooltip(crate::controls::tooltip(reason))
            .into_any_element(),
        None => element
            .cursor_pointer()
            .hover(|style| style.bg(theme.ely.hover))
            .on_click(move |_, window, cx| on_click(window, cx))
            .into_any_element(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::right_panel::{PanelView, RightPanel, RightPanelActionEvent};
    use gpui::{Entity, Modifiers, TestAppContext, VisualTestContext};
    use std::cell::RefCell;
    use std::rc::Rc;

    fn facts(agents: &[(&'static str, &'static str)], git: bool) -> OpenListFacts {
        OpenListFacts {
            agents: agents
                .iter()
                .map(|(id, name)| (*id, SharedString::from(*name)))
                .collect(),
            changes_disabled: (!git).then(|| "This worktree is not a git repository".into()),
            project_settings_disabled: None,
        }
    }

    /// A panel showing the Open view, with every `OpenSurface` it emits
    /// recorded in order.
    fn open_list_window(
        cx: &mut TestAppContext,
        facts: OpenListFacts,
    ) -> (VisualTestContext, Entity<RightPanel>, Rc<RefCell<Vec<OpenSurface>>>) {
        cx.update(sirio_theme::Theme::init);
        cx.update(|cx| PanelView::set(PanelView::Open, cx));
        let window = cx.add_window(|_window, _cx| RightPanel::new(std::env::temp_dir()));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        let panel =
            cx.update(|window, _cx| window.root::<RightPanel>().flatten().expect("panel root"));
        let emitted = Rc::new(RefCell::new(Vec::new()));
        let seen = emitted.clone();
        cx.update(|_window, cx| {
            cx.subscribe(&panel, move |_, event: &RightPanelActionEvent, _| {
                if let RightPanelActionEvent::OpenSurface(surface) = event {
                    seen.borrow_mut().push(*surface);
                }
            })
            .detach();
        });
        panel.update(&mut cx, |panel, cx| panel.set_open_list_facts(facts, cx));
        cx.run_until_parked();
        (cx, panel, emitted)
    }

    fn click(cx: &mut VisualTestContext, id: &'static str) {
        let bounds = cx
            .debug_bounds(id)
            .unwrap_or_else(|| panic!("{id} is drawn"));
        cx.simulate_click(bounds.center(), Modifiers::none());
        cx.run_until_parked();
    }

    // (a) every row is drawn, under its own id, and the rail has its button.
    #[gpui::test]
    fn every_row_of_the_open_list_is_drawn(cx: &mut TestAppContext) {
        let (mut cx, _, _) = open_list_window(cx, facts(&[("codex", "Codex")], true));
        assert!(cx.debug_bounds("right-panel-tab-open").is_some(), "the rail button");
        assert!(cx.debug_bounds("right-panel-open-list").is_some(), "the list");
        for id in [
            "open-list-terminal",
            "open-list-chat",
            "open-list-browser",
            "open-list-changes",
            "open-list-open-file",
            "open-list-project-settings",
        ] {
            let row = cx.debug_bounds(id).unwrap_or_else(|| panic!("{id} is drawn"));
            assert!(f32::from(row.size.height) > 0.0, "{id} has real height");
        }
        assert!(
            cx.debug_bounds("open-list-agent-codex").is_none(),
            "Chat starts collapsed"
        );
    }

    // (b) an enabled row reports its surface; (c) a disabled one reports nothing.
    #[gpui::test]
    fn only_enabled_rows_report_a_click(cx: &mut TestAppContext) {
        let (mut cx, _, emitted) = open_list_window(cx, facts(&[], false));
        click(&mut cx, "open-list-changes");
        click(&mut cx, "open-list-terminal");
        click(&mut cx, "open-list-project-settings");
        assert_eq!(
            *emitted.borrow(),
            vec![OpenSurface::Terminal, OpenSurface::ProjectSettings],
            "the disabled Changes row reports nothing"
        );
    }

    // (d) Chat expands inline, and an agent row reports that agent.
    #[gpui::test]
    fn chat_expands_inline_and_an_agent_row_opens_that_agent(cx: &mut TestAppContext) {
        let (mut cx, panel, emitted) =
            open_list_window(cx, facts(&[("claude", "Claude Code"), ("codex", "Codex")], true));
        click(&mut cx, "open-list-chat");
        assert!(panel.read_with(&cx, |panel, _| panel.open_list_chat_expanded()));
        assert!(cx.debug_bounds("open-list-agent-claude").is_some());
        assert!(cx.debug_bounds("open-list-other-agents").is_some());
        assert!(cx.debug_bounds("open-list-no-agents").is_none());
        click(&mut cx, "open-list-agent-codex");
        assert_eq!(*emitted.borrow(), vec![OpenSurface::Chat("codex")]);
        click(&mut cx, "open-list-chat");
        assert!(
            cx.debug_bounds("open-list-agent-claude").is_none(),
            "a second click collapses it"
        );
    }

    // (e) with no agent, the expansion still offers the way to install one.
    #[gpui::test]
    fn chat_with_no_agent_offers_other_agents_and_says_why(cx: &mut TestAppContext) {
        let (mut cx, _, emitted) = open_list_window(cx, facts(&[], true));
        click(&mut cx, "open-list-chat");
        assert!(cx.debug_bounds("open-list-no-agents").is_some());
        click(&mut cx, "open-list-other-agents");
        assert_eq!(*emitted.borrow(), vec![OpenSurface::OtherAgents]);
    }

    // (f) Review Focus 4: the host pushes facts every render; equal facts
    // must not notify, or the panel redraws forever.
    #[gpui::test]
    fn pushing_equal_facts_does_not_notify(cx: &mut TestAppContext) {
        let pushed = facts(&[("codex", "Codex")], true);
        let (mut cx, panel, _) = open_list_window(cx, pushed.clone());
        let notified = Rc::new(RefCell::new(0usize));
        let count = notified.clone();
        cx.update(|_window, cx| {
            cx.observe(&panel, move |_, _| *count.borrow_mut() += 1).detach();
        });
        panel.update(&mut cx, |panel, cx| panel.set_open_list_facts(pushed.clone(), cx));
        cx.run_until_parked();
        assert_eq!(*notified.borrow(), 0, "equal facts are a no-op");
        panel.update(&mut cx, |panel, cx| {
            panel.set_open_list_facts(facts(&[], true), cx)
        });
        cx.run_until_parked();
        assert!(*notified.borrow() > 0, "changed facts redraw");
    }
}
