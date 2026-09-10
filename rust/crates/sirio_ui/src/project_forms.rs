//! Project-entry surfaces: clone an existing repository or create a folder.

use std::path::{Component, Path, PathBuf};
use std::sync::mpsc::{self, TryRecvError};
use std::time::Duration;

use bezel::ui::input::TextField;
use gpui::{
    App, Context, Entity, EventEmitter, FocusHandle, Focusable, FontWeight, KeyDownEvent, Render,
    Task, Window, div, prelude::*, px,
};
use sirio_git::{GitRemote, clone_repository};

use crate::loading;
use sirio_project::create_project;
use sirio_theme::Theme;

/// The observable states of a clone operation.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum CloneStatus {
    /// No clone is running and the form may be submitted.
    #[default]
    Ready,
    /// Git is receiving objects; the value is between zero and one.
    Running { progress: f64 },
    /// Git failed. The URL remains in the form so the user can retry.
    Failed(String),
    /// The repository was cloned successfully. `truncated` is set when the
    /// clone's streamed diagnostic capture hit its byte cap (see
    /// `GitCommandResult::truncated`); the clone itself still completed, so
    /// this is a notice, not a failure — see `clone_status_line` for how it
    /// is worded and colored.
    Complete {
        destination: PathBuf,
        truncated: bool,
    },
}

/// Pure state and guards for [`CloneForm`].
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CloneFormState {
    url: String,
    status: CloneStatus,
}

impl CloneFormState {
    /// Replaces the URL draft. Editing after a failure returns the form to
    /// the ready state; editing during a clone cannot cancel that clone.
    pub fn set_url(&mut self, url: impl Into<String>) {
        self.url = url.into();
        if !matches!(self.status, CloneStatus::Running { .. }) {
            self.status = CloneStatus::Ready;
        }
    }

    pub fn url(&self) -> &str {
        &self.url
    }

    pub fn status(&self) -> &CloneStatus {
        &self.status
    }

    pub fn error(&self) -> Option<&str> {
        match &self.status {
            CloneStatus::Failed(error) => Some(error),
            _ => None,
        }
    }

    pub fn progress(&self) -> Option<f64> {
        match self.status {
            CloneStatus::Running { progress } => Some(progress),
            _ => None,
        }
    }

    /// Whether a click may start a clone. A failed attempt is intentionally
    /// submit-able so the same form is the retry surface.
    pub fn can_submit(&self) -> bool {
        !self.url.trim().is_empty()
            && !matches!(
                self.status,
                CloneStatus::Running { .. } | CloneStatus::Complete { .. }
            )
    }

    /// Transitions into the in-flight state, refusing a second start.
    pub fn begin(&mut self) -> bool {
        if !self.can_submit() {
            return false;
        }
        self.status = CloneStatus::Running { progress: 0.0 };
        true
    }

    pub fn set_progress(&mut self, progress: f64) {
        if let CloneStatus::Running { progress: current } = &mut self.status {
            *current = progress.clamp(0.0, 1.0);
        }
    }

    pub fn fail(&mut self, error: impl Into<String>) {
        self.status = CloneStatus::Failed(error.into());
    }

    pub fn complete(&mut self, destination: PathBuf, truncated: bool) {
        self.status = CloneStatus::Complete {
            destination,
            truncated,
        };
    }
}

/// The typed result published when [`CloneForm`] finishes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CloneFormEvent {
    /// `truncated` mirrors [`CloneStatus::Complete`]'s field: the clone
    /// succeeded either way. A subscriber that auto-dismisses the form on
    /// this event (as `Sidebar` does) should keep it open when `truncated`
    /// is set, so the status line's notice is actually seen rather than
    /// closed the same frame it appears — see `Sidebar::start_clone_project`.
    Cloned {
        destination: PathBuf,
        truncated: bool,
    },
}

enum CloneWorkerMessage {
    Progress(f64),
    /// `Ok` carries the destination and whether `GitClone::clone`'s
    /// diagnostic capture was truncated — a non-fatal condition the form
    /// surfaces as a notice rather than folding into `Failed`.
    Finished(Result<(PathBuf, bool), String>),
}

