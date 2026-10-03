//! Merging (spec §6): the strip between the header and the inner tabs, and
//! the confirmation dialog — the only confirmation of a write in the app.

use std::collections::BTreeMap;

use ely_gpui_component::forms::{Checkbox, Choice, Select, TextInput};
use ely_gpui_component::overlays::Dialog;
use ely_gpui_component::primitives::Severity;
use sirio_forge::{Action, ChangeState, MergeCapability, MergeMethod, MergeVerdict};

use super::ely_ui::{ButtonState, message, new_input, normalize, text_button};
use super::*;

/// What the strip offers, from the change request's state and the forge's
/// verdict.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum StripKind {
    /// No strip: merged, closed, or the forge did not say.
    None,
    Merge,
    /// Checks are running and the repository merges when they pass.
    AutoMerge,
    /// Checks are running and nothing can wait for them.
    Waiting,
    Blocked(String),
    /// An auto-merge is set, with the method it will use.
    Cancel(MergeMethod),
}

impl StripKind {
    /// The word the control socket reports.
    pub(crate) fn word(&self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Merge => "merge",
            Self::AutoMerge => "auto-merge",
            Self::Waiting => "waiting",
            Self::Blocked(_) => "blocked",
            Self::Cancel(_) => "cancel",
        }
    }

    pub(crate) fn text(&self) -> String {
        match self {
            Self::None => String::new(),
            Self::Merge => "Ready to merge".to_string(),
            Self::AutoMerge | Self::Waiting => "Waiting for checks".to_string(),
            Self::Blocked(reason) => format!("Blocked: {reason}"),
            Self::Cancel(method) => format!("Auto-merge enabled · {}", method.word()),
        }
    }

    fn severity(&self) -> Severity {
        match self {
            Self::Merge => Severity::Success,
            Self::Blocked(_) => Severity::Warning,
            _ => Severity::Info,
        }
    }
}

pub(crate) fn strip_kind(state: ChangeState, merge: &MergeCapability) -> StripKind {
    if !matches!(state, ChangeState::Open | ChangeState::Draft) {
        return StripKind::None;
    }
    if let Some(method) = merge.auto_merge_enabled {
        return StripKind::Cancel(method);
    }
    match &merge.verdict {
        MergeVerdict::Unreported => StripKind::None,
        MergeVerdict::Ready => StripKind::Merge,
        MergeVerdict::WaitingOnChecks if merge.can_auto_merge => StripKind::AutoMerge,
        MergeVerdict::WaitingOnChecks => StripKind::Waiting,
        MergeVerdict::Blocked(reason) => StripKind::Blocked(reason.text()),
    }
}

/// The methods the repository allows, as the strip's choices.
pub(crate) fn merge_choices(merge: &MergeCapability) -> Vec<Choice> {
    merge
        .methods
        .list()
        .into_iter()
        .map(|method| Choice::new(method.word(), method.label()))
        .collect()
}

/// The commit title the dialog opens with.
pub(crate) fn preset_title(method: MergeMethod, title: &str, number: u64) -> String {
    match method {
        MergeMethod::Squash => format!("{title} (#{number})"),
        MergeMethod::Merge | MergeMethod::Rebase => title.to_string(),
    }
}

fn method_of(word: &str) -> Option<MergeMethod> {
    [MergeMethod::Merge, MergeMethod::Squash, MergeMethod::Rebase]
        .into_iter()
        .find(|method| method.word() == word)
}

/// A merge being confirmed. `head` is the sha the user saw when the dialog
/// opened: the merge is refused if the branch moved since.
pub(crate) struct MergeDialog {
    pub(crate) method: MergeMethod,
    pub(crate) when_checks_pass: bool,
    pub(crate) title: Entity<TextInput>,
    /// `None` for a rebase, which writes no commit.
    pub(crate) message: Option<Entity<TextInput>>,
    pub(crate) delete_branch: bool,
    pub(crate) offer_delete: bool,
    pub(crate) head: String,
    pub(crate) target: String,
}

#[derive(Default)]
pub(crate) struct MergeUi {
    /// The method chosen in the strip; `None` follows the forge's default.
    pub(crate) method: Option<MergeMethod>,
    pub(crate) dialog: Option<MergeDialog>,
}

