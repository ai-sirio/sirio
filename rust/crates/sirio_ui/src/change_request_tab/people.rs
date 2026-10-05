//! Reviewers and labels (spec §6): a row under the meta line, and a picker
//! per kind that asks the forge for candidates and sends one change when
//! it closes.

use std::collections::{BTreeMap, BTreeSet};

use ely_gpui_component::buttons::IconButton;
use ely_gpui_component::data_display::{Avatar, AvatarGroup, Tag};
use ely_gpui_component::forms::{Choice, InputEvent, ListBox, TextInput};
use ely_gpui_component::overlays::Popover;
use ely_gpui_component::primitives::Severity;
use sirio_forge::{Action, Candidate};

use gpui::{SharedString, Subscription};

use super::actions::ActionState;
use crate::ely_ui::{message, new_input};
use super::*;

/// How long typing must pause before the forge is asked: the right panel's
/// list waits the same (`right_panel/change_requests.rs`).
pub(crate) const SEARCH_DEBOUNCE: Duration = Duration::from_millis(300);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PickerKind {
    Reviewers,
    Labels,
}

impl PickerKind {
    pub(crate) fn word(self) -> &'static str {
        match self {
            Self::Reviewers => "reviewers",
            Self::Labels => "labels",
        }
    }
}

const BUSY: &str = "Wait for the change being sent to finish.";

/// What to add and what to take away to turn `original` into `chosen`.
pub(crate) fn difference(
    original: &BTreeSet<String>,
    chosen: &BTreeSet<String>,
) -> (Vec<String>, Vec<String>) {
    (
        chosen.difference(original).cloned().collect(),
        original.difference(chosen).cloned().collect(),
    )
}

/// The rows a picker lists: who or what was set when it opened and every
/// pick since, then what the search found, one row per id — so neither a
/// current reviewer nor an earlier pick drops out of sight with a new search.
pub(crate) fn picker_choices(current: &BTreeMap<String, String>, found: &[Candidate]) -> Vec<Choice> {
    let mut seen = BTreeSet::new();
    let mut choices = Vec::new();
    for (id, label) in current {
        if seen.insert(id.clone()) {
            choices.push(Choice::new(id.clone(), label.clone()));
        }
    }
    for candidate in found {
        if seen.insert(candidate.id.clone()) {
            let mut choice = Choice::new(candidate.id.clone(), candidate.label.clone());
            choice.note = candidate.note.clone().map(Into::into);
            choices.push(choice);
        }
    }
    choices
}

/// One open picker. `original` is the set when it opened; closing sends the
/// difference to `chosen` as one write (spec §6).
pub(crate) struct PickerState {
    pub(crate) kind: PickerKind,
    query: Entity<TextInput>,
    text: String,
    pub(crate) found: Option<Vec<Candidate>>,
    error: Option<String>,
    /// The set when it opened, and every pick since: always listed.
    current: BTreeMap<String, String>,
    pub(crate) original: BTreeSet<String>,
    pub(crate) chosen: BTreeSet<String>,
    generation: u64,
    debounce: Option<Task<()>>,
    search: Option<Task<()>>,
    _typing: Subscription,
}

impl ChangeRequestTab {
    pub(crate) fn picker_candidates(&self) -> usize {
        self.actions
            .picker
            .as_ref()
            .and_then(|picker| picker.found.as_ref())
            .map_or(0, Vec::len)
    }

    pub(crate) fn picker_is(&self, kind: PickerKind) -> bool {
        self.actions.picker.as_ref().is_some_and(|picker| picker.kind == kind)
    }

    fn picker_offered(&self, kind: PickerKind) -> bool {
        self.header.value().is_some_and(|header| match kind {
            PickerKind::Reviewers => header.capabilities.can_edit_reviewers,
            PickerKind::Labels => header.capabilities.can_edit_labels,
        })
    }