/// A GPUI surface for cloning a repository from a URL.
pub struct CloneForm {
    parent: PathBuf,
    state: CloneFormState,
    url_field: Entity<TextField>,
    task: Option<Task<()>>,
}

impl CloneForm {
    /// Creates a form whose destination folders will be placed under
    /// `parent`. The host can change that location before submission.
    pub fn new(parent: PathBuf, cx: &mut Context<Self>) -> Self {
        ensure_theme(cx);
        let url_field = cx.new(|cx| {
            TextField::new(cx).with_placeholder("https://github.com/owner/repository.git")
        });
        cx.observe(&url_field, |form, field, cx| {
            let url = field.read(cx).content().to_string();
            if url != form.state.url() {
                form.state.set_url(url);
                cx.notify();
            }
        })
        .detach();
        Self {
            parent,
            state: CloneFormState::default(),
            url_field,
            task: None,
        }
    }

    pub fn set_parent(&mut self, parent: PathBuf, cx: &mut Context<Self>) {
        self.parent = parent;
        cx.notify();
    }

    pub fn parent(&self) -> &Path {
        &self.parent
    }

    pub fn set_url(&mut self, url: impl Into<String>, cx: &mut Context<Self>) {
        let url = url.into();
        self.state.set_url(url.clone());
        self.url_field
            .update(cx, |field, cx| field.set_content(url, cx));
        cx.notify();
    }

    pub fn url(&self) -> &str {
        self.state.url()
    }

    /// The destination preview derived from the URL, if it has a safe final
    /// folder component.
    pub fn destination(&self) -> Option<PathBuf> {
        destination_for(&self.parent, self.state.url())
    }

    pub fn status(&self) -> &CloneStatus {
        self.state.status()
    }

    /// Starts the clone worker, or does nothing when the state guard rejects
    /// the click. Progress is forwarded from GitClone's existing parser.
    pub fn submit(&mut self, cx: &mut Context<Self>) {
        let url = self.url_field.read(cx).content().to_string();
        if url != self.state.url() {
            self.state.set_url(url);
        }
        if !self.state.begin() {
            return;
        }

        let Some(destination) = self.destination() else {
            self.state
                .fail("the URL does not contain a destination folder name");
            cx.notify();
            return;
        };
        let url = self.state.url().to_owned();
        let (sender, receiver) = mpsc::channel();
        let worker_destination = destination.clone();
        std::thread::spawn(move || {
            let result = clone_repository(&url, &worker_destination, |progress| {
                let _ = sender.send(CloneWorkerMessage::Progress(progress));
            })
            .map(|truncated| (worker_destination, truncated))
            .map_err(|error| error.to_string());
            let _ = sender.send(CloneWorkerMessage::Finished(result));
        });

        self.task = Some(cx.spawn(async move |this, cx| {
            loop {
                loop {
                    let message = match receiver.try_recv() {
                        Ok(message) => message,
                        Err(TryRecvError::Empty) => break,
                        Err(TryRecvError::Disconnected) => {
                            let _ = this.update(cx, |form, cx| {
                                form.task = None;
                                form.state.fail("clone worker stopped unexpectedly");
                                cx.notify();
                            });
                            return;
                        }
                    };
                    let finished = match message {
                        CloneWorkerMessage::Progress(progress) => {
                            let _ = this.update(cx, |form, cx| {
                                form.state.set_progress(progress);
                                cx.notify();
                            });
                            false
                        }
                        CloneWorkerMessage::Finished(result) => {
                            let _ = this.update(cx, |form, cx| {
                                form.task = None;
                                match result {
                                    Ok((destination, truncated)) => {
                                        form.state.complete(destination.clone(), truncated);
                                        cx.emit(CloneFormEvent::Cloned {
                                            destination,
                                            truncated,
                                        });
                                    }
                                    Err(error) => form.state.fail(error),
                                }
                                cx.notify();
                            });
                            true
                        }
                    };
                    if finished {
                        return;
                    }
                }
                cx.background_executor()
                    .timer(Duration::from_millis(20))
                    .await;
            }
        }));
    }

