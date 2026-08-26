//! PROTOTYPE — throwaway. Three variants of the Threads view's list and row,
//! at the sidebar's real 280px width, switchable from a floating bottom bar.
//! Answers #164 for the map in #160. Not production code: no tests, no state,
//! no persistence, fixtures inline.
//!
//! Run: `cargo run -p tiller_ui --example threads_view_proto`
//!
//! The question: map decision 6 puts two lines on every thread row —
//! `[agent icon] Title`, then `project · branch · relative time` — plus a
//! hover delete and a live status glyph competing for the right edge. That is
//! a lot for 280px, and #151 exists because the current rows already overflow
//! it. These three are deliberately structurally different, not restyled:
//!
//!   A — two-line flat, the map as written (and Zed's own shape)
//!   B — one-line dense, metadata right-aligned on the same line
//!   C — grouped tree, metadata promoted into the headers so the row is bare
//!
//! Everything is rendered at exactly SIDEBAR_WIDTH so the truncation is real.

use gpui::prelude::FluentBuilder;
use gpui::{
    App, AppContext, Bounds, Context, Entity, InteractiveElement, IntoElement, ParentElement,
    Render, StatefulInteractiveElement, Styled, TitlebarOptions, Window, WindowBounds,
    WindowOptions, div, point, px, size,
};
use gpui_platform::application;
use tiller_theme::Theme;

/// The sidebar's real default width (`main.rs:21691`), which is the whole
/// point of the exercise.
const SIDEBAR_WIDTH: f32 = 280.0;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Variant {
    TwoLineFlat,
    OneLineDense,
    GroupedTree,
}

impl Variant {
    fn label(self) -> &'static str {
        match self {
            Variant::TwoLineFlat => "A · two-line",
            Variant::OneLineDense => "B · one-line",
            Variant::GroupedTree => "C · grouped",
        }
    }
}

/// One fixture thread. Deliberately includes the hard cases named in #164:
/// a long branch, a degenerate title, a running thread, and a bucket of one.
struct Thread {
    agent: &'static str,
    title: &'static str,
    /// The first-user-message snippet shown when the title is degenerate
    /// (decision 21: `auto_naming` off leaves the title as the agent's name).
    snippet: Option<&'static str>,
    project: &'static str,
    branch: &'static str,
    age: &'static str,
    running: bool,
}

fn buckets() -> Vec<(&'static str, Vec<Thread>)> {
    vec![
        (
            "Today",
            vec![
                Thread {
                    agent: "✳",
                    title: "Tiller UI bugs #59 e #58",
                    snippet: None,
                    project: "tiller",
                    branch: "main",
                    age: "2m",
                    running: true,
                },
                Thread {
                    agent: "◍",
                    title: "Claude Code",
                    snippet: Some("why does the composer keep its width when I…"),
                    project: "tiller",
                    branch: "feature/some-long-branch-name",
                    age: "18m",
                    running: false,
                },
                Thread {
                    agent: "◆",
                    title: "Persist tool locations across a restart",
                    snippet: None,
                    project: "tiller-experiments",
                    branch: "fix/168-persist-tool-locations",
                    age: "2h",
                    running: false,
                },
            ],
        ),
        (
            "Yesterday",
            vec![Thread {
                agent: "✳",
                title: "Webview scale factor",
                snippet: None,
                project: "tiller",
                branch: "fix/143-webview-scale",
                age: "1d",
                running: false,
            }],
        ),
    ]
}

struct ThreadsProto {
    variant: Variant,
    /// Which row the pointer is over, as `(bucket, row)`. The prototype fakes
    /// hover rather than wiring real events: the question is what the right
    /// edge does when a *running* row is hovered, and that is easier to judge
    /// pinned than chased with a mouse.
    hovered: Option<(usize, usize)>,
    empty: bool,
}

impl ThreadsProto {
    fn new(_cx: &mut Context<Self>) -> Self {
        Self {
            variant: Variant::TwoLineFlat,
            hovered: Some((0, 0)),
            empty: false,
        }
    }

