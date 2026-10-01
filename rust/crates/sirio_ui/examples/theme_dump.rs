//! Every resolved theme token, as each consumer holds it.
//!
//! For the 7 base colours × 2 appearances, opaque and translucent, writes
//! into <out-dir>:
//! - `sirio.tsv`: Sirio's own tokens, `combo\ttoken\th\ts\tl\ta`;
//! - `ely.tsv`: the `Palette` in Ely's global, same columns;
//! - `bezel.txt`: bezel-theme's installed `Theme`, Debug-printed per combo;
//! - `theme.txt`: the non-colour tokens (spacing, radii, typography, chrome).
//!
//! A colour is the `Hsla` gpui paints, each f32 printed with `{:?}`, which
//! round-trips exactly. Evidence for the theme migration:
//! docs/testing/theme-ely-palette/README.md.
//!
//!     cargo run -p sirio_ui --example theme_dump -- <out-dir>

use std::fmt::Write as _;
use std::path::PathBuf;

use ely_gpui_component::theme::{Palette, Syntax, Theme as ElyTheme};
use gpui::{Hsla, TestAppContext};
use sirio_theme::{BaseColor, Theme, ThemeMode};

fn push(out: &mut String, combo: &str, token: &str, color: Hsla) {
    writeln!(
        out,
        "{combo}\t{token}\t{:?}\t{:?}\t{:?}\t{:?}",
        color.h, color.s, color.l, color.a
    )
    .unwrap();
}

/// Sirio's tokens, in this tree's vocabulary.
fn sirio_tokens(theme: &Theme) -> Vec<(String, Hsla)> {
    let c = theme.colors;
    [
        ("frame_surface", c.frame_surface),
        ("bg", c.bg),
        ("surface", c.surface),
        ("border_opaque", c.border_opaque),
        ("terminal_surface", c.terminal_surface),
        ("warning", c.warning),
        ("success", c.success),
        ("danger", c.danger),
        ("border", c.border),
        ("element_hover", c.element_hover),
        ("text", c.text),
        ("text_muted", c.text_muted),
        ("text_faint", c.text_faint),
        ("tree_guide", c.tree_guide),
        ("git_untracked", c.git_untracked),
        ("diff_add", c.diff_add),
        ("diff_add_bg", c.diff_add_bg),
        ("diff_del", c.diff_del),
        ("diff_del_bg", c.diff_del_bg),
        ("file_link", c.file_link),
        ("surface_raised", c.surface_raised),
        ("input_bg", c.input_bg),
        ("dialog_surface", c.dialog_surface),
        ("floating_surface", c.floating_surface),
        ("overlay", c.overlay),
        ("overlay_strong", c.overlay_strong),
        ("border_strong", c.border_strong),
        ("ring", c.ring),
        ("text_dim", c.text_dim),
        ("brand_coral", c.brand_coral),
        ("accent", c.accent),
        ("element_active", c.element_active),
        ("selection", c.selection),
        ("code_wash", c.code_wash),
        ("solid", c.solid),
        ("on_solid", c.on_solid),
        ("favorite", c.favorite),
        ("danger_muted", c.danger_muted),
    ]
    .into_iter()
    .map(|(name, color)| (name.to_string(), Hsla::from(color)))
    .collect()
}

fn ely_tokens(palette: &Palette) -> Vec<(String, Hsla)> {
    let mut rows: Vec<(String, Hsla)> = Palette::NAMES
        .iter()
        .map(|name| (name.to_string(), palette.token(name)))
        .collect();
    rows.extend(
        palette
            .chart
            .iter()
            .enumerate()
            .map(|(i, c)| (format!("chart.{i}"), *c)),
    );
    rows.extend(
        palette
            .ansi
            .iter()
            .enumerate()
            .map(|(i, c)| (format!("ansi.{i}"), *c)),
    );
    rows.extend(
        Syntax::NAMES
            .iter()
            .map(|name| (format!("syntax.{name}"), palette.syntax.token(name))),
    );
    rows
}

fn main() {
    let out: PathBuf = std::env::args()
        .nth(1)
        .expect("usage: theme_dump <out-dir>")
        .into();
    std::fs::create_dir_all(&out).expect("create the output directory");
    let cx = TestAppContext::single();
    cx.update(|cx| {
        Theme::init(cx);
        sirio_ui::chat::init(cx);
    });
    cx.run_until_parked();
    let (mut sirio, mut ely, mut bezel, mut theme_txt) =
        (String::new(), String::new(), String::new(), String::new());
    cx.update(|cx| {
        let t = Theme::get(cx);
        writeln!(
            theme_txt,
            "{:#?}\n{:#?}\n{:#?}\n{:#?}\n{:#?}",
            t.spacing, t.radii, t.typography, t.browser_chrome, t.windows_caption
        )
        .unwrap();
    });
    for base in BaseColor::ALL {
        for mode in [ThemeMode::Dark, ThemeMode::Light] {
            for translucent in [false, true] {
                let combo = format!(
                    "{}/{mode:?}/{}",
                    base.title(),
                    if translucent { "translucent" } else { "opaque" }
                );
                cx.update(|cx| {
                    Theme::set_mode(mode, cx);
                    Theme::set_base_color(base, cx);
                    let theme = *Theme::get(cx);
                    cx.set_global(theme.with_translucency(translucent));
                });
                cx.run_until_parked();
                cx.update(|cx| {
                    let theme = *Theme::get(cx);
                    for (token, color) in sirio_tokens(&theme) {
                        push(&mut sirio, &combo, &token, color);
                    }
                    for (token, color) in ely_tokens(&cx.global::<ElyTheme>().colors) {
                        push(&mut ely, &combo, &token, color);
                    }
                    writeln!(bezel, "== {combo}\n{:#?}", bezel::theme::Theme::of(cx)).unwrap();
                });
            }
        }
    }
    for (name, text) in [
        ("sirio.tsv", sirio),
        ("ely.tsv", ely),
        ("bezel.txt", bezel),
        ("theme.txt", theme_txt),
    ] {
        std::fs::write(out.join(name), text).expect("write a dump file");
    }
    println!("THEME DUMP OK: {}", out.display());
}
