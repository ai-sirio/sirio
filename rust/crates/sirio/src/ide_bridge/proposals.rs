//! Claude's `openDiff` as a Proposal tab (spec 2026-11-15, 4.7, 4.8, 10.4, 10.8, 10.14).
//!
//! `sirio_ide` registers each `openDiff` and waits; this module is the app's side of it. An
//! `OpenDiff` becomes a [`ProposalTab`] in the Secondary half of the owner's worktree (or is held
//! tab-less for a worktree that is not on screen), and the request is answered only by the user:
//! Accept, Reject, or Reject in the close confirmation. Nothing here ever answers on its own.
//! `DiffWithdrawn` (Claude's `close_tab`, `closeAllDiffTabs`) and a connection closing close the
//! tab without asking and answer nothing.
//!
//! Rules this code keeps:
//! - A proposal's answer is sent, then its tab closes, whatever `answer_diff` returned: `Ok`
//!   means queued, and `Err(Gone)` means Claude did not get the decision (a toast says so).
//! - Closing a proposal tab, a worktree or a project that holds one asks first (the in-window
//!   confirmation below). The release `workspace.close` verb cannot ask and refuses instead.
//! - No path, proposal text or comment goes into a log line, a perf label or the debug state
//!   (which shows a worktree root and counts).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use gpui::{AnyElement, App, AppContext as _, BorrowAppContext as _, ClipboardItem, Context, Entity, EntityId, FocusHandle, Subscription, Window};
use serde_json::{Value, json};
use sirio_ide::{BridgeEvent, ConnId, DiffAnswer, DiffId, Gone, ReadOutcome, Refusal};
use sirio_ui::modal::{ModalButton, ModalButtonTone, ModalFocus, ModalSpec, render_modal};
use sirio_ui::sidebar::SidebarEvent;
use sirio_ui::proposal_tab::{ProposalAction, ProposalEvent, ProposalInit, ProposalTab};
use sirio_theme::Theme;

use super::IdeBridge;
use crate::{SirioWorkspace, TabContent, paths_name_the_same_document};

/// The toast of a proposal whose connection went away, or whose answer could not be delivered.
pub(crate) const CLOSED_TOAST: &str = "Claude closed its proposed change";

/// The toast of comments that were not typed anywhere: they are on the clipboard.
pub(crate) const COMMENTS_COPIED_TOAST: &str = "Comments copied: paste them into Claude";

/// What the release `workspace.close` verb answers while the worktree holds a proposal.
pub(crate) const WORKTREE_HOLDS_PROPOSAL: &str = "Claude has a proposed change open in this worktree";

const NOT_REGULAR_COPY: &str =
    "This change cannot be drawn: the path is not a regular file. You can still Reject or Accept it.";
const UNREADABLE_COPY: &str =
    "This change cannot be drawn: the file cannot be read. You can still Reject or Accept it.";

const CONFIRM_TITLE: &str = "Closing rejects Claude's change";
const CONFIRM_BODY: &str = "Claude is waiting for a decision on this change. Closing sends it a rejection.";

/// What `read_regular_file` refused, in the words the tab's body shows.
fn refusal_copy(refusal: Refusal) -> String {
    match refusal {
        Refusal::TooLarge(_) => ProposalTab::too_large_message(),
        Refusal::NotRegular => NOT_REGULAR_COPY.to_owned(),
        Refusal::Unreadable => UNREADABLE_COPY.to_owned(),
    }
}

/// `<database directory>/ide-proposals`: where the diff's temporary files go.
fn scratch_dir() -> PathBuf {
    let database = crate::session::database_path();
    database.parent().unwrap_or(Path::new(".")).join("ide-proposals")
}

/// Removes whatever a crashed Sirio left in the scratch directory (a crash between writing a
/// proposal's file and dropping it leaves file contents behind) and makes the directory again,
/// private. A symlink is removed, never followed. Runs before the bridge starts.
pub(crate) fn sweep_scratch() {
    let dir = scratch_dir();
    match std::fs::symlink_metadata(&dir) {
        Ok(meta) if meta.is_dir() => {
            let _ = std::fs::remove_dir_all(&dir);
        }
        Ok(_) => {
            let _ = std::fs::remove_file(&dir);
        }
        Err(_) => {}
    }
    let mut builder = std::fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    let _ = builder.create(&dir);
}

