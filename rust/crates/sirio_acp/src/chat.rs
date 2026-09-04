//! Non-drawing chat session state over one live ACP connection.
//!
//! [`AcpClient`] deliberately exposes protocol-shaped events. This module is
//! the seam a surface or a control socket uses instead: it owns one chat's
//! lifecycle, folds streamed events into rendered transcript entries, and
//! persists settled turns without involving GPUI or a window.

use anyhow::{Result, anyhow};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

use sirio_persistence::{
    AppDatabase, ChatEntry, ChatPermissionOption, ChatPermissionOutcome, ChatPlanEntry,
    ChatToolLocation, ChatTranscript, ChatTurn,
};

use crate::{AcpClient, AcpEvent, AgentCommand, EventStream, ToolCallLocationInfo};

/// Observable state of a non-drawing chat session.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ChatStatus {
    /// No turn has been submitted yet, or the composer is ready again.
    Idle,
    /// A prompt is in flight and streamed events may still arrive.
    Streaming,
    /// The last turn completed normally.
    Completed,
    /// The last in-flight turn was cancelled by the caller.
    Stopped,
    /// The session or its agent reported a failure.
    Error,
}

impl ChatStatus {
    /// Stable wire spelling used by the chat control response.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Streaming => "streaming",
            Self::Completed => "completed",
            Self::Stopped => "stopped",
            Self::Error => "error",
        }
    }
}

/// Inputs needed to create a live, non-drawing chat session.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChatSessionConfig {
    /// Stable chat-tab identity. Transcript rows are owned by this id.
    pub tab_id: String,
    /// Worktree containing the chat tab and agent process.
    pub worktree_id: String,
    /// ACP agent executable and arguments.
    pub command: AgentCommand,
    /// Working directory passed to ACP session creation.
    pub cwd: PathBuf,
    /// SQLite database shared with the workspace persistence layer.
    pub database_path: PathBuf,
}

impl ChatSessionConfig {
    /// Builds a live-session configuration without shell parsing.
    #[must_use]
    pub fn new(
        tab_id: impl Into<String>,
        worktree_id: impl Into<String>,
        command: AgentCommand,
        cwd: impl AsRef<Path>,
        database_path: impl AsRef<Path>,
    ) -> Self {
        Self {
            tab_id: tab_id.into(),
            worktree_id: worktree_id.into(),
            command,
            cwd: cwd.as_ref().to_path_buf(),
            database_path: database_path.as_ref().to_path_buf(),
        }
    }
}

/// The complete readback surface for one chat session.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChatSnapshot {
    /// Stable chat-tab identity.
    pub tab_id: String,
    /// Worktree owning the chat tab.
    pub worktree_id: String,
    /// ACP session identity, when a live agent was launched.
    pub agent_session_id: Option<String>,
    /// Current lifecycle status.
    pub status: ChatStatus,
    /// Draft text that has not been submitted.
    pub composer_text: String,
    /// Text typed while a turn was streaming.
    pub queued_text: String,
    /// Settled transcript plus the currently streaming turn, if any.
    pub transcript: ChatTranscript,
    /// A transport/persistence failure suitable for a readback surface.
    pub error: Option<String>,
}

#[derive(Debug)]
struct ChatState {
    tab_id: String,
    worktree_id: String,
    agent_session_id: Option<String>,
    status: ChatStatus,
    composer_text: String,
    queued_text: String,
    transcript: ChatTranscript,
    current_turn: Vec<ChatEntry>,
    error: Option<String>,
}

impl ChatState {
    fn new(
        tab_id: String,
        worktree_id: String,
        agent_session_id: Option<String>,
        transcript: ChatTranscript,
    ) -> Self {
        let status = match transcript.turns.last() {
            None => ChatStatus::Idle,
            Some(turn) if turn_ended_stopped(turn) => ChatStatus::Stopped,
            Some(_) => ChatStatus::Completed,
        };
        Self {
            tab_id,
            worktree_id,
            agent_session_id,
            status,
            composer_text: String::new(),
            queued_text: String::new(),
            transcript,
            current_turn: Vec::new(),
            error: None,
        }
    }

    fn snapshot(&self) -> ChatSnapshot {
        let mut transcript = self.transcript.clone();
        if !self.current_turn.is_empty() {
            transcript.turns.push(ChatTurn {
                entries: self.current_turn.clone(),
            });
        }
        ChatSnapshot {
            tab_id: self.tab_id.clone(),
            worktree_id: self.worktree_id.clone(),
            agent_session_id: self.agent_session_id.clone(),
            status: self.status,
            composer_text: self.composer_text.clone(),
            queued_text: self.queued_text.clone(),
            transcript,
            error: self.error.clone(),
        }
    }

