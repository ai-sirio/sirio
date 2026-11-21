//! Native probe for the horizontal scroll a wide fenced block and a wide table get under a
//! Markdown body drawn through Ely's renderer — the body the file view's Preview, change
//! requests and the chat share.
//!
//! Two documents sit in a column, each under an element id of its own the way
//! chat entries do. Document A is a `json` fence of one very long line, then a
//! table of eight columns whose floors (96 px each) exceed the width, so both
//! blocks must scroll sideways. Document B is the same fence alone, which is
//! how the script tells that scrolling A's block did not scroll B's.
//!
//! Document A sits in a wrapper that, like the chat's `MarkdownBody`, reacts
//! to any left mouse-down over the response; it prints `PRESS doc-a <x> <y>`.
//! A press on a scrollbar's thumb must never reach it, a press on text must.
//!
//! `Scripts/Tests/test-markdown-scroll-probe.sh` reads the frame and this
//! output, so both are a contract: a 560×560 window with 24 px of padding, so
//! each block is 512 px wide from x 24; a 16 px gap between the documents; and
//! one `BLOCK <doc a|b> <ix> <x> <y> <w> <h>` line on stdout (window
//! coordinates, rounded) whenever the set of block bounds changes; `markdown_probe_view`
//! prints them from the sideways frames it draws. Document A has blocks 0 (the fence) and
//! 1 (the table), document B block 0.
//!
//! Env: `MD_PROBE_APPEARANCE` (`light`, otherwise dark).
use std::io::Write;

use gpui::{
    App, Bounds, Context, MouseButton, Window, WindowBounds, WindowOptions, div, prelude::*, px,
    size,
};

const JSON: &str = "```json\n{\"content\":{\"informazioniAggiuntive\":{\"codiceCliente\":\"0000123456\",\
\"descrizioneEstesa\":\"pratica di rimborso chilometrico\",\"importoTotale\":1234.56,\"valuta\":\"EUR\",\
\"note\":[\"verificato\",\"approvato\",\"in attesa di pagamento\"]},\"allKmPaidByPAS\":true,\
\"tratte\":[{\"da\":\"Milano\",\"a\":\"Torino\",\"km\":143},{\"da\":\"Torino\",\"a\":\"Genova\",\"km\":170}],\
\"ultimaRiga\":\"fine\"}}\n```";

// Eight columns, each as wide as its longest word: Ely draws a table at its natural
// width, so what makes it scroll is the width of its cells, not a floor per column (bezel
// gave each column 96 px). Roughly 125 px a column is 1000 px, past the 512 px body.
const TABLE: &str = "| Alpha-column | Bravo-column | Charlie-column | Delta-column | Echo-column | Foxtrot-column | Golf-column | Hotel-column |\n\
|---|---|---|---|---|---|---|---|\n\
| a1-cell-text | b1-cell-text | c1-cell-text | d1-cell-text | e1-cell-text | f1-cell-text | g1-cell-text | h1-cell-text |\n\
| a2-cell-text | b2-cell-text | c2-cell-text | d2-cell-text | e2-cell-text | f2-cell-text | g2-cell-text | h2-cell-text |";

struct Probe {
    doc_a: String,
    doc_b: String,
}

impl Render for Probe {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *sirio_theme::Theme::get(cx);
        div()
            .size_full()
            .p(px(24.0))
            .bg(theme.ely.bg)
            .text_color(theme.ely.fg)
            .flex()
            .flex_col()
            .gap(px(16.0))
            .child(
                div()
                    .id("doc-a")
                    .on_mouse_down(MouseButton::Left, |event, _, _| {
                        let mut out = std::io::stdout().lock();
                        let _ = writeln!(
                            out,
                            "PRESS doc-a {} {}",
                            f32::from(event.position.x).round() as i64,
                            f32::from(event.position.y).round() as i64
                        );
                        let _ = out.flush();
                    })
                    .child(sirio_ui::markdown_probe_view("a", &self.doc_a)),
            )
            .child(
                div()
                    .id("doc-b")
                    .child(sirio_ui::markdown_probe_view("b", &self.doc_b)),
            )
    }
}

fn main() {
    sirio_ui::app_platform::application()
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
            let bounds = Bounds::centered(None, size(px(560.0), px(560.0)), cx);
            cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    ..Default::default()
                },
                |window, cx| {
                    window.set_window_title("Markdown Scroll Probe");
                    cx.new(|_| Probe {
                        doc_a: format!("{JSON}\n\n{TABLE}"),
                        doc_b: JSON.to_string(),
                    })
                },
            )
            .unwrap();
            cx.activate(true);
        });
}
