//! The Connectors card: the Claude session's MCP servers, grouped by where
//! they were configured, each with its state and the one action that state
//! offers. Native Claude chats only; no other agent reports its servers.
//!
//! The card reads a cached list and what is in flight; nothing here blocks.
//! The three calls to the CLI wait for its answer, so they run through a
//! `McpControl` handle inside `background_spawn`, one task per server so a
//! second action on a server replaces the first.

use super::{Row, Section, Tone};
use crate::chat::Chat;
use gpui::{AppContext as _, Context as GpuiContext, Task};
use sirio_acp::{McpControl, McpOrigin, McpServer, McpState};
use std::collections::{HashMap, HashSet};
use std::time::Duration;

/// How often a sign-in is looked at while the browser step is open, and how
/// long it is looked at for. Past the bound the row offers Reconnect: the
/// step may have finished without the CLI noticing.
const POLL_EVERY: Duration = Duration::from_secs(3);
const POLL_FOR: Duration = Duration::from_secs(120);

/// How long after an ask that left a server starting the next one comes:
/// about a minute in all, then the card leaves the server as it is.
const STARTING_BACKOFF: [Duration; 5] = [
    Duration::from_secs(2),
    Duration::from_secs(4),
    Duration::from_secs(8),
    Duration::from_secs(16),
    Duration::from_secs(30),
];

/// What a server's row says while a call about it is open: the cached state
/// is known to be stale, so it is not shown.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Busy {
    Reconnecting,
    /// `mcp_authenticate` has not answered yet.
    SigningIn,
    /// The sign-in page is open and the poll is watching.
    Waiting,
}

impl Busy {
    fn word(self) -> &'static str {
        match self {
            Self::Reconnecting => "reconnecting…",
            Self::SigningIn => "signing in…",
            Self::Waiting => "waiting for sign-in…",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Action {
    SignIn,
    Reconnect,
}

impl Action {
    fn label(self) -> &'static str {
        match self {
            Self::SignIn => "Sign in",
            Self::Reconnect => "Reconnect",
        }
    }
}

#[derive(Default)]
pub(super) struct Connectors {
    /// The last list read, without the servers that have no name; `None`
    /// until one has been read, which is not the same as a list of none.
    servers: Option<Vec<McpServer>>,
    /// Whether a status was asked for since this client came up: the init
    /// list names no origin, so the first ask follows it once, and what that
    /// ask itself announces never starts another.
    asked: bool,
    refreshing: bool,
    refresh_task: Option<Task<()>>,
    /// The call open for each server, by name.
    busy: HashMap<String, Busy>,
    /// The CLI's sentence for an action that failed, with the stamp it was
    /// kept under, until a refresh asked for after it, the column opening
    /// again or the server being acted on again replaces it.
    failures: HashMap<String, (String, u64)>,
    /// Counts what is asked and what fails, to tell which came after which.
    stamp: u64,
    /// Servers whose sign-in poll ran out still needing one.
    reconnect_offered: HashSet<String>,
    /// The task of each server's open call or poll.
    tasks: HashMap<String, Task<()>>,
    /// Whether the backoff for servers still starting is running, and its
    /// task. One at a time: it ends on its own, or with the column.
    backoff_running: bool,
    backoff_task: Option<Task<()>>,
}

impl Connectors {
    /// A new client is a new session: nothing of the last one is current.
    pub(super) fn clear(&mut self) {
        *self = Self::default();
    }

    fn next_stamp(&mut self) -> u64 {
        self.stamp += 1;
        self.stamp
    }

    fn has_starting(&self) -> bool {
        self.servers
            .iter()
            .flatten()
            .any(|server| server.status == McpState::Pending)
    }

    /// The column closed: nothing is waiting for a server to start.
    pub(super) fn stop_backoff(&mut self) {
        self.backoff_running = false;
        self.backoff_task = None;
    }

    fn fail(&mut self, name: String, text: String) {
        let stamp = self.next_stamp();
        self.failures.insert(name, (text, stamp));
    }