/// One open proposal, whether its tab is mounted or parked.
struct Record {
    id: DiffId,
    conn: ConnId,
    tab_name: String,
    /// The tab's title, `✻ <file name>`.
    title: String,
    /// The worktree it belongs to, spelled as the catalog (and `working_directory`) spell it.
    worktree: String,
    /// The file as Claude named it, canonical when it exists; only compared, never shown.
    path: PathBuf,
    view: Entity<ProposalTab>,
    _decided: Subscription,
}

/// What a confirmed Reject goes on to do.
pub(crate) enum Then {
    /// Close these tabs (the original `close_tab` or Close Others / Close Tabs to the Right).
    CloseTabs(Vec<usize>),
    /// The sidebar's confirmed "Remove worktree?" goes on: git removes the checkout, then
    /// `WorktreeRemoved` ends its sessions.
    RemoveWorktree { row_id: usize, remote: Option<sirio_git::UpstreamBranch> },
    /// Remove this project from Sirio.
    RemoveProject(String),
}

impl Then {
    fn name(&self) -> &'static str {
        match self {
            Then::CloseTabs(_) => "close_tabs",
            Then::RemoveWorktree { .. } => "remove_worktree",
            Then::RemoveProject(_) => "remove_project",
        }
    }
}

/// The "Closing rejects Claude's change" question, while it is up.
pub(crate) struct Confirmation {
    views: Vec<EntityId>,
    then: Then,
    pub(crate) focus: FocusHandle,
    pub(crate) needs_focus: bool,
}

/// Everything the workspace holds about proposals.
#[derive(Default)]
pub(crate) struct Proposals {
    records: Vec<Record>,
    /// `OpenDiff`s still waiting for their connection's owner to be resolved: none of them has a
    /// tab yet, and a withdrawal, a disconnect or a replacement drops it from here.
    waiting: HashMap<DiffId, ConnId>,
    pub(crate) confirmation: Option<Confirmation>,
}

impl Proposals {
    pub(crate) fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    fn position_of_view(&self, view: EntityId) -> Option<usize> {
        self.records.iter().position(|record| record.view.entity_id() == view)
    }

    /// The title a parked proposal's tab comes back with.
    pub(crate) fn title_of(&self, view: EntityId) -> Option<String> {
        self.position_of_view(view).map(|index| self.records[index].title.clone())
    }

    fn by_tab_name(&self, tab_name: &str) -> Option<&Record> {
        self.records.iter().find(|record| record.tab_name == tab_name)
    }
}

/// The fields of an `OpenDiff` event.
struct Opening {
    id: DiffId,
    conn: ConnId,
    old_path: PathBuf,
    canonical: Option<PathBuf>,
    original: ReadOutcome,
    contents: String,
    tab_name: String,
    replaces: Option<DiffId>,
}

fn answer_diff(cx: &App, id: &DiffId, answer: DiffAnswer) -> Result<(), Gone> {
    if !cx.has_global::<IdeBridge>() {
        return Err(Gone);
    }
    match cx.global::<IdeBridge>().bridge() {
        Some(bridge) => bridge.answer_diff(id, answer),
        None => Err(Gone),
    }
}

impl SirioWorkspace {
    /// A toast the user sees and the debug state records (content-free copy only).
    pub(crate) fn ide_toast(&mut self, message: &str, cx: &mut Context<Self>) {
        if cx.has_global::<IdeBridge>() {
            cx.update_global(|bridge: &mut IdeBridge, _| bridge.push_toast(message));
        }
        self.show_toast(message, cx);
    }

    /// The bridge's `OpenDiff` and `DiffWithdrawn` events, onto the UI thread.
    pub(crate) fn ide_proposal_event(&mut self, event: BridgeEvent, cx: &mut Context<Self>) {
        match event {
            BridgeEvent::OpenDiff {
                id,
                conn,
                old_path,
                canonical,
                original,
                contents,
                tab_name,
                replaces,
                ..
            } => {
                let opening = Opening {
                    id,
                    conn,
                    old_path,
                    canonical,
                    original,
                    contents,
                    tab_name,
                    replaces,
                };
                self.ide_open_diff(opening, cx);
            }
            BridgeEvent::DiffWithdrawn { id } => self.ide_diff_withdrawn(&id, cx),
            _ => {}
        }
    }

