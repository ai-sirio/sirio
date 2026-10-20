//! The Change request card: the pull or merge request of the chat's own
//! branch, one row that opens its tab. Hidden when the worktree is on no
//! forge, on a detached HEAD with no link, or when nothing installed a
//! `ChangeRequestSource`.
//!
//! The card reads a cached answer; nothing here blocks. A read is one task:
//! `connect` (it runs git), a look at the host's shared pause on the GPUI
//! thread, then `read_branch_card`, each hop off the GPUI thread. It runs
//! when the column opens or the card unfolds (if the last answer is a
//! minute old) and then every minute while the card is open; closing the
//! column or folding the card drops the timer and any read in flight.

use super::{Row, Section, environment};
use crate::branch_card::{BranchCard, BranchCardRead, read_branch_card};
use crate::change_request_style as style;
use crate::chat::{Chat, ChatEvent};
use crate::forge_pause;
use crate::forge_source::{self, Connection};
use gpui::{AppContext as _, Context as GpuiContext, Task};
use sirio_forge::{ChangeSummary, Forge, ForgeError, ReviewState};
use std::time::Duration;

/// How often the forge is asked while the card is open, and how old an
/// answer may be before opening the column asks again.
const READ_EVERY: Duration = Duration::from_secs(60);
const STALE_SECS: i64 = 60;

pub(super) enum Shown {
    Found(Box<ChangeSummary>),
    /// The branch has no change request; the forge names the noun.
    Missing(Forge),
}

#[derive(Default)]
pub(super) struct ChangeRequest {
    /// The last answer, kept while a later read fails or is paused. `None`
    /// is "nothing to show", which with no `error` hides the card.
    shown: Option<Shown>,
    /// Why the latest read gave no answer: shown under the last one.
    error: Option<ForgeError>,
    /// The host the last connection named, so a paused host is skipped
    /// before `connect` runs git for nothing.
    host: Option<String>,
    /// Unix seconds of the last answer.
    last_read: Option<i64>,
    /// Counts the reads started, so an answer to an older one is dropped.
    generation: u64,
    read_task: Option<Task<()>>,
    timer_task: Option<Task<()>>,
}

fn noun(forge: Forge) -> &'static str {
    match forge {
        Forge::GitHub => "pull request",
        Forge::GitLab => "merge request",
    }
}

/// `Open · CI passed · Approved`: the facts the right panel's row wears,
/// in words.
fn detail(summary: &ChangeSummary) -> String {
    let mut parts = vec![style::state_label(summary.state).to_string(), style::ci_text(summary.ci)];
    if summary.review_requested_from_me {
        parts.push("Review requested".to_string());
    } else {
        match summary.review {
            ReviewState::Approved { .. } => parts.push("Approved".to_string()),
            ReviewState::ChangesRequested => parts.push("Changes requested".to_string()),
            ReviewState::ReviewRequired => parts.push("Review required".to_string()),
            ReviewState::None => {}
        }
    }
    parts.join(" · ")
}

pub(super) fn section(state: &ChangeRequest) -> Option<Section> {
    let mut rows = Vec::new();
    match &state.shown {
        Some(Shown::Found(summary)) => rows.push(Row {
            icon: Some(style::state_icon(summary.state)),
            action: Some("open".to_string()),
            ..Row::new(
                "change_request/card",
                format!("{} {}", summary.reference.label(), summary.title),
                &detail(summary),
            )
        }),
        Some(Shown::Missing(forge)) => rows.push(Row::new(
            "change_request/none",
            format!("No {} for this branch", noun(*forge)),
            "",
        )),
        None => {}
    }
    if let Some(error) = &state.error {
        // A rate limit fits the line; any other failure's sentence wraps.
        let row = match error {
            ForgeError::RateLimited { .. } => {
                Row::new("change_request/error", crate::right_panel::error_text(error), "")
            }
            _ => Row {
                note: crate::right_panel::error_text(error),
                ..Row::new(
                    "change_request/error",
                    "Could not read the change request".to_string(),
                    "",
                )
            },
        };
        rows.push(Row { muted: true, ..row });
    }
    if rows.is_empty() {
        return None;
    }
    Some(Section {
        key: "change_request",
        title: "Change request",
        summary: String::new(),
        note: String::new(),
        bar: Vec::new(),
        rows,
    })
}