    /// Takes a list that answers a refresh asked for at `since`: it is newer
    /// than every failure stamped up to then, so those are no longer current.
    fn refreshed(&mut self, list: Vec<McpServer>, since: u64) {
        self.set_servers(list);
        self.failures.retain(|_, (_, stamp)| *stamp > since);
    }

    /// Keeps a list. A server that no longer needs a sign-in has no reason
    /// left to offer Reconnect in its place: a later one starts over.
    fn set_servers(&mut self, list: Vec<McpServer>) {
        let servers = listed(list);
        self.reconnect_offered.retain(|name| {
            servers
                .iter()
                .any(|server| &server.name == name && server.status == McpState::NeedsAuth)
        });
        self.servers = Some(servers);
    }

    fn action(&self, server: &McpServer) -> Option<Action> {
        match self.busy.get(&server.name) {
            Some(Busy::Waiting) => return Some(Action::SignIn),
            Some(_) => return None,
            None => {}
        }
        match server.status {
            McpState::NeedsAuth if self.reconnect_offered.contains(&server.name) => {
                Some(Action::Reconnect)
            }
            McpState::NeedsAuth => Some(Action::SignIn),
            McpState::Failed => Some(Action::Reconnect),
            _ => None,
        }
    }
}

/// The servers a card can show: one with no name cannot be asked about.
fn listed(list: Vec<McpServer>) -> Vec<McpServer> {
    list.into_iter().filter(|server| !server.name.is_empty()).collect()
}

fn origin_slug(origin: &McpOrigin) -> (&'static str, &'static str) {
    match origin {
        McpOrigin::ClaudeAi => ("claude.ai", "claude.ai"),
        McpOrigin::Plugin => ("plugins", "Plugins"),
        McpOrigin::User => ("user", "User"),
        McpOrigin::Project => ("project", "Project"),
        McpOrigin::Other(_) => ("other", "Other"),
    }
}

/// The groups in the order the card draws them.
const GROUPS: [&str; 5] = ["claude.ai", "plugins", "user", "project", "other"];

fn server_key(server: &McpServer) -> String {
    format!("connectors/{}/{}", origin_slug(&server.origin).0, server.name)
}

fn state_word(state: &McpState) -> String {
    match state {
        McpState::Connected => "connected".to_string(),
        McpState::Pending => "starting".to_string(),
        McpState::NeedsAuth => "needs sign-in".to_string(),
        McpState::Failed => "failed".to_string(),
        McpState::Disabled => "disabled".to_string(),
        McpState::Other(word) if word.is_empty() => "unknown".to_string(),
        McpState::Other(word) => word.clone(),
    }
}

fn tone(state: &McpState) -> Tone {
    match state {
        McpState::Connected => Tone::Good,
        McpState::NeedsAuth | McpState::Pending => Tone::Warn,
        McpState::Failed => Tone::Bad,
        McpState::Disabled | McpState::Other(_) => Tone::Quiet,
    }
}

fn needs_attention(state: &McpState) -> bool {
    matches!(state, McpState::NeedsAuth | McpState::Failed)
}

/// The card, or `None` for an agent that does not report its servers.
/// `flipped` holds the groups the reader closed; it is read, never changed.
pub(super) fn section(
    connectors: &Connectors,
    supported: bool,
    flipped: &HashSet<String>,
) -> Option<Section> {
    if !supported {
        return None;
    }
    let Some(servers) = &connectors.servers else {
        return Some(Section {
            key: "connectors",
            title: "Connectors",
            summary: String::new(),
            note: String::new(),
            bar: Vec::new(),
            rows: vec![Row::new("connectors/none", "Reading connectors…".to_string(), "")],
        });
    };
    let signing_in = servers
        .iter()
        .filter(|server| server.status == McpState::NeedsAuth)
        .count();
    let mut summary = servers.len().to_string();
    if signing_in > 0 {
        summary.push_str(&format!(" · {signing_in} need sign-in"));
    }
    let mut rows = Vec::new();
    for slug in GROUPS {
        let mut group: Vec<&McpServer> = servers
            .iter()
            .filter(|server| origin_slug(&server.origin).0 == slug)
            .collect();
        if group.is_empty() {
            continue;
        }
        group.sort_by_cached_key(|server| {
            (
                !needs_attention(&server.status),
                server.name.to_lowercase(),
                server.name.clone(),
            )
        });
        let key = format!("connectors/{slug}");
        let open = !flipped.contains(&key);
        let label = origin_slug(&group[0].origin).1;
        rows.push(Row {
            expanded: Some(open),
            ..Row::new(&key, label.to_string(), &group.len().to_string())
        });
        if !open {
            continue;
        }
        for server in group {
            let action = connectors.action(server);
            // The sentence of a failed action goes under the row in full; the
            // row keeps saying what state the server is in.
            let (detail, tone, note) = match connectors.busy.get(&server.name) {
                Some(busy) => (busy.word().to_string(), Tone::Quiet, String::new()),
                None => (
                    state_word(&server.status),
                    tone(&server.status),
                    connectors
                        .failures
                        .get(&server.name)
                        .map(|(failure, _)| failure.clone())
                        .unwrap_or_default(),
                ),
            };
            rows.push(Row {
                depth: 1,
                tone: Some(tone),
                action: action.map(|action| action.label().to_string()),
                show_action: true,
                note,
                ..Row::new(&server_key(server), server.name.clone(), &detail)
            });
        }
    }
    if rows.is_empty() {
        rows.push(Row::new("connectors/none", "No connectors".to_string(), ""));
    }
    Some(Section {
        key: "connectors",
        title: "Connectors",
        summary,
        note: String::new(),
        bar: Vec::new(),
        rows,
    })
}

impl Chat {
    /// Takes the client's latest list into the card. An answer to an ask
    /// lands here too, which is why this never asks again by itself beyond
    /// the first time.
    pub(in crate::chat) fn refresh_session_connectors(&mut self, cx: &mut GpuiContext<Self>) {
        if let Some(list) = self.client.as_ref().and_then(|client| client.mcp_servers()) {
            self.session_panel.connectors.set_servers(list);
            cx.notify();
        }
        self.ensure_session_connectors(cx);
    }

