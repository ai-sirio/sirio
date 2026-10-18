//! The breadcrumb over a file: where the file is, and where the caret is in it.
//!
//! A row above the code (and above the find bar: it names the file, the find
//! bar belongs to the code under it) reading `src › main.rs › impl Foo › bar`:
//! the file's path from its worktree's root, then the symbols that enclose the
//! caret. The path's parts are text; a symbol is a button that reveals its line.
//!
//! The symbols come from the app (`FileView::set_symbols`), which asks the
//! file's language server. With none, the row is the path alone. The row is
//! drawn in Code and in Preview, since it names the file, and not at all for a
//! snapshot: a revision has no worktree path or server, and its own bar names it.

use gpui::{AnyElement, Context, Entity, prelude::*, px};
use sirio_theme::Theme;

use super::FileView;
use crate::editor::LoadStatus;
use crate::outline::OutlineSymbol;

/// The symbols that enclose the caret, outermost first, as far as start lines
/// can tell: the protocol's outline gives each symbol's first line and not its
/// last, so a symbol "encloses" the caret from its own line until a symbol at
/// the same depth or a shallower one starts. A function that ended above the
/// caret is still named while nothing else has begun.
pub(super) fn chain(symbols: &[OutlineSymbol], caret_line: usize) -> Vec<&OutlineSymbol> {
    let mut found: Vec<&OutlineSymbol> = Vec::new();
    for symbol in symbols.iter().filter(|symbol| symbol.line <= caret_line) {
        found.truncate(symbol.depth);
        found.push(symbol);
    }
    found
}

/// One part of the breadcrumb.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Crumb {
    pub(super) label: String,
    /// The line a press reveals: `Some` for a symbol, `None` for the path.
    pub(super) line: Option<usize>,
}

/// The separator between parts, as the row draws it and the state reads it.
const SEPARATOR: &str = " › ";

/// The zero-based line `offset` is on.
fn line_of(text: &str, offset: usize) -> usize {
    let end = offset.min(text.len());
    text.as_bytes()[..end]
        .iter()
        .filter(|byte| **byte == b'\n')
        .count()
}

impl FileView {
    /// The symbols of this file, as the app's language server last gave them.
    /// An empty list is how a server says it has none.
    pub fn set_symbols(&mut self, symbols: Vec<OutlineSymbol>, cx: &mut Context<Self>) {
        if self.symbols != symbols {
            self.symbols = symbols;
            cx.notify();
        }
    }

    /// The path the breadcrumb starts with: this file relative to its worktree's
    /// root, as the app spells it. `None` when the file is under no known
    /// worktree, which shows the file's own name.
    pub fn set_location(&mut self, location: Option<String>, cx: &mut Context<Self>) {
        if self.location != location {
            self.location = location;
            cx.notify();
        }
    }

    /// Takes the caret's line from the code editor; true when it moved. The
    /// code editor reports no caret move to this view, so its owner calls this
    /// from an observer of the editor.
    pub(super) fn refresh_caret_line(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(code) = &self.code_editor else {
            return false;
        };
        let code = code.read(cx);
        let line = line_of(code.text(), code.primary().head);
        let moved = line != self.caret_line;
        self.caret_line = line;
        moved
    }

    /// The breadcrumb's parts in drawing order: the path's, then the symbols
    /// around the caret. Empty while there is nothing to name: no file loaded,
    /// or a snapshot.
    pub(super) fn breadcrumb_crumbs(&self) -> Vec<Crumb> {
        let loaded = self
            .editor()
            .is_some_and(|editor| editor.status() == &LoadStatus::Loaded);
        if !loaded || self.code_editor.is_none() || self.is_snapshot() {
            return Vec::new();
        }
        let path = match &self.location {
            Some(location) => location.clone(),
            None => self
                .path()
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default(),
        };
        let mut crumbs: Vec<Crumb> = path
            .split(['/', '\\'])
            .filter(|part| !part.is_empty())
            .map(|part| Crumb {
                label: part.to_owned(),
                line: None,
            })
            .collect();
        crumbs.extend(chain(&self.symbols, self.caret_line).into_iter().map(|symbol| Crumb {
            label: symbol.name.clone(),
            line: Some(symbol.line),
        }));
        crumbs
    }

    /// The breadcrumb as text, parts joined by ` › `: what the row reads.
    /// Empty when the row is not drawn.
    pub fn breadcrumb_text(&self) -> String {
        self.breadcrumb_crumbs()
            .iter()
            .map(|crumb| crumb.label.as_str())
            .collect::<Vec<_>>()
            .join(SEPARATOR)
    }

    /// A press on the crumb at `index` (the row's order): a symbol reveals its
    /// line, as Go to Symbol does. The path's parts are not buttons. The row's
    /// click and the socket's `surface.editor.crumb` both come here.
    pub fn press_crumb(&mut self, index: usize, cx: &mut Context<Self>) -> Result<(), String> {
        let crumbs = self.breadcrumb_crumbs();
        let crumb = crumbs
            .get(index)
            .ok_or_else(|| format!("the breadcrumb has {} parts, none at {index}", crumbs.len()))?;
        let line = crumb
            .line
            .ok_or_else(|| format!("\"{}\" is part of the path, not a symbol", crumb.label))?;
        self.reveal_at(line, cx);
        Ok(())
    }

