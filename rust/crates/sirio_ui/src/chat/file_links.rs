//! Finds the file paths a chat response names and marks them as links.
//!
//! Works on the response's tree (`Inline` text), after parsing. A path becomes a
//! link only when it names an existing regular file (see
//! `docs/superpowers/specs/2026-10-24-chat-file-links-design.md`), so every
//! candidate costs one disk lookup, which [`FileLinkMemo`] answers once per
//! entry instead of once per streamed chunk.

use std::collections::HashMap;
use std::ops::Range;
use std::path::{Path, PathBuf};

use ely_gpui_component::documents::{Inline, Style};
use sirio_project::{FileLinkTarget, resolve_file_link};

use crate::markdown::FileLinkInputs;

/// Candidate string -> what it resolved to, for one chat entry. Not keyed by
/// working directory or home: an entry's memo lives and dies with its entry.
#[derive(Default)]
pub(crate) struct FileLinkMemo {
    links: HashMap<String, Option<FileLinkTarget>>,
    /// Image url -> the address the renderer is handed for it, or `None` when it is shown
    /// as its alt text: the resolution and the size check, asked once.
    images: HashMap<String, Option<String>>,
    /// Only what the memo already knows is answered, and nothing is asked of the disk or added
    /// (see [`Self::lookup_only`]).
    lookup_only: bool,
}

impl FileLinkMemo {
    /// Sets whether the memo only looks up. A tail drawn on a frame must not stat a path (the
    /// last word of the text revealed so far is a new candidate on nearly every frame, and the
    /// disk is asked while a document is built, never while it paints): what the build of the
    /// received text found is linked, and anything else is plain text.
    pub(crate) fn lookup_only(&mut self, on: bool) {
        self.lookup_only = on;
    }

    pub(crate) fn clear(&mut self) {
        self.links.clear();
        self.images.clear();
    }

    /// How many candidates and pictures the memo has an answer for.
    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.links.len() + self.images.len()
    }

    /// The memoised resolution of one candidate, asking the disk only the
    /// first time the candidate is seen.
    fn resolve(
        &mut self,
        candidate: &str,
        cwd: &Path,
        home: Option<&Path>,
    ) -> Option<FileLinkTarget> {
        if let Some(known) = self.links.get(candidate) {
            return known.clone();
        }
        if self.lookup_only {
            return None;
        }
        let found = resolve_chat_path(candidate, cwd, home);
        self.links.insert(candidate.to_string(), found.clone());
        found
    }
}

/// A place a chat path may name, found on the text alone.
pub(crate) struct ChatPathCandidate {
    pub(crate) target: FileLinkTarget,
    /// What the text itself contributes to `target.path`: all of an absolute path or a file
    /// URL's, and only the part under the project of a relative one or under the home of `~/`.
    /// The project's and the home's own names are the person's, not the response's.
    pub(crate) contributed: PathBuf,
}

/// The places a response's path may name, in the order they are tried, without asking the disk.
///
/// `#L42` / `#L42-L50` and `:42-50` become a plain line (`:42`), `~/` expands
/// against `home` (a home that is not UTF-8 names nothing: its lossy copy would be another
/// folder), and an absolute path is followed by the same path inside `cwd`, because an agent's
/// `/docs/x.md` usually means the project's. Text that names a share or a device has no place.
pub(crate) fn chat_path_candidates(
    raw: &str,
    cwd: &Path,
    home: Option<&Path>,
) -> Vec<ChatPathCandidate> {
    let normalized = lowercase_file_scheme(normalize_location(raw.trim()));
    let (text, under_home) = match normalized.strip_prefix("~/") {
        Some(rest) => {
            let Some(home) = home.filter(|home| home.to_str().is_some()) else {
                return Vec::new();
            };
            (home.join(rest).to_string_lossy().into_owned(), Some(home))
        }
        None => (normalized, None),
    };
    // Not even the fallback below: `//host/x` is a share, not "x" in the project.
    if is_network_or_device_path(Path::new(&text)) {
        return Vec::new();
    }
    let mut candidates = Vec::new();
    // `trusted`: the folders the person owns, cut off in turn from the path `text` resolves to.
    let mut place = |text: &str, trusted: &[&Path]| {
        let Some(target) = resolve_file_link(text, cwd) else {
            return;
        };
        let mut contributed = target.path.as_path();
        for folder in trusted {
            if let Ok(rest) = contributed.strip_prefix(folder) {
                contributed = rest;
            }
        }
        let contributed = contributed.to_path_buf();
        candidates.push(ChatPathCandidate { target, contributed });
    };
    let is_file_url = text.starts_with("file://");
    let rooted = |text: &str| is_file_url || Path::new(text).is_absolute();
    match under_home {
        Some(home) => place(&text, &[home]),
        None if rooted(&text) => place(&text, &[]),
        None => place(&text, &[cwd]),
    }
    if !is_file_url && text.starts_with(['/', '\\']) {
        let inside = text.trim_start_matches(['/', '\\']);
        // The expanded home is no part of what `~/` contributes: it follows `cwd` here.
        let home_inside = under_home
            .and_then(Path::to_str)
            .map(|home| Path::new(home.trim_start_matches(['/', '\\'])));
        let mut trusted = vec![cwd];
        trusted.extend(home_inside);
        match rooted(inside) {
            true => place(inside, &[]),
            false => place(inside, &trusted),
        }
    }
    candidates
}