    /// The CLI printed `system/init` again (a session start, a turn): its
    /// list names no origin and may be older than the servers' own state, so
    /// the card asks for the status again, and a backoff for servers still
    /// starting begins anew. What the answer itself announces comes through
    /// `refresh_session_connectors` and asks for nothing.
    pub(in crate::chat) fn session_connectors_init(&mut self, cx: &mut GpuiContext<Self>) {
        if let Some(list) = self.client.as_ref().and_then(|client| client.mcp_servers()) {
            self.session_panel.connectors.set_servers(list);
            cx.notify();
        }
        if self.session_panel.open {
            self.session_panel.connectors.stop_backoff();
            self.ask_session_connectors(cx);
        }
    }

    /// Asks the CLI for each server's state once per client, when the column
    /// is open.
    pub(in crate::chat) fn ensure_session_connectors(&mut self, cx: &mut GpuiContext<Self>) {
        if self.session_panel.open && !self.session_panel.connectors.asked {
            self.ask_session_connectors(cx);
        }
    }

    /// The column was opened: what a failed action said is old news, and the
    /// states are read again.
    pub(in crate::chat) fn reopen_session_connectors(&mut self, cx: &mut GpuiContext<Self>) {
        self.session_panel.connectors.failures.clear();
        self.ask_session_connectors(cx);
    }

    fn ask_session_connectors(&mut self, cx: &mut GpuiContext<Self>) {
        let Some(control) = self.client.as_ref().and_then(|client| client.mcp_control()) else {
            return;
        };
        let connectors = &mut self.session_panel.connectors;
        connectors.asked = true;
        if connectors.refreshing {
            return;
        }
        connectors.refreshing = true;
        let since = connectors.next_stamp();
        connectors.refresh_task = Some(cx.spawn(async move |this, cx| {
            let answer = cx
                .background_spawn(async move { control.refresh_status() })
                .await;
            let _ = this.update(cx, |chat, cx| {
                let connectors = &mut chat.session_panel.connectors;
                connectors.refreshing = false;
                if let Ok(list) = answer {
                    connectors.refreshed(list, since);
                }
                cx.notify();
                chat.watch_starting_connectors(cx);
            });
        }));
    }

