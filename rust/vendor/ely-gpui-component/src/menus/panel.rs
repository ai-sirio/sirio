use gpui::{
    Anchor, Animation, AnimationExt, AnyElement, App, Div, ElementId, FontWeight,
    InteractiveElement, IntoElement, MouseButton, ParentElement, Pixels, Point, RenderOnce,
    ScrollHandle, SharedString, Stateful, StatefulInteractiveElement, Styled, Window, anchored,
    deferred, div, prelude::*, px,
};

use super::{
    draw::row,
    model::{Entry, Menu},
};
use crate::{
    forms::surface,
    motion,
    primitives::raise,
    theme::{ActiveTheme, TextSize},
};

/// The card Ely's floating lists sit on, for a host that draws its own content in it.
pub fn panel_surface(id: impl Into<ElementId>, cx: &App) -> Stateful<Div> {
    surface(id, cx)
}

/// Where a floating layer hangs: over the top edge of the `relative` element it is a child of,
/// left-aligned or right-aligned to it, or at a point of the window.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Hang {
    Above,
    AboveEnd,
    At(Point<Pixels>),
}

/// Floats `content` as a layer of its own, for a host that decides when it exists: kept inside the
/// window, occluding what is under it, fading in. A press outside it is the host's to handle.
pub fn layer(
    id: impl Into<ElementId>,
    at: Hang,
    content: impl IntoElement,
    cx: &App,
) -> AnyElement {
    let id = id.into();
    let enter =
        Animation::new(motion::duration(motion::FAST, cx)).with_easing(motion::ease_out_cubic);
    let body =
        div()
            .occlude()
            .child(content)
            .with_animation((id.clone(), "in"), enter, |body, t| body.opacity(t));
    let placed = |anchor: Anchor| {
        anchored()
            .anchor(anchor)
            .snap_to_window_with_margin(px(8.0))
    };
    match at {
        Hang::Above => div()
            .absolute()
            .top_0()
            .left_0()
            .size_0()
            .child(raise((id, "layer"), placed(Anchor::BottomLeft).child(body)).with_priority(1))
            .into_any_element(),
        Hang::AboveEnd => div()
            .absolute()
            .top_0()
            .right_0()
            .size_0()
            .child(raise((id, "layer"), placed(Anchor::BottomRight).child(body)).with_priority(1))
            .into_any_element(),
        Hang::At(point) => raise(
            (id, "layer"),
            placed(Anchor::TopLeft).position(point).child(body),
        )
        .with_priority(1)
        .into_any_element(),
    }
}

/// A menu's rows drawn the way Ely draws them, for a host that owns the rest: whether the menu is
/// open, where it hangs, which row the keys mark and what closes it. A press on a row runs its
/// `on_click` and leaves the menu to the host, which may keep it open for a toggle.
#[derive(IntoElement)]
pub struct MenuPanel {
    id: ElementId,
    menu: Menu,
    current: Option<usize>,
    head: Option<AnyElement>,
    empty: Option<AnyElement>,
    selector: Option<SharedString>,
    scroll: Option<(ScrollHandle, Pixels)>,
    width: Option<Pixels>,
}

impl MenuPanel {
    pub fn new(id: impl Into<ElementId>, menu: Menu) -> Self {
        Self {
            id: id.into(),
            menu,
            current: None,
            head: None,
            empty: None,
            selector: None,
            scroll: None,
            width: None,
        }
    }

    /// The row the host's keys have marked.
    pub fn current(mut self, entry: Option<usize>) -> Self {
        self.current = entry;
        self
    }

    /// Over the rows, such as a filter field.
    pub fn head(mut self, head: impl IntoElement) -> Self {
        self.head = Some(head.into_any_element());
        self
    }

    /// What to show in place of rows when the menu has none.
    pub fn empty(mut self, empty: impl IntoElement) -> Self {
        self.empty = Some(empty.into_any_element());
        self
    }

    /// A debug selector on the list of rows (a no-op in release builds).
    pub fn selector(mut self, selector: impl Into<SharedString>) -> Self {
        self.selector = Some(selector.into());
        self
    }

    /// Rows scroll with `handle` once they are taller than `max_height`.
    pub fn scroll(mut self, handle: &ScrollHandle, max_height: Pixels) -> Self {
        self.scroll = Some((handle.clone(), max_height));
        self
    }

    pub fn width(mut self, width: Pixels) -> Self {
        self.width = Some(width);
        self
    }
}

impl RenderOnce for MenuPanel {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();
        let colors = theme.colors;
        let has_rows = !self.menu.entries.is_empty();
        let rows = self
            .menu
            .entries
            .iter()
            .enumerate()
            .map(|(ix, entry)| match entry {
                Entry::Separator => div().h_px().my_1().bg(colors.border).into_any_element(),
                Entry::Heading(title) => div()
                    .px_2()
                    .pt_1p5()
                    .pb_1()
                    .text_size(theme.text_size(TextSize::Xs))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(colors.fg_subtle)
                    .child(title.clone())
                    .into_any_element(),
                Entry::Item(item) => {
                    let (run, hover) = (item.on_click.clone(), colors.hover);
                    row(item, ix, self.current == Some(ix), cx)
                        .when(!item.disabled, |row| {
                            row.hover(move |style| style.bg(hover)).on_mouse_down(
                                MouseButton::Left,
                                move |_, window, cx| {
                                    window.prevent_default();
                                    cx.stop_propagation();
                                    if let Some(run) = &run {
                                        run(window, cx);
                                    }
                                },
                            )
                        })
                        .into_any_element()
                }
            })
            .collect::<Vec<_>>();
        let list = div()
            .id((self.id.clone(), "rows"))
            .flex()
            .flex_col()
            .when_some(self.selector, |list, selector| {
                list.debug_selector(move || selector.to_string())
            })
            .when_some(self.scroll, |list, (handle, max)| {
                list.max_h(max).overflow_y_scroll().track_scroll(&handle)
            })
            .children(rows);
        surface((self.id.clone(), "panel"), cx)
            .relative()
            .min_w(theme.menu_width())
            .when_some(self.width, |panel, width| panel.w(width))
            .p_1()
            .flex()
            .flex_col()
            .children(self.head)
            .when(has_rows, |panel| panel.child(list))
            .when_some(self.empty.filter(|_| !has_rows), |panel, empty| {
                panel.child(empty)
            })
    }
}
