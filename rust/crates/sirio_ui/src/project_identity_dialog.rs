//! The Project added dialog: the prompt that offers a name and an icon for a
//! project the moment it joins the sidebar.

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;

use ely_gpui_component::buttons::{Button, ButtonVariant};
use ely_gpui_component::forms::{Input, InputEvent, TextInput};
use ely_gpui_component::overlays::{Close, Dialog};
use gpui::{
    AnyElement, App, Bounds, Context, Entity, EventEmitter, FocusHandle, Focusable, FontWeight,
    InteractiveElement, IntoElement, Pixels, Render, Subscription, Window, canvas, div,
    prelude::*,
};
use sirio_theme::Theme;

use crate::project_identity::{ProjectIcon, ProjectIconPicker, normalize_display_name};
use crate::text_selection::selectable_text;

/// Where focus goes when the dialog opens.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FocusMode {
    /// Into the name field, with its text selected.
    Field,
    /// Onto the dialog's body, so a keystroke does not land in a field the
    /// user did not choose.
    Dialog,
}

/// How the dialog ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IdentityOutcome {
    /// Save, with a label or an icon that differs from the defaults.
    Saved {
        display_name: Option<String>,
        icon: ProjectIcon,
    },
    /// Save, with nothing changed: nothing is written.
    Unchanged,
    /// Skip, Escape or any other way out.
    Skipped,
}

/// What the dialog tells its owner. Sent once, as the dialog ends.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProjectIdentityEvent {
    Closed {
        project_id: String,
        outcome: IdentityOutcome,
    },
}

/// Another project in the catalog, for the same-name warning.
#[derive(Clone, Debug)]
pub struct OtherProject {
    /// The project's label, or its root name when it has none.
    pub shown_name: String,
    pub path: PathBuf,
}

/// Everything the dialog is built from.
#[derive(Clone, Debug)]
pub struct ProjectIdentityDialogInput {
    /// The owner's per-workspace counter, so each dialog has its own id.
    pub seq: u64,
    pub project_id: String,
    pub root_name: String,
    pub root_path: PathBuf,
    pub initial_icon: ProjectIcon,
    pub focus_mode: FocusMode,
    /// The truncation notice of a clone whose progress output was cut short.
    pub notice: Option<String>,
    pub others: Vec<OtherProject>,
}

/// The Project added dialog: the root path, a name field prefilled with the
/// root name, the icon picker and Skip / Save. It never writes anything: it
/// reports how it ended through [`ProjectIdentityEvent`], once, and its
/// owner stores the result and lets the dialog go.
pub struct ProjectIdentityDialog {
    project_id: String,
    root_name: String,
    root_path: PathBuf,
    seq: u64,
    field: Entity<TextInput>,
    field_focus: FocusHandle,
    /// Tracked on the div that wraps the whole body, inside Ely's layer, so
    /// the layer's Escape listener stays on the dispatch path.
    body_focus: FocusHandle,
    picker: Entity<ProjectIconPicker>,
    initial_icon: ProjectIcon,
    focus_mode: FocusMode,
    notice: Option<String>,
    others: Vec<OtherProject>,
    /// Set by the first of `save` and `skip`; the dialog emits once. Shared
    /// with the `on_close` closure, which runs on every way out.
    emitted: Rc<Cell<bool>>,
    /// The dialog's own `Close`. Ely hands it to the `.action(..)` builders
    /// while the dialog is drawn, so the entity can only keep a copy in a
    /// cell those builders also hold.
    stored_close: Rc<RefCell<Option<Close>>>,
    selected_once: bool,
    rendered_once: bool,
    field_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    path_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    _field_changed: Subscription,
}

impl EventEmitter<ProjectIdentityEvent> for ProjectIdentityDialog {}