    fn commit_current_turn(&mut self) -> Option<ChatTranscript> {
        if self.current_turn.is_empty() {
            return None;
        }
        self.transcript.turns.push(ChatTurn {
            entries: std::mem::take(&mut self.current_turn),
        });
        Some(self.transcript.clone())
    }

    fn mark_error(&mut self, message: String) -> Option<ChatTranscript> {
        self.error = Some(message.clone());
        self.status = ChatStatus::Error;
        expire_pending_permissions(&mut self.current_turn);
        self.current_turn.push(ChatEntry::Error {
            message,
            retryable: true,
        });
        self.commit_current_turn()
    }
}

/// A live ACP-backed chat or a restored transcript with no live agent.
#[derive(Debug)]
pub struct ChatSession {
    state: Arc<Mutex<ChatState>>,
    client: Arc<Mutex<Option<AcpClient>>>,
    worker: Option<JoinHandle<()>>,
}

impl ChatSession {
    /// Launches an ACP agent and starts folding its events into readback state.
    pub fn launch(config: ChatSessionConfig) -> Result<Self> {
        let database = AppDatabase::open(&config.database_path)?;
        let transcript = database
            .load_chat_transcript(&config.tab_id)?
            .unwrap_or_else(|| ChatTranscript {
                tab_id: config.tab_id.clone(),
                turns: Vec::new(),
            });
        drop(database);

        let (client, events) = AcpClient::launch(config.command, &config.cwd)?;
        let agent_session_id = Some(client.session_id().to_string());
        let state = Arc::new(Mutex::new(ChatState::new(
            config.tab_id,
            config.worktree_id,
            agent_session_id,
            transcript,
        )));
        let client = Arc::new(Mutex::new(Some(client)));
        let worker = spawn_event_worker(
            events,
            Arc::clone(&state),
            Arc::clone(&client),
            config.database_path,
        )?;
        Ok(Self {
            state,
            client,
            worker: Some(worker),
        })
    }

    /// Restores a persisted transcript without starting a window or agent.
    pub fn restore(
        database_path: impl AsRef<Path>,
        tab_id: impl Into<String>,
        worktree_id: impl Into<String>,
    ) -> Result<Self> {
        let tab_id = tab_id.into();
        let database = AppDatabase::open(database_path.as_ref())?;
        let transcript =
            database
                .load_chat_transcript(&tab_id)?
                .unwrap_or_else(|| ChatTranscript {
                    tab_id: tab_id.clone(),
                    turns: Vec::new(),
                });
        Ok(Self {
            state: Arc::new(Mutex::new(ChatState::new(
                tab_id,
                worktree_id.into(),
                None,
                transcript,
            ))),
            client: Arc::new(Mutex::new(None)),
            worker: None,
        })
    }

    /// Returns a point-in-time readback, including streamed text already seen.
    pub fn read(&self) -> Result<ChatSnapshot> {
        Ok(self.state.lock().map_err(lock_error)?.snapshot())
    }

    /// Sends a turn without waiting for the final response blob.
    pub fn send(&self, text: impl Into<String>) -> Result<()> {
        let text = text.into();
        if text.trim().is_empty() {
            return Err(anyhow!("chat prompt cannot be empty"));
        }

        {
            let mut state = self.state.lock().map_err(lock_error)?;
            if state.status == ChatStatus::Streaming {
                state.queued_text = text;
                return Ok(());
            }
            state.error = None;
            state.composer_text.clear();
            state.current_turn = vec![ChatEntry::UserMessage { text: text.clone(), at: None }];
            state.status = ChatStatus::Streaming;
        }

        let prompt_result = {
            let client = self.client.lock().map_err(lock_error)?;
            match client.as_ref() {
                Some(client) => client.prompt(text),
                None => Err(anyhow!("chat session has no live agent")),
            }
        };
        if let Err(error) = prompt_result {
            let mut state = self.state.lock().map_err(lock_error)?;
            state.mark_error(error.to_string());
            return Err(error);
        }
        Ok(())
    }

    /// Stores draft text, or queues it while a turn is streaming.
    pub fn compose(&self, text: impl Into<String>) -> Result<()> {
        let mut state = self.state.lock().map_err(lock_error)?;
        if state.status == ChatStatus::Streaming {
            state.queued_text = text.into();
        } else {
            state.composer_text = text.into();
        }
        Ok(())
    }