impl Chat {
    /// The column opened or the card unfolded: reads when the last answer is
    /// old, and keeps reading every minute until stopped.
    pub(in crate::chat) fn start_session_change_request(&mut self, cx: &mut GpuiContext<Self>) {
        if !self.session_panel.open || self.session_panel.collapsed.contains("change_request") {
            return;
        }
        let stale = self
            .session_panel
            .change_request
            .last_read
            .is_none_or(|at| style::now() - at >= STALE_SECS);
        if stale {
            self.read_session_change_request(cx);
        }
        self.session_panel.change_request.timer_task = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(READ_EVERY).await;
                let alive = this.update(cx, |chat, cx| {
                    // A narrow chat keeps the column undrawn: nothing to feed.
                    if chat.session_panel_drawn() {
                        chat.read_session_change_request(cx);
                    }
                });
                if alive.is_err() {
                    break;
                }
            }
        }));
    }

    /// The column closed or the card folded: no timer, no read in flight, and
    /// an answer already on its way is dropped.
    pub(in crate::chat) fn stop_session_change_request(&mut self) {
        let state = &mut self.session_panel.change_request;
        state.generation += 1;
        state.timer_task = None;
        state.read_task = None;
    }

    fn read_session_change_request(&mut self, cx: &mut GpuiContext<Self>) {
        let Some(source) = forge_source::source(cx) else {
            return;
        };
        if let Some(host) = self.session_panel.change_request.host.clone()
            && let Some(until) = forge_pause::paused_until(&host, style::now(), cx)
        {
            self.hold_session_change_request(host, until, cx);
            return;
        }
        let root = self
            .session_panel
            .environment
            .worktree
            .as_ref()
            .map_or_else(|| self.agent_cwd.clone(), |worktree| worktree.root.clone());
        let state = &mut self.session_panel.change_request;
        state.generation += 1;
        let generation = state.generation;
        state.read_task = Some(cx.spawn(async move |this, cx| {
            let connecting = source.clone();
            let connecting_root = root.clone();
            let connection = cx
                .background_spawn(async move { connecting.connect(&connecting_root) })
                .await;
            let Connection::Ready(ready) = connection else {
                // No forge here: nothing to show, and nothing to read.
                let _ = this.update(cx, |chat, cx| {
                    chat.land_session_change_request(generation, None, cx);
                });
                return;
            };
            let host = ready.client.host().to_string();
            let forge = ready.client.forge();
            // The pause is the host's and is kept on the GPUI thread; it is
            // looked at again here because the host was unknown until now.
            let go = this.update(cx, |chat, cx| {
                let state = &mut chat.session_panel.change_request;
                if state.generation != generation {
                    return false;
                }
                state.host = Some(host.clone());
                match forge_pause::paused_until(&host, style::now(), cx) {
                    Some(until) => {
                        chat.hold_session_change_request(host, until, cx);
                        false
                    }
                    None => true,
                }
            });
            if !matches!(go, Ok(true)) {
                return;
            }
            let read = cx
                .background_spawn(async move { read_branch_card(&*source, &root, &ready) })
                .await;
            let _ = this.update(cx, |chat, cx| {
                chat.land_session_change_request(generation, Some((forge, read)), cx);
            });
        }));
    }

    /// A paused host is not asked: the card keeps its last answer and says
    /// when the forge is looked at again.
    fn hold_session_change_request(&mut self, host: String, until: i64, cx: &mut GpuiContext<Self>) {
        self.session_panel.change_request.error = Some(ForgeError::RateLimited {
            host,
            reset_at: Some(until),
        });
        cx.notify();
    }

    /// Takes a read's answer, or `None` when the worktree has no forge.
    fn land_session_change_request(
        &mut self,
        generation: u64,
        answer: Option<(Forge, BranchCardRead)>,
        cx: &mut GpuiContext<Self>,
    ) {
        let state = &mut self.session_panel.change_request;
        if state.generation != generation {
            return;
        }
        state.last_read = Some(style::now());
        let Some((forge, read)) = answer else {
            state.shown = None;
            state.error = None;
            cx.notify();
            return;
        };
        let BranchCardRead {
            card: mut result,
            mut linked_error,
            branch,
        } = read;
        // Both failures of a read reach the host's shared pause.
        for error in linked_error.iter_mut().chain(result.as_mut().err()) {
            forge_pause::note_rate_limited(error, style::now(), cx);
        }
        // The Environment line reads the same branch, so the two never
        // disagree; a detached HEAD leaves it to its own read, which knows
        // the short hash.
        if let Some(branch) = branch {
            self.session_panel.environment.branch = environment::Branch::Named(branch);
        }
        let state = &mut self.session_panel.change_request;
        match result {
            Ok(card) => {
                state.error = None;
                state.shown = match card {
                    BranchCard::Hidden => None,
                    BranchCard::Found(summary) => Some(Shown::Found(Box::new(summary))),
                    BranchCard::Missing { .. } => Some(Shown::Missing(forge)),
                };
            }
            Err(error) => state.error = Some(error),
        }
        cx.notify();
    }

    /// Runs the card's action: opens the change request's tab.
    pub(in crate::chat) fn activate_change_request(
        &mut self,
        key: &str,
        cx: &mut GpuiContext<Self>,
    ) -> Result<(), String> {
        let Some(Shown::Found(summary)) = &self.session_panel.change_request.shown else {
            return Err(format!("no row with an action has key {key}"));
        };
        if key != "change_request/card" {
            return Err(format!("no row with an action has key {key}"));
        }
        cx.emit(ChatEvent::OpenChangeRequest {
            reference: summary.reference.clone(),
            title: summary.title.clone(),
        });
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::super::{RowSnapshot, SectionSnapshot, SessionPanelAction};
    use crate::change_request_style as style;
    use crate::chat::tests::{offline_chat_view, pump_chat_until, refresh_frame};
    use crate::chat::{Chat, ChatEvent};
    use crate::forge_pause;
    use crate::forge_source::testing::{self, CannedForge, FakeSource};
    use crate::forge_source::{self, Connection};
    use gpui::{Entity, TestAppContext, VisualTestContext, px, size};
    use sirio_forge::ForgeError;
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::sync::Arc;

    fn forge_with(branch: Vec<serde_json::Value>) -> Arc<CannedForge> {
        let forge = Arc::new(CannedForge::default());
        forge.answer("Viewer", testing::viewer());
        forge.answer("ChangeRequestForBranch", testing::branch(branch));
        forge
    }

    /// A chat whose column is open against `source`.
    fn open_with(
        cx: &mut TestAppContext,
        source: Arc<FakeSource>,
    ) -> (Entity<Chat>, &mut VisualTestContext) {
        let (chat, cx) = offline_chat_view(cx);
        cx.update(|_, cx| forge_source::set_source(source, cx));
        cx.simulate_resize(size(px(1100.0), px(700.0)));
        chat.update(cx, |chat, cx| chat.set_session_panel_open(true, cx));
        (chat, cx)
    }

    fn section_of(chat: &Entity<Chat>, cx: &VisualTestContext, key: &str) -> Option<SectionSnapshot> {
        chat.read_with(&cx.cx, |chat, _| chat.control_snapshot())
            .session_panel
            .sections
            .into_iter()
            .find(|section| section.key == key)
    }

    fn settle(chat: &Entity<Chat>, cx: &mut VisualTestContext) -> SectionSnapshot {
        pump_chat_until(cx, chat, |chat| {
            chat.control_snapshot()
                .session_panel
                .sections
                .iter()
                .any(|section| section.key == "change_request")
        });
        for _ in 0..3 {
            refresh_frame(cx);
        }
        section_of(chat, cx, "change_request").expect("a change request section")
    }

    fn only_row(section: &SectionSnapshot) -> &RowSnapshot {
        assert_eq!(section.rows.len(), 1, "{:?}", section.rows);
        &section.rows[0]
    }

    #[gpui::test]
    async fn the_branch_change_request_is_shown_and_opens_its_tab(cx: &mut TestAppContext) {
        let forge = forge_with(vec![testing::summary(112, "Fix login")]);
        let source = FakeSource::ready(testing::github_client(forge), Some("feat/login"));
        let (chat, cx) = open_with(cx, source);
        let section = settle(&chat, cx);
        assert_eq!(section.rows[0].label, "#112 Fix login");
        assert!(section.rows[0].detail.starts_with("Open"), "{:?}", section.rows[0]);
        assert_eq!(section.rows[0].action.as_deref(), Some("open"));

        let opened = Rc::new(RefCell::new(Vec::new()));
        let seen = opened.clone();
        cx.update(|_, cx| {
            cx.subscribe(&chat, move |_, event: &ChatEvent, _| {
                if let ChatEvent::OpenChangeRequest { reference, title } = event {
                    seen.borrow_mut().push((reference.number, title.clone()));
                }
            })
            .detach();
        });
        chat.update_in(cx, |chat, window, cx| {
            chat.session_panel_act(
                SessionPanelAction::Activate(section.rows[0].key.clone()),
                window,
                cx,
            )
        })
        .expect("the card opens");
        assert_eq!(*opened.borrow(), [(112, "Fix login".to_string())]);
    }

    #[gpui::test]
    async fn a_branch_without_one_says_so(cx: &mut TestAppContext) {
        let forge = forge_with(Vec::new());
        let source = FakeSource::ready(testing::github_client(forge), Some("feat/none"));
        let (chat, cx) = open_with(cx, source);
        let section = settle(&chat, cx);
        let row = only_row(&section);
        assert_eq!(row.label, "No pull request for this branch");
        assert!(row.action.is_none(), "there is nothing to open");
    }

    #[gpui::test]
    async fn a_paused_host_is_not_read(cx: &mut TestAppContext) {
        let forge = forge_with(vec![testing::summary(112, "Fix login")]);
        let source = FakeSource::ready(testing::github_client(forge.clone()), Some("feat/login"));
        cx.update(|cx| {
            let mut error = ForgeError::RateLimited {
                host: "ghe.test".to_string(),
                reset_at: None,
            };
            forge_pause::note_rate_limited(&mut error, style::now(), cx);
        });
        let (chat, cx) = open_with(cx, source);
        let section = settle(&chat, cx);
        assert_eq!(forge.count("ChangeRequestForBranch"), 0, "a paused host is not asked");
        assert!(only_row(&section).label.starts_with("Rate limited"), "{:?}", section.rows);
    }

    #[gpui::test]
    async fn no_forge_hides_the_section(cx: &mut TestAppContext) {
        let source = FakeSource::with(Connection::NoForgeRemote);
        let (chat, cx) = open_with(cx, source.clone());
        pump_chat_until(cx, &chat, |_| *source.connects.lock().unwrap() >= 1);
        for _ in 0..5 {
            refresh_frame(cx);
        }
        assert!(section_of(&chat, cx, "change_request").is_none());
        assert!(section_of(&chat, cx, "environment").is_some());
    }

    #[gpui::test(iterations = 8)]
    async fn the_live_branch_read_updates_the_environment_line(cx: &mut TestAppContext) {
        let forge = forge_with(vec![testing::summary(7, "Whatever")]);
        let source = FakeSource::ready(testing::github_client(forge), Some("feat/y"));
        let (chat, cx) = open_with(cx, source);
        settle(&chat, cx);
        for _ in 0..5 {
            refresh_frame(cx);
        }
        let environment = section_of(&chat, cx, "environment").expect("environment");
        assert!(
            environment.rows.iter().any(|row| row.label == "feat/y"),
            "the two sections read one branch: {:?}",
            environment.rows
        );
    }

    #[gpui::test]
    async fn the_card_is_read_every_minute_only_while_open(cx: &mut TestAppContext) {
        let forge = forge_with(vec![testing::summary(112, "Fix login")]);
        let source = FakeSource::ready(testing::github_client(forge.clone()), Some("feat/login"));
        let (chat, cx) = open_with(cx, source);
        settle(&chat, cx);
        assert_eq!(forge.count("ChangeRequestForBranch"), 1);

        let wait = |cx: &mut VisualTestContext, seconds: u64| {
            for _ in 0..seconds {
                cx.cx.executor().advance_clock(std::time::Duration::from_secs(1));
                cx.run_until_parked();
            }
            for _ in 0..3 {
                refresh_frame(cx);
            }
        };
        wait(cx, 30);
        assert_eq!(forge.count("ChangeRequestForBranch"), 1, "not before the minute");
        wait(cx, 40);
        assert_eq!(forge.count("ChangeRequestForBranch"), 2, "once a minute while open");

        let fold = |chat: &Entity<Chat>, cx: &mut VisualTestContext| {
            chat.update_in(cx, |chat, window, cx| {
                chat.session_panel_act(
                    SessionPanelAction::Toggle("change_request".to_string()),
                    window,
                    cx,
                )
            })
            .expect("the card folds");
        };
        fold(&chat, cx);
        wait(cx, 200);
        assert_eq!(forge.count("ChangeRequestForBranch"), 2, "a folded card is not read");

        // GPUI's simulated clock does not move the wall clock the age of an
        // answer is read against, so the answer is made old by hand.
        chat.update(cx, |chat, _| {
            let read = &mut chat.session_panel.change_request.last_read;
            *read = read.map(|at| at - 120);
        });
        fold(&chat, cx);
        wait(cx, 2);
        assert_eq!(forge.count("ChangeRequestForBranch"), 3, "unfolding reads the stale answer again");
        chat.update(cx, |chat, cx| chat.set_session_panel_open(false, cx));
        wait(cx, 200);
        assert_eq!(forge.count("ChangeRequestForBranch"), 3, "a closed column is not read");
    }
}