    /// An `openDiff` is owner-dependent (the worktree it opens in, the bar's owner title), so it
    /// goes through the same resolution an at-mention does. It has no deadline.
    fn ide_open_diff(&mut self, opening: Opening, cx: &mut Context<Self>) {
        self.ide_proposals.waiting.insert(opening.id.clone(), opening.conn);
        // The proposal this one replaces never opens, if it was still waiting.
        if let Some(old) = &opening.replaces {
            self.ide_proposals.waiting.remove(old);
        }
        self.ide_when_resolved(cx, move |workspace, cx| workspace.ide_open_diff_resolved(opening, cx));
    }

    fn ide_open_diff_resolved(&mut self, opening: Opening, cx: &mut Context<Self>) {
        // Withdrawn, disconnected or replaced while it waited: nothing opens.
        if self.ide_proposals.waiting.remove(&opening.id).is_none() {
            return;
        }
        let Some((owner, owner_worktree)) = cx.has_global::<IdeBridge>().then(|| {
            cx.global::<IdeBridge>().owner_of(opening.conn)
        }).flatten() else {
            return;
        };
        let path = opening.canonical.clone().unwrap_or_else(|| opening.old_path.clone());
        let file_worktree = || {
            // A path that climbs out with `..` is Claude's claim, not a file in the worktree.
            let climbs = path.components().any(|part| matches!(part, std::path::Component::ParentDir));
            if climbs { None } else { self.worktree_and_relative(&path).map(|(root, _)| root) }
        };
        let worktree = owner_worktree
            .or_else(file_worktree)
            .unwrap_or_else(|| self.working_directory.clone());
        let worktree_id = worktree.to_string_lossy().into_owned();
        let (original, refusal) = match &opening.original {
            ReadOutcome::Contents(bytes) => (Some(bytes.clone()), None),
            ReadOutcome::Missing => (None, None),
            ReadOutcome::Refused(why) => (None, Some(refusal_copy(*why))),
        };
        let owner_title = owner.and_then(|pane| {
            self.tabs
                .iter()
                .find(|tab| tab.panes.contains(pane))
                .map(|tab| tab.title.clone())
        });
        let init = ProposalInit {
            tab_name: opening.tab_name.clone(),
            path: path.clone(),
            display_root: worktree.clone(),
            owner_title,
            original,
            proposal: opening.contents,
            refusal,
            dirty_in_editor: self.ide_path_dirty(&path, cx),
            scratch: scratch_dir(),
        };
        let title = proposal_title(&path, &opening.tab_name);
        // The same `tab_name` again, in place: the old request was answered with an error by the
        // server, and Claude uses a fresh name per proposal, so this is the defensive path.
        if let Some(old) = &opening.replaces
            && let Some(record) = self.ide_proposals.records.iter_mut().find(|record| &record.id == old)
        {
            record.id = opening.id;
            record.conn = opening.conn;
            record.path = path;
            record.title = title.clone();
            let view = record.view.clone();
            view.update(cx, |tab, cx| tab.replace(init, cx));
            let view_id = view.entity_id();
            if let Some(tab_id) = self.proposal_tab_id(view_id)
                && let Some(tab) = self.tabs.iter_mut().find(|tab| tab.id == tab_id)
            {
                tab.title = title;
            }
            cx.notify();
            return;
        }
        let view = cx.new(|cx| ProposalTab::new(init, cx));
        let decided = cx.subscribe(&view, |workspace, view, event: &ProposalEvent, cx| {
            workspace.ide_proposal_decided(&view, event, cx);
        });
        self.ide_proposals.records.push(Record {
            id: opening.id,
            conn: opening.conn,
            tab_name: opening.tab_name,
            title: title.clone(),
            worktree: worktree_id.clone(),
            path,
            view: view.clone(),
            _decided: decided,
        });
        if paths_name_the_same_document(&worktree, &self.working_directory) {
            self.add_proposal_tab(view, title, cx);
        } else {
            self.parked_proposals.entry(worktree_id).or_default().push(view);
            let name = self.worktree_name(&worktree);
            self.ide_toast(&format!("Claude proposes a change in {name}"), cx);
        }
        cx.notify();
    }

