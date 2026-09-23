//! The worktree select the centre shows when no worktree is selected: every
//! worktree of every project, as `project / branch`, searchable.
//!
//! An entity because bezel's `Combobox` is one (it owns its query field). It
//! reports a *path*, not an index, so the host can route the choice down the
//! same selection path a sidebar click takes.

use bezel::ui::combobox::{Combobox, ComboboxEvent};
use gpui::{
    Context, Entity, EventEmitter, SharedString, Subscription, Window, div, prelude::*, px,
};
use std::path::PathBuf;

/// Width of the select's face; its menu matches it.
const PICKER_WIDTH: f32 = 280.0;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorktreeChoice {
    pub project: SharedString,
    pub branch: SharedString,
    pub path: PathBuf,
}

impl WorktreeChoice {
    /// The row text: `project / branch`.
    pub fn label(&self) -> SharedString {
        format!("{} / {}", self.project, self.branch).into()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WorktreePickerEvent {
    Selected(PathBuf),
}

pub struct WorktreePicker {
    choices: Vec<WorktreeChoice>,
    combobox: Entity<Combobox>,
    _selection: Subscription,
}

impl EventEmitter<WorktreePickerEvent> for WorktreePicker {}

impl WorktreePicker {
    pub fn new(choices: Vec<WorktreeChoice>, cx: &mut Context<Self>) -> Self {
        let (combobox, selection) = Self::build(&choices, cx);
        Self {
            choices,
            combobox,
            _selection: selection,
        }
    }

    /// Replaces the list. A no-op when nothing changed, so the host may call
    /// it every frame; otherwise the combobox is rebuilt, because bezel's
    /// `Combobox` takes its items once.
    pub fn set_choices(&mut self, choices: Vec<WorktreeChoice>, cx: &mut Context<Self>) {
        if choices == self.choices {
            return;
        }
        let (combobox, selection) = Self::build(&choices, cx);
        self.choices = choices;
        self.combobox = combobox;
        self._selection = selection;
        cx.notify();
    }

    pub fn choices(&self) -> &[WorktreeChoice] {
        &self.choices
    }

    fn build(
        choices: &[WorktreeChoice],
        cx: &mut Context<Self>,
    ) -> (Entity<Combobox>, Subscription) {
        let items = choices.iter().map(WorktreeChoice::label).collect();
        let combobox = cx.new(|cx| Combobox::new(items, "Select a worktree", cx));
        let selection = cx.subscribe(&combobox, |picker, _, event: &ComboboxEvent, cx| {
            match event {
                // bezel reports an index into the list the combobox was built
                // with, never into the filtered view.
                ComboboxEvent::Selected(index) => {
                    if let Some(choice) = picker.choices.get(*index) {
                        cx.emit(WorktreePickerEvent::Selected(choice.path.clone()));
                    }
                }
            }
        });
        (combobox, selection)
    }
}

impl Render for WorktreePicker {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("worktree-picker")
            .debug_selector(|| "worktree-picker".to_owned())
            .w(px(PICKER_WIDTH))
            .child(self.combobox.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::TestAppContext;
    use std::cell::RefCell;
    use std::rc::Rc;

    fn choice(project: &str, branch: &str, path: &str) -> WorktreeChoice {
        WorktreeChoice {
            project: project.to_owned().into(),
            branch: branch.to_owned().into(),
            path: PathBuf::from(path),
        }
    }

    #[gpui::test]
    fn a_selection_reports_the_path_of_the_row_chosen(cx: &mut TestAppContext) {
        cx.update(bezel::ui::input::init);
        let picker = cx.new(|cx| {
            WorktreePicker::new(
                vec![
                    choice("sirio", "main", "/repos/sirio"),
                    choice("sirio", "feature", "/repos/sirio-feature"),
                ],
                cx,
            )
        });
        let seen = Rc::new(RefCell::new(Vec::new()));
        let sink = seen.clone();
        cx.update(|cx| {
            cx.subscribe(&picker, move |_, event: &WorktreePickerEvent, _| {
                sink.borrow_mut().push(event.clone())
            })
            .detach()
        });

        let combobox = picker.read_with(cx, |picker, _| picker.combobox.clone());
        combobox.update(cx, |_, cx| cx.emit(ComboboxEvent::Selected(1)));
        cx.run_until_parked();

        assert_eq!(
            *seen.borrow(),
            vec![WorktreePickerEvent::Selected(PathBuf::from("/repos/sirio-feature"))]
        );
    }

    #[gpui::test]
    fn set_choices_rebuilds_only_when_the_list_changed(cx: &mut TestAppContext) {
        cx.update(bezel::ui::input::init);
        let first = vec![choice("sirio", "main", "/repos/sirio")];
        let picker = cx.new(|cx| WorktreePicker::new(first.clone(), cx));
        let before = picker.read_with(cx, |picker, _| picker.combobox.entity_id());

        picker.update(cx, |picker, cx| picker.set_choices(first.clone(), cx));
        assert_eq!(
            picker.read_with(cx, |picker, _| picker.combobox.entity_id()),
            before,
            "the same list keeps the same combobox, so an open menu survives a frame"
        );

        picker.update(cx, |picker, cx| picker.set_choices(Vec::new(), cx));
        assert_ne!(
            picker.read_with(cx, |picker, _| picker.combobox.entity_id()),
            before,
            "a different list rebuilds it"
        );
        assert!(picker.read_with(cx, |picker, _| picker.choices().is_empty()));
    }

    #[test]
    fn a_choice_is_labelled_project_slash_branch() {
        assert_eq!(choice("sirio", "main", "/r").label(), SharedString::from("sirio / main"));
    }
}
