//! The Change request card: the pull or merge request of the chat's own
//! branch, one row that opens its tab. Hidden when the worktree is on no
//! forge, on a detached HEAD with no link, or when nothing installed a
//! `ChangeRequestSource`. A forge nothing is signed in to keeps its card,
//! with one row that says so.
//!
//! The card starts folded and a folded card asks the forge nothing, so it
//! cannot wait for a read to be drawn. When the column opens, `has_forge`
//! decides from what the source already holds whether the worktree has a
//! forge, and if so the card is drawn with its title alone; the read starts
//! when the reader unfolds it.
//!
//! The card reads a cached answer; nothing here blocks. A read is one task:
//! `connect` (it runs git, and `gh` or `glab` on a first look), a look at
//! the host's shared pause on the GPUI thread, then `read_branch_card`,
//! each hop off the GPUI thread. It runs on the first frame that paints the
//! column after the card unfolds or the column opens with it unfolded (if
//! the last answer is a minute old) and then every minute while the card is
//! open and a frame painted it since the last tick: a chat in a background
//! tab, or restored and never shown, asks the forge for nothing. Closing the
//! column or folding the card drops the timer and any read in flight.

use super::{Row, Section, environment};
use crate::branch_card::{BranchCard, BranchCardRead, read_branch_card};
use crate::change_request_style as style;
use crate::change_request_tab::unreachable_text;
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
    /// The forge is known and nothing is signed in to it: the host, and the
    /// sentence the right panel and the tab give for it.
    NotSignedIn { host: String, text: String },
}

/// Whether the worktree has a forge, as the probe at the column's opening
/// and the reads since have found.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum ForgeKnown {
    /// Not decided: the probe has not landed since the column opened.
    #[default]
    Unknown,
    /// No remote on a forge, or a host nothing names a forge for: no card.
    None,
    /// A forge: the card is drawn, folded until the reader opens it.
    Present,
}

#[derive(Default)]
pub(super) struct ChangeRequest {
    forge: ForgeKnown,
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
    /// Whether the read owed to the next paint must happen even when the
    /// last answer is fresh: the branch moved.
    read_even_if_fresh: bool,
    read_task: Option<Task<()>>,
    timer_task: Option<Task<()>>,
    probe_task: Option<Task<()>>,
}

impl ChangeRequest {
    /// Makes the last answer `seconds` older: GPUI's simulated clock does
    /// not move the wall clock an answer's age is read against.
    #[cfg(test)]
    pub(super) fn age_by(&mut self, seconds: i64) {
        self.last_read = self.last_read.map(|at| at - seconds);
    }
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
            // The title takes the line; the facts go under it.
            detail_below: true,
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
        Some(Shown::NotSignedIn { host, text }) => rows.push(Row {
            muted: true,
            note: text.clone(),
            ..Row::new(
                "change_request/not_connected",
                format!("Not signed in to {host}"),
                "",
            )
        }),
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
    // The forge was looked for and there is none: no card, whatever an
    // earlier read left behind. With nothing read, the card is the probe's.
    if state.forge == ForgeKnown::None || (rows.is_empty() && state.forge != ForgeKnown::Present) {
        return None;
    }
    Some(Section {
        key: "change_request",
        title: "Change request",
        summary: String::new(),
        note: String::new(),
        bar: Vec::new(),
        rows,
        custom_body: false,
    })
}

impl Chat {
    /// The column opened: decides whether the worktree has a forge, without
    /// asking one, so the card can be drawn folded. A read that lands first
    /// knows more and keeps its answer.
    pub(in crate::chat) fn probe_session_change_request_forge(&mut self, cx: &mut GpuiContext<Self>) {
        let state = &mut self.session_panel.change_request;
        state.forge = ForgeKnown::Unknown;
        state.probe_task = None;
        let Some(source) = forge_source::source(cx) else {
            return;
        };
        let root = self
            .session_panel
            .environment
            .worktree
            .as_ref()
            .map_or_else(|| self.agent_cwd.clone(), |worktree| worktree.root.clone());
        self.session_panel.change_request.probe_task = Some(cx.spawn(async move |this, cx| {
            let has_forge = cx
                .background_spawn(async move { source.has_forge(&root) })
                .await;
            let _ = this.update(cx, |chat, cx| {
                let state = &mut chat.session_panel.change_request;
                if state.forge == ForgeKnown::Unknown {
                    state.forge = if has_forge {
                        ForgeKnown::Present
                    } else {
                        ForgeKnown::None
                    };
                    cx.notify();
                }
            });
        }));
    }

