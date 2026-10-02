//! How a change request's facts look, shared by the right panel's list and
//! the detail tab so that one state always wears one glyph and one colour.

use gpui::Hsla;
use sirio_forge::{ChangeState, CheckStatus, CiState, Forge, ReviewState};
use sirio_theme::Theme;

use crate::sidebar::icons::Icon;

/// Neither Sirio's theme nor bezel's has a purple token, so *merged* takes
/// the syntax palette's keyword colour — purple in both appearances — rather
/// than adding a Sirio-owned token (spec §7.2).
pub(crate) fn state_color(state: ChangeState, theme: &Theme) -> Hsla {
    match state {
        ChangeState::Open => theme.ely.success,
        ChangeState::Draft => theme.ely.fg_subtle,
        ChangeState::Closed => theme.ely.danger,
        ChangeState::Merged => theme.syntax_palette().keyword,
    }
}

pub(crate) fn state_label(state: ChangeState) -> &'static str {
    match state {
        ChangeState::Draft => "Draft",
        ChangeState::Open => "Open",
        ChangeState::Merged => "Merged",
        ChangeState::Closed => "Closed",
    }
}

/// `None` draws nothing: a change request with no checks.
pub(crate) fn ci_mark(ci: CiState, theme: &Theme) -> Option<(Icon, Hsla)> {
    match ci {
        CiState::NoChecks => None,
        CiState::Running(_) => Some((Icon::Clock, theme.ely.warning)),
        CiState::Passed => Some((Icon::Check, theme.ely.success)),
        CiState::Failed => Some((Icon::XCircle, theme.ely.danger)),
        CiState::Canceled => Some((Icon::Dash, theme.ely.fg_subtle)),
    }
}

pub(crate) fn ci_text(ci: CiState) -> String {
    match ci {
        CiState::NoChecks => "No checks".to_string(),
        CiState::Running(Some(progress)) => format!("CI {}/{}", progress.done, progress.total),
        CiState::Running(None) => "CI running".to_string(),
        CiState::Passed => "CI passed".to_string(),
        CiState::Failed => "CI failed".to_string(),
        CiState::Canceled => "CI canceled".to_string(),
    }
}

/// A review waiting on the signed-in user outranks the change request's
/// own review state: it is the one thing on the row asking for an action.
pub(crate) fn review_mark(
    review: ReviewState,
    requested_from_me: bool,
    theme: &Theme,
) -> Option<(Icon, Hsla)> {
    if requested_from_me {
        return Some((Icon::Eye, theme.sirio.quantity));
    }
    match review {
        ReviewState::Approved { .. } => Some((Icon::UserCheck, theme.ely.success)),
        ReviewState::ChangesRequested => Some((Icon::Warning, theme.ely.danger)),
        ReviewState::ReviewRequired => Some((Icon::Eye, theme.ely.fg_subtle)),
        ReviewState::None => None,
    }
}

pub(crate) fn check_mark(status: CheckStatus, theme: &Theme) -> (Icon, Hsla) {
    match status {
        CheckStatus::Passed => (Icon::Check, theme.ely.success),
        CheckStatus::Failed => (Icon::XCircle, theme.ely.danger),
        CheckStatus::Running => (Icon::Clock, theme.ely.warning),
        CheckStatus::Queued => (Icon::Circle, theme.ely.fg_subtle),
        CheckStatus::Canceled | CheckStatus::Skipped | CheckStatus::Neutral => {
            (Icon::Dash, theme.ely.fg_subtle)
        }
    }
}

pub(crate) fn forge_mark(forge: Forge) -> Icon {
    match forge {
        Forge::GitHub => Icon::GitHubMark,
        Forge::GitLab => Icon::GitLabMark,
    }
}

pub(crate) fn cli_name(forge: Forge) -> &'static str {
    match forge {
        Forge::GitHub => "gh",
        Forge::GitLab => "glab",
    }
}

/// What a token needs, to read and to act: the two are different scopes on
/// both forges, and a token saved for reading alone is refused on its first
/// write.
pub(crate) fn token_scopes(forge: Forge) -> &'static str {
    match forge {
        Forge::GitHub => {
            "read access to pull requests, checks and metadata to read, and write access to pull requests to comment, review and edit"
        }
        Forge::GitLab => "the read_api scope to read, and the api scope to comment, review and edit",
    }
}

/// What a token needs to act, in the words the forge uses.
pub(crate) fn write_scopes(forge: Forge) -> &'static str {
    match forge {
        Forge::GitHub => {
            "write access to pull requests (`repo` on a classic token, Pull requests: Read and write on a fine-grained one)"
        }
        Forge::GitLab => "the api scope",
    }
}

pub(crate) fn now() -> i64 {
    chrono::Utc::now().timestamp()
}

/// `now`, `5m`, `3h`, `2d`, `4w`, then the date.
pub(crate) fn age(now: i64, then: Option<i64>) -> String {
    let Some(then) = then else {
        return String::new();
    };
    let seconds = (now - then).max(0);
    match seconds {
        0..60 => "now".to_string(),
        60..3_600 => format!("{}m", seconds / 60),
        3_600..86_400 => format!("{}h", seconds / 3_600),
        86_400..2_592_000 if seconds < 604_800 => format!("{}d", seconds / 86_400),
        86_400..2_592_000 => format!("{}w", seconds / 604_800),
        _ => chrono::DateTime::from_timestamp(then, 0)
            .map(|time| time.format("%Y-%m-%d").to_string())
            .unwrap_or_default(),
    }
}

/// `45s`, `4m`, `1h 5m`.
pub(crate) fn duration_text(secs: u64) -> String {
    match secs {
        0..60 => format!("{secs}s"),
        60..3_600 => format!("{}m", secs / 60),
        _ => format!("{}h {}m", secs / 3_600, (secs % 3_600) / 60),
    }
}