    /// A server that is still starting changes without anyone asking, so the
    /// card asks again after 2, 4, 8, 16 and 30 seconds, until none is
    /// starting or the minute is out. One loop at a time; its own answers
    /// never begin another.
    fn watch_starting_connectors(&mut self, cx: &mut GpuiContext<Self>) {
        let connectors = &mut self.session_panel.connectors;
        if !self.session_panel.open || connectors.backoff_running || !connectors.has_starting() {
            return;
        }
        let Some(control) = self.client.as_ref().and_then(|client| client.mcp_control()) else {
            return;
        };
        connectors.backoff_running = true;
        connectors.backoff_task = Some(cx.spawn(async move |this, cx| {
            for delay in STARTING_BACKOFF {
                cx.background_executor().timer(delay).await;
                let Ok(since) = this.update(cx, |chat, _| chat.session_panel.connectors.next_stamp())
                else {
                    return;
                };
                let asking = control.clone();
                let Ok(list) = cx
                    .background_spawn(async move { asking.refresh_status() })
                    .await
                else {
                    break;
                };
                let still_starting = this.update(cx, |chat, cx| {
                    let connectors = &mut chat.session_panel.connectors;
                    connectors.refreshed(list, since);
                    cx.notify();
                    connectors.has_starting()
                });
                if !matches!(still_starting, Ok(true)) {
                    break;
                }
            }
            let _ = this.update(cx, |chat, _| chat.session_panel.connectors.backoff_running = false);
        }));
    }

    /// Runs the action of a server's row: Sign in or Reconnect.
    pub(in crate::chat) fn activate_connector(
        &mut self,
        key: &str,
        cx: &mut GpuiContext<Self>,
    ) -> Result<(), String> {
        let connectors = &self.session_panel.connectors;
        let found = connectors
            .servers
            .iter()
            .flatten()
            .find(|server| server_key(server) == key)
            .and_then(|server| connectors.action(server).map(|action| (server.name.clone(), action)));
        let Some((name, action)) = found else {
            return Err(format!("no row with an action has key {key}"));
        };
        let Some(control) = self.client.as_ref().and_then(|client| client.mcp_control()) else {
            return Err("this session has no connectors".to_string());
        };
        let connectors = &mut self.session_panel.connectors;
        connectors.failures.remove(&name);
        connectors.reconnect_offered.remove(&name);
        let task = match action {
            Action::Reconnect => {
                connectors.busy.insert(name.clone(), Busy::Reconnecting);
                Self::reconnect_connector(name.clone(), control, cx)
            }
            Action::SignIn => {
                connectors.busy.insert(name.clone(), Busy::SigningIn);
                Self::sign_in_connector(name.clone(), control, cx)
            }
        };
        // Replacing a server's task drops the one before it, which is how a
        // second Sign in restarts the poll instead of doubling it.
        connectors.tasks.insert(name, task);
        cx.notify();
        Ok(())
    }

    fn reconnect_connector(
        name: String,
        control: McpControl,
        cx: &mut GpuiContext<Self>,
    ) -> Task<()> {
        cx.spawn(async move |this, cx| {
            let asked = name.clone();
            let again = control.clone();
            let outcome = cx
                .background_spawn(async move { control.reconnect(&asked) })
                .await;
            let answer = cx
                .background_spawn(async move { again.refresh_status() })
                .await;
            let _ = this.update(cx, |chat, cx| {
                let connectors = &mut chat.session_panel.connectors;
                connectors.busy.remove(&name);
                // This answer is the action's own follow-up, so it does not
                // retire the sentence the action may have just earned.
                if let Ok(list) = answer {
                    connectors.set_servers(list);
                }
                if let Err(error) = outcome {
                    connectors.fail(name, format!("{error:#}"));
                }
                cx.notify();
            });
        })
    }