    /// The column opened or the card unfolded: reads on the first frame that
    /// paints it, when the last answer is old, and keeps reading every
    /// minute it was painted in until stopped.
    pub(in crate::chat) fn start_session_change_request(&mut self, cx: &mut GpuiContext<Self>) {
        if !self.session_panel.open || self.session_panel.collapsed.contains("change_request") {
            return;
        }
        let frame = &self.session_panel.frame;
        frame.painted.set(false);
        frame.read_on_paint.set(true);
        self.session_panel.change_request.timer_task = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(READ_EVERY).await;
                let alive = this.update(cx, |chat, cx| {
                    if chat.session_panel.frame.painted.replace(false) {
                        chat.read_session_change_request(cx);
                    } else {
                        // Not painted since the last tick: either nobody can
                        // see the card, or an idle window drew nothing. Owe
                        // the read to the next paint and ask for one; a chat
                        // that is not on screen is never painted, so it
                        // still reads nothing.
                        let frame = &chat.session_panel.frame;
                        frame.read_on_paint.set(true);
                        chat.session_panel.change_request.read_even_if_fresh = true;
                        cx.notify();
                    }
                });
                if alive.is_err() {
                    break;
                }
            }
        }));
        cx.notify();
    }

    /// A frame painted the chat while a read was owed to the next one.
    pub(in crate::chat) fn session_panel_painted(&mut self, cx: &mut GpuiContext<Self>) {
        if !self.session_panel.frame.read_on_paint.replace(false) {
            return;
        }
        let state = &mut self.session_panel.change_request;
        let forced = std::mem::take(&mut state.read_even_if_fresh);
        if !self.session_panel.open || self.session_panel.collapsed.contains("change_request") {
            return;
        }
        let stale = state
            .last_read
            .is_none_or(|at| style::now() - at >= STALE_SECS);
        if forced || stale {
            self.read_session_change_request(cx);
        }
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
        let branch_seq = self.session_panel.environment.read_seq;
        let state = &mut self.session_panel.change_request;
        state.generation += 1;
        let generation = state.generation;
        state.read_task = Some(cx.spawn(async move |this, cx| {
            let connecting = source.clone();
            let connecting_root = root.clone();
            let connection = cx
                .background_spawn(async move { connecting.connect(&connecting_root) })
                .await;
            let ready = match connection {
                Connection::Ready(ready) => ready,
                // Nothing to read: no forge here, or none signed in to.
                unready => {
                    let _ = this.update(cx, |chat, cx| {
                        chat.land_unready_session_change_request(generation, unready, cx);
                    });
                    return;
                }
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
                        chat.hold_session_change_request(host.clone(), until, cx);
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
                chat.land_session_change_request(
                    generation,
                    branch_seq,
                    (forge, host, read),
                    cx,
                );
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

    /// The Environment line read another branch than the one it showed: the
    /// card is read at the next frame that paints it rather than at the next
    /// minute, and an answer still in flight for the old branch is dropped
    /// by the new generation.
    pub(in crate::chat) fn session_branch_moved(&mut self, cx: &mut GpuiContext<Self>) {
        if self.session_panel.open && !self.session_panel.collapsed.contains("change_request") {
            self.session_panel.change_request.read_even_if_fresh = true;
            self.session_panel.frame.read_on_paint.set(true);
            cx.notify();
        }
    }

    /// Takes the answer of a read that found no client. A forge nothing is
    /// signed in to keeps its card, with a row that says so; no forge at all
    /// takes it away.
    fn land_unready_session_change_request(
        &mut self,
        generation: u64,
        connection: Connection,
        cx: &mut GpuiContext<Self>,
    ) {
        let state = &mut self.session_panel.change_request;
        if state.generation != generation {
            return;
        }
        state.last_read = Some(style::now());
        // A host cached from before must not block a different remote.
        state.host = None;
        state.error = None;
        match &connection {
            Connection::NotConnected { host, .. } => {
                state.shown = Some(Shown::NotSignedIn {
                    host: host.clone(),
                    text: unreachable_text(&connection),
                });
                state.forge = ForgeKnown::Present;
            }
            Connection::NoForgeRemote | Connection::UnknownForge { .. } | Connection::Ready(_) => {
                state.shown = None;
                state.forge = ForgeKnown::None;
            }
        }
        cx.notify();
    }

    /// Takes a read's answer. `branch_seq` is how many Environment reads had
    /// started when this one did.
    fn land_session_change_request(
        &mut self,
        generation: u64,
        branch_seq: u64,
        answer: (Forge, String, BranchCardRead),
        cx: &mut GpuiContext<Self>,
    ) {
        let state = &mut self.session_panel.change_request;
        if state.generation != generation {
            return;
        }
        state.last_read = Some(style::now());
        state.forge = ForgeKnown::Present;
        let (forge, host, read) = answer;
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
        // the short hash. A read that began before the line's latest one is
        // the older of the two and leaves it be.
        if let Some(branch) = branch
            && self.session_panel.environment.read_seq == branch_seq
        {
            self.session_panel.environment.branch = environment::Branch::Named(branch);
        }
        let state = &mut self.session_panel.change_request;
        match result {
            Ok(card) => {
                // A pause noted after this read began still holds.
                state.error = forge_pause::paused_until(&host, style::now(), cx).map(|until| {
                    ForgeError::RateLimited {
                        host: host.clone(),
                        reset_at: Some(until),
                    }
                });
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
    use crate::chat::tests::{TempDir, offline_chat_view, pump_chat_until, refresh_frame};
    use crate::chat::{Chat, ChatEvent};
    use crate::forge_pause;
    use crate::forge_source::testing::{self, CannedForge, FakeSource};
    use crate::forge_source::{self, Connection};
    use gpui::{Entity, TestAppContext, VisualTestContext, px, size};
    use crate::forge_source::ReadyConnection;
    use super::{BranchCard, BranchCardRead, ForgeKnown};
    use sirio_forge::{Forge, ForgeError, Means};
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

    /// Waits for the folded card: the probe has found a forge.
    fn drawn(chat: &Entity<Chat>, cx: &mut VisualTestContext) {
        pump_chat_until(cx, chat, |chat| {
            chat.control_snapshot()
                .session_panel
                .sections
                .iter()
                .any(|section| section.key == "change_request")
        });
    }

    /// Unfolds the card as a click on its title does, once it is drawn.
    fn unfold(chat: &Entity<Chat>, cx: &mut VisualTestContext) {
        drawn(chat, cx);
        chat.update_in(cx, |chat, window, cx| {
            chat.session_panel_act(
                SessionPanelAction::Toggle("change_request".to_string()),
                window,
                cx,
            )
        })
        .expect("the card unfolds");
    }

    /// A chat whose column is open against `source` with the card unfolded:
    /// it starts folded, and a folded card asks the forge nothing.
    fn open_unfolded(
        cx: &mut TestAppContext,
        source: Arc<FakeSource>,
    ) -> (Entity<Chat>, &mut VisualTestContext) {
        let (chat, cx) = open_with(cx, source);
        unfold(&chat, cx);
        (chat, cx)
    }

    /// Waits for a row, not for the section: the folded card is one before
    /// anything has been read.
    fn settle(chat: &Entity<Chat>, cx: &mut VisualTestContext) -> SectionSnapshot {
        pump_chat_until(cx, chat, |chat| {
            chat.control_snapshot()
                .session_panel
                .sections
                .iter()
                .any(|section| section.key == "change_request" && !section.rows.is_empty())
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
        let (chat, cx) = open_unfolded(cx, source);
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
    async fn the_title_has_the_whole_line_and_the_detail_its_own(cx: &mut TestAppContext) {
        let forge = forge_with(vec![testing::summary(
            112,
            "Login from a deleted fork that has a long title",
        )]);
        let source = FakeSource::ready(testing::github_client(forge), Some("feat/login"));
        let (chat, cx) = open_unfolded(cx, source);
        let section = settle(&chat, cx);
        let key = &section.rows[0].key;
        let row = cx
            .debug_bounds(Box::leak(format!("session-row-{key}").into_boxed_str()))
            .expect("the row is drawn");
        let title = cx
            .debug_bounds(Box::leak(format!("session-row-label-{key}").into_boxed_str()))
            .expect("the title is drawn");
        // Nothing shares the title's line: it runs to the card's edge, where
        // it truncates.
        assert!(
            title.right() >= row.right() - px(2.0),
            "the title {title:?} is squeezed inside the row {row:?}"
        );
        let detail = cx
            .debug_bounds(Box::leak(format!("session-row-detail-{key}").into_boxed_str()))
            .expect("the detail is drawn");
        assert!(detail.top() >= title.bottom(), "{detail:?} is under {title:?}");
        assert_eq!(detail.left(), row.left() + px(20.0), "indented past the icon");
    }

    #[gpui::test]
    async fn a_branch_without_one_says_so(cx: &mut TestAppContext) {
        let forge = forge_with(Vec::new());
        let source = FakeSource::ready(testing::github_client(forge), Some("feat/none"));
        let (chat, cx) = open_unfolded(cx, source);
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
        let (chat, cx) = open_unfolded(cx, source);
        let section = settle(&chat, cx);
        assert_eq!(forge.count("ChangeRequestForBranch"), 0, "a paused host is not asked");
        assert!(only_row(&section).label.starts_with("Rate limited"), "{:?}", section.rows);
    }

    #[gpui::test]
    async fn no_forge_hides_the_section(cx: &mut TestAppContext) {
        let source = FakeSource::with(Connection::NoForgeRemote);
        let (chat, cx) = open_with(cx, source.clone());
        pump_chat_until(cx, &chat, |chat| {
            chat.session_panel.change_request.forge != ForgeKnown::Unknown
        });
        assert_eq!(*source.forge_probes.lock().unwrap(), 1);
        for _ in 0..5 {
            refresh_frame(cx);
        }
        assert!(section_of(&chat, cx, "change_request").is_none());
        assert!(section_of(&chat, cx, "environment").is_some());
    }

    #[gpui::test(iterations = 8)]
    async fn a_folded_card_is_drawn_without_asking_the_forge(cx: &mut TestAppContext) {
        let forge = forge_with(vec![testing::summary(7, "Whatever")]);
        let source = FakeSource::ready(testing::github_client(forge.clone()), Some("feat/y"));
        let (chat, cx) = open_with(cx, source.clone());
        drawn(&chat, cx);
        for _ in 0..5 {
            refresh_frame(cx);
        }
        let section = section_of(&chat, cx, "change_request").expect("drawn folded");
        assert!(
            section.rows.is_empty() && section.summary.is_empty() && !section.expanded,
            "{section:?}"
        );
        assert_eq!(forge.requests(), Vec::<String>::new(), "no forge request while folded");
        assert_eq!(*source.connects.lock().unwrap(), 0, "connect waits for the unfold");
    }

    #[gpui::test(iterations = 8)]
    async fn unfolding_the_card_reads_it(cx: &mut TestAppContext) {
        let forge = forge_with(vec![testing::summary(7, "Whatever")]);
        let source = FakeSource::ready(testing::github_client(forge.clone()), Some("feat/y"));
        let (chat, cx) = open_with(cx, source);
        unfold(&chat, cx);
        let section = settle(&chat, cx);
        assert!(section.expanded);
        assert!(
            section.rows.iter().any(|row| row.key == "change_request/card"),
            "{section:?}"
        );
        assert_eq!(forge.count("ChangeRequestForBranch"), 1);
    }

    /// A host that is a forge by name but where nothing is signed in: the
    /// probe cannot tell, so the folded card is drawn, and once unfolded it
    /// says what is missing instead of going away.
    #[gpui::test]
    async fn a_card_whose_read_finds_no_sign_in_says_so(cx: &mut TestAppContext) {
        let source = FakeSource::with(Connection::NotConnected {
            forge: Forge::GitHub,
            host: "ghe.test".to_string(),
        });
        let (chat, cx) = open_unfolded(cx, source.clone());
        let section = settle(&chat, cx);
        assert!(section.expanded);
        let row = only_row(&section);
        assert_eq!(row.key, "change_request/not_connected");
        assert_eq!(row.label, "Not signed in to ghe.test");
        assert!(
            row.note.contains("`gh auth login --hostname ghe.test`"),
            "the sentence the right panel and the tab use: {:?}",
            row.note
        );
        assert!(row.action.is_none(), "there is nothing to open");
    }

    /// The remote went away between the probe and the unfold: the read finds
    /// no forge and the card goes.
    #[gpui::test]
    async fn a_card_whose_read_finds_no_forge_goes_away(cx: &mut TestAppContext) {
        let forge = forge_with(vec![testing::summary(7, "Whatever")]);
        let source = FakeSource::ready(testing::github_client(forge), Some("feat/y"));
        let (chat, cx) = open_with(cx, source.clone());
        drawn(&chat, cx);
        source.set(Connection::NoForgeRemote);
        unfold(&chat, cx);
        pump_chat_until(cx, &chat, |_| *source.connects.lock().unwrap() >= 1);
        for _ in 0..5 {
            refresh_frame(cx);
        }
        assert!(section_of(&chat, cx, "change_request").is_none());
    }

    /// A toggle that reaches the chat before the probe has drawn the card
    /// still opens it: the fold of a default-folded card is not decided by
    /// whether its section is drawn yet.
    #[gpui::test]
    async fn a_toggle_before_the_probe_lands_still_unfolds_the_card(cx: &mut TestAppContext) {
        let forge = forge_with(vec![testing::summary(7, "Whatever")]);
        let source = FakeSource::ready(testing::github_client(forge), Some("feat/y"));
        let (chat, cx) = offline_chat_view(cx);
        cx.update(|_, cx| forge_source::set_source(source, cx));
        cx.simulate_resize(size(px(1100.0), px(700.0)));
        chat.update(cx, |chat, cx| {
            chat.set_session_panel_open(true, cx);
            assert!(
                chat.session_panel_sections().iter().all(|section| section.key != "change_request"),
                "the probe has not landed: no card is drawn"
            );
            chat.toggle_session_fold("change_request", cx);
        });
        let section = settle(&chat, cx);
        assert!(section.expanded, "{section:?}");
        assert!(
            section.rows.iter().any(|row| row.key == "change_request/card"),
            "{section:?}"
        );
    }

    #[gpui::test(iterations = 8)]
    async fn the_live_branch_read_updates_the_environment_line(cx: &mut TestAppContext) {
        let forge = forge_with(vec![testing::summary(7, "Whatever")]);
        let source = FakeSource::ready(testing::github_client(forge), Some("feat/y"));
        let (chat, cx) = open_unfolded(cx, source);
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
        let (chat, cx) = open_unfolded(cx, source);
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

        chat.update(cx, |chat, _| chat.session_panel.change_request.age_by(120));
        fold(&chat, cx);
        wait(cx, 2);
        assert_eq!(forge.count("ChangeRequestForBranch"), 3, "unfolding reads the stale answer again");
        chat.update(cx, |chat, cx| chat.set_session_panel_open(false, cx));
        wait(cx, 200);
        assert_eq!(forge.count("ChangeRequestForBranch"), 3, "a closed column is not read");
    }

    /// A window that shows a chat that lives apart from it.
    struct Host(Entity<Chat>);

    impl gpui::Render for Host {
        fn render(
            &mut self,
            _: &mut gpui::Window,
            _: &mut gpui::Context<Self>,
        ) -> impl gpui::IntoElement {
            use gpui::{ParentElement as _, Styled as _};
            gpui::div().size_full().child(self.0.clone())
        }
    }

    #[gpui::test]
    async fn a_column_nobody_sees_does_not_read_the_forge(cx: &mut TestAppContext) {
        use gpui::AppContext as _;
        let forge = forge_with(vec![testing::summary(112, "Fix login")]);
        let source = FakeSource::ready(testing::github_client(forge.clone()), Some("feat/login"));
        cx.update(sirio_theme::Theme::init);
        cx.update(bezel::ui::input::init);
        cx.update(crate::chat::init);
        cx.update(|cx| forge_source::set_source(source.clone(), cx));
        // A restored chat, or one in a background tab: open, in no window.
        let chat = cx.new(|cx| {
            Chat::new(
                Some(crate::chat::LaunchSpec::Acp(sirio_acp::AgentCommand::new(
                    "/definitely/missing/sirio-acp-agent",
                ))),
                std::env::temp_dir(),
                cx,
            )
        });
        chat.update(cx, |chat, cx| chat.set_session_panel_open(true, cx));
        // The card is drawn folded once the probe has landed, and a reader
        // had opened it before the chat went to a background tab.
        cx.run_until_parked();
        assert!(
            chat.read_with(cx, |chat, _| chat
                .session_panel_sections()
                .iter()
                .any(|section| section.key == "change_request")),
            "the probe drew the folded card"
        );
        chat.update(cx, |chat, cx| chat.toggle_session_fold("change_request", cx));
        let wait = |cx: &mut TestAppContext, seconds: u64| {
            for _ in 0..seconds {
                cx.executor().advance_clock(std::time::Duration::from_secs(1));
                cx.run_until_parked();
            }
        };
        wait(cx, 130);
        assert_eq!(forge.count("ChangeRequestForBranch"), 0, "two ticks, no paint, no read");
        assert_eq!(*source.connects.lock().unwrap(), 0, "not even a connection");

        // The first frame that paints it reads once.
        let host = chat.clone();
        let (_, cx) = cx.add_window_view(|_, _| Host(host));
        cx.simulate_resize(size(px(1100.0), px(700.0)));
        for _ in 0..3 {
            refresh_frame(cx);
        }
        settle(&chat, cx);
        assert_eq!(forge.count("ChangeRequestForBranch"), 1);
    }

    #[gpui::test]
    async fn a_gitlab_branch_without_one_says_merge_request(cx: &mut TestAppContext) {
        let forge = Arc::new(CannedForge::default());
        forge.answer(
            "CurrentUser",
            serde_json::json!({"data": {"currentUser": {"username": "me"}}}).to_string(),
        );
        forge.answer(
            "MergeRequestForBranch",
            serde_json::json!({"data": {"project": {"mergeRequests": {"nodes": []}}}}).to_string(),
        );
        let source = FakeSource::ready(testing::gitlab_client(forge), Some("feat/none"));
        let (chat, cx) = open_unfolded(cx, source);
        let section = settle(&chat, cx);
        assert_eq!(only_row(&section).label, "No merge request for this branch");
    }

    #[gpui::test]
    async fn a_linked_read_that_is_rate_limited_pauses_the_host(cx: &mut TestAppContext) {
        let forge = forge_with(vec![testing::summary(112, "Fix login")]);
        forge.rate_limited("ChangeRequestByNumber", style::now() + 600);
        let source = FakeSource::with(Connection::Ready(ReadyConnection {
            client: testing::github_client(forge.clone()),
            means: Means::Token,
            branch: Some("feat/login".to_string()),
            source_owner: None,
            linked: Some(testing::reference(7)),
        }));
        let (chat, cx) = open_unfolded(cx, source.clone());
        let section = settle(&chat, cx);
        assert_eq!(section.rows[0].label, "#112 Fix login", "the branch lookup decided");
        assert!(
            cx.update(|_, cx| forge_pause::paused_until("ghe.test", style::now(), cx)).is_some(),
            "the linked read's rate limit reached the shared pause"
        );
        // The pause was noted after the read began; its answer must not
        // wipe the label while the pause holds.
        assert!(
            section.rows.iter().any(|row| row.label.starts_with("Rate limited")),
            "{:?}",
            section.rows
        );

        chat.update(cx, |chat, cx| chat.set_session_panel_open(false, cx));
        chat.update(cx, |chat, _| chat.session_panel.change_request.age_by(120));
        chat.update(cx, |chat, cx| chat.set_session_panel_open(true, cx));
        for _ in 0..5 {
            refresh_frame(cx);
        }
        assert_eq!(*source.connects.lock().unwrap(), 1, "a paused host is not even connected to");
        assert_eq!(forge.count("ChangeRequestForBranch"), 1);
        assert_eq!(forge.count("ChangeRequestByNumber"), 1);
    }

    fn git(dir: &std::path::Path, args: &[&str]) {
        let status = std::process::Command::new("git")
            .args(["-c", "user.name=t", "-c", "user.email=t@t", "-c", "commit.gpgsign=false"])
            .args(args)
            .current_dir(dir)
            .status()
            .expect("git runs");
        assert!(status.success(), "git {args:?} failed");
    }

    #[gpui::test]
    async fn a_turn_that_switches_branch_moves_the_card_with_the_line(cx: &mut TestAppContext) {
        let dir = TempDir::new();
        let repo = dir.0.join("p");
        std::fs::create_dir_all(&repo).unwrap();
        git(&repo, &["init", "-q", "-b", "main"]);
        git(&repo, &["commit", "-q", "--allow-empty", "-m", "first"]);

        let forge = forge_with(vec![testing::summary(112, "Fix login")]);
        let source = FakeSource::ready(testing::github_client(forge.clone()), Some("main"));
        cx.update(sirio_theme::Theme::init);
        cx.update(bezel::ui::input::init);
        cx.update(crate::chat::init);
        let (chat, cx) = cx.add_window_view(|_, cx| {
            Chat::new(
                Some(crate::chat::LaunchSpec::Acp(sirio_acp::AgentCommand::new(
                    "/definitely/missing/sirio-acp-agent",
                ))),
                repo.clone(),
                cx,
            )
        });
        cx.update(|_, cx| forge_source::set_source(source.clone(), cx));
        cx.simulate_resize(size(px(1100.0), px(700.0)));
        chat.update(cx, |chat, cx| chat.set_session_panel_open(true, cx));
        unfold(&chat, cx);
        settle(&chat, cx);
        assert_eq!(forge.count("ChangeRequestForBranch"), 1);

        // The turn switches the branch, in git and for the forge's source.
        git(&repo, &["switch", "-q", "-c", "feat/new"]);
        source.switch_to("feat/new");
        forge.answer(
            "ChangeRequestForBranch",
            testing::branch(vec![testing::summary(200, "Another")]),
        );
        chat.update(cx, |chat, cx| chat.reread_session_branch(cx));
        // Well inside the minute: only the move can have asked, and only
        // once a frame has painted the column.
        cx.cx.executor().allow_parking();
        for _ in 0..200 {
            if forge.count("ChangeRequestForBranch") >= 2 {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
            cx.cx.run_until_parked();
            refresh_frame(cx);
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
        cx.cx.run_until_parked();
        for _ in 0..3 {
            refresh_frame(cx);
        }
        assert_eq!(forge.count("ChangeRequestForBranch"), 2, "the move read the card at once");
        let environment = section_of(&chat, cx, "environment").expect("environment");
        assert!(
            environment.rows.iter().any(|row| row.label == "feat/new"),
            "{:?}",
            environment.rows
        );
    }

    #[gpui::test]
    async fn an_older_card_read_does_not_overwrite_a_newer_branch_line(cx: &mut TestAppContext) {
        let source = FakeSource::with(Connection::NoForgeRemote);
        let (chat, cx) = open_with(cx, source);
        let landed = |chat: &Entity<Chat>, cx: &mut VisualTestContext, started_at: u64| {
            chat.update(cx, |chat, cx| {
                chat.session_panel.environment.branch =
                    crate::chat::session_panel::environment::Branch::Named("new".to_string());
                chat.session_panel.environment.read_seq = 5;
                let generation = chat.session_panel.change_request.generation;
                let read = BranchCardRead {
                    card: Ok(BranchCard::Hidden),
                    linked_error: None,
                    branch: Some("old".to_string()),
                };
                chat.land_session_change_request(
                    generation,
                    started_at,
                    (Forge::GitHub, "ghe.test".to_string(), read),
                    cx,
                );
                chat.session_panel.environment.branch.clone()
            })
        };
        use crate::chat::session_panel::environment::Branch;
        assert_eq!(landed(&chat, cx, 4), Branch::Named("new".to_string()), "older read");
        assert_eq!(landed(&chat, cx, 5), Branch::Named("old".to_string()), "no newer read");
    }
}
