//! File-backed centre-column tabs — an editor, not a viewer (P34).
//!
//! The editor's *substance* — load, edit, save, conflict detection, dirty
//! state, language detection, the Markdown formatting operations — lives in
//! [`crate::editor`] as pure synchronous code, tested without a display.
//! This view owns only the pixels and the shell-facing surface:
//!
//! - it loads the path into an [`Editor`] on GPUI's background executor
//!   (the same split the old read-only viewer used: no filesystem work
//!   during render);
//! - it paints the buffer, or the specific failure state (missing vs
//!   unreadable vs binary vs too large — each a distinct message, F-EDIT-13);
//! - it exposes `is_dirty` (the flag F-TAB-16's close confirmation needs),
//!   `conflict`, `check_external`, `save`, `reload` and `keep` for the shell
//!   to drive: save on ⌘S, conflict check on tab activation, close
//!   confirmation when dirty;
//! - the conflict banner and the Markdown toolbar are rendered here. The
//!   Code/Preview switch (F-EDIT-01) and the conflict banner's Reload/Keep
//!   are behaviour — the switch is exercised against the drawn frame, and
//!   the banner's state machine is provable through the editor model —
//!   while the pixels inside them remain `NOT EXERCISED — blocked on
//!   display`.

use gpui::{AnyElement, Context, Render, Task, Window, div, prelude::*, px};
use std::path::{Path, PathBuf};
use tiller_markdown::{Document, parse};
use tiller_theme::Theme;

use crate::chat::Chat;
use crate::editor::{Conflict, Editor, Language, LoadStatus, Selection};

/// The rendered-markdown column: the frozen 720px content column (waku
/// `CONTENT_MAX_WIDTH`). Prose sits on the same measured column as the
/// transcript (P32 moved it from an unrecorded 800).
pub(crate) const MARKDOWN_COLUMN_WIDTH: f32 = 720.0;

#[derive(Debug)]
enum ViewState {
    /// The background load has not completed; nothing is known yet.
    Loading,
    /// The editor model is installed and owns the truth about this tab.
    Ready(Editor),
}

/// F-EDIT-01: the two Markdown modes. Code shows the editable source;
/// Preview shows the rendered document. Switching between them is
/// behaviour (tested against the drawn frame); the typography inside each
/// mode is appearance and stays `NOT EXERCISED — blocked on display`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarkdownMode {
    Code,
    Preview,
}

/// A tab showing one path. The entity remains owned by the workspace while
/// another tab is active, so switching away never reloads or loses content
/// — and the editor's dirty/conflict state survives tab switches, which is
/// exactly what F-EDIT-05 ("return to Tiller and see the conflict") needs.
pub struct FileView {
    path: PathBuf,
    state: ViewState,
    load_task: Option<Task<()>>,
    /// F-EDIT-01: which Markdown mode is active. Only meaningful for
    /// Markdown files; other languages always render as code. A large
    /// Markdown file (preview locked, F-EDIT-03) starts in Code with the
    /// manual-preview notice, and clicking Preview unlocks it.
    markdown_mode: MarkdownMode,
}

