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
use sirio_persistence::{AppSettings, EditorCursorShape, EditorLineNumbers, settings_ranges};

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

    /// Publishes `prefs` app-wide. Windows are refreshed only when the value
    /// changed, so republishing the same preferences repaints nothing.
    pub fn apply(prefs: Self, cx: &mut App) {
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
}

fn in_range(value: i64, range: RangeInclusive<i64>) -> usize {
    value.clamp(*range.start(), *range.end()) as usize
}
