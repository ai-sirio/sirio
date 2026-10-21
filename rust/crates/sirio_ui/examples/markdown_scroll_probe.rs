//! Native probe for the horizontal scroll bezel-markdown gives a wide fenced
//! block and a wide table under a rendered document — the painter a chat
//! response and the file view's Preview share.
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
//! coordinates, rounded) whenever the set of block bounds changes. Document A
//! has blocks 0 (the fence) and 1 (the table), document B block 0.
//!
//! Env: `MD_PROBE_APPEARANCE` (`light`, otherwise dark).
use std::cell::RefCell;
use std::io::Write;
use std::rc::Rc;

use gpui::{
    App, Bounds, Context, MouseButton, Window, WindowBounds, WindowOptions, canvas, div,
    prelude::*, px, size,
};

const JSON: &str = "```json\n{\"content\":{\"informazioniAggiuntive\":{\"codiceCliente\":\"0000123456\",\
\"descrizioneEstesa\":\"pratica di rimborso chilometrico\",\"importoTotale\":1234.56,\"valuta\":\"EUR\",\
\"note\":[\"verificato\",\"approvato\",\"in attesa di pagamento\"]},\"allKmPaidByPAS\":true,\
\"tratte\":[{\"da\":\"Milano\",\"a\":\"Torino\",\"km\":143},{\"da\":\"Torino\",\"a\":\"Genova\",\"km\":170}],\
\"ultimaRiga\":\"fine\"}}\n```";

const TABLE: &str = "| Alpha | Bravo | Charlie | Delta | Echo | Foxtrot | Golf | Hotel |\n\
|---|---|---|---|---|---|---|---|\n\
| a1 | b1 | c1 | d1 | e1 | f1 | g1 | h1 |\n\
| a2 | b2 | c2 | d2 | e2 | f2 | g2 | h2 |";

type Bounds4 = (i64, i64, i64, i64);

struct Probe {
    doc_a: markdown::Doc,
    doc_b: markdown::Doc,
    layouts_a: markdown::BlockLayouts,
    layouts_b: markdown::BlockLayouts,
    printed: Rc<RefCell<Vec<(char, usize, Bounds4)>>>,
}

fn rounded(bounds: Bounds<gpui::Pixels>) -> Bounds4 {
    let r = |v: gpui::Pixels| f32::from(v).round() as i64;
    (
        r(bounds.origin.x),
        r(bounds.origin.y),
        r(bounds.size.width),
        r(bounds.size.height),
    )
}

impl Render for Probe {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *sirio_theme::Theme::get(cx);
        let render = |doc: &markdown::Doc,
                      layouts: &markdown::BlockLayouts,
                      window: &mut Window,
                      cx: &mut Context<Self>| {
            markdown::render_with_selection(
                doc,
                None,
                Some(layouts),
                None,
                markdown::Caption::Shown,
                window,
                cx,
            )
        };
        let a = render(&self.doc_a, &self.layouts_a, window, cx);
        let b = render(&self.doc_b, &self.layouts_b, window, cx);

        // Last child, so its paint runs after every block recorded its box.
        let report = {
            let printed = self.printed.clone();
            let layouts = [
                ('a', self.layouts_a.clone(), self.doc_a.blocks.len()),
                ('b', self.layouts_b.clone(), self.doc_b.blocks.len()),
            ];
            canvas(
                |_, _, _| (),
                move |_, _, window, _| {
                    let mut now = Vec::new();
                    let mut missing = false;
                    for (name, layouts, blocks) in &layouts {
                        for ix in 0..*blocks {
                            match layouts.block_bounds(ix) {
                                Some(bounds) => now.push((*name, ix, rounded(bounds))),
                                None => missing = true,
                            }
                        }
                    }
                    if *printed.borrow() != now {
                        let mut out = std::io::stdout().lock();
                        for (name, ix, (x, y, w, h)) in &now {
                            let _ = writeln!(out, "BLOCK doc {name} {ix} {x} {y} {w} {h}");
                        }
                        let _ = out.flush();
                        *printed.borrow_mut() = now;
                    }
                    if missing {
                        window.request_animation_frame();
                    }
                },
            )
            .absolute()
            .size(px(0.0))
        };

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
                    .child(a),
            )
            .child(div().id("doc-b").child(b))
            .child(report)
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
            let bounds = Bounds::centered(None, size(px(560.0), px(560.0)), cx);
            cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    ..Default::default()
                },
                |window, cx| {
                    window.set_window_title("Markdown Scroll Probe");
                    cx.new(|_| Probe {
                        doc_a: markdown::parse(&format!("{JSON}\n\n{TABLE}")),
                        doc_b: markdown::parse(JSON),
                        layouts_a: markdown::BlockLayouts::default(),
                        layouts_b: markdown::BlockLayouts::default(),
                        printed: Rc::new(RefCell::new(Vec::new())),
                    })
                },
            )
            .unwrap();
            cx.activate(true);
        });
}