    /// Opens a picker seeded with the header's set, and asks the forge for
    /// its first page at once.
    pub(crate) fn open_picker(
        &mut self,
        kind: PickerKind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        if !self.picker_offered(kind) {
            return Err(format!("the {} cannot be changed here", kind.word()));
        }
        if self.action_busy() {
            return Err(BUSY.to_string());
        }
        let header = self.header.value().ok_or("the change request is not loaded")?;
        let current: BTreeMap<String, String> = match kind {
            PickerKind::Reviewers => header
                .reviewers
                .iter()
                .filter_map(|reviewer| Some((reviewer.id.clone()?, reviewer.login.clone())))
                .collect(),
            PickerKind::Labels => header
                .labels
                .iter()
                .map(|label| (label.id.clone(), label.name.clone()))
                .collect(),
        };
        let original: BTreeSet<String> = current.keys().cloned().collect();
        // One picker at a time: the other one sends what changed first.
        if self.actions.picker.is_some() {
            let _ = self.close_picker(cx);
        }
        let query = new_input(window, cx, "", None, "Search");
        let typing = cx.subscribe(&query, |tab, input, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Changed) {
                let text = input.read(cx).text().to_string();
                tab.type_in_picker(text, cx);
            }
        });
        self.actions.picker = Some(PickerState {
            kind,
            query,
            text: String::new(),
            found: None,
            error: None,
            current,
            chosen: original.clone(),
            original,
            generation: 0,
            debounce: None,
            search: None,
            _typing: typing,
        });
        self.search_candidates(0, cx);
        cx.notify();
        Ok(())
    }

    /// The forge is asked once typing pauses for `SEARCH_DEBOUNCE`.
    pub(crate) fn type_in_picker(&mut self, text: String, cx: &mut Context<Self>) {
        let Some(picker) = self.actions.picker.as_mut() else {
            return;
        };
        if picker.text == text {
            return;
        }
        picker.text = text;
        picker.generation += 1;
        let generation = picker.generation;
        picker.debounce = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(SEARCH_DEBOUNCE).await;
            let _ = this.update(cx, |tab, cx| tab.search_candidates(generation, cx));
        }));
    }

    /// Asks now; an answer for text that has since changed is dropped.
    pub(crate) fn search_candidates(&mut self, generation: u64, cx: &mut Context<Self>) {
        let Some(client) = self.client.clone() else {
            return;
        };
        if self.rate_paused() {
            if let Some(picker) = self.actions.picker.as_mut() {
                picker.error = Some("The forge asked Sirio to wait; try again after its reset.".to_string());
            }
            cx.notify();
            return;
        }
        let Some(picker) = self.actions.picker.as_mut() else {
            return;
        };
        if picker.generation != generation {
            return;
        }
        let (kind, text, number) = (picker.kind, picker.text.clone(), self.reference.number);
        picker.search = Some(cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    match kind {
                        PickerKind::Reviewers => client.reviewer_candidates(number, &text),
                        PickerKind::Labels => client.label_candidates(&text),
                    }
                })
                .await;
            let _ = this.update(cx, |tab, cx| {
                if let Err(error) = &result {
                    tab.note_rate_limited(error);
                }
                let forge = tab.reference.forge;
                let Some(picker) = tab.actions.picker.as_mut() else {
                    return;
                };
                if picker.generation != generation {
                    return;
                }
                match result {
                    Ok(found) => {
                        picker.found = Some(found);
                        picker.error = None;
                    }
                    Err(error) => picker.error = Some(actions::action_error_text(&error, forge, "candidates")),
                }
                cx.notify();
            });
        }));
    }

    pub(crate) fn toggle_in_picker(&mut self, id: String, cx: &mut Context<Self>) {
        if let Some(picker) = self.actions.picker.as_mut() {
            if !picker.chosen.remove(&id) {
                picker.chosen.insert(id);
            }
            cx.notify();
        }
    }

    /// What the list hands back after a click: the rows it shows that are
    /// ticked, with the clicked one toggled.
    /// A pick stays listed through later searches, and a row the list
    /// does not show keeps its tick.
    pub(crate) fn choose_in_picker(&mut self, values: BTreeSet<String>, cx: &mut Context<Self>) {
        if let Some(picker) = self.actions.picker.as_mut() {
            let found = picker.found.as_deref().unwrap_or(&[]);
            let shown: BTreeSet<String> = picker_choices(&picker.current, found)
                .into_iter()
                .map(|choice| choice.value.to_string())
                .collect();
            for candidate in found.iter().filter(|candidate| values.contains(&candidate.id)) {
                picker.current.entry(candidate.id.clone()).or_insert_with(|| candidate.label.clone());
            }
            picker.chosen = picker.chosen.difference(&shown).cloned().chain(values).collect();
        }
        cx.notify();
    }

    /// Closing is the send: one write with the difference, or none.
    /// While another write is in flight a picker with picks stays open,
    /// picks and all: one write at a time per tab.
    pub(crate) fn close_picker(&mut self, cx: &mut Context<Self>) -> Result<(), String> {
        let Some(picker) = self.actions.picker.as_ref() else {
            return Ok(());
        };
        let (add, remove) = difference(&picker.original, &picker.chosen);
        let changed = !(add.is_empty() && remove.is_empty());
        if changed && self.action_busy() {
            return Err(BUSY.to_string());
        }
        let Some(picker) = self.actions.picker.take() else {
            return Ok(());
        };
        cx.notify();
        if !changed {
            return Ok(());
        }
        let action = match picker.kind {
            PickerKind::Reviewers => Action::SetReviewers { add, remove },
            PickerKind::Labels => Action::SetLabels { add, remove },
        };
        let result = self.perform(action, cx);
        if let Err(reason) = &result {
            self.actions.state = ActionState::Failed {
                kind: picker.kind.word(),
                message: reason.clone(),
            };
        }
        result
    }

    /// The panel of an open picker: the search field, then the rows, or a
    /// line saying why there are none (Ely's `ListBox` needs at least one).
    pub(crate) fn picker_body(
        &self,
        theme: &Theme,
        entity: &Entity<Self>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(picker) = self.actions.picker.as_ref() else {
            return div().into_any_element();
        };
        let choices = picker_choices(&picker.current, picker.found.as_deref().unwrap_or(&[]));
        let list = if let Some(error) = &picker.error {
            message(Severity::Danger, error.clone(), theme)
        } else if picker.found.is_none() {
            message(Severity::Info, "Searching…".to_string(), theme)
        } else if choices.is_empty() {
            let text = match picker.kind {
                PickerKind::Reviewers => "No one matches. Teams are not offered.",
                PickerKind::Labels => "No label matches.",
            };
            message(Severity::Info, text.to_string(), theme)
        } else {
            let pick = entity.clone();
            ListBox::new(("change-request-picker-list", picker.generation as usize), choices)
                .multiple()
                .selected(picker.chosen.iter().cloned())
                .on_change(move |values, _, cx| {
                    let values: BTreeSet<String> = values.iter().map(|value| value.to_string()).collect();
                    pick.update(cx, |tab, cx| tab.choose_in_picker(values, cx));
                })
                .into_any_element()
        };
        div()
            .debug_selector(|| "change-request-picker".to_owned())
            .w(px(280.0))
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(picker.query.clone())
            .child(div().max_h(px(260.0)).child(list))
            .into_any_element()
    }

    /// `Reviewers: … [+]   Labels: … [+]`, under the meta line.
    pub(crate) fn render_people_row(&self, theme: &Theme, entity: &Entity<Self>) -> Option<AnyElement> {
        let header = self.header.value()?;
        let avatars = (!header.reviewers.is_empty()).then(|| {
            AvatarGroup::new(
                header
                    .reviewers
                    .iter()
                    .enumerate()
                    .map(|(index, reviewer)| Avatar::new(("change-request-reviewer", index), reviewer.login.clone())),
            )
            .max(6)
        });
        let tags = header
            .labels
            .iter()
            .enumerate()
            .map(|(index, label)| Tag::new(("change-request-label", index), label.name.clone()));
        let caption = |text: &'static str| div().flex_none().text_color(theme.ely.fg_muted).child(text);
        Some(
            div()
                .debug_selector(|| "change-request-people".to_owned())
                .flex()
                .flex_wrap()
                .items_center()
                .gap(px(8.0))
                .text_size(theme.typography.footnote)
                .child(caption("Reviewers:"))
                .children(avatars)
                .children(self.picker_button(PickerKind::Reviewers, theme, entity))
                .child(div().w(px(12.0)))
                .child(caption("Labels:"))
                .children(tags)
                .children(self.picker_button(PickerKind::Labels, theme, entity))
                .into_any_element(),
        )
    }

    /// The `+` that opens a picker, only where its capability holds.
    fn picker_button(&self, kind: PickerKind, theme: &Theme, entity: &Entity<Self>) -> Option<AnyElement> {
        if !self.picker_offered(kind) {
            return None;
        }
        let (id, selector): (&'static str, &'static str) = match kind {
            PickerKind::Reviewers => ("change-request-reviewers-picker", "change-request-reviewers-add"),
            PickerKind::Labels => ("change-request-labels-picker", "change-request-labels-add"),
        };
        let disabled = self.action_busy();
        let open = self.picker_is(kind);
        let (opener, closer, body) = (entity.clone(), entity.clone(), entity.clone());
        let theme = *theme;
        Some(
            div()
                .debug_selector(move || selector.to_owned())
                .child(
                    Popover::with_opener(
                        id,
                        move |_toggle| {
                            IconButton::new(SharedString::from(format!("{id}-plus")), IconName::Plus)
                                .tooltip(match kind {
                                    PickerKind::Reviewers => "Change reviewers",
                                    PickerKind::Labels => "Change labels",
                                })
                                .disabled(disabled)
                                // The popover follows the tab's picker (`open` below):
                                // a click opens or closes the picker, not the panel.
                                .on_click(move |_, window, cx| {
                                    opener.update(cx, |tab, cx| {
                                        if tab.picker_is(kind) {
                                            let _ = tab.close_picker(cx);
                                        } else {
                                            let _ = tab.open_picker(kind, window, cx);
                                        }
                                    })
                                })
                        },
                        move |_, window, cx| body.update(cx, |tab, cx| tab.picker_body(&theme, &body, window, cx)),
                    )
                    // A picker opened over the control socket shows its panel too.
                    .open(open)
                    // Escape, a press outside or focus leaving: the panel
                    // closed, so this kind's picker sends what changed.
                    .on_close(move |_, cx| {
                        closer.update(cx, |tab, cx| {
                            if tab.picker_is(kind) {
                                let _ = tab.close_picker(cx);
                            }
                        })
                    }),
                )
                .into_any_element(),
        )
    }

    /// The control socket's picker verbs (debug builds only, as every write).
    pub(crate) fn control_picker(
        &mut self,
        name: &str,
        params: &BTreeMap<String, String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        match name {
            "picker-open" => {
                let kind = match params.get("kind").map(String::as_str) {
                    Some("reviewers") => PickerKind::Reviewers,
                    Some("labels") => PickerKind::Labels,
                    _ => return Err("picker-open needs kind: reviewers or labels".to_string()),
                };
                self.open_picker(kind, window, cx)
            }
            "picker-type" => {
                let text = params.get("text").cloned().unwrap_or_default();
                let picker = self.actions.picker.as_ref().ok_or("no picker is open")?;
                let query = picker.query.clone();
                query.update(cx, |input, cx| input.set_text(text.clone(), cx));
                self.type_in_picker(text, cx);
                if params.get("now").map(String::as_str) == Some("yes") {
                    let generation = self.actions.picker.as_ref().map_or(0, |picker| picker.generation);
                    if let Some(picker) = self.actions.picker.as_mut() {
                        picker.debounce = None;
                        picker.found = None;
                    }
                    self.search_candidates(generation, cx);
                }
                Ok(())
            }
            "picker-pick" => {
                let id = params.get("id").cloned().ok_or("picker-pick needs id")?;
                if self.actions.picker.is_none() {
                    return Err("no picker is open".to_string());
                }
                self.toggle_in_picker(id, cx);
                Ok(())
            }
            "picker-close" => self.close_picker(cx),
            other => Err(format!("unknown action {other}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use gpui::TestAppContext;
    use serde_json::json;

    use super::*;
    use crate::forge_source::testing::{self, CannedForge, FakeSource};

    fn ids(list: &[&str]) -> BTreeSet<String> {
        list.iter().map(|id| id.to_string()).collect()
    }

    #[test]
    fn the_difference_is_what_changed_and_nothing_when_nothing_did() {
        assert_eq!(difference(&ids(&["a", "b"]), &ids(&["a", "b"])), (vec![], vec![]));
        assert_eq!(difference(&ids(&["a"]), &ids(&["b"])), (vec!["b".to_string()], vec!["a".to_string()]));
        assert_eq!(difference(&ids(&[]), &ids(&[])), (vec![], vec![]));
    }

    #[test]
    fn the_current_set_stays_listed_when_a_search_does_not_return_it() {
        let current = BTreeMap::from([("U1".to_string(), "carol".to_string())]);
        let found = vec![Candidate { id: "U2".into(), label: "ann".into(), note: Some("Ann Lee".into()) }];
        let values: Vec<String> = picker_choices(&current, &found).into_iter().map(|choice| choice.value.to_string()).collect();
        assert_eq!(values, vec!["U1", "U2"]);
        let again = vec![Candidate { id: "U1".into(), label: "carol".into(), note: None }];
        assert_eq!(picker_choices(&current, &again).len(), 1, "one row per id");
    }

    fn forge() -> Arc<CannedForge> {
        let forge = Arc::new(CannedForge::default());
        forge.answer("Viewer", testing::viewer());
        let mut header: serde_json::Value =
            serde_json::from_str(&testing::header(101, "Fix", "Body")).expect("header JSON");
        header["data"]["repository"]["pullRequest"]["viewerCanUpdate"] = json!(true);
        forge.answer("ChangeRequestHeader", header.to_string());
        forge.answer(
            "ReviewerCandidates",
            json!({"data": {"repository": {"pullRequest": {"author": {"login": "alice"}},
                "assignableUsers": {"nodes": [{"id": "U_ann", "login": "ann", "name": "Ann Lee"}]}}}})
            .to_string(),
        );
        forge
    }

    fn opened(cx: &mut TestAppContext, forge: Arc<CannedForge>) -> (Entity<ChangeRequestTab>, &mut gpui::VisualTestContext) {
        cx.update(Theme::init);
        cx.update(|cx| forge_source::set_source(FakeSource::ready(testing::github_client(forge), None), cx));
        let (tab, cx) = cx.add_window_view(|_, cx| {
            ChangeRequestTab::new(testing::reference(101), String::new(), std::env::temp_dir(), cx)
        });
        tab.update(cx, |tab, cx| tab.on_selected(cx));
        cx.run_until_parked();
        assert!(tab.read_with(cx, |tab, _| tab.header.value().is_some()), "the header did not load");
        tab.update_in(cx, |tab, window, cx| tab.open_picker(PickerKind::Reviewers, window, cx))
            .expect("the picker opened");
        cx.run_until_parked();
        (tab, cx)
    }

    #[gpui::test]
    fn typing_asks_the_forge_after_the_debounce_and_not_before(cx: &mut TestAppContext) {
        let forge = forge();
        let (tab, cx) = opened(cx, forge.clone());
        let opening = forge.count("ReviewerCandidates");
        assert_eq!(opening, 1, "opening asks once, at once");
        tab.update(cx, |tab, cx| tab.type_in_picker("a".into(), cx));
        cx.executor().advance_clock(SEARCH_DEBOUNCE / 2);
        tab.update(cx, |tab, cx| tab.type_in_picker("an".into(), cx));
        cx.executor().advance_clock(SEARCH_DEBOUNCE - Duration::from_millis(1));
        cx.run_until_parked();
        assert_eq!(forge.count("ReviewerCandidates"), opening, "asked before the debounce ran out");
        cx.executor().advance_clock(Duration::from_millis(1));
        cx.run_until_parked();
        assert_eq!(forge.count("ReviewerCandidates"), opening + 1, "one read for the last text");
        assert_eq!(tab.read_with(cx, |tab, _| tab.picker_candidates()), 1);
    }

    #[gpui::test]
    fn a_search_with_no_candidates_draws_a_line_not_an_empty_list(cx: &mut TestAppContext) {
        let forge = forge();
        forge.answer(
            "ReviewerCandidates",
            json!({"data": {"repository": {"pullRequest": {"author": {"login": "alice"}},
                "assignableUsers": {"nodes": []}}}})
            .to_string(),
        );
        cx.update(Theme::init);
        cx.update(|cx| forge_source::set_source(FakeSource::ready(testing::github_client(forge), None), cx));
        let (tab, cx) = cx.add_window_view(|_, cx| {
            ChangeRequestTab::new(testing::reference(101), String::new(), std::env::temp_dir(), cx)
        });
        tab.update(cx, |tab, cx| tab.on_selected(cx));
        cx.run_until_parked();
        // The real click: the popover opens and draws its panel, which with
        // nothing to list must not reach Ely's ListBox (it panics on none).
        let plus = cx.debug_bounds("change-request-reviewers-add").expect("the + button").center();
        cx.simulate_click(plus, gpui::Modifiers::none());
        cx.run_until_parked();
        assert!(cx.debug_bounds("change-request-picker").is_some(), "the picker's panel was not drawn");
        assert_eq!(tab.read_with(cx, |tab, _| tab.picker_candidates()), 0);
    }

    /// The rows the open picker lists, as ids.
    fn listed(tab: &Entity<ChangeRequestTab>, cx: &mut gpui::VisualTestContext) -> Vec<String> {
        tab.read_with(cx, |tab, _| {
            let picker = tab.actions.picker.as_ref().expect("a picker is open");
            picker_choices(&picker.current, picker.found.as_deref().unwrap_or(&[]))
                .into_iter()
                .map(|choice| choice.value.to_string())
                .collect()
        })
    }

    /// A click on row `id`, as Ely's `ListBox` reports it (`check::toggled`):
    /// only the rows it shows, the ticked ones, with `id` toggled.
    fn click_row(tab: &Entity<ChangeRequestTab>, id: &str, cx: &mut gpui::VisualTestContext) {
        let chosen = tab.read_with(cx, |tab, _| tab.actions.picker.as_ref().expect("a picker is open").chosen.clone());
        let values = listed(tab, cx)
            .into_iter()
            .filter(|row| if row == id { !chosen.contains(id) } else { chosen.contains(row) })
            .collect();
        tab.update(cx, |tab, cx| tab.choose_in_picker(values, cx));
    }

    #[gpui::test]
    fn a_pick_survives_the_next_search(cx: &mut TestAppContext) {
        let forge = forge();
        let (tab, cx) = opened(cx, forge.clone());
        click_row(&tab, "U_ann", cx);
        forge.answer(
            "ReviewerCandidates",
            json!({"data": {"repository": {"pullRequest": {"author": {"login": "alice"}},
                "assignableUsers": {"nodes": [{"id": "U_bob", "login": "bob", "name": null}]}}}})
            .to_string(),
        );
        tab.update(cx, |tab, cx| tab.type_in_picker("bob".into(), cx));
        cx.executor().advance_clock(SEARCH_DEBOUNCE);
        cx.run_until_parked();
        assert!(listed(&tab, cx).contains(&"U_ann".to_string()), "a pick vanished from the list");
        click_row(&tab, "U_bob", cx);
        let chosen = tab.read_with(cx, |tab, _| tab.actions.picker.as_ref().expect("open").chosen.clone());
        assert_eq!(chosen, ids(&["U_ann", "U_bob"]), "picks across two searches");
    }

    #[gpui::test]
    fn a_picker_neither_opens_nor_sends_while_another_write_is_in_flight(cx: &mut TestAppContext) {
        let forge = forge();
        let (tab, cx) = opened(cx, forge.clone());
        click_row(&tab, "U_ann", cx);
        tab.update(cx, |tab, _| tab.actions.state = ActionState::Working("comment"));
        let closed = tab.update(cx, |tab, cx| tab.close_picker(cx));
        assert!(closed.is_err(), "a pick was sent beside a write in flight");
        tab.read_with(cx, |tab, _| {
            assert!(matches!(tab.actions.state, ActionState::Working("comment")), "the write in flight lost its state");
            assert!(tab.picker_is(PickerKind::Reviewers), "the picks were thrown away");
        });
        let opened = tab.update_in(cx, |tab, window, cx| tab.open_picker(PickerKind::Labels, window, cx));
        assert!(opened.is_err(), "a picker opened beside a write in flight");
        assert!(tab.read_with(cx, |tab, _| tab.picker_is(PickerKind::Reviewers)), "the open picker was closed");
    }

    #[gpui::test]
    fn closing_without_a_change_sends_nothing(cx: &mut TestAppContext) {
        let forge = forge();
        forge.answer("ChangeRequestActionContext", "{}".to_string());
        let (tab, cx) = opened(cx, forge.clone());
        tab.update(cx, |tab, cx| tab.toggle_in_picker("U_ann".into(), cx));
        tab.update(cx, |tab, cx| tab.toggle_in_picker("U_ann".into(), cx));
        tab.update(cx, |tab, cx| {
            let _ = tab.close_picker(cx);
        });
        cx.run_until_parked();
        assert_eq!(forge.count("ChangeRequestActionContext"), 0, "a picker closed unchanged wrote");
        assert!(tab.read_with(cx, |tab, _| tab.actions.picker.is_none()));
    }
}
