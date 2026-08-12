//! The centre-pane tab strip and its new-tab menu.

use gpui::{
    Anchor, App, Bounds, Context, FocusHandle, KeyBinding, Pixels, Render, Window,
    actions, anchored, canvas, deferred, div, point, prelude::*, px, text,
};
use std::cell::Cell;
use std::rc::Rc;
use std::sync::Once;
use tiller_theme::Theme;

use crate::sidebar::icons::{Icon, IconElement};

const HEIGHT: f32 = 34.0;

actions!(new_tab_menu, [DismissMenu]);

static MENU_KEYS_BOUND: Once = Once::new();

/// Actions emitted by the new-tab menu. The owner decides how to handle them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NewTabAction {
    NewTerminal,
    ClaudeCode,
    Codex,
    OpenCode,
    Pi,
    OhMyPi,
    SplitClaudeCode,
    NewBrowser,
    NewChat,
}

/// A horizontal tab strip with a callback-driven new-tab menu.
pub struct TabBar {
    menu_open: bool,
    focus_handle: FocusHandle,
    anchor_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    on_new_tab: Option<Rc<dyn Fn(NewTabAction)>>,
}

impl TabBar {
    /// Installs the menu-dismissal keymap in the host application. Mirrors
    /// `Chat::bind_keys`: a namespaced `actions!` type bound through GPUI's
    /// action-dispatch system (`zed-ref/crates/ui/context_menu.rs`'s own
    /// `menu::Cancel` pattern), not a hand-rolled raw-key string match — the
    /// hand-rolled version was the bug: Escape fell through past it and hit
    /// the first menu row's own click handling instead of dismissing.
    fn bind_keys(cx: &mut App) {
        MENU_KEYS_BOUND.call_once(|| {
            cx.bind_keys([KeyBinding::new("escape", DismissMenu, Some("NewTabMenu"))]);
        });
    }

    fn dismiss_menu(&mut self, _: &DismissMenu, _: &mut Window, cx: &mut Context<Self>) {
        self.close_menu(cx);
    }

    /// Creates the menu-only tab-bar host. The shell owns the open tabs.
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self::bind_keys(cx);
        Self {
            menu_open: false,
            focus_handle: cx.focus_handle(),
            anchor_bounds: Rc::new(Cell::new(None)),
            on_new_tab: None,
        }
    }

    /// Installs the callback used by every menu action.
    pub fn on_new_tab(mut self, callback: impl Fn(NewTabAction) + 'static) -> Self {
        self.on_new_tab = Some(Rc::new(callback));
        self
    }

    fn toggle_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.menu_open = !self.menu_open;
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
        if self.menu_open {
            self.menu_open = false;
            cx.notify();
        }
    }

    fn emit(&mut self, action: NewTabAction, cx: &mut Context<Self>) {
        self.menu_open = false;
        if let Some(callback) = &self.on_new_tab {
            callback(action);
        }
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
            "Claude Code" | "Split Claude Code" => {
                (Icon::ClaudeCode, gpui::rgb(0xd97757))
            }
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
            .w_full()
            .h(px(29.0))
            .px(px(12.0))
            .flex()
            .items_center()
            .justify_between()
            .rounded(px(4.0))
            .text_size(px(13.0))
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
                            .child(IconElement::new(icon, px(14.0)).text_color(glyph_color)),
                    )
                    .child(text!(id = format!("new-tab-label-{label}"), label)),
            )
            .when(chevron, |this| {
                this.child(
                    div()
                        .text_color(theme.meta)
                        .child(
                            IconElement::new(Icon::ChevronRight, px(12.0))
                                .text_color(theme.meta),
                        ),
                )
            })
    }

    fn separator(theme: Theme) -> impl IntoElement {
        div().mx(px(8.0)).h(px(1.0)).bg(theme.hairline)
    }
}

impl Render for TabBar {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *Theme::get(cx);
        let entity = cx.entity();
        let menu_open = self.menu_open;
        let anchor_bounds = self.anchor_bounds.clone();

        let menu = div()
            .id("new-tab-menu")
            .w(px(170.0))
            .p(px(6.0))
            .rounded(px(12.0))
            .border_1()
            .border_color(theme.hairline)
            .bg(theme.background)
            .shadow_lg()
            .child(Self::render_menu_item(
                "New Terminal",
                NewTabAction::NewTerminal,
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
            .child(Self::render_menu_item(
                "New Browser",
                NewTabAction::NewBrowser,
                entity.clone(),
                theme,
                false,
            ))
            .child(Self::separator(theme))
            .child(Self::render_menu_item(
                "New Chat",
                NewTabAction::NewChat,
                entity,
                theme,
                true,
            ));

        let menu = menu.on_mouse_down_out(cx.listener(|this, _, _, cx| this.close_menu(cx)));

        let mut new_tab_button = div()
            .id("new-tab-button")
            .w(px(24.0))
            .h(px(24.0))
            .mr(px(2.0))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(4.0))
            .text_size(px(13.0))
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
                    move |bounds, _, _| anchor_bounds.set(Some(bounds)),
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full(),
            )
            .child(IconElement::new(Icon::Plus, px(13.0)).text_color(theme.meta));

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

        let root = div()
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
            .child(new_tab_button);
        root
    }
}
