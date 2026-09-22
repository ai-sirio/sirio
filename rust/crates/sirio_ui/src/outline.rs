//! The Go to Symbol overlay: a file's symbols, filtered by typing, jumped
//! to with Enter, gone on Escape.
//!
//! It lives here rather than beside the command palette because the palette
//! is entangled with the app's typed command dispatch and this is not. It
//! owns what belongs to an overlay — open flag, query, selection, focus,
//! and the symbols with the path they were asked for. The app owns the
//! fetch and the jump.
//!
//! There is no sequence counter. Hover needs one because a moving pointer
//! asks overlapping questions; only one overlay is ever open, so "still
//! open, still this path" is the whole reconciliation key.

use std::path::{Path, PathBuf};

use gpui::{
    Context, EventEmitter, FocusHandle, KeyDownEvent, MouseButton, Render, Window, div, prelude::*,
    px,
};
use sirio_theme::Theme;

/// Row height, the palette's own (`main.rs`, `render_command_palette`).
const ROW_HEIGHT: f32 = 31.0;

/// The kinds the outline draws. Converted from the protocol's by the app,
/// so this crate stays free of `lsp-types`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutlineKind {
    Function,
    Method,
    Struct,
    Enum,
    Interface,
    Field,
    Constant,
    Variable,
    Module,
    Other,
}

impl OutlineKind {
    /// A one-word tag rather than an icon: the rail's glyphs are SVG assets
    /// with a provenance file, and ten new ones for this would be ten new
    /// vendored files for a list that is read, not scanned.
    fn tag(self) -> &'static str {
        match self {
            Self::Function => "fn",
            Self::Method => "fn",
            Self::Struct => "struct",
            Self::Enum => "enum",
            Self::Interface => "trait",
            Self::Field => "field",
            Self::Constant => "const",
            Self::Variable => "let",
            Self::Module => "mod",
            Self::Other => "",
        }
    }
}

/// One row of a file's outline.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutlineSymbol {
    pub name: String,
    pub detail: Option<String>,
    pub kind: OutlineKind,
    /// Zero-based, as `open_at_line` expects.
    pub line: usize,
    /// Nesting depth, drawn as an indent — and dropped while a filter is
    /// active, because an indent with its parents filtered away is a lie.
    pub depth: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum OutlineState {
    Loading,
    Ready(Vec<OutlineSymbol>),
    Failed(String),
}

pub enum OutlineEvent {
    Jump { path: PathBuf, line: usize },
}

/// Substring, case-insensitive, on the name only — the same rule as
/// `command_palette::filter_entries`. Two search boxes in one app that
/// disagree about matching is a bug report waiting to happen.
pub fn filter_symbols<'a>(symbols: &'a [OutlineSymbol], query: &str) -> Vec<&'a OutlineSymbol> {
    let query = query.trim().to_ascii_lowercase();
    symbols
        .iter()
        .filter(|symbol| query.is_empty() || symbol.name.to_ascii_lowercase().contains(&query))
        .collect()
}

pub struct Outline {
    open: bool,
    /// The file the current question was asked for. An answer carrying any
    /// other path is an answer to a question nobody is still asking.
    path: Option<PathBuf>,
    state: OutlineState,
    query: String,
    selected: usize,
    focus: FocusHandle,
}