    fn header(&self, theme: &Theme) -> impl IntoElement {
        // Decision 8: two icons plus a `+` plus a label that switches
        // Projects / Threads, all inside FILTER_LEFT_INSET.
        div()
            .flex()
            .items_center()
            .gap(px(6.0))
            .px(px(12.0))
            .h(px(34.0))
            .child(
                div()
                    .text_color(theme.colors.meta)
                    .text_size(px(13.0))
                    .child("▤"),
            )
            .child(
                div()
                    .text_color(theme.colors.title)
                    .text_size(px(13.0))
                    .child("☰"),
            )
            .child(
                div()
                    .flex_1()
                    .text_color(theme.colors.title)
                    .text_size(px(13.0))
                    .child("Threads"),
            )
            .child(
                div()
                    .text_color(theme.colors.meta)
                    .text_size(px(15.0))
                    .child("+"),
            )
    }

    /// A — the map as written, and the shape Zed uses: agent glyph + title,
    /// then `project · branch · relative time` underneath.
    fn row_two_line(&self, t: &Thread, hovered: bool, theme: &Theme) -> impl IntoElement {
        let meta = format!("{} · {} · {}", t.project, t.branch, t.age);
        div()
            .flex()
            .flex_col()
            .gap(px(2.0))
            .px(px(12.0))
            .py(px(6.0))
            .when(hovered, |this| this.bg(theme.colors.raised))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(7.0))
                    .child(
                        div()
                            .flex_none()
                            .text_color(theme.colors.meta)
                            .child(t.agent),
                    )
                    .child(
                        div()
                            .min_w_0()
                            .flex_1()
                            .truncate()
                            .text_size(px(13.0))
                            .text_color(theme.colors.title)
                            .child(t.title),
                    )
                    .when(t.running && !hovered, |this| {
                        this.child(div().flex_none().text_color(theme.colors.meta).child("●"))
                    })
                    .when(hovered, |this| {
                        this.child(div().flex_none().text_color(theme.colors.meta).child("×"))
                    }),
            )
            .child(
                div()
                    .pl(px(20.0))
                    .min_w_0()
                    .truncate()
                    .text_size(px(11.0))
                    .text_color(theme.colors.meta)
                    .child(meta),
            )
            .when_some(t.snippet, |this, snippet| {
                this.child(
                    div()
                        .pl(px(20.0))
                        .min_w_0()
                        .truncate()
                        .text_size(px(11.0))
                        .text_color(theme.colors.meta)
                        .child(snippet),
                )
            })
    }

    /// B — one line. The age moves to the right edge and the row drops
    /// project/branch entirely, betting that the bucket header plus the
    /// selected project already carry them.
    fn row_one_line(&self, t: &Thread, hovered: bool, theme: &Theme) -> impl IntoElement {
        div()
            .flex()
            .items_center()
            .gap(px(7.0))
            .px(px(12.0))
            .py(px(7.0))
            .when(hovered, |this| this.bg(theme.colors.raised))
            .child(
                div()
                    .flex_none()
                    .text_color(theme.colors.meta)
                    .child(t.agent),
            )
            .child(
                div()
                    .min_w_0()
                    .flex_1()
                    .truncate()
                    .text_size(px(13.0))
                    .text_color(theme.colors.title)
                    .child(if t.title == "Claude Code" {
                        t.snippet.unwrap_or(t.title)
                    } else {
                        t.title
                    }),
            )
            .child(
                div()
                    .flex_none()
                    .text_size(px(11.0))
                    .text_color(theme.colors.meta)
                    .child(if hovered {
                        "×".to_string()
                    } else if t.running {
                        "●".to_string()
                    } else {
                        t.age.to_string()
                    }),
            )
    }

    /// C — the metadata becomes the structure. Threads nest under a
    /// `project / branch` header, so the row itself carries only glyph,
    /// title and status and never competes for width.
    fn row_grouped(&self, t: &Thread, hovered: bool, theme: &Theme) -> impl IntoElement {
        div()
            .flex()
            .items_center()
            .gap(px(7.0))
            .pl(px(24.0))
            .pr(px(12.0))
            .py(px(6.0))
            .when(hovered, |this| this.bg(theme.colors.raised))
            .child(
                div()
                    .flex_none()
                    .text_color(theme.colors.meta)
                    .child(t.agent),
            )
            .child(
                div()
                    .min_w_0()
                    .flex_1()
                    .truncate()
                    .text_size(px(13.0))
                    .text_color(theme.colors.title)
                    .child(t.title),
            )
            .when(t.running && !hovered, |this| {
                this.child(div().flex_none().text_color(theme.colors.meta).child("●"))
            })
            .when(hovered, |this| {
                this.child(div().flex_none().text_color(theme.colors.meta).child("×"))
            })
    }

    fn empty_state(&self, theme: &Theme) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .gap(px(4.0))
            .px(px(12.0))
            .py(px(24.0))
            .child(
                div()
                    .text_size(px(13.0))
                    .text_color(theme.colors.title)
                    .child("No threads yet"),
            )
            .child(
                div()
                    .text_size(px(11.0))
                    .text_color(theme.colors.meta)
                    .child("Start one from a worktree and it shows up here."),
            )
    }

    fn bottom_bar(&self, cx: &mut Context<Self>, theme: &Theme) -> impl IntoElement {
        let current = self.variant;
        let empty = self.empty;
        div()
            .flex()
            .items_center()
            .gap(px(4.0))
            .px(px(8.0))
            .h(px(34.0))
            .border_t_1()
            .border_color(theme.colors.hairline)
            .children(
                [
                    Variant::TwoLineFlat,
                    Variant::OneLineDense,
                    Variant::GroupedTree,
                ]
                .into_iter()
                .map(|variant| {
                    div()
                        .id(variant.label())
                        .px(px(6.0))
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
                    .id("empty")
                    .ml(px(4.0))
                    .px(px(6.0))
                    .py(px(3.0))
                    .rounded(px(4.0))
                    .text_size(px(11.0))
                    .when(empty, |this| this.bg(theme.colors.raised))
                    .text_color(theme.colors.meta)
                    .child("empty")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.empty = !this.empty;
                        cx.notify();
                    })),
            )
    }
}

