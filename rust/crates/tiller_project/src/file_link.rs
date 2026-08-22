use std::path::{Path, PathBuf};

/// A file link target, optionally carrying a one-based line and column.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileLinkTarget {
    pub path: PathBuf,
    pub line: Option<usize>,
    pub column: Option<usize>,
}

/// Resolves a markdown/document link against a worktree.
///
/// Link-resolution contract: a `file://` URL carries a **URI-shaped** path
/// — absolute by the URL grammar (RFC 8089), forward-slash separated, and
/// POSIX in form on every platform, because docs are written once and read
/// everywhere (`file:///tmp/a b.md` from a doc authored on a mac must
/// resolve the same on a Windows host). It is therefore taken as its own
/// resolution without consulting the host platform's `Path::is_absolute`:
/// on Windows an absolute *filesystem* path needs a drive letter or UNC
/// prefix, and asking the platform here would reject a perfectly formed
/// file URL that merely looks POSIX. A bare path, by contrast, is
/// **platform-shaped**: it answers the host's own `is_absolute()` and is
/// otherwise relative to the worktree.
pub fn resolve_file_link(raw: &str, worktree: &Path) -> Option<FileLinkTarget> {
    let raw = raw.trim();
    let from_url = raw.starts_with("file://");
    let path_text = if let Some(rest) = raw.strip_prefix("file://") {
        let rest = rest.strip_prefix("localhost/").unwrap_or(rest);
        let decoded = percent_decode(rest)?;
        // URI-shaped means absolute: a decoded path that does not start
        // with `/` is a malformed file URL (`file://../x`, `file://x`)
        // and is rejected here rather than silently joined to the
        // worktree, which would have been a traversal hole in the
        // relative branch's guard.
        if !decoded.starts_with('/') {
            return None;
        }
        uri_text_to_native_absolute(decoded)
    } else {
        if raw.contains("://") || raw.starts_with("mailto:") {
            return None;
        }
        raw.to_string()
    };
    let (path_text, line, column) = split_location(&path_text);
    let path = PathBuf::from(path_text);
    // Both absoluteness votes — the URL grammar's and the platform's —
    // land here in the same arm: a file URL is absolute by contract
    // (see [`resolve_file_link`]'s doc comment) and a bare path answers
    // the host's `is_absolute`.
    let path = if from_url || path.is_absolute() {
        path
    } else {
        validate_relative(&path)?;
        worktree.join(path)
    };
    Some(FileLinkTarget { path, line, column })
}

/// Unix: a URI-shaped path is already the native absolute form — identity.
#[cfg(not(target_os = "windows"))]
fn uri_text_to_native_absolute(path: String) -> String {
    path
}

/// Windows: ungarbles the RFC 8089 drive-letter form of a Windows path
/// inside a file URL — `/C:/dir/file` — back to a native absolute path
/// (`C:/dir/file`). POSIX-shaped paths with no drive letter (`/tmp/a b.md`)
/// stay as-is: rooted at the current drive's root, which is the only
/// honest place a Windows host can point a POSIX absolute path that names
/// no drive.
#[cfg(target_os = "windows")]
fn uri_text_to_native_absolute(path: String) -> String {
    let Some(rest) = path.strip_prefix('/') else {
        return path;
    };
    let Some((drive, rest)) = rest.split_once('/') else {
        return path;
    };
    let Some(letter) = drive.strip_suffix(':') else {
        return path;
    };
    if letter.len() == 1 && letter.as_bytes()[0].is_ascii_alphabetic() {
        format!("{letter}:/{rest}")
    } else {
        path
    }
}

/// Whether a path uses one of the markdown extensions understood by the
/// editor surface.
pub fn is_markdown_path(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            matches!(
                extension.to_ascii_lowercase().as_str(),
                "md" | "markdown" | "mdown" | "mkdn"
            )
        })
}

fn split_location(raw: &str) -> (String, Option<usize>, Option<usize>) {
    let mut pieces = raw.rsplitn(3, ':');
    let last = pieces.next();
    let before_last = pieces.next();
    let before_before_last = pieces.next();
    match (last, before_last, before_before_last) {
        (Some(column), Some(line), Some(path))
            if is_positive_number(line) && is_positive_number(column) =>
        {
            (path.to_string(), line.parse().ok(), column.parse().ok())
        }
        (Some(line), Some(path), _) if is_positive_number(line) => {
            (path.to_string(), line.parse().ok(), None)
        }
        _ => (raw.to_string(), None, None),
    }
}

fn is_positive_number(value: &str) -> bool {
    !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit()) && value != "0"
}

fn validate_relative(path: &Path) -> Option<()> {
    if path.components().any(|component| {
        matches!(
            component,
            std::path::Component::ParentDir
                | std::path::Component::RootDir
                | std::path::Component::Prefix(_)
        )
    }) {
        None
    } else {
        Some(())
    }
}

fn percent_decode(value: &str) -> Option<String> {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let high = bytes.get(index + 1).and_then(|byte| hex(*byte))?;
            let low = bytes.get(index + 2).and_then(|byte| hex(*byte))?;
            decoded.push((high << 4) | low);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(decoded).ok()
}

fn hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_relative_absolute_and_file_urls_with_locations() {
        let root = Path::new("/worktree");
        assert_eq!(
            resolve_file_link("docs/read me.md:12:4", root),
            Some(FileLinkTarget {
                path: PathBuf::from("/worktree/docs/read me.md"),
                line: Some(12),
                column: Some(4),
            })
        );
        // A file URL's path is URI-shaped, not platform-shaped: `/tmp/a b.md`
        // is absolute by the URL grammar and must resolve the same on
        // Windows, where a bare-path `/tmp/...` would need a drive letter.
        assert_eq!(
            resolve_file_link("file:///tmp/a%20b.md:7", root),
            Some(FileLinkTarget {
                path: PathBuf::from("/tmp/a b.md"),
                line: Some(7),
                column: None,
            })
        );
        // The RFC 8089 drive-letter form of a Windows path resolves to a
        // native absolute path on Windows and stays POSIX-shaped elsewhere.
        let windows_drive_form = resolve_file_link("file:///C:/work/a%20b.md", root).unwrap();
        #[cfg(target_os = "windows")]
        let windows_drive_form_expected = PathBuf::from("C:/work/a b.md");
        #[cfg(not(target_os = "windows"))]
        let windows_drive_form_expected = PathBuf::from("/C:/work/a b.md");
        assert_eq!(windows_drive_form.path, windows_drive_form_expected);
        assert_eq!(windows_drive_form.line, None);
    }

    #[test]
    fn rejects_other_schemes_and_traversal() {
        let root = Path::new("/worktree");
        assert_eq!(resolve_file_link("https://example.com/a.md", root), None);
        assert_eq!(resolve_file_link("../outside.md", root), None);
        // A file URL whose decoded path is not absolute is malformed — it
        // must not be silently joined into the worktree (that would dodge
        // the relative branch's traversal guard).
        assert_eq!(resolve_file_link("file://../outside.md", root), None);
        assert!(is_markdown_path(Path::new("README.MD")));
        assert!(!is_markdown_path(Path::new("image.png")));
    }
}
