//! Native probe for the code editor's hosting hooks: a per-line highlighter,
//! git marks, a diagnostic, view options, no breakpoints and the events a host
//! listens to, with Sirio's theme, on Sirio's GPUI packages. Driven by
//! `Scripts/Tests/test-ely-editor-probe.sh`.
//!
//! Lines are the editor's zero-based indices, as its events carry them: the
//! highlighted line is 3, the git marks sit on 5–7, the diagnostic on 9. The
//! editor fills the window at its origin with no banner, so row `n` starts at
//! `n × line_height` and the gutter takes `gutter_columns` advances, laid out
//! as `gutter.rs` does: the breakpoint slot `[0, 2a)`, the digits, the fold
//! chevron `[4a, 6a)` and the git bar at `6a`. The text starts one advance
//! past the gutter. The `geometry` line prints those values, computed the way
//! `CodeEditor::render` computes them, so the script can aim at a row.
//!
//! stdout: `geometry …` and `line 3 bytes …` once; `breakpoints {…}` at start
//! and on each change; `event …` for every `EditorEvent`.
//! Env: `ELY_PROBE_APPEARANCE` (`light`, otherwise dark), `ELY_PROBE_UI_SIZE`,
//! and `ELY_PROBE_BREAKPOINTS=on`, which keeps the breakpoint slot live: the
//! script's control run, proving a click there reaches the gutter when it may.
use std::{collections::BTreeSet, ops::Range};

use ely_gpui_component::{
    editor::{CodeEditor, CursorShape, Diagnostic, EditorEvent, GitMark, LineNumbers, ViewOptions},
    primitives::Severity,
    theme::{ActiveTheme, TextSize},
};
use gpui::{
    App, Bounds, Context, Entity, HighlightStyle, Subscription, Window, WindowBounds,
    WindowOptions, div, font, prelude::*, px, rgb, size,
};

/// Rows in the fixture.
const LINES: usize = 60;
/// The line whose text the host colours magenta.
const HIGHLIGHTED: usize = 3;
/// Rows of code a row's height holds, as leading (`view.rs`).
const LEADING: f32 = 1.6;

