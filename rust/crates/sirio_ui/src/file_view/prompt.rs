//! Prompt mode: a file tab over a saved prompt (spec §3,
//! `docs/superpowers/specs/2026-11-01-prompt-list-design.md`).
//!
//! A prompt is one file in the prompts folder, holding exactly the text Copy
//! puts on the clipboard; its title lives in the folder's index. A `FileView`
//! whose file is in that folder is in prompt mode, whichever route opened it.
//! In prompt mode the view:
//!
//! - draws a title field where the breadcrumb was;
//! - stays in Code: no Preview, no Markdown row;
//! - saves the text and the title by itself, `AUTOSAVE_DELAY` after the last
//!   change to either, and on Ctrl+S, on a switch to another worktree, when
//!   its tab or its leaf in a split closes, and when the app quits;
//! - after a failed save, reads as unsaved until a write lands
//!   (`prompt_unsaved`), so closing its tab asks first (the pane banner of
//!   Ctrl+Alt+W asks about the pane only);
//! - never writes a file that is no longer on disk, so a deleted prompt stays
//!   deleted, and shows the missing-file state instead;
//! - on an edit made elsewhere, keeps what is still to save, with no banner,
//!   and lets its next save overwrite the file (the last write wins).
//!
//! A plain file tab has none of this.

use std::cell::Cell;
use std::path::Path;
use std::rc::Rc;
use std::time::Duration;

use ely_gpui_component::forms::{Input, InputEvent, TextInput};
use gpui::{
    AnyElement, App, Context, Entity, Focusable, MouseButton, Subscription, Task, WeakEntity,
    Window, div, prelude::*, px,
};
use sirio_theme::Theme;

use super::{FileView, FileViewEvent};
use crate::editor::{Conflict, LoadStatus};
use crate::ely_ui::new_input;
use crate::prompts::{self, LivePrompt};

/// How long typing must pause before the prompt saves itself.
const AUTOSAVE_DELAY: Duration = Duration::from_millis(400);

/// What a prompt with an empty title is called: its tab, and its title
/// field's placeholder.
const UNTITLED: &str = "Untitled prompt";

/// The prompt-mode half of a `FileView`; `None` on a plain file tab.
pub(super) struct PromptMode {
    /// The title as the index holds it: read when the view opened, then moved
    /// by every retitle that succeeded. The field differing from it is a title
    /// still to save.
    saved_title: String,
    /// The title field, made at the first frame that draws the file (Ely's
    /// input needs the window `new` lacks).
    title: Option<Entity<TextInput>>,
    _title_subscription: Option<Subscription>,
    /// Bumped by every change to the text or the title; a timer that fires
    /// under an older number saves nothing.
    generation: u64,
    /// The pending autosave. Replacing it cancels the one before.
    autosave: Option<Task<()>>,
    /// Set once the prompt is deleted (`LivePrompt::forget`): from then on
    /// nothing saves, so a pending save cannot bring the file back. Shared
    /// with the registry's handle, which sets it without touching this view:
    /// a delete can run while this view is being updated.
    forgotten: Rc<Cell<bool>>,
    /// Whether the keyboard still has to be placed, at the next frame that
    /// draws the title field. Only an explicit open asks for it
    /// (`focus_prompt_on_open`): a tab restored at launch or shown on a
    /// worktree switch takes no focus.
    focus_pending: bool,
    /// Whether the last flush that had something to write failed, for the
    /// text or the title. The next flush that writes clears it. With typing
    /// still not on disk it makes the tab dirty, so its close asks.
    save_failed: bool,
}

/// The registry's handle on an open prompt view.
struct LiveView {
    view: WeakEntity<FileView>,
    forgotten: Rc<Cell<bool>>,
}

impl LivePrompt for LiveView {
    fn alive(&self, _cx: &App) -> bool {
        self.view.upgrade().is_some()
    }

    /// Only the text the view has loaded: a view still reading its file, or
    /// one that could not, answers `None`, and Copy reads the file itself.
    fn text(&self, cx: &App) -> Option<String> {
        self.view.upgrade()?.read(cx).prompt_text()
    }

    fn flush(&self, cx: &mut App) {
        if let Some(view) = self.view.upgrade() {
            view.update(cx, |view, cx| view.flush_prompt(cx));
        }
    }

    fn forget(&self, _cx: &mut App) {
        self.forgotten.set(true);
    }
}

/// The sentence a tab shows for a file that is not there, the one `Editor`
/// gives a file missing at open.
fn missing_message(path: &Path) -> String {
    format!("This file does not exist: {}", path.display())
}

impl FileView {
    /// Turns prompt mode on when the file is in the prompts folder. Called by
    /// `new`, before the file is read.
    pub(super) fn detect_prompt(&mut self, cx: &mut Context<Self>) {
        if !prompts::is_prompt_path(&self.path, cx) {
            return;
        }
        let saved_title = prompts::store(cx)
            .map(|store| store.title(&self.path))
            .unwrap_or_default();
        let forgotten = Rc::new(Cell::new(false));
        let live = LiveView {
            view: cx.weak_entity(),
            forgotten: forgotten.clone(),
        };
        // Registered once this view exists: `new` runs before it does, and
        // the registry drops an entry whose view cannot be reached.
        let path = self.path.clone();
        cx.defer(move |cx| prompts::register_live(path, Box::new(live), cx));
        self.prompt = Some(PromptMode {
            saved_title,
            title: None,
            _title_subscription: None,
            generation: 0,
            autosave: None,
            forgotten,
            focus_pending: false,
            save_failed: false,
        });
    }