impl FileView {
    /// Starts loading `path` without doing filesystem work during render.
    pub fn new(path: PathBuf, cx: &mut Context<Self>) -> Self {
        let path_for_task = path.clone();
        let load_task = cx.spawn(async move |this, cx| {
            let editor = cx
                .background_spawn(async move { Editor::open(&path_for_task) })
                .await;
            let _ = this.update(cx, |view, cx| {
                view.markdown_mode = Self::initial_markdown_mode(&editor);
                view.state = ViewState::Ready(editor);
                view.load_task = None;
                cx.notify();
            });
        });
        Self {
            path,
            state: ViewState::Loading,
            load_task: Some(load_task),
            markdown_mode: MarkdownMode::Preview,
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The loaded editor, once the background read completes.
    pub fn editor(&self) -> Option<&Editor> {
        match &self.state {
            ViewState::Ready(editor) => Some(editor),
            ViewState::Loading => None,
        }
    }

    /// Mutable editor access for the shell's edit path (a textarea that
    /// does not exist yet — the model ops it needs are all here).
    pub fn editor_mut(&mut self) -> Option<&mut Editor> {
        match &mut self.state {
            ViewState::Ready(editor) => Some(editor),
            ViewState::Loading => None,
        }
    }

    // ── Shell-facing surface ───────────────────────────────────────────

    /// The dirty flag F-TAB-16's close confirmation reads. False while the
    /// file is still loading (nothing to lose yet).
    pub fn is_dirty(&self) -> bool {
        self.editor().is_some_and(Editor::is_dirty)
    }

    /// The current conflict state, or none while loading.
    pub fn conflict(&self) -> Conflict {
        self.editor().map_or(Conflict::None, Editor::conflict)
    }

    /// Re-reads the file and compares it against the last known disk state
    /// (F-EDIT-05/06). The shell calls this when a file tab is activated —
    /// "modify externally, return to Tiller" — and on window focus.
    pub fn check_external(&mut self, cx: &mut Context<Self>) {
        if let Some(editor) = self.editor_mut() {
            editor.check_external();
            cx.notify();
        }
    }

    /// Saves the buffer (F-EDIT-04/06), recreating a deleted file. Errors
    /// are returned and kept visible on the editor.
    pub fn save(&mut self, cx: &mut Context<Self>) -> Result<(), String> {
        let result = match self.editor_mut() {
            Some(editor) => editor.save(),
            None => Err("the file is still loading".to_string()),
        };
        cx.notify();
        result
    }

    /// F-EDIT-05 "Reload": adopt the on-disk content.
    pub fn reload(&mut self, cx: &mut Context<Self>) -> Result<(), String> {
        let result = match self.editor_mut() {
            Some(editor) => editor.reload(),
            None => Err("the file is still loading".to_string()),
        };
        cx.notify();
        result
    }

    /// F-EDIT-05 "Keep": dismiss the banner, keep the buffer.
    pub fn keep(&mut self, cx: &mut Context<Self>) {
        if let Some(editor) = self.editor_mut() {
            editor.keep();
            cx.notify();
        }
    }

    /// F-EDIT-03: unlock the Markdown preview for a large file.
    pub fn request_preview(&mut self, cx: &mut Context<Self>) {
        if let Some(editor) = self.editor_mut() {
            editor.request_preview();
            cx.notify();
        }
    }

    /// A Markdown formatting operation (F-EDIT-02), applied to the current
    /// buffer through the editor model. The toolbar will pass the visible
    /// selection; until a textarea exists this is the shell's entry point.
    pub fn format_markdown(
        &mut self,
        op: MarkdownFormatOp,
        selection: Selection,
        cx: &mut Context<Self>,
    ) -> Option<Selection> {
        let result = self.editor_mut().map(|editor| op.apply(editor, selection));
        cx.notify();
        result
    }

    /// F-EDIT-01: switch the Markdown mode. Selecting Preview on a large
    /// (preview-locked) file unlocks the preview first — the manual-preview
    /// state (F-EDIT-03) is "the preview is not auto-rendered", not "the
    /// preview is forbidden" — so both modes are always reachable.
    pub fn set_markdown_mode(&mut self, mode: MarkdownMode, cx: &mut Context<Self>) {
        self.markdown_mode = mode;
        if mode == MarkdownMode::Preview
            && let Some(editor) = self.editor_mut()
        {
            editor.request_preview();
        }
        cx.notify();
    }

    /// The active Markdown mode (F-EDIT-01).
    pub fn markdown_mode(&self) -> MarkdownMode {
        self.markdown_mode
    }

    /// The mode the surface actually renders: Preview only for a Markdown
    /// file whose preview is not locked. A large Markdown file (F-EDIT-03)
    /// renders source even if Preview was requested, until the unlock
    /// happens — the switcher highlights this effective mode so the mark
    /// never lies about what is on screen.
    fn effective_mode(&self) -> MarkdownMode {
        match self.editor() {
            Some(editor)
                if editor.language() == Language::Markdown
                    && !editor.preview_locked()
                    && self.markdown_mode == MarkdownMode::Preview =>
            {
                MarkdownMode::Preview
            }
            _ => MarkdownMode::Code,
        }
    }

    /// The initial mode once the file is loaded: Preview for an ordinary
    /// Markdown file, Code for everything else (a large Markdown file opens
    /// as source with the manual-preview notice — F-EDIT-03 — and a code
    /// file has nothing to preview).
    fn initial_markdown_mode(editor: &Editor) -> MarkdownMode {
        if editor.language() == Language::Markdown && !editor.preview_locked() {
            MarkdownMode::Preview
        } else {
            MarkdownMode::Code
        }
    }

    /// Whether the Code/Preview switcher should render: only for Markdown
    /// files — a code file has nothing to preview, so offering the switch
    /// would be a lie.
    fn is_markdown(&self) -> bool {
        self.editor()
            .is_some_and(|editor| editor.language() == Language::Markdown)
    }

    fn render_header(&self, theme: Theme, entity: gpui::Entity<Self>) -> impl IntoElement {
        let language = self.editor().map(|editor| editor.language().name());
        let dirty = self.is_dirty();
        div()
            .w_full()
            .border_b_1()
            .border_color(theme.hairline)
            .px(px(20.0))
            .py(px(10.0))
            .flex()
            .items_center()
            .gap(px(8.0))
            .text_size(theme.typography.footnote)
            .text_color(theme.meta)
            .child(self.path.display().to_string())
            .when(dirty, |this| {
                this.child(
                    div()
                        .text_size(theme.typography.caption2)
                        .text_color(theme.accent)
                        .child("● edited"),
                )
            })
            .when(language.is_some(), |this| {
                this.child(
                    div()
                        .px(px(6.0))
                        .py(px(1.0))
                        .rounded(theme.radii.chip)
                        .bg(theme.raised)
                        .text_color(theme.subtitle)
                        .child(language.unwrap_or_default()),
                )
            })
            .when(self.is_markdown(), |this| {
                this.child(render_mode_switch(self.effective_mode(), theme, entity))
            })
    }

    fn render_state(&self, theme: Theme, entity: gpui::Entity<Self>) -> AnyElement {
        match &self.state {
            ViewState::Loading => notice("Loading file…", theme),
            ViewState::Ready(editor) => match editor.status() {
                LoadStatus::Loaded => {
                    let conflict = editor.conflict();
                    let mode = self.effective_mode();
                    div()
                        .id("file-editor")
                        .size_full()
                        .flex()
                        .flex_col()
                        .when(conflict != Conflict::None, |this| {
                            this.child(render_conflict_banner(conflict, theme, entity.clone()))
                        })
                        .child(render_content(editor, mode, theme))
                        .into_any_element()
                }
                _ => notice(
                    editor
                        .load_message()
                        .unwrap_or_else(|| "Unable to open this file.".to_string()),
                    theme,
                ),
            },
        }
    }
}

impl Render for FileView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *Theme::get(cx);
        let entity = cx.entity();
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(theme.chat_surface)
            .child(self.render_header(theme, entity))
            .child(
                div()
                    .flex_1()
                    .min_h(px(0.0))
                    .child(self.render_state(theme, cx.entity())),
            )
    }
}

