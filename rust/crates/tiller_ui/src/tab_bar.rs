//! The centre-pane tab strip and its new-tab menu.

use gpui::{
    Anchor, App, Bounds, Context, FocusHandle, KeyBinding, Pixels, Render, Window, actions,
    anchored, canvas, deferred, div, point, prelude::*, px, text,
};
use std::cell::Cell;
use std::rc::Rc;
use tiller_agents::{AgentAvailability, discover_availability};
use tiller_theme::Theme;

use crate::sidebar::icons::{Icon, IconElement, IconSize};

const HEIGHT: f32 = 34.0;

actions!(new_tab_menu, [DismissMenu]);

/// Actions emitted by the new-tab menu. The owner decides how to handle them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NewTabAction {
    NewTerminal,
    NewChanges,
    ClaudeCode,
    Codex,
    OpenCode,
    Pi,
    OhMyPi,
    SplitClaudeCode,
    NewBrowser,
    NewChat,
}

/// Commands exposed by a tab's context menu. The shell owns the transitions;
/// this crate owns their drawn, typed surface.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TabContextAction {
    Dismiss,
    OpenFile,
    ResumeChat,
    Rename,
    Close,
    CloseOthers,
    CloseTabsToRight,
    MoveEarlier,
    MoveLater,
    // F-TAB-12: there used to be a `MoveToCurrentPane` variant here, backing
    // a tab-context-menu "Move to This Pane" item that was built
    // unconditionally `disabled(...)` in `tiller`'s `tab_context_items()`.
    // It could never be enabled: that menu only ever opens on a tab that
    // already belongs to the workspace's active pane group, so "move it to
    // this (its own) pane" had no reachable non-trivial destination. See
    // the removal comment at that call site for the fuller rationale.
    MoveToPane(usize),
    AttachToCurrentTerminal,
}

/// One row in the shell-owned tab context menu.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TabContextItem {
    label: String,
    selector: String,
    action: TabContextAction,
    enabled: bool,
    disabled_reason: Option<String>,
    separator_before: bool,
}

impl TabContextItem {
    pub fn enabled(
        label: impl Into<String>,
        selector: impl Into<String>,
        action: TabContextAction,
    ) -> Self {
        Self {
            label: label.into(),
            selector: selector.into(),
            action,
            enabled: true,
            disabled_reason: None,
            separator_before: false,
        }
    }

    pub fn disabled(
        label: impl Into<String>,
        selector: impl Into<String>,
        action: TabContextAction,
        reason: impl Into<String>,
    ) -> Self {
        Self {
            label: label.into(),
            selector: selector.into(),
            action,
            enabled: false,
            disabled_reason: Some(reason.into()),
            separator_before: false,
        }
    }

    pub fn separator() -> Self {
        Self {
            label: String::new(),
            selector: String::new(),
            action: TabContextAction::Dismiss,
            enabled: false,
            disabled_reason: None,
            separator_before: true,
        }
    }
}

/// Draws a tab context menu and emits only actions for enabled rows.
pub fn render_tab_context_menu(
    items: Vec<TabContextItem>,
    on_action: Rc<dyn Fn(TabContextAction, &mut Window, &mut App)>,
    theme: Theme,
) -> impl IntoElement {
    let mut menu = div()
        .id("tab-context-menu")
        .debug_selector(|| "tab-context-menu".to_owned())
        .w(theme.spacing.menu_width)
        .p(theme.spacing.titlebar_control_spacing)
        .rounded(theme.radii.user_pill)
        .border_1()
        .border_color(theme.hairline)
        .bg(theme.card_fill)
        .shadow_lg();

    for item in items {
        if item.separator_before {
            menu = menu.child(
                div()
                    .mx(theme.spacing.card_gap)
                    .my(theme.spacing.titlebar_control_spacing)
                    .h(theme.spacing.hairline_thickness)
                    .bg(theme.hairline),
            );
            continue;
        }

        let action = item.action;
        let enabled = item.enabled;
        let selector = item.selector.clone();
        let selector_for_debug = selector.clone();
        let mut row = div()
            .id(format!("tab-command-{selector}"))
            .debug_selector(move || format!("tab-command-{selector_for_debug}"))
            .w_full()
            .min_h(theme.typography.ui_line_height)
            .px(theme.spacing.card_gap)
            .py(theme.spacing.titlebar_control_spacing)
            .flex()
            .items_center()
            .justify_between()
            .gap(theme.spacing.titlebar_control_spacing)
            .rounded(theme.radii.control)
            .text_size(theme.typography.footnote)
            .text_color(if enabled { theme.title } else { theme.meta })
            .when(enabled, |this| {
                this.hover(|style| style.bg(theme.row_hover))
            })
            .child(item.label);

        if let Some(reason) = item.disabled_reason {
            row = row.child(
                div()
                    .id(format!("tab-command-disabled-{selector}"))
                    .debug_selector(move || format!("tab-command-disabled-{selector}"))
                    .text_size(theme.typography.caption2)
                    .text_color(theme.meta)
                    .child(reason),
            );
        }
        if enabled {
            let callback = on_action.clone();
            row = row.on_click(move |_, window, cx| callback(action, window, cx));
        }
        menu = menu.child(row);
    }

    menu
}

