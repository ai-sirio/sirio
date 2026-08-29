//! **PROTOTYPE — throwaway.** Non fa parte del prodotto: risponde alla domanda
//! di <https://github.com/ai-sirio/sirio/issues/282> e poi va buttato.
//!
//! Domanda: gpui su Windows risolve davvero la famiglia `Segoe Fluent Icons`,
//! e rende i quattro glifi dei caption button? E cosa succede quando la
//! famiglia richiesta **non esiste** — `font_fallbacks` fa da rete, o no?
//!
//! Run: `cargo run -p sirio_ui --bin prototype_caption_glyphs`
//!
//! Cosa guardare nella finestra:
//! - riga 1 e 2: se i glifi appaiono, la famiglia risolve;
//! - riga 3: famiglia inesistente, nessun fallback — è il caso Windows 10
//!   senza `Segoe Fluent Icons`. Rettangolo vuoto/niente = fallimento silenzioso;
//! - riga 4: stessa famiglia inesistente **con** `font_fallbacks`. Se qui i
//!   glifi compaiono, i fallback coprono una famiglia assente. Se resta
//!   uguale alla riga 3, non la coprono.
//! - riga 5: controllo negativo — famiglia valida, codepoint che nessuno ha.

use gpui::{
    App, AppContext as _, Bounds, Context, Font, FontFallbacks, IntoElement, Render, SharedString,
    Window, WindowBounds, WindowOptions, div, font, prelude::*, px, rgb, size,
};

/// I quattro glifi che Zed usa in `platform_windows.rs`.
const MINIMIZE: &str = "\u{e921}";
const MAXIMIZE: &str = "\u{e922}";
const RESTORE: &str = "\u{e923}";
const CLOSE: &str = "\u{e8bb}";
const GLYPHS: [&str; 4] = [MINIMIZE, MAXIMIZE, RESTORE, CLOSE];

const FLUENT: &str = "Segoe Fluent Icons";
const MDL2: &str = "Segoe MDL2 Assets";
/// Deliberatamente inesistente: simula Windows 10, dove `Segoe Fluent Icons`
/// non è installato.
const ABSENT: &str = "Sirio Prototype Nonexistent Family";

struct GlyphProbe {
    /// Cosa `TextSystem::all_font_names()` riporta per le famiglie che ci
    /// interessano — la via alternativa a `RtlGetVersion` per sapere se il
    /// font c'è.
    fluent_seen: bool,
    mdl2_seen: bool,
    absent_seen: bool,
    total_families: usize,
}

fn probe_row(
    label: &str,
    detail: &str,
    family: &'static str,
    fallbacks: Option<Vec<String>>,
    glyphs: [&'static str; 4],
) -> impl IntoElement {
    let mut probe_font: Font = font(family);
    probe_font.fallbacks = fallbacks.map(FontFallbacks::from_fonts);

    let buttons_font = probe_font.clone();
    let large_font = probe_font.clone();

    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(px(20.))
        .child(
            div()
                .w(px(300.))
                .flex()
                .flex_col()
                .gap(px(2.))
                .child(
                    div()
                        .text_size(px(13.))
                        .child(SharedString::from(label.to_owned())),
                )
                .child(
                    div()
                        .text_size(px(11.))
                        .text_color(rgb(0x999999))
                        .child(SharedString::from(detail.to_owned())),
                ),
        )
        // A misura reale: 36 x 38, text_size 10 — la geometria decisa nella mappa.
        .child(
            div()
                .flex()
                .flex_row()
                .font(buttons_font)
                .text_size(px(10.))
                .children(glyphs.map(|glyph| {
                    div()
                        .w(px(36.))
                        .h(px(38.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .bg(rgb(0x2c2c2c))
                        .child(SharedString::from(glyph))
                })),
        )
        // Ingranditi, per vedere la forma del glifo e non solo se c'è.
        .child(
            div()
                .flex()
                .flex_row()
                .gap(px(14.))
                .font(large_font)
                .text_size(px(30.))
                .text_color(rgb(0xffcc66))
                .children(glyphs.map(|glyph| div().child(SharedString::from(glyph)))),
        )
}

impl Render for GlyphProbe {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let yes_no = |seen: bool| if seen { "SÌ" } else { "NO" };

        div()
            .size_full()
            .bg(rgb(0x1b1b1b))
            .text_color(rgb(0xf0f0f0))
            .p(px(24.))
            .flex()
            .flex_col()
            .gap(px(18.))
            .child(
                div()
                    .text_size(px(16.))
                    .child("PROTOTIPO — i glifi dei caption button Windows in gpui"),
            )
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(rgb(0x9ecbff))
                    .child(SharedString::from(format!(
                        "text_system().all_font_names(): {} famiglie · \"{FLUENT}\" presente: {} · \"{MDL2}\" presente: {} · famiglia finta presente: {}",
                        self.total_families,
                        yes_no(self.fluent_seen),
                        yes_no(self.mdl2_seen),
                        yes_no(self.absent_seen),
                    ))),
            )
            .child(probe_row(
                "1. Segoe Fluent Icons",
                "la scelta di Zed su Win11 (build >= 22000)",
                FLUENT,
                None,
                GLYPHS,
            ))
            .child(probe_row(
                "2. Segoe MDL2 Assets",
                "la scelta di Zed su Win10",
                MDL2,
                None,
                GLYPHS,
            ))
            .child(probe_row(
                "3. Famiglia inesistente, senza fallback",
                "simula Win10 senza Segoe Fluent Icons",
                ABSENT,
                None,
                GLYPHS,
            ))
            .child(probe_row(
                "4. Famiglia inesistente + font_fallbacks",
                "fallbacks = [Segoe Fluent Icons, Segoe MDL2 Assets]",
                ABSENT,
                Some(vec![FLUENT.to_owned(), MDL2.to_owned()]),
                GLYPHS,
            ))
            .child(probe_row(
                "5. Controllo negativo",
                "Segoe UI + codepoint PUA che nessuno copre",
                "Segoe UI",
                None,
                ["\u{f8ff}", "\u{e921}", "\u{e922}", "\u{e8bb}"],
            ))
            .child(
                div()
                    .text_size(px(11.))
                    .text_color(rgb(0x999999))
                    .child("Righe 3 e 4 identiche => font_fallbacks NON copre una famiglia assente."),
            )
    }
}

fn main() {
    gpui_platform::application().run(|cx: &mut App| {
        let names = cx.text_system().all_font_names();
        let has = |needle: &str| names.iter().any(|name| name == needle);
        let probe = GlyphProbe {
            fluent_seen: has(FLUENT),
            mdl2_seen: has(MDL2),
            absent_seen: has(ABSENT),
            total_families: names.len(),
        };

        eprintln!(
            "all_font_names(): {} famiglie\n  {FLUENT}: {}\n  {MDL2}: {}\n  {ABSENT}: {}",
            probe.total_families, probe.fluent_seen, probe.mdl2_seen, probe.absent_seen
        );
        for name in names.iter().filter(|name| name.starts_with("Segoe")) {
            eprintln!("  segoe: {name}");
        }

        let bounds = Bounds::centered(None, size(px(1080.), px(560.)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |_, cx| cx.new(|_| probe),
        )
        .expect("failed to open the glyph prototype window");
        cx.activate(true);
    });
}
