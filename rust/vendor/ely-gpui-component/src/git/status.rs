use crate::data_display::Tone;

/// A file's state in git.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GitStatus {
    Modified,
    Added,
    Deleted,
    Untracked,
    Renamed,
    Conflicted,
}

impl GitStatus {
    pub(crate) fn word(self) -> &'static str {
        match self {
            Self::Modified => "Modified",
            Self::Added => "Added",
            Self::Deleted => "Deleted",
            Self::Untracked => "Untracked",
            Self::Renamed => "Renamed",
            Self::Conflicted => "Conflicted",
        }
    }

    pub(crate) fn letter(self) -> &'static str {
        match self {
            Self::Modified => "M",
            Self::Added => "A",
            Self::Deleted => "D",
            Self::Untracked => "U",
            Self::Renamed => "R",
            Self::Conflicted => "!",
        }
    }

    pub(crate) fn tone(self) -> Tone {
        match self {
            Self::Modified => Tone::Warning,
            Self::Added | Self::Untracked => Tone::Success,
            Self::Deleted | Self::Conflicted => Tone::Danger,
            Self::Renamed => Tone::Info,
        }
    }
}