    fn on_url_key(&mut self, event: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        if matches!(event.keystroke.key.as_str(), "enter" | "return") {
            self.submit(cx);
        }
    }
}

impl Focusable for CloneForm {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.url_field.read(cx).focus_handle(cx)
    }
}

impl EventEmitter<CloneFormEvent> for CloneForm {}

impl Render for CloneForm {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *Theme::get(cx);
        let url_field = self.url_field.clone();
        let destination = self
            .destination()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "derived from the URL".to_owned());
        let (status_line, status_color) = clone_status_line(&self.state, &theme);
        let can_submit = self.state.can_submit();
        let button_label = if self.state.error().is_some() {
            "Retry clone"
        } else {
            "Clone repository"
        };

        div()
            .id("clone-form")
            .w_full()
            .p(px(16.0))
            .flex()
            .flex_col()
            .gap(px(10.0))
            .bg(theme.dialog_surface)
            .child(
                div()
                    .text_size(theme.typography.headline)
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme.text)
                    .child("Clone repository"),
            )
            .child(
                div()
                    .text_size(theme.typography.footnote)
                    .text_color(theme.text_muted)
                    .child("Paste a Git URL and choose where its folder should live."),
            )
            .child(form_label("Repository URL", &theme))
            .child(
                div()
                    .id("clone-url-field")
                    .debug_selector(|| "clone-url-field".to_owned())
                    .w_full()
                    .on_key_down(cx.listener(Self::on_url_key))
                    .overflow_hidden()
                    .child(
                        div()
                            .id("clone-url-field-text")
                            .debug_selector(|| "clone-url-field-text".to_owned())
                            .w_full()
                            .min_w_0()
                            .child(url_field),
                    ),
            )
            .child(form_label("Destination", &theme))
            .child(
                div()
                    .id("clone-destination")
                    .w_full()
                    .text_size(theme.typography.footnote)
                    .text_color(theme.text_muted)
                    .child(destination),
            )
            .child(
                div()
                    .id("clone-submit")
                    .debug_selector(|| "clone-submit".to_owned())
                    .h(px(30.0))
                    .px(px(12.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(theme.radii.control)
                    .bg(if can_submit {
                        theme.element_active
                    } else {
                        theme.surface_raised
                    })
                    .text_size(theme.typography.footnote)
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(if can_submit {
                        theme.text
                    } else {
                        theme.text_faint
                    })
                    .on_click(cx.listener(|form, _, _, cx| form.submit(cx)))
                    .child(button_label),
            )
            .when(self.state.progress().is_some(), |this| {
                let progress = self.state.progress().unwrap_or_default();
                this.child(
                    div()
                        .id("clone-progress")
                        .debug_selector(|| "clone-progress".to_owned())
                        .w_full()
                        .flex()
                        .justify_center()
                        .child(loading::progress(progress as f32, &theme)),
                )
            })
            .child(
                div()
                    .id("clone-status")
                    .text_size(theme.typography.footnote)
                    .text_color(status_color)
                    .child(status_line),
            )
    }
}

/// The observable states of a create-directory operation.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum CreateStatus {
    #[default]
    Ready,
    Running,
    Failed(String),
    Complete(PathBuf),
}

/// Pure state and guards for [`CreateForm`].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CreateFormState {
    name: String,
    status: CreateStatus,
}