impl Render for ThreadsProto {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *Theme::get(cx);
        let variant = self.variant;
        let hovered = self.hovered;

        let mut list = div().flex().flex_col().w_full().min_w_0();
        if self.empty {
            list = list.child(self.empty_state(&theme));
        } else {
            for (bucket_index, (label, threads)) in buckets().into_iter().enumerate() {
                let header_text: String = match variant {
                    Variant::GroupedTree => {
                        let first = &threads[0];
                        format!("{} / {}", first.project, first.branch)
                    }
                    _ => label.to_string(),
                };
                list = list.child(
                    div()
                        .px(px(12.0))
                        .pt(px(10.0))
                        .pb(px(4.0))
                        .min_w_0()
                        .truncate()
                        .text_size(px(11.0))
                        .text_color(theme.colors.meta)
                        .child(header_text),
                );
                for (row_index, thread) in threads.iter().enumerate() {
                    let is_hovered = hovered == Some((bucket_index, row_index));
                    list = list.child(match variant {
                        Variant::TwoLineFlat => self
                            .row_two_line(thread, is_hovered, &theme)
                            .into_any_element(),
                        Variant::OneLineDense => self
                            .row_one_line(thread, is_hovered, &theme)
                            .into_any_element(),
                        Variant::GroupedTree => self
                            .row_grouped(thread, is_hovered, &theme)
                            .into_any_element(),
                    });
                }
            }
        }

        div()
            .flex()
            .flex_col()
            .w(px(SIDEBAR_WIDTH))
            .h_full()
            .bg(theme.colors.background)
            .border_r_1()
            .border_color(theme.colors.hairline)
            .child(self.header(&theme))
            .child(
                div()
                    .mx(px(12.0))
                    .mb(px(4.0))
                    .px(px(8.0))
                    .py(px(4.0))
                    .rounded(px(6.0))
                    .border_1()
                    .border_color(theme.colors.hairline)
                    .text_size(px(12.0))
                    .text_color(theme.colors.meta)
                    .child("Search threads…"),
            )
            .child(div().flex_1().min_h_0().child(list))
            .child(self.bottom_bar(cx, &theme))
    }
}

fn main() {
    application().run(|cx: &mut App| {
        Theme::init(cx);
        let bounds = Bounds::centered(None, size(px(SIDEBAR_WIDTH), px(720.0)), cx);
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
            |_, cx| -> Entity<ThreadsProto> { cx.new(ThreadsProto::new) },
        )
        .expect("open threads prototype window");
        cx.activate(true);
    });
}
