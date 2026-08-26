//! PROTOTYPE — throwaway. Three ways to spend 720px on the Project Settings
//! sheet in a *single* column, switchable from a floating bottom bar.
//! Answers #156 for the map in #154. Not production code.
//!
//! Run: `cargo run -p tiller_ui --example project_settings_proto`
//!
//! The constraint (map decisions 7 and 8): one column, 720px, reusing
//! `settings::CONTENT_WIDTH`, `max_h` 80% with `.overflow_y_scroll()`. Two
//! columns was considered and rejected as a redesign nobody asked for — so
//! these three differ in how the single column *uses* the width, not in how
//! the fields are grouped.
//!
//!   A — label left, control right. A settings form: a fixed label column,
//!       controls right-aligned against a common edge.
//!   B — stacked, full width. Today's shape simply given the room: label
//!       above control, every control spanning the column.
//!   C — grouped cards. Section headers outside bordered cards, controls
//!       full width inside them.
//!
//! What is being judged, from #156: the Colour label that currently wraps one
//! letter per line, the six-glyph icon row that breaks 5+1, the helper text
//! that splits a path mid-drive, and the Choose button pinned to the edge.

use gpui::prelude::FluentBuilder;
use gpui::{
    App, AppContext, Bounds, Context, Entity, InteractiveElement, IntoElement, ParentElement,
    Render, StatefulInteractiveElement, Styled, TitlebarOptions, Window, WindowBounds,
    WindowOptions, div, point, px, rgb, size,
};
use gpui_platform::application;
use tiller_theme::Theme;

/// `settings::CONTENT_WIDTH`, which map decision 8 says to reuse.
const CONTENT_WIDTH: f32 = 720.0;
/// The label column in variant A. Wide enough for "Default Worktree Base",
/// which is the longest label on the sheet.
const LABEL_COLUMN: f32 = 180.0;

const GLYPHS: [&str; 6] = ["▢", "⑂", "◍", "▣", "▤", "◈"];
const SWATCHES: [u32; 8] = [
    0xd98b5f, 0xe0b341, 0x5fbf7f, 0xe06b8a, 0x7b8cf0, 0x9a6bd8, 0xe0c341, 0x8a8d99,
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum Variant {
    LabelLeft,
    Stacked,
    Cards,
}

impl Variant {
    fn label(self) -> &'static str {
        match self {
            Variant::LabelLeft => "A · label left",
            Variant::Stacked => "B · stacked",
            Variant::Cards => "C · cards",
        }
    }
}

struct ProjectSettingsProto {
    variant: Variant,
    /// Map fog patch: what the sheet does below 720px. The bar can squeeze the
    /// content to 520px so the answer is visible rather than argued about.
    narrow: bool,
    /// The other fog patch: Remove Project's confirm stacking inside a modal.
    confirming: bool,
}

impl ProjectSettingsProto {
    fn new(_cx: &mut Context<Self>) -> Self {
        Self {
            variant: Variant::LabelLeft,
            narrow: false,
            confirming: false,
        }
    }

    fn width(&self) -> f32 {
        if self.narrow { 520.0 } else { CONTENT_WIDTH }
    }

    fn field(&self, placeholder: &'static str, theme: &Theme) -> impl IntoElement {
        div()
            .w_full()
            .min_w_0()
            .px(px(10.0))
            .py(px(6.0))
            .rounded(px(6.0))
            .border_1()
            .border_color(theme.colors.hairline)
            .text_size(px(13.0))
            .text_color(theme.colors.meta)
            .child(placeholder)
    }

    fn button(&self, label: &'static str, theme: &Theme) -> impl IntoElement {
        div()
            .flex_none()
            .px(px(12.0))
            .py(px(6.0))
            .rounded(px(6.0))
            .border_1()
            .border_color(theme.colors.hairline)
            .text_size(px(13.0))
            .text_color(theme.colors.title)
            .child(label)
    }