impl ChangeRequestTab {
    pub(crate) fn strip(&self) -> StripKind {
        self.header.value().map_or(StripKind::None, |header| {
            strip_kind(header.summary.state, &header.capabilities.merge)
        })
    }

    /// The method the strip shows: the chosen one while the repository still
    /// allows it, else the forge's default.
    pub(crate) fn merge_method(&self) -> Option<MergeMethod> {
        let merge = &self.header.value()?.capabilities.merge;
        self.actions
            .merge
            .method
            .filter(|method| merge.methods.contains(*method))
            .or(merge.default_method)
    }

    pub(crate) fn merge_verdict_word(&self) -> String {
        match self.header.value().map(|header| &header.capabilities.merge.verdict) {
            None => "none".to_string(),
            Some(MergeVerdict::Unreported) => "unreported".to_string(),
            Some(MergeVerdict::Ready) => "ready".to_string(),
            Some(MergeVerdict::WaitingOnChecks) => "waiting".to_string(),
            Some(MergeVerdict::Blocked(reason)) => format!("blocked:{}", reason.text()),
        }
    }

    /// Opens the confirmation; refused unless the strip offers a merge.
    pub(crate) fn open_merge_dialog(
        &mut self,
        method: Option<MergeMethod>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        if self.action_busy() {
            return Err("an action is already in flight".to_string());
        }
        let when_checks_pass = match self.strip() {
            StripKind::Merge => false,
            StripKind::AutoMerge => true,
            other => return Err(format!("this change request cannot be merged now ({})", other.word())),
        };
        let header = self.header.value().ok_or("the change request is not loaded")?;
        let merge = &header.capabilities.merge;
        let method = method
            .or(self.merge_method())
            .filter(|method| merge.methods.contains(*method))
            .ok_or("the repository does not allow that merge method")?;
        let head = header
            .revisions
            .as_ref()
            .map(|revisions| revisions.head_sha.clone())
            .ok_or("the forge did not say which commit is the head")?;
        let title = preset_title(method, &header.summary.title, self.reference.number);
        let target = header.summary.target_branch.clone();
        let delete_branch = merge.delete_branch_default;
        // GitHub's auto-merge has no delete flag: the repository's own
        // setting decides (plan ruling 4).
        let offer_delete = !(when_checks_pass && self.reference.forge == Forge::GitHub);
        let title = new_input(window, cx, &title, None, "Commit title");
        let message = (method != MergeMethod::Rebase)
            .then(|| new_input(window, cx, "", Some((3, 10)), "Commit message"));
        self.actions.merge.method = Some(method);
        self.actions.merge.dialog = Some(MergeDialog {
            method,
            when_checks_pass,
            title,
            message,
            delete_branch,
            offer_delete,
            head,
            target,
        });
        cx.notify();
        Ok(())
    }

    pub(crate) fn close_merge_dialog(&mut self, cx: &mut Context<Self>) {
        self.actions.merge.dialog = None;
        cx.notify();
    }

    /// Sends the merge the dialog describes, with the head it opened on.
    pub(crate) fn confirm_merge(&mut self, cx: &mut Context<Self>) -> Result<(), String> {
        let dialog = self.actions.merge.dialog.as_ref().ok_or("no merge is being confirmed")?;
        let action = Action::Merge {
            method: dialog.method,
            commit_title: (dialog.method != MergeMethod::Rebase)
                .then(|| normalize(dialog.title.read(cx).text(), false)),
            commit_message: dialog
                .message
                .as_ref()
                .map(|input| normalize(input.read(cx).text(), true)),
            delete_branch: dialog.offer_delete && dialog.delete_branch,
            when_checks_pass: dialog.when_checks_pass,
            expected_head: dialog.head.clone(),
        };
        self.perform(action, cx)
    }

    pub(crate) fn cancel_auto_merge(&mut self, cx: &mut Context<Self>) -> Result<(), String> {
        self.perform(Action::CancelAutoMerge, cx)
    }

