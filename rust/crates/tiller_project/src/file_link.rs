use std::path::{Path, PathBuf};

/// A file link target, optionally carrying a one-based line and column.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileLinkTarget {
    pub path: PathBuf,
    pub line: Option<usize>,
    pub column: Option<usize>,
}

/// Resolves a markdown/document link against a worktree.
pub fn resolve_file_link(raw: &str, worktree: &Path) -> Option<FileLinkTarget> {
    let raw = raw.trim();
    let path_text = if let Some(rest) = raw.strip_prefix("file://") {
        let rest = rest.strip_prefix("localhost/").unwrap_or(rest);
        percent_decode(rest)?
    } else {
        if raw.contains("://") || raw.starts_with("mailto:") {
            return None;
        }
        raw.to_string()
    };
    let (path_text, line, column) = split_location(&path_text);
    let path = PathBuf::from(path_text);
    let path = if path.is_absolute() {
        path
    } else {
        validate_relative(&path)?;
        worktree.join(path)
    };
    Some(FileLinkTarget { path, line, column })
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
        assert_eq!(
            resolve_file_link("file:///tmp/a%20b.md:7", root),
            Some(FileLinkTarget {
                path: PathBuf::from("/tmp/a b.md"),
                line: Some(7),
                column: None,
            })
        );
    }

    #[test]
    fn rejects_other_schemes_and_traversal() {
        let root = Path::new("/worktree");
        assert_eq!(resolve_file_link("https://example.com/a.md", root), None);
        assert_eq!(resolve_file_link("../outside.md", root), None);
        assert!(is_markdown_path(Path::new("README.MD")));
        assert!(!is_markdown_path(Path::new("image.png")));
    }
}