/// A horizontal tab strip with a callback-driven new-tab menu.
pub struct TabBar {
    menu_open: bool,
    chat_picker_open: bool,
    focus_handle: FocusHandle,
    anchor_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    on_new_tab: Option<Rc<dyn Fn(NewTabAction)>>,
    on_chat_agent: Option<Rc<dyn Fn(&'static str)>>,
    /// F-TAB-08: the empty-state "Other agents…" card has nowhere else to
    /// send a click — it is not a chat-agent row, so `on_chat_agent`
    /// doesn't fit. A separate channel mirrors `on_new_tab`/`on_chat_agent`'s
    /// own callback pattern rather than growing either of their payloads.
    on_open_agent_settings: Option<Rc<dyn Fn()>>,
    chat_agents: Vec<AgentAvailability>,
}

impl TabBar {
    /// Installs the menu-dismissal keymap in the host application. Mirrors
    /// `Chat::bind_keys`: a namespaced `actions!` type bound through GPUI's
    /// action-dispatch system (`zed-ref/crates/ui/context_menu.rs`'s own
    /// `menu::Cancel` pattern), not a hand-rolled raw-key string match — the
    /// hand-rolled version was the bug: Escape fell through past it and hit
    /// the first menu row's own click handling instead of dismissing.
    fn bind_keys(cx: &mut App) {
        // Bind per App. A process-global Once would leave every independent
        // GPUI TestAppContext after the first one without this keymap.
        cx.bind_keys([KeyBinding::new("escape", DismissMenu, Some("NewTabMenu"))]);
    }

    fn dismiss_menu(&mut self, _: &DismissMenu, _: &mut Window, cx: &mut Context<Self>) {
        self.close_menu(cx);
    }

    /// Creates the menu-only tab-bar host. The shell owns the open tabs.
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self::bind_keys(cx);
        Self {
            menu_open: false,
            chat_picker_open: false,
            focus_handle: cx.focus_handle(),
            anchor_bounds: Rc::new(Cell::new(None)),
            on_new_tab: None,
            on_chat_agent: None,
            on_open_agent_settings: None,
            chat_agents: discover_availability(),
        }
    }

    /// Overrides discovery for deterministic callers and headless tests. The
    /// render path still applies the same availability gate as production.
    pub fn with_chat_agents(mut self, agents: Vec<AgentAvailability>) -> Self {
        self.chat_agents = agents;
        self
    }

    /// Installs the callback used by every menu action.
    pub fn on_new_tab(mut self, callback: impl Fn(NewTabAction) + 'static) -> Self {
        self.on_new_tab = Some(Rc::new(callback));
        self
    }

    /// Installs the callback used by a selected ACP chat provider. It is a
    /// separate channel so sidebar actions can keep their stable NewChat
    /// enum without growing a cross-surface variant.
    pub fn on_chat_agent(mut self, callback: impl Fn(&'static str) + 'static) -> Self {
        self.on_chat_agent = Some(Rc::new(callback));
        self
    }

    /// F-TAB-08: installs the callback the empty-state "Other agents…" card
    /// fires when clicked, so the New Chat menu can hand the user straight
    /// to the Settings screen's Agents section instead of leaving them with
    /// a dead-end static label.
    pub fn on_open_agent_settings(mut self, callback: impl Fn() + 'static) -> Self {
        self.on_open_agent_settings = Some(Rc::new(callback));
        self
    }

    fn toggle_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.menu_open = !self.menu_open;
        self.chat_picker_open = false;
        if self.menu_open {
            // The menu is painted through `deferred(...)`, which links its
            // subtree into the window's dispatch tree only after its own
            // (later) paint pass runs — see the identical comment on
            // `show_menu` in `zed-ref/crates/ui/src/components/popover_menu.rs`.
            // Focusing synchronously here targets a handle that dispatch
            // doesn't consider linked yet, so `on_action` bindings scoped to
            // it never match. Waiting two frames, exactly as that reference
            // does, focuses it only once the deferred pass has actually run.
            let focus_handle = self.focus_handle.clone();
            window.on_next_frame(move |window, _cx| {
                window.on_next_frame(move |window, cx| {
                    window.focus(&focus_handle, cx);
                });
            });
        }
        cx.notify();
    }

    fn close_menu(&mut self, cx: &mut Context<Self>) {
        if self.menu_open || self.chat_picker_open {
            self.menu_open = false;
            self.chat_picker_open = false;
            cx.notify();
        }
    }

    fn emit(&mut self, action: NewTabAction, cx: &mut Context<Self>) {
        self.menu_open = false;
        self.chat_picker_open = false;
        if let Some(callback) = &self.on_new_tab {
            callback(action);
        }
        cx.notify();
    }

    fn emit_chat_agent(&mut self, id: &'static str, cx: &mut Context<Self>) {
        self.menu_open = false;
        self.chat_picker_open = false;
        if let Some(callback) = &self.on_chat_agent {
            callback(id);
        }
        cx.notify();
    }

    fn emit_open_agent_settings(&mut self, cx: &mut Context<Self>) {
        self.menu_open = false;
        self.chat_picker_open = false;
        if let Some(callback) = &self.on_open_agent_settings {
            callback();
        }
        cx.notify();
    }

    fn toggle_chat_picker(&mut self, cx: &mut Context<Self>) {
        self.chat_picker_open = !self.chat_picker_open;
        cx.notify();
    }