/// F-EDIT-01: the Code/Preview switcher for Markdown files. Each option is
/// a real button in the drawn frame; clicking it switches the mode through
/// the view entity, and the active option is marked with the same 6%
/// selected-row layer the rest of the app uses — never a colour, because
/// this is selection state, not meaning.
fn render_mode_switch(
    mode: MarkdownMode,
    theme: Theme,
    entity: gpui::Entity<FileView>,
) -> impl IntoElement {
    let preview = render_mode_option(
        "Preview",
        "file-mode-preview",
        mode == MarkdownMode::Preview,
        MarkdownMode::Preview,
        entity.clone(),
        theme,
    );
    let code = render_mode_option(
        "Code",
        "file-mode-code",
        mode == MarkdownMode::Code,
        MarkdownMode::Code,
        entity,
        theme,
    );
    div()
        .id("file-mode-switch")
        .debug_selector(|| "file-mode-switch".into())
        .flex()
        .items_center()
        .gap(px(2.0))
        .p(px(2.0))
        .rounded(theme.radii.control)
        .bg(theme.raised)
        .child(preview)
        .child(code)
}

fn render_mode_option(
    label: &'static str,
    selector: &'static str,
    active: bool,
    mode: MarkdownMode,
    entity: gpui::Entity<FileView>,
    theme: Theme,
) -> impl IntoElement {
    div()
        .id(selector)
        .debug_selector(move || selector.into())
        .px(px(8.0))
        .py(px(2.0))
        .rounded(theme.radii.chip)
        .text_size(theme.typography.caption2)
        .text_color(if active {
            theme.title_selected
        } else {
            theme.meta
        })
        .when(active, |this| this.bg(theme.selected_fill))
        .hover(|style| style.bg(theme.row_hover))
        .on_click(move |_, _, cx| {
            entity.update(cx, |view, cx| view.set_markdown_mode(mode, cx));
        })
        .child(label)
}