    /// Cancels the current turn; readback changes to `stopped` when ACP
    /// delivers its terminal cancellation outcome.
    pub fn stop(&self) -> Result<()> {
        {
            let state = self.state.lock().map_err(lock_error)?;
            if state.status != ChatStatus::Streaming {
                return Err(anyhow!("chat session has no streaming turn"));
            }
        }
        let client = self.client.lock().map_err(lock_error)?;
        let Some(client) = client.as_ref() else {
            return Err(anyhow!("chat session has no live agent"));
        };
        client.cancel()
    }

    /// Answers one pending permission card and records the selected outcome.
    pub fn respond_permission(&self, request_id: u64, option_id: &str) -> Result<()> {
        let selected = {
            let mut state = self.state.lock().map_err(lock_error)?;
            let entry = state
                .current_turn
                .iter_mut()
                .rev()
                .find_map(|entry| match entry {
                    ChatEntry::Permission {
                        request_id: id,
                        options,
                        outcome,
                        ..
                    } if *id == request_id => Some((options, outcome)),
                    _ => None,
                })
                .ok_or_else(|| anyhow!("unknown chat permission request {request_id}"))?;
            let option = entry
                .0
                .iter()
                .find(|option| option.id == option_id)
                .ok_or_else(|| anyhow!("unknown chat permission option {option_id}"))?;
            let selected = (option.id.clone(), option.name.clone());
            *entry.1 = ChatPermissionOutcome::Selected {
                option_id: selected.0.clone(),
                label: selected.1.clone(),
            };
            selected
        };

        let client = self.client.lock().map_err(lock_error)?;
        let Some(client) = client.as_ref() else {
            return Err(anyhow!("chat session has no live agent"));
        };
        client.respond_permission(request_id, selected.0)
    }

    /// Answers one pending permission card with free text (F-CHAT-25). The
    /// protocol has no free-text channel, so the text rides the selected
    /// option id; the recorded label keeps the transcript readable.
    pub fn respond_permission_text(&self, request_id: u64, text: &str) -> Result<()> {
        let answer = text.trim();
        if answer.is_empty() {
            return Err(anyhow!("permission answer cannot be empty"));
        }
        {
            let mut state = self.state.lock().map_err(lock_error)?;
            let entry = state
                .current_turn
                .iter_mut()
                .rev()
                .find_map(|entry| match entry {
                    ChatEntry::Permission {
                        request_id: id,
                        outcome,
                        ..
                    } if *id == request_id => Some(outcome),
                    _ => None,
                })
                .ok_or_else(|| anyhow!("unknown chat permission request {request_id}"))?;
            *entry = ChatPermissionOutcome::Selected {
                option_id: answer.to_string(),
                label: answer.to_string(),
            };
        }

        let client = self.client.lock().map_err(lock_error)?;
        let Some(client) = client.as_ref() else {
            return Err(anyhow!("chat session has no live agent"));
        };
        client.respond_permission(request_id, answer)
    }

    /// Withdraws one pending permission card without selecting an option
    /// (F-CHAT-25); the card is no longer answerable.
    pub fn cancel_permission(&self, request_id: u64) -> Result<()> {
        {
            let mut state = self.state.lock().map_err(lock_error)?;
            let entry = state
                .current_turn
                .iter_mut()
                .rev()
                .find_map(|entry| match entry {
                    ChatEntry::Permission {
                        request_id: id,
                        outcome,
                        ..
                    } if *id == request_id => Some(outcome),
                    _ => None,
                })
                .ok_or_else(|| anyhow!("unknown chat permission request {request_id}"))?;
            *entry = ChatPermissionOutcome::Cancelled;
        }

        let client = self.client.lock().map_err(lock_error)?;
        let Some(client) = client.as_ref() else {
            return Err(anyhow!("chat session has no live agent"));
        };
        client.cancel_permission(request_id)
    }

    /// Stops a live agent and joins its event-folding worker.
    pub fn shutdown(&mut self) -> Result<()> {
        let client_result = {
            let mut client = self.client.lock().map_err(lock_error)?;
            client
                .take()
                .map(|mut client| client.shutdown())
                .transpose()
        };
        if let Some(worker) = self.worker.take() {
            worker
                .join()
                .map_err(|_| anyhow!("chat event worker panicked"))?;
        }
        // A transport that died before shutdown makes the client's own
        // shutdown fail — the agent is already gone and the worker is
        // joined above either way, so the failure is not actionable.
        let _ = client_result;
        Ok(())
    }
}

