//! `file://` URIs, the way Claude Code spells a path.

use std::path::{Path, PathBuf};

use url::Url;

/// The `file://` URI of an absolute path, percent-encoded. A path that is not absolute has no
/// URI; the bare `file://` + path spelling stands in so the call never fails.
pub fn to_file_uri(path: &Path) -> String {
    Url::from_file_path(path)
        .map(String::from)
        .unwrap_or_else(|()| format!("file://{}", path.display()))
}

/// The path of a `file://` URI naming this machine, or `None` for any other scheme or host.
pub fn from_file_uri(uri: &str) -> Option<PathBuf> {
    let url = Url::parse(uri).ok()?;
    if url.scheme() != "file" || !matches!(url.host_str(), None | Some("") | Some("localhost")) {
        return None;
    }
    url.to_file_path().ok()
}
