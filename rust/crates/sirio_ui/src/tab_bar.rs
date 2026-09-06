//! The centre-pane tab strip and its new-tab menu.

use bezel::motion::{Fade, Painter};
use bezel::ui::popover::{self, Popup};
use gpui::{
    Anchor, App, Bounds, Context, FocusHandle, KeyBinding, Pixels, Render, Window, actions,
    canvas, div, point, prelude::*, px, text,
};
use sirio_agents::{AgentAvailability, discover_availability};
use sirio_registry::LaunchSource;
use sirio_theme::Theme;
use std::cell::Cell;
use std::rc::Rc;

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
    // F-TAB-12 removed `MoveToCurrentPane` here, and #319 removed the
    // `MoveToPane(usize)` that outlived it. Both named a gesture the center
    // split no longer has: a tab's half is derived from its `TabKind`
    // (`TabKind::pane_role`), so there is no destination to offer. Moving a
    // tab between halves would mean changing what the tab *is*.
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

    /// The row's drawn text. Read by the shell's tests to assert on what a
    /// menu offers rather than on how many rows it happens to have.
    pub fn label(&self) -> &str {
        &self.label
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
pub fn render_tab_context_menu<T: 'static>(
    items: Vec<TabContextItem>,
    on_action: Rc<dyn Fn(TabContextAction, &mut Window, &mut App)>,
    theme: Theme,
    cx: &Context<T>,
) -> impl IntoElement {
    let bezel_theme = theme.to_bezel_theme();
    let painter = Painter::of(cx);
    let mut menu = popover::popover_card(&bezel_theme)
        .id("tab-context-menu")
        .debug_selector(|| "tab-context-menu".to_owned())
        .w(theme.spacing.menu_width);

    for item in items {
        if item.separator_before {
            menu = menu.child(popover::divider());
            continue;
        }

        let action = item.action;
        let enabled = item.enabled;
        let selector = item.selector.clone();
        let selector_for_debug = selector.clone();
        let mut row = popover::menu_row(
            &bezel_theme,
            false,
            Fade::new(painter, format!("tab-command-{selector}")),
        )
            .id(format!("tab-command-{selector}"))
            .debug_selector(move || format!("tab-command-{selector_for_debug}"))
            .w_full()
            .min_h(theme.typography.ui_line_height)
            .justify_between()
            .gap(theme.spacing.titlebar_control_spacing)
            .text_color(if enabled {
                bezel_theme.text
            } else {
                bezel_theme.text_faint
            })
            .when(!enabled, |this| {
                this.cursor_default().bg(gpui::transparent_black())
            })
            .child(item.label);

        if let Some(reason) = item.disabled_reason {
            row = row.child(
                div()
                    .id(format!("tab-command-disabled-{selector}"))
                    .debug_selector(move || format!("tab-command-disabled-{selector}"))
                    .text_size(theme.typography.caption2)
                    .text_color(bezel_theme.text_faint)
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
    menu_open: Popup<()>,
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
    /// The resolved launch source per adapter id (Task 9). The New Chat
    /// picker offers exactly the adapters whose source can launch a chat
    /// today, instead of asking a compile-time claim.
    chat_launch_sources: std::collections::BTreeMap<String, LaunchSource>,
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
        self.close_menu_now(cx);
    }

    /// Creates the menu-only tab-bar host. The shell owns the open tabs.
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self::bind_keys(cx);
        Self {
            menu_open: Popup::default(),
            chat_picker_open: false,
            focus_handle: cx.focus_handle(),
            anchor_bounds: Rc::new(Cell::new(None)),
            on_new_tab: None,
            on_chat_agent: None,
            on_open_agent_settings: None,
            chat_agents: discover_availability(),
            chat_launch_sources: std::collections::BTreeMap::new(),
        }
    }

    /// Overrides discovery for deterministic callers and headless tests. The
    /// render path still applies the same availability gate as production.
    pub fn with_chat_agents(mut self, agents: Vec<AgentAvailability>) -> Self {
        self.chat_agents = agents;
        self
    }

    /// Pins the launch sources the New Chat picker filters on (Task 9).
    pub fn with_chat_launch_sources(mut self, sources: Vec<(String, LaunchSource)>) -> Self {
        self.chat_launch_sources = sources.into_iter().collect();
        self
    }

    /// Applies a fresh launch-source sweep from the host (Task 8).
    pub fn apply_chat_launch_sources(&mut self, sources: Vec<(String, LaunchSource)>) {
        self.chat_launch_sources = sources.into_iter().collect();
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

    /// #376: whether the `+` new-tab menu is currently mounted (open or
    /// playing its exit animation). The host reads this every frame to hide
    /// the native browser child while the menu is up: on Windows the WebView2
    /// HWND sits above GPUI's surface, so a GPUI menu overlapping the page
    /// would otherwise paint behind it and stop being clickable.
    ///
    /// `get()` (mounted) rather than `is_open()` (open and interactive) is
    /// deliberate: during the exit phase the card still paints its fade-out
    /// over the same rectangle and still needs the page out of the way.
    pub fn is_menu_open(&self) -> bool {
        self.menu_open.get().is_some()
    }

    fn toggle_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.menu_open.take_press_was_open() {
            self.close_menu(cx);
            return;
        }
        self.menu_open.open(());
        self.chat_picker_open = false;
        if self.menu_open.is_open() {
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
        let closing = self.menu_open.begin_close();
        if closing || self.chat_picker_open {
            self.chat_picker_open = false;
            if closing {
                popover::reap_popup(cx, |tab_bar| &mut tab_bar.menu_open);
            }
            cx.notify();
        }
    }

    fn close_menu_now(&mut self, cx: &mut Context<Self>) {
        if self.menu_open.get().is_some() || self.chat_picker_open {
            self.menu_open.close();
            self.chat_picker_open = false;
            cx.notify();
        }
    }

    fn emit(&mut self, action: NewTabAction, cx: &mut Context<Self>) {
        self.close_menu_now(cx);
        if let Some(callback) = &self.on_new_tab {
            callback(action);
        }
    }

    fn emit_chat_agent(&mut self, id: &'static str, cx: &mut Context<Self>) {
        self.close_menu_now(cx);
        if let Some(callback) = &self.on_chat_agent {
            callback(id);
        }
    }

    fn emit_open_agent_settings(&mut self, cx: &mut Context<Self>) {
        self.close_menu_now(cx);
        if let Some(callback) = &self.on_open_agent_settings {
            callback();
        }
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
        bezel_theme: &bezel::theme::Theme,
        painter: Painter,
        chevron: bool,
        hint: Option<&'static str>,
    ) -> impl IntoElement {
        let (icon, glyph_color) = match label {
            "New Terminal" => (Icon::SquareTerminal, theme.text_faint),
            "Changes" => (Icon::File, theme.text_faint),
            // Agent marks wear their published brand colour, not a theme
            // token: Claude its orange, the monochrome trio the foreground
            // they are authored in. omp's tint is ignored by its
            // full-colour path.
            "Claude Code" | "Split Claude Code" => (
                Icon::ClaudeCode,
                Icon::ClaudeCode
                    .agent_mark_color(theme.text)
                    .unwrap_or(theme.text),
            ),
            "Codex" => (Icon::Codex, theme.text),
            "OpenCode" => (Icon::OpenCode, theme.text),
            "Pi" => (Icon::Pi, theme.text),
            "Oh-My-Pi" => (Icon::OhMyPi, theme.text),
            "New Browser" => (Icon::Globe, theme.text_faint),
            "New Chat" => (Icon::MessageSquare, theme.text_faint),
            _ => (Icon::File, theme.text_faint),
        };

        popover::menu_row(
            bezel_theme,
            false,
            Fade::new(painter, format!("new-tab-item-{}", menu_selector(label))),
        )
            .id(label)
            .debug_selector(move || format!("new-tab-item-{}", menu_selector(label)))
            .w_full()
            .h(px(29.0))
            .justify_between()
            .text_color(bezel_theme.text)
            .on_click(move |_, _, cx| entity.update(cx, |this, cx| this.emit(action, cx)))
            .child(
                div()
                    .flex()
                    .items_center()
                    .min_w_0()
                    .overflow_hidden()
                    .gap(px(7.0))
                    .child(
                        div()
                            .w(px(14.0))
                            .flex_shrink_0()
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(IconElement::new(icon, IconSize::Small).text_color(glyph_color)),
                    )
                    .child(text!(id = format!("new-tab-label-{label}"), label)),
            )
            .when(chevron, |this| {
                this.child(
                    div().text_color(theme.text_faint).child(
                        IconElement::new(Icon::ChevronRight, IconSize::XSmall)
                            .text_color(theme.text_faint),
                    ),
                )
            })
            // #205: annotate, never disable. The row stays clickable because
            // availability is probed against Sirio's own PATH while the agent
            // is launched through a login shell that can resolve more -- so a
            // gate here would grey out working nvm installs of pi and omp.
            .when_some(hint, |this, hint| {
                this.child(
                    div()
                        .debug_selector(move || format!("new-tab-hint-{}", menu_selector(label)))
                        .flex_shrink_0()
                        .ml(px(8.0))
                        .text_size(theme.typography.caption2)
                        .text_color(theme.text_faint)
                        .child(hint),
                )
            })
    }

    /// #205: the menu's "not on PATH" note for an agent row, or `None` when
    /// the binary resolved or the row is not an agent.
    ///
    /// Reuses `AgentAvailability::status_label`, the same text Settings shows
    /// for the same adapter -- the two surfaces disagreeing about one agent is
    /// what made this worth fixing. "Split Claude Code" launches the same
    /// binary as "Claude Code", so it borrows that verdict.
    fn path_hint(&self, label: &'static str) -> Option<&'static str> {
        let display = if label == "Split Claude Code" {
            "Claude Code"
        } else {
            label
        };
        self.chat_agents
            .iter()
            .find(|agent| agent.display_name == display)
            .filter(|agent| !agent.is_available())
            .map(|agent| agent.status_label())
    }

    fn separator() -> impl IntoElement {
        popover::divider()
    }

    fn render_new_chat_item(
        entity: gpui::Entity<Self>,
        theme: Theme,
        bezel_theme: &bezel::theme::Theme,
        painter: Painter,
        expanded: bool,
    ) -> impl IntoElement {
        popover::menu_row(
            bezel_theme,
            false,
            Fade::new(painter, "new-tab-item-new-chat"),
        )
            .id("New Chat")
            .debug_selector(|| "new-tab-item-new-chat".to_owned())
            .w_full()
            .h(px(29.0))
            .justify_between()
            .text_color(bezel_theme.text)
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
                                    .text_color(theme.text),
                            ),
                    )
                    .child(text!(id = "new-tab-label-New Chat", "New Chat")),
            )
            .child(div().text_color(theme.text_faint).child(IconElement::new(
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
        bezel_theme: &bezel::theme::Theme,
        painter: Painter,
    ) -> impl IntoElement {
        let id = agent.id;
        let selector = format!("new-tab-chat-agent-{id}");
        let selector_for_debug = selector.clone();
        let display_name = agent.display_name;
        let icon = Icon::for_agent_id(id).unwrap_or(Icon::MessageSquare);
        let mut mark = IconElement::new(icon, IconSize::Small);
        if let Some(tint) = icon.agent_mark_color(theme.text) {
            mark = mark.text_color(tint);
        }
        let mark_element = mark;
        popover::menu_row(bezel_theme, false, Fade::new(painter, selector.clone()))
            .id(selector.clone())
            .debug_selector(move || selector_for_debug.clone())
            .w_full()
            .h(px(29.0))
            .gap(px(7.0))
            .text_color(bezel_theme.text)
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
    fn render_chat_empty(
        entity: gpui::Entity<Self>,
        theme: Theme,
        bezel_theme: &bezel::theme::Theme,
        painter: Painter,
    ) -> impl IntoElement {
        popover::menu_row(bezel_theme, false, Fade::new(painter, "new-chat-empty"))
            .id("new-chat-empty")
            .debug_selector(|| "new-chat-empty".to_owned())
            .w_full()
            .flex_col()
            .gap(theme.spacing.titlebar_control_spacing)
            .text_color(bezel_theme.text_faint)
            .on_click(move |_, _, cx| {
                entity.update(cx, |this, cx| this.emit_open_agent_settings(cx))
            })
            .child("Other agents…")
            .child(
                div()
                    .text_size(theme.typography.caption2)
                    .text_color(theme.text_faint)
                    .child("No supported agent found on PATH"),
            )
    }
}

impl TabBar {
    /// The standing route to the agent registry, shown under the agents that
    /// *are* available.
    ///
    /// Deliberately quieter than an agent row -- meta text, no icon -- because
    /// it is chrome, not a choice among the agents above it. It shares the
    /// empty state's destination, and carries no "nothing found" caption:
    /// something was found, which is why this variant exists.
    fn render_other_agents_link(
        entity: gpui::Entity<Self>,
        theme: Theme,
        bezel_theme: &bezel::theme::Theme,
        painter: Painter,
    ) -> impl IntoElement {
        popover::menu_row(
            bezel_theme,
            false,
            Fade::new(painter, "new-chat-other-agents"),
        )
            .id("new-chat-other-agents")
            .debug_selector(|| "new-chat-other-agents".to_owned())
            .w_full()
            .h(px(29.0))
            .mt(theme.spacing.titlebar_control_spacing)
            .border_t_1()
            .border_color(theme.border)
            .text_color(bezel_theme.text_faint)
            .on_click(move |_, _, cx| {
                entity.update(cx, |this, cx| this.emit_open_agent_settings(cx))
            })
            .child("Other agents…")
    }
}

impl Render for TabBar {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let _perf = sirio_perf::span("TabBar.render", cx.entity_id().as_u64());
        let theme = *Theme::get(cx);
        let bezel_theme = theme.to_bezel_theme();
        let painter = Painter::of(cx);
        let entity = cx.entity();
        let menu_open = self.menu_open.get().is_some();
        let menu_closing = self.menu_open.closing_since();
        let chat_picker_open = self.chat_picker_open;
        let anchor_bounds = self.anchor_bounds.clone();

        // The picker offers exactly the adapters whose resolved source can
        // launch a chat today (Task 9): a Builtin or Installed source.
        // Before the host pushes sources, nothing is offered — hiding beats
        // guessing from a compile-time claim.
        let available_chat_agents = self
            .chat_agents
            .iter()
            .filter(|agent| {
                matches!(
                    self.chat_launch_sources.get(agent.id),
                    Some(LaunchSource::Builtin { .. } | LaunchSource::Installed(_))
                )
            })
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
            chat_agent_menu = chat_agent_menu.child(Self::render_chat_empty(
                entity.clone(),
                theme,
                &bezel_theme,
                painter,
            ));
        } else {
            // The way to install another agent used to live ONLY in the empty
            // state above: it appeared when nothing was available and vanished
            // the moment one agent worked. That is the empty-state trap --
            // offered when it is least likely to be wanted, hidden from the
            // user who already has a working setup and is the one most likely
            // to want a second agent.
            //
            // A blind review of this menu, seeing only the populated case,
            // reported "no path at all: the menu is a closed list; a user with
            // an unlisted agent gets no hint where to go". It was right about
            // what it could see, and what it could see is what a user sees.
            //
            // Same destination as the empty state (`emit_open_agent_settings`);
            // it just no longer disappears once it has company.
            for agent in available_chat_agents {
                chat_agent_menu = chat_agent_menu.child(Self::render_chat_agent_item(
                    agent,
                    entity.clone(),
                    theme,
                    &bezel_theme,
                    painter,
                ));
            }
            chat_agent_menu = chat_agent_menu.child(Self::render_other_agents_link(
                entity.clone(),
                theme,
                &bezel_theme,
                painter,
            ));
        }

        let menu = popover::popover_card(&bezel_theme)
            .id("new-tab-menu")
            .debug_selector(|| "new-tab-menu".to_owned())
            // #205: widened from 170px to fit the "Not found on PATH" note an
            // agent row can carry. Measured, not guessed: the note overran the
            // old border by 76px.
            .w(px(250.0))
            .child(Self::render_menu_item(
                "New Terminal",
                NewTabAction::NewTerminal,
                entity.clone(),
                theme,
                &bezel_theme,
                painter,
                false,
                None,
            ))
            .child(Self::render_menu_item(
                "Changes",
                NewTabAction::NewChanges,
                entity.clone(),
                theme,
                &bezel_theme,
                painter,
                false,
                None,
            ))
            .child(Self::render_menu_item(
                "New Browser",
                NewTabAction::NewBrowser,
                entity.clone(),
                theme,
                &bezel_theme,
                painter,
                false,
                None,
            ))
            .child(Self::separator())
            .child(Self::render_menu_item(
                "Claude Code",
                NewTabAction::ClaudeCode,
                entity.clone(),
                theme,
                &bezel_theme,
                painter,
                false,
                self.path_hint("Claude Code"),
            ))
            .child(Self::render_menu_item(
                "Codex",
                NewTabAction::Codex,
                entity.clone(),
                theme,
                &bezel_theme,
                painter,
                false,
                self.path_hint("Codex"),
            ))
            .child(Self::render_menu_item(
                "OpenCode",
                NewTabAction::OpenCode,
                entity.clone(),
                theme,
                &bezel_theme,
                painter,
                false,
                self.path_hint("OpenCode"),
            ))
            .child(Self::render_menu_item(
                "Pi",
                NewTabAction::Pi,
                entity.clone(),
                theme,
                &bezel_theme,
                painter,
                false,
                self.path_hint("Pi"),
            ))
            .child(Self::render_menu_item(
                "Oh-My-Pi",
                NewTabAction::OhMyPi,
                entity.clone(),
                theme,
                &bezel_theme,
                painter,
                false,
                self.path_hint("Oh-My-Pi"),
            ))
            .child(Self::separator())
            .child(Self::render_menu_item(
                "Split Claude Code",
                NewTabAction::SplitClaudeCode,
                entity.clone(),
                theme,
                &bezel_theme,
                painter,
                false,
                self.path_hint("Split Claude Code"),
            ))
            .child(Self::separator())
            .child(Self::render_new_chat_item(
                entity,
                theme,
                &bezel_theme,
                painter,
                chat_picker_open,
            ))
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
            .text_color(theme.text_faint)
            .hover(|style| style.bg(theme.element_hover))
            .on_mouse_down(gpui::MouseButton::Left, cx.listener(|this, _, _, _| {
                this.menu_open.note_trigger_press();
            }))
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
            .child(IconElement::new(Icon::Plus, IconSize::Small).text_color(theme.text));

        if menu_open {
            let anchor = self
                .anchor_bounds
                .get()
                .map(|bounds| bounds.corner(Anchor::BottomLeft))
                .unwrap_or_default();
            new_tab_button = new_tab_button.child(popover::menu_at(
                "new-tab-menu-layer",
                point(anchor.x + px(6.0), anchor.y + px(3.0)),
                menu.into_any_element(),
                menu_closing,
            ));
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
            .bg(theme.surface)
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
                .with_chat_launch_sources(vec![(
                    "codex".to_string(),
                    LaunchSource::Builtin {
                        program: "codex-acp".into(),
                        args: vec![],
                    },
                )])
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
    /// tab strip (`RIGHT_PANEL_WIDTH` in `sirio/src/main.rs`), so the menu
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

    /// #205: the `+` menu offered agents the app already knew were missing,
    /// while Settings said "Not found on PATH" about the same adapter. It is
    /// annotated rather than disabled, for the reason the issue records: the
    /// availability probe resolves against Sirio's own PATH, but agents are
    /// launched through a login shell that can resolve more -- so greying out
    /// `pi` or `omp` would break working nvm setups. The palette's own comment
    /// states the same principle: "unavailable operations remain discoverable".
    #[gpui::test]
    async fn the_new_tab_menu_marks_agents_that_are_not_on_path(cx: &mut TestAppContext) {
        cx.update(Theme::init);
        let window = cx.add_window(|_window, cx| {
            TabBar::new(cx).with_chat_agents(vec![
                available_agent("codex", "Codex"),
                AgentAvailability {
                    id: "omp",
                    display_name: "Oh-My-Pi",
                    executable: None,
                },
            ])
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

        assert!(
            cx.debug_bounds("new-tab-hint-oh-my-pi").is_some(),
            "an agent that is not on PATH says so in the menu"
        );
        assert!(
            cx.debug_bounds("new-tab-item-oh-my-pi").is_some(),
            "and stays clickable -- the login shell may still resolve it"
        );
        assert!(
            cx.debug_bounds("new-tab-hint-codex").is_none(),
            "an agent that is on PATH carries no hint"
        );

        // #212's lesson applied: the menu is a fixed width, so the note has to
        // be shown to FIT, not merely to render. The first attempt overflowed
        // the border and collided with the label.
        let menu = cx.debug_bounds("new-tab-menu").expect("the menu is drawn");
        let hint = cx
            .debug_bounds("new-tab-hint-oh-my-pi")
            .expect("hint drawn");
        assert!(
            hint.right() <= menu.right(),
            "the note must stay inside the menu: hint right {:?} vs menu right {:?}",
            hint.right(),
            menu.right()
        );
        assert!(
            hint.left() >= menu.left(),
            "and inside its left edge: hint left {:?} vs menu left {:?}",
            hint.left(),
            menu.left()
        );
    }

    /// #376: the host-visible mirror of the `+` menu. The workspace hides the
    /// native browser child while this reads open, so it must track the drawn
    /// menu: closed before the first click, open while the card is up, closed
    /// again once a row is picked.
    #[gpui::test]
    async fn new_tab_menu_open_state_tracks_the_drawn_menu(cx: &mut TestAppContext) {
        cx.update(Theme::init);
        let tab_bar_cell: Rc<RefCell<Option<gpui::Entity<TabBar>>>> =
            Rc::new(RefCell::new(None));
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
            .expect("the window build captured the tab bar entity");
        assert!(
            !tab_bar.read_with(&cx.cx, |bar, _| bar.is_menu_open()),
            "the menu starts closed"
        );

        let plus = cx
            .debug_bounds("new-tab-button")
            .expect("the plus control is drawn");
        cx.simulate_click(plus.center(), Modifiers::none());
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.simulate_next_frame(cx);
            window.simulate_next_frame(cx);
        });
        cx.run_until_parked();
        assert!(
            cx.debug_bounds("new-tab-menu").is_some(),
            "clicking + draws the menu"
        );
        assert!(
            tab_bar.read_with(&cx.cx, |bar, _| bar.is_menu_open()),
            "the mirror must read open while the card is up"
        );

        let item = cx
            .debug_bounds("new-tab-item-new-terminal")
            .expect("the first menu row is drawn");
        cx.simulate_click(item.center(), Modifiers::none());
        cx.run_until_parked();
        assert!(
            cx.debug_bounds("new-tab-menu").is_none(),
            "picking a row closes the menu"
        );
        assert!(
            !tab_bar.read_with(&cx.cx, |bar, _| bar.is_menu_open()),
            "the mirror must read closed again afterwards"
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
                .with_chat_launch_sources(vec![
                    (
                        "codex".to_string(),
                        LaunchSource::Builtin {
                            program: "codex-acp".into(),
                            args: vec![],
                        },
                    ),
                    (
                        "pi".to_string(),
                        LaunchSource::Installed(sirio_registry::InstalledAgent {
                            id: "pi-acp".into(),
                            version: "1.0.0".into(),
                            executable: "/data/pi-acp/bin/pi-acp".into(),
                            args: vec![],
                            integrity: sirio_registry::Integrity::Sha256,
                        }),
                    ),
                    (
                        "claude".to_string(),
                        LaunchSource::Unavailable(sirio_registry::UnavailableReason::NotInRegistry),
                    ),
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
            cx.debug_bounds("new-tab-chat-agent-pi").is_some(),
            "a Sirio-managed install is offered whether or not the CLI is on PATH"
        );
        assert!(
            cx.debug_bounds("new-tab-chat-agent-claude").is_none(),
            "an adapter with no resolvable launch source must not be offered"
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

    /// The route to install another agent must survive having agents.
    ///
    /// It used to live only in the empty state: offered when nothing was
    /// available, gone the moment one agent worked -- so the user most likely
    /// to want a second agent was the one who could no longer find the way to
    /// get one. A blind review of the populated menu called it "a closed
    /// list", which is exactly what it was.
    #[gpui::test]
    async fn other_agents_stays_reachable_once_an_agent_is_available(cx: &mut TestAppContext) {
        cx.update(Theme::init);
        let opened = std::rc::Rc::new(std::cell::RefCell::new(false));
        let opened_for_callback = opened.clone();
        let window = cx.add_window(|_window, cx| {
            TabBar::new(cx)
                .with_chat_agents(vec![available_agent("codex", "Codex")])
                .with_chat_launch_sources(vec![(
                    "codex".to_string(),
                    LaunchSource::Builtin {
                        program: "codex-acp".into(),
                        args: vec![],
                    },
                )])
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

        assert!(
            cx.debug_bounds("new-tab-chat-agent-codex").is_some(),
            "the populated case: an agent IS offered"
        );
        assert!(
            cx.debug_bounds("new-chat-empty").is_none(),
            "and the empty-state card is correctly absent"
        );
        let other = cx
            .debug_bounds("new-chat-other-agents")
            .expect("the route to more agents survives having one");
        cx.simulate_click(other.center(), Modifiers::none());
        cx.run_until_parked();
        assert!(
            *opened.borrow(),
            "and it reaches the same destination the empty state did"
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
                _cx,
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
    // in the shell (`sirio/src/main.rs`, routed to codex12). What this crate
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