    /// The name the sidebar gives `worktree`: its branch, else its directory's name.
    fn worktree_name(&self, worktree: &Path) -> String {
        let branch = self.project_catalog.projects().iter().find_map(|project| {
            project
                .worktrees
                .iter()
                .find(|known| paths_name_the_same_document(&known.path, worktree))
                .map(|known| known.branch.clone())
        });
        branch
            .filter(|branch| !branch.is_empty())
            .or_else(|| worktree.file_name().map(|name| name.to_string_lossy().into_owned()))
            .unwrap_or_else(|| "its worktree".to_owned())
    }

    /// The id of the tab that shows `view`, when one is mounted.
    pub(crate) fn proposal_tab_id(&self, view: EntityId) -> Option<usize> {
        self.tabs.iter().find_map(|tab| {
            let mut found = false;
            tab.panes.for_each(&mut |_, content| {
                found |= matches!(content, TabContent::Proposal(shown) if shown.entity_id() == view);
            });
            found.then_some(tab.id)
        })
    }

    /// Whether any open editor tab on `path` has unsaved edits.
    fn ide_path_dirty(&self, path: &Path, cx: &App) -> bool {
        let mut dirty = false;
        for tab in &self.tabs {
            tab.panes.for_each(&mut |_, content| {
                if let TabContent::File { view } = content {
                    let view = view.read(cx);
                    dirty |= !view.is_snapshot()
                        && view.is_dirty()
                        && paths_name_the_same_document(view.path(), path);
                }
            });
        }
        dirty
    }

    /// An editor tab's dirty state moved: the bar's warning follows it (ruling 56).
    pub(crate) fn ide_proposal_dirty_changed(&mut self, path: &Path, cx: &mut Context<Self>) {
        if self.ide_proposals.is_empty() {
            return;
        }
        let affected: Vec<(Entity<ProposalTab>, PathBuf)> = self
            .ide_proposals
            .records
            .iter()
            .map(|record| (record.view.clone(), record.path.clone()))
            .collect();
        for (view, record_path) in affected {
            if paths_name_the_same_document(&record_path, path) {
                let dirty = self.ide_path_dirty(&record_path, cx);
                view.update(cx, |tab, cx| tab.set_dirty_in_editor(dirty, cx));
            }
        }
    }

    /// A proposal's bar was pressed: send the decision, then close the tab, whatever the send
    /// returned (`Ok` is queued, not delivered; `Gone` is a toast).
    fn ide_proposal_decided(&mut self, view: &Entity<ProposalTab>, event: &ProposalEvent, cx: &mut Context<Self>) {
        let Some(index) = self.ide_proposals.position_of_view(view.entity_id()) else {
            return;
        };
        let record = self.ide_proposals.records.remove(index);
        let (answer, comments) = match event {
            ProposalEvent::Accept(text) => (DiffAnswer::Saved(text.clone()), None),
            ProposalEvent::Reject { comments } => (DiffAnswer::Rejected, comments.clone()),
        };
        if answer_diff(cx, &record.id, answer).is_err() {
            self.ide_toast(CLOSED_TOAST, cx);
        }
        if let Some(text) = comments {
            // Typing them into the owner's terminal is a later layer; the clipboard always works.
            cx.write_to_clipboard(ClipboardItem::new_string(text));
            self.ide_toast(COMMENTS_COPIED_TOAST, cx);
        }
        self.ide_remove_view(record.view.entity_id(), cx);
        // The subscription this runs from is dropped after it returns.
        cx.defer(move |_| drop(record));
    }

    /// Takes `view` out of the tab strip and the parked lists (its record is already gone, so the
    /// close guard lets the tab go).
    fn ide_remove_view(&mut self, view: EntityId, cx: &mut Context<Self>) {
        for parked in self.parked_proposals.values_mut() {
            parked.retain(|shown| shown.entity_id() != view);
        }
        self.parked_proposals.retain(|_, parked| !parked.is_empty());
        if let Some(tab_id) = self.proposal_tab_id(view) {
            self.close_tab_by_id(tab_id, None, cx);
        }
        self.ide_prune_confirmation(cx);
    }

