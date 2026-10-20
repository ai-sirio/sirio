//! The breadcrumb over a file: where the file is, and where the caret is in it.
//!
//! A row above the code (and above the find bar: it names the file, the find
//! bar belongs to the code under it) reading `src › main.rs › impl Foo › bar`:
//! the file's path from its worktree's root, then the symbols that enclose the
//! caret. The path's parts are text; a symbol is a button that reveals its line.
//!
//! The symbols come from the app (`FileView::set_symbols`), which asks the
//! file's language server. With none, the row is the path alone. The row is
//! drawn in Code and in Preview, since it names the file, but only Code names
//! symbols: Preview hides the code editor, so a caret there is not what the
//! reader is looking at and a press on its symbol would move nothing visible.
//! It is not drawn at all for a snapshot: a revision has no worktree path or
//! server, and its own bar names it. In a narrow pane the directories give way
//! first (the leading ones are cut with an ellipsis), then the file's name and
//! the symbols are what stays.

use gpui::{AnyElement, Context, Entity, prelude::*, px};
use sirio_theme::Theme;

use super::{FileView, MarkdownMode};
use crate::editor::LoadStatus;
use crate::outline::OutlineSymbol;

/// The symbols that enclose the caret, outermost first, as far as start lines
/// can tell: the protocol's outline gives each symbol's first line and not its
/// last, so a symbol "encloses" the caret from its own line until a symbol at
/// the same depth or a shallower one starts. A function that ended above the
/// caret is still named while nothing else has begun.
///
/// A server need not sort its `DocumentSymbol`s or step one level at a time, so
/// the walk is by line (stable: symbols on one line keep the order they came
/// in) and a symbol closes whatever in the chain is as deep as itself or deeper.
pub(super) fn chain(symbols: &[OutlineSymbol], caret_line: usize) -> Vec<&OutlineSymbol> {
    let mut before: Vec<&OutlineSymbol> = symbols
        .iter()
        .filter(|symbol| symbol.line <= caret_line)
        .collect();
    before.sort_by_key(|symbol| symbol.line);
    let mut found: Vec<&OutlineSymbol> = Vec::new();
    for symbol in before {
        while found.last().is_some_and(|last| last.depth >= symbol.depth) {
            found.pop();
        }
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
        // `position` is a search over the buffer's line starts and counts from one.
        let line = code.read(cx).position().0.saturating_sub(1);
        let moved = line != self.caret_line;
        self.caret_line = line;
        moved
    }

    /// The breadcrumb's parts in drawing order: the path's, then the symbols
    /// around the caret. Empty while there is nothing to name: no file loaded,
    /// or a snapshot; in Markdown Preview the path alone.
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
        if self.effective_mode() != MarkdownMode::Preview {
            crumbs.extend(
                chain(&self.symbols, self.caret_line)
                    .into_iter()
                    .map(|symbol| Crumb {
                        label: symbol.name.clone(),
                        line: Some(symbol.line),
                    }),
            );
        }
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
        self.reveal(super::RevealTarget::Line(line), cx);
        // The code editor's own notification reaches the observer after this
        // returns; the caller (the socket's reply) reads the breadcrumb now.
        if self.refresh_caret_line(cx) {
            cx.notify();
        }
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
        let path_parts = crumbs.iter().filter(|crumb| crumb.line.is_none()).count();
        let separator = || gpui::div().flex_none().px(px(2.0)).child("›");
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
        // The path is one child that shrinks: the directories are a single
        // text cut with an ellipsis when the pane is narrow, the file's name
        // after them is never cut. The symbols stay as wide as they are.
        let mut path = gpui::div()
            .min_w(px(0.0))
            .flex_shrink(1.0)
            .flex()
            .items_center()
            .gap(px(2.0))
            .overflow_hidden();
        let directories: Vec<&str> = crumbs[..path_parts.saturating_sub(1)]
            .iter()
            .map(|crumb| crumb.label.as_str())
            .collect();
        if !directories.is_empty() {
            path = path
                .child(
                    gpui::div()
                        .id("file-crumb-directories")
                        .debug_selector(|| "file-crumb-directories".to_owned())
                        .min_w(px(0.0))
                        .flex_shrink(1.0)
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .px(px(4.0))
                        .child(directories.join(SEPARATOR)),
                )
                .child(separator());
        }
        let mut symbols: Vec<AnyElement> = Vec::new();
        for (index, crumb) in crumbs.into_iter().enumerate() {
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
            match crumb.line {
                Some(_) => {
                    let entity = entity.clone();
                    symbols.push(separator().into_any_element());
                    symbols.push(
                        part.cursor_pointer()
                            .hover(|style| style.bg(theme.ely.hover))
                            .on_click(move |_, _, cx| {
                                entity.update(cx, |view, cx| {
                                    // A crumb that went with the last refresh refuses.
                                    let _ = view.press_crumb(index, cx);
                                });
                            })
                            .child(crumb.label)
                            .into_any_element(),
                    );
                }
                // Only the file's own name is drawn here; the directories are
                // already in the one text above.
                None if index + 1 == path_parts => path = path.child(part.child(crumb.label)),
                None => {}
            }
        }
        row = row.child(path).children(symbols);
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
    //! 7. A server that does not sort its children (`[A, alpha at 3, zeta at
    //!    1]`) is read in the order it sent them: the caret at line 3 is named
    //!    `zeta`, the last one sent, though `alpha` is the later symbol.
    //! 8. A depth that jumps by more than one (`[A at 0, C at 2, D at 2]`)
    //!    keeps the sibling `C` beside `D`, because the chain is cut by index
    //!    (`truncate(depth)`) and not by the depth of what is already in it.

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
    fn symbols_are_read_in_line_order_whatever_order_they_come_in() {
        let symbols = vec![symbol("A", 0, 0), symbol("alpha", 3, 1), symbol("zeta", 1, 1)];
        assert_eq!(names(&chain(&symbols, 3)), ["A", "alpha"]);
        assert_eq!(names(&chain(&symbols, 2)), ["A", "zeta"]);
    }

    #[test]
    fn a_depth_that_jumps_still_closes_its_sibling() {
        let symbols = vec![symbol("A", 0, 0), symbol("C", 1, 2), symbol("D", 2, 2)];
        assert_eq!(names(&chain(&symbols, 2)), ["A", "D"]);
        assert_eq!(names(&chain(&symbols, 1)), ["A", "C"]);
    }

    #[test]
    fn siblings_replace_each_other() {
        let symbols = outline();
        assert_eq!(names(&chain(&symbols, 15)), ["impl Foo", "second"]);
        assert_eq!(names(&chain(&symbols, 3)), ["free"]);
    }
}