impl ProjectIdentityDialog {
    pub fn new(input: ProjectIdentityDialogInput, cx: &mut Context<Self>) -> Self {
        // The field's keys are bound once, here or at startup, the way the
        // clone and create forms bind them.
        if !cx.has_global::<Theme>() {
            Theme::init(cx);
        }
        crate::ely::init(cx);
        let field = cx.new(TextInput::detached);
        field.update(cx, |field, cx| field.set_text(input.root_name.clone(), cx));
        // `TextInput::focus` is not public; the handle is.
        let field_focus = field.read(cx).focus_handle(cx);
        let picker = cx.new(|cx| {
            ProjectIconPicker::with_value_and_repo(input.initial_icon.clone(), &input.root_path, cx)
                .bare()
        });
        // The warning follows what is typed.
        let _field_changed = cx.subscribe(&field, |_, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Changed) {
                cx.notify();
            }
        });
        Self {
            project_id: input.project_id,
            root_name: input.root_name,
            root_path: input.root_path,
            seq: input.seq,
            field,
            field_focus,
            body_focus: cx.focus_handle(),
            picker,
            initial_icon: input.initial_icon,
            focus_mode: input.focus_mode,
            notice: input.notice,
            others: input.others,
            emitted: Rc::new(Cell::new(false)),
            stored_close: Rc::new(RefCell::new(None)),
            selected_once: false,
            rendered_once: false,
            field_bounds: Rc::new(Cell::new(None)),
            path_bounds: Rc::new(Cell::new(None)),
            _field_changed,
        }
    }

    /// Ends the dialog with what is typed and chosen. Nothing happens when
    /// the dialog has already ended.
    pub fn save(&mut self, cx: &mut Context<Self>) {
        if self.emitted.replace(true) {
            return;
        }
        let display_name = normalize_display_name(self.field.read(cx).text(), &self.root_name);
        let icon = self.picker.read(cx).value().clone();
        let outcome = if display_name.is_none() && icon == self.initial_icon {
            IdentityOutcome::Unchanged
        } else {
            IdentityOutcome::Saved { display_name, icon }
        };
        self.close_with(outcome, cx);
    }

    /// Ends the dialog without a change. Nothing happens when the dialog
    /// has already ended.
    pub fn skip(&mut self, cx: &mut Context<Self>) {
        if self.emitted.replace(true) {
            return;
        }
        self.close_with(IdentityOutcome::Skipped, cx);
    }

    fn close_with(&self, outcome: IdentityOutcome, cx: &mut Context<Self>) {
        cx.emit(ProjectIdentityEvent::Closed {
            project_id: self.project_id.clone(),
            outcome,
        });
    }

    /// Whether focus is anywhere in the dialog, the field and the picker
    /// included.
    pub fn contains_focus(&self, window: &Window, cx: &App) -> bool {
        self.body_focus.contains_focused(window, cx)
    }

    /// Takes focus back: into the field in field mode, onto the body in
    /// dialog mode.
    pub fn reclaim_focus(&self, window: &mut Window, cx: &mut App) {
        match self.focus_mode {
            FocusMode::Field => window.focus(&self.field_focus, cx),
            FocusMode::Dialog => window.focus(&self.body_focus, cx),
        }
    }

    /// True once the dialog has been drawn. Ely records the focus to give
    /// back during the dialog's first frame, so a focus rule that ran before
    /// it would be taken for the focus the dialog replaced.
    pub fn rendered_once(&self) -> bool {
        self.rendered_once
    }

    /// The first other project whose shown name equals the one this label
    /// would give, in catalog order.
    fn same_name(&self, cx: &App) -> Option<&OtherProject> {
        let shown = normalize_display_name(self.field.read(cx).text(), &self.root_name)
            .unwrap_or_else(|| self.root_name.clone());
        self.others.iter().find(|other| other.shown_name == shown)
    }

    /// The dialog's state as `key, value` pairs, for the debug read verb.
    pub fn report(&self, window: &Window, cx: &App) -> Vec<(String, String)> {
        let (kind, value) = self.picker.read(cx).value().persisted_parts();
        let color = self.picker.read(cx).value().tint.id();
        let focus = if self.field_focus.is_focused(window) {
            "field"
        } else if self.body_focus.contains_focused(window, cx) {
            "dialog"
        } else {
            ""
        };
        let on_off = |on: bool| if on { "on" } else { "off" };
        let bounds = |cell: &Cell<Option<Bounds<Pixels>>>| match cell.get() {
            Some(b) => format!(
                "{:.0},{:.0},{:.0},{:.0}",
                f32::from(b.origin.x),
                f32::from(b.origin.y),
                f32::from(b.size.width),
                f32::from(b.size.height)
            ),
            None => String::new(),
        };
        vec![
            ("projectId".into(), self.project_id.clone()),
            (
                "focusMode".into(),
                match self.focus_mode {
                    FocusMode::Field => "field",
                    FocusMode::Dialog => "dialog",
                }
                .into(),
            ),
            ("nameDraft".into(), self.field.read(cx).text().to_string()),
            ("iconKind".into(), kind),
            ("iconValue".into(), value.unwrap_or_default()),
            ("color".into(), color.into()),
            ("focus".into(), focus.into()),
            ("warning".into(), on_off(self.same_name(cx).is_some()).into()),
            ("notice".into(), on_off(self.notice.is_some()).into()),
            ("fieldBounds".into(), bounds(&self.field_bounds)),
            ("pathBounds".into(), bounds(&self.path_bounds)),
        ]
    }

    /// The debug act verb: `save`, `skip`, `mode <icon|emoji|avatar>`,
    /// `open-emoji-grid` and `pick-glyph <id>`. It runs inside an update of
    /// this entity, so `save` and `skip` end the dialog through the stored
    /// `Close`: it hands focus back and runs `on_close`, which returns at
    /// once because `emitted` is already set, so the entity is leased once.
    pub fn control_act(
        &mut self,
        action: &str,
        value: Option<&str>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        match action {
            "save" | "skip" => {
                let close = self
                    .stored_close
                    .borrow()
                    .clone()
                    .ok_or_else(|| "the dialog has not been drawn yet".to_string())?;
                if action == "save" {
                    self.save(cx);
                } else {
                    self.skip(cx);
                }
                close(window, cx);
                Ok(())
            }
            "mode" => {
                let mode = value
                    .filter(|mode| matches!(*mode, "icon" | "emoji" | "avatar"))
                    .ok_or_else(|| "mode takes icon, emoji or avatar".to_string())?;
                self.picker
                    .update(cx, |picker, cx| picker.control_set_mode(mode, cx));
                Ok(())
            }
            "open-emoji-grid" => {
                self.picker
                    .update(cx, |picker, cx| picker.control_open_emoji_grid(window, cx));
                Ok(())
            }
            "pick-glyph" => {
                let id = value.ok_or_else(|| "pick-glyph takes a glyph id".to_string())?;
                self.picker
                    .update(cx, |picker, cx| picker.control_pick_glyph(id, cx))
            }
            other => Err(format!("unknown act {other:?}")),
        }
    }
}