    /// The user opened this prompt (Edit, a card, New): at the next frame
    /// that draws it, the keyboard goes to the title when it is empty, else
    /// to the text. Also for a tab that was already open.
    pub fn focus_prompt_on_open(&mut self, cx: &mut Context<Self>) {
        if let Some(prompt) = self.prompt.as_mut() {
            prompt.focus_pending = true;
            cx.notify();
        }
    }

    /// Whether this tab is a saved prompt.
    pub fn is_prompt(&self) -> bool {
        self.prompt.is_some()
    }

    /// The prompt's title for its tab, `Untitled prompt` when it has none;
    /// `None` for a plain file tab. It follows the field as it is typed.
    pub fn prompt_title(&self, cx: &App) -> Option<String> {
        self.prompt.as_ref()?;
        let title = self.prompt_title_text(cx);
        Some(if title.is_empty() {
            UNTITLED.to_owned()
        } else {
            title
        })
    }

    /// The title as the field holds it, or as the index did before the field
    /// was made.
    fn prompt_title_text(&self, cx: &App) -> String {
        let Some(prompt) = &self.prompt else {
            return String::new();
        };
        match &prompt.title {
            Some(title) => title.read(cx).text().to_owned(),
            None => prompt.saved_title.clone(),
        }
    }

    /// The file's text as a save of this buffer writes it, with the file's own
    /// line ending: what Copy hands over. `None` until the file is loaded.
    fn prompt_text(&self) -> Option<String> {
        let editor = self.editor()?;
        (editor.status() == &LoadStatus::Loaded)
            .then(|| editor.line_ending().apply(editor.buffer()))
    }

    /// Whether this prompt was deleted, by Sirio (`forget`) or, as the last
    /// disk check found, by another program. The tab then shows the
    /// missing-file state.
    pub(super) fn prompt_deleted(&self) -> bool {
        self.prompt.as_ref().is_some_and(|prompt| {
            prompt.forgotten.get() || self.conflict() == Conflict::DeletedOnDisk
        })
    }

