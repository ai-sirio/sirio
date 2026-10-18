//! The code editor's preferences, published app-wide.
//!
//! The twelve `editor.*` settings live in `sirio_persistence`. The host
//! installs them here as a GPUI global, so every editor reads one set of view
//! options and a change in Settings reaches each open editor. Code lens and
//! inlay hints have no slot in Ely's `ViewOptions`; they stay on this type for
//! the editor's own code to read.

use std::ops::RangeInclusive;

use ely_gpui_component::editor::{CursorShape, LineNumbers, ViewOptions};
use gpui::App;
use sirio_persistence::{
    AppSettings, EditorCursorShape, EditorLineNumbers, settings_keys, settings_ranges,
};

/// The code editor's preferences, as the Settings snapshot carries them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EditorPrefs {
    pub minimap: bool,
    pub sticky_scroll: bool,
    pub indent_guides: bool,
    pub rainbow_brackets: bool,
    pub code_lens: bool,
    pub inlay_hints: bool,
    pub whitespace: bool,
    pub ruler: bool,
    /// The column the ruler is drawn at, within `settings_ranges::EDITOR_RULER_COLUMN`.
    pub ruler_column: usize,
    pub line_numbers: EditorLineNumbers,
    pub cursor_shape: EditorCursorShape,
    /// Spaces a Tab inserts and an indent guide spans, within `settings_ranges::EDITOR_TAB_SIZE`.
    pub tab_size: usize,
}

impl gpui::Global for EditorPrefs {}

impl Default for EditorPrefs {
    /// The stored defaults, so the two cannot disagree.
    fn default() -> Self {
        Self::from_settings(&AppSettings::default())
    }
}

impl EditorPrefs {
    /// The preferences held in `settings`. A stored number is clamped into its
    /// range here as well, because `AppSettings` can be built by hand.
    pub fn from_settings(settings: &AppSettings) -> Self {
        Self {
            minimap: settings.editor_minimap,
            sticky_scroll: settings.editor_sticky_scroll,
            indent_guides: settings.editor_indent_guides,
            rainbow_brackets: settings.editor_rainbow_brackets,
            code_lens: settings.editor_code_lens,
            inlay_hints: settings.editor_inlay_hints,
            whitespace: settings.editor_whitespace,
            ruler: settings.editor_ruler,
            ruler_column: in_range(
                settings.editor_ruler_column,
                settings_ranges::EDITOR_RULER_COLUMN,
            ),
            line_numbers: settings.editor_line_numbers,
            cursor_shape: settings.editor_cursor_shape,
            tab_size: in_range(settings.editor_tab_size, settings_ranges::EDITOR_TAB_SIZE),
        }
    }

    /// The preferences in force, or the defaults when none were published.
    pub fn current(cx: &App) -> Self {
        cx.try_global::<Self>().cloned().unwrap_or_default()
    }

    /// Publishes `prefs` app-wide. A global is replaced only when its value
    /// changed or none exists yet, because `set_global` notifies every global
    /// observer; windows are refreshed only when the value changed.
    pub fn apply(prefs: Self, cx: &mut App) {
        if cx.try_global::<Self>() == Some(&prefs) {
            return;
        }
        let changed = Self::current(cx) != prefs;
        cx.set_global(prefs);
        if changed {
            cx.refresh_windows();
        }
    }

    /// The view settings Ely's `CodeEditor` takes. A ruler is drawn only when
    /// `ruler` is on.
    pub fn view_options(&self) -> ViewOptions {
        ViewOptions {
            minimap: self.minimap,
            sticky_scroll: self.sticky_scroll,
            indent_guides: self.indent_guides,
            rainbow_brackets: self.rainbow_brackets,
            whitespace: self.whitespace,
            rulers: if self.ruler {
                vec![self.ruler_column]
            } else {
                Vec::new()
            },
            tab_size: self.tab_size,
            line_numbers: match self.line_numbers {
                EditorLineNumbers::Absolute => LineNumbers::Absolute,
                EditorLineNumbers::Relative => LineNumbers::Relative,
                EditorLineNumbers::Hidden => LineNumbers::Hidden,
            },
            cursor_shape: match self.cursor_shape {
                EditorCursorShape::Line => CursorShape::Line,
                EditorCursorShape::Block => CursorShape::Block,
                EditorCursorShape::Underline => CursorShape::Underline,
            },
        }
    }