    /// The strip: the verdict on the left, the method and the button on the
    /// right. No strip for a merged or closed change request.
    pub(crate) fn render_merge_strip(&self, theme: &Theme, entity: &Entity<Self>) -> Option<AnyElement> {
        let kind = self.strip();
        if kind == StripKind::None {
            return None;
        }
        let header = self.header.value()?;
        let busy = self.action_busy();
        let working = |kinds: &[&str]| kinds.contains(&self.actions.state.kind());
        let controls = match &kind {
            StripKind::Cancel(_) => {
                let entity = entity.clone();
                div().child(text_button(
                    "change-request-merge-cancel",
                    "Cancel auto-merge",
                    None,
                    ButtonState::enabled(!busy).loading(working(&["cancel-auto-merge"])),
                    move |_, cx| {
                        entity.update(cx, |tab, cx| {
                            let _ = tab.cancel_auto_merge(cx);
                        })
                    },
                ))
            }
            _ => {
                let choices = merge_choices(&header.capabilities.merge);
                let offers = matches!(kind, StripKind::Merge | StripKind::AutoMerge);
                let label = if matches!(kind, StripKind::AutoMerge | StripKind::Waiting) {
                    "Merge when checks pass"
                } else {
                    "Merge"
                };
                let select = (!choices.is_empty()).then(|| {
                    let pick = entity.clone();
                    let mut select = Select::new("change-request-merge-method", choices)
                        .disabled(busy || !offers)
                        .on_change(move |value, _, cx| {
                            let method = method_of(value);
                            pick.update(cx, |tab, cx| {
                                tab.actions.merge.method = method;
                                cx.notify();
                            });
                        });
                    if let Some(method) = self.merge_method() {
                        select = select.selected(method.word());
                    }
                    div()
                        .id("change-request-merge-method")
                        .debug_selector(|| "change-request-merge-method".to_owned())
                        .w(px(180.0))
                        .child(select)
                });
                let open = entity.clone();
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .children(select)
                    .child(text_button(
                        "change-request-merge-button",
                        label,
                        None,
                        ButtonState::enabled(offers && !busy)
                            .primary()
                            .loading(working(&["merge", "auto-merge"])),
                        move |window, cx| {
                            open.update(cx, |tab, cx| {
                                let _ = tab.open_merge_dialog(None, window, cx);
                            })
                        },
                    ))
            }
        };
        Some(
            div()
                .id("change-request-merge-strip")
                .debug_selector(|| "change-request-merge-strip".to_owned())
                .flex_none()
                .flex()
                .flex_wrap()
                .items_center()
                .justify_between()
                .gap(px(8.0))
                .px(px(16.0))
                .py(px(8.0))
                .border_t_1()
                .border_b_1()
                .border_color(theme.ely.border)
                .child(div().min_w_0().child(message(kind.severity(), kind.text(), theme)))
                .child(controls.flex_none())
                .into_any_element(),
        )
    }

    /// The confirmation, drawn over the tab while a merge is being confirmed.
    pub(crate) fn render_merge_dialog(&self, entity: &Entity<Self>) -> Option<AnyElement> {
        let dialog = self.actions.merge.dialog.as_ref()?;
        let busy = self.action_busy();
        let short = &dialog.head[..dialog.head.len().min(7)];
        let close = entity.clone();
        let label = if dialog.when_checks_pass { "Merge when checks pass" } else { "Merge" };
        let confirm = entity.clone();
        let mut card = Dialog::new(
            "change-request-merge-dialog",
            format!("Merge #{}", self.reference.number),
            move |_, cx| close.update(cx, |tab, cx| tab.close_merge_dialog(cx)),
        )
        .detail(format!(
            "{} into {} · head {short}",
            dialog.method.label(),
            dialog.target
        ))
        .action(|close| {
            text_button(
                "change-request-merge-dialog-cancel",
                "Cancel",
                None,
                ButtonState::IDLE,
                move |window, cx| close(window, cx),
            )
        })
        .action(move |_| {
            text_button(
                "change-request-merge-confirm",
                label,
                None,
                ButtonState::enabled(!busy).primary().loading(busy),
                move |_, cx| {
                    confirm.update(cx, |tab, cx| {
                        let _ = tab.confirm_merge(cx);
                    })
                },
            )
        })
        .child(dialog.title.clone());
        if let Some(message) = &dialog.message {
            card = card.child(message.clone());
        }
        if dialog.offer_delete {
            let toggle = entity.clone();
            card = card.child(
                Checkbox::new("change-request-merge-delete", dialog.delete_branch)
                    .label("Delete branch")
                    .disabled(busy)
                    .on_change(move |on, _, cx| {
                        toggle.update(cx, |tab, cx| {
                            if let Some(dialog) = tab.actions.merge.dialog.as_mut() {
                                dialog.delete_branch = on;
                            }
                            cx.notify();
                        })
                    }),
            );
        }
        Some(
            div()
                .debug_selector(|| "change-request-merge-dialog".to_owned())
                .child(card)
                .into_any_element(),
        )
    }