    /// The row above the code, or `None` when there is nothing to name.
    pub(super) fn render_breadcrumb(
        &self,
        theme: Theme,
        entity: Entity<Self>,
    ) -> Option<AnyElement> {
        let crumbs = self.breadcrumb_crumbs();
        let last = crumbs.len().checked_sub(1)?;
        let mut row = gpui::div()
            .id("file-breadcrumb")
            .debug_selector(|| "file-breadcrumb".to_owned())
            .w_full()
            .min_w(px(0.0))
            .px(px(12.0))
            .py(px(4.0))
            .flex()
            .flex_none()
            .items_center()
            .gap(px(2.0))
            .overflow_hidden()
            .whitespace_nowrap()
            .border_b_1()
            .border_color(theme.ely.border)
            .bg(theme.ely.surface)
            .text_size(theme.typography.footnote)
            .text_color(theme.ely.fg_muted);
        for (index, crumb) in crumbs.into_iter().enumerate() {
            if index > 0 {
                row = row.child(gpui::div().flex_none().px(px(2.0)).child("›"));
            }
            let color = if index == last {
                theme.ely.fg
            } else {
                theme.ely.fg_muted
            };
            let part = gpui::div()
                .id(("file-crumb", index))
                .debug_selector(move || format!("file-crumb-{index}"))
                .flex_none()
                .px(px(4.0))
                .rounded(theme.radii.control)
                .text_color(color);
            row = row.child(match crumb.line {
                Some(_) => {
                    let entity = entity.clone();
                    part.cursor_pointer()
                        .hover(|style| style.bg(theme.ely.hover))
                        .on_click(move |_, _, cx| {
                            entity.update(cx, |view, cx| {
                                // A crumb that went with the last refresh refuses.
                                let _ = view.press_crumb(index, cx);
                            });
                        })
                        .child(crumb.label)
                }
                None => part.child(crumb.label),
            });
        }
        Some(row.into_any_element())
    }
}

#[cfg(test)]
mod tests {
    //! Failure list, written before the code:
    //!
    //! 1. A caret above every symbol is given a chain anyway (the first symbol
    //!    is taken for what it is not yet inside).
    //! 2. A symbol that starts below the caret is put in the chain, because the
    //!    walk does not stop at the caret's line.
    //! 3. Inside a method the chain stops at the impl: the method, one level
    //!    deeper, is dropped, or the impl is.
    //! 4. A top-level symbol that starts after a method does not close the
    //!    method's levels, so the chain names the impl of a function it is not
    //!    in.
    //! 5. A sibling does not replace its sibling: the chain names both methods
    //!    of one impl, as if the caret were in each.
    //! 6. A symbol at the caret's own line is left out, so a jump to a symbol
    //!    lands the caret on it and the breadcrumb does not name it.

    use super::chain;
    use crate::outline::{OutlineKind, OutlineSymbol};

    fn symbol(name: &str, line: usize, depth: usize) -> OutlineSymbol {
        OutlineSymbol {
            name: name.to_owned(),
            detail: None,
            kind: OutlineKind::Other,
            line,
            depth,
        }
    }

    fn names<'a>(chain: &[&'a OutlineSymbol]) -> Vec<&'a str> {
        chain.iter().map(|symbol| symbol.name.as_str()).collect()
    }

    /// `fn free` at 2, `impl Foo` at 6 with `first` at 7 and `second` at 12,
    /// `fn tail` at 20.
    fn outline() -> Vec<OutlineSymbol> {
        vec![
            symbol("free", 2, 0),
            symbol("impl Foo", 6, 0),
            symbol("first", 7, 1),
            symbol("second", 12, 1),
            symbol("tail", 20, 0),
        ]
    }

    #[test]
    fn a_caret_before_every_symbol_has_no_chain() {
        let symbols = outline();
        assert_eq!(names(&chain(&symbols, 0)), Vec::<&str>::new());
        assert_eq!(names(&chain(&symbols, 1)), Vec::<&str>::new());
        assert_eq!(names(&chain(&[], 5)), Vec::<&str>::new());
    }

    #[test]
    fn a_caret_inside_a_method_names_its_impl_and_method() {
        let symbols = outline();
        assert_eq!(names(&chain(&symbols, 9)), ["impl Foo", "first"]);
        // On the symbol's own line it is already in the chain.
        assert_eq!(names(&chain(&symbols, 7)), ["impl Foo", "first"]);
        assert_eq!(names(&chain(&symbols, 6)), ["impl Foo"]);
    }

    #[test]
    fn a_shallower_symbol_closes_the_deeper_ones() {
        let symbols = outline();
        assert_eq!(names(&chain(&symbols, 13)), ["impl Foo", "second"]);
        assert_eq!(names(&chain(&symbols, 22)), ["tail"]);
    }

    #[test]
    fn siblings_replace_each_other() {
        let symbols = outline();
        assert_eq!(names(&chain(&symbols, 15)), ["impl Foo", "second"]);
        assert_eq!(names(&chain(&symbols, 3)), ["free"]);
    }
}