    /// The twelve settings as `key=value` pairs, comma-separated, in the order
    /// Global Constraints lists them, each key without its `editor.` prefix.
    pub fn describe(&self) -> String {
        [
            format!("minimap={}", self.minimap),
            format!("stickyScroll={}", self.sticky_scroll),
            format!("indentGuides={}", self.indent_guides),
            format!("rainbowBrackets={}", self.rainbow_brackets),
            format!("codeLens={}", self.code_lens),
            format!("inlayHints={}", self.inlay_hints),
            format!("whitespace={}", self.whitespace),
            format!("ruler={}", self.ruler),
            format!("rulerColumn={}", self.ruler_column),
            format!("lineNumbers={}", self.line_numbers.raw()),
            format!("cursorShape={}", self.cursor_shape.raw()),
            format!("tabSize={}", self.tab_size),
        ]
        .join(",")
    }

    /// Sets one setting from its text, as the Settings → Editor page stores it.
    /// `key` is the stored key (`editor.minimap`) or its suffix (`minimap`). An
    /// unknown key, a value that does not parse and a number outside its range
    /// are errors: nothing is clamped.
    pub fn set_from_text(&mut self, key: &str, value: &str) -> Result<(), String> {
        let stored = if key.starts_with("editor.") {
            key.to_string()
        } else {
            format!("editor.{key}")
        };
        let flag = || {
            value
                .parse::<bool>()
                .map_err(|_| format!("{key} takes true or false, not '{value}'"))
        };
        let number = |range: RangeInclusive<i64>| {
            value
                .parse::<i64>()
                .ok()
                .filter(|number| range.contains(number))
                .map(|number| number as usize)
                .ok_or_else(|| {
                    format!(
                        "{key} takes a number from {} to {}, not '{value}'",
                        range.start(),
                        range.end()
                    )
                })
        };
        match stored.as_str() {
            settings_keys::EDITOR_MINIMAP => self.minimap = flag()?,
            settings_keys::EDITOR_STICKY_SCROLL => self.sticky_scroll = flag()?,
            settings_keys::EDITOR_INDENT_GUIDES => self.indent_guides = flag()?,
            settings_keys::EDITOR_RAINBOW_BRACKETS => self.rainbow_brackets = flag()?,
            settings_keys::EDITOR_CODE_LENS => self.code_lens = flag()?,
            settings_keys::EDITOR_INLAY_HINTS => self.inlay_hints = flag()?,
            settings_keys::EDITOR_WHITESPACE => self.whitespace = flag()?,
            settings_keys::EDITOR_RULER => self.ruler = flag()?,
            settings_keys::EDITOR_RULER_COLUMN => {
                self.ruler_column = number(settings_ranges::EDITOR_RULER_COLUMN)?
            }
            settings_keys::EDITOR_LINE_NUMBERS => {
                self.line_numbers = EditorLineNumbers::parse(value).ok_or_else(|| {
                    format!("{key} takes absolute, relative or hidden, not '{value}'")
                })?
            }
            settings_keys::EDITOR_CURSOR_SHAPE => {
                self.cursor_shape = EditorCursorShape::parse(value).ok_or_else(|| {
                    format!("{key} takes line, block or underline, not '{value}'")
                })?
            }
            settings_keys::EDITOR_TAB_SIZE => {
                self.tab_size = number(settings_ranges::EDITOR_TAB_SIZE)?
            }
            _ => return Err(format!("unknown editor setting '{key}'")),
        }
        Ok(())
    }
}

fn in_range(value: i64, range: RangeInclusive<i64>) -> usize {
    value.clamp(*range.start(), *range.end()) as usize
}