    fn render_menu_item(
        label: &'static str,
        action: NewTabAction,
        entity: gpui::Entity<Self>,
        theme: Theme,
        chevron: bool,
    ) -> impl IntoElement {
        let (icon, glyph_color) = match label {
            "New Terminal" => (Icon::SquareTerminal, theme.meta),
            "Changes" => (Icon::File, theme.meta),
            // Agent marks wear their published brand colour, not a theme
            // token: Claude its orange, the monochrome trio the foreground
            // they are authored in. omp's tint is ignored by its
            // full-colour path.
            "Claude Code" | "Split Claude Code" => (
                Icon::ClaudeCode,
                Icon::ClaudeCode
                    .agent_mark_color(theme.title)
                    .unwrap_or(theme.title),
            ),
            "Codex" => (Icon::Codex, theme.title),
            "OpenCode" => (Icon::OpenCode, theme.title),
            "Pi" => (Icon::Pi, theme.title),
            "Oh-My-Pi" => (Icon::OhMyPi, theme.title),
            "New Browser" => (Icon::Globe, theme.meta),
            "New Chat" => (Icon::MessageSquare, theme.meta),
            _ => (Icon::File, theme.meta),
        };

        div()
            .id(label)
            .debug_selector(move || format!("new-tab-item-{}", menu_selector(label)))
            .w_full()
            .h(px(29.0))
            .px(px(12.0))
            .flex()
            .items_center()
            .justify_between()
            .rounded(theme.radii.control)
            .text_size(theme.typography.footnote)
            .text_color(theme.title)
            .hover(|style| style.bg(theme.row_hover))
            .on_click(move |_, _, cx| entity.update(cx, |this, cx| this.emit(action, cx)))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(7.0))
                    .child(
                        div()
                            .w(px(14.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(IconElement::new(icon, IconSize::Small).text_color(glyph_color)),
                    )
                    .child(text!(id = format!("new-tab-label-{label}"), label)),
            )
            .when(chevron, |this| {
                this.child(div().text_color(theme.meta).child(
                    IconElement::new(Icon::ChevronRight, IconSize::XSmall).text_color(theme.meta),
                ))
            })
    }

    fn separator(theme: Theme) -> impl IntoElement {
        div()
            .mx(px(8.0))
            .h(theme.spacing.hairline_thickness)
            .bg(theme.hairline)
    }

    fn render_new_chat_item(
        entity: gpui::Entity<Self>,
        theme: Theme,
        expanded: bool,
    ) -> impl IntoElement {
        div()
            .id("New Chat")
            .debug_selector(|| "new-tab-item-new-chat".to_owned())
            .w_full()
            .h(px(29.0))
            .px(px(12.0))
            .flex()
            .items_center()
            .justify_between()
            .rounded(theme.radii.control)
            .text_size(theme.typography.footnote)
            .text_color(theme.title)
            .hover(|style| style.bg(theme.row_hover))
            .on_click(move |_, _, cx| entity.update(cx, |this, cx| this.toggle_chat_picker(cx)))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(7.0))
                    .child(
                        div()
                            .w(px(14.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(
                                IconElement::new(Icon::MessageSquare, IconSize::Small)
                                    .text_color(theme.title),
                            ),
                    )
                    .child(text!(id = "new-tab-label-New Chat", "New Chat")),
            )
            .child(div().text_color(theme.meta).child(IconElement::new(
                if expanded {
                    Icon::ChevronDown
                } else {
                    Icon::ChevronRight
                },
                IconSize::XSmall,
            )))
    }

    fn render_chat_agent_item(
        agent: &AgentAvailability,
        entity: gpui::Entity<Self>,
        theme: Theme,
    ) -> impl IntoElement {
        let id = agent.id;
        let selector = format!("new-tab-chat-agent-{id}");
        let selector_for_debug = selector.clone();
        let display_name = agent.display_name;
        let icon = Icon::for_agent_id(id).unwrap_or(Icon::MessageSquare);
        let mut mark = IconElement::new(icon, IconSize::Small);
        if let Some(tint) = icon.agent_mark_color(theme.title) {
            mark = mark.text_color(tint);
        }
        let mark_element = mark;
        div()
            .id(selector.clone())
            .debug_selector(move || selector_for_debug.clone())
            .w_full()
            .h(px(29.0))
            .pl(theme.spacing.card_gap)
            .pr(px(12.0))
            .flex()
            .items_center()
            .gap(px(7.0))
            .rounded(theme.radii.control)
            .text_size(theme.typography.footnote)
            .text_color(theme.title)
            .hover(|style| style.bg(theme.row_hover))
            .on_click(move |_, _, cx| entity.update(cx, |this, cx| this.emit_chat_agent(id, cx)))
            .child(mark_element)
            .child(text!(id = format!("new-tab-chat-label-{id}"), display_name))
    }

    /// F-TAB-08: this was a bare static label with no `entity`/`cx` capture
    /// and no `.on_click` -- it could not emit an event to open Settings
    /// even in principle. `entity` (already in scope at the render-time call
    /// site, mirroring `render_chat_agent_item`'s identical parameter) lets
    /// a click route through the same `on_open_agent_settings` callback
    /// channel every other TabBar action uses.
    fn render_chat_empty(entity: gpui::Entity<Self>, theme: Theme) -> impl IntoElement {
        div()
            .id("new-chat-empty")
            .debug_selector(|| "new-chat-empty".to_owned())
            .w_full()
            .px(theme.spacing.card_gap)
            .py(theme.spacing.titlebar_control_spacing)
            .flex()
            .flex_col()
            .gap(theme.spacing.titlebar_control_spacing)
            .text_size(theme.typography.footnote)
            .text_color(theme.meta)
            .hover(|style| style.bg(theme.row_hover))
            .on_click(move |_, _, cx| {
                entity.update(cx, |this, cx| this.emit_open_agent_settings(cx))
            })
            .child("Other agents…")
            .child(
                div()
                    .text_size(theme.typography.caption2)
                    .text_color(theme.meta)
                    .child("No supported agent found on PATH"),
            )
    }
}