    /// The icon card's contents, shared by all three variants: the segmented
    /// control, six glyphs, eight swatches with their label, and Reset.
    fn icon_controls(&self, inline_label: bool, theme: &Theme) -> impl IntoElement {
        let swatches = div()
            .flex()
            .items_center()
            .gap(px(8.0))
            .children(SWATCHES.into_iter().map(|colour| {
                div()
                    .w(px(20.0))
                    .h(px(20.0))
                    .rounded(px(10.0))
                    .bg(rgb(colour))
            }));

        div()
            .flex()
            .flex_col()
            .gap(px(12.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(2.0))
                    .child(self.segment("Icon", true, theme))
                    .child(self.segment("Emoji", false, theme))
                    .child(self.segment("Avatar", false, theme)),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .children(GLYPHS.into_iter().map(|glyph| {
                        div()
                            .w(px(32.0))
                            .h(px(32.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(6.0))
                            .border_1()
                            .border_color(theme.colors.hairline)
                            .text_color(theme.colors.title)
                            .child(glyph)
                    })),
            )
            .child(if inline_label {
                div()
                    .flex()
                    .items_center()
                    .gap(px(12.0))
                    .child(
                        div()
                            .flex_none()
                            .text_size(px(12.0))
                            .text_color(theme.colors.meta)
                            .child("Colour"),
                    )
                    .child(swatches)
                    .into_any_element()
            } else {
                div()
                    .flex()
                    .flex_col()
                    .gap(px(6.0))
                    .child(
                        div()
                            .text_size(px(12.0))
                            .text_color(theme.colors.meta)
                            .child("Colour"),
                    )
                    .child(swatches)
                    .into_any_element()
            })
            .child(
                div()
                    .text_size(px(13.0))
                    .text_color(theme.colors.meta)
                    .child("Reset"),
            )
    }

    fn segment(&self, label: &'static str, active: bool, theme: &Theme) -> impl IntoElement {
        div()
            .px(px(12.0))
            .py(px(4.0))
            .rounded(px(5.0))
            .when(active, |this| this.bg(theme.colors.raised))
            .text_size(px(12.0))
            .text_color(if active {
                theme.colors.title
            } else {
                theme.colors.meta
            })
            .child(label)
    }

    /// A — a fixed label column with controls against a common right edge.
    fn row_label_left(
        &self,
        label: &'static str,
        control: gpui::AnyElement,
        theme: &Theme,
    ) -> impl IntoElement {
        div()
            .flex()
            .items_start()
            .gap(px(16.0))
            .w_full()
            .child(
                div()
                    .w(px(LABEL_COLUMN))
                    .flex_none()
                    .pt(px(6.0))
                    .text_size(px(13.0))
                    .text_color(theme.colors.meta)
                    .child(label),
            )
            .child(div().flex_1().min_w_0().child(control))
    }

    fn body(&self, theme: &Theme) -> gpui::AnyElement {
        match self.variant {
            Variant::LabelLeft => div()
                .flex()
                .flex_col()
                .gap(px(18.0))
                .child(self.row_label_left(
                    "Display name",
                    self.field("tiller", theme).into_any_element(),
                    theme,
                ))
                .child(self.row_label_left(
                    "Project icon",
                    self.icon_controls(true, theme).into_any_element(),
                    theme,
                ))
                .child(self.row_label_left(
                    "Default Worktree Base",
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(6.0))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(12.0))
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .text_size(px(13.0))
                                        .text_color(theme.colors.title)
                                        .child("main"),
                                )
                                .child(self.button("Use Primary", theme)),
                        )
                        .child(
                            div()
                                .text_size(px(11.0))
                                .text_color(theme.colors.meta)
                                .child("Following primary branch (main)"),
                        )
                        .child(self.field("Search branches by name…", theme))
                        .into_any_element(),
                    theme,
                ))
                .child(self.row_label_left(
                    "Worktree Location",
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(6.0))
                        .child(
                            div()
                                .text_size(px(11.0))
                                .text_color(theme.colors.meta)
                                .child(
                                    "Parent folder for new worktrees. \
                                     Empty uses the default: D:\\Progetti\\tiller",
                                ),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(10.0))
                                .child(div().flex_1().min_w_0().child(
                                    self.field("D:\\Progetti\\tiller", theme),
                                ))
                                .child(self.button("Choose…", theme)),
                        )
                        .into_any_element(),
                    theme,
                ))
                .into_any_element(),

            Variant::Stacked => div()
                .flex()
                .flex_col()
                .gap(px(18.0))
                .children([
                    ("Display name", self.field("tiller", theme).into_any_element()),
                    (
                        "Project icon",
                        self.icon_controls(false, theme).into_any_element(),
                    ),
                ]
                .into_iter()
                .map(|(label, control)| {
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(6.0))
                        .child(
                            div()
                                .text_size(px(13.0))
                                .text_color(theme.colors.meta)
                                .child(label),
                        )
                        .child(control)
                }))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(6.0))
                        .child(
                            div()
                                .text_size(px(13.0))
                                .text_color(theme.colors.meta)
                                .child("Default Worktree Base"),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(12.0))
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .text_size(px(13.0))
                                        .text_color(theme.colors.title)
                                        .child("main"),
                                )
                                .child(self.button("Use Primary", theme)),
                        )
                        .child(
                            div()
                                .text_size(px(11.0))
                                .text_color(theme.colors.meta)
                                .child("Following primary branch (main)"),
                        )
                        .child(self.field("Search branches by name…", theme)),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(6.0))
                        .child(
                            div()
                                .text_size(px(13.0))
                                .text_color(theme.colors.meta)
                                .child("Worktree Location"),
                        )
                        .child(div().text_size(px(11.0)).text_color(theme.colors.meta).child(
                            "Parent folder for new worktrees. Empty uses the default: \
                             D:\\Progetti\\tiller",
                        ))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(10.0))
                                .child(div().flex_1().min_w_0().child(
                                    self.field("D:\\Progetti\\tiller", theme),
                                ))
                                .child(self.button("Choose…", theme)),
                        ),
                )
                .into_any_element(),

            Variant::Cards => div()
                .flex()
                .flex_col()
                .gap(px(20.0))
                .child(self.card(
                    "Identity",
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(14.0))
                        .child(self.field("tiller", theme))
                        .child(self.icon_controls(true, theme))
                        .into_any_element(),
                    theme,
                ))
                .child(self.card(
                    "Worktrees",
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(12.0))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(12.0))
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .text_size(px(13.0))
                                        .text_color(theme.colors.title)
                                        .child("main · following primary branch"),
                                )
                                .child(self.button("Use Primary", theme)),
                        )
                        .child(self.field("Search branches by name…", theme))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(10.0))
                                .child(div().flex_1().min_w_0().child(
                                    self.field("D:\\Progetti\\tiller", theme),
                                ))
                                .child(self.button("Choose…", theme)),
                        )
                        .child(div().text_size(px(11.0)).text_color(theme.colors.meta).child(
                            "Parent folder for new worktrees. Empty uses the default: \
                             D:\\Progetti\\tiller",
                        ))
                        .into_any_element(),
                    theme,
                ))
                .into_any_element(),
        }
    }

    fn card(
        &self,
        title: &'static str,
        body: gpui::AnyElement,
        theme: &Theme,
    ) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(
                div()
                    .text_size(px(12.0))
                    .text_color(theme.colors.meta)
                    .child(title),
            )
            .child(
                div()
                    .w_full()
                    .min_w_0()
                    .p(px(16.0))
                    .rounded(px(8.0))
                    .border_1()
                    .border_color(theme.colors.hairline)
                    .child(body),
            )
    }

    fn bottom_bar(&self, cx: &mut Context<Self>, theme: &Theme) -> impl IntoElement {
        let current = self.variant;
        let narrow = self.narrow;
        let confirming = self.confirming;
        div()
            .flex()
            .items_center()
            .gap(px(4.0))
            .px(px(10.0))
            .h(px(36.0))
            .border_t_1()
            .border_color(theme.colors.hairline)
            .children(
                [Variant::LabelLeft, Variant::Stacked, Variant::Cards]
                    .into_iter()
                    .map(|variant| {
                        div()
                            .id(variant.label())
                            .px(px(8.0))
                            .py(px(3.0))
                            .rounded(px(4.0))
                            .text_size(px(11.0))
                            .when(variant == current, |this| this.bg(theme.colors.raised))
                            .text_color(if variant == current {
                                theme.colors.title
                            } else {
                                theme.colors.meta
                            })
                            .child(variant.label())
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.variant = variant;
                                cx.notify();
                            }))
                    }),
            )
            .child(
                div()
                    .id("narrow")
                    .ml(px(8.0))
                    .px(px(8.0))
                    .py(px(3.0))
                    .rounded(px(4.0))
                    .text_size(px(11.0))
                    .when(narrow, |this| this.bg(theme.colors.raised))
                    .text_color(theme.colors.meta)
                    .child("520px")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.narrow = !this.narrow;
                        cx.notify();
                    })),
            )
            .child(
                div()
                    .id("confirm")
                    .px(px(8.0))
                    .py(px(3.0))
                    .rounded(px(4.0))
                    .text_size(px(11.0))
                    .when(confirming, |this| this.bg(theme.colors.raised))
                    .text_color(theme.colors.meta)
                    .child("remove-confirm")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.confirming = !this.confirming;
                        cx.notify();
                    })),
            )
    }
}