impl Render for ProjectIdentityDialog {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        crate::ely::sync_theme_if_changed(cx);
        // The name starts selected, so typing replaces it, as `PromptDialog`
        // does on its first frame.
        if self.focus_mode == FocusMode::Field && !self.selected_once {
            self.selected_once = true;
            self.field.update(cx, |field, cx| {
                let end = field.text().len();
                field.select(0..end, cx);
            });
        }
        self.rendered_once = true;

        let theme = *Theme::get(cx);
        let this = cx.weak_entity();
        // Ely runs `on_close` on every way out, and also after a stored
        // `Close` that `save`, `skip` or `control_act` started. The guard
        // keeps that second run from leasing the entity again.
        let on_close = {
            let (this, emitted) = (this.clone(), self.emitted.clone());
            move |_: &mut Window, cx: &mut App| {
                if !emitted.get() {
                    this.update(cx, |dialog, cx| dialog.skip(cx)).ok();
                }
            }
        };

        let dim = |text: &'static str| {
            div()
                .text_size(theme.typography.footnote)
                .text_color(theme.ely.fg_muted)
                .child(text)
        };
        let warning = self.same_name(cx).map(|other| {
            div()
                .flex()
                .flex_col()
                .gap_1()
                .text_size(theme.typography.footnote)
                .text_color(theme.ely.fg_muted)
                .child(
                    selectable_text(format!(
                        "Another project is also named \"{}\"",
                        other.shown_name
                    ))
                    .id("project-identity-warning"),
                )
                .child(
                    selectable_text(other.path.display().to_string())
                        .id("project-identity-warning-path"),
                )
        });
        let notice = self.notice.clone().map(|notice| {
            div()
                .text_size(theme.typography.footnote)
                .text_color(theme.ely.warning)
                .child(selectable_text(notice).id("project-identity-notice"))
        });
        let body = div()
            .track_focus(&self.body_focus)
            .flex()
            .flex_col()
            .gap_5()
            .child(measured(
                &self.path_bounds,
                div()
                    .text_size(theme.typography.footnote)
                    .text_color(theme.ely.fg_muted)
                    .child(
                        selectable_text(self.root_path.display().to_string())
                            .id("project-identity-path"),
                    ),
            ))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1p5()
                    .child(
                        div()
                            .text_size(theme.typography.footnote)
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme.ely.fg)
                            .child("Name in the sidebar"),
                    )
                    .child(measured(&self.field_bounds, Input::new(&self.field)))
                    .child(dim("The folder on disk keeps its name.")),
            )
            .children(warning)
            .children(notice)
            .child(self.picker.clone());

        let mut dialog = Dialog::new(("project-identity", self.seq), "Project added", on_close)
            .held()
            .focus_first(match self.focus_mode {
                FocusMode::Field => self.field_focus.clone(),
                FocusMode::Dialog => self.body_focus.clone(),
            })
            .on_enter(self.field_focus.clone(), {
                let (this, stored) = (this.clone(), self.stored_close.clone());
                move |close| {
                    *stored.borrow_mut() = Some(close.clone());
                    Rc::new(move |window: &mut Window, cx: &mut App| {
                        this.update(cx, |dialog, cx| dialog.save(cx)).ok();
                        close(window, cx);
                    })
                }
            })
            .child(body);
        dialog = dialog
            .action({
                let (this, stored) = (this.clone(), self.stored_close.clone());
                move |close| {
                    *stored.borrow_mut() = Some(close.clone());
                    dialog_button(
                        "project-identity-skip",
                        "Skip",
                        ButtonVariant::Ghost,
                        move |window, cx| {
                            this.update(cx, |dialog, cx| dialog.skip(cx)).ok();
                            close(window, cx);
                        },
                    )
                }
            })
            .action({
                let stored = self.stored_close.clone();
                move |close| {
                    *stored.borrow_mut() = Some(close.clone());
                    dialog_button(
                        "project-identity-save",
                        "Save",
                        ButtonVariant::Primary,
                        move |window, cx| {
                            this.update(cx, |dialog, cx| dialog.save(cx)).ok();
                            close(window, cx);
                        },
                    )
                }
            });
        dialog
    }
}