/// Resolves what a response wrote as a path to an existing regular file: the first of its
/// [`chat_path_candidates`] that is one.
pub(crate) fn resolve_chat_path(
    raw: &str,
    cwd: &Path,
    home: Option<&Path>,
) -> Option<FileLinkTarget> {
    chat_path_candidates(raw, cwd, home)
        .into_iter()
        .map(|candidate| candidate.target)
        .find(|target| !is_network_or_device_path(&target.path) && target.path.is_file())
}

/// Whether `path` names a network share or a device rather than a place on
/// this machine: `//host/..` and `\\host\..` (also `\\?\` and `\\.\`), and on
/// Windows a UNC, device or verbatim prefix. Text an agent wrote can be
/// prompt-injected, and asking the disk about such a path opens a connection
/// (NTLM on Windows) or hangs on a dead mount, so it is refused before any
/// lookup, and again before a click opens it.
pub(crate) fn is_network_or_device_path(path: &Path) -> bool {
    let text = path.to_string_lossy();
    // Two separators in any mix: `//host`, `\\host`, `\\?\`, `\\.\`, and the odd `\/`.
    let mut chars = text.chars();
    if matches!(
        (chars.next(), chars.next()),
        (Some('/' | '\\'), Some('/' | '\\'))
    ) {
        return true;
    }
    // On Windows a UNC, device or verbatim prefix; no other platform reads a prefix.
    if let Some(std::path::Component::Prefix(prefix)) = path.components().next() {
        use std::path::Prefix;
        return matches!(
            prefix.kind(),
            Prefix::UNC(..) | Prefix::VerbatimUNC(..) | Prefix::DeviceNS(_) | Prefix::Verbatim(_)
        );
    }
    false
}

/// `FILE://x` is `file://x`: the scheme is case-insensitive.
fn lowercase_file_scheme(text: String) -> String {
    match text.get(..7) {
        Some(head) if head.eq_ignore_ascii_case("file://") => format!("file://{}", &text[7..]),
        _ => text,
    }
}

/// Whether a click on a file link may open `path`, judged at the click (one
/// stat, never at paint). A regular file does. A file that was there when the
/// response was built and is gone now does too: the editor tab it opens says
/// the file is missing, which is better than a link that does nothing. A
/// share or device, a directory, a FIFO and any other error do not.
pub(crate) fn may_open_on_click(path: &Path) -> bool {
    if is_network_or_device_path(path) {
        return false;
    }
    match std::fs::metadata(path) {
        Ok(metadata) => metadata.is_file(),
        Err(error) => error.kind() == std::io::ErrorKind::NotFound,
    }
}

/// Rewrites a trailing `#L<n>`, `#L<n>-L<m>` or `:<n>-<m>` into `:<n>`;
/// anything else is returned as it came.
fn normalize_location(text: &str) -> String {
    if let Some((path, anchor)) = text.rsplit_once("#L")
        && let Some(line) = anchor_line(anchor)
    {
        return format!("{path}:{line}");
    }
    if let Some((head, range)) = text.rsplit_once(':')
        && let Some((first, last)) = range.split_once('-')
        && is_number(first)
        && is_number(last)
    {
        return format!("{head}:{first}");
    }
    text.to_string()
}

/// The first line of a GitHub anchor's `<n>` or `<n>-L<m>` (after `#L`).
fn anchor_line(anchor: &str) -> Option<&str> {
    let first = match anchor.split_once("-L") {
        Some((first, last)) if is_number(last) => first,
        Some(_) => return None,
        None => anchor,
    };
    is_number(first).then_some(first)
}

fn is_number(value: &str) -> bool {
    !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit())
}

/// The path of the regular file a non-web image URL names, as the address the renderer is
/// handed, if any. A URL that starts with whitespace or a control character is refused before
/// anything else: the resolver trims it, so it would draw another file than the one written.
/// A line suffix means nothing to a picture and is dropped.
pub(crate) fn local_image_path(
    url: &str,
    cwd: &Path,
    home: Option<&Path>,
    memo: &mut FileLinkMemo,
) -> Option<String> {
    if let Some(known) = memo.images.get(url) {
        return known.clone();
    }
    if memo.lookup_only {
        return None;
    }
    let kept = local_image_on_disk(url, cwd, home);
    memo.images.insert(url.to_string(), kept.clone());
    kept
}

fn local_image_on_disk(url: &str, cwd: &Path, home: Option<&Path>) -> Option<String> {
    let path = picture_lookups(url, cwd, home)?
        .into_iter()
        .find(|path| path.is_file())?;
    // Read whole when painted: an empty file is no picture (and procfs calls
    // its endless files regular and empty), and past the chat's image cap
    // the read would stall the turn.
    let len = std::fs::metadata(&path).ok()?.len();
    if len == 0 || len > super::MAX_IMAGE_BYTES {
        return None;
    }
    // `picture_lookups` asks only about absolute UTF-8 paths: the address names exactly this file.
    crate::markdown::picture_address(path.to_str()?)
}

/// The paths the disk may be asked about for the picture `url` names, in order, or none when the
/// text is refused. It is decided on the text, before any disk call: on Windows a reserved device
/// name (`NUL`, `COM1`, `CONIN$`) in a path opens the device, and a COM port with nothing behind it
/// blocks the UI thread, so a picture never asks the disk about one, on any platform. What is
/// judged is what the response contributes to each place it may name, by the Preview's rule
/// (`names_are_plain`). Of those places, only an absolute path that is UTF-8 is asked about.
fn picture_lookups(url: &str, cwd: &Path, home: Option<&Path>) -> Option<Vec<PathBuf>> {
    let first = url.chars().next()?;
    if first.is_whitespace() || first.is_control() {
        return None;
    }
    let candidates: Vec<_> = chat_path_candidates(url, cwd, home)
        .into_iter()
        .filter(|candidate| !is_network_or_device_path(&candidate.target.path))
        .collect();
    let plain = candidates.iter().all(|candidate| {
        candidate
            .contributed
            .to_str()
            .is_some_and(crate::markdown::names_are_plain)
    });
    plain.then(|| {
        candidates
            .into_iter()
            .map(|candidate| candidate.target.path)
            // Only a path the renderer can be handed is asked about: a string, as a lossy copy
            // names another file, and absolute, as the renderer has no folder to resolve against.
            .filter(|path| path.is_absolute() && path.to_str().is_some())
            .collect()
    })
}