impl Render for ProjectSettingsProto {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *Theme::get(cx);
        let width = self.width();

        let sheet = div()
            .id("project-settings-sheet")
            .w(px(width))
            .max_h(px(620.0))
            .overflow_y_scroll()
            .p(px(24.0))
            .rounded(px(10.0))
            .border_1()
            .border_color(theme.colors.hairline)
            .bg(theme.colors.background)
            .flex()
            .flex_col()
            .gap(px(20.0))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .child(
                        div()
                            .text_size(px(16.0))
                            .text_color(theme.colors.title)
                            .child("Project Settings · tiller"),
                    )
                    .child(
                        div()
                            .text_size(px(12.0))
                            .text_color(theme.colors.meta)
                            .child("D:\\Progetti\\tiller\\tiller"),
                    )
                    .child(
                        div()
                            .text_size(px(12.0))
                            .text_color(theme.colors.meta)
                            .child("Repository: Git"),
                    ),
            )
            .child(self.body(&theme))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .pt(px(8.0))
                    .child(
                        div()
                            .text_size(px(13.0))
                            .text_color(rgb(0xe06b8a))
                            .child("Remove Project"),
                    )
                    .child(self.button("Close", &theme)),
            )
            .when(self.confirming, |this| {
                this.child(
                    div()
                        .w_full()
                        .p(px(14.0))
                        .rounded(px(8.0))
                        .border_1()
                        .border_color(rgb(0xe06b8a))
                        .flex()
                        .flex_col()
                        .gap(px(8.0))
                        .child(
                            div()
                                .text_size(px(13.0))
                                .text_color(theme.colors.title)
                                .child("Remove tiller from Tiller?"),
                        )
                        .child(
                            div().text_size(px(11.0)).text_color(theme.colors.meta).child(
                                "The checkout stays on disk. Its threads and tabs are forgotten.",
                            ),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(8.0))
                                .child(self.button("Cancel", &theme))
                                .child(self.button("Remove", &theme)),
                        ),
                )
            });

        div()
            .size_full()
            .bg(theme.colors.raised)
            .flex()
            .flex_col()
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(sheet),
            )
            .child(self.bottom_bar(cx, &theme))
    }
}

fn main() {
    application().run(|cx: &mut App| {
        Theme::init(cx);
        let bounds = Bounds::centered(None, size(px(880.0), px(800.0)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(TitlebarOptions {
                    appears_transparent: false,
                    traffic_light_position: Some(point(px(12.0), px(12.0))),
                    ..Default::default()
                }),
                ..Default::default()
            },
            |_, cx| -> Entity<ProjectSettingsProto> { cx.new(ProjectSettingsProto::new) },
        )
        .expect("open project settings prototype window");
        cx.activate(true);
    });
}