impl CreateFormState {
    pub fn set_name(&mut self, name: impl Into<String>) {
        self.name = name.into();
        if !matches!(self.status, CreateStatus::Running) {
            self.status = CreateStatus::Ready;
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn status(&self) -> &CreateStatus {
        &self.status
    }

    pub fn error(&self) -> Option<&str> {
        match &self.status {
            CreateStatus::Failed(error) => Some(error),
            _ => None,
        }
    }

    pub fn can_submit(&self) -> bool {
        !self.name.trim().is_empty()
            && !matches!(
                self.status,
                CreateStatus::Running | CreateStatus::Complete(_)
            )
    }

    pub fn begin(&mut self) -> bool {
        if !self.can_submit() {
            return false;
        }
        self.status = CreateStatus::Running;
        true
    }

    pub fn fail(&mut self, error: impl Into<String>) {
        self.status = CreateStatus::Failed(error.into());
    }

    pub fn complete(&mut self, destination: PathBuf) {
        self.status = CreateStatus::Complete(destination);
    }
}

/// The typed result published when [`CreateForm`] finishes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CreateFormEvent {
    Created(PathBuf),
}

/// A GPUI surface for creating a new project directory.
pub struct CreateForm {
    parent: PathBuf,
    state: CreateFormState,
    name_field: Entity<TextField>,
    task: Option<Task<()>>,
}

impl CreateForm {
    pub fn new(parent: PathBuf, cx: &mut Context<Self>) -> Self {
        ensure_theme(cx);
        let name_field = cx.new(|cx| TextField::new(cx).with_placeholder("project-folder-name"));
        cx.observe(&name_field, |form, field, cx| {
            let name = field.read(cx).content().to_string();
            if name != form.state.name() {
                form.state.set_name(name);
                cx.notify();
            }
        })
        .detach();
        Self {
            parent,
            state: CreateFormState::default(),
            name_field,
            task: None,
        }
    }

    pub fn set_parent(&mut self, parent: PathBuf, cx: &mut Context<Self>) {
        self.parent = parent;
        cx.notify();
    }

    pub fn parent(&self) -> &Path {
        &self.parent
    }

    pub fn set_name(&mut self, name: impl Into<String>, cx: &mut Context<Self>) {
        let name = name.into();
        self.state.set_name(name.clone());
        self.name_field
            .update(cx, |field, cx| field.set_content(name, cx));
        cx.notify();
    }

    pub fn name(&self) -> &str {
        self.state.name()
    }

    pub fn destination(&self) -> PathBuf {
        self.parent.join(self.state.name().trim())
    }

    pub fn status(&self) -> &CreateStatus {
        self.state.status()
    }

    /// Starts directory creation, retaining the draft on failure for retry.
    pub fn submit(&mut self, cx: &mut Context<Self>) {
        let name = self.name_field.read(cx).content().to_string();
        if name != self.state.name() {
            self.state.set_name(name);
        }
        if !self.state.begin() {
            return;
        }
        let parent = self.parent.clone();
        let name = self.state.name().to_owned();
        self.task = Some(cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    create_project(&parent, &name).map_err(|error| error.to_string())
                })
                .await;
            let _ = this.update(cx, |form, cx| {
                form.task = None;
                match result {
                    Ok(destination) => {
                        form.state.complete(destination.clone());
                        cx.emit(CreateFormEvent::Created(destination));
                    }
                    Err(error) => form.state.fail(error),
                }
                cx.notify();
            });
        }));
    }

    fn on_name_key(&mut self, event: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        if matches!(event.keystroke.key.as_str(), "enter" | "return") {
            self.submit(cx);
        }
    }
}

impl Focusable for CreateForm {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.name_field.read(cx).focus_handle(cx)
    }
}

impl EventEmitter<CreateFormEvent> for CreateForm {}

impl Render for CreateForm {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *Theme::get(cx);
        let name_field = self.name_field.clone();
        let parent = self.parent.display().to_string();
        let destination = self.destination().display().to_string();
        let (status_line, status_color) = create_status_line(&self.state, &theme);
        let can_submit = self.state.can_submit();
        let creating = matches!(self.state.status(), CreateStatus::Running);
        let button_label = if self.state.error().is_some() {
            "Retry creation"
        } else {
            "Create project"
        };

