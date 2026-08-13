//! The centre-pane tab strip and its new-tab menu.

use gpui::{
    Anchor, App, Bounds, Context, FocusHandle, KeyBinding, Pixels, Render, Window, actions,
    anchored, canvas, deferred, div, point, prelude::*, px, text,
};
use std::cell::Cell;
use std::rc::Rc;
use tiller_theme::Theme;

use crate::sidebar::icons::{Icon, IconElement};

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
            "Changes" => (Icon::File, theme.meta),
            "Claude Code" | "Split Claude Code" => (Icon::ClaudeCode, gpui::rgb(0xd97757)),
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
                            .child(IconElement::new(icon, px(14.0)).text_color(glyph_color)),
                    )
                    .child(text!(id = format!("new-tab-label-{label}"), label)),
            )
            .when(chevron, |this| {
                this.child(
                    div().text_color(theme.meta).child(
                        IconElement::new(Icon::ChevronRight, px(12.0)).text_color(theme.meta),
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
            .debug_selector(|| "new-tab-button".to_owned())
            .w(px(24.0))
            .h(px(24.0))
            .mr(px(2.0))
            .flex()
            .items_center()
            .justify_center()
            .rounded(theme.radii.control)
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
    use std::rc::Rc;
    use std::sync::Once;

    #[gpui::test]
    async fn drawn_new_tab_menu_dispatches_every_item_action(cx: &mut TestAppContext) {
        cx.update(Theme::init);
        let actions = Rc::new(RefCell::new(Vec::new()));
        let collected = actions.clone();
        let window = cx.add_window(|_window, cx| {
            TabBar::new(cx).on_new_tab(move |action| collected.borrow_mut().push(action))
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
            ("New Chat", "new-chat", NewTabAction::NewChat),
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