impl Outline {
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            open: false,
            path: None,
            state: OutlineState::Loading,
            query: String::new(),
            selected: 0,
            focus: cx.focus_handle(),
        }
    }

    /// Opens the overlay for `path`, waiting. Deliberately called *before*
    /// the request goes out: against a server that is still indexing,
    /// opening on the answer would leave the command looking inert for
    /// several seconds.
    pub fn begin(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        self.open = true;
        self.path = Some(path);
        self.state = OutlineState::Loading;
        self.query.clear();
        self.selected = 0;
        self.focus.focus(window, cx);
        cx.notify();
    }

    pub fn set_symbols(
        &mut self,
        path: &Path,
        symbols: Vec<OutlineSymbol>,
        cx: &mut Context<Self>,
    ) {
        if !self.is_current(path) {
            return;
        }
        self.state = OutlineState::Ready(symbols);
        self.selected = 0;
        cx.notify();
    }

    /// An error, already phrased for a reader — `LspError`'s `Display`
    /// covers the still-indexing case in those terms.
    pub fn fail(&mut self, path: &Path, message: String, cx: &mut Context<Self>) {
        if !self.is_current(path) {
            return;
        }
        self.state = OutlineState::Failed(message);
        cx.notify();
    }

    pub fn close(&mut self, cx: &mut Context<Self>) {
        self.open = false;
        self.path = None;
        self.state = OutlineState::Loading;
        self.query.clear();
        self.selected = 0;
        cx.notify();
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    pub fn is_loading(&self) -> bool {
        matches!(self.state, OutlineState::Loading)
    }

    pub fn selected_index(&self) -> usize {
        self.selected
    }

    /// How many rows the filter currently leaves. Public for the tests, and
    /// cheap: the app never calls it.
    pub fn visible_count(&self) -> usize {
        self.visible().len()
    }

    fn is_current(&self, path: &Path) -> bool {
        self.open && self.path.as_deref() == Some(path)
    }

    fn visible(&self) -> Vec<&OutlineSymbol> {
        match &self.state {
            OutlineState::Ready(symbols) => filter_symbols(symbols, &self.query),
            _ => Vec::new(),
        }
    }

    fn handle_key(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        let key = event.keystroke.key.as_str();
        if key == "escape" {
            self.close(cx);
            return;
        }
        let visible = self.visible().len();
        match key {
            "backspace" | "delete" => {
                self.query.pop();
                self.selected = 0;
                cx.notify();
            }
            // Clamping, not wrapping — `handle_palette_key`'s behaviour.
            "up" => {
                self.selected = self.selected.saturating_sub(1);
                cx.notify();
            }
            "down" => {
                if visible > 0 {
                    self.selected = (self.selected + 1).min(visible - 1);
                    cx.notify();
                }
            }
            "enter" | "return" => self.accept(cx),
            _ if !event.keystroke.modifiers.platform
                && !event.keystroke.modifiers.control
                && !event.keystroke.modifiers.alt
                && event
                    .keystroke
                    .key_char
                    .as_deref()
                    .is_some_and(|text| !text.is_empty()) =>
            {
                self.query
                    .push_str(event.keystroke.key_char.as_deref().unwrap_or_default());
                self.selected = 0;
                cx.notify();
            }
            _ => {}
        }
    }

    fn accept(&mut self, cx: &mut Context<Self>) {
        let Some(path) = self.path.clone() else {
            return;
        };
        let Some(line) = self.visible().get(self.selected).map(|symbol| symbol.line) else {
            return;
        };
        cx.emit(OutlineEvent::Jump { path, line });
        self.close(cx);
    }
}

impl EventEmitter<OutlineEvent> for Outline {}

impl Render for Outline {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *Theme::get(cx);
        let filtering = !self.query.trim().is_empty();
        let visible = self
            .visible()
            .into_iter()
            .cloned()
            .collect::<Vec<OutlineSymbol>>();
        let selected = self.selected.min(visible.len().saturating_sub(1));

        let message = match &self.state {
            OutlineState::Loading => Some("Loading symbols…".to_owned()),
            OutlineState::Failed(text) => Some(text.clone()),
            OutlineState::Ready(symbols) if symbols.is_empty() => {
                Some("No symbols in this file".to_owned())
            }
            OutlineState::Ready(_) if visible.is_empty() => Some("No match".to_owned()),
            OutlineState::Ready(_) => None,
        };

        let mut rows = div().id("outline-rows").flex().flex_col().gap(px(1.0));
        for (index, symbol) in visible.into_iter().enumerate() {
            let active = index == selected;
            let line = symbol.line;
            // Indent only while unfiltered: with the parents filtered away
            // an indent points at nothing.
            let indent = if filtering {
                0.0
            } else {
                (symbol.depth as f32) * 12.0
            };
            rows = rows.child(
                div()
                    .id(("outline-row", index))
                    .debug_selector(move || format!("outline-row-{index}"))
                    .h(px(ROW_HEIGHT))
                    .w_full()
                    .pl(px(10.0 + indent))
                    .pr(px(10.0))
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .rounded(theme.radii.control)
                    .text_size(theme.typography.footnote)
                    .text_color(theme.text)
                    .when(active, |this| this.bg(theme.element_active))
                    .hover(|style| style.bg(theme.element_hover))
                    .on_click(cx.listener(move |outline, _event, _window, cx| {
                        if let Some(path) = outline.path.clone() {
                            cx.emit(OutlineEvent::Jump { path, line });
                            outline.close(cx);
                        }
                    }))
                    .child(
                        div()
                            .w(px(44.0))
                            .flex_none()
                            .text_color(theme.text_faint)
                            .child(symbol.kind.tag()),
                    )
                    .child(symbol.name.clone())
                    .when_some(symbol.detail.clone(), |this, detail| {
                        this.child(div().text_color(theme.text_faint).child(detail))
                    })
                    .child(
                        div()
                            .ml_auto()
                            .text_color(theme.text_faint)
                            .child(format!("{}", symbol.line + 1)),
                    ),
            );
        }