impl Render for TabBar {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *Theme::get(cx);
        let entity = cx.entity();
        let menu_open = self.menu_open;
        let chat_picker_open = self.chat_picker_open;
        let anchor_bounds = self.anchor_bounds.clone();

        let available_chat_agents = self
            .chat_agents
            .iter()
            .filter(|agent| agent.is_available() && agent.acp_program().is_some())
            .collect::<Vec<_>>();
        let mut chat_agent_menu = div()
            .id("new-chat-agent-menu")
            .debug_selector(|| "new-chat-agent-menu".to_owned())
            .w_full()
            .mt(theme.spacing.titlebar_control_spacing)
            .pl(theme.spacing.titlebar_control_spacing)
            .flex()
            .flex_col()
            .gap(theme.spacing.titlebar_control_spacing);
        if available_chat_agents.is_empty() {
            chat_agent_menu = chat_agent_menu.child(Self::render_chat_empty(entity.clone(), theme));
        } else {
            for agent in available_chat_agents {
                chat_agent_menu = chat_agent_menu.child(Self::render_chat_agent_item(
                    agent,
                    entity.clone(),
                    theme,
                ));
            }
        }

        let menu = div()
            .id("new-tab-menu")
            .debug_selector(|| "new-tab-menu".to_owned())
            .w(px(170.0))
            .p(px(6.0))
            .rounded(theme.radii.user_pill)
            .border_1()
            .border_color(theme.hairline)
            .bg(theme.card_fill)
            .shadow_lg()
            .child(Self::render_menu_item(
                "New Terminal",
                NewTabAction::NewTerminal,
                entity.clone(),
                theme,
                false,
            ))
            .child(Self::render_menu_item(
                "Changes",
                NewTabAction::NewChanges,
                entity.clone(),
                theme,
                false,
            ))
            .child(Self::render_menu_item(
                "New Browser",
                NewTabAction::NewBrowser,
                entity.clone(),
                theme,
                false,
            ))
            .child(Self::separator(theme))
            .child(Self::render_menu_item(
                "Claude Code",
                NewTabAction::ClaudeCode,
                entity.clone(),
                theme,
                false,
            ))
            .child(Self::render_menu_item(
                "Codex",
                NewTabAction::Codex,
                entity.clone(),
                theme,
                false,
            ))
            .child(Self::render_menu_item(
                "OpenCode",
                NewTabAction::OpenCode,
                entity.clone(),
                theme,
                false,
            ))
            .child(Self::render_menu_item(
                "Pi",
                NewTabAction::Pi,
                entity.clone(),
                theme,
                false,
            ))
            .child(Self::render_menu_item(
                "Oh-My-Pi",
                NewTabAction::OhMyPi,
                entity.clone(),
                theme,
                false,
            ))
            .child(Self::separator(theme))
            .child(Self::render_menu_item(
                "Split Claude Code",
                NewTabAction::SplitClaudeCode,
                entity.clone(),
                theme,
                false,
            ))
            .child(Self::separator(theme))
            .child(Self::render_new_chat_item(entity, theme, chat_picker_open))
            .when(chat_picker_open, |this| this.child(chat_agent_menu));

        let menu = menu.on_mouse_down_out(cx.listener(|this, _, _, cx| this.close_menu(cx)));

        let mut new_tab_button = div()
            .id("new-tab-button")
            .debug_selector(|| "new-tab-button".to_owned())
            .w(px(24.0))
            .h(px(24.0))
            .mr(px(2.0))
            .flex()
            .items_center()
            .justify_center()
            .rounded(theme.radii.control)
            .text_size(px(14.0))
            .text_color(theme.meta)
            .hover(|style| style.bg(theme.row_hover))
            // Swallowing mouse-down here would stop GPUI ever pairing it
            // with the mouse-up into a click, so the menu never opened.
            // Stop propagation inside the click instead.
            .on_click(cx.listener(|this, _, window, cx| {
                cx.stop_propagation();
                this.toggle_menu(window, cx)
            }))
            .child(
                canvas(
                    // `render()` reads `anchor_bounds` synchronously, one full
                    // render pass BEFORE this prepaint callback can update it
                    // with the button's bounds for the frame being built now
                    // -- so the deferred menu below is always positioned from
                    // last frame's measurement. Invisible while the button's
                    // position never changes frame to frame, but a window
                    // resize while the menu is open does move it, and with
                    // nothing forcing a follow-up render the menu stayed
                    // pinned to the button's PRE-resize spot forever
                    // (PLUS-MENU-INVESTIGATION.md). `Window::refresh` is a
                    // no-op here -- prepaint runs mid-draw
                    // (`invalidator.not_drawing()` is false), exactly the
                    // guard that stops a redraw from re-triggering itself
                    // inside its own frame. Deferring through `on_next_frame`
                    // (the same "wait for the real next frame" primitive
                    // `toggle_menu` already uses below, for the same reason:
                    // deferred/anchored content settles one frame late) calls
                    // `refresh` from OUTSIDE any draw, where it actually
                    // marks the window dirty -- so the next frame renders
                    // with the corrected position instead of never catching
                    // up.
                    move |bounds, window, _| {
                        if anchor_bounds.get() != Some(bounds) {
                            anchor_bounds.set(Some(bounds));
                            window.on_next_frame(|window, _cx| window.refresh());
                        }
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full(),
            )
            .child(IconElement::new(Icon::Plus, IconSize::Small).text_color(theme.title));

        if menu_open {
            new_tab_button = new_tab_button.child(
                deferred(
                    anchored()
                        .anchor(Anchor::TopLeft)
                        .position(
                            self.anchor_bounds
                                .get()
                                .map(|bounds| bounds.corner(Anchor::BottomLeft))
                                .unwrap_or_default(),
                        )
                        .offset(point(px(6.0), px(3.0)))
                        .snap_to_window_with_margin(px(8.0))
                        .child(menu),
                )
                .priority(1),
            );
        }

        div()
            .key_context("NewTabMenu")
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::dismiss_menu))
            .relative()
            .w_full()
            .h(px(HEIGHT))
            .flex()
            .items_center()
            .bg(theme.background)
            .child(
                div()
                    .flex_1()
                    .h_full()
                    .flex()
                    .items_start()
                    .gap(px(1.0))
                    .pl(px(5.0)),
            )
            .child(new_tab_button)
    }
}