    /// Why a save of this prompt must write nothing: it was deleted, and
    /// writing would bring it back. `None` for a plain file tab.
    pub(super) fn prompt_save_refusal(&self) -> Option<&'static str> {
        let prompt = self.prompt.as_ref()?;
        (prompt.forgotten.get() || matches!(self.path.try_exists(), Ok(false)))
            .then_some("the prompt was deleted")
    }

    /// Whether closing this prompt would lose typing: its last save failed
    /// and the text or the title is still not on disk. A deleted prompt never
    /// is.
    pub fn prompt_unsaved(&self, cx: &App) -> bool {
        self.prompt.as_ref().is_some_and(|prompt| prompt.save_failed)
            && !self.prompt_deleted()
            && self.prompt_pending(cx)
    }

    /// Whether something is still to save: text the file does not hold yet,
    /// or a title the index does not.
    fn prompt_pending(&self, cx: &App) -> bool {
        self.is_dirty()
            || self
                .prompt
                .as_ref()
                .is_some_and(|prompt| self.prompt_title_text(cx) != prompt.saved_title)
    }

    /// Restarts the autosave timer after a change to the text or the title.
    pub(super) fn schedule_prompt_save(&mut self, cx: &mut Context<Self>) {
        let Some(prompt) = self.prompt.as_mut() else {
            return;
        };
        if prompt.forgotten.get() {
            return;
        }
        prompt.generation = prompt.generation.wrapping_add(1);
        let generation = prompt.generation;
        prompt.autosave = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(AUTOSAVE_DELAY).await;
            let _ = this.update(cx, |view, cx| {
                if view
                    .prompt
                    .as_ref()
                    .is_some_and(|prompt| prompt.generation == generation)
                {
                    view.flush_prompt(cx);
                }
            });
        }));
    }

    /// Writes the pending text and title now. A deleted prompt writes
    /// nothing. A failed write keeps what was typed, says so (`Save failed:
    /// …`) and leaves the tab dirty until a later write lands; the next
    /// change tries again. Each write that lands emits `Saved` once and tells
    /// the prompts list.
    pub fn flush_prompt(&mut self, cx: &mut Context<Self>) {
        let Some(prompt) = self.prompt.as_mut() else {
            return;
        };
        prompt.autosave = None;
        if prompt.forgotten.get() {
            return;
        }
        if self.prompt_save_refusal().is_some() {
            // Never recreate: the disk check takes the tab to its
            // missing-file state instead.
            self.check_external(cx);
            return;
        }
        let mut saved_text = false;
        if self.is_dirty() {
            // `save` raises its own failure.
            if self.save(cx).is_err() {
                self.set_prompt_save_failed(true);
                return;
            }
            saved_text = true;
        }
        let title = self.prompt_title_text(cx);
        let mut retitled = false;
        let mut retitle_failed = false;
        if self
            .prompt
            .as_ref()
            .is_some_and(|prompt| prompt.saved_title != title)
        {
            match prompts::retitle(&self.path, &title, cx) {
                Ok(()) => {
                    if let Some(prompt) = self.prompt.as_mut() {
                        prompt.saved_title = title;
                    }
                    retitled = true;
                }
                Err(error) => {
                    self.set_message(format!("Save failed: {error}"), cx);
                    retitle_failed = true;
                }
            }
        }
        self.set_prompt_save_failed(retitle_failed);
        // `save` has emitted `Saved` for the text, and `retitle` has told the
        // list: each is said once.
        if saved_text && !retitled {
            prompts::changed(cx);
        }
        if retitled && !saved_text {
            cx.emit(FileViewEvent::Saved(self.path.clone()));
        }
    }

    fn set_prompt_save_failed(&mut self, failed: bool) {
        if let Some(prompt) = self.prompt.as_mut() {
            prompt.save_failed = failed;
        }
    }

    /// The prompt's tab is closing: write what is pending, then remove the
    /// prompt if it is still untitled and empty, so New followed by a close
    /// leaves nothing behind. A prompt whose write failed is kept.
    pub fn close_prompt(&mut self, cx: &mut Context<Self>) {
        if self.prompt.is_none() {
            return;
        }
        self.flush_prompt(cx);
        // A deleted prompt has nothing left to remove.
        if self.prompt_deleted() || self.prompt_pending(cx) {
            return;
        }
        prompts::remove_if_blank(&self.path, cx);
    }

    /// Makes the title field the first frame the file is drawn.
    pub(super) fn ensure_prompt_title(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.is_loaded() {
            return;
        }
        let Some(prompt) = self.prompt.as_ref() else {
            return;
        };
        if prompt.title.is_some() {
            return;
        }
        let title = new_input(window, cx, &prompt.saved_title, None, UNTITLED);
        let subscription = cx.subscribe_in(
            &title,
            window,
            |view, _title, event: &InputEvent, window, cx| match event {
                // Never write the title back from here: `set_text` emits
                // `Changed` again. The notify lets the tab label follow.
                InputEvent::Changed => {
                    view.schedule_prompt_save(cx);
                    cx.notify();
                }
                InputEvent::Submit => view.focus_prompt_text(window, cx),
                _ => {}
            },
        );
        if let Some(prompt) = self.prompt.as_mut() {
            prompt.title = Some(title);
            prompt._title_subscription = Some(subscription);
        }
    }

    /// The keyboard goes to the prompt's text.
    fn focus_prompt_text(&self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(code) = &self.code_editor {
            code.read(cx).focus_handle(cx).focus(window, cx);
        }
    }

    /// When an open asked for it, the keyboard goes to the title field when
    /// the title is empty (a new prompt), else to the text.
    /// On the next frame: focus reads the last drawn frame, and neither was
    /// in it.
    pub(super) fn settle_prompt_focus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.prompt_deleted() {
            return;
        }
        let Some(code) = self.code_editor.clone() else {
            return;
        };
        let Some(prompt) = self.prompt.as_mut() else {
            return;
        };
        let Some(title) = prompt.title.clone().filter(|_| prompt.focus_pending) else {
            return;
        };
        prompt.focus_pending = false;
        let target = if title.read(cx).text().is_empty() {
            title.read(cx).focus_handle(cx)
        } else {
            code.read(cx).focus_handle(cx)
        };
        window.on_next_frame(move |window, cx| target.focus(window, cx));
    }

    /// The row the breadcrumb leaves to the title field, `None` until the
    /// field is made.
    pub(super) fn render_prompt_title(
        &self,
        theme: Theme,
        entity: Entity<Self>,
    ) -> Option<AnyElement> {
        let title = self.prompt.as_ref()?.title.clone()?;
        Some(
            div()
                .id("file-prompt-title")
                .debug_selector(|| "file-prompt-title".to_owned())
                .w_full()
                .flex_none()
                .px(px(12.0))
                .py(px(6.0))
                .border_b_1()
                .border_color(theme.ely.border)
                .bg(theme.ely.surface)
                // The column hands every left press to the code; a press in
                // the field keeps the keyboard in the field.
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                // Tab goes on to the text, as Enter does: the field binds
                // neither to anything of its own.
                .capture_key_down(move |event, window, cx| {
                    if event.keystroke.key == "tab" && !event.keystroke.modifiers.modified() {
                        cx.stop_propagation();
                        entity.update(cx, |view, cx| view.focus_prompt_text(window, cx));
                    }
                })
                .child(Input::new(&title))
                .into_any_element(),
        )
    }

    /// What a deleted prompt's tab shows instead of its text.
    pub(super) fn render_prompt_missing(&self, theme: Theme) -> AnyElement {
        super::notice(missing_message(&self.path), theme)
    }
}
