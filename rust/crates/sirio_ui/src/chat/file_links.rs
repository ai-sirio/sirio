//! Finds the file paths a chat response names and marks them as links.
//!
//! Works on the response's `markdown::Doc`, after parsing. A path becomes a
//! link only when it names an existing regular file (see
//! `docs/superpowers/specs/2026-10-24-chat-file-links-design.md`), so every
//! candidate costs one disk lookup, which [`FileLinkMemo`] answers once per
//! entry instead of once per streamed chunk.

use std::collections::HashMap;
use std::path::Path;

use sirio_project::{FileLinkTarget, resolve_file_link};

/// Candidate string -> what it resolved to, for one chat entry. Not keyed by
/// working directory or home: an entry's memo lives and dies with its entry.
#[derive(Default)]
pub(crate) struct FileLinkMemo(HashMap<String, Option<FileLinkTarget>>);

impl FileLinkMemo {
    pub(crate) fn clear(&mut self) {
        self.0.clear();
    }

    /// The memoised resolution of one candidate, asking the disk only the
    /// first time the candidate is seen.
    fn resolve(
        &mut self,
        candidate: &str,
        cwd: &Path,
        home: Option<&Path>,
    ) -> Option<FileLinkTarget> {
        if let Some(known) = self.0.get(candidate) {
            return known.clone();
        }
        let found = resolve_chat_path(candidate, cwd, home);
        self.0.insert(candidate.to_string(), found.clone());
        found
    }
}

/// Resolves what a response wrote as a path to an existing regular file.
///
/// `#L42` / `#L42-L50` and `:42-50` become a plain line (`:42`), `~/` expands
/// against `home`, and an absolute path that names nothing is tried again
/// inside `cwd`, because an agent's `/docs/x.md` usually means the project's.
pub(crate) fn resolve_chat_path(
    raw: &str,
    cwd: &Path,
    home: Option<&Path>,
) -> Option<FileLinkTarget> {
    let normalized = lowercase_file_scheme(normalize_location(raw.trim()));
    let text = match normalized.strip_prefix("~/") {
        Some(rest) => home?.join(rest).to_string_lossy().into_owned(),
        None => normalized,
    };
    // Not even the fallback below: `//host/x` is a share, not "x" in the project.
    if is_network_or_device_path(Path::new(&text)) {
        return None;
    }
    if let Some(target) = existing_file(&text, cwd) {
        return Some(target);
    }
    if text.starts_with("file://") || !text.starts_with(['/', '\\']) {
        return None;
    }
    existing_file(text.trim_start_matches(['/', '\\']), cwd)
}