        div()
            .id("create-form")
            .w_full()
            .p(px(16.0))
            .flex()
            .flex_col()
            .gap(px(10.0))
            .bg(theme.dialog_surface)
            .child(
                div()
                    .text_size(theme.typography.headline)
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme.text)
                    .child("Create project"),
            )
            .child(
                div()
                    .text_size(theme.typography.footnote)
                    .text_color(theme.text_muted)
                    .child("Make a new folder for a project in the selected location."),
            )
            .child(form_label("Project name", &theme))
            .child(
                div()
                    .id("create-name-field")
                    .debug_selector(|| "create-name-field".to_owned())
                    .w_full()
                    .on_key_down(cx.listener(Self::on_name_key))
                    .overflow_hidden()
                    .child(
                        div()
                            .id("create-name-field-text")
                            .debug_selector(|| "create-name-field-text".to_owned())
                            .w_full()
                            .min_w_0()
                            .child(name_field),
                    ),
            )
            .child(form_label("Parent location", &theme))
            .child(
                div()
                    .id("create-parent")
                    .w_full()
                    .text_size(theme.typography.footnote)
                    .text_color(theme.text_muted)
                    .child(parent),
            )
            .child(
                div()
                    .id("create-destination")
                    .w_full()
                    .text_size(theme.typography.footnote)
                    .text_color(theme.text_faint)
                    .child(format!("Creates {destination}")),
            )
            .child(
                div()
                    .id("create-submit")
                    .debug_selector(|| "create-submit".to_owned())
                    .h(px(30.0))
                    .px(px(12.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(theme.radii.control)
                    .bg(if can_submit {
                        theme.element_active
                    } else {
                        theme.surface_raised
                    })
                    .text_size(theme.typography.footnote)
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(if can_submit {
                        theme.text
                    } else {
                        theme.text_faint
                    })
                    .on_click(cx.listener(|form, _, _, cx| form.submit(cx)))
                    .child(button_label),
            )
            .when(creating, |this| {
                this.child(
                    div()
                        .id("create-loading")
                        .debug_selector(|| "create-loading".to_owned())
                        .w_full()
                        .flex()
                        .justify_center()
                        .child(loading::indeterminate(
                            "create-loading-orb",
                            loading::GENERIC_ORB,
                            &theme,
                            window,
                            cx,
                        )),
                )
            })
            .child(
                div()
                    .id("create-status")
                    .text_size(theme.typography.footnote)
                    .text_color(status_color)
                    .child(status_line),
            )
    }
}

fn destination_for(parent: &Path, url: &str) -> Option<PathBuf> {
    let name = GitRemote::project_name(url);
    let mut components = Path::new(&name).components();
    if !matches!(
        (components.next(), components.next()),
        (Some(Component::Normal(_)), None)
    ) {
        return None;
    }
    Some(parent.join(name))
}

fn clone_status_line(state: &CloneFormState, theme: &Theme) -> (String, gpui::Rgba) {
    match state.status() {
        CloneStatus::Ready => ("Ready to clone".to_owned(), theme.text_muted),
        CloneStatus::Running { progress } => (
            format!("Cloning… {}%", (progress * 100.0).round() as u8),
            theme.text,
        ),
        CloneStatus::Failed(error) => (format!("Clone failed: {error}"), theme.danger),
        CloneStatus::Complete {
            destination,
            truncated: true,
        } => (
            format!(
                "Cloned to {} — progress output was truncated (repository is large)",
                destination.display()
            ),
            theme.warning,
        ),
        CloneStatus::Complete {
            destination,
            truncated: false,
        } => (
            format!("Cloned to {}", destination.display()),
            theme.success,
        ),
    }
}

fn create_status_line(state: &CreateFormState, theme: &Theme) -> (String, gpui::Rgba) {
    match state.status() {
        CreateStatus::Ready => ("Ready to create".to_owned(), theme.text_muted),
        CreateStatus::Running => ("Creating project…".to_owned(), theme.text),
        CreateStatus::Failed(error) => (format!("Creation failed: {error}"), theme.danger),
        CreateStatus::Complete(destination) => {
            (format!("Created {}", destination.display()), theme.success)
        }
    }
}

fn form_label(label: &'static str, theme: &Theme) -> impl IntoElement {
    div()
        .text_size(theme.typography.footnote)
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(theme.text)
        .child(label)
}