/// The links an agent wrote are an allowlist: a web link stays as written, a path to a
/// file that was checked becomes a link to that file, anything else (a path that is not
/// there, a share, a drive path, `smb:`, `vscode:`, an anchor) would be handed to the
/// system opener by a click, so it goes. Without a lookup only the web survives.
fn allowed_target(
    target: &str,
    files: Option<(&Path, Option<&Path>, &mut FileLinkMemo)>,
) -> Option<String> {
    if is_web_target(target) {
        return Some(target.to_owned());
    }
    let (cwd, home, memo) = files?;
    let found = memo.resolve(target, cwd, home)?;
    Some(file_link_url(&found))
}

/// [`allowed_target`] for a tree's links.
pub(crate) fn allowed_link(
    target: &str,
    files: &mut Option<FileLinkInputs<'_>>,
) -> Option<String> {
    let files = files
        .as_mut()
        .map(|files| (files.worktree, files.home, &mut *files.memo));
    allowed_target(target, files)
}

/// The part of inline code `range` that may name a file: its trimmed text, unless it is
/// empty or an address with a scheme.
fn code_candidate(text: &str, range: &Range<usize>) -> Option<Range<usize>> {
    let slice = text.get(range.clone())?;
    let trimmed = slice.trim();
    if trimmed.is_empty() || trimmed.contains("://") {
        return None;
    }
    let start = range.start + (slice.len() - slice.trim_start().len());
    Some(start..start + trimmed.len())
}

/// Whether a token of prose may name a file: it holds a `/` or a `.`, and no scheme.
fn may_name_a_file_in_prose(candidate: &str) -> bool {
    (candidate.contains('/') || candidate.contains('.')) && !candidate.contains("://")
}

/// [`link_text`] on a tree's text: the links an agent wrote are an allowlist
/// ([`allowed_link`]), and the files it names in inline code and in prose become links.
/// The text is never touched, only its styles and links grow; a link that goes takes its
/// look with it.
pub(crate) fn link_inline(inline: &mut Inline, files: &mut Option<FileLinkInputs<'_>>) {
    let mut gone = Vec::new();
    let mut kept = Vec::with_capacity(inline.links.len());
    for (range, target) in std::mem::take(&mut inline.links) {
        match allowed_link(&target, files) {
            Some(url) => kept.push((range, url)),
            None => gone.push(range),
        }
    }
    inline.links = kept;
    inline
        .styles
        .retain(|(range, style)| *style != Style::Link || !gone.contains(range));
    let Some(files) = files.as_mut() else {
        return;
    };

    let mut added: Vec<(Range<usize>, String)> = Vec::new();
    // Inline code: the trimmed span is the candidate. Code already inside a link keeps it.
    for (range, style) in &inline.styles {
        let in_a_link = inline
            .links
            .iter()
            .any(|(link, _)| link.start <= range.start && range.end <= link.end);
        if *style != Style::Code || in_a_link {
            continue;
        }
        let Some(candidate) = code_candidate(&inline.text, range) else {
            continue;
        };
        let written = &inline.text[candidate.clone()];
        if let Some(found) = files.memo.resolve(written, files.worktree, files.home) {
            added.push((candidate, file_link_url(&found)));
        }
    }
    // Prose: whitespace-delimited tokens outside code, links, formulas and notes.
    let covered = (inline.styles.iter())
        .filter(|(_, style)| {
            matches!(
                style,
                Style::Code | Style::Link | Style::Math | Style::Footnote
            )
        })
        .map(|(range, _)| range.clone())
        .chain(inline.links.iter().map(|(range, _)| range.clone()))
        .collect();
    for segment in uncovered(inline.text.len(), covered) {
        for (token_start, token) in tokens(&inline.text, segment) {
            let Some(range) = trim_token(token, token_start) else {
                continue;
            };
            let candidate = &inline.text[range.clone()];
            if !may_name_a_file_in_prose(candidate) {
                continue;
            }
            if let Some(found) = files.memo.resolve(candidate, files.worktree, files.home) {
                added.push((range, file_link_url(&found)));
            }
        }
    }
    for (range, url) in added {
        inline.styles.push((range.clone(), Style::Link));
        inline.links.push((range, url));
    }
    inline.links.sort_by_key(|(range, _)| range.start);
}