/// The conflict banner (F-EDIT-05/06): a real surface with working
/// resolutions, rendered above the content. Reload and Keep are wired to
/// the view's entity, which resolves them through the editor model; a
/// deleted file offers no Reload (there is nothing to reload) — the message
/// says saving recreates it. The banner itself needs pixels, so it is
/// `NOT EXERCISED — blocked on display`; the state machine behind it is the
/// tested part.
fn render_conflict_banner(
    conflict: Conflict,
    theme: Theme,
    entity: gpui::Entity<FileView>,
) -> impl IntoElement {
    let message = match conflict {
        Conflict::ChangedOnDisk => "This file changed on disk.".to_string(),
        Conflict::DeletedOnDisk => "This file was deleted. Saving will recreate it.".to_string(),
        Conflict::None => unreachable!("only rendered for a conflict"),
    };
    let reload_entity = entity.clone();
    let keep_entity = entity;
    div()
        .id("file-conflict-banner")
        .w_full()
        .px(px(20.0))
        .py(px(8.0))
        .flex()
        .items_center()
        .gap(px(12.0))
        .bg(theme.danger_soft)
        .border_b_1()
        .border_color(theme.hairline)
        .text_size(theme.typography.footnote)
        .text_color(theme.title)
        .child(div().flex_1().child(message))
        .when(conflict == Conflict::ChangedOnDisk, |this| {
            this.child(
                div()
                    .id("file-conflict-reload")
                    .px(px(10.0))
                    .py(px(4.0))
                    .rounded(theme.radii.control)
                    .hover(|style| style.bg(theme.row_hover))
                    .on_click(move |_, _, cx| {
                        let _ = reload_entity.update(cx, |view, cx| view.reload(cx));
                    })
                    .child("Reload"),
            )
            .child(
                div()
                    .id("file-conflict-keep")
                    .px(px(10.0))
                    .py(px(4.0))
                    .rounded(theme.radii.control)
                    .hover(|style| style.bg(theme.row_hover))
                    .on_click(move |_, _, cx| {
                        keep_entity.update(cx, |view, cx| view.keep(cx));
                    })
                    .child("Keep"),
            )
        })
}