fn menu_selector(label: &str) -> &'static str {
    match label {
        "New Terminal" => "new-terminal",
        "Changes" => "changes",
        "Claude Code" => "claude-code",
        "Codex" => "codex",
        "OpenCode" => "opencode",
        "Pi" => "pi",
        "Oh-My-Pi" => "oh-my-pi",
        "Split Claude Code" => "split-claude-code",
        "New Browser" => "new-browser",
        "New Chat" => "new-chat",
        _ => "unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{Modifiers, TestAppContext, VisualTestContext};
    use std::cell::RefCell;
    use std::path::PathBuf;
    use std::rc::Rc;
    use std::sync::Once;

    fn available_agent(id: &'static str, display_name: &'static str) -> AgentAvailability {
        AgentAvailability {
            id,
            display_name,
            executable: Some(PathBuf::from(format!("/usr/bin/{id}"))),
        }
    }

    #[gpui::test]
    async fn drawn_new_tab_menu_dispatches_every_item_action(cx: &mut TestAppContext) {
        cx.update(Theme::init);
        let actions = Rc::new(RefCell::new(Vec::new()));
        let collected = actions.clone();
        let window = cx.add_window(|_window, cx| {
            TabBar::new(cx)
                .with_chat_agents(vec![available_agent("codex", "Codex")])
                .on_new_tab(move |action| collected.borrow_mut().push(action))
        });
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        let entries = [
            ("New Terminal", "new-terminal", NewTabAction::NewTerminal),
            ("Changes", "changes", NewTabAction::NewChanges),
            ("Claude Code", "claude-code", NewTabAction::ClaudeCode),
            ("Codex", "codex", NewTabAction::Codex),
            ("OpenCode", "opencode", NewTabAction::OpenCode),
            ("Pi", "pi", NewTabAction::Pi),
            ("Oh-My-Pi", "oh-my-pi", NewTabAction::OhMyPi),
            (
                "Split Claude Code",
                "split-claude-code",
                NewTabAction::SplitClaudeCode,
            ),
            ("New Browser", "new-browser", NewTabAction::NewBrowser),
        ];

        for (_label, selector, expected) in entries {
            let button = cx
                .debug_bounds("new-tab-button")
                .expect("the plus control is in the drawn frame");
            cx.simulate_click(button.center(), Modifiers::none());
            cx.run_until_parked();
            cx.update(|window, cx| {
                // The menu is deferred and receives focus after two frames.
                window.simulate_next_frame(cx);
                window.simulate_next_frame(cx);
            });
            cx.run_until_parked();

            assert!(
                cx.debug_bounds("new-tab-menu").is_some(),
                "clicking + draws the menu"
            );
            let item_selector = match selector {
                "new-terminal" => "new-tab-item-new-terminal",
                "changes" => "new-tab-item-changes",
                "claude-code" => "new-tab-item-claude-code",
                "codex" => "new-tab-item-codex",
                "opencode" => "new-tab-item-opencode",
                "pi" => "new-tab-item-pi",
                "oh-my-pi" => "new-tab-item-oh-my-pi",
                "split-claude-code" => "new-tab-item-split-claude-code",
                "new-browser" => "new-tab-item-new-browser",
                "new-chat" => "new-tab-item-new-chat",
                _ => unreachable!("test selector is covered above"),
            };
            let item = cx
                .debug_bounds(item_selector)
                .expect("the menu item is laid out in the drawn frame");
            cx.simulate_click(item.center(), Modifiers::none());
            cx.run_until_parked();
            assert!(
                cx.debug_bounds("new-tab-menu").is_none(),
                "selecting an item closes the menu"
            );
            assert_eq!(actions.borrow().last().copied(), Some(expected));
        }
    }

    #[gpui::test]
    async fn drawn_new_tab_menu_offers_browser_action(cx: &mut TestAppContext) {
        cx.update(Theme::init);
        let window = cx.add_window(|_window, cx| TabBar::new(cx));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        let plus = cx
            .debug_bounds("new-tab-button")
            .expect("the plus control is in the drawn frame");
        cx.simulate_click(plus.center(), Modifiers::none());
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.simulate_next_frame(cx);
            window.simulate_next_frame(cx);
        });
        cx.run_until_parked();

        assert!(cx.debug_bounds("new-tab-menu").is_some());
        assert!(
            cx.debug_bounds("new-tab-item-new-browser").is_some(),
            "the menu offers the mounted browser surface"
        );
    }

    /// A bare `TabBar` fills the whole test window, so its "+" button always
    /// sits flush against the window's true right edge and the 170px menu
    /// always needs `snap_to_window_with_margin` to pull it back onscreen --
    /// which saturates to the same clamped position regardless of whether
    /// the button moved, masking the staleness this test exists to catch.
    /// The real app never hits that: a Files panel sits to the right of the
    /// tab strip (`RIGHT_PANEL_WIDTH` in `tiller/src/main.rs`), so the menu
    /// has real room to its right. This host reproduces that gutter.
    struct ResizeHost {
        tab_bar: gpui::Entity<TabBar>,
    }

    impl Render for ResizeHost {
        fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
            div()
                .flex()
                .w_full()
                .h_full()
                .child(div().flex_1().child(self.tab_bar.clone()))
                .child(div().id("files-gutter").w(px(400.0)).h_full())
        }
    }

    /// PLUS-MENU-INVESTIGATION.md: `toggle_menu`'s anchored menu positions
    /// itself from `anchor_bounds`, a `Cell` the "+" button's own `canvas`
    /// only updates during ITS prepaint -- one render pass after the value
    /// is read to build the menu's `anchored().position(...)`. That lag is
    /// invisible while the button never moves, but a window resize while the
    /// menu is open moves it, and nothing forced a follow-up render to pick
    /// up the corrected bounds: the menu stayed pinned to the button's
    /// PRE-resize spot forever. Reproduced live under Wayland (down on a
    /// menu item, a window resize landed in between, up missed every row) --
    /// see the investigation doc for the screenshots. This is the harness's
    /// `shot()` doing that resize as a side effect, but the underlying
    /// staleness is real: any resize while the menu is open reproduces it.
    #[gpui::test]
    async fn drawn_new_tab_menu_tracks_the_button_after_a_window_resize(cx: &mut TestAppContext) {
        cx.update(Theme::init);
        let tab_bar_cell: Rc<RefCell<Option<gpui::Entity<TabBar>>>> = Rc::new(RefCell::new(None));
        let tab_bar_cell_for_window = tab_bar_cell.clone();
        let window = cx.add_window(move |_window, cx| {
            let tab_bar = cx.new(TabBar::new);
            *tab_bar_cell_for_window.borrow_mut() = Some(tab_bar.clone());
            ResizeHost { tab_bar }
        });
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();
        let tab_bar = tab_bar_cell
            .borrow_mut()
            .take()
            .expect("the window's build closure captured the tab bar entity");

        let plus = cx
            .debug_bounds("new-tab-button")
            .expect("the plus control is in the drawn frame");
        cx.simulate_click(plus.center(), Modifiers::none());
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.simulate_next_frame(cx);
            window.simulate_next_frame(cx);
        });
        cx.run_until_parked();
        assert!(
            cx.debug_bounds("new-tab-menu").is_some(),
            "the menu opens before the resize, clear of the window's right edge"
        );

        // A real window resize while the menu stays open.
        cx.simulate_resize(gpui::size(px(1400.0), px(700.0)));
        cx.run_until_parked();

        let plus_mid_resize = cx
            .debug_bounds("new-tab-button")
            .expect("the plus control is still drawn after the resize");
        assert_ne!(
            plus_mid_resize.origin.x, plus.origin.x,
            "the resize must actually have moved the button, or this test proves nothing"
        );

        // Tests have no platform frame loop (see `Window::simulate_next_frame`'s
        // own doc comment): the fix schedules its correction through
        // `on_next_frame`, exactly like `toggle_menu`'s focus dance above,
        // so it needs the one manual pump a real compositor's own next frame
        // callback would already have delivered by itself. Without the fix,
        // nothing is scheduled there and this changes nothing.
        cx.update(|window, cx| {
            window.simulate_next_frame(cx);
        });
        cx.run_until_parked();

        let plus_after = cx
            .debug_bounds("new-tab-button")
            .expect("the plus control is still drawn");
        let menu_after = cx
            .debug_bounds("new-tab-menu")
            .expect("the menu is still drawn");
        let anchor_after = tab_bar.read_with(&cx.cx, |tab_bar, _| tab_bar.anchor_bounds.get());
        assert_eq!(
            anchor_after,
            Some(plus_after),
            "the canvas-measured anchor itself must have caught up with the button's true \
             post-resize bounds by now"
        );
        assert_eq!(
            menu_after.origin.x,
            plus_after.origin.x + px(6.0),
            "the open menu must self-heal onto the button's post-resize position within one \
             more delivered frame, not stay pinned to where the button was before the resize \
             forever: button={plus_after:?} menu={menu_after:?}"
        );
    }

    #[gpui::test]
    async fn drawn_new_chat_picker_gates_unavailable_agents_and_emits_the_selected_id(
        cx: &mut TestAppContext,
    ) {
        cx.update(Theme::init);
        let actions = Rc::new(RefCell::new(Vec::new()));
        let collected = actions.clone();
        let selected_agents = Rc::new(RefCell::new(Vec::new()));
        let selected_agents_for_callback = selected_agents.clone();
        let window = cx.add_window(|_window, cx| {
            TabBar::new(cx)
                .with_chat_agents(vec![
                    available_agent("codex", "Codex"),
                    available_agent("pi", "Pi"),
                    AgentAvailability {
                        id: "claude",
                        display_name: "Claude Code",
                        executable: None,
                    },
                ])
                .on_new_tab(move |action| collected.borrow_mut().push(action))
                .on_chat_agent(move |id| selected_agents_for_callback.borrow_mut().push(id))
        });
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        let plus = cx.debug_bounds("new-tab-button").expect("plus is drawn");
        cx.simulate_click(plus.center(), Modifiers::none());
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.simulate_next_frame(cx);
            window.simulate_next_frame(cx);
        });
        cx.run_until_parked();

        let new_chat = cx
            .debug_bounds("new-tab-item-new-chat")
            .expect("New Chat is drawn");
        cx.simulate_click(new_chat.center(), Modifiers::none());
        cx.run_until_parked();

        assert!(cx.debug_bounds("new-chat-agent-menu").is_some());
        assert!(cx.debug_bounds("new-tab-chat-agent-codex").is_some());
        assert!(
            cx.debug_bounds("new-tab-chat-agent-pi").is_none(),
            "an installed adapter without an ACP server must not be offered"
        );
        assert!(
            cx.debug_bounds("new-tab-chat-agent-claude").is_none(),
            "an unavailable adapter must not be offered"
        );

        let codex = cx
            .debug_bounds("new-tab-chat-agent-codex")
            .expect("the available adapter is selectable");
        cx.simulate_click(codex.center(), Modifiers::none());
        cx.run_until_parked();

        assert_eq!(
            selected_agents.borrow().as_slice(),
            &["codex"],
            "choosing a chat provider must carry its stable adapter id"
        );
    }

    #[gpui::test]
    async fn drawn_new_chat_picker_explains_when_no_agent_is_installed(cx: &mut TestAppContext) {
        cx.update(Theme::init);
        let window = cx.add_window(|_window, cx| {
            TabBar::new(cx).with_chat_agents(vec![AgentAvailability {
                id: "codex",
                display_name: "Codex",
                executable: None,
            }])
        });
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        let plus = cx.debug_bounds("new-tab-button").expect("plus is drawn");
        cx.simulate_click(plus.center(), Modifiers::none());
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.simulate_next_frame(cx);
            window.simulate_next_frame(cx);
        });
        cx.run_until_parked();
        let new_chat = cx
            .debug_bounds("new-tab-item-new-chat")
            .expect("New Chat is drawn");
        cx.simulate_click(new_chat.center(), Modifiers::none());
        cx.run_until_parked();

        assert!(cx.debug_bounds("new-chat-agent-menu").is_some());
        assert!(cx.debug_bounds("new-chat-empty").is_some());
        assert!(cx.debug_bounds("new-tab-chat-agent-codex").is_none());
    }

    /// F-TAB-08: clicking the "Other agents…" empty-state card, drawn when
    /// no supported agent is on PATH, must reach a real callback rather than
    /// being a dead static label.
    #[gpui::test]
    async fn clicking_the_no_agent_card_opens_agent_settings(cx: &mut TestAppContext) {
        cx.update(Theme::init);
        let opened = std::rc::Rc::new(std::cell::RefCell::new(false));
        let opened_for_callback = opened.clone();
        let window = cx.add_window(|_window, cx| {
            TabBar::new(cx)
                .with_chat_agents(vec![AgentAvailability {
                    id: "codex",
                    display_name: "Codex",
                    executable: None,
                }])
                .on_open_agent_settings(move || {
                    *opened_for_callback.borrow_mut() = true;
                })
        });
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        let plus = cx.debug_bounds("new-tab-button").expect("plus is drawn");
        cx.simulate_click(plus.center(), Modifiers::none());
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.simulate_next_frame(cx);
            window.simulate_next_frame(cx);
        });
        cx.run_until_parked();
        let new_chat = cx
            .debug_bounds("new-tab-item-new-chat")
            .expect("New Chat is drawn");
        cx.simulate_click(new_chat.center(), Modifiers::none());
        cx.run_until_parked();

        let empty_card = cx
            .debug_bounds("new-chat-empty")
            .expect("the no-agent card is drawn");
        assert!(
            !*opened.borrow(),
            "the callback has not fired before the click"
        );
        cx.simulate_click(empty_card.center(), Modifiers::none());
        cx.run_until_parked();

        assert!(
            *opened.borrow(),
            "clicking the no-agent card reaches on_open_agent_settings"
        );
    }

    #[gpui::test]
    async fn drawn_new_tab_menu_escape_dispatches_through_the_real_key_path(
        cx: &mut TestAppContext,
    ) {
        cx.update(Theme::init);
        let window = cx.add_window(|_window, cx| TabBar::new(cx));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();
        let button = cx
            .debug_bounds("new-tab-button")
            .expect("the plus control is in the drawn frame");
        cx.simulate_click(button.center(), Modifiers::none());
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.simulate_next_frame(cx);
            window.simulate_next_frame(cx);
        });
        assert!(cx.debug_bounds("new-tab-menu").is_some());
        cx.simulate_keystrokes("escape");
        cx.run_until_parked();
        assert!(
            cx.debug_bounds("new-tab-menu").is_none(),
            "Escape closes the drawn menu through GPUI action dispatch"
        );
    }

    struct TabContextMenuFixture {
        actions: Rc<RefCell<Vec<TabContextAction>>>,
        items: Vec<TabContextItem>,
    }

    impl Render for TabContextMenuFixture {
        fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
            let actions = self.actions.clone();
            render_tab_context_menu(
                self.items.clone(),
                Rc::new(move |action, _, _| actions.borrow_mut().push(action)),
                *Theme::get(_cx),
            )
        }
    }

    #[gpui::test]
    async fn drawn_tab_context_menu_dispatches_enabled_actions_and_explains_disabled_ones(
        cx: &mut TestAppContext,
    ) {
        cx.update(Theme::init);
        let actions = Rc::new(RefCell::new(Vec::new()));
        let window = cx.add_window(|_window, _cx| TabContextMenuFixture {
            actions: actions.clone(),
            items: vec![
                TabContextItem::enabled("Rename", "rename", TabContextAction::Rename),
                TabContextItem::separator(),
                TabContextItem::disabled(
                    "Close Tabs to the Right",
                    "close-right",
                    TabContextAction::CloseTabsToRight,
                    "already the last tab",
                ),
                TabContextItem::enabled("Close", "close", TabContextAction::Close),
            ],
        });
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        assert!(cx.debug_bounds("tab-context-menu").is_some());
        assert!(cx.debug_bounds("tab-command-close-right").is_some());
        assert!(
            cx.debug_bounds("tab-command-disabled-close-right")
                .is_some()
        );

        for (selector, expected) in [
            ("tab-command-rename", TabContextAction::Rename),
            ("tab-command-close", TabContextAction::Close),
        ] {
            let item = cx
                .debug_bounds(selector)
                .expect("the enabled tab command is drawn");
            cx.simulate_click(item.center(), Modifiers::none());
            cx.run_until_parked();
            assert_eq!(actions.borrow().last().copied(), Some(expected));
        }
    }

    // ── F-TAB-19/20/28 harness capability: modifier chords ──────────────
    //
    // The tab strip and its `ctrl-tab` / `ctrl-1…9` / `ctrl-w` bindings live
    // in the shell (`tiller/src/main.rs`, routed to codex12). What this crate
    // owns is the proof that the harness can send a *modifier chord* through
    // the real key-dispatch path: a key context, a scoped `KeyBinding`, and a
    // focused element with an `on_action` handler. The shell's chord tests
    // then use exactly this recipe against its workspace.

    actions!(
        chord_fixture,
        [CycleFixtureTab, JumpFixtureTab, CloseFixtureTab]
    );

    static CHORD_FIXTURE_KEYS_BOUND: Once = Once::new();

    struct ChordFixture {
        fired: Rc<RefCell<Vec<&'static str>>>,
        focus_handle: FocusHandle,
    }

    impl ChordFixture {
        /// Installs the same scoped-binding shape the shell uses for its
        /// workspace chords, but against this fixture's context so the
        /// dispatch path is the only thing under test.
        fn bind_keys(cx: &mut App) {
            CHORD_FIXTURE_KEYS_BOUND.call_once(|| {
                cx.bind_keys([
                    KeyBinding::new("ctrl-tab", CycleFixtureTab, Some("ChordFixture")),
                    KeyBinding::new("ctrl-1", JumpFixtureTab, Some("ChordFixture")),
                    KeyBinding::new("ctrl-w", CloseFixtureTab, Some("ChordFixture")),
                ]);
            });
        }

        fn new(cx: &mut Context<Self>) -> Self {
            Self::bind_keys(cx);
            Self {
                fired: Default::default(),
                focus_handle: cx.focus_handle(),
            }
        }
    }

    impl Render for ChordFixture {
        fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            div()
                .key_context("ChordFixture")
                .track_focus(&self.focus_handle)
                .debug_selector(|| "chord-fixture".into())
                .on_action(cx.listener(|this, _: &CycleFixtureTab, _, _| {
                    this.fired.borrow_mut().push("cycle");
                }))
                .on_action(cx.listener(|this, _: &JumpFixtureTab, _, _| {
                    this.fired.borrow_mut().push("jump");
                }))
                .on_action(cx.listener(|this, _: &CloseFixtureTab, _, _| {
                    this.fired.borrow_mut().push("close");
                }))
                .child("chord fixture")
        }
    }

    #[gpui::test]
    async fn modifier_chords_dispatch_actions_through_the_real_key_path(cx: &mut TestAppContext) {
        cx.update(Theme::init);
        let window = cx.add_window(|_window, cx| ChordFixture::new(cx));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();
        let fixture = cx.update(|window, _| {
            window
                .root::<ChordFixture>()
                .flatten()
                .expect("fixture root")
        });
        let focus_handle = fixture.read_with(&cx.cx, |fixture, _| fixture.focus_handle.clone());
        cx.update(|window, app| focus_handle.focus(window, app));
        cx.run_until_parked();

        cx.simulate_keystrokes("ctrl-tab ctrl-1 ctrl-w");
        cx.run_until_parked();

        let fired = fixture.read_with(&cx.cx, |fixture, _| fixture.fired.borrow().clone());
        assert_eq!(
            fired,
            vec!["cycle", "jump", "close"],
            "modifier chords must dispatch through GPUI's key path"
        );
    }
}