    /// Starts the sign-in, opens its page in the system browser (the CLI's
    /// callback listens on this machine) and then looks at the server every
    /// few seconds until it stops needing one, the bound ends or the session
    /// does.
    fn sign_in_connector(
        name: String,
        control: McpControl,
        cx: &mut GpuiContext<Self>,
    ) -> Task<()> {
        cx.spawn(async move |this, cx| {
            let asked = name.clone();
            let starting = control.clone();
            let started = cx
                .background_spawn(async move { starting.authenticate(&asked) })
                .await;
            let url = match started {
                Ok(start) => start.auth_url,
                Err(error) => {
                    let _ = this.update(cx, |chat, cx| {
                        let connectors = &mut chat.session_panel.connectors;
                        connectors.busy.remove(&name);
                        connectors.fail(name, format!("{error:#}"));
                        cx.notify();
                    });
                    return;
                }
            };
            let opened = this.update(cx, |chat, cx| {
                let connectors = &mut chat.session_panel.connectors;
                match url.as_deref() {
                    // Only a web page: the CLI's answer is not trusted to
                    // name a program for the system to launch.
                    Some(url) if url.starts_with("http://") || url.starts_with("https://") => {
                        cx.open_url(url);
                        connectors.busy.insert(name.clone(), Busy::Waiting);
                    }
                    _ => {
                        connectors.busy.remove(&name);
                        connectors.fail(name.clone(), "the sign-in has no page to open".to_string());
                    }
                }
                cx.notify();
                connectors.busy.contains_key(&name)
            });
            if !matches!(opened, Ok(true)) {
                return;
            }
            let started = cx.background_executor().now();
            // Whether the poll ran out with the server still needing a sign-in.
            let mut offer_reconnect = true;
            while cx.background_executor().now().duration_since(started) < POLL_FOR {
                cx.background_executor().timer(POLL_EVERY).await;
                let Ok(since) = this.update(cx, |chat, _| chat.session_panel.connectors.next_stamp())
                else {
                    break;
                };
                let asking = control.clone();
                let Ok(list) = cx
                    .background_spawn(async move { asking.refresh_status() })
                    .await
                else {
                    // The session is gone or the CLI is not answering.
                    break;
                };
                let status = list
                    .iter()
                    .find(|server| server.name == name)
                    .map(|server| server.status.clone());
                let alive = this.update(cx, |chat, cx| {
                    chat.session_panel.connectors.refreshed(list, since);
                    cx.notify();
                });
                if alive.is_err() {
                    break;
                }
                // Signed in, or no longer listed at all: nothing left to wait for.
                if status != Some(McpState::NeedsAuth) {
                    offer_reconnect = false;
                    break;
                }
            }
            let _ = this.update(cx, |chat, cx| {
                let connectors = &mut chat.session_panel.connectors;
                connectors.busy.remove(&name);
                if offer_reconnect {
                    connectors.reconnect_offered.insert(name);
                }
                cx.notify();
            });
        })
    }
}

#[cfg(test)]
mod tests {
    use super::super::{SectionSnapshot, SessionPanelAction};
    use crate::chat::tests::{TempDir, chat_view, pump_chat_until, refresh_frame};
    use crate::chat::{Chat, LaunchSpec};
    use gpui::{Entity, TestAppContext, VisualTestContext};
    use sirio_acp::{ChatClient, ClaudeLaunch};
    use sirio_theme::Theme;
    use std::path::Path;
    use std::time::Duration;

    const CLAUDE_FIXTURE: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../sirio_acp/tests/fixtures/claude_fixture.py"
    );

    /// A chat on the native transport against the fixture CLI in its
    /// `session_panel` mode, with the column already open.
    fn native_chat(cx: &mut TestAppContext) -> (Entity<Chat>, &mut VisualTestContext) {
        fixture_chat(cx, "session_panel", None)
    }

