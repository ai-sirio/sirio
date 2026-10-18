//! What the status row's items do when pressed: the language, the indent and
//! the line ending each open a short list; the caret's place opens a field that
//! goes to a line.
//!
//! The popovers and the control socket's `surface.editor.status` verb act
//! through the same functions (`apply_status_value`, `open_status_popover`), so
//! what the E2E drives is what a click does.
//!
//! Each choice belongs to this tab and is not kept across a relaunch:
//! - The language is *colour only*. The language server stays the path's
//!   (`sirio_lsp::LanguageTable::for_path` never sees a `Language`), and so does
//!   the Markdown Preview. Choosing the path's own language clears the choice.
//! - The indent, spaces or tabs and how wide, drives what Tab inserts, the
//!   width Ely's guides span and the Tab width; a tab that was never given one
//!   follows the Editor setting.
//! - The line ending is the one the next save writes; changing it is an unsaved
//!   edit (`Editor::set_line_ending` marks it).
//!
//! What a refused item does: a snapshot is read-only, so its indent and line
//! ending are not offered (the items draw but take no press, and the verb
//! answers why); its language is. Markdown Preview shows no code, so the
//! language and go-to-line items take no press there; the indent and the line
//! ending still describe the file and stay pressable.

use std::rc::Rc;

use ely_gpui_component::forms::{Choice, Input, InputEvent, ListBox};
use ely_gpui_component::overlays::Popover;
use gpui::{
    AnyElement, App, Context, Entity, Focusable as _, KeyDownEvent, SharedString, Window, div,
    prelude::*, px,
};
use sirio_markdown::LineEnding;
use sirio_theme::Theme;

use super::code_surface::line_start_offset;
use super::{FileView, MarkdownMode};
use crate::editor::{Language, LoadStatus};
use crate::ely_ui::new_input;

/// The widest indent a choice may name: a larger one is a typo, and a Tab would
/// insert that many spaces.
const MAX_INDENT: usize = 32;

/// An item of the status row that takes a press.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StatusItem {
    Language,
    Indent,
    LineEnding,
    /// The caret's place.
    Goto,
}

impl StatusItem {
    /// The item named as the control socket names it: `language`, `indent`,
    /// `line_ending` or `goto`.
    pub fn parse(word: &str) -> Option<Self> {
        match word {
            "language" => Some(Self::Language),
            "indent" => Some(Self::Indent),
            "line_ending" => Some(Self::LineEnding),
            "goto" => Some(Self::Goto),
            _ => None,
        }
    }

    /// The word `parse` reads.
    pub fn word(self) -> &'static str {
        match self {
            Self::Language => "language",
            Self::Indent => "indent",
            Self::LineEnding => "line_ending",
            Self::Goto => "goto",
        }
    }

    fn popover_id(self) -> &'static str {
        match self {
            Self::Language => "status-language-popover",
            Self::Indent => "status-indent-popover",
            Self::LineEnding => "status-line-ending-popover",
            Self::Goto => "status-goto-popover",
        }
    }
}

/// The stable id of a language: its Markdown fence tag, except that plain text
/// is `plain` (a fence has no tag for it) and the shell is `shell` (its fence tag
/// is `bash`, which `parse_language` also reads).
pub fn language_id(language: Language) -> &'static str {
    match language {
        Language::PlainText => "plain",
        Language::Shell => "shell",
        other => other.fence_tag(),
    }
}

/// A language named by its display name (`Rust`, `C++`, `plain text`), its id
/// (`rust`, `cpp`, `plain`) or its fence tag (`bash`), in any case.
pub fn parse_language(value: &str) -> Option<Language> {
    let value = value.trim().to_ascii_lowercase();
    Language::ALL.into_iter().find(|language| {
        language.name().eq_ignore_ascii_case(&value)
            || language_id(*language) == value
            || (!language.fence_tag().is_empty() && language.fence_tag() == value)
    })
}

/// The value a list row of an indent carries and the verb takes: `spaces:4`,
/// `tabs:2`.
fn indent_value(spaces: bool, size: usize) -> String {
    format!("{}:{size}", if spaces { "spaces" } else { "tabs" })
}

/// `spaces:N` or `tabs:N` with `N` from one to `MAX_INDENT`.
fn parse_indent(value: &str) -> Result<(bool, usize), String> {
    let wrong = || format!("'{value}' is not an indent: use spaces:N or tabs:N, N from 1 to {MAX_INDENT}");
    let (kind, size) = value.trim().split_once(':').ok_or_else(wrong)?;
    let spaces = match kind.to_ascii_lowercase().as_str() {
        "spaces" => true,
        "tabs" => false,
        _ => return Err(wrong()),
    };
    let size: usize = size.trim().parse().map_err(|_| wrong())?;
    if size == 0 || size > MAX_INDENT {
        return Err(wrong());
    }
    Ok((spaces, size))
}

