//! Find and replace in the file's code, with Ely's `FindWidget`.
//!
//! The widget and `find_all` are Ely's; this module owns the state around
//! them: which matches the query has in the code editor's text, which one is
//! current, and the washes that show them.
//!
//! How it can fail, and what guards each one: a unit test in `file_view.rs`'s
//! `tests`, named under its failure, or nothing.
//!
//! - Replace-one leaves the matches where they stood before the edit, so a
//!   later match is replaced at an offset the earlier replacement moved. The
//!   test puts a multi-byte character before the matches and a replacement
//!   longer than the match. Test:
//!   `replacing_one_match_moves_on_to_the_next_and_keeps_the_later_ones_right`.
//! - Replace-one with a replacement that holds the query finds that text
//!   again and never moves on (`x` to `xx`). Same test.
//! - Replace-all is one edit per match, so it takes one undo per match. Test:
//!   `replacing_every_match_is_one_undo_step`.
//! - A text change in the code leaves the matches where the old text had
//!   them. Test: `an_edit_in_the_code_recomputes_the_matches`, which checks the
//!   count only. No test or E2E assertion checks the washes.
//! - Escape in the code with find open collapses the cursors instead of
//!   closing find, or closes find and the hunk together, or swallows the
//!   Escape that should collapse the cursors once nothing is open. Test:
//!   `escape_closes_find_then_a_hunk_and_leaves_the_cursors_until_none_is_open`.
//! - Escape in the query field closes find but leaves the keyboard in
//!   nothing, so the next key goes nowhere. Test:
//!   `escape_in_the_query_closes_find_and_hands_the_keyboard_to_the_code`.
//! - A card already drawn for the code stays when the pointer slides from the
//!   code onto the bar. The code editor itself says when the pointer leaves it
//!   (vendored Ely, `LOCAL-CHANGES.md`). Test:
//!   `the_pointer_sliding_from_the_code_onto_the_find_bar_drops_the_card`.
//! - Find pressed in the query field, or with the current match selected in
//!   the code, replaces the pattern being typed with the text it matched.
//!   Tests: `the_find_key_in_the_query_selects_it_and_never_reseeds_it` and
//!   `a_selection_that_is_the_current_match_is_not_a_new_query`.
//! - A selection seeds a regular expression verbatim, so `f(` is an error.
//!   Test: `a_selection_seeds_a_regular_expression_query_literally`.
//! - Find steps, or replaces, while the code is not shown (Preview, a file
//!   still loading) act on code that is not drawn. `find_step` and
//!   `replace_matches` refuse there. Nothing guards it: no test was asked for,
//!   and the E2E never does find in Preview.

use std::ops::Range;

use ely_gpui_component::editor::{FindOptions, FindWidget, find_all};
use ely_gpui_component::forms::{InputEvent, TextInput};
use gpui::{
    AnyElement, App, Context, Entity, Focusable as _, Hsla, MouseButton, SharedString,
    Subscription, Window, div, prelude::*, px,
};
use sirio_theme::Theme;

use super::FileView;

/// An open find: its two fields, what they found, and which match is current.
pub(super) struct FindState {
    pub(super) query: Entity<TextInput>,
    pub(super) replacement: Entity<TextInput>,
    pub(super) options: FindOptions,
    /// Byte ranges in the code editor's text, in order; empty when nothing matches.
    pub(super) matches: Vec<Range<usize>>,
    /// An index into `matches`: `None` only while there is no match.
    pub(super) current: Option<usize>,
    /// Why the query found nothing, when it is a pattern that does not compile.
    pub(super) error: Option<SharedString>,
    pub(super) replace_open: bool,
    /// The washes last pushed to the code editor: every match, and the current one.
    washes: (Hsla, Hsla),
    _subscriptions: Vec<Subscription>,
}

/// The first match that starts at or after `at`, wrapping to the first one
/// when none does; `None` without matches.
fn current_after(matches: &[Range<usize>], at: usize) -> Option<usize> {
    if matches.is_empty() {
        return None;
    }
    Some(matches.iter().position(|hit| hit.start >= at).unwrap_or(0))
}

/// The wash of every match, and the stronger one of the current match.
fn washes(cx: &App) -> (Hsla, Hsla) {
    let theme = Theme::get(cx);
    (theme.ely.warning.opacity(0.2), theme.ely.selection)
}

/// The last line of a pattern error: the regex crate's message draws the
/// pattern above it, and only that last line fits the widget's count slot.
fn last_line(message: &str) -> &str {
    message.lines().last().unwrap_or(message).trim()
}