    /// The control socket's merge verbs (debug builds only, as every write).
    pub(crate) fn control_merge(
        &mut self,
        name: &str,
        params: &BTreeMap<String, String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        match name {
            "merge-open" => {
                let method = match params.get("method") {
                    Some(word) => Some(method_of(word).ok_or_else(|| format!("unknown method {word}"))?),
                    None => None,
                };
                self.open_merge_dialog(method, window, cx)
            }
            "merge-confirm" => {
                let dialog = self.actions.merge.dialog.as_mut().ok_or("no merge is being confirmed")?;
                if let Some(title) = params.get("title") {
                    let title = title.clone();
                    dialog.title.update(cx, |input, cx| input.set_text(title, cx));
                }
                if let (Some(text), Some(input)) = (params.get("message"), dialog.message.as_ref()) {
                    let text = text.clone();
                    input.update(cx, |input, cx| input.set_text(text, cx));
                }
                if let Some(delete) = params.get("delete") {
                    dialog.delete_branch = delete == "yes";
                }
                self.confirm_merge(cx)
            }
            "merge-close" => {
                self.close_merge_dialog(cx);
                Ok(())
            }
            "cancel-auto-merge" => self.cancel_auto_merge(cx),
            other => Err(format!("unknown action {other}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sirio_forge::{BlockReason, MergeMethods};

    fn ready() -> MergeCapability {
        MergeCapability {
            verdict: MergeVerdict::Ready,
            methods: MergeMethods { merge: true, squash: true, rebase: false },
            default_method: Some(MergeMethod::Merge),
            can_auto_merge: true,
            auto_merge_enabled: None,
            delete_branch_default: false,
        }
    }

    #[test]
    fn the_strip_follows_the_verdict() {
        let kind = |state, merge: &MergeCapability| strip_kind(state, merge).word();
        assert_eq!(kind(ChangeState::Open, &ready()), "merge");
        let waiting = MergeCapability { verdict: MergeVerdict::WaitingOnChecks, ..ready() };
        assert_eq!(kind(ChangeState::Open, &waiting), "auto-merge");
        let cannot_wait = MergeCapability { can_auto_merge: false, ..waiting.clone() };
        assert_eq!(kind(ChangeState::Open, &cannot_wait), "waiting");
        let blocked = MergeCapability { verdict: MergeVerdict::Blocked(BlockReason::ReviewRequired), ..ready() };
        assert_eq!(kind(ChangeState::Open, &blocked), "blocked");
        let on = MergeCapability { auto_merge_enabled: Some(MergeMethod::Squash), ..waiting };
        assert_eq!(kind(ChangeState::Open, &on), "cancel");
        assert_eq!(kind(ChangeState::Open, &MergeCapability::default()), "none", "unreported offers nothing");
        assert_eq!(kind(ChangeState::Merged, &ready()), "none");
        assert_eq!(kind(ChangeState::Closed, &ready()), "none");
    }

    #[test]
    fn a_blocked_strip_names_its_reason() {
        let blocked = MergeCapability { verdict: MergeVerdict::Blocked(BlockReason::ReviewRequired), ..ready() };
        assert_eq!(strip_kind(ChangeState::Open, &blocked).text(), "Blocked: a review is required");
    }

    #[test]
    fn the_method_choice_is_only_what_the_repository_allows() {
        let values: Vec<String> = merge_choices(&ready()).into_iter().map(|choice| choice.value.to_string()).collect();
        assert_eq!(values, vec!["merge", "squash"]);
    }

    #[test]
    fn a_squash_title_names_the_change_request() {
        assert_eq!(preset_title(MergeMethod::Squash, "Fix login", 101), "Fix login (#101)");
        assert_eq!(preset_title(MergeMethod::Merge, "Fix login", 101), "Fix login");
    }
}