fn parse_line_ending(value: &str) -> Result<LineEnding, String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "lf" => Ok(LineEnding::Lf),
        "crlf" => Ok(LineEnding::Crlf),
        _ => Err(format!("'{value}' is not a line ending: use lf or crlf")),
    }
}

/// Runs the way out of a popover: closes it and hands focus back.
type Close = Rc<dyn Fn(&mut Window, &mut App)>;

/// Escape in a panel: the popover closes on it (its own handler, one level up,
/// which this one does not stop), and the keyboard goes to the code after.
fn escape_returns_focus(event: &KeyDownEvent, view: &Entity<FileView>, cx: &mut App) {
    if event.keystroke.key == "escape" {
        view.update(cx, |view, _| view.focus_code_on_close = true);
    }
}

impl FileView {
    /// The status row's open popover, if any.
    pub fn status_popover(&self) -> Option<StatusItem> {
        self.status_popover
    }

    /// Why `item` takes no press now, or `None` when it does.
    pub(super) fn item_refusal(&self, item: StatusItem) -> Option<&'static str> {
        let loaded = self
            .editor()
            .is_some_and(|editor| editor.status() == &LoadStatus::Loaded)
            && self.code_editor.is_some();
        if !loaded {
            return Some("the file is not loaded yet");
        }
        let preview = self.effective_mode() == MarkdownMode::Preview;
        match item {
            StatusItem::Language if preview => {
                Some("the language colours the code, which Markdown Preview does not show")
            }
            StatusItem::Goto if preview => {
                Some("go to line needs the code, which Markdown Preview does not show")
            }
            StatusItem::Indent if self.is_snapshot() => {
                Some("a snapshot is read-only: its indent is not changed")
            }
            StatusItem::LineEnding if self.is_snapshot() => {
                Some("a snapshot is read-only: its line ending is not changed")
            }
            _ => None,
        }
    }

    /// Colours this tab's code as `language`. Choosing the path's own language
    /// ends the choice. Colour only: the language server stays the path's.
    pub fn set_language_override(
        &mut self,
        language: Language,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        if let Some(why) = self.item_refusal(StatusItem::Language) {
            return Err(why.to_owned());
        }
        let path_language = self.editor().map_or(Language::PlainText, |editor| editor.language());
        self.language_override = (language != path_language).then_some(language);
        self.refresh_highlighter(cx);
        cx.notify();
        Ok(())
    }

    /// Indents this tab with spaces or tabs, `size` wide, from now on.
    pub fn set_indent(
        &mut self,
        spaces: bool,
        size: usize,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        if let Some(why) = self.item_refusal(StatusItem::Indent) {
            return Err(why.to_owned());
        }
        if size == 0 || size > MAX_INDENT {
            return Err(format!("an indent is from 1 to {MAX_INDENT} wide"));
        }
        self.indent_override = Some((spaces, size));
        // The code editor follows on the next frame (`sync_code_prefs`).
        cx.notify();
        Ok(())
    }

    /// Makes the next save write `line_ending`; until then the tab has an
    /// unsaved change.
    pub fn set_line_ending_choice(
        &mut self,
        line_ending: LineEnding,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        if let Some(why) = self.item_refusal(StatusItem::LineEnding) {
            return Err(why.to_owned());
        }
        let editor = self.editor_mut().ok_or("the file is not loaded yet")?;
        editor.set_line_ending(line_ending);
        cx.notify();
        Ok(())
    }

    /// Selects the start of line `line`, from one; a line past the end is the
    /// last one. Answers the line it reached.
    pub fn go_to_line(&mut self, line: usize, cx: &mut Context<Self>) -> Result<usize, String> {
        if let Some(why) = self.item_refusal(StatusItem::Goto) {
            return Err(why.to_owned());
        }
        if line == 0 {
            return Err("lines count from 1".to_owned());
        }
        let code = self.code_editor.clone().ok_or("the file is not loaded yet")?;
        let (offset, reached) = {
            let text = code.read(cx).text();
            let last = text.matches('\n').count();
            let at = (line - 1).min(last);
            (line_start_offset(text, at), at + 1)
        };
        code.update(cx, |code, cx| code.select([offset..offset], cx));
        cx.notify();
        Ok(reached)
    }

    /// What a list row or the verb's `value` does for `item`: the one function
    /// the popovers and `surface.editor.status` share.
    pub fn apply_status_value(
        &mut self,
        item: StatusItem,
        value: &str,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        match item {
            StatusItem::Language => {
                let language = parse_language(value).ok_or_else(|| {
                    format!("'{value}' is not a language: use a name (Rust), an id (rust, plain, shell, cpp) or a fence tag")
                })?;
                self.set_language_override(language, cx)
            }
            StatusItem::Indent => {
                let (spaces, size) = parse_indent(value)?;
                self.set_indent(spaces, size, cx)
            }
            StatusItem::LineEnding => {
                let line_ending = parse_line_ending(value)?;
                self.set_line_ending_choice(line_ending, cx)
            }
            StatusItem::Goto => {
                let line: usize = value
                    .trim()
                    .parse()
                    .map_err(|_| format!("'{value}' is not a line number"))?;
                self.go_to_line(line, cx).map(|_| ())
            }
        }
    }

    /// The control socket's `surface.editor.status`: `open=true` opens the
    /// item's popover, `open=false` closes it, and otherwise `value` is applied
    /// as a choice in the popover would apply it.
    pub fn control_status(
        &mut self,
        item: &str,
        value: Option<&str>,
        open: Option<bool>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let item = StatusItem::parse(item).ok_or_else(|| {
            format!("'{item}' is not a status item: use language, indent, line_ending or goto")
        })?;
        match open {
            Some(true) => self.open_status_popover(item, window, cx),
            Some(false) => {
                if self.status_popover == Some(item) {
                    self.status_popover = None;
                    self.focus_code_on_close = true;
                    cx.notify();
                }
                Ok(())
            }
            None => {
                let value = value.ok_or_else(|| format!("status {} needs a value", item.word()))?;
                self.apply_status_value(item, value, cx)
            }
        }
    }

    /// Opens `item`'s popover, and closes whichever was open: one at a time.
    pub(super) fn open_status_popover(
        &mut self,
        item: StatusItem,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        if let Some(why) = self.item_refusal(item) {
            return Err(why.to_owned());
        }
        if item == StatusItem::Goto {
            let input = self.goto_field(window, cx);
            input.update(cx, |input, cx| input.set_text("", cx));
            self.goto_focus_pending = true;
        }
        self.status_popover = Some(item);
        cx.notify();
        Ok(())
    }

    /// A press on an item: opens its popover, or closes it when it is open.
    pub(super) fn toggle_status_popover(
        &mut self,
        item: StatusItem,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.status_popover == Some(item) {
            self.status_popover = None;
            self.focus_code_on_close = true;
            cx.notify();
        } else if let Err(why) = self.open_status_popover(item, window, cx) {
            eprintln!("[files] status {}: {why}", item.word());
        }
    }

    /// `item`'s panel closed, by whichever way: Escape, a press outside, a
    /// choice, or the owner.
    pub(super) fn status_popover_closed(
        &mut self,
        item: StatusItem,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.status_popover == Some(item) {
            self.status_popover = None;
            cx.notify();
        }
        // Escape, a finished choice, Enter and the item pressed again send the
        // keyboard to the code, whatever held it before the panel opened; a
        // press outside leaves it where the press put it.
        if std::mem::take(&mut self.focus_code_on_close) {
            self.focus_target(cx).focus(window, cx);
        }
    }

    /// The go-to-line field, built the first time it is needed (Ely's input
    /// needs the window the constructor does not have). Enter goes to the line.
    fn goto_field(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<ely_gpui_component::forms::TextInput> {
        if let Some(input) = &self.goto_input {
            return input.clone();
        }
        let input = new_input(window, cx, "", None, "Line number");
        cx.subscribe_in(&input, window, |view, input, event: &InputEvent, window, cx| {
            if matches!(event, InputEvent::Submit) {
                let text = input.read(cx).text().to_owned();
                view.submit_goto(&text, window, cx);
            }
        })
        .detach();
        self.goto_input = Some(input.clone());
        input
    }

    /// Enter in the go-to-line field: a line number goes there and closes the
    /// popover; anything else does nothing, and the field keeps what was typed.
    fn submit_goto(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Ok(line) = text.trim().parse::<usize>() else {
            return;
        };
        if self.go_to_line(line, cx).is_ok() {
            // The panel follows the owner (`Popover::open`) on the next draw
            // and hands focus back; the code takes it now.
            self.status_popover = None;
            self.focus_code_on_close = true;
            self.focus_target(cx).focus(window, cx);
            cx.notify();
        }
    }

    /// `trigger` is the item, built with the press that opens its popover
    /// (`None` when the item takes no press now). The popover is held open by
    /// `status_popover`, so a click, the control socket and a close all go
    /// through the one field.
    pub(super) fn with_status_popover(
        &self,
        entity: &Entity<Self>,
        item: StatusItem,
        theme: Theme,
        trigger: impl FnOnce(Option<Rc<dyn Fn(&mut Window, &mut App)>>) -> AnyElement + 'static,
    ) -> AnyElement {
        if self.item_refusal(item).is_some() {
            return trigger(None);
        }
        let held = self.status_popover == Some(item);
        let (opener, closer, body) = (entity.clone(), entity.clone(), entity.clone());
        Popover::with_opener(
            item.popover_id(),
            move |_toggle| {
                let press: Rc<dyn Fn(&mut Window, &mut App)> = Rc::new(move |window, cx| {
                    opener.update(cx, |view, cx| view.toggle_status_popover(item, window, cx));
                });
                trigger(Some(press))
            },
            move |close, window, cx| {
                let entity = body.clone();
                body.update(cx, |view, cx| {
                    view.status_popover_body(item, theme, entity, close, window, cx)
                })
            },
        )
        .open(held)
        .on_close(move |window, cx| {
            closer.update(cx, |view, cx| view.status_popover_closed(item, window, cx));
        })
        .into_any_element()
    }

    /// The panel of an open popover.
    fn status_popover_body(
        &mut self,
        item: StatusItem,
        theme: Theme,
        entity: Entity<Self>,
        close: Close,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (choices, current): (Vec<Choice>, String) = match item {
            StatusItem::Language => (
                Language::ALL
                    .into_iter()
                    .map(|language| Choice::new(language_id(language), language.name()))
                    .collect(),
                language_id(self.effective_language()).to_owned(),
            ),
            StatusItem::Indent => {
                let (spaces, size) = self.effective_indent(cx);
                let choices = [true, false]
                    .into_iter()
                    .flat_map(|spaces| [2, 4, 8].into_iter().map(move |size| (spaces, size)))
                    .map(|(spaces, size)| {
                        let label = if spaces {
                            format!("Spaces: {size}")
                        } else {
                            format!("Tab Size: {size}")
                        };
                        Choice::new(indent_value(spaces, size), label)
                    })
                    .collect();
                (choices, indent_value(spaces, size))
            }
            StatusItem::LineEnding => {
                let crlf = self
                    .editor()
                    .is_some_and(|editor| editor.line_ending() == LineEnding::Crlf);
                (
                    vec![Choice::new("lf", "LF"), Choice::new("crlf", "CRLF")],
                    if crlf { "crlf" } else { "lf" }.to_owned(),
                )
            }
            StatusItem::Goto => return self.goto_body(theme, window, cx),
        };
        let id = match item {
            StatusItem::Language => "status-language-list",
            StatusItem::Indent => "status-indent-list",
            _ => "status-line-ending-list",
        };
        let escaping = entity.clone();
        div()
            .debug_selector(move || id.to_owned())
            .w(px(190.0))
            .text_color(theme.ely.fg)
            .on_key_down(move |event, _, cx| escape_returns_focus(event, &escaping, cx))
            .child(
                ListBox::new(id, choices)
                    .selected([SharedString::from(current)])
                    .on_change(move |values, window, cx| {
                        let Some(value) = values.first() else {
                            return;
                        };
                        let value = value.to_string();
                        let outcome = entity
                            .update(cx, |view, cx| view.apply_status_value(item, &value, cx));
                        if let Err(why) = outcome {
                            eprintln!("[files] status {}: {why}", item.word());
                        }
                        close(window, cx);
                        entity.update(cx, |view, cx| view.focus_target(cx).focus(window, cx));
                    }),
            )
            .into_any_element()
    }

    /// The go-to-line panel: what the field takes, then the field.
    fn goto_body(&mut self, theme: Theme, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let input = self.goto_field(window, cx);
        if self.goto_focus_pending {
            // The panel took the keyboard as it opened; typing starts at once.
            self.goto_focus_pending = false;
            input.read(cx).focus_handle(cx).focus(window, cx);
        }
        let lines = self
            .code_editor
            .as_ref()
            .map_or(1, |code| code.read(cx).text().matches('\n').count() + 1);
        let escaping = cx.entity();
        div()
            .debug_selector(|| "status-goto-panel".to_owned())
            .w(px(200.0))
            .flex()
            .flex_col()
            .gap(px(6.0))
            .text_color(theme.ely.fg)
            .on_key_down(move |event, _, cx| escape_returns_focus(event, &escaping, cx))
            .child(
                div()
                    .text_size(theme.typography.footnote)
                    .text_color(theme.ely.fg_muted)
                    .child(format!("Go to line, 1 to {lines}")),
            )
            .child(Input::new(&input))
            .into_any_element()
    }
}
