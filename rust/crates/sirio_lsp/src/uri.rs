//! Paths and `file:` URIs, in both directions.
//!
//! LSP addresses every document by URI, so a client that cannot build one
//! cannot ask a question, and one that cannot read a path back out cannot
//! tell which file an answer is about.
//!
//! Percent-encoding is not decoration. `format!("file://{}", path.display())`
//! looks right until a path contains a space or a `#`, and then the server
//! receives an invalid URI and answers nothing — a failure with no symptom,
//! which is the kind this crate keeps meeting.

use std::path::{Path, PathBuf};

use lsp_types::Uri;

use crate::LspError;

/// Bytes that survive unencoded inside a path. RFC 3986's unreserved set,
/// plus `/` because it is the separator itself and `:` because a Windows
/// drive letter needs it (a colon is a legal `pchar`).
fn is_safe(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~' | b'/' | b':')
}

/// The `file:` URI naming `path`.
///
/// Refuses a relative path: it has no unambiguous URI, and inventing one
/// sends the server after a file that is not there.
pub fn uri_for_path(path: &Path) -> Result<Uri, LspError> {
    if !path.is_absolute() {
        return Err(LspError::Launch(format!(
            "not an absolute path: {}",
            path.display()
        )));
    }

    // Windows paths are `C:\x`; the URI form is `/C:/x`. Normalising here
    // keeps one code path for both platforms.
    let text = path.to_string_lossy().replace('\\', "/");
    let text = if text.starts_with('/') {
        text
    } else {
        format!("/{text}")
    };

    let mut encoded = String::from("file://");
    for &byte in text.as_bytes() {
        if is_safe(byte) {
            encoded.push(byte as char);
        } else {
            encoded.push('%');
            encoded.push_str(&format!("{byte:02X}"));
        }
    }

    encoded
        .parse()
        .map_err(|_| LspError::Launch(format!("path has no URI: {}", path.display())))
}

/// The path a `file:` URI names, or `None` for any other scheme.
///
/// `None` is not an error: a server may legitimately point at something
/// that is not a local file, and the caller's answer is to ignore it.
pub fn path_for_uri(uri: &Uri) -> Option<PathBuf> {
    let rest = uri.as_str().strip_prefix("file://")?;
    // `file://host/path` names a file on another machine. We have nothing
    // to open, so say so rather than guess.
    if !rest.starts_with('/') {
        return None;
    }
    let decoded = percent_decode(rest)?;
    // `/C:/x` came from `C:\x`; give it back in the shape it arrived in.
    let decoded = match decoded.strip_prefix('/') {
        Some(without_slash) if looks_like_windows_drive(without_slash) => {
            without_slash.replace('/', "\\")
        }
        _ => decoded,
    };
    Some(PathBuf::from(decoded))
}

fn looks_like_windows_drive(text: &str) -> bool {
    let mut characters = text.chars();
    matches!(
        (characters.next(), characters.next()),
        (Some(letter), Some(':')) if letter.is_ascii_alphabetic()
    )
}

fn percent_decode(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let hex = text.get(index + 1..index + 3)?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            index += 3;
        } else {
            out.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(out).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};

    fn round_trip(path: &str) {
        let uri = uri_for_path(Path::new(path)).expect("a path has a URI");
        let back = path_for_uri(&uri).expect("a file: URI has a path");
        assert_eq!(back, PathBuf::from(path), "round trip of {path}");
    }

    #[test]
    fn a_plain_path_round_trips() {
        round_trip("/home/user/project/src/main.rs");
    }

    #[test]
    fn a_path_with_a_space_round_trips() {
        round_trip("/home/user/my project/main.rs");
    }

    #[test]
    fn a_path_with_uri_punctuation_round_trips() {
        // `#` would otherwise start a fragment and `?` a query, silently
        // truncating the path the server is told about.
        round_trip("/tmp/a#b/c?d/e+f.rs");
    }

    #[test]
    fn a_path_with_non_ascii_round_trips() {
        round_trip("/tmp/caffè/π/λ.rs");
    }

    #[test]
    fn a_space_is_encoded_as_percent_twenty() {
        let uri = uri_for_path(Path::new("/tmp/a b")).expect("encodes");
        assert_eq!(uri.as_str(), "file:///tmp/a%20b");
    }

    #[test]
    fn a_relative_path_is_refused() {
        // Guessing an absolute path here would point the server at a file
        // that does not exist, and it would report nothing wrong.
        assert!(uri_for_path(Path::new("src/main.rs")).is_err());
    }

    #[test]
    fn a_non_file_uri_has_no_path() {
        let uri: lsp_types::Uri = "https://example.com/x".parse().expect("parses");
        assert_eq!(path_for_uri(&uri), None);
    }
}