impl Drop for ChatSession {
    fn drop(&mut self) {
        let _ = self.shutdown();
    }
}

fn spawn_event_worker(
    events: EventStream,
    state: Arc<Mutex<ChatState>>,
    client: Arc<Mutex<Option<AcpClient>>>,
    database_path: PathBuf,
) -> Result<JoinHandle<()>> {
    thread::Builder::new()
        .name("sirio-chat-session".into())
        .spawn(move || run_event_worker(events, state, client, database_path))
        .map_err(|error| anyhow!("spawn chat event worker: {error}"))
}

fn run_event_worker(
    events: EventStream,
    state: Arc<Mutex<ChatState>>,
    client: Arc<Mutex<Option<AcpClient>>>,
    database_path: PathBuf,
) {
    let database = match AppDatabase::open(&database_path) {
        Ok(database) => database,
        Err(error) => {
            if let Ok(mut state) = state.lock() {
                state.status = ChatStatus::Error;
                state.error = Some(error.to_string());
            }
            return;
        }
    };

    while let Ok(event) = events.recv_blocking() {
        let fold = {
            let Ok(mut state) = state.lock() else {
                return;
            };
            apply_event(&mut state, event)
        };
        let persisted = fold.persisted;
        let persistence_succeeded = match persisted {
            Some(transcript) => match database.save_chat_transcript(&transcript) {
                Ok(()) => true,
                Err(error) => {
                    if let Ok(mut state) = state.lock() {
                        state.status = ChatStatus::Error;
                        state.error = Some(error.to_string());
                    }
                    false
                }
            },
            None => true,
        };

        if persistence_succeeded && let Some(text) = fold.next_prompt {
            let prompt_result = match client.lock() {
                Ok(client) => match client.as_ref() {
                    Some(client) => client.prompt(text),
                    None => Err(anyhow!("chat session has no live agent")),
                },
                Err(error) => Err(lock_error(error)),
            };
            if let Err(error) = prompt_result {
                let failed_transcript = state
                    .lock()
                    .ok()
                    .and_then(|mut state| state.mark_error(error.to_string()));
                if let Some(transcript) = failed_transcript {
                    let _ = database.save_chat_transcript(&transcript);
                }
            }
        }
    }
}

#[derive(Default)]
struct EventFold {
    persisted: Option<ChatTranscript>,
    next_prompt: Option<String>,
}

fn apply_event(state: &mut ChatState, event: AcpEvent) -> EventFold {
    match event {
        AcpEvent::AgentMessageChunk(text) => {
            append_assistant(&mut state.current_turn, text);
            state.status = ChatStatus::Streaming;
            EventFold::default()
        }
        AcpEvent::ThoughtChunk(text) => {
            state.current_turn.push(ChatEntry::Thought { text, duration_ms: None });
            state.status = ChatStatus::Streaming;
            EventFold::default()
        }
        AcpEvent::ToolCallStarted {
            id,
            title,
            status,
            kind,
            locations,
            ..
        } => {
            // #168: `kind` and `locations` used to be dropped here by the
            // `..`, so a restored transcript could name neither the tool nor
            // the file it touched — the evidence vanished at the restart.
            state.current_turn.push(ChatEntry::ToolCall {
                id,
                title,
                status,
                kind: Some(kind),
                locations: persisted_locations(&locations),
                duration_ms: None,
            });
            state.status = ChatStatus::Streaming;
            EventFold::default()
        }
        AcpEvent::ToolCallUpdated {
            id, title, status, ..
        } => {
            update_tool(&mut state.current_turn, &id, title, status);
            state.status = ChatStatus::Streaming;
            EventFold::default()
        }
        AcpEvent::ToolCallCompleted { id, status, .. } => {
            update_tool(&mut state.current_turn, &id, None, Some(status));
            state.status = ChatStatus::Streaming;
            EventFold::default()
        }
        AcpEvent::PermissionRequest {
            request_id,
            title,
            options,
            ..
        } => {
            state.current_turn.push(ChatEntry::Permission {
                request_id,
                title,
                options: options
                    .into_iter()
                    .map(|option| ChatPermissionOption {
                        id: option.id,
                        name: option.name,
                        kind: option.kind,
                    })
                    .collect(),
                outcome: ChatPermissionOutcome::Pending,
            });
            state.status = ChatStatus::Streaming;
            EventFold::default()
        }
        AcpEvent::PlanUpdate { entries } => {
            let entries = entries
                .into_iter()
                .map(|entry| ChatPlanEntry {
                    content: entry.content,
                    status: entry.status,
                })
                .collect();
            // A plan is a running state: each update replaces the previous
            // plan card in this turn, so entries can advance in place.
            if let Some(ChatEntry::Plan { entries: existing }) = state
                .current_turn
                .iter_mut()
                .rev()
                .find(|entry| matches!(entry, ChatEntry::Plan { .. }))
            {
                *existing = entries;
            } else {
                state.current_turn.push(ChatEntry::Plan { entries });
            }
            state.status = ChatStatus::Streaming;
            EventFold::default()
        }
        AcpEvent::TurnEnded { stop_reason } => {
            let stopped = is_stopped_reason(&stop_reason);
            // F-CHAT-27: a turn that ends while a permission card is still
            // unanswered leaves that card permanently unanswerable — the
            // surface must not sit in a wait nothing can resolve.
            expire_pending_permissions(&mut state.current_turn);
            state
                .current_turn
                .push(ChatEntry::TurnFooter { text: stop_reason });
            state.status = if stopped {
                ChatStatus::Stopped
            } else {
                ChatStatus::Completed
            };
            state.error = None;
            let persisted = state.commit_current_turn();
            let next_prompt = if stopped {
                None
            } else {
                let text = std::mem::take(&mut state.queued_text);
                if text.is_empty() {
                    None
                } else {
                    state.current_turn = vec![ChatEntry::UserMessage { text: text.clone(), at: None }];
                    state.status = ChatStatus::Streaming;
                    Some(text)
                }
            };
            EventFold {
                persisted,
                next_prompt,
            }
        }
        AcpEvent::TransportError(message) => EventFold {
            persisted: state.mark_error(message),
            next_prompt: None,
        },
        AcpEvent::Timeout {
            operation,
            duration,
        } => EventFold {
            persisted: state.mark_error(format!("ACP {operation:?} timed out after {duration:?}")),
            next_prompt: None,
        },
        AcpEvent::ModelCatalog(_)
        | AcpEvent::AvailableCommands(_)
        | AcpEvent::Effort(_)
        | AcpEvent::ContextUsage(_)
        | AcpEvent::TokenUsageBreakdown { .. }
        | AcpEvent::OtherSessionUpdate { .. } => EventFold::default(),
    }
}