/// Sixty lines of Rust; line 3 is a run of pipes, whose stems are solid.
fn fixture() -> String {
    (0..LINES)
        .map(|line| match line {
            HIGHLIGHTED => "    let bars = \"||||||||||||||||\";".to_string(),
            _ => format!("    let value_{line:02} = compute({line});"),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The bytes of a zero-based line of `text`, without its newline.
fn bytes_of(text: &str, line: usize) -> Range<usize> {
    let start: usize = text.split('\n').take(line).map(|line| line.len() + 1).sum();
    start..start + text.split('\n').nth(line).unwrap_or_default().len()
}

/// The event as one transcript line: the offsets and lines the script checks.
fn describe(event: &EditorEvent) -> String {
    match event {
        EditorEvent::Pointer { offset, .. } => format!("Pointer offset={offset:?}"),
        EditorEvent::SecondaryClick { offset, .. } => format!("SecondaryClick offset={offset:?}"),
        EditorEvent::PlatformClick { offset } => format!("PlatformClick offset={offset}"),
        EditorEvent::GitMarkPressed { line } => format!("GitMarkPressed line={line}"),
        other => format!("{other:?}"),
    }
}

struct Probe {
    editor: Entity<CodeEditor>,
    /// The breakpoints last printed, so a change shows once.
    shown: BTreeSet<usize>,
    _subscriptions: Vec<Subscription>,
}

impl Probe {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let text = fixture();
        let highlighted = bytes_of(&text, HIGHLIGHTED);
        let editor = cx.new(|cx| {
            let editor = CodeEditor::new(text.clone(), window, cx)
                .language("Rust")
                .highlighter(|line, row: &str, _| {
                    if line != HIGHLIGHTED {
                        return Vec::new();
                    }
                    let style = HighlightStyle {
                        color: Some(rgb(0xff00ff).into()),
                        ..HighlightStyle::default()
                    };
                    vec![(0..row.len(), style)]
                });
            let mut editor = if std::env::var("ELY_PROBE_BREAKPOINTS").as_deref() == Ok("on") {
                editor
            } else {
                editor.without_breakpoints()
            };
            editor.set_git_marks(
                vec![
                    (5, GitMark::Added),
                    (6, GitMark::Modified),
                    (7, GitMark::Deleted),
                ],
                cx,
            );
            editor.set_diagnostics(
                vec![Diagnostic {
                    range: bytes_of(&text, 9),
                    severity: Severity::Warning,
                    message: "probe diagnostic".into(),
                }],
                cx,
            );
            editor.set_view_options(
                ViewOptions {
                    minimap: true,
                    sticky_scroll: true,
                    indent_guides: true,
                    rainbow_brackets: false,
                    whitespace: false,
                    rulers: Vec::new(),
                    tab_size: 4,
                    line_numbers: LineNumbers::Absolute,
                    cursor_shape: CursorShape::Line,
                },
                cx,
            );
            editor
        });
        let shown = BTreeSet::new();
        println!("line {HIGHLIGHTED} bytes {}..{}", highlighted.start, highlighted.end);
        println!("breakpoints {shown:?}");
        let subscriptions = vec![
            cx.subscribe(&editor, |_, _, event: &EditorEvent, _| {
                println!("event {}", describe(event));
            }),
            cx.observe(&editor, |probe, editor, cx| {
                let now = editor.read(cx).breakpoints().clone();
                if now != probe.shown {
                    println!("breakpoints {now:?}");
                    probe.shown = now;
                }
            }),
        ];
        print_geometry(window, cx);
        Self {
            editor,
            shown,
            _subscriptions: subscriptions,
        }
    }
}

/// The layout the script aims by, from the same font and size the editor uses.
fn print_geometry(window: &mut Window, cx: &App) {
    let theme = cx.theme();
    let pixels = theme.text_size(TextSize::Sm).to_pixels(window.rem_size());
    let mono = window.text_system().resolve_font(&font(theme.mono_family.clone()));
    let advance = match window.text_system().advance(mono, pixels, 'm') {
        Ok(advance) => advance.width,
        Err(error) => panic!("no advance for the code font: {error:#}"),
    };
    let line_height = (pixels * LEADING).round();
    let digits = LINES.to_string().len().max(2);
    println!(
        "geometry advance={} line_height={} digits={digits} gutter_columns={}",
        f32::from(advance),
        f32::from(line_height),
        digits + 6
    );
}

impl Render for Probe {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child(self.editor.clone())
    }
}

fn main() {
    gpui_platform::application()
        .with_assets(sirio_ui::ely::AppAssets)
        .run(|cx: &mut App| {
            sirio_theme::register_ui_fonts(cx).expect("Sirio fonts");
            sirio_theme::Theme::init(cx);
            sirio_ui::chat::init(cx);
            if std::env::var("ELY_PROBE_APPEARANCE").as_deref() == Ok("light") {
                sirio_theme::Theme::set_mode(sirio_theme::ThemeMode::Light, cx);
            } else {
                sirio_theme::Theme::set_mode(sirio_theme::ThemeMode::Dark, cx);
            }
            if let Some(size) = std::env::var("ELY_PROBE_UI_SIZE")
                .ok()
                .and_then(|size| size.parse::<i32>().ok())
            {
                sirio_theme::Theme::set_interface_font_size(size, cx);
            }
            // Theme observers run when this closure returns; Ely's palette must
            // already follow the mode and size chosen above when the window draws.
            sirio_ui::ely::init(cx);
            let bounds = Bounds::centered(None, size(px(900.0), px(600.0)), cx);
            cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    ..Default::default()
                },
                |window, cx| {
                    window.set_window_title("Ely Editor Probe");
                    cx.new(|cx| Probe::new(window, cx))
                },
            )
            .unwrap();
            cx.activate(true);
        });
}