fn ensure_theme(cx: &mut Context<impl Sized>) {
    if !cx.has_global::<Theme>() {
        Theme::init(cx);
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{
        CloneForm, CloneFormState, CloneStatus, CreateForm, CreateFormState, CreateStatus,
        clone_status_line,
    };
    use gpui::{Modifiers, TestAppContext, VisualTestContext};
    use sirio_theme::Theme;
    use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};
    use std::time::Duration;

    fn init_test_ui(cx: &mut gpui::App) {
        Theme::init(cx);
        bezel::ui::input::init(cx);
    }

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(tag: &str) -> Self {
            static COUNTER: AtomicU64 = AtomicU64::new(0);
            let unique = COUNTER.fetch_add(1, AtomicOrdering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "sirio-project-forms-{tag}-{}-{unique}",
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

    fn git(dir: &std::path::Path, args: &[&str]) {
        let status = std::process::Command::new("git")
            .args(args)
            .current_dir(dir)
            .status()
            .expect("run git");
        assert!(status.success(), "git {args:?} failed in {dir:?}");
    }

    /// The URL spelling for a local fixture repository: forward slashes on
    /// every platform, which is the form git itself prints and accepts.
    ///
    /// A clone URL is parsed as a URL, not as a native path:
    /// `GitRemote::project_name` cuts the destination folder name at the
    /// last `/` or `:`, so `Path::display`'s Windows spelling
    /// (`C:\Users\...\clone-source`) leaves the whole `\Users\...` tail as
    /// the "name" and the form rejects it before any clone starts. Only the
    /// spelling handed to the field changes here; `git clone` takes this
    /// form on Windows exactly as it takes the backslash one, and the
    /// fixture's own `Path` is untouched. Same normalization, same reason,
    /// as `porcelain_spelling` in `sirio_git/tests/worktree_integration.rs`.
    fn clone_url_for(path: &std::path::Path) -> String {
        let spelling = path.to_string_lossy();
        #[cfg(windows)]
        let spelling = spelling.strip_prefix(r"\\?\").unwrap_or(&spelling);
        spelling.replace('\\', "/")
    }

    /// F-PRJ-06: the "cannot be started twice" guard is proven through the
    /// real drawn Clone button, clicked twice back to back, against a real
    /// local repository -- not only at the pure `CloneFormState` layer.
    /// `begin()` flips to `Running` synchronously inside the first click's
    /// `submit()`, before any worker thread runs, so the second click's
    /// `submit()` call deterministically sees `Running` and no-ops; a
    /// broken guard would instead race two real `git clone` processes into
    /// the same destination folder and surface as a `Failed` status.
    /// #212: a value must stay inside its field. This is the clone URL box,
    /// where long values are the norm rather than an edge case.
    ///
    /// Nine input fields in this app are hand-rolled from the same shape --
    /// a flex row holding the text and, after it, the caret -- and all nine
    /// had the text as a bare child, so it laid out at its natural width and
    /// drew across whatever was behind the field. #208 fixed the first; this
    /// covers the one guaranteed to hit it in ordinary use.
    ///
    /// The fixture's URL is absurdly long on purpose. The harness window is
    /// wider than the real sidebar, so a merely realistic URL fits inside it
    /// and the test passes against the broken code -- I wrote that version
    /// first and it did. The invariant is that *any* value stays inside, so
    /// the fixture picks one that cannot fit at any plausible width.
    ///
    /// Geometry, not text, because geometry is what the defect was.
    #[gpui::test]
    async fn a_long_url_stays_inside_the_clone_url_field(cx: &mut TestAppContext) {
        cx.update(init_test_ui);
        let parent = TempDir::new("clone-layout");
        let window = cx.add_window(|_, cx| CloneForm::new(parent.0.clone(), cx));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        let form = cx.update(|window, _| window.root::<CloneForm>().flatten().expect("form root"));

        form.update(&mut cx.cx, |form, cx| {
            form.set_url(
                "https://gitlab.example.com/organisation/subgroup/team-tools/a-very-long-repository-name.git"
                    .repeat(8),
                cx,
            )
        });
        cx.run_until_parked();

        let field = cx
            .debug_bounds("clone-url-field")
            .expect("the URL field is drawn");
        let text = cx
            .debug_bounds("clone-url-field-text")
            .expect("the URL field's text is drawn");

        assert!(
            text.right() <= field.right(),
            "a typed URL must not draw past the field's own right edge: \
             text={text:?} field={field:?}"
        );
        assert!(
            text.left() >= field.left(),
            "nor past its left edge: text={text:?} field={field:?}"
        );
    }

    #[gpui::test]
    async fn the_drawn_clone_button_cannot_start_a_second_clone(cx: &mut TestAppContext) {
        cx.update(init_test_ui);
        let source = TempDir::new("clone-source");
        std::fs::write(source.0.join("file.txt"), "hello\n").expect("seed source file");
        git(&source.0, &["init", "-q"]);
        git(&source.0, &["config", "user.email", "test@example.test"]);
        git(&source.0, &["config", "user.name", "Test"]);
        git(&source.0, &["add", "file.txt"]);
        git(&source.0, &["commit", "-q", "-m", "seed"]);

        let parent = TempDir::new("clone-parent");
        let window = cx.add_window(|_, cx| CloneForm::new(parent.0.clone(), cx));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        let form = cx.update(|window, _| window.root::<CloneForm>().flatten().expect("form root"));
        form.update(&mut cx.cx, |form, cx| {
            form.set_url(clone_url_for(&source.0), cx)
        });
        cx.run_until_parked();

        let submit = cx
            .debug_bounds("clone-submit")
            .expect("the Clone button is drawn");
        cx.simulate_click(submit.center(), Modifiers::none());
        cx.simulate_click(submit.center(), Modifiers::none());

        cx.cx.executor().allow_parking();
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        while std::time::Instant::now() < deadline {
            cx.run_until_parked();
            cx.cx.executor().advance_clock(Duration::from_millis(25));
            cx.run_until_parked();
            if !matches!(
                form.read_with(&cx.cx, |form, _| form.status().clone()),
                CloneStatus::Running { .. }
            ) {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }

        let status = form.read_with(&cx.cx, |form, _| form.status().clone());
        assert!(
            matches!(status, CloneStatus::Complete { .. }),
            "two real clicks must produce exactly one successful clone, not a destination-exists \
             failure from a second racing clone: {status:?}"
        );
    }

    /// F-PRJ-09: the equivalent guard for Create, against a real drawn
    /// button and a real filesystem destination.
    #[gpui::test]
    async fn the_drawn_create_button_cannot_start_a_second_creation(cx: &mut TestAppContext) {
        cx.update(init_test_ui);
        let parent = TempDir::new("create-parent");
        let window = cx.add_window(|_, cx| CreateForm::new(parent.0.clone(), cx));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        let form = cx.update(|window, _| window.root::<CreateForm>().flatten().expect("form root"));
        form.update(&mut cx.cx, |form, cx| form.set_name("new-project", cx));
        cx.run_until_parked();

        let submit = cx
            .debug_bounds("create-submit")
            .expect("the Create button is drawn");
        cx.simulate_click(submit.center(), Modifiers::none());
        cx.simulate_click(submit.center(), Modifiers::none());

        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        while std::time::Instant::now() < deadline {
            cx.run_until_parked();
            if !matches!(
                form.read_with(&cx.cx, |form, _| form.status().clone()),
                CreateStatus::Running
            ) {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }

        let status = form.read_with(&cx.cx, |form, _| form.status().clone());
        assert!(
            matches!(status, CreateStatus::Complete(_)),
            "two real clicks must produce exactly one successful creation, not a \
             destination-exists failure from a second racing create: {status:?}"
        );
    }

    #[test]
    fn a_clone_bar_reports_the_truthful_fraction() {
        assert_eq!(crate::loading::clamp_fraction(0.0), 0.0);
        assert_eq!(crate::loading::clamp_fraction(0.42), 0.42);
        assert_eq!(crate::loading::clamp_fraction(1.0), 1.0);
    }

    #[gpui::test]
    async fn a_cancelled_clone_shows_no_progress_bar(cx: &mut TestAppContext) {
        cx.update(init_test_ui);
        let parent = TempDir::new("clone-cancelled");
        let window = cx.add_window(|_, cx| CloneForm::new(parent.0.clone(), cx));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        let form = cx.update(|window, _| window.root::<CloneForm>().flatten().expect("form root"));

        form.update(&mut cx.cx, |form, cx| {
            form.state.set_url("file:///tmp/source");
            assert!(form.state.begin());
            form.state.fail("cancelled");
            cx.notify();
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.refresh();
            window.simulate_next_frame(cx);
        });

        assert!(
            cx.debug_bounds("clone-progress").is_none(),
            "a terminal clone state wins over the determinate progress bar"
        );
    }

    #[test]
    fn clone_state_disables_empty_url_and_double_submission() {
        let mut state = CloneFormState::default();
        assert!(!state.can_submit());

        state.set_url("file:///tmp/source");
        assert!(state.can_submit());
        assert!(state.begin());
        assert!(!state.can_submit());
        assert!(!state.begin());
        assert!(matches!(state.status(), CloneStatus::Running { .. }));
    }

    #[test]
    fn clone_failure_keeps_url_and_allows_retry() {
        let mut state = CloneFormState::default();
        state.set_url("https://example.test/repository.git");
        assert!(state.begin());
        state.fail("connection refused");

        assert_eq!(state.url(), "https://example.test/repository.git");
        assert_eq!(state.error(), Some("connection refused"));
        assert!(state.can_submit());
        assert!(state.begin());
    }

    #[test]
    fn clone_progress_and_completion_are_explicit_states() {
        let mut state = CloneFormState::default();
        state.set_url("file:///tmp/source");
        assert!(state.begin());
        state.set_progress(0.47);
        assert_eq!(state.progress(), Some(0.47));
        let destination = PathBuf::from("/tmp/source");
        state.complete(destination.clone(), false);
        assert_eq!(
            state.status(),
            &CloneStatus::Complete {
                destination,
                truncated: false,
            }
        );
        assert!(!state.can_submit());
    }

    /// F-PRJ-truncation: a truncated clone completes (it is not `Failed`)
    /// but the status line names the condition and uses the theme's
    /// warning hue rather than the success color, so a truncated clone
    /// cannot be mistaken for a Failed one or an ordinary Complete one.
    /// This is the pure-state layer of the finding this change fixes —
    /// `GitCommandResult::truncated` used to have nowhere to go.
    #[test]
    fn clone_completion_can_carry_a_truncation_notice() {
        let mut state = CloneFormState::default();
        state.set_url("file:///tmp/source");
        assert!(state.begin());
        let destination = PathBuf::from("/tmp/source");
        state.complete(destination.clone(), true);
        assert_eq!(
            state.status(),
            &CloneStatus::Complete {
                destination: destination.clone(),
                truncated: true,
            }
        );

        let theme = Theme::dark();
        let (line, color) = clone_status_line(&state, &theme);
        assert!(
            line.contains("truncated"),
            "the status line must name the truncation, got: {line:?}"
        );
        assert_ne!(
            color, theme.danger,
            "a truncated clone is not a failure and must not use the error color"
        );
        assert_ne!(
            color, theme.success,
            "a truncated clone must be visually distinct from a clean completion"
        );
    }

    #[test]
    fn create_state_disables_empty_name_and_double_submission() {
        let mut state = CreateFormState::default();
        assert!(!state.can_submit());

        state.set_name("new-project");
        assert!(state.can_submit());
        assert!(state.begin());
        assert!(!state.can_submit());
        assert!(!state.begin());
        assert!(matches!(state.status(), CreateStatus::Running));
    }

    #[test]
    fn create_failure_keeps_name_and_allows_retry() {
        let mut state = CreateFormState::default();
        state.set_name("already-there");
        assert!(state.begin());
        state.fail("already exists");

        assert_eq!(state.name(), "already-there");
        assert_eq!(state.error(), Some("already exists"));
        assert!(state.can_submit());
        assert!(state.begin());
    }
}