/// Marks every unanswered permission card in the turn as expired, so a
/// finished turn never leaves the surface waiting on a decision that can no
/// longer be delivered (F-CHAT-27).
fn expire_pending_permissions(entries: &mut [ChatEntry]) {
    for entry in entries {
        if let ChatEntry::Permission { outcome, .. } = entry
            && *outcome == ChatPermissionOutcome::Pending
        {
            *outcome = ChatPermissionOutcome::Expired;
        }
    }
}

fn is_stopped_reason(reason: &str) -> bool {
    reason.eq_ignore_ascii_case("cancelled")
        || reason.eq_ignore_ascii_case("stopped")
        || reason.eq_ignore_ascii_case("canceled")
}

fn turn_ended_stopped(turn: &ChatTurn) -> bool {
    matches!(
        turn.entries.last(),
        Some(ChatEntry::TurnFooter { text }) if is_stopped_reason(text)
    )
}

fn append_assistant(entries: &mut Vec<ChatEntry>, text: String) {
    if let Some(ChatEntry::AssistantMessage { text: existing }) = entries.last_mut() {
        existing.push_str(&text);
    } else {
        entries.push(ChatEntry::AssistantMessage { text });
    }
}

/// Converts the ACP locations of a tool call into their stored form (#168).
/// The path is kept as the agent wrote it; it is de-verbatimised and
/// shortened only when rendered, which is where that decision belongs.
fn persisted_locations(locations: &[ToolCallLocationInfo]) -> Vec<ChatToolLocation> {
    locations
        .iter()
        .map(|location| ChatToolLocation {
            path: location.path.to_string_lossy().into_owned(),
            line: location.line,
        })
        .collect()
}

fn update_tool(entries: &mut [ChatEntry], id: &str, title: Option<String>, status: Option<String>) {
    if let Some(ChatEntry::ToolCall {
        title: entry_title,
        status: entry_status,
        ..
    }) = entries
        .iter_mut()
        .find(|entry| matches!(entry, ChatEntry::ToolCall { id: entry_id, .. } if entry_id == id))
    {
        if let Some(title) = title {
            *entry_title = title;
        }
        if let Some(status) = status {
            *entry_status = status;
        }
    }
}

fn lock_error<T>(_: std::sync::PoisonError<T>) -> anyhow::Error {
    anyhow!("chat session state is poisoned")
}