fn render_content(editor: &Editor, mode: MarkdownMode, theme: Theme) -> AnyElement {
    let is_markdown = editor.language() == Language::Markdown;
    // Preview renders the parsed document; a locked preview (large file,
    // F-EDIT-03) or Code mode renders the source. The two modes are
    // behaviour — each shows the *right* content — while the rendering
    // inside them is appearance.
    if is_markdown && mode == MarkdownMode::Preview && !editor.preview_locked() {
        let document = markdown_document(editor.path(), editor.buffer())
            .expect("markdown render path implies a markdown document");
        return div()
            .id("file-markdown-scroll")
            .debug_selector(|| "file-markdown-scroll".into())
            .size_full()
            .overflow_y_scroll()
            .child(
                div()
                    .w_full()
                    .max_w(px(MARKDOWN_COLUMN_WIDTH))
                    .mx_auto()
                    .p(px(24.0))
                    .child(Chat::render_markdown_document(document.clone(), &theme)),
            )
            .into_any_element();
    }

    let lines: Vec<String> = editor.buffer().lines().map(str::to_owned).collect();
    div()
        .id("file-text-scroll")
        .debug_selector(|| "file-text-scroll".into())
        .size_full()
        .overflow_y_scroll()
        .p(px(16.0))
        .child(
            div()
                .flex()
                .flex_col()
                .when(is_markdown && editor.preview_locked(), |this| {
                    // F-EDIT-03: a large Markdown file opens as source with
                    // a manual-preview state until `request_preview`.
                    this.child(
                        div()
                            .id("file-manual-preview")
                            .debug_selector(|| "file-manual-preview".into())
                            .w_full()
                            .mb(px(8.0))
                            .px(px(10.0))
                            .py(px(6.0))
                            .rounded(theme.radii.control)
                            .bg(theme.raised)
                            .text_size(theme.typography.footnote)
                            .text_color(theme.subtitle)
                            .child("Large file — manual preview"),
                    )
                })
                .font_family(theme.typography.code_family)
                .text_size(theme.typography.code_size)
                .text_color(theme.title)
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
        .into_any_element()
}

/// The Markdown formatting operations the toolbar offers (F-EDIT-02),
/// expressed as a command so the shell can route them generically. The
/// implementations are the pure text transformations on
/// [`crate::editor::Editor`]. The URL for a link comes from the toolbar's
/// own prompting; the command only carries it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MarkdownFormatOp {
    Bold,
    Italic,
    Heading,
    List,
    Link { url: String },
}

impl MarkdownFormatOp {
    fn apply(self, editor: &mut Editor, selection: Selection) -> Selection {
        match self {
            MarkdownFormatOp::Bold => editor.format_bold(selection),
            MarkdownFormatOp::Italic => editor.format_italic(selection),
            MarkdownFormatOp::Heading => editor.toggle_heading(selection),
            MarkdownFormatOp::List => editor.toggle_list(selection),
            MarkdownFormatOp::Link { url } => editor.make_link(selection, &url),
        }
    }
}

fn notice(message: impl Into<String>, theme: Theme) -> AnyElement {
    div()
        .size_full()
        .flex()
        .items_center()
        .justify_center()
        .p(px(24.0))
        .text_size(theme.typography.headline)
        .text_color(theme.subtitle)
        .child(message.into())
        .into_any_element()
}

