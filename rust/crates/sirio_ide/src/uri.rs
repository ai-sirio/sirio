//! `file://` URIs, the way Claude Code spells a path: the prefix and the raw native path.
//!
//! Claude builds its URIs as `file://` + path and compares them after stripping that prefix,
//! with no percent-encoding or decoding on either side. Encoding here would make a path with a
//! space, `#`, `%` or a non-ASCII character fail that comparison (it logs `path_mismatch` and
//! keeps no baseline), so neither direction encodes.

use std::path::{Path, PathBuf};

const SCHEME: &str = "file://";

/// `file://` followed by the path exactly as the OS spells it.
pub fn to_file_uri(path: &Path) -> String {
    format!("{SCHEME}{}", path.display())
}

/// The path after the `file://` prefix, or `None` without it. On Windows the `/` in front of a
/// drive letter goes too (`file:///C:/x` is `C:/x`).
pub fn from_file_uri(uri: &str) -> Option<PathBuf> {
    let rest = uri.strip_prefix(SCHEME)?;
    #[cfg(windows)]
    let rest = {
        let b = rest.as_bytes();
        if b.len() >= 3 && b[0] == b'/' && b[1].is_ascii_alphabetic() && b[2] == b':' {
            &rest[1..]
        } else {
            rest
        }
    };
    Some(PathBuf::from(rest))
}

#[cfg(test)]
mod tests {
    use super::*;

    const AWKWARD: [&str; 5] = [
        "/work/my project/a b.rs",
        "/work/c#/x?.rs",
        "/work/%41/100%.rs",
        "/work/caffè/日本.rs",
        "/work/{a}/[b].rs",
    ];

    #[test]
    fn space_is_not_encoded() {
        assert_eq!(to_file_uri(Path::new("/w/a b.rs")), "file:///w/a b.rs");
    }

    #[test]
    fn hash_and_question_mark_are_kept() {
        assert_eq!(to_file_uri(Path::new("/w/c#/x?.rs")), "file:///w/c#/x?.rs");
        assert_eq!(from_file_uri("file:///w/c#/x?.rs"), Some(PathBuf::from("/w/c#/x?.rs")));
    }

    #[test]
    fn percent_sequence_is_not_decoded() {
        assert_eq!(from_file_uri("file:///w/%41.rs"), Some(PathBuf::from("/w/%41.rs")));
        assert_eq!(to_file_uri(Path::new("/w/%41.rs")), "file:///w/%41.rs");
    }

    #[test]
    fn non_ascii_is_kept() {
        assert_eq!(to_file_uri(Path::new("/w/日本.rs")), "file:///w/日本.rs");
    }

    #[test]
    fn round_trip_is_identity() {
        for p in AWKWARD {
            assert_eq!(from_file_uri(&to_file_uri(Path::new(p))), Some(PathBuf::from(p)));
        }
    }

    #[test]
    fn non_file_scheme_is_none() {
        assert_eq!(from_file_uri("https://example.com/x.rs"), None);
        assert_eq!(from_file_uri("/w/x.rs"), None);
    }

    #[cfg(windows)]
    #[test]
    fn drive_letter_loses_the_leading_slash() {
        assert_eq!(from_file_uri("file:///C:/x/y.rs"), Some(PathBuf::from("C:/x/y.rs")));
    }
}
