//! Native probe for the selection bezel-markdown paints under a rendered
//! document — the painter a chat response and the file view's Preview share.
//!
//! The whole document is selected, so every row of it should be washed from
//! its first glyph. The window is narrow, so the first paragraph soft-wraps;
//! a markdown line break (`\` at the end of a line) inside it makes a second
//! line of the same shaped text, which soft-wraps too; the inline code span
//! straddles a wrap; and the second paragraph is a block of its own. Every
//! way a row can start.
//!
//! `Scripts/Tests/test-markdown-selection-probe.sh` reads the frame, so the
//! layout is a contract: a 520×360 window and 24 px of padding, which puts
//! every row's first glyph at x 24.
//!
//! Env: `MD_PROBE_APPEARANCE` (`light`, otherwise dark).
use gpui::{
    App, Bounds, Context, Window, WindowBounds, WindowOptions, div, prelude::*, px, size,
};

const SOURCE: &str = "Autocompact is thrashing: the context refilled to the limit within 3 turns \
of the previous compact, 3 times in a row.\\\nA file being read or a tool output is likely too \
large for the context window. Try reading in smaller chunks, or use `/clear to start fresh in a new session` \
instead.\n\nSecond paragraph, a block of its own.";

struct Probe {
    document: markdown::Doc,
}

impl Render for Probe {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *sirio_theme::Theme::get(cx);
        let selection = markdown::Selection::all(&self.document);
        let rendered = markdown::render_with_selection(
            &self.document,
            Some(selection),
            None,
            None,
            markdown::Caption::Shown,
            window,
            cx,
        );
        div()
            .size_full()
            .p(px(24.0))
            .bg(theme.ely.bg)
            .text_color(theme.ely.fg)
            .child(rendered)
    }
}

fn main() {
    gpui_platform::application()
        .with_assets(sirio_ui::ely::AppAssets)
        .run(|cx: &mut App| {
            sirio_theme::register_ui_fonts(cx).expect("Sirio fonts");
            sirio_theme::Theme::init(cx);
            sirio_ui::chat::init(cx);
            let mode = if std::env::var("MD_PROBE_APPEARANCE").as_deref() == Ok("light") {
                sirio_theme::ThemeMode::Light
            } else {
                sirio_theme::ThemeMode::Dark
            };
            sirio_theme::Theme::set_mode(mode, cx);
            sirio_ui::ely::init(cx);
            let bounds = Bounds::centered(None, size(px(520.0), px(360.0)), cx);
            cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    ..Default::default()
                },
                |window, cx| {
                    window.set_window_title("Markdown Selection Probe");
                    cx.new(|_| Probe {
                        document: markdown::parse(SOURCE),
                    })
                },
            )
            .unwrap();
            cx.activate(true);
        });
}