    /// Claude closed the proposal (`close_tab`, `closeAllDiffTabs`): the tab goes without asking.
    /// Comments written so far move to the clipboard.
    fn ide_diff_withdrawn(&mut self, id: &DiffId, cx: &mut Context<Self>) {
        self.ide_proposals.waiting.remove(id);
        let Some(index) = self.ide_proposals.records.iter().position(|record| &record.id == id) else {
            return;
        };
        let record = self.ide_proposals.records.remove(index);
        if let Some(text) = record.view.read(cx).comments_text() {
            cx.write_to_clipboard(ClipboardItem::new_string(text));
            self.ide_toast(COMMENTS_COPIED_TOAST, cx);
        }
        self.ide_remove_view(record.view.entity_id(), cx);
        cx.defer(move |_| drop(record));
        cx.notify();
    }

    /// A connection closed: every proposal of it, open or parked, is closed and nothing is owed.
    pub(crate) fn ide_proposals_disconnected(&mut self, conn: ConnId, cx: &mut Context<Self>) {
        self.ide_proposals.waiting.retain(|_, waiting| *waiting != conn);
        self.ide_close_proposals(|record| record.conn == conn, cx);
    }

    /// The switch went off: no connection survives it, so no proposal does.
    pub(crate) fn ide_close_all_proposals(&mut self, cx: &mut Context<Self>) {
        self.ide_proposals.waiting.clear();
        self.ide_close_proposals(|_| true, cx);
    }

    fn ide_close_proposals(&mut self, matching: impl Fn(&Record) -> bool, cx: &mut Context<Self>) {
        let (doomed, kept): (Vec<Record>, Vec<Record>) = std::mem::take(&mut self.ide_proposals.records)
            .into_iter()
            .partition(|record| matching(record));
        self.ide_proposals.records = kept;
        if doomed.is_empty() {
            return;
        }
        for record in &doomed {
            self.ide_remove_view(record.view.entity_id(), cx);
        }
        self.ide_toast(CLOSED_TOAST, cx);
        cx.defer(move |_| drop(doomed));
        cx.notify();
    }

    // ---- the guard: every way of closing what holds a proposal asks first

    /// The proposals whose tab is among `ids`.
    fn proposal_views_in(&self, ids: &[usize]) -> Vec<EntityId> {
        let mut views = Vec::new();
        for tab in self.tabs.iter().filter(|tab| ids.contains(&tab.id)) {
            tab.panes.for_each(&mut |_, content| {
                if let TabContent::Proposal(view) = content
                    && self.ide_proposals.position_of_view(view.entity_id()).is_some()
                {
                    views.push(view.entity_id());
                }
            });
        }
        views
    }

    /// The tabs in `ids` hold a proposal: asks "Closing rejects Claude's change" and returns
    /// `true`, and the caller must not close anything. Reject answers each proposal and then
    /// does `then`.
    pub(crate) fn ide_guard_tab_close(&mut self, ids: &[usize], then: Then, cx: &mut Context<Self>) -> bool {
        if self.ide_proposals.is_empty() {
            return false;
        }
        let views = self.proposal_views_in(ids);
        if views.is_empty() {
            return false;
        }
        self.ide_ask(views, then, cx);
        true
    }

    /// Every proposal of `worktree`, open or parked.
    fn proposal_views_of_worktree(&self, worktree: &Path) -> Vec<EntityId> {
        self.ide_proposals
            .records
            .iter()
            .filter(|record| paths_name_the_same_document(Path::new(&record.worktree), worktree))
            .map(|record| record.view.entity_id())
            .collect()
    }

