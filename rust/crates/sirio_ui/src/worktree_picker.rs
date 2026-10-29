//! The worktree select the centre shows when no worktree is selected: every
//! worktree of every project, as `project / branch`, searchable.
//!
//! An entity because the `Combobox` is one (it owns its query field). It
//! reports a *path*, not an index, so the host can route the choice down the
//! same selection path a sidebar click takes.

use crate::combobox::{Combobox, ComboboxEvent};
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
    /// it every frame; otherwise the combobox is rebuilt, because the
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
                // The combobox reports an index into the list it was built
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
    use gpui::{Modifiers, TestAppContext, VisualTestContext, point};
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::time::Duration;

    fn choice(project: &str, branch: &str, path: &str) -> WorktreeChoice {
        WorktreeChoice {
            project: project.to_owned().into(),
            branch: branch.to_owned().into(),
            path: PathBuf::from(path),
        }
    }

    #[gpui::test]
    fn a_selection_reports_the_path_of_the_row_chosen(cx: &mut TestAppContext) {
        cx.update(crate::ely::init);
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
        cx.update(crate::ely::init);
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

    /// The page the picker sits in, with a probe under it that hears the pointer only where
    /// no open menu covers it: the menu hangs over the probe, so what the pointer can reach
    /// there says whether the menu is mounted and how far down it reaches.
    struct Harness {
        picker: Entity<WorktreePicker>,
        probe_hovered: bool,
    }

    impl Render for Harness {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            div()
                .size_full()
                .flex()
                .flex_col()
                .child(self.picker.clone())
                .child(
                    div()
                        .id("probe")
                        .debug_selector(|| "probe".to_owned())
                        .w(px(PICKER_WIDTH))
                        .h(px(300.0))
                        .on_hover(cx.listener(|harness, hovered: &bool, _, cx| {
                            harness.probe_hovered = *hovered;
                            cx.notify();
                        })),
                )
        }
    }

    /// The keys of the open select, with its query field holding the focus: Down and Enter
    /// choose the second row, a query with no match leaves nothing to choose (and the menu
    /// still reaches below the field, the "No matches" row), Escape closes. The field has
    /// keys of its own for Up, Down and Enter, one level deeper than the combobox's: a field
    /// that is not hosted under the combobox's context takes them for itself, and the select
    /// can then be opened but never answered.
    #[gpui::test]
    async fn the_worktree_combobox_answers_its_keys_while_its_search_field_has_focus(
        cx: &mut TestAppContext,
    ) {
        cx.update(|cx| {
            sirio_theme::Theme::init(cx);
            crate::ely::init(cx);
            crate::combobox::init(cx);
        });
        let picker = cx.new(|cx| {
            WorktreePicker::new(
                vec![
                    choice("sirio", "main", "/repos/sirio"),
                    choice("sirio", "feature", "/repos/sirio-feature"),
                    choice("sirio", "hotfix", "/repos/sirio-hotfix"),
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
        let window = cx.add_window({
            let picker = picker.clone();
            move |_, _| Harness {
                picker,
                probe_hovered: false,
            }
        });
        let harness = window.root(cx).expect("the harness is the window's root");
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        let trigger = cx
            .debug_bounds("worktree-picker")
            .expect("the picker is drawn");
        let probe = cx.debug_bounds("probe").expect("the probe is drawn");
        // Where the open menu's second block sits: below the trigger, its gap, the card's
        // padding and the search field, where the rows (or "No matches") are.
        let below_the_field = point(probe.center().x, probe.top() + px(70.0));
        let hovered = |cx: &mut VisualTestContext| {
            cx.run_until_parked();
            harness.read_with(&cx.cx, |harness, _| harness.probe_hovered)
        };
        // Closed, that spot is the probe's: a pointer there reaches it. This is what makes
        // "covered" below mean something.
        cx.simulate_mouse_move(below_the_field, None, Modifiers::none());
        assert!(hovered(&mut cx), "the probe hears a pointer nothing covers");
        let outside = point(probe.center().x, probe.bottom() + px(40.0));
        cx.simulate_mouse_move(outside, None, Modifiers::none());
        assert!(!hovered(&mut cx), "and not one outside it");

        // Open it with a click on the trigger: the query field takes the focus.
        cx.simulate_click(trigger.center(), Modifiers::none());
        cx.run_until_parked();
        cx.simulate_mouse_move(below_the_field, None, Modifiers::none());
        assert!(!hovered(&mut cx), "the open menu covers the probe");
        cx.simulate_keystrokes("down enter");
        cx.run_until_parked();
        assert_eq!(
            *seen.borrow(),
            vec![WorktreePickerEvent::Selected(PathBuf::from("/repos/sirio-feature"))],
            "Down moved to the second row and Enter chose it, while the query field had focus"
        );

        // Let the closing menu leave the tree, then open it again and narrow it to nothing.
        cx.executor().advance_clock(Duration::from_secs(1));
        cx.run_until_parked();
        cx.simulate_mouse_move(probe.center(), None, Modifiers::none());
        cx.simulate_mouse_move(below_the_field, None, Modifiers::none());
        assert!(hovered(&mut cx), "a chosen row closes the menu");
        cx.simulate_click(trigger.center(), Modifiers::none());
        cx.run_until_parked();
        cx.simulate_input("zzz");
        cx.run_until_parked();
        cx.simulate_mouse_move(probe.center(), None, Modifiers::none());
        cx.simulate_mouse_move(below_the_field, None, Modifiers::none());
        assert!(
            !hovered(&mut cx),
            "with nothing matching, the card still reaches below the field: its \"No matches\" row"
        );
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();
        assert_eq!(seen.borrow().len(), 1, "no row matches `zzz`, so Enter chooses nothing");

        // Escape closes it.
        cx.simulate_keystrokes("escape");
        cx.executor().advance_clock(Duration::from_secs(1));
        cx.run_until_parked();
        cx.simulate_mouse_move(probe.center(), None, Modifiers::none());
        cx.simulate_mouse_move(below_the_field, None, Modifiers::none());
        assert!(hovered(&mut cx), "Escape closes the menu");
        assert_eq!(seen.borrow().len(), 1, "and chooses nothing");
    }
}