impl FileView {
    /// Opens find on the code, or brings the open one back to the query.
    ///
    /// `query` replaces the query's text (`None` leaves it, empty for a find
    /// that was not open). `replace` opens the replace row with that text
    /// (`None` leaves the row as it is, closed for a find that was not open);
    /// a read-only code (a snapshot, or a file not unlocked) never shows the
    /// row. The query takes the keyboard, its text selected. Returns `false`,
    /// opening nothing, when the code is not what the tab shows (no file yet,
    /// or the Markdown preview).
    pub fn open_find(
        &mut self,
        query: Option<String>,
        options: FindOptions,
        replace: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.code_shown() {
            return false;
        }
        let mut find = match self.find.take() {
            Some(find) => find,
            None => self.new_find(window, cx),
        };
        find.options = options;
        if let Some(query) = query {
            find.query.update(cx, |field, cx| field.set_text(query, cx));
        }
        if let Some(replace) = replace {
            find.replacement
                .update(cx, |field, cx| field.set_text(replace, cx));
            find.replace_open = true;
        }
        let query = find.query.clone();
        self.find = Some(find);
        self.find_recompute(None, true, cx);
        let length = query.read(cx).text().len();
        query.update(cx, |field, cx| field.select(0..length, cx));
        query.read(cx).focus_handle(cx).focus(window, cx);
        cx.notify();
        true
    }

    /// Makes the two fields and listens to the query: a change recomputes the
    /// matches, Enter steps to the next one.
    fn new_find(&self, window: &mut Window, cx: &mut Context<Self>) -> FindState {
        let query = cx.new(|cx| TextInput::new(window, cx).placeholder("Find"));
        let replacement = cx.new(|cx| TextInput::new(window, cx).placeholder("Replace"));
        let subscription = cx.subscribe_in(
            &query,
            window,
            |view, _, event: &InputEvent, window, cx| match event {
                InputEvent::Changed => view.find_recompute(None, true, cx),
                InputEvent::Submit => view.find_step(true, window, cx),
                _ => {}
            },
        );
        FindState {
            query,
            replacement,
            options: FindOptions::default(),
            matches: Vec::new(),
            current: None,
            error: None,
            replace_open: false,
            washes: washes(cx),
            _subscriptions: vec![subscription],
        }
    }

    /// Finds the query again in the code's text, and paints it.
    ///
    /// `current` becomes the first match that starts at or after `from`, or
    /// after the primary selection's start when `from` is `None`: the start,
    /// not the caret, because a step selects its match, which puts the caret
    /// at the match's end. With `reveal` the code selects the current match
    /// and scrolls to it; a recompute caused by typing in the code does not,
    /// because that would take the caret from the text being typed.
    ///
    /// Called directly by everything here that changes the matches, so the
    /// accessors are right as soon as it returns; the query's and the code's
    /// change events call it again later and land on the same answer.
    fn find_recompute(&mut self, from: Option<usize>, reveal: bool, cx: &mut Context<Self>) {
        let Some(code) = self.code_editor.clone() else {
            return;
        };
        let Some(find) = self.find.as_mut() else {
            return;
        };
        let (matches, error) = match find_all(
            code.read(cx).text(),
            find.query.read(cx).text(),
            find.options,
        ) {
            Ok(matches) => (matches, None),
            Err(message) => (Vec::new(), Some(message.into())),
        };
        let at = from.unwrap_or_else(|| code.read(cx).primary().range().start);
        find.current = current_after(&matches, at);
        find.matches = matches;
        find.error = error;
        let select = reveal.then(|| find.current.map(|ix| find.matches[ix].clone()));
        self.paint_find(cx);
        if let Some(Some(range)) = select {
            code.update(cx, |code, cx| code.select([range], cx));
        }
        cx.notify();
    }

    /// The code's text changed (`code_changed`): the matches move with it.
    pub(super) fn find_text_changed(&mut self, cx: &mut Context<Self>) {
        self.find_recompute(None, false, cx);
    }

    /// Hands the code editor the washes: every match dim, the current one strong.
    /// Find owns the code editor's backgrounds; nothing else in Sirio sets them.
    fn paint_find(&mut self, cx: &mut Context<Self>) {
        let Some(code) = self.code_editor.clone() else {
            return;
        };
        let washes = washes(cx);
        let backgrounds = match self.find.as_mut() {
            Some(find) => {
                find.washes = washes;
                find.matches
                    .iter()
                    .enumerate()
                    .map(|(ix, range)| {
                        let wash = if Some(ix) == find.current { washes.1 } else { washes.0 };
                        (range.clone(), wash)
                    })
                    .collect()
            }
            None => Vec::new(),
        };
        code.update(cx, |code, cx| code.set_backgrounds(backgrounds, cx));
    }