/// The scheme of a link target: the text before the first `:` when it starts
/// with a letter and goes on with letters, digits, `+`, `-` or `.`. A single
/// letter is a Windows drive (`C:\x`), not a scheme.
fn url_scheme(target: &str) -> Option<&str> {
    let (scheme, _) = target.split_once(':')?;
    let mut chars = scheme.chars();
    let first = chars.next()?;
    (scheme.len() >= 2
        && first.is_ascii_alphabetic()
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.')))
    .then_some(scheme)
}

/// A link a click may hand to the system: http, https or mailto, in any case.
pub(crate) fn is_web_target(target: &str) -> bool {
    url_scheme(target).is_some_and(|scheme| {
        ["http", "https", "mailto"]
            .iter()
            .any(|web| scheme.eq_ignore_ascii_case(web))
    })
}

/// The byte ranges of a text `len` bytes long that `covered` leaves out.
fn uncovered(len: usize, mut covered: Vec<Range<usize>>) -> Vec<Range<usize>> {
    covered.sort_by_key(|range| range.start);
    let mut segments = Vec::new();
    let mut cursor = 0;
    for range in covered {
        if range.start > cursor {
            segments.push(cursor..range.start.min(len));
        }
        cursor = cursor.max(range.end);
    }
    if cursor < len {
        segments.push(cursor..len);
    }
    segments
}

/// The whitespace-delimited tokens of one segment, with their byte offsets.
fn tokens(text: &str, segment: std::ops::Range<usize>) -> Vec<(usize, &str)> {
    let Some(slice) = text.get(segment.clone()) else {
        return Vec::new();
    };
    let mut found = Vec::new();
    let mut start = None;
    for (offset, ch) in slice.char_indices() {
        match (ch.is_whitespace(), start) {
            (true, Some(from)) => {
                found.push((segment.start + from, &slice[from..offset]));
                start = None;
            }
            (false, None) => start = Some(offset),
            _ => {}
        }
    }
    if let Some(from) = start {
        found.push((segment.start + from, &slice[from..]));
    }
    found
}

/// The token without the quotes and brackets around it and the sentence
/// punctuation after it, as a byte range of the whole text.
fn trim_token(token: &str, start: usize) -> Option<std::ops::Range<usize>> {
    let leading = token.trim_start_matches(['"', '\'', '`', '(', '[', '{', '<', '«']);
    let trimmed =
        leading.trim_end_matches(['"', '\'', ')', ']', '}', '>', '»', '.', ',', ';', ':', '!', '?']);
    if trimmed.is_empty() {
        return None;
    }
    let from = start + (token.len() - leading.len());
    Some(from..from + trimmed.len())
}

/// A `file://` URL for a target: the absolute path with forward slashes, every
/// byte outside `A-Za-z0-9-._~/` percent-encoded, then `:line[:column]`. On
/// Windows a drive path gets the leading `/` of RFC 8089 (`/C%3A/dir/f`).
pub(crate) fn file_link_url(target: &FileLinkTarget) -> String {
    let path = target.path.to_string_lossy().replace('\\', "/");
    let mut url = String::from("file://");
    if !path.starts_with('/') {
        url.push('/');
    }
    for byte in path.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b'/' => {
                url.push(byte as char)
            }
            _ => url.push_str(&format!("%{byte:02X}")),
        }
    }
    if let Some(line) = target.line {
        url.push_str(&format!(":{line}"));
        if let Some(column) = target.column {
            url.push_str(&format!(":{column}"));
        }
    }
    url
}

