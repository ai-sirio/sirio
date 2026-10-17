use gpui::{
    AnyElement, App, ClickEvent, ElementId, InteractiveElement, IntoElement, ParentElement,
    RenderOnce, SharedString, StatefulInteractiveElement, Styled, Window, div, prelude::*,
};

use crate::{
    primitives::{Icon, IconName, Tooltip},
    theme::{ActiveTheme, IconSize, Radius},
};

type ClickHandler = Box<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

/// One status: an icon, a label, or both. Clickable when given a handler.
#[derive(IntoElement)]
pub struct StatusBarItem {
    id: ElementId,
    leading: Option<AnyElement>,
    icon: Option<IconName>,
    label: Option<SharedString>,
    tooltip: Option<SharedString>,
    on_click: Option<ClickHandler>,
}

impl StatusBarItem {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            leading: None,
            icon: None,
            label: None,
            tooltip: None,
            on_click: None,
        }
    }

    /// Drawn before the icon, such as a mark with a count.
    pub fn leading(mut self, leading: impl IntoElement) -> Self {
        self.leading = Some(leading.into_any_element());
        self
    }

    pub fn icon(mut self, icon: IconName) -> Self {
        self.icon = Some(icon);
        self
    }

    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }

    pub fn tooltip(mut self, tooltip: impl Into<SharedString>) -> Self {
        self.tooltip = Some(tooltip.into());
        self
    }

    pub fn on_click(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_click = Some(Box::new(handler));
        self
    }
}

impl RenderOnce for StatusBarItem {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();
        let hover = theme.colors.hover;
        div()
            .id(self.id)
            .flex()
            .items_center()
            .gap_1()
            .h_full()
            .px_1p5()
            .rounded(theme.radius(Radius::Sm))
            .children(self.leading)
            .when_some(self.icon, |item, icon| {
                item.child(
                    Icon::new(icon)
                        .size(IconSize::Xs)
                        .color(theme.colors.fg_muted),
                )
            })
            .when_some(self.label, |item, label| item.child(label))
            .when_some(self.tooltip, |item, text| item.tooltip(Tooltip::text(text)))
            .when_some(self.on_click, |item, handler| {
                item.cursor_pointer()
                    .hover(|style| style.bg(hover))
                    .on_click(handler)
            })
    }
}
