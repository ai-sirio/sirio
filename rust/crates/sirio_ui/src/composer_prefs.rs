//! The composer's preferences, published app-wide.
//!
//! How many rows the chat composer's text field may grow to is one setting for
//! the whole app. The host installs it here as a GPUI global; each chat
//! observes the global, so a change reaches every open composer.

use gpui::App;
use sirio_persistence::{AppSettings, settings_ranges};

/// The composer's preferences, as the stored settings hold them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ComposerPrefs {
    /// The most rows the field may grow to, within `settings_ranges::CHAT_COMPOSER_MAX_ROWS`.
    pub max_rows: usize,
}

impl gpui::Global for ComposerPrefs {}

impl Default for ComposerPrefs {
    /// The stored default, so the two cannot disagree.
    fn default() -> Self {
        Self::from_settings(&AppSettings::default())
    }
}

impl ComposerPrefs {
    /// The preferences held in `settings`. The stored number is clamped into
    /// its range here as well, because `AppSettings` can be built by hand.
    pub fn from_settings(settings: &AppSettings) -> Self {
        let range = settings_ranges::CHAT_COMPOSER_MAX_ROWS;
        Self {
            max_rows: settings
                .composer_max_rows
                .clamp(*range.start(), *range.end()) as usize,
        }
    }

    /// The preferences in force, or the defaults when none were published.
    pub fn current(cx: &App) -> Self {
        cx.try_global::<Self>().cloned().unwrap_or_default()
    }

    /// Publishes `prefs` app-wide. The global is replaced only when its value
    /// changed or none exists yet, because `set_global` notifies every global
    /// observer. Windows are not refreshed: each chat observes the global.
    pub fn apply(prefs: Self, cx: &mut App) {
        if cx.try_global::<Self>() == Some(&prefs) {
            return;
        }
        cx.set_global(prefs);
    }
}