    /// The sidebar's "Remove worktree?" was confirmed (`WorktreeRemovalRequested`): when the
    /// worktree holds a proposal, asks before git touches the checkout or any session ends;
    /// Reject answers the proposals and then goes on, Cancel leaves everything as it was. With
    /// none, the removal goes on at once. This is the one entry the removal passes through.
    pub(crate) fn ide_request_worktree_removal(
        &mut self,
        row_id: usize,
        remote: Option<sirio_git::UpstreamBranch>,
        worktree: &Path,
        cx: &mut Context<Self>,
    ) {
        let views = self.proposal_views_of_worktree(worktree);
        if views.is_empty() {
            self.ide_continue_worktree_removal(row_id, remote, cx);
            return;
        }
        self.ide_ask(views, Then::RemoveWorktree { row_id, remote }, cx);
    }

    fn ide_continue_worktree_removal(
        &mut self,
        row_id: usize,
        remote: Option<sirio_git::UpstreamBranch>,
        cx: &mut Context<Self>,
    ) {
        self.sidebar.update(cx, |sidebar, cx| sidebar.remove_worktree_row(row_id, remote, cx));
    }

    /// The release `workspace.close` verb cannot ask: whether `worktree` holds a proposal.
    pub(crate) fn ide_worktree_holds_proposal(&self, worktree: &Path) -> bool {
        !self.proposal_views_of_worktree(worktree).is_empty()
    }

    /// Removing project `id`: asks first when any worktree of it holds a proposal.
    pub(crate) fn ide_guard_project_removal(&mut self, worktrees: &[PathBuf], id: &str, cx: &mut Context<Self>) -> bool {
        let views: Vec<EntityId> = worktrees
            .iter()
            .flat_map(|worktree| self.proposal_views_of_worktree(worktree))
            .collect();
        if views.is_empty() {
            return false;
        }
        self.ide_ask(views, Then::RemoveProject(id.to_owned()), cx);
        true
    }

    /// A worktree or project closed: whatever it parked goes with it (its proposals were
    /// answered by the confirmation first).
    pub(crate) fn ide_forget_worktree_proposals(&mut self, worktree_id: &str) {
        self.parked_proposals.remove(worktree_id);
        self.ide_proposals
            .records
            .retain(|record| !paths_name_the_same_document(Path::new(&record.worktree), Path::new(worktree_id)));
    }

    fn ide_ask(&mut self, views: Vec<EntityId>, then: Then, cx: &mut Context<Self>) {
        if self.ide_proposals.confirmation.is_some() {
            return;
        }
        self.ide_proposals.confirmation = Some(Confirmation {
            views,
            then,
            focus: cx.focus_handle().tab_stop(true),
            needs_focus: true,
        });
        cx.notify();
    }

    /// Reject: every proposal of the question is answered `DIFF_REJECTED`, then the close that
    /// was asked for goes on.
    pub(crate) fn ide_confirm_reject(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(confirmation) = self.ide_proposals.confirmation.take() else {
            return false;
        };
        let mut undelivered = false;
        let mut dropped = Vec::new();
        for view in &confirmation.views {
            let Some(index) = self.ide_proposals.position_of_view(*view) else {
                continue;
            };
            let record = self.ide_proposals.records.remove(index);
            undelivered |= answer_diff(cx, &record.id, DiffAnswer::Rejected).is_err();
            for parked in self.parked_proposals.values_mut() {
                parked.retain(|shown| shown.entity_id() != *view);
            }
            dropped.push(record);
        }
        self.parked_proposals.retain(|_, parked| !parked.is_empty());
        if undelivered {
            self.ide_toast(CLOSED_TOAST, cx);
        }
        cx.defer(move |_| drop(dropped));
        match confirmation.then {
            Then::CloseTabs(ids) => self.request_close_ids(ids, window, cx),
            Then::RemoveWorktree { row_id, remote } => self.ide_continue_worktree_removal(row_id, remote, cx),
            Then::RemoveProject(id) => {
                self.remove_project(&id, cx);
                self.refresh_closed_sessions(cx);
            }
        }
        cx.notify();
        true
    }

    /// Cancel: nothing is answered and nothing closes.
    pub(crate) fn ide_cancel_confirmation(&mut self, window: Option<&mut Window>, cx: &mut Context<Self>) -> bool {
        if self.ide_proposals.confirmation.take().is_none() {
            return false;
        }
        if let Some(window) = window {
            self.focus_active_pane(window, cx);
        }
        cx.notify();
        true
    }