/// `child`, with its window bounds recorded at prepaint into `cell`, the
/// way `tab_bar.rs` records the anchor of its menu.
fn measured(cell: &Rc<Cell<Option<Bounds<Pixels>>>>, child: impl IntoElement) -> impl IntoElement {
    let cell = cell.clone();
    div().relative().child(child).child(
        canvas(move |bounds, _, _| cell.set(Some(bounds)), |_, _, _, _| {})
            .absolute()
            .size_full(),
    )
}

/// One of the dialog's buttons, findable by its id.
fn dialog_button(
    id: &'static str,
    label: &'static str,
    variant: ButtonVariant,
    on_click: impl Fn(&mut Window, &mut App) + 'static,
) -> AnyElement {
    div()
        .id(id)
        .debug_selector(move || id.to_owned())
        .flex_none()
        .child(
            Button::new(id, label)
                .variant(variant)
                .on_click(move |_, window, cx| on_click(window, cx)),
        )
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{Modifiers, TestAppContext, VisualTestContext};
    use sirio_theme::Theme;

    type Events = Rc<RefCell<Vec<ProjectIdentityEvent>>>;

    /// The owner's side: it keeps the dialog while it is open and lets it go
    /// on the event, the way the workspace does.
    struct Host {
        dialog: Option<Entity<ProjectIdentityDialog>>,
    }

    impl Render for Host {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div().size_full().children(self.dialog.clone())
        }
    }

    fn input() -> ProjectIdentityDialogInput {
        ProjectIdentityDialogInput {
            seq: 1,
            project_id: "project-1".into(),
            root_name: "project-folder".into(),
            root_path: PathBuf::from("/nonexistent/sirio-identity-dialog/project-folder"),
            initial_icon: ProjectIcon::default(),
            focus_mode: FocusMode::Field,
            notice: None,
            others: Vec::new(),
        }
    }

    fn init(cx: &mut TestAppContext) {
        cx.update(|cx| {
            Theme::init(cx);
            crate::ely::init(cx);
        });
    }

    /// The dialog open and drawn, and every event it has emitted.
    fn open(
        cx: &mut TestAppContext,
    ) -> (Entity<ProjectIdentityDialog>, Events, &mut VisualTestContext) {
        init(cx);
        let events: Events = Rc::default();
        let (host, cx) = cx.add_window_view({
            let events = events.clone();
            move |_, cx| {
                let dialog = cx.new(|cx| ProjectIdentityDialog::new(input(), cx));
                cx.subscribe(
                    &dialog,
                    move |host: &mut Host, _, event: &ProjectIdentityEvent, cx| {
                        events.borrow_mut().push(event.clone());
                        host.dialog = None;
                        cx.notify();
                    },
                )
                .detach();
                Host {
                    dialog: Some(dialog),
                }
            }
        });
        cx.run_until_parked();
        let dialog = host.read_with(cx, |host, _| host.dialog.clone().expect("the dialog is open"));
        (dialog, events, cx)
    }

    fn click(cx: &mut VisualTestContext, selector: &'static str) {
        let bounds = cx
            .debug_bounds(selector)
            .unwrap_or_else(|| panic!("{selector} is drawn"));
        cx.simulate_click(bounds.center(), Modifiers::none());
        cx.run_until_parked();
    }

    fn select_all_and_type(cx: &mut VisualTestContext, text: &str) {
        cx.simulate_keystrokes(if cfg!(target_os = "macos") {
            "cmd-a"
        } else {
            "ctrl-a"
        });
        cx.simulate_input(text);
        cx.run_until_parked();
    }

    fn only_event(events: &Events) -> ProjectIdentityEvent {
        let events = events.borrow();
        assert_eq!(events.len(), 1, "exactly one event, got {events:?}");
        events[0].clone()
    }

    fn closed_as(outcome: IdentityOutcome) -> ProjectIdentityEvent {
        ProjectIdentityEvent::Closed {
            project_id: "project-1".into(),
            outcome,
        }
    }

    #[gpui::test]
    fn save_with_no_edit_closes_as_unchanged(cx: &mut TestAppContext) {
        let (_dialog, events, cx) = open(cx);
        click(cx, "project-identity-save");
        assert_eq!(only_event(&events), closed_as(IdentityOutcome::Unchanged));
        assert!(
            cx.debug_bounds("project-identity-save").is_none(),
            "the owner let the dialog go"
        );
    }

    #[gpui::test]
    fn a_typed_label_is_saved(cx: &mut TestAppContext) {
        let (_dialog, events, cx) = open(cx);
        select_all_and_type(cx, "Client App");
        click(cx, "project-identity-save");
        match only_event(&events) {
            ProjectIdentityEvent::Closed {
                project_id,
                outcome:
                    IdentityOutcome::Saved {
                        display_name,
                        icon,
                    },
            } => {
                assert_eq!(project_id, "project-1");
                assert_eq!(display_name.as_deref(), Some("Client App"));
                assert_eq!(icon, ProjectIcon::default(), "the icon was not touched");
            }
            other => panic!("expected a saved label, got {other:?}"),
        }
    }

    #[gpui::test]
    fn the_root_name_typed_again_with_the_icon_unchanged_is_unchanged(cx: &mut TestAppContext) {
        let (_dialog, events, cx) = open(cx);
        select_all_and_type(cx, "project-folder");
        click(cx, "project-identity-save");
        assert_eq!(only_event(&events), closed_as(IdentityOutcome::Unchanged));
    }

    #[gpui::test]
    fn escape_skips(cx: &mut TestAppContext) {
        let (_dialog, events, cx) = open(cx);
        cx.simulate_keystrokes("escape");
        cx.run_until_parked();
        assert_eq!(only_event(&events), closed_as(IdentityOutcome::Skipped));
        assert!(
            cx.debug_bounds("project-identity-save").is_none(),
            "the owner let the dialog go"
        );
    }

    #[gpui::test]
    fn the_act_verb_saves_from_inside_an_update_without_a_second_lease(cx: &mut TestAppContext) {
        let (dialog, events, cx) = open(cx);
        let result = cx.update(|window, cx| {
            dialog.update(cx, |dialog, cx| dialog.control_act("save", None, window, cx))
        });
        cx.run_until_parked();
        assert_eq!(result, Ok(()));
        assert_eq!(only_event(&events), closed_as(IdentityOutcome::Unchanged));
    }

    #[gpui::test]
    fn the_act_verb_skips_from_inside_an_update_without_a_second_lease(cx: &mut TestAppContext) {
        let (dialog, events, cx) = open(cx);
        let result = cx.update(|window, cx| {
            dialog.update(cx, |dialog, cx| dialog.control_act("skip", None, window, cx))
        });
        cx.run_until_parked();
        assert_eq!(result, Ok(()));
        assert_eq!(only_event(&events), closed_as(IdentityOutcome::Skipped));
    }

    #[gpui::test]
    fn the_act_verb_refuses_before_the_first_draw_and_on_an_unknown_action(
        cx: &mut TestAppContext,
    ) {
        init(cx);
        let (_host, cx) = cx.add_window_view(|_, _| Host { dialog: None });
        let events: Events = Rc::default();
        let dialog = cx.new(|cx| ProjectIdentityDialog::new(input(), cx));
        cx.update(|_, cx| {
            let events = events.clone();
            cx.subscribe(&dialog, move |_, event: &ProjectIdentityEvent, _| {
                events.borrow_mut().push(event.clone());
            })
            .detach();
        });

        for action in ["save", "skip"] {
            let result = cx.update(|window, cx| {
                dialog.update(cx, |dialog, cx| dialog.control_act(action, None, window, cx))
            });
            assert_eq!(
                result,
                Err("the dialog has not been drawn yet".to_string()),
                "{action} before the first draw"
            );
        }
        let result = cx.update(|window, cx| {
            dialog.update(cx, |dialog, cx| dialog.control_act("explode", None, window, cx))
        });
        assert!(result.is_err(), "an unknown action is an error");
        cx.run_until_parked();
        assert!(events.borrow().is_empty(), "a refused act emits nothing");
    }
}
