//! The Claude Code version this protocol was verified against.

/// The oldest Claude Code Sirio will drive natively.
///
/// This is the version the official ACP wrapper's Agent SDK bundles, and
/// therefore the one the control protocol used here is certified against.
/// Below it, resolution falls back to that wrapper rather than guessing
/// which verbs an older CLI answers.
pub const MIN_CLAUDE_VERSION: &str = "2.1.257";

/// A parsed `major.minor.patch`.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct ClaudeVersion {
    major: u32,
    minor: u32,
    patch: u32,
}

impl ClaudeVersion {
    /// Builds a version from its components.
    #[must_use]
    pub const fn new(major: u32, minor: u32, patch: u32) -> Self {
        Self {
            major,
            minor,
            patch,
        }
    }

    /// Reads `claude --version` output, e.g. `2.1.273 (Claude Code)`.
    /// `None` when there is no complete triple to read — an unreadable
    /// version is never assumed to be new enough.
    #[must_use]
    pub fn parse(output: &str) -> Option<Self> {
        let token = output.split_whitespace().next()?;
        let mut parts = token.split('.');
        let major = parts.next()?.parse().ok()?;
        let minor = parts.next()?.parse().ok()?;
        // A trailing suffix on the patch (`257-rc1`) still names 257.
        let patch_token = parts.next()?;
        let digits: String = patch_token
            .chars()
            .take_while(char::is_ascii_digit)
            .collect();
        let patch = digits.parse().ok()?;
        Some(Self::new(major, minor, patch))
    }

    /// Whether this version is at or above [`MIN_CLAUDE_VERSION`].
    #[must_use]
    pub fn meets_floor(self) -> bool {
        Self::parse(MIN_CLAUDE_VERSION).is_some_and(|floor| self >= floor)
    }
}

impl std::fmt::Display for ClaudeVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_version_is_read_from_the_cli_banner() {
        // `claude --version` prints exactly this.
        let version = ClaudeVersion::parse("2.1.273 (Claude Code)").expect("a version");
        assert_eq!(version, ClaudeVersion::new(2, 1, 273));
        assert_eq!(version.to_string(), "2.1.273");
        // A bare triple works too, for a CLI that drops the banner.
        assert_eq!(
            ClaudeVersion::parse("2.1.273\n"),
            Some(ClaudeVersion::new(2, 1, 273))
        );
    }

    #[test]
    fn unreadable_output_is_none_rather_than_a_guess() {
        assert_eq!(ClaudeVersion::parse(""), None);
        assert_eq!(ClaudeVersion::parse("command not found"), None);
        assert_eq!(ClaudeVersion::parse("2.1"), None);
    }

    #[test]
    fn the_floor_is_inclusive_and_compares_by_component() {
        let floor = ClaudeVersion::parse(MIN_CLAUDE_VERSION).expect("the floor parses");
        assert_eq!(floor, ClaudeVersion::new(2, 1, 257));
        assert!(ClaudeVersion::new(2, 1, 257).meets_floor());
        assert!(ClaudeVersion::new(2, 1, 273).meets_floor());
        assert!(ClaudeVersion::new(3, 0, 0).meets_floor());
        assert!(!ClaudeVersion::new(2, 1, 256).meets_floor());
        assert!(!ClaudeVersion::new(2, 0, 999).meets_floor());
        // A patch number that sorts wrong as text must not sort wrong here.
        assert!(ClaudeVersion::new(2, 1, 1000).meets_floor());
    }
}