    /// A proposal left while a question about it was up (Claude closed it): the question has
    /// nothing left to ask.
    fn ide_prune_confirmation(&mut self, cx: &mut Context<Self>) {
        let Some(confirmation) = &self.ide_proposals.confirmation else {
            return;
        };
        let any_left = confirmation
            .views
            .iter()
            .any(|view| self.ide_proposals.position_of_view(*view).is_some());
        if !any_left {
            self.ide_proposals.confirmation = None;
            cx.notify();
        }
    }

    /// The confirmation's keys: Escape and Enter cancel, the destructive answer has no key, and
    /// nothing else reaches what is underneath.
    pub(crate) fn handle_proposal_confirm_key(
        &mut self,
        event: &gpui::KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let key = event.keystroke.key.as_str();
        if key == "escape" || key == "enter" || key == "return" {
            self.ide_cancel_confirmation(Some(window), cx);
        }
    }

    pub(crate) fn render_proposal_confirm(&self, theme: Theme, entity: Entity<Self>) -> Option<AnyElement> {
        let confirmation = self.ide_proposals.confirmation.as_ref()?;
        let focus = confirmation.focus.clone();
        let (reject_entity, cancel_entity, key_entity) = (entity.clone(), entity.clone(), entity);
        Some(render_modal(
            ModalSpec {
                id: "proposal-close-confirm",
                title: CONFIRM_TITLE.into(),
                body: CONFIRM_BODY.into(),
                text_field: None,
                buttons: vec![
                    ModalButton::new("cancel", "Cancel", ModalButtonTone::Plain, move |_, window, cx| {
                        cancel_entity.update(cx, |workspace, cx| {
                            workspace.ide_cancel_confirmation(Some(window), cx);
                        });
                    }),
                    ModalButton::new("reject", "Reject", ModalButtonTone::Destructive, move |_, window, cx| {
                        reject_entity.update(cx, |workspace, cx| {
                            workspace.ide_confirm_reject(window, cx);
                        });
                    }),
                ],
                focus: Some(ModalFocus::new(focus, move |event, window, cx| {
                    cx.stop_propagation();
                    key_entity.update(cx, |workspace, cx| {
                        workspace.handle_proposal_confirm_key(event, window, cx);
                    });
                })),
            },
            theme,
        ))
    }

    // ---- the debug verb and state

    /// The `proposals` and `confirmation` of `surface.ide.state`: counts and names, no text.
    pub(crate) fn ide_proposals_state(&self, cx: &App) -> (Value, Value) {
        let proposals: Vec<Value> = self
            .ide_proposals
            .records
            .iter()
            .map(|record| {
                let tab = record.view.read(cx);
                json!({
                    "tab_name": record.tab_name,
                    "conn": record.conn.0,
                    "worktree": sirio_project::display_absolute_path(Path::new(&record.worktree)),
                    "on_screen": self.proposal_tab_id(record.view.entity_id()).is_some(),
                    "ready": tab.accept_live(),
                    "dirty": tab.is_dirty_in_editor(),
                    "refusal": tab.is_refused(),
                    "editing": tab.is_editing(),
                    "comments": tab.comment_count(),
                    "reject_label": tab.reject_label(),
                    "owner_named": tab.has_owner_title(),
                    "counts": tab.diff_counts().map(|(added, deleted)| json!([added, deleted])),
                })
            })
            .collect();
        let confirmation = self.ide_proposals.confirmation.as_ref().map(|confirmation| {
            json!({
                "title": CONFIRM_TITLE,
                "buttons": ["Reject", "Cancel"],
                "then": confirmation.then.name(),
                "proposals": confirmation
                    .views
                    .iter()
                    .filter_map(|view| self.ide_proposals.position_of_view(*view))
                    .map(|index| self.ide_proposals.records[index].tab_name.clone())
                    .collect::<Vec<_>>(),
            })
        });
        (json!(proposals), confirmation.unwrap_or(Value::Null))
    }