    /// The same against the fixture in `mode`; `counts` names the file the
    /// fixture writes a line to for each `mcp_status` request, passed on its
    /// command line so no process-wide environment is touched.
    fn fixture_chat<'a>(
        cx: &'a mut TestAppContext,
        mode: &str,
        counts: Option<&Path>,
    ) -> (Entity<Chat>, &'a mut VisualTestContext) {
        // The CLI's worker thread wakes the chat's event task, which the
        // test scheduler refuses unless it is told real threads are in play.
        cx.executor().allow_parking();
        cx.update(Theme::init);
        cx.update(bezel::ui::input::init);
        cx.update(crate::chat::init);
        let (chat, cx) = cx.add_window_view(|_, cx| {
            let mut args = vec![CLAUDE_FIXTURE.to_string(), mode.to_string()];
            if let Some(counts) = counts {
                args.push(format!("--fixture-count={}", counts.display()));
            }
            let launch = ClaudeLaunch::fixture("python3", args);
            Chat::from_test_command(LaunchSpec::Claude(launch), std::env::temp_dir(), cx)
        });
        chat.update(cx, |chat, cx| chat.set_session_panel_open(true, cx));
        (chat, cx)
    }

    fn connectors(chat: &Entity<Chat>, cx: &VisualTestContext) -> Option<SectionSnapshot> {
        chat.read_with(&cx.cx, |chat, _| chat.control_snapshot())
            .session_panel
            .sections
            .into_iter()
            .find(|section| section.key == "connectors")
    }

    /// The row's detail and action, or `None` while the row is not there.
    fn row(
        chat: &Entity<Chat>,
        cx: &VisualTestContext,
        key: &str,
    ) -> Option<(String, Option<String>)> {
        connectors(chat, cx)?
            .rows
            .into_iter()
            .find(|row| row.key == key)
            .map(|row| (row.detail, row.action))
    }

    /// Waits on real time, with the virtual clock still, until `condition`
    /// holds: a sign-in's poll must not run ahead of the test.
    fn settle(cx: &mut VisualTestContext, mut condition: impl FnMut(&VisualTestContext) -> bool) {
        cx.cx.executor().allow_parking();
        for _ in 0..1500 {
            if condition(cx) {
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
            cx.cx.run_until_parked();
        }
        panic!("the condition never held");
    }

    fn activate(chat: &Entity<Chat>, cx: &mut VisualTestContext, key: &str) {
        chat.update_in(cx, |chat, window, cx| {
            chat.session_panel_act(SessionPanelAction::Activate(key.to_string()), window, cx)
        })
        .expect("the row has an action");
    }

    const NOTION: &str = "connectors/claude.ai/claude.ai Notion";
    const CODEGRAPH: &str = "connectors/user/codegraph";

    #[gpui::test]
    async fn connectors_are_grouped_by_origin(cx: &mut TestAppContext) {
        let (chat, cx) = native_chat(cx);
        settle(cx, |cx| row(&chat, cx, NOTION).is_some_and(|(d, _)| d == "needs sign-in"));
        refresh_frame(cx);
        let section = connectors(&chat, cx).expect("a connectors section");
        assert_eq!(section.summary, "2 · 1 need sign-in");
        let keys: Vec<&str> = section.rows.iter().map(|row| row.key.as_str()).collect();
        assert_eq!(
            keys,
            [
                "connectors/claude.ai",
                NOTION,
                "connectors/user",
                CODEGRAPH,
            ]
        );
        assert_eq!(
            row(&chat, cx, NOTION),
            Some(("needs sign-in".into(), Some("Sign in".into())))
        );
        assert_eq!(
            row(&chat, cx, CODEGRAPH),
            Some(("failed".into(), Some("Reconnect".into())))
        );
    }

    #[gpui::test]
    async fn reconnect_refreshes_the_row(cx: &mut TestAppContext) {
        let (chat, cx) = native_chat(cx);
        settle(cx, |cx| row(&chat, cx, CODEGRAPH).is_some_and(|(d, _)| d == "failed"));
        activate(&chat, cx, CODEGRAPH);
        settle(cx, |cx| row(&chat, cx, CODEGRAPH) == Some(("connected".into(), None)));
    }

    #[gpui::test]
    async fn sign_in_opens_the_url_and_polls(cx: &mut TestAppContext) {
        let (chat, cx) = native_chat(cx);
        settle(cx, |cx| row(&chat, cx, NOTION).is_some());
        activate(&chat, cx, NOTION);
        settle(cx, |cx| {
            row(&chat, cx, NOTION).is_some_and(|(d, _)| d == "waiting for sign-in…")
        });
        assert_eq!(
            cx.cx.opened_url().as_deref(),
            Some("http://127.0.0.1:9/authorize")
        );
    }

    #[gpui::test]
    async fn sign_in_polling_stops_after_its_bound(cx: &mut TestAppContext) {
        let dir = TempDir::new();
        let counts = dir.0.join("mcp-status-requests");
        let (chat, cx) = fixture_chat(cx, "session_panel", Some(&counts));
        settle(cx, |cx| row(&chat, cx, NOTION).is_some());
        activate(&chat, cx, NOTION);
        settle(cx, |cx| {
            row(&chat, cx, NOTION).is_some_and(|(d, _)| d == "waiting for sign-in…")
        });
        // Past the 120 s bound: the clock moves three seconds at a time and
        // each step lets the poll's timer fire. The CLI is a real process, so
        // its answers come back in real time; a short yield between steps is
        // all that waits on it. The server never signs in, so the bound is
        // what ends the poll.
        for _ in 0..5000 {
            if row(&chat, cx, NOTION).is_some_and(|(d, _)| d != "waiting for sign-in…") {
                break;
            }
            cx.cx.executor().advance_clock(Duration::from_secs(3));
            cx.cx.run_until_parked();
            std::thread::sleep(Duration::from_millis(2));
        }
        // The browser step may have finished without the CLI noticing, so
        // the row offers Reconnect rather than another Sign in.
        assert_eq!(
            row(&chat, cx, NOTION),
            Some(("needs sign-in".into(), Some("Reconnect".into())))
        );
        let asked = count_lines(&counts);
        // Absence has nothing to wait on: let a minute of virtual time pass
        // and give a stray poll a moment of real time to show itself.
        cx.cx.executor().advance_clock(Duration::from_secs(60));
        cx.cx.run_until_parked();
        std::thread::sleep(Duration::from_millis(200));
        cx.cx.run_until_parked();
        assert_eq!(count_lines(&counts), asked, "nothing asks once the bound has ended");
        // It polled, and not twice over as a doubled loop would.
        assert!((2..=45).contains(&asked), "the poll's requests: {asked}");
    }

    fn count_lines(path: &Path) -> usize {
        std::fs::read_to_string(path).map_or(0, |text| text.lines().count())
    }

    /// Moves the virtual clock a second at a time, letting each step's
    /// real-process answers come back, until `condition` holds.
    fn advance_until(
        cx: &mut VisualTestContext,
        seconds: u64,
        mut condition: impl FnMut(&VisualTestContext) -> bool,
    ) -> bool {
        for _ in 0..seconds {
            if condition(cx) {
                return true;
            }
            cx.cx.executor().advance_clock(Duration::from_secs(1));
            cx.cx.run_until_parked();
            std::thread::sleep(Duration::from_millis(5));
            cx.cx.run_until_parked();
        }
        condition(cx)
    }

    #[gpui::test]
    async fn a_server_that_is_still_starting_is_asked_about_again(cx: &mut TestAppContext) {
        let dir = TempDir::new();
        let counts = dir.0.join("mcp-status-requests");
        let (chat, cx) = fixture_chat(cx, "session_panel_starting", Some(&counts));
        settle(cx, |cx| row(&chat, cx, CODEGRAPH).is_some_and(|(d, _)| d == "starting"));
        // Nobody touches the column: the card asks again on its own.
        assert!(
            advance_until(cx, 60, |cx| row(&chat, cx, CODEGRAPH)
                .is_some_and(|(d, _)| d == "connected")),
            "the server came up and the card never heard"
        );
        // The third answer was the one that said so: one at the start and
        // two backoff asks.
        let asked = count_lines(&counts);
        assert!((3..=4).contains(&asked), "asked {asked} times");
        advance_until(cx, 200, |_| false);
        std::thread::sleep(Duration::from_millis(100));
        cx.cx.run_until_parked();
        assert_eq!(count_lines(&counts), asked, "nothing is pending, so nothing asks");
    }

    #[gpui::test]
    async fn asking_again_about_a_server_that_never_starts_stops(cx: &mut TestAppContext) {
        let dir = TempDir::new();
        let counts = dir.0.join("mcp-status-requests");
        let (chat, cx) = fixture_chat(cx, "session_panel_pending", Some(&counts));
        settle(cx, |cx| row(&chat, cx, CODEGRAPH).is_some_and(|(d, _)| d == "starting"));
        advance_until(cx, 90, |_| false);
        std::thread::sleep(Duration::from_millis(100));
        cx.cx.run_until_parked();
        let asked = count_lines(&counts);
        // The first ask, then 2, 4, 8, 16 and 30 seconds after.
        assert!((4..=7).contains(&asked), "the backoff's asks: {asked}");
        advance_until(cx, 200, |_| false);
        std::thread::sleep(Duration::from_millis(100));
        cx.cx.run_until_parked();
        assert_eq!(count_lines(&counts), asked, "the backoff ended after about a minute");
        assert_eq!(
            row(&chat, cx, CODEGRAPH).map(|(detail, _)| detail).as_deref(),
            Some("starting")
        );
    }

    #[gpui::test]
    async fn a_new_init_asks_again_and_an_answers_own_signal_does_not(cx: &mut TestAppContext) {
        use sirio_acp::AcpEvent;
        let dir = TempDir::new();
        let counts = dir.0.join("mcp-status-requests");
        let (chat, cx) = fixture_chat(cx, "session_panel", Some(&counts));
        settle(cx, |cx| row(&chat, cx, NOTION).is_some() && count_lines(&counts) == 1);
        let signal = |chat: &Entity<Chat>, cx: &mut VisualTestContext, kind: &str| {
            chat.update(cx, |chat, cx| {
                chat.handle_event(
                    AcpEvent::OtherSessionUpdate { kind: kind.to_string() },
                    cx,
                )
            });
        };
        signal(&chat, cx, "McpServersInit");
        settle(cx, |_| count_lines(&counts) == 2);
        // What the CLI announces after answering must not be asked about
        // again, or each answer would bring the next.
        signal(&chat, cx, "McpServers");
        std::thread::sleep(Duration::from_millis(200));
        cx.cx.run_until_parked();
        assert_eq!(count_lines(&counts), 2);
    }

    /// What the row says under itself, in full.
    fn note(chat: &Entity<Chat>, cx: &VisualTestContext, key: &str) -> Option<String> {
        connectors(chat, cx)?
            .rows
            .into_iter()
            .find(|row| row.key == key)
            .map(|row| row.note)
    }

    #[gpui::test]
    async fn a_refused_reconnect_shows_the_cli_sentence_until_the_next_refresh(
        cx: &mut TestAppContext,
    ) {
        let (chat, cx) = fixture_chat(cx, "session_panel_refuse", None);
        settle(cx, |cx| row(&chat, cx, CODEGRAPH).is_some_and(|(d, _)| d == "failed"));
        activate(&chat, cx, CODEGRAPH);
        settle(cx, |cx| note(&chat, cx, CODEGRAPH).is_some_and(|note| !note.is_empty()));
        let sentence = note(&chat, cx, CODEGRAPH).unwrap();
        assert!(
            sentence.ends_with("Connection refused: codegraph did not start (exit 1)"),
            "the CLI's whole sentence, not a cut one: {sentence}"
        );
        // The server is still what it was, and can be tried again.
        assert_eq!(
            row(&chat, cx, CODEGRAPH),
            Some(("failed".into(), Some("Reconnect".into())))
        );
        // Opening the column reads the states again; what failed before is
        // old news.
        chat.update(cx, |chat, cx| chat.set_session_panel_open(false, cx));
        chat.update(cx, |chat, cx| chat.set_session_panel_open(true, cx));
        settle(cx, |cx| note(&chat, cx, CODEGRAPH) == Some(String::new()));
    }

    #[gpui::test]
    async fn an_acp_chat_has_no_connectors_section(cx: &mut TestAppContext) {
        let (chat, cx) = chat_view(cx, &["plain"]);
        pump_chat_until(cx, &chat, |chat| chat.client.is_some());
        assert!(
            chat.read_with(&cx.cx, |chat, _| matches!(chat.client, Some(ChatClient::Acp(_)))),
            "the fixture agent speaks ACP"
        );
        chat.update(cx, |chat, cx| chat.set_session_panel_open(true, cx));
        refresh_frame(cx);
        assert!(connectors(&chat, cx).is_none());
    }
}
