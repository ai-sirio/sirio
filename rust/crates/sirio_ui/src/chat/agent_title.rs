//! The name Sirio asked the native Claude session to carry (A4).
//!
//! Only a manual rename of the tab reaches here, through
//! `Workspace::apply_manual_tab_title`. The wanted title lives as long as the
//! `Chat`: with no client it waits (`pending`), and every connection that
//! becomes ready sends it once, because a new process knows nothing of an
//! earlier one's `accepted`, `failed` or `retired`. A connection that ends
//! puts a wanted title back to `pending`, clears the retirement and the
//! error: what the next connection will say is not yet known. Sirio never
//! adopts a name the CLI reports; this is one way only.
//!
//! The CLI's answer never touches the tab. A refusal retires the feature for
//! the connection and says nothing; any other error is that one rename's
//! failure and shows in the transcript.

use gpui::Context;

use super::{Chat, Entry, NoticeKind};

/// Where the wanted title stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum TitleState {
    /// Held: there is no connection to send it on.
    Pending,
    /// Written to the connection; its answer has not come yet.
    Sent,
    Accepted,
    Failed,
    /// The CLI refused the verb: nothing is sent until the next connection.
    Retired,
}

impl TitleState {
    fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Sent => "sent",
            Self::Accepted => "accepted",
            Self::Failed => "failed",
            Self::Retired => "retired",
        }
    }
}

/// The one record: the title wanted, where it stands, and why it failed.
#[derive(Debug)]
pub(super) struct AgentTitle {
    wanted: Option<String>,
    state: TitleState,
    error: Option<String>,
    retired: bool,
}

impl AgentTitle {
    pub(super) fn new() -> Self {
        Self {
            wanted: None,
            state: TitleState::Pending,
            error: None,
            retired: false,
        }
    }
}

impl Chat {
    /// Asks the CLI to carry `title` as its session's name. A chat that is
    /// not native Claude has nothing to ask and keeps no title.
    pub fn rename_agent_session(&mut self, title: String, cx: &mut Context<Self>) {
        if !self
            .agent_launch
            .as_ref()
            .is_some_and(sirio_chat::LaunchSpec::is_native_claude)
        {
            return;
        }
        self.agent_title.wanted = Some(title);
        self.agent_title.error = None;
        if self.client.is_none() {
            self.agent_title.state = TitleState::Pending;
        } else if self.agent_title.retired {
            self.agent_title.state = TitleState::Retired;
        } else {
            self.send_wanted_title();
        }
        cx.notify();
    }

    /// A connection just became ready: it is sent the wanted title, whatever
    /// the last connection's answer was.
    pub(super) fn send_title_on_ready(&mut self) {
        self.agent_title.retired = false;
        if self.agent_title.wanted.is_some() {
            self.send_wanted_title();
        }
    }

    fn send_wanted_title(&mut self) {
        let (Some(client), Some(title)) = (&self.client, self.agent_title.wanted.clone()) else {
            return;
        };
        self.agent_title.state = match client.rename_session(title) {
            Ok(()) => TitleState::Sent,
            // The writer is gone; the transport error that follows ends this
            // connection and the next one sends the title again.
            Err(_) => TitleState::Pending,
        };
    }

    /// The connection ended: a wanted title waits for a new connection, which
    /// sends it afresh, and the old connection's refusal and error end with it.
    pub(super) fn reset_agent_title(&mut self) {
        if self.agent_title.wanted.is_some() {
            self.agent_title.state = TitleState::Pending;
            self.agent_title.retired = false;
            self.agent_title.error = None;
        }
    }

    /// The CLI answered a rename. `title` is the one that was sent.
    pub(super) fn apply_session_renamed(&mut self, title: &str, error: Option<String>) {
        // Anything else is an answer to an older rename or an older
        // connection.
        if self.client.is_none()
            || self.agent_title.state != TitleState::Sent
            || self.agent_title.wanted.as_deref() != Some(title)
        {
            return;
        }
        match error {
            None => self.agent_title.state = TitleState::Accepted,
            Some(error) if sirio_chat::is_refusal(&error) => {
                self.agent_title.retired = true;
                self.agent_title.state = TitleState::Retired;
            }
            Some(error) => {
                self.push_entry(Entry::Notice {
                    text: format!("Claude Code kept its earlier name: {error}"),
                    kind: NoticeKind::Warning,
                });
                self.agent_title.error = Some(error);
                self.agent_title.state = TitleState::Failed;
            }
        }
    }

    /// `surface.chat.read`'s `agentTitle`: `""` until a manual rename.
    pub(super) fn agent_title_json(&self) -> String {
        let Some(title) = &self.agent_title.wanted else {
            return String::new();
        };
        let mut value = serde_json::json!({
            "title": title,
            "state": self.agent_title.state.as_str(),
        });
        if self.agent_title.state == TitleState::Failed
            && let Some(error) = &self.agent_title.error
        {
            value["error"] = serde_json::Value::String(error.clone());
        }
        serde_json::to_string(&value).expect("the title record is serializable")
    }
}