    /// Repaints the washes when the theme moved them since they were pushed.
    /// Called every frame, like `sync_code_prefs`.
    pub(super) fn sync_find_washes(&mut self, cx: &mut Context<Self>) {
        if self
            .find
            .as_ref()
            .is_some_and(|find| find.washes != washes(cx))
        {
            self.paint_find(cx);
        }
    }

    /// Moves to the next match (`forward`) or the previous one, wrapping, and
    /// selects it in the code. Does nothing without a match, or while the code
    /// is not shown.
    pub fn find_step(&mut self, forward: bool, _window: &mut Window, cx: &mut Context<Self>) {
        if !self.code_shown() {
            return;
        }
        let Some(code) = self.code_editor.clone() else {
            return;
        };
        let Some(find) = self.find.as_mut() else {
            return;
        };
        let total = find.matches.len();
        let Some(current) = find.current else {
            return;
        };
        let next = if forward {
            (current + 1) % total
        } else {
            (current + total - 1) % total
        };
        find.current = Some(next);
        let range = find.matches[next].clone();
        self.paint_find(cx);
        code.update(cx, |code, cx| code.select([range], cx));
        cx.notify();
    }

    /// Whether the code takes edits: not a snapshot, and not read-only.
    fn find_can_replace(&self, cx: &App) -> bool {
        self.snapshot.is_none()
            && self
                .code_editor
                .as_ref()
                .is_some_and(|code| !code.read(cx).is_read_only())
    }

    /// Replaces the current match with the replacement field's text, or every
    /// match (`all`). The text goes in as typed: `$1` is not a capture group,
    /// even under a regular expression. Replace-all is one `CodeEditor::edit`,
    /// so one undo. After replacing one, the next match becomes current: the
    /// first one that starts at or after the end of the text just written (so
    /// a replacement that holds the query is not found again), wrapping to the
    /// first. Answers how many matches were replaced: `Ok(0)` when there was
    /// none to replace, `Err` when the call is refused: find is not open, the
    /// code is not shown (the file is loading or in Preview), or the code takes
    /// no edits.
    pub fn replace_matches(
        &mut self,
        all: bool,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<usize, &'static str> {
        let Some(find) = self.find.as_ref() else {
            return Err("find is not open");
        };
        let Some(code) = self.code_editor.clone().filter(|_| self.code_shown()) else {
            return Err("the code is not shown");
        };
        if !self.find_can_replace(cx) {
            return Err("the file is read-only");
        }
        // `edit` takes text as it is, and the code never holds a carriage return.
        let with = find
            .replacement
            .read(cx)
            .text()
            .replace("\r\n", "\n")
            .replace('\r', "\n");
        let chosen: Vec<Range<usize>> = if all {
            find.matches.clone()
        } else {
            find.current
                .and_then(|ix| find.matches.get(ix))
                .cloned()
                .into_iter()
                .collect()
        };
        let Some(first) = chosen.first() else {
            return Ok(0);
        };
        let resume = (!all).then(|| first.start + with.len());
        let replaced = chosen.len();
        let edits: Vec<(Range<usize>, String)> = chosen
            .into_iter()
            .map(|range| (range, with.clone()))
            .collect();
        code.update(cx, |code, cx| code.edit(edits, cx));
        self.find_recompute(resume, resume.is_some(), cx);
        Ok(replaced)
    }

    /// Closes find: its washes go, and the keyboard goes back to the code.
    pub fn close_find(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.find.take().is_none() {
            return;
        }
        self.paint_find(cx);
        self.focus_target(cx).focus(window, cx);
        cx.notify();
    }

    /// The tab's Find and Replace keys.
    ///
    /// With the keyboard in the query or the replacement field they only select
    /// the query's text and bring the keyboard back to it (and, for Replace,
    /// open the row where the code takes edits): seeding there would replace
    /// what is being typed with the code's selection, which after any step is
    /// the current match. From the code, the primary selection becomes the query
    /// when it is not empty, holds no line break and is not the current match
    /// itself; under a regular expression it is escaped, so it is searched as
    /// the text it is. Otherwise the query stays. Replace opens the row only
    /// where the code takes edits, and falls back to Find where it does not.
    pub(super) fn find_from_key(
        &mut self,
        replace: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(code) = self.code_editor.clone() else {
            return;
        };
        let can_replace = self.find_can_replace(cx);
        if let Some(find) = self.find.as_mut() {
            let in_fields = find.query.read(cx).focus_handle(cx).contains_focused(window, cx)
                || find
                    .replacement
                    .read(cx)
                    .focus_handle(cx)
                    .contains_focused(window, cx);
            if in_fields {
                if replace && can_replace {
                    find.replace_open = true;
                }
                let query = find.query.clone();
                let length = query.read(cx).text().len();
                query.update(cx, |field, cx| field.select(0..length, cx));
                query.read(cx).focus_handle(cx).focus(window, cx);
                cx.notify();
                return;
            }
        }
        let options = self.find.as_ref().map_or_else(FindOptions::default, |find| find.options);
        let query = {
            let code = code.read(cx);
            let range = code.primary().range();
            let is_current_match = self
                .find
                .as_ref()
                .and_then(|find| find.current.and_then(|ix| find.matches.get(ix)))
                .is_some_and(|hit| *hit == range);
            let selected = &code.text()[range];
            (!selected.is_empty() && !selected.contains('\n') && !is_current_match).then(|| {
                if options.regex {
                    regex::escape(selected)
                } else {
                    selected.to_owned()
                }
            })
        };
        let replace = (replace && can_replace).then(|| {
            self.find
                .as_ref()
                .map_or_else(String::new, |find| find.replacement.read(cx).text().to_owned())
        });
        self.open_find(query, options, replace, window, cx);
    }