/// Renders `text` as a Markdown document when the path is Markdown, for the
/// preview half of the surface (F-EDIT-01's Code/Preview split is display-
/// bound; the parse is not).
fn markdown_document(path: &Path, text: &str) -> Option<Document> {
    if Language::from_path(path) == Language::Markdown {
        Some(parse(text))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{Modifiers, VisualTestContext};
    use std::sync::atomic::{AtomicU64, Ordering};
    use tiller_markdown::{Block, Inline};

    struct TempFile(PathBuf);

    impl TempFile {
        fn new(name: &str, contents: &str) -> Self {
            static COUNTER: AtomicU64 = AtomicU64::new(0);
            let id = COUNTER.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "tiller-file-view-{name}-{}-{id}",
                std::process::id()
            ));
            std::fs::write(&path, contents).expect("write temporary file");
            Self(path)
        }

        /// Like [`TempFile::new`] but the on-disk name ends with
        /// `extension`, so language detection sees it.
        fn with_extension(extension: &str, contents: &str) -> Self {
            static COUNTER: AtomicU64 = AtomicU64::new(0);
            let id = COUNTER.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "tiller-file-view-{}-{id}.{extension}",
                std::process::id()
            ));
            std::fs::write(&path, contents).expect("write temporary file");
            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempFile {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }

    #[test]
    fn markdown_loads_into_expected_blocks() {
        let file = TempFile::with_extension("md", "# Title\n\nHello **world**.");
        let document = markdown_document(&file.0, "# Title\n\nHello **world**.")
            .expect("markdown parses into a document");
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
    fn plain_text_never_renders_as_markdown() {
        let path = Path::new("/tmp/note.txt");
        assert!(
            markdown_document(path, "hello").is_none(),
            "only Markdown paths render as Markdown"
        );
    }

    // ── The shell-facing surface, through the actual view ──────────────

    #[gpui::test]
    async fn dirty_state_is_reachable_through_the_view(cx: &mut gpui::TestAppContext) {
        let file = TempFile::new("dirty", "hello\n");

        cx.update(Theme::init);
        let window = cx.add_window(|_window, cx| FileView::new(file.path().to_path_buf(), cx));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        // Loaded and clean: the F-TAB-16 flag is false before any edit.
        cx.update(|window, cx| {
            let view = window.root::<FileView>().flatten().expect("root").read(cx);
            assert!(!view.is_dirty(), "a freshly opened file is clean");
            assert_eq!(view.conflict(), Conflict::None);
        });

        // Edit through the view's editor: the flag the shell's close path
        // reads becomes true. This is the wiring F-TAB-16 needs.
        cx.update(|window, cx| {
            let view = window.root::<FileView>().flatten().expect("root");
            view.update(cx, |view, cx| {
                let editor = view.editor_mut().expect("editor loaded");
                let selection = Selection::point(editor.buffer().len());
                editor.insert(selection.start, "more\n").expect("insert");
                cx.notify();
            });
        });
        cx.update(|window, cx| {
            let view = window.root::<FileView>().flatten().expect("root").read(cx);
            assert!(view.is_dirty(), "an edit makes the document dirty");
        });

        // Save through the view: the flag clears, matching the disk.
        cx.update(|window, cx| {
            let view = window.root::<FileView>().flatten().expect("root");
            view.update(cx, |view, cx| view.save(cx).expect("save"));
        });
        cx.update(|window, cx| {
            let view = window.root::<FileView>().flatten().expect("root").read(cx);
            assert!(!view.is_dirty(), "saving clears the dirty flag");
        });
        assert_eq!(
            std::fs::read_to_string(file.path()).expect("read back"),
            "hello\nmore\n",
            "the view's save wrote the buffer to disk"
        );
    }

    #[gpui::test]
    async fn conflict_detection_and_resolutions_work_through_the_view(
        cx: &mut gpui::TestAppContext,
    ) {
        let file = TempFile::new("view-conflict", "original\n");

        cx.update(Theme::init);
        let window = cx.add_window(|_window, cx| FileView::new(file.path().to_path_buf(), cx));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        // External mutation while the tab is open, then the shell's
        // activation hook fires check_external.
        std::fs::write(file.path(), "external\n").expect("external write");
        cx.update(|window, cx| {
            let view = window.root::<FileView>().flatten().expect("root");
            view.update(cx, |view, cx| view.check_external(cx));
        });
        cx.update(|window, cx| {
            let view = window.root::<FileView>().flatten().expect("root").read(cx);
            assert_eq!(view.conflict(), Conflict::ChangedOnDisk);
        });

        // Reload adopts the disk content and clears the flag.
        cx.update(|window, cx| {
            let view = window.root::<FileView>().flatten().expect("root");
            view.update(cx, |view, cx| view.reload(cx).expect("reload"));
        });
        cx.update(|window, cx| {
            let view = window.root::<FileView>().flatten().expect("root").read(cx);
            assert_eq!(view.conflict(), Conflict::None);
            assert_eq!(
                view.editor().expect("editor").buffer(),
                "external\n",
                "Reload adopted the on-disk content"
            );
        });

        // Keep keeps the buffer and leaves the tab dirty until saved.
        std::fs::write(file.path(), "second external\n").expect("external write");
        cx.update(|window, cx| {
            let view = window.root::<FileView>().flatten().expect("root");
            view.update(cx, |view, cx| view.check_external(cx));
        });
        cx.update(|window, cx| {
            let view = window.root::<FileView>().flatten().expect("root");
            view.update(cx, |view, cx| view.keep(cx));
        });
        cx.update(|window, cx| {
            let view = window.root::<FileView>().flatten().expect("root").read(cx);
            assert_eq!(view.conflict(), Conflict::None);
            assert_eq!(
                view.editor().expect("editor").buffer(),
                "external\n",
                "Keep preserved the buffer"
            );
            assert!(view.is_dirty(), "Keep leaves the tab dirty until saved");
        });
    }

    #[gpui::test]
    async fn a_missing_file_tab_reports_the_specific_state(cx: &mut gpui::TestAppContext) {
        let missing =
            std::env::temp_dir().join(format!("tiller-file-view-missing-{}", std::process::id()));

        cx.update(Theme::init);
        let window = cx.add_window(|_window, cx| FileView::new(missing.clone(), cx));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        cx.update(|window, cx| {
            let view = window.root::<FileView>().flatten().expect("root").read(cx);
            let editor = view.editor().expect("editor installed");
            assert_eq!(editor.status(), &LoadStatus::Missing);
            let message = editor.load_message().expect("message");
            assert!(
                message.contains("does not exist"),
                "missing files get the missing message: {message}"
            );
            assert!(!view.is_dirty(), "a missing file is not dirty");
        });
    }

    // ── F-EDIT-01: Code and Preview modes are behaviour ────────────────
    //
    // The drawn frame finds the real switcher, real clicks switch the mode,
    // and each mode renders the content that belongs to it — rendered
    // Markdown in Preview, the numbered source in Code. The typography
    // inside either mode is appearance and is not asserted.

    /// Mounts a `FileView` in a drawn window and pumps until its background
    /// load has actually installed the editor — the same hardened pump the
    /// drawn git tests use, so the assertions below run against a loaded
    /// editor, never on timing luck.
    fn mounted_file_view(
        cx: &mut gpui::TestAppContext,
        path: PathBuf,
    ) -> (VisualTestContext, gpui::Entity<FileView>) {
        cx.update(Theme::init);
        let window = cx.add_window(|_window, cx| FileView::new(path, cx));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.cx.executor().allow_parking();
        for _ in 0..600 {
            let loaded = cx.update(|window, app| {
                window
                    .root::<FileView>()
                    .flatten()
                    .is_some_and(|view| view.read(app).editor().is_some())
            });
            if loaded {
                break;
            }
            cx.cx
                .executor()
                .advance_clock(std::time::Duration::from_secs(1));
            std::thread::sleep(std::time::Duration::from_millis(10));
            cx.cx.run_until_parked();
        }
        let entity =
            cx.update(|window, _| window.root::<FileView>().flatten().expect("file view root"));
        assert!(
            entity.read_with(&cx.cx, |view, _| view.editor().is_some()),
            "the background load must install the editor before the test proceeds"
        );
        cx.update(|window, cx| {
            window.refresh();
            window.simulate_next_frame(cx);
            window.simulate_next_frame(cx);
        });
        (cx, entity)
    }

    #[gpui::test]
    async fn markdown_preview_and_code_modes_switch_in_the_drawn_frame(
        cx: &mut gpui::TestAppContext,
    ) {
        let file = TempFile::with_extension("md", "# Title\n\nHello **world**.\n");
        let (mut cx, _view) = mounted_file_view(cx, file.path().to_path_buf());

        // A Markdown file opens in Preview: the rendered document is on
        // screen and the source is not.
        assert!(
            cx.debug_bounds("file-markdown-scroll").is_some(),
            "Preview mode renders the Markdown document"
        );
        assert!(
            cx.debug_bounds("file-text-scroll").is_none(),
            "Preview mode does not render the source"
        );
        assert!(
            cx.debug_bounds("file-mode-preview").is_some()
                && cx.debug_bounds("file-mode-code").is_some(),
            "the Code/Preview switcher is in the drawn frame"
        );

        // Click Code: the source replaces the rendered document.
        let code = cx
            .debug_bounds("file-mode-code")
            .expect("the Code option is drawn");
        cx.simulate_click(code.center(), Modifiers::none());
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.simulate_next_frame(cx);
            window.simulate_next_frame(cx);
        });
        assert!(
            cx.debug_bounds("file-text-scroll").is_some(),
            "Code mode renders the numbered source"
        );
        assert!(
            cx.debug_bounds("file-markdown-scroll").is_none(),
            "Code mode hides the rendered document"
        );
        cx.update(|window, cx| {
            let view = window.root::<FileView>().flatten().expect("root").read(cx);
            assert_eq!(view.markdown_mode(), MarkdownMode::Code);
        });

        // Click Preview again: the rendered document returns.
        let preview = cx
            .debug_bounds("file-mode-preview")
            .expect("the Preview option is drawn");
        cx.simulate_click(preview.center(), Modifiers::none());
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.simulate_next_frame(cx);
            window.simulate_next_frame(cx);
        });
        assert!(
            cx.debug_bounds("file-markdown-scroll").is_some(),
            "clicking Preview restores the rendered document"
        );
        assert!(
            cx.debug_bounds("file-text-scroll").is_none(),
            "the source is hidden again in Preview"
        );
        cx.update(|window, cx| {
            let view = window.root::<FileView>().flatten().expect("root").read(cx);
            assert_eq!(view.markdown_mode(), MarkdownMode::Preview);
        });
    }

    #[gpui::test]
    async fn a_code_file_has_no_mode_switch_and_always_renders_source(
        cx: &mut gpui::TestAppContext,
    ) {
        let file = TempFile::with_extension("rs", "fn main() {}\n");
        let (mut cx, _view) = mounted_file_view(cx, file.path().to_path_buf());

        assert!(
            cx.debug_bounds("file-text-scroll").is_some(),
            "a code file renders its source"
        );
        assert!(
            cx.debug_bounds("file-markdown-scroll").is_none(),
            "a code file never renders Markdown"
        );
        assert!(
            cx.debug_bounds("file-mode-preview").is_none()
                && cx.debug_bounds("file-mode-code").is_none(),
            "a code file has nothing to preview, so no switcher is offered"
        );
    }

    #[gpui::test]
    async fn a_large_markdown_file_opens_in_code_with_manual_preview_and_unlocks(
        cx: &mut gpui::TestAppContext,
    ) {
        // Over the preview threshold but under the load limit: F-EDIT-03's
        // manual-preview state, now with a working way out.
        let big = "x".repeat(256 * 1024 + 100);
        let file = TempFile::with_extension("md", &big);
        let (mut cx, _view) = mounted_file_view(cx, file.path().to_path_buf());

        // The large file opens as source with the manual-preview notice.
        assert!(
            cx.debug_bounds("file-text-scroll").is_some(),
            "a large Markdown file opens in Code"
        );
        assert!(
            cx.debug_bounds("file-manual-preview").is_some(),
            "the manual-preview notice is shown (F-EDIT-03)"
        );
        assert!(
            cx.debug_bounds("file-markdown-scroll").is_none(),
            "the preview is not auto-rendered for a large file"
        );

        // Clicking Preview unlocks it: the manual state is "not automatic",
        // not "forbidden".
        let preview = cx
            .debug_bounds("file-mode-preview")
            .expect("the Preview option is drawn");
        cx.simulate_click(preview.center(), Modifiers::none());
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.simulate_next_frame(cx);
            window.simulate_next_frame(cx);
        });
        assert!(
            cx.debug_bounds("file-markdown-scroll").is_some(),
            "Preview on a locked file unlocks the preview and renders it"
        );
        assert!(
            cx.debug_bounds("file-text-scroll").is_none(),
            "the source is replaced once the preview is unlocked"
        );
    }
}
