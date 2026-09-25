//! The launcher an empty centre pane shows: one tile per surface that pane
//! can open, with an optional keyboard chord under it.
//!
//! A render function, not an entity — it holds no state. What a tile *does*
//! stays with the host (`sirio`'s `main.rs`), the only place that can create
//! a tab: a click reports the item's `action` and the host decides. bezel has
//! no launcher tile of its own — `Scaffolding::option_card` is a 148px
//! preview frame and `row_tile` a 36px identity mark — so the tile is drawn
//! from bezel's tokens, the way `settings::agents_page` draws its identity
//! tile.

use bezel::theme::Theme as BezelTheme;
use bezel::ui::tooltip::Tooltip;
use gpui::{AnyElement, App, SharedString, Window, div, prelude::*, px};
use sirio_theme::Theme;
use std::rc::Rc;

use crate::sidebar::icons::{Icon, IconElement, IconSize};

/// Side of a tile's square frame.
pub const LAUNCHER_TILE_SIZE: f32 = 64.0;

/// One tile. `A` is the host's own action type, so the host can match on it
/// exhaustively instead of on strings.
#[derive(Clone, Debug, PartialEq)]
pub struct LauncherItem<A> {
    /// The tile's element id and debug selector, verbatim.
    pub id: &'static str,
    /// What a click on this tile reports to the host.
    pub action: A,
    pub icon: Icon,
    pub label: SharedString,
    /// The chord, as the host's shortcut table spells it. `None` draws no
    /// line.
    pub shortcut: Option<SharedString>,
    /// Why the tile cannot be used right now. `Some` dims the tile, drops
    /// its click, and says the reason in a tooltip.
    pub disabled: Option<SharedString>,
}

type LauncherCallback<A> = Rc<dyn Fn(A, &mut Window, &mut App)>;

/// The row of tiles, wrapping when the pane is narrow. The caller centres
/// it in the pane.
pub fn pane_launcher<A: Copy + 'static>(
    container_id: &'static str,
    items: &[LauncherItem<A>],
    theme: Theme,
    on_click: impl Fn(A, &mut Window, &mut App) + 'static,
) -> AnyElement {
    let on_click: LauncherCallback<A> = Rc::new(on_click);
    let bezel_theme = theme.to_bezel_theme();
    let mut row = div()
        .id(container_id)
        .debug_selector(move || container_id.to_owned())
        .max_w_full()
        .flex()
        .flex_row()
        .flex_wrap()
        .items_start()
        .justify_center()
        .gap(px(BezelTheme::SPACE_LG));
    for item in items {
        row = row.child(tile(item.clone(), theme, &bezel_theme, on_click.clone()));
    }
    row.into_any_element()
}

fn tile<A: Copy + 'static>(
    item: LauncherItem<A>,
    theme: Theme,
    bezel_theme: &BezelTheme,
    on_click: LauncherCallback<A>,
) -> AnyElement {
    let id = item.id;
    let enabled = item.disabled.is_none();
    let ink = if enabled { theme.text } else { theme.text_faint };
    let frame = div()
        .size(px(LAUNCHER_TILE_SIZE))
        .rounded(px(BezelTheme::panel_radius()))
        .border_1()
        .border_color(bezel_theme.border)
        .bg(bezel_theme.ink(0.03))
        .flex()
        .items_center()
        .justify_center()
        .when(enabled, |frame| {
            frame.group_hover(id, |style| style.bg(theme.element_hover))
        })
        .child(IconElement::new(item.icon, IconSize::Medium).text_color(ink));
    let column = div()
        .id(id)
        .debug_selector(move || id.to_owned())
        .group(id)
        .flex()
        .flex_col()
        .items_center()
        .gap(px(BezelTheme::SPACE_SM))
        .child(frame)
        .child(
            div()
                .text_size(theme.typography.footnote)
                .text_color(ink)
                .child(item.label.clone()),
        )
        .when_some(item.shortcut.clone(), |column, shortcut| {
            column.child(
                div()
                    .text_size(theme.typography.caption2)
                    .text_color(theme.text_faint)
                    .child(shortcut),
            )
        });
    match item.disabled {
        Some(reason) => column
            .opacity(0.5)
            .tooltip(move |window, cx| Tooltip::text(reason.clone(), window, cx))
            .into_any_element(),
        None => {
            let action = item.action;
            column
                .cursor_pointer()
                .on_click(move |_, window, cx| on_click(action, window, cx))
                .into_any_element()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{Modifiers, TestAppContext, VisualTestContext};
    use std::cell::RefCell;

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum Probe {
        Enabled,
        Disabled,
    }

    struct Harness {
        items: Vec<LauncherItem<Probe>>,
        clicked: Rc<RefCell<Vec<Probe>>>,
    }

    impl Render for Harness {
        fn render(&mut self, _: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
            let clicked = self.clicked.clone();
            div().size_full().child(pane_launcher(
                "test-launcher",
                &self.items,
                *Theme::get(cx),
                move |action, _, _| clicked.borrow_mut().push(action),
            ))
        }
    }

    fn item(id: &'static str, action: Probe, disabled: Option<&'static str>) -> LauncherItem<Probe> {
        LauncherItem {
            id,
            action,
            icon: Icon::Globe,
            label: "Probe".into(),
            shortcut: Some("Ctrl+P".into()),
            disabled: disabled.map(SharedString::from),
        }
    }

    #[gpui::test]
    async fn every_item_draws_one_tile_and_only_enabled_tiles_report_a_click(
        cx: &mut TestAppContext,
    ) {
        cx.update(Theme::init);
        let clicked = Rc::new(RefCell::new(Vec::new()));
        let seen = clicked.clone();
        let window = cx.add_window(move |_, _| Harness {
            items: vec![
                item("launcher-enabled", Probe::Enabled, None),
                item("launcher-disabled", Probe::Disabled, Some("Not available here")),
            ],
            clicked: seen,
        });
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        assert!(cx.debug_bounds("test-launcher").is_some(), "the row is drawn");
        let enabled = cx
            .debug_bounds("launcher-enabled")
            .expect("the enabled tile is drawn under its own id");
        let disabled = cx
            .debug_bounds("launcher-disabled")
            .expect("a disabled tile is still drawn");

        cx.simulate_click(disabled.center(), Modifiers::none());
        cx.simulate_click(enabled.center(), Modifiers::none());
        cx.run_until_parked();

        assert_eq!(
            *clicked.borrow(),
            vec![Probe::Enabled],
            "a disabled tile never reports a click"
        );
    }
}