    /// How many matches find holds, `None` while find is closed.
    pub fn find_matches(&self) -> Option<usize> {
        self.find.as_ref().map(|find| find.matches.len())
    }

    /// The current match as an index into the matches, counting from 0: the
    /// number Ely's widget shows, less one. `None` while find is closed or
    /// has no match.
    pub fn find_current(&self) -> Option<usize> {
        self.find.as_ref().and_then(|find| find.current)
    }

    /// Why the query is not a pattern, the regex crate's whole message, while
    /// it is not. `None` while find is closed or the query compiles.
    pub fn find_error(&self) -> Option<&str> {
        self.find.as_ref().and_then(|find| find.error.as_deref())
    }

    /// The find bar docked above the code, when find is open and the code is
    /// shown. The code below it is as much shorter as the bar is tall, so the
    /// bar covers none of the text, the minimap or the read-only banner.
    pub(super) fn render_find(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let find = self.find.as_ref()?;
        if !self.code_shown() {
            return None;
        }
        let entity = cx.entity();
        let options = {
            let entity = entity.clone();
            move |options, _: &mut Window, cx: &mut App| {
                entity.update(cx, |view, cx| {
                    if let Some(find) = view.find.as_mut() {
                        find.options = options;
                    }
                    view.find_recompute(None, true, cx);
                });
            }
        };
        let step = {
            let entity = entity.clone();
            move |forward, window: &mut Window, cx: &mut App| {
                entity.update(cx, |view, cx| view.find_step(forward, window, cx));
            }
        };
        let close = {
            let entity = entity.clone();
            move |window: &mut Window, cx: &mut App| {
                entity.update(cx, |view, cx| view.close_find(window, cx));
            }
        };
        let mut widget = FindWidget::new("file-find", &find.query, find.current, find.matches.len())
            .options(find.options)
            .on_options(options)
            .on_step(step)
            .on_close(close);
        if let Some(message) = &find.error {
            widget = widget.error(last_line(message).to_owned());
        }
        if self.find_can_replace(cx) {
            let fold = entity.clone();
            let replace = entity.clone();
            widget = widget
                .on_toggle_replace(move |open, _, cx| {
                    fold.update(cx, |view, cx| {
                        if let Some(find) = view.find.as_mut() {
                            find.replace_open = open;
                        }
                        cx.notify();
                    });
                })
                .on_replace(move |all, window, cx| {
                    replace.update(cx, |view, cx| {
                        let _ = view.replace_matches(all, window, cx);
                    });
                });
            if find.replace_open {
                widget = widget.replace(&find.replacement);
            }
        }
        // The press stops here: the tab's own press hands the keyboard to the
        // code, which would take it back from the field the press was for.
        // The bar spans the code's width with the widget right-aligned in it;
        // the widget shrinks in a pane narrower than its own fixed width
        // rather than running off the left edge, and `overflow_hidden` clips
        // what even the shrunken widget cannot fit, so nothing paints past the
        // pane's edge. The bar's padding is small so the query field keeps what
        // the widget's other controls leave of a narrow pane.
        let theme = Theme::get(cx);
        Some(
            div()
                .id("file-find-bar")
                .debug_selector(|| "file-find-bar".into())
                .w_full()
                .flex()
                .justify_end()
                .overflow_hidden()
                .px(px(4.0))
                .py(px(4.0))
                .bg(theme.ely.surface)
                .border_b_1()
                .border_color(theme.ely.border)
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation())
                .child(
                    div()
                        .id("file-find")
                        .debug_selector(|| "file-find".into())
                        .flex()
                        .min_w_0()
                        .child(widget),
                )
                .into_any_element(),
        )
    }
}
