//! File-backed centre-column tabs.
//!
//! File IO is deliberately split from rendering: the loader is a small pure
//! function that runs on GPUI's background executor, while this view only
//! paints the completed result and a visible state for every failure mode.

use gpui::{AnyElement, Context, Render, Task, Window, div, prelude::*, px};
use std::path::{Path, PathBuf};
use tiller_markdown::{Document, parse};
use tiller_theme::Theme;

use crate::chat::Chat;

/// Maximum file size accepted by the viewer. Refusing a larger file keeps a
/// click in the Files tree from allocating an unbounded string on the UI
/// process; the notice is rendered in place of the content.
pub const MAX_FILE_BYTES: u64 = 1_048_576;

#[derive(Debug)]
enum LoadedFile {
    Markdown(Document),
    Text(Vec<String>),
    Binary,
    TooLarge { size: u64 },
    Unreadable(String),
}

#[derive(Debug)]
enum FileState {
    Loading,
    Loaded(LoadedFile),
}

/// A tab showing one path. The entity remains owned by the workspace while
/// another tab is active, so switching away never reloads or loses content.
pub struct FileView {
    path: PathBuf,
    state: FileState,
    load_task: Option<Task<()>>,
}

impl FileView {
    /// Starts loading `path` without doing filesystem work during render.
    pub fn new(path: PathBuf, cx: &mut Context<Self>) -> Self {
        let path_for_task = path.clone();
        let load_task = cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move { load_file(&path_for_task) })
                .await;
            let _ = this.update(cx, |view, cx| {
                view.state = FileState::Loaded(result);
                view.load_task = None;
                cx.notify();
            });
        });
        Self {
            path,
            state: FileState::Loading,
            load_task: Some(load_task),
        }
    }

    fn render_header(&self, theme: Theme) -> impl IntoElement {
        div()
            .w_full()
            .border_b_1()
            .border_color(theme.hairline)
            .px(px(20.0))
            .py(px(10.0))
            .text_size(px(11.0))
            .text_color(theme.meta)
            .child(self.path.display().to_string())
    }

    fn render_state(&self, theme: Theme) -> AnyElement {
        match &self.state {
            FileState::Loading => notice("Loading file…", theme),
            FileState::Loaded(LoadedFile::Binary) => notice(
                "This file appears to be binary and cannot be shown as text.",
                theme,
            ),
            FileState::Loaded(LoadedFile::TooLarge { size }) => notice(
                format!(
                    "This file is too large to open ({size} bytes; limit is {MAX_FILE_BYTES} bytes)."
                ),
                theme,
            ),
            FileState::Loaded(LoadedFile::Unreadable(error)) => {
                notice(format!("Unable to open this file: {error}"), theme)
            }
            FileState::Loaded(LoadedFile::Markdown(document)) => div()
                .id("file-markdown-scroll")
                .size_full()
                .overflow_y_scroll()
                .child(
                    div()
                        .w_full()
                        .max_w(px(800.0))
                        .mx_auto()
                        .p(px(24.0))
                        .child(Chat::render_markdown_document(document.clone(), &theme)),
                )
                .into_any_element(),
            FileState::Loaded(LoadedFile::Text(lines)) => div()
                .id("file-text-scroll")
                .size_full()
                .overflow_y_scroll()
                .p(px(16.0))
                .child(
                    div()
                        .font_family("SFMono-Regular")
                        .text_size(px(12.0))
                        .text_color(theme.title)
                        .flex()
                        .flex_col()
                        .children(lines.iter().enumerate().map(|(index, line)| {
                            div()
                                .id(("file-line", index))
                                .w_full()
                                .min_h(px(18.0))
                                .flex()
                                .whitespace_nowrap()
                                .child(
                                    div()
                                        .w(px(52.0))
                                        .flex_none()
                                        .text_color(theme.meta)
                                        .child(format!("{:>5} ", index + 1)),
                                )
                                .child(line.clone())
                        })),
                )
                .into_any_element(),
        }
    }
}