/// Whether `path` names a network share or a device rather than a place on
/// this machine: `//host/..` and `\\host\..` (also `\\?\` and `\\.\`), and on
/// Windows a UNC, device or verbatim prefix. Text an agent wrote can be
/// prompt-injected, and asking the disk about such a path opens a connection
/// (NTLM on Windows) or hangs on a dead mount, so it is refused before any
/// lookup, and again before a click opens it.
pub(crate) fn is_network_or_device_path(path: &Path) -> bool {
    let text = path.to_string_lossy();
    if text.starts_with("//") || text.starts_with("\\\\") {
        return true;
    }
    #[cfg(windows)]
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

fn existing_file(text: &str, cwd: &Path) -> Option<FileLinkTarget> {
    resolve_file_link(text, cwd)
        .filter(|target| !is_network_or_device_path(&target.path) && target.path.is_file())
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

/// Marks every existing file a response names, in its prose, its inline code
/// and its Markdown links. The text is never touched, only `marks` grows.
///
/// A new mark on prose is appended; one over an inline-code span goes just
/// before the code mark, so the link stays the outer of the two, which is the
/// order `Text::marks` documents (outermost first) and what the parser writes
/// for ``[`x`](url)``.
pub(crate) fn link_file_paths(
    doc: &mut markdown::Doc,
    cwd: &Path,
    home: Option<&Path>,
    memo: &mut FileLinkMemo,
) {
    for block in &mut doc.blocks {
        // A block that would open, or load, something that is not on the web
        // is shown as plain text instead. A bookmark card opens its URL when
        // clicked; an image block loads its URL as a picture, which for a
        // share or a device is a connection the reader never asked for. (A
        // plain local path is a picture the reader can see, and stays.)
        match &block.kind {
            markdown::BlockKind::Bookmark { url, .. } if !is_web_target(url) => {
                block.kind = markdown::BlockKind::Paragraph(markdown::Text::plain(url.clone()));
            }
            markdown::BlockKind::Image { url, alt, .. }
                if !is_web_target(url)
                    && (url_scheme(url).is_some()
                        || is_network_or_device_path(Path::new(url))) =>
            {
                block.kind = markdown::BlockKind::Paragraph(markdown::Text::plain(alt.text.clone()));
            }
            _ => {}
        }
        if matches!(
            block.kind,
            markdown::BlockKind::Code { .. } | markdown::BlockKind::Image { .. }
        ) {
            continue;
        }
        for part in block.parts() {
            if let Some(text) = block.text_at_mut(part) {
                link_text(text, cwd, home, memo);
            }
        }
    }
}

fn link_text(text: &mut markdown::Text, cwd: &Path, home: Option<&Path>, memo: &mut FileLinkMemo) {
    // The links an agent wrote are an allowlist: one survives only if it is a
    // web link (http, https, mailto) or points at a file that was checked, and
    // then it is retargeted to that file. Anything else (a path that is not
    // there, a share, a drive path, `smb:`, `vscode:`, an anchor) would be
    // handed to the system opener by a click, so it loses its mark and its
    // text stays.
    text.marks.retain_mut(|span| {
        // A chip or an inline picture is clicked open like a link, so it gets
        // the same rule, minus the retargeting: not web, no mark, text stays.
        if let markdown::Mark::Mention { url, .. } | markdown::Mark::Image(url) = &span.mark {
            return is_web_target(url);
        }
        let markdown::Mark::Link(target) = &span.mark else {
            return true;
        };
        if is_web_target(target) {
            return true;
        }
        match memo.resolve(target, cwd, home) {
            Some(found) => {
                span.mark = markdown::Mark::Link(file_link_url(&found));
                true
            }
            None => false,
        }
    });

    let mut before_code: Vec<(usize, markdown::MarkSpan)> = Vec::new();
    let mut in_prose: Vec<markdown::MarkSpan> = Vec::new();
    // Inline code: the trimmed span is the candidate. Code already inside a
    // Markdown link keeps that link.
    for (index, span) in text.marks.iter().enumerate() {
        if span.mark != markdown::Mark::Code || inside_a_link(&text.marks, &span.range) {
            continue;
        }
        let Some(slice) = text.text.get(span.range.clone()) else {
            continue;
        };
        let trimmed = slice.trim();
        if trimmed.is_empty() || trimmed.contains("://") {
            continue;
        }
        let start = span.range.start + (slice.len() - slice.trim_start().len());
        let range = start..start + trimmed.len();
        if let Some(found) = memo.resolve(trimmed, cwd, home) {
            before_code.push((
                index,
                markdown::MarkSpan {
                    range,
                    mark: markdown::Mark::Link(file_link_url(&found)),
                },
            ));
        }
    }
    // Prose: whitespace-delimited tokens outside code, links, mentions and images.
    for segment in prose_segments(text) {
        for (token_start, token) in tokens(&text.text, segment) {
            let Some(range) = trim_token(token, token_start) else {
                continue;
            };
            let candidate = &text.text[range.clone()];
            if !(candidate.contains('/') || candidate.contains('.')) || candidate.contains("://") {
                continue;
            }
            if let Some(found) = memo.resolve(candidate, cwd, home) {
                in_prose.push(markdown::MarkSpan {
                    range,
                    mark: markdown::Mark::Link(file_link_url(&found)),
                });
            }
        }
    }
    // Insert back to front so earlier indexes stay valid.
    for (index, span) in before_code.into_iter().rev() {
        text.marks.insert(index, span);
    }
    text.marks.extend(in_prose);
}

fn inside_a_link(marks: &[markdown::MarkSpan], range: &std::ops::Range<usize>) -> bool {
    marks.iter().any(|span| {
        matches!(span.mark, markdown::Mark::Link(_))
            && span.range.start <= range.start
            && range.end <= span.range.end
    })
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
fn is_web_target(target: &str) -> bool {
    url_scheme(target).is_some_and(|scheme| {
        ["http", "https", "mailto"]
            .iter()
            .any(|web| scheme.eq_ignore_ascii_case(web))
    })
}

/// The byte ranges of `text` that no code, link, mention or image mark covers.
fn prose_segments(text: &markdown::Text) -> Vec<std::ops::Range<usize>> {
    let mut covered: Vec<std::ops::Range<usize>> = text
        .marks
        .iter()
        .filter(|span| {
            matches!(
                span.mark,
                markdown::Mark::Code
                    | markdown::Mark::Link(_)
                    | markdown::Mark::Mention { .. }
                    | markdown::Mark::Image(_)
            )
        })
        .map(|span| span.range.clone())
        .collect();
    covered.sort_by_key(|range| range.start);
    let mut segments = Vec::new();
    let mut cursor = 0;
    for range in covered {
        if range.start > cursor {
            segments.push(cursor..range.start.min(text.text.len()));
        }
        cursor = cursor.max(range.end);
    }
    if cursor < text.text.len() {
        segments.push(cursor..text.text.len());
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

    type Link = (String, PathBuf, Option<usize>, Option<usize>);

    fn linked_with(fx: &Fixture, source: &str, home: Option<&Path>) -> markdown::Doc {
        let mut doc = markdown::parse(source);
        link_file_paths(&mut doc, &fx.wt, home, &mut FileLinkMemo::default());
        doc
    }

    fn linked(fx: &Fixture, source: &str) -> markdown::Doc {
        linked_with(fx, source, Some(&fx.home))
    }

    /// Every file link in the doc: the text it covers, and what it opens.
    fn links(doc: &markdown::Doc) -> Vec<Link> {
        let mut found = Vec::new();
        for block in &doc.blocks {
            for part in block.parts() {
                let Some(text) = block.text_at(part) else {
                    continue;
                };
                for span in &text.marks {
                    if let markdown::Mark::Link(url) = &span.mark
                        && let Some(target) = file_link_target(url)
                    {
                        found.push((
                            text.text[span.range.clone()].to_string(),
                            target.path,
                            target.line,
                            target.column,
                        ));
                    }
                }
            }
        }
        found
    }

    fn link(text: &str, path: PathBuf, line: Option<usize>, column: Option<usize>) -> Link {
        (text.to_string(), path, line, column)
    }

    fn texts(doc: &markdown::Doc) -> Vec<String> {
        let mut all = Vec::new();
        for block in &doc.blocks {
            for part in block.parts() {
                if let Some(text) = block.text_at(part) {
                    all.push(text.text.clone());
                }
            }
        }
        all
    }

    #[test]
    fn network_and_device_paths_are_refused() {
        for shape in [
            r"\\host\share\x.md",
            "//host/share/x.md",
            r"\\?\C:\x",
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

    fn file_marks(doc: &markdown::Doc) -> Vec<String> {
        let mut found = Vec::new();
        for block in &doc.blocks {
            for part in block.parts() {
                if let Some(text) = block.text_at(part) {
                    for span in &text.marks {
                        if let markdown::Mark::Link(url) = &span.mark
                            && url.starts_with("file://")
                        {
                            found.push(url.clone());
                        }
                    }
                }
            }
        }
        found
    }

    #[test]
    fn a_markdown_link_to_a_missing_file_url_is_unlinked_and_its_text_stays() {
        let fx = fixture();
        let doc = linked(&fx, "see [x](file:///definitely/missing.md) now");
        assert_eq!(file_marks(&doc), Vec::<String>::new());
        assert_eq!(texts(&doc), vec!["see x now".to_string()]);
    }

    #[test]
    fn a_markdown_link_to_a_network_file_url_is_unlinked() {
        let fx = fixture();
        let doc = linked(&fx, "see [x](file:////host/share/x.md) now");
        assert_eq!(file_marks(&doc), Vec::<String>::new());
        let outside = fx.wt.parent().unwrap().join("outside.txt");
        let doc = linked(&fx, &format!("see [x](file://{}) now", format!("/{}", outside.display())));
        assert_eq!(file_marks(&doc), Vec::<String>::new());
    }

    #[test]
    fn a_markdown_link_to_an_existing_file_url_is_kept() {
        let fx = fixture();
        let readme = fx.wt.join("README.md");
        let doc = linked(&fx, &format!("see [x](file://{}) now", readme.display()));
        assert_eq!(file_marks(&doc).len(), 1);
    }

    /// Every `Mark::Link` target in the doc, whatever it points at.
    fn link_marks(doc: &markdown::Doc) -> Vec<String> {
        let mut found = Vec::new();
        for block in &doc.blocks {
            for part in block.parts() {
                if let Some(text) = block.text_at(part) {
                    for span in &text.marks {
                        if let markdown::Mark::Link(url) = &span.mark {
                            found.push(url.clone());
                        }
                    }
                }
            }
        }
        found
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
            // the parser does make a link of it, so the pass is what removes it
            assert_eq!(link_marks(&markdown::parse(&source)).len(), 1, "{target}");
            let doc = linked(&fx, &source);
            assert_eq!(link_marks(&doc), Vec::<String>::new(), "{target}");
            assert_eq!(texts(&doc), vec!["see x now".to_string()], "{target}");
        }
    }

    #[test]
    fn a_web_link_keeps_its_mark_in_any_case_of_its_scheme() {
        let fx = fixture();
        for target in ["HTTPS://example.com/a", "https://example.com/a", "mailto:a@b.c"] {
            let doc = linked(&fx, &format!("see [x]({target}) now"));
            assert_eq!(link_marks(&doc), vec![target.to_string()], "{target}");
        }
    }

    #[test]
    fn a_file_url_in_capitals_resolves_like_the_lowercase_one() {
        let fx = fixture();
        let readme = fx.wt.join("README.md");
        let doc = linked(&fx, &format!("see [x](FILE://{}) now", readme.display()));
        assert_eq!(links(&doc).len(), 1, "{:?}", link_marks(&doc));
        assert!(file_link_target("File:///tmp/a.md").is_some());
    }

    /// Every URL the doc could hand to the system when clicked: a link,
    /// chip or image mark, and a bookmark or image block.
    fn clickable(doc: &markdown::Doc) -> Vec<String> {
        let mut found = Vec::new();
        for block in &doc.blocks {
            match &block.kind {
                markdown::BlockKind::Bookmark { url, .. }
                | markdown::BlockKind::Image { url, .. } => found.push(url.clone()),
                _ => {}
            }
            for part in block.parts() {
                if let Some(text) = block.text_at(part) {
                    for span in &text.marks {
                        match &span.mark {
                            markdown::Mark::Link(url)
                            | markdown::Mark::Image(url)
                            | markdown::Mark::Mention { url, .. } => found.push(url.clone()),
                            _ => {}
                        }
                    }
                }
            }
        }
        found
    }

    #[test]
    fn a_non_web_chip_or_inline_image_loses_its_click() {
        let fx = fixture();
        for source in [
            "see <smb://host/x> now",
            "see <FILE:////host/x> now",
            "<smb://host/x>",
            r#"see [x](<C:\evil.exe> "chip") now"#,
            r#"see [x](<\\host\share\x> "embed") now"#,
            r"see ![x](<\\host\share\x.exe>) now",
        ] {
            // the parser does make a clickable of it, so the pass is what removes it
            assert_eq!(clickable(&markdown::parse(source)).len(), 1, "{source}");
            let doc = linked(&fx, source);
            assert_eq!(clickable(&doc), Vec::<String>::new(), "{source}");
            assert_eq!(doc.blocks.len(), 1, "{source}");
            assert_eq!(texts(&doc), texts(&markdown::parse(source)), "{source}");
        }
    }

    #[test]
    fn a_non_web_bookmark_or_image_block_becomes_a_paragraph() {
        let fx = fixture();
        // The parser only makes a bookmark of an http(s) URL, so a non-web one is
        // built by hand: a document from elsewhere (or a parser that grows) is
        // not trusted either.
        let bookmark = |url: &str| markdown::Doc {
            blocks: vec![markdown::Block::new(markdown::BlockKind::Bookmark {
                url: url.to_string(),
                form: markdown::Form::Chip,
            })],
        };
        for (raw, shown) in [
            (bookmark("smb://host/x"), "smb://host/x"),
            (bookmark(r"\\host\share\x"), r"\\host\share\x"),
            (markdown::parse("![alt words](//host/share/x.png)"), "alt words"),
            (markdown::parse("![alt words](smb://host/x.png)"), "alt words"),
        ] {
            assert!(
                matches!(
                    raw.blocks[0].kind,
                    markdown::BlockKind::Bookmark { .. } | markdown::BlockKind::Image { .. }
                ),
                "{shown}: {:?}",
                raw.blocks[0].kind
            );
            let source = shown;
            let mut doc = raw;
            link_file_paths(&mut doc, &fx.wt, Some(&fx.home), &mut FileLinkMemo::default());
            assert_eq!(clickable(&doc), Vec::<String>::new(), "{source}");
            assert!(
                matches!(&doc.blocks[0].kind, markdown::BlockKind::Paragraph(text) if text.text == shown),
                "{source}: {:?}",
                doc.blocks[0].kind
            );
        }
    }

    #[test]
    fn web_chips_images_and_blocks_keep_their_click() {
        let fx = fixture();
        for (source, url) in [
            ("see <https://example.com> now", "https://example.com"),
            (r#"see [x](https://example.com "chip") now"#, "https://example.com"),
            ("see ![x](https://example.com/a.png) now", "https://example.com/a.png"),
            ("<https://example.com>", "https://example.com"),
            ("![x](https://example.com/a.png)", "https://example.com/a.png"),
            ("![x](docs/a.png)", "docs/a.png"),
        ] {
            let before = markdown::parse(source);
            assert_eq!(clickable(&before), vec![url.to_string()], "{source}");
            assert_eq!(linked(&fx, source), before, "{source}");
        }
    }

    #[test]
    fn a_relative_path_in_prose_is_linked() {
        let fx = fixture();
        let doc = linked(&fx, "l'ho messo in docs/NOME-FILE.md.");
        assert_eq!(
            links(&doc),
            vec![link(
                "docs/NOME-FILE.md",
                fx.wt.join("docs/NOME-FILE.md"),
                None,
                None
            )]
        );
    }

    #[test]
    fn a_leading_slash_falls_back_to_the_worktree() {
        let fx = fixture();
        let doc = linked(&fx, "`/docs/NOME-FILE.md`");
        assert_eq!(
            links(&doc),
            vec![link(
                "/docs/NOME-FILE.md",
                fx.wt.join("docs/NOME-FILE.md"),
                None,
                None
            )]
        );
    }

    #[test]
    fn an_absolute_path_that_exists_is_taken_as_written() {
        let fx = fixture();
        let written = format!("{}/src/main.rs", fx.wt.display());
        let doc = linked(&fx, &format!("open {written} now"));
        assert_eq!(
            links(&doc),
            vec![link(&written, fx.wt.join("src/main.rs"), None, None)]
        );
    }

    #[test]
    fn line_and_column_suffixes_are_kept() {
        let fx = fixture();
        let main = fx.wt.join("src/main.rs");
        assert_eq!(
            links(&linked(&fx, "see src/main.rs:42")),
            vec![link("src/main.rs:42", main.clone(), Some(42), None)]
        );
        assert_eq!(
            links(&linked(&fx, "see src/main.rs:42:7")),
            vec![link("src/main.rs:42:7", main, Some(42), Some(7))]
        );
    }

    #[test]
    fn a_range_and_a_github_anchor_open_at_their_first_line() {
        let fx = fixture();
        let main = fx.wt.join("src/main.rs");
        assert_eq!(
            links(&linked(&fx, "see src/main.rs:42-50")),
            vec![link("src/main.rs:42-50", main.clone(), Some(42), None)]
        );
        assert_eq!(
            links(&linked(&fx, "see src/main.rs#L42")),
            vec![link("src/main.rs#L42", main.clone(), Some(42), None)]
        );
        assert_eq!(
            links(&linked(&fx, "see src/main.rs#L42-L50")),
            vec![link("src/main.rs#L42-L50", main, Some(42), None)]
        );
    }

    #[test]
    fn surrounding_punctuation_is_not_part_of_the_path() {
        let fx = fixture();
        let covered = |source: &str| -> Vec<String> {
            links(&linked(&fx, source))
                .into_iter()
                .map(|link| link.0)
                .collect()
        };
        assert_eq!(covered("(see ./README.md),"), vec!["./README.md"]);
        assert_eq!(covered("\"docs/NOME-FILE.md\""), vec!["docs/NOME-FILE.md"]);
        assert_eq!(covered("docs/NOME-FILE.md:"), vec!["docs/NOME-FILE.md"]);
    }

    #[test]
    fn a_bare_file_name_resolves_against_the_worktree() {
        let fx = fixture();
        assert_eq!(
            links(&linked(&fx, "see README.md for details")),
            vec![link("README.md", fx.wt.join("README.md"), None, None)]
        );
    }

    #[test]
    fn inline_code_may_hold_spaces_prose_may_not() {
        let fx = fixture();
        assert_eq!(
            links(&linked(&fx, "`dir with space/a b.txt`")),
            vec![link(
                "dir with space/a b.txt",
                fx.wt.join("dir with space/a b.txt"),
                None,
                None
            )]
        );
        assert!(links(&linked(&fx, "dir with space/a b.txt")).is_empty());
    }

    #[test]
    fn a_home_relative_path_expands_to_home() {
        let fx = fixture();
        assert_eq!(
            links(&linked(&fx, "see ~/notes.txt")),
            vec![link("~/notes.txt", fx.home.join("notes.txt"), None, None)]
        );
        assert!(links(&linked_with(&fx, "see ~/notes.txt", None)).is_empty());
    }

    #[test]
    fn a_markdown_link_to_a_file_is_retargeted() {
        let fx = fixture();
        let doc = linked(&fx, "[il file](docs/NOME-FILE.md)");
        let path = fx.wt.join("docs/NOME-FILE.md");
        assert_eq!(
            links(&doc),
            vec![link("il file", path.clone(), None, None)]
        );
        let expected = file_link_url(&FileLinkTarget {
            path,
            line: None,
            column: None,
        });
        let marks = &doc.blocks[0].text_at(markdown::Part::Body).unwrap().marks;
        assert!(
            marks
                .iter()
                .any(|span| span.mark == markdown::Mark::Link(expected.clone())),
            "{marks:?}"
        );
    }

    #[test]
    fn web_links_and_urls_are_left_alone() {
        let fx = fixture();
        let doc = linked(
            &fx,
            "[sito](https://example.com/docs/NOME-FILE.md) and https://example.com/README.md",
        );
        assert!(links(&doc).is_empty());
        let marks = &doc.blocks[0].text_at(markdown::Part::Body).unwrap().marks;
        assert!(
            marks.iter().any(|span| span.mark
                == markdown::Mark::Link("https://example.com/docs/NOME-FILE.md".to_string())),
            "{marks:?}"
        );
    }

    #[test]
    fn what_does_not_exist_is_never_linked() {
        let fx = fixture();
        let doc = linked(&fx, "missing/nowhere.md, 1.2.3, 2026/10/24, 3/4");
        assert!(links(&doc).is_empty());
    }

    #[test]
    fn a_directory_is_never_linked() {
        let fx = fixture();
        assert!(links(&linked(&fx, "docs/sub/")).is_empty());
        assert!(links(&linked(&fx, "docs/sub")).is_empty());
        assert!(links(&linked(&fx, "`docs/sub`")).is_empty());
    }

    #[test]
    fn climbing_out_of_the_worktree_is_never_linked() {
        let fx = fixture();
        assert!(links(&linked(&fx, "../outside.txt")).is_empty());
        assert!(links(&linked(&fx, "`../outside.txt`")).is_empty());
    }

    #[test]
    fn fenced_code_is_never_linked() {
        let fx = fixture();
        let doc = linked(&fx, "```text\nsrc/main.rs\n```");
        assert!(links(&doc).is_empty());
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
            let before = markdown::parse(source);
            let after = linked(&fx, source);
            assert_eq!(texts(&before), texts(&after), "{source}");
            for block in &after.blocks {
                for part in block.parts() {
                    let text = block.text_at(part).unwrap();
                    for span in &text.marks {
                        assert!(
                            span.range.end <= text.text.len()
                                && text.text.is_char_boundary(span.range.start)
                                && text.text.is_char_boundary(span.range.end),
                            "{source}: {span:?}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn the_memo_answers_without_the_disk_until_cleared() {
        let fx = fixture();
        let mut memo = FileLinkMemo::default();
        let mut first = markdown::parse("see README.md");
        link_file_paths(&mut first, &fx.wt, Some(&fx.home), &mut memo);
        assert_eq!(links(&first).len(), 1);

        std::fs::remove_file(fx.wt.join("README.md")).unwrap();
        let mut second = markdown::parse("see README.md");
        link_file_paths(&mut second, &fx.wt, Some(&fx.home), &mut memo);
        assert_eq!(links(&second).len(), 1, "the memo still knows it");

        memo.clear();
        let mut third = markdown::parse("see README.md");
        link_file_paths(&mut third, &fx.wt, Some(&fx.home), &mut memo);
        assert!(links(&third).is_empty());
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