        let body = match message {
            Some(text) => div()
                .id("outline-message")
                .debug_selector(|| "outline-message".to_owned())
                .h(px(ROW_HEIGHT))
                .w_full()
                .px(px(10.0))
                .flex()
                .items_center()
                .text_size(theme.typography.footnote)
                .text_color(theme.text_faint)
                .child(text)
                .into_any_element(),
            None => rows.into_any_element(),
        };

        let query = self.query.clone();
        div()
            .id("outline")
            .debug_selector(|| "outline".to_owned())
            .key_context("Outline")
            .track_focus(&self.focus)
            .capture_key_down(cx.listener(|outline, event: &KeyDownEvent, _window, cx| {
                if outline.open {
                    outline.handle_key(event, cx);
                    cx.stop_propagation();
                }
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|outline, _event, window, cx| outline.focus.focus(window, cx)),
            )
            // The always-drawn launcher can sit under it.
            .occlude()
            // The palette's own geometry, so the two read as siblings.
            .flex()
            .flex_col()
            .absolute()
            .top(px(56.0))
            .left(px(220.0))
            .w(px(620.0))
            .h(px(470.0))
            .p(px(8.0))
            .rounded(theme.radii.user_pill)
            .border_1()
            .border_color(theme.border)
            .bg(theme.floating_surface)
            .shadow_lg()
            .child(
                div()
                    .id("outline-filter")
                    .debug_selector(|| "outline-filter".to_owned())
                    .h(px(34.0))
                    .w_full()
                    .px(px(10.0))
                    .flex()
                    .items_center()
                    .rounded(theme.radii.control)
                    .bg(theme.input_bg)
                    .border_1()
                    .border_color(theme.text)
                    .text_size(theme.typography.headline)
                    .text_color(if query.is_empty() {
                        theme.text_faint
                    } else {
                        theme.text
                    })
                    .child(if query.is_empty() {
                        "Type to filter symbols".to_owned()
                    } else {
                        query
                    }),
            )
            .child(
                div()
                    .id("outline-rows-viewport")
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .overflow_y_scroll()
                    .child(body),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{TestAppContext, VisualTestContext};
    use sirio_theme::Theme;

    fn symbol(name: &str, line: usize) -> OutlineSymbol {
        OutlineSymbol {
            name: name.to_owned(),
            detail: None,
            kind: OutlineKind::Function,
            line,
            depth: 0,
        }
    }

    fn mounted(cx: &mut TestAppContext) -> (VisualTestContext, gpui::Entity<Outline>) {
        cx.update(|cx| Theme::init(cx));
        let window = cx.add_window(|_window, cx| Outline::new(cx));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        let entity = cx
            .update(|window, _| window.root::<Outline>().flatten())
            .expect("outline root");
        (cx, entity)
    }

    #[test]
    fn the_filter_narrows_by_substring_case_insensitively() {
        // The same rule as the command palette's, deliberately: two search
        // boxes in one app that disagree about matching is a bug report.
        let symbols = vec![
            symbol("spawn_router", 10),
            symbol("Spawner", 20),
            symbol("close", 30),
        ];
        let all = filter_symbols(&symbols, "");
        assert_eq!(all.len(), 3, "an empty query shows everything");

        let narrowed = filter_symbols(&symbols, "SPA");
        assert_eq!(narrowed.len(), 2);
        assert_eq!(narrowed[0].name, "spawn_router");
        assert_eq!(narrowed[1].name, "Spawner");

        // Mid-string: 'awn' sits at offset 2, so a prefix matcher
        // would miss it entirely.
        let mid = filter_symbols(&symbols, "awn");
        assert_eq!(mid.len(), 2);
        assert_eq!(mid[0].name, "spawn_router");
        assert_eq!(mid[1].name, "Spawner");

        // Surrounding whitespace is trimmed before matching.
        let padded = filter_symbols(&symbols, "  SPA  ");
        assert_eq!(padded.len(), 2);
        assert_eq!(padded[0].name, "spawn_router");
        assert_eq!(padded[1].name, "Spawner");

        assert!(filter_symbols(&symbols, "zzz").is_empty());
    }

    #[gpui::test]
    async fn typing_filters_and_enter_jumps_exactly_once(cx: &mut TestAppContext) {
        let (mut cx, outline) = mounted(cx);
        let path = std::path::PathBuf::from("/repo/a.rs");
        cx.update(|window, cx| {
            outline.update(cx, |outline, cx| {
                outline.begin(path.clone(), window, cx);
                outline.set_symbols(
                    &path,
                    vec![symbol("alpha", 1), symbol("beta", 2), symbol("betamax", 3)],
                    cx,
                );
            });
        });

        let jumps = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let seen = jumps.clone();
        cx.update(|_, cx| {
            cx.subscribe(&outline, move |_, event: &OutlineEvent, _| {
                let OutlineEvent::Jump { line, .. } = event;
                seen.borrow_mut().push(*line);
            })
            .detach();
        });

        cx.simulate_input("beta");
        cx.run_until_parked();
        assert_eq!(
            outline.read_with(&cx.cx, |outline, _| outline.visible_count()),
            2,
            "beta and betamax survive the filter"
        );

        cx.simulate_keystrokes("down");
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();

        assert_eq!(&*jumps.borrow(), &[3], "the second match, once");
        assert!(
            !outline.read_with(&cx.cx, |outline, _| outline.is_open()),
            "a jump closes the overlay"
        );
    }

    #[gpui::test]
    async fn the_arrows_clamp_rather_than_wrap(cx: &mut TestAppContext) {
        let (mut cx, outline) = mounted(cx);
        let path = std::path::PathBuf::from("/repo/a.rs");
        cx.update(|window, cx| {
            outline.update(cx, |outline, cx| {
                outline.begin(path.clone(), window, cx);
                outline.set_symbols(&path, vec![symbol("a", 1), symbol("b", 2)], cx);
            });
        });

        cx.simulate_keystrokes("up");
        cx.run_until_parked();
        assert_eq!(
            outline.read_with(&cx.cx, |o, _| o.selected_index()),
            0,
            "a single up from the first row clamps, not wraps to the last"
        );

        cx.simulate_keystrokes("up up");
        cx.run_until_parked();
        assert_eq!(outline.read_with(&cx.cx, |o, _| o.selected_index()), 0);

        cx.simulate_keystrokes("down down down");
        cx.run_until_parked();
        assert_eq!(
            outline.read_with(&cx.cx, |o, _| o.selected_index()),
            1,
            "the last row, not back to the first"
        );

        cx.simulate_keystrokes("down");
        cx.run_until_parked();
        assert_eq!(
            outline.read_with(&cx.cx, |o, _| o.selected_index()),
            1,
            "a single down from the last row clamps, not wraps to the first"
        );
    }

    #[gpui::test]
    async fn escape_closes_without_jumping(cx: &mut TestAppContext) {
        let (mut cx, outline) = mounted(cx);
        let path = std::path::PathBuf::from("/repo/a.rs");
        cx.update(|window, cx| {
            outline.update(cx, |outline, cx| {
                outline.begin(path.clone(), window, cx);
                outline.set_symbols(&path, vec![symbol("a", 1)], cx);
            });
        });

        let jumps = std::rc::Rc::new(std::cell::RefCell::new(0usize));
        let seen = jumps.clone();
        cx.update(|_, cx| {
            cx.subscribe(&outline, move |_, _: &OutlineEvent, _| {
                *seen.borrow_mut() += 1;
            })
            .detach();
        });

        cx.simulate_keystrokes("escape");
        cx.run_until_parked();
        assert!(!outline.read_with(&cx.cx, |o, _| o.is_open()));
        assert_eq!(*jumps.borrow(), 0);
    }

    #[gpui::test]
    async fn an_answer_for_another_file_is_discarded(cx: &mut TestAppContext) {
        // The reconciliation key. A reply that outlived the question it was
        // asked for must not fill the overlay with another file's symbols.
        let (mut cx, outline) = mounted(cx);
        let asked = std::path::PathBuf::from("/repo/a.rs");
        let stale = std::path::PathBuf::from("/repo/b.rs");
        cx.update(|window, cx| {
            outline.update(cx, |outline, cx| {
                outline.begin(asked.clone(), window, cx);
                outline.set_symbols(&stale, vec![symbol("wrong", 9)], cx);
            });
        });
        assert_eq!(
            outline.read_with(&cx.cx, |o, _| o.visible_count()),
            0,
            "the stale answer was dropped"
        );
        assert!(
            outline.read_with(&cx.cx, |o, _| o.is_loading()),
            "and the overlay is still waiting for its own"
        );
    }

    #[gpui::test]
    async fn a_closed_overlay_ignores_a_late_answer(cx: &mut TestAppContext) {
        let (mut cx, outline) = mounted(cx);
        let path = std::path::PathBuf::from("/repo/a.rs");
        cx.update(|window, cx| {
            outline.update(cx, |outline, cx| {
                outline.begin(path.clone(), window, cx);
                outline.close(cx);
                outline.set_symbols(&path, vec![symbol("late", 4)], cx);
            });
        });
        assert!(!outline.read_with(&cx.cx, |o, _| o.is_open()));
        assert_eq!(outline.read_with(&cx.cx, |o, _| o.visible_count()), 0);
    }
}