impl Render for FileView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *Theme::get(cx);
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(theme.chat_surface)
            .child(self.render_header(theme))
            .child(
                div()
                    .flex_1()
                    .min_h(px(0.0))
                    .child(self.render_state(theme)),
            )
    }
}

fn notice(message: impl Into<String>, theme: Theme) -> AnyElement {
    div()
        .size_full()
        .flex()
        .items_center()
        .justify_center()
        .p(px(24.0))
        .text_size(px(13.0))
        .text_color(theme.subtitle)
        .child(message.into())
        .into_any_element()
}

fn load_file(path: &Path) -> LoadedFile {
    let metadata = match std::fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(error) => return LoadedFile::Unreadable(error.to_string()),
    };
    if metadata.len() > MAX_FILE_BYTES {
        return LoadedFile::TooLarge {
            size: metadata.len(),
        };
    }

    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) => return LoadedFile::Unreadable(error.to_string()),
    };
    if bytes.contains(&0) {
        return LoadedFile::Binary;
    }
    let text = match String::from_utf8(bytes) {
        Ok(text) => text,
        Err(_) => return LoadedFile::Binary,
    };
    if is_markdown(path) {
        LoadedFile::Markdown(parse(&text))
    } else {
        LoadedFile::Text(text.lines().map(str::to_owned).collect())
    }
}

fn is_markdown(path: &Path) -> bool {
    path.extension().is_some_and(|extension| {
        extension.eq_ignore_ascii_case("md") || extension.eq_ignore_ascii_case("markdown")
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tiller_markdown::{Block, Inline};
    use std::sync::atomic::{AtomicU64, Ordering};

    struct TempFile(PathBuf);

    impl TempFile {
        fn new(extension: &str, bytes: &[u8]) -> Self {
            static COUNTER: AtomicU64 = AtomicU64::new(0);
            let id = COUNTER.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "tiller-file-view-{}-{id}.{extension}",
                std::process::id()
            ));
            std::fs::write(&path, bytes).expect("write temporary file");
            Self(path)
        }
    }

    impl Drop for TempFile {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }

    #[test]
    fn markdown_loads_into_expected_blocks() {
        let file = TempFile::new("md", b"# Title\n\nHello **world**.");
        let LoadedFile::Markdown(document) = load_file(&file.0) else {
            panic!("markdown should parse as markdown")
        };
        assert_eq!(
            document.blocks,
            vec![
                Block::Heading {
                    level: 1,
                    inline: vec![Inline::Text("Title".into())],
                },
                Block::Paragraph {
                    inline: vec![
                        Inline::Text("Hello ".into()),
                        Inline::Strong(vec![Inline::Text("world".into())]),
                        Inline::Text(".".into()),
                    ],
                },
            ]
        );
    }

    #[test]
    fn plain_text_loads_as_numbered_lines() {
        let file = TempFile::new("txt", b"first\nsecond\n");
        assert!(matches!(
            load_file(&file.0),
            LoadedFile::Text(lines) if lines == vec!["first", "second"]
        ));
    }

    #[test]
    fn nul_bytes_are_reported_as_binary() {
        let file = TempFile::new("dat", b"header\0payload");
        assert!(matches!(load_file(&file.0), LoadedFile::Binary));
    }

    #[test]
    fn files_over_the_limit_are_refused() {
        let file = TempFile::new("txt", &vec![b'x'; MAX_FILE_BYTES as usize + 1]);
        assert!(matches!(
            load_file(&file.0),
            LoadedFile::TooLarge { size } if size == MAX_FILE_BYTES + 1
        ));
    }

    #[test]
    fn a_deleted_file_becomes_a_visible_error() {
        let file = TempFile::new("txt", b"will disappear");
        std::fs::remove_file(&file.0).expect("delete temporary file");
        assert!(matches!(load_file(&file.0), LoadedFile::Unreadable(error) if !error.is_empty()));
    }
}