/// The inverse of [`file_link_url`]: `Some` only for a `file://` URL.
pub(crate) fn file_link_target(url: &str) -> Option<FileLinkTarget> {
    let url = lowercase_file_scheme(url.to_string());
    if !url.starts_with("file://") {
        return None;
    }
    resolve_file_link(&url, Path::new("/"))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::*;

    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            static COUNTER: AtomicU64 = AtomicU64::new(0);
            let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "sirio-file-links-test-{}-{unique}",
                std::process::id()
            ));
            std::fs::create_dir_all(&path).expect("create temp dir");
            Self(path)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// A worktree inside a scratch root (so `outside.txt` sits in its parent),
    /// and a separate directory playing `home`.
    struct Fixture {
        _root: TempDir,
        _home: TempDir,
        wt: PathBuf,
        home: PathBuf,
    }

    fn fixture() -> Fixture {
        let root = TempDir::new();
        let home_dir = TempDir::new();
        let wt = root.0.join("wt");
        std::fs::create_dir_all(wt.join("docs/sub")).unwrap();
        std::fs::create_dir_all(wt.join("src")).unwrap();
        std::fs::create_dir_all(wt.join("dir with space")).unwrap();
        std::fs::write(wt.join("docs/NOME-FILE.md"), "# nome\n").unwrap();
        let lines: String = (1..=60).map(|n| format!("line {n}\n")).collect();
        std::fs::write(wt.join("src/main.rs"), lines).unwrap();
        std::fs::write(wt.join("README.md"), "readme\n").unwrap();
        std::fs::write(wt.join("dir with space/a b.txt"), "x\n").unwrap();
        std::fs::write(root.0.join("outside.txt"), "x\n").unwrap();
        std::fs::write(home_dir.0.join("notes.txt"), "x\n").unwrap();
        let home = home_dir.0.clone();
        Fixture {
            _root: root,
            _home: home_dir,
            wt,
            home,
        }
    }

    /// `source` as a chat reply is built, with the fixture's worktree and `home`.
    fn built_with(
        fx: &Fixture,
        source: &str,
        home: Option<&Path>,
    ) -> crate::markdown::MarkdownBlocks {
        let mut memo = FileLinkMemo::default();
        crate::markdown::build(
            source,
            &mut crate::markdown::Policy {
                surface: crate::markdown::Surface::Chat,
                base: Some(&fx.wt),
                diagrams: None,
                file_links: Some(FileLinkInputs {
                    worktree: &fx.wt,
                    home,
                    memo: &mut memo,
                }),
            },
        )
    }

    fn built(fx: &Fixture, source: &str) -> std::rc::Rc<[ely_gpui_component::documents::MarkdownBlock]> {
        built_with(fx, source, Some(&fx.home)).nodes
    }

    /// The paragraphs of a flat document.
    fn paragraphs(
        nodes: &[ely_gpui_component::documents::MarkdownBlock],
    ) -> Vec<&ely_gpui_component::documents::Inline> {
        nodes
            .iter()
            .filter_map(|node| match node {
                ely_gpui_component::documents::MarkdownBlock::Paragraph(inline) => Some(inline),
                _ => None,
            })
            .collect()
    }

    /// Every link address of the paragraphs, whatever it points at.
    fn link_marks(nodes: &[ely_gpui_component::documents::MarkdownBlock]) -> Vec<String> {
        paragraphs(nodes)
            .into_iter()
            .flat_map(|inline| inline.links.iter().map(|(_, url)| url.clone()))
            .collect()
    }

    fn file_marks(nodes: &[ely_gpui_component::documents::MarkdownBlock]) -> Vec<String> {
        link_marks(nodes)
            .into_iter()
            .filter(|url| url.starts_with("file://"))
            .collect()
    }

    fn texts(nodes: &[ely_gpui_component::documents::MarkdownBlock]) -> Vec<String> {
        paragraphs(nodes)
            .into_iter()
            .map(|inline| inline.text.clone())
            .collect()
    }

    #[test]
    fn network_and_device_paths_are_refused() {
        for shape in [
            r"\\host\share\x.md",
            "//host/share/x.md",
            r"\\?\C:\x",
            "//?/C:/x",
            r"\\?\UNC\host\share\x",
            r"\\.\pipe\x",
        ] {
            assert!(is_network_or_device_path(Path::new(shape)), "{shape}");
        }
    }
    #[test]
    fn ordinary_paths_are_not_refused() {
        for shape in ["/tmp/x", "docs/x.md", "C:/x", "./a//b", "/a//b"] {
            assert!(!is_network_or_device_path(Path::new(shape)), "{shape}");
        }
    }
    #[test]
    fn a_file_url_with_an_empty_host_and_a_network_path_is_refused() {
        let target = file_link_target("file:////host/share/x.md");
        assert!(
            target
                .as_ref()
                .is_none_or(|target| is_network_or_device_path(&target.path)),
            "{target:?}"
        );
    }
    #[test]
    fn a_double_slash_path_is_never_looked_up_even_when_it_names_a_local_file() {
        // `//tmp/..` is a local file on unix, and exactly the shape that
        // names a network host on Windows: it is refused before the disk is asked.
        let fx = fixture();
        let outside = fx.wt.parent().unwrap().join("outside.txt");
        let double = format!("/{}", outside.display());
        assert!(double.starts_with("//"));
        assert_eq!(resolve_chat_path(&double, &fx.wt, None), None);
        assert_eq!(resolve_chat_path(&format!("file://{double}"), &fx.wt, None), None);
        // the same file, written plainly, is still a link
        assert!(resolve_chat_path(&outside.display().to_string(), &fx.wt, None).is_some());
    }

    #[test]
    fn a_markdown_link_to_a_missing_file_url_is_unlinked_and_its_text_stays() {
        let fx = fixture();
        let doc = built(&fx, "see [x](file:///definitely/missing.md) now");
        assert_eq!(file_marks(&doc), Vec::<String>::new());
        assert_eq!(texts(&doc), vec!["see x now".to_string()]);
    }

    #[test]
    fn a_markdown_link_to_a_network_file_url_is_unlinked() {
        let fx = fixture();
        let doc = built(&fx, "see [x](file:////host/share/x.md) now");
        assert_eq!(file_marks(&doc), Vec::<String>::new());
        let outside = fx.wt.parent().unwrap().join("outside.txt");
        let doc = built(&fx, &format!("see [x](file://{}) now", format!("/{}", outside.display())));
        assert_eq!(file_marks(&doc), Vec::<String>::new());
    }

    #[test]
    fn a_markdown_link_to_an_existing_file_url_is_kept() {
        let fx = fixture();
        let readme = fx.wt.join("README.md");
        let doc = built(&fx, &format!("see [x](file://{}) now", readme.display()));
        assert_eq!(file_marks(&doc).len(), 1);
    }

    #[test]
    fn a_markdown_link_that_is_neither_web_nor_a_checked_file_loses_its_mark() {
        let fx = fixture();
        for target in [
            r"\\host\share\x.md",
            "//host/share/x.md",
            r"C:\path\evil.exe",
            "FILE:////host/share/x.md",
            "smb://host/x",
            "vscode://file/x",
            "tel:123",
            "#top",
            "missing/nowhere.md",
        ] {
            // angle brackets let a target hold backslashes and the like
            let source = format!("see [x](<{target}>) now");
            // the reader does make a link of it, so the pass is what removes it
            let read = ely_gpui_component::documents::parse_markdown_with(&source, &|_: &str| None);
            assert_eq!(link_marks(&read).len(), 1, "{target}");
            let doc = built(&fx, &source);
            assert_eq!(link_marks(&doc), Vec::<String>::new(), "{target}");
            assert_eq!(texts(&doc), vec!["see x now".to_string()], "{target}");
        }
    }

    #[test]
    fn a_web_link_keeps_its_mark_in_any_case_of_its_scheme() {
        let fx = fixture();
        for target in ["HTTPS://example.com/a", "https://example.com/a", "mailto:a@b.c"] {
            let doc = built(&fx, &format!("see [x]({target}) now"));
            assert_eq!(link_marks(&doc), vec![target.to_string()], "{target}");
        }
    }

    #[test]
    fn a_file_url_in_capitals_resolves_like_the_lowercase_one() {
        let fx = fixture();
        let readme = fx.wt.join("README.md");
        let doc = built(&fx, &format!("see [x](FILE://{}) now", readme.display()));
        assert_eq!(file_marks(&doc).len(), 1, "{:?}", link_marks(&doc));
        assert!(file_link_target("File:///tmp/a.md").is_some());
    }

    #[test]
    fn linking_never_changes_the_text() {
        let fx = fixture();
        let sources = [
            "l'ho messo in docs/NOME-FILE.md.",
            "`/docs/NOME-FILE.md`",
            "see src/main.rs:42:7 and src/main.rs#L42-L50",
            "(see ./README.md), \"docs/NOME-FILE.md\" docs/NOME-FILE.md:",
            "`dir with space/a b.txt` ~/notes.txt",
            "[il file](docs/NOME-FILE.md) and **README.md** and `README.md`",
            "[sito](https://example.com/docs/NOME-FILE.md) https://example.com/README.md",
            "missing/nowhere.md, 1.2.3, 2026/10/24, 3/4 docs/sub/ ../outside.txt",
            "```text\nsrc/main.rs\n```\n\n| a | b |\n|---|---|\n| README.md | `src/main.rs` |",
        ];
        for source in sources {
            // The same reply with no worktree to look in: nothing becomes a file link.
            let before = crate::markdown::build(
                source,
                &mut crate::markdown::Policy {
                    surface: crate::markdown::Surface::Chat,
                    base: None,
                    diagrams: None,
                    file_links: None,
                },
            );
            let after = built_with(&fx, source, Some(&fx.home));
            assert_eq!(before.text.text, after.text.text, "{source}");
            for inline in paragraphs(&after.nodes) {
                for (range, _) in &inline.links {
                    assert!(
                        range.end <= inline.text.len()
                            && inline.text.is_char_boundary(range.start)
                            && inline.text.is_char_boundary(range.end),
                        "{source}: {range:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn a_drive_letter_is_not_a_scheme() {
        assert_eq!(url_scheme(r"C:\x"), None);
        assert_eq!(url_scheme("C:/x"), None);
    }

    /// `local_image_path` on a URL, with the fixture's worktree and `home`.
    fn picture(fx: &Fixture, url: &str) -> Option<String> {
        local_image_path(url, &fx.wt, Some(&fx.home), &mut FileLinkMemo::default())
    }

    #[test]
    fn a_local_image_block_that_is_not_a_regular_file_becomes_its_alt_text() {
        // gpui reads a local picture with `fs::read` as soon as it is painted:
        // a device never ends, a FIFO blocks, and a path that is not a checked
        // file in the project is not a picture the response may show.
        let fx = fixture();
        std::fs::write(fx.wt.join("docs/a.png"), b"png").unwrap();
        let fifo = fx.wt.join("docs/pipe.png");
        let made_fifo = std::process::Command::new("mkfifo")
            .arg(&fifo)
            .status()
            .is_ok_and(|status| status.success());
        let mut urls = vec![
            ("/dev/zero", "/dev/zero"),
            ("/dev/random", "/dev/random"),
            ("missing", "docs/missing.png"),
            ("directory", "docs/sub"),
            ("../outside", "../outside.txt"),
            // a leading space or control character: the renderer would still see
            // `://` and fetch it, and the resolver would trim it to a file
            ("space web", " http://example.com/a.png"),
            ("control web", "\u{1}https://example.com/a.png"),
            ("space file", " docs/a.png"),
            ("tab file", "\tdocs/a.png"),
        ];
        if made_fifo {
            urls.push(("fifo", "docs/pipe.png"));
        } else {
            eprintln!("mkfifo unavailable: the FIFO case is skipped");
        }
        for (case, url) in urls {
            assert_eq!(picture(&fx, url), None, "{case}");
        }
    }

    #[test]
    fn a_local_image_block_that_is_a_file_reads_it_by_its_absolute_path() {
        // The renderer hands a non-URL to gpui as a path, which the process resolves
        // against its own cwd: the picture must name exactly the checked file.
        let fx = fixture();
        std::fs::write(fx.wt.join("docs/a.png"), b"png").unwrap();
        std::fs::write(fx.home.join("pic.png"), b"png").unwrap();
        let in_wt = fx.wt.join("docs/a.png").display().to_string();
        for (url, expected) in [
            ("docs/a.png".to_string(), in_wt.clone()),
            ("/docs/a.png".to_string(), in_wt.clone()),
            (in_wt.clone(), in_wt.clone()),
            (format!("file://{in_wt}"), in_wt.clone()),
            ("~/pic.png".to_string(), fx.home.join("pic.png").display().to_string()),
        ] {
            let found = picture(&fx, &url);
            assert_eq!(found, Some(expected), "{url}");
            assert!(Path::new(found.as_deref().unwrap()).is_absolute(), "{url}");
        }
    }

    #[test]
    fn a_local_image_is_shown_only_within_the_image_size_cap() {
        // The picture is read whole when painted: an empty file is no
        // picture, and past the chat's image cap the read would stall it.
        let fx = fixture();
        std::fs::write(fx.wt.join("docs/empty.png"), b"").unwrap();
        let sized = |name: &str, len: u64| {
            let file = std::fs::File::create(fx.wt.join("docs").join(name)).unwrap();
            file.set_len(len).unwrap();
        };
        sized("big.png", super::super::MAX_IMAGE_BYTES + 1);
        sized("cap.png", super::super::MAX_IMAGE_BYTES);
        for url in ["docs/empty.png", "docs/big.png"] {
            assert_eq!(picture(&fx, url), None, "{url}");
        }
        assert_eq!(
            picture(&fx, "docs/cap.png"),
            Some(fx.wt.join("docs/cap.png").display().to_string())
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn a_procfs_file_is_never_shown_as_a_local_image() {
        // procfs reports its files as regular and empty; pagemap reads on
        // for hundreds of gigabytes.
        if !Path::new("/proc/self/pagemap").exists() {
            eprintln!("/proc/self/pagemap missing: skipped");
            return;
        }
        let fx = fixture();
        assert_eq!(picture(&fx, "/proc/self/pagemap"), None);
    }

    #[test]
    fn a_local_image_is_read_from_the_disk_once_per_entry() {
        let fx = fixture();
        let file = fx.wt.join("docs/a.png");
        std::fs::write(&file, b"png").unwrap();
        let mut memo = FileLinkMemo::default();
        let first = local_image_path("docs/a.png", &fx.wt, None, &mut memo);
        assert_eq!(first, Some(file.display().to_string()));
        // a streamed chunk later, the entry is linked again with the same memo
        std::fs::remove_file(&file).unwrap();
        assert_eq!(
            local_image_path("docs/a.png", &fx.wt, None, &mut memo),
            first,
            "the second pass asked the disk again"
        );
    }

    #[test]
    fn a_picture_naming_a_windows_device_is_never_looked_up() {
        // Nothing here exists, so a refusal can only have been decided on the text. On
        // Windows a device name opens the device, and a COM port with nothing behind it
        // blocks the UI thread: it is refused on every platform, before the disk.
        let root = if cfg!(windows) {
            Path::new(r"C:\nonexistent-sirio-root")
        } else {
            Path::new("/nonexistent-sirio-root")
        };
        let (cwd, home) = (root.join("wt"), root.join("home"));
        let lookups = |url: &str| picture_lookups(url, &cwd, Some(&home));
        for url in [
            "COM1",
            "nul.png",
            "CONIN$",
            "aux/x.png",
            "NUL.tar.gz",
            "com1 .png",
            "LPT¹.png",
            "dot.png.",
            "sp /p.png",
            "docs/a.png:zone",
            "x/COM1:",
            "~/nul.png",
            "/nul.png",
            "/w/COM1.png",
            "file:///w/COM1%2Epng",
        ] {
            assert_eq!(lookups(url), None, "{url}");
        }
        // What is plain is asked about: the places the text may name, in order. A rooted
        // text is a path only where the platform says so, and else only the project's.
        let in_cwd = cwd.join("docs/a.png");
        assert_eq!(lookups("docs/a.png"), Some(vec![in_cwd.clone()]));
        assert_eq!(lookups("docs/a.png:12"), Some(vec![in_cwd.clone()]));
        let mut rooted = Vec::new();
        if Path::new("/docs/a.png").is_absolute() {
            rooted.push(PathBuf::from("/docs/a.png"));
        }
        rooted.push(in_cwd);
        assert_eq!(lookups("/docs/a.png"), Some(rooted));
    }

    /// A non-empty regular file at `dir`/`name`, its folders made.
    #[cfg(unix)]
    fn made(dir: &Path, name: &str) -> PathBuf {
        let file = dir.join(name);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, b"png").unwrap();
        assert!(file.is_file(), "{name}");
        file
    }

    #[cfg(unix)]
    #[test]
    fn a_picture_with_a_device_name_dot_space_or_stream_is_its_alt_text() {
        // Every file below exists with content, so only the refusal of its name keeps it
        // from being drawn.
        let fx = fixture();
        let wt = fx.wt.as_path();
        let home = fx.home.as_path();
        let decoded = format!("file://{}/COM1%2Epng", wt.display());
        made(wt, "COM1.png");
        let mut drawn = Vec::new();
        for (case, url, base, file) in [
            ("device", "COM1", wt, "COM1"),
            ("device with extension", "nul.png", wt, "nul.png"),
            ("console device", "CONIN$", wt, "CONIN$"),
            ("device folder", "aux/x.png", wt, "aux/x.png"),
            ("trailing dot", "dot.png.", wt, "dot.png."),
            ("trailing space", "sp /p.png", wt, "sp /p.png"),
            ("stream", "docs/a.png:zone", wt, "docs/a.png:zone"),
            ("device under home", "~/nul.png", home, "nul.png"),
            ("device by the project fallback", "/nul.png", wt, "nul.png"),
            ("device named by an escape in a file url", decoded.as_str(), wt, "COM1.png"),
        ] {
            made(base, file);
            if picture(&fx, url).is_some() {
                drawn.push(case);
            }
        }
        assert_eq!(drawn, Vec::<&str>::new(), "drawn although their names are refused");
    }

    #[cfg(unix)]
    #[test]
    fn a_picture_path_reaches_the_renderer_as_the_checked_file() {
        // The renderer reads what it is given as a URL: it cuts it at `?` and `#`, and decodes
        // `%XX`. Whatever it is handed must read back as exactly the file that was checked.
        use ely_gpui_component::documents::markdown_picture_path;
        let fx = fixture();
        let mut elsewhere = Vec::new();
        for (case, name) in [
            ("hash", "C#/a.png"),
            ("question mark", "what?.png"),
            ("escape in the name", "COM1%2Epng"),
            ("percent", "100%.png"),
            ("percent escape", "50%25.png"),
        ] {
            let file = made(&fx.wt, name);
            match picture(&fx, name) {
                Some(handed) if markdown_picture_path(&handed).as_deref() == Some(file.as_path()) => {}
                _ => elsewhere.push(case),
            }
        }
        assert_eq!(elsewhere, Vec::<&str>::new(), "not drawn, or drawn from another path");
    }

    #[cfg(unix)]
    #[test]
    fn a_picture_under_a_worktree_that_is_not_utf8_is_refused() {
        // A lossy copy of the path names another file: a folder spelled with U+FFFD, here.
        use std::os::unix::ffi::OsStrExt;
        let fx = fixture();
        let root = fx.wt.parent().unwrap();
        let (wt_bytes, home_bytes) = (&b"wt-\xff"[..], &b"home-\xff"[..]);
        let (bad_wt, bad_home) = (
            root.join(std::ffi::OsStr::from_bytes(wt_bytes)),
            root.join(std::ffi::OsStr::from_bytes(home_bytes)),
        );
        if std::fs::create_dir_all(&bad_wt).is_err() {
            eprintln!("the file system refuses a name that is not UTF-8: skipped");
            return;
        }
        std::fs::create_dir_all(&bad_home).unwrap();
        for folder in [&bad_wt, &bad_home] {
            made(folder, "a.png");
        }
        for bytes in [wt_bytes, home_bytes] {
            made(&root.join(String::from_utf8_lossy(bytes).as_ref()), "a.png");
        }
        let ask = |url: &str, cwd: &Path, home: &Path| {
            local_image_path(url, cwd, Some(home), &mut FileLinkMemo::default())
        };
        let mut drawn = Vec::new();
        if ask("a.png", &bad_wt, &fx.home).is_some() {
            drawn.push("worktree");
        }
        if ask("~/a.png", &fx.wt, &bad_home).is_some() {
            drawn.push("home");
        }
        assert_eq!(drawn, Vec::<&str>::new(), "drawn although their folder is not UTF-8");
    }

    #[cfg(unix)]
    #[test]
    fn an_absolute_picture_is_drawn_under_a_worktree_that_is_not_utf8() {
        // The absolute path names its file whatever the worktree is called: the worktree only
        // adds a fallback candidate, which is not UTF-8 and is never asked about.
        use std::os::unix::ffi::OsStrExt;
        let fx = fixture();
        let root = fx.wt.parent().unwrap();
        let bad_wt = root.join(std::ffi::OsStr::from_bytes(b"wt-\xff"));
        if std::fs::create_dir_all(&bad_wt).is_err() {
            eprintln!("the file system refuses a name that is not UTF-8: skipped");
            return;
        }
        let abs = made(&fx.wt, "abs.png");
        let abs_text = abs.display().to_string();
        assert_eq!(
            local_image_path(&abs_text, &bad_wt, Some(&fx.home), &mut FileLinkMemo::default()),
            Some(abs.display().to_string()),
            "an absolute picture does not depend on the worktree's name"
        );
    }

    #[cfg(unix)]
    #[test]
    fn the_worktrees_own_names_are_not_judged() {
        // Only what the text contributes is judged: a project folder called `con` and a
        // home called `aux.d` are the user's, and do not refuse every picture inside them.
        let fx = fixture();
        let root = fx.wt.parent().unwrap();
        let (project, home) = (root.join("con"), root.join("aux.d"));
        let in_project = made(&project, "a.png");
        let in_home = made(&home, "a.png");
        let ask = |url: &str| local_image_path(url, &project, Some(&home), &mut FileLinkMemo::default());
        assert_eq!(ask("a.png"), Some(in_project.display().to_string()));
        assert_eq!(ask("~/a.png"), Some(in_home.display().to_string()));
    }

    #[test]
    fn a_line_suffix_still_names_the_picture() {
        let fx = fixture();
        let file = fx.wt.join("docs/a.png");
        std::fs::write(&file, b"png").unwrap();
        for url in ["docs/a.png:12", "docs/a.png:12:4"] {
            assert_eq!(picture(&fx, url), Some(file.display().to_string()), "{url}");
        }
    }

    #[test]
    fn a_click_opens_a_regular_file_or_one_that_vanished_and_nothing_else() {
        let fx = fixture();
        let file = fx.wt.join("README.md");
        assert!(may_open_on_click(&file), "a file");
        // deleted since the response was built: the tab shows it as missing
        assert!(may_open_on_click(&fx.wt.join("docs/gone.md")), "a vanished file");
        assert!(!may_open_on_click(&fx.wt.join("docs")), "a directory");
        assert!(!may_open_on_click(Path::new("//host/share/x.md")), "a share");
        #[cfg(unix)]
        {
            assert!(!may_open_on_click(Path::new("/dev/null")), "a device");
            let fifo = fx.wt.join("pipe");
            let made = std::process::Command::new("mkfifo").arg(&fifo).status();
            if made.is_ok_and(|status| status.success()) {
                assert!(!may_open_on_click(&fifo), "a FIFO");
            }
            // a file inside something that is not a directory is not a vanished file
            assert!(!may_open_on_click(&file.join("inside")), "under a file");
        }
    }

    #[test]
    fn a_file_url_round_trips() {
        let paths = [
            "/tmp/plain.txt",
            "/tmp/with space/a b.txt",
            "/tmp/hash#tag/file#1.txt",
            "/tmp/100%/done%20.txt",
            "/tmp/città/è già.txt",
            "/tmp/a:b/f.txt",
            "/C:/dir/f.rs",
        ];
        for path in paths {
            for (line, column) in [(None, None), (Some(42), None), (Some(42), Some(7))] {
                let target = FileLinkTarget {
                    path: PathBuf::from(path),
                    line,
                    column,
                };
                let url = file_link_url(&target);
                assert!(url.starts_with("file://"), "{url}");
                assert_eq!(file_link_target(&url), Some(target), "{url}");
            }
        }
        assert_eq!(file_link_target("https://example.com/a.md"), None);
        assert_eq!(file_link_target("docs/a.md"), None);
    }
}