    /// Debug-only `surface.ide.act`: presses what a user presses, on the proposal named
    /// `tab_name`. `edit` with `text` puts it in the editor, and errors when Edit is not
    /// available (a refused or over-cap proposal). `confirm_reject` and `cancel` answer the
    /// close confirmation, and `close_others` is the tab menu's Close Others on the tab at the
    /// position `text` names; neither takes a `tab_name`. `close_worktree` closes the
    /// proposal's worktree the way a sidebar removal does (without git removing the checkout).
    pub(crate) fn control_ide_act(
        &mut self,
        tab_name: Option<&str>,
        action: &str,
        text: Option<&str>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<Vec<(String, String)>, String> {
        match action {
            "confirm_reject" => {
                return if self.ide_confirm_reject(window, cx) {
                    Ok(Vec::new())
                } else {
                    Err("no confirmation is open".to_owned())
                };
            }
            "cancel" => {
                return if self.ide_cancel_confirmation(Some(window), cx) {
                    Ok(Vec::new())
                } else {
                    Err("no confirmation is open".to_owned())
                };
            }
            // Close Others on the tab at position `text` (1-based): the tab menu's command.
            "close_others" => {
                let position: usize = text
                    .and_then(|text| text.parse().ok())
                    .ok_or_else(|| "close_others needs text=<position of the tab kept>".to_owned())?;
                self.tab_menu_tab = Some(self.listed_tab_id(position)?);
                self.request_close_other_tabs(window, cx);
                self.tab_menu_tab = None;
                return Ok(Vec::new());
            }
            _ => {}
        }
        let tab_name = tab_name.ok_or_else(|| format!("{action} needs a tab_name"))?;
        let (view, worktree) = self
            .ide_proposals
            .by_tab_name(tab_name)
            .map(|record| (record.view.clone(), record.worktree.clone()))
            .ok_or_else(|| "no proposal has that tab_name".to_owned())?;
        match action {
            "accept" => view.update(cx, |tab, cx| {
                if !tab.accept_live() {
                    return Err("Accept is not live yet: the diff is still being computed".to_owned());
                }
                tab.press(ProposalAction::Accept, window, cx);
                Ok(())
            })?,
            "reject" => view.update(cx, |tab, cx| tab.press(ProposalAction::Reject, window, cx)),
            "edit" => view.update(cx, |tab, cx| match text {
                Some(text) => {
                    if tab.set_edit_text(text, window, cx) {
                        Ok(())
                    } else {
                        Err("Edit is not available for this proposal".to_owned())
                    }
                }
                None if tab.can_edit() => {
                    tab.press(ProposalAction::Edit, window, cx);
                    Ok(())
                }
                None => Err("Edit is not available for this proposal".to_owned()),
            })?,
            "show_diff" => view.update(cx, |tab, cx| tab.press(ProposalAction::ShowDiff, window, cx)),
            "close" => {
                let tab_id = self
                    .proposal_tab_id(view.entity_id())
                    .ok_or_else(|| "the proposal has no tab on screen".to_owned())?;
                self.request_close_tab_by_id(tab_id, window, cx);
            }
            // What the sidebar's confirmed "Remove worktree?" does, through the same entry.
            "close_worktree" => {
                let path = PathBuf::from(&worktree);
                let row_id = self
                    .sidebar
                    .read(cx)
                    .worktree_row_for(&path)
                    .ok_or_else(|| "the proposal's worktree has no sidebar row".to_owned())?;
                self.handle_sidebar_event(
                    &SidebarEvent::WorktreeRemovalRequested { row_id, remote: None, path },
                    cx,
                );
            }
            // What the sidebar's confirmed "Remove project from Sirio?" does.
            "remove_project" => {
                let path = PathBuf::from(&worktree);
                let project_id = self
                    .project_catalog
                    .projects()
                    .iter()
                    .find(|project| {
                        project.worktrees.iter().any(|known| paths_name_the_same_document(&known.path, &path))
                    })
                    .map(|project| project.id.clone())
                    .ok_or_else(|| "the proposal's worktree is in no project".to_owned())?;
                self.handle_sidebar_event(&SidebarEvent::RemoveProject(project_id), cx);
            }
            other => return Err(format!("unknown surface.ide.act action: {other}")),
        }
        Ok(Vec::new())
    }
}

/// `✻ <file name>`, the proposal tab's title.
fn proposal_title(path: &Path, tab_name: &str) -> String {
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| tab_name.to_owned());
    format!("✻ {name}")
}
