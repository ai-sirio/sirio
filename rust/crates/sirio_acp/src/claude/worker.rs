//! The thread that owns the `claude` process.
//!
//! One thread, three jobs: write command lines to its stdin, fold its
//! stdout into events, and make sure that when it dies the user is told
//! why. The stderr drain, the exit probe and the tail report are the ACP
//! client's own helpers, shared rather than re-derived — a death report
//! that names one cause here and another there would be worse than none.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

use async_process::{Command as AsyncCommand, Stdio};
use futures::StreamExt as _;
use futures::executor::block_on;
use futures::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use sirio_claude::{
    Catalog, CliMessage, ContextUsageReport, ControlEnvelope, ControlRequest, EFFORT_DEFAULT,
    LaunchLine, RewindOutcome,
};

use super::ClaudeLaunch;
use super::events::{Fold, ResultSummary};
use super::permission;
use crate::{
    AcpError, AcpEvent, AgentMode, ChildHandle, ContextCost, ContextUsage, EffortChoice,
    EffortOption, ExitStatusSlot, ModeCatalog, ModelCatalog, ModelOption, StderrTail,
    TimeoutOperation,
};

/// What the client asks the worker to do.
#[derive(Debug)]
pub(super) enum Command {
    /// A turn, already built as its `user` line, plus the id it is known by.
    Prompt {
        line: serde_json::Value,
        uuid: String,
    },
    /// Switch model.
    SetModel(String),
    /// Switch permission mode.
    SetMode(String),
    /// Set the effort level; `None` resets to the settings' own value.
    SetEffort(Option<String>),
    /// End the turn in flight.
    Cancel,
    /// Stop: close the CLI's stdin, wait for it, acknowledge.
    Shutdown(mpsc::SyncSender<()>),
    /// Answer a permission card the CLI asked about.
    RespondPermission {
        request_id: u64,
        choice: PermissionChoice,
    },
    /// Ask the CLI to restore tracked files to a turn's checkpoint.
    RewindFiles {
        user_message_id: String,
        dry_run: bool,
        reply: mpsc::SyncSender<Result<RewindOutcome, String>>,
    },
}

/// How long a finished turn waits for the meter's answer before the result's
/// own numbers stand in. The request is answered from local state, so this is
/// a liveness bound rather than patience: a CLI that never answers must not
/// hold the turn's end open behind a measurement.
const CONTEXT_USAGE_GRACE: Duration = Duration::from_secs(2);

/// A card the surface is showing, waiting on a human.
struct PendingPermission {
    request: sirio_claude::CanUseTool,
    /// When it was asked, for the expiry sweep. A card is not a turn: it
    /// has its own deadline, and missing it is a refusal rather than a
    /// fault.
    asked_at: std::time::Instant,
}

/// A finished turn waiting on the meter. The turn's own end is held in here:
/// the transcript reads one ordering as "the turn is over", and a measurement
/// that arrives after it is a measurement nobody looks at.
struct PendingContextUsage {
    /// The request id whose answer resolves this.
    request_id: String,
    /// What the turn's `result` reported, the fallback when no answer comes.
    summary: ResultSummary,
    /// The `TurnEnded` event, held back until the meter is emitted.
    held: Option<AcpEvent>,
    /// When the ask went out, for the grace.
    asked_at: std::time::Instant,
}

/// What the surface answered with.
#[derive(Debug)]
pub(super) enum PermissionChoice {
    Selected(String),
    Cancelled,
}

/// The handshake's progress, reported to the launching thread.
pub(super) enum Startup {
    /// The child exists; the launcher can reap it if the rest fails.
    Process(ChildHandle),
    /// The handshake succeeded and the catalogues are readable.
    Ready,
    /// It did not.
    Failed(AcpError),
}

/// State the client reads while the worker writes it.
#[derive(Debug, Default)]
pub(super) struct Shared {
    session_id: Mutex<Option<String>>,
    claude_version: Mutex<Option<String>>,
    model_catalog: Mutex<Option<ModelCatalog>>,
    mode_catalog: Mutex<Option<ModeCatalog>>,
    mcp_warnings: Mutex<Vec<String>>,
    last_user_message_id: Mutex<Option<String>>,
    resumed_session_refused: AtomicBool,
}

impl Shared {
    pub(super) fn session_id(&self) -> Option<String> {
        self.session_id.lock().ok().and_then(|id| id.clone())
    }

    pub(super) fn claude_version(&self) -> Option<String> {
        self.claude_version.lock().ok().and_then(|v| v.clone())
    }

    pub(super) fn model_catalog(&self) -> Option<ModelCatalog> {
        self.model_catalog.lock().ok().and_then(|c| c.clone())
    }

    pub(super) fn mode_catalog(&self) -> Option<ModeCatalog> {
        self.mode_catalog.lock().ok().and_then(|c| c.clone())
    }

    pub(super) fn mcp_warnings(&self) -> Vec<String> {
        self.mcp_warnings
            .lock()
            .map(|w| w.clone())
            .unwrap_or_default()
    }

    pub(super) fn last_user_message_id(&self) -> Option<String> {
        self.last_user_message_id
            .lock()
            .ok()
            .and_then(|id| id.clone())
    }

    pub(super) fn resumed_session_refused(&self) -> bool {
        self.resumed_session_refused.load(Ordering::Acquire)
    }

    fn push_mcp_warnings(&self, warnings: Vec<String>) {
        if warnings.is_empty() {
            return;
        }
        if let Ok(mut held) = self.mcp_warnings.lock() {
            for warning in warnings {
                if held.len() >= crate::MAX_MCP_WARNINGS {
                    held.remove(0);
                }
                held.push(warning);
            }
        }
    }
}

/// Everything `run` needs, in one struct so the signature stays readable.
pub(super) struct Config {
    pub launch: ClaudeLaunch,
    pub cwd: std::path::PathBuf,
    pub command_rx: async_channel::Receiver<Command>,
    pub event_tx: async_channel::Sender<AcpEvent>,
    pub startup_tx: mpsc::SyncSender<Startup>,
    pub shared: Arc<Shared>,
    pub prompt_timeout: Duration,
    pub permission_timeout: Duration,
}

/// Runs one session to completion. Returns when the CLI is gone.
pub(super) fn run(config: Config) {
    let Config {
        launch,
        cwd,
        command_rx,
        event_tx,
        startup_tx,
        shared,
        prompt_timeout,
        permission_timeout,
    } = config;

    // A resume the CLI will not honour is not a dead tab: the visible
    // transcript came from Sirio's own database, so a fresh session under
    // it loses the agent's memory of the conversation and nothing else.
    // Exactly one retry: a second failure is a real failure.
    let mut resume = launch.resume.clone();
    for attempt in 0..2 {
        let retryable = attempt == 0 && launch.resume.is_some();
        match run_attempt(Attempt {
            launch: ClaudeLaunch {
                program: launch.program.clone(),
                resume: resume.clone(),
                prefix_args: launch.prefix_args.clone(),
            },
            cwd: cwd.clone(),
            command_rx: command_rx.clone(),
            event_tx: event_tx.clone(),
            startup_tx: startup_tx.clone(),
            shared: Arc::clone(&shared),
            prompt_timeout,
            permission_timeout,
            retryable,
        }) {
            AttemptOutcome::Finished => break,
            AttemptOutcome::RetryWithoutResume => {
                resume = None;
                shared
                    .resumed_session_refused
                    .store(true, Ordering::Release);
            }
        }
    }
    // A tab can close in the same instant its agent exits. A `Shutdown`
    // queued after the session loop was gone still has to be answered, or
    // the close waits out the whole timeout for a process already reaped —
    // and the queued command would keep its ack channel alive to the end.
    while let Ok(command) = command_rx.try_recv() {
        if let Command::Shutdown(ack) = command {
            let _ = ack.send(());
        }
    }
}

/// One process: spawn, handshake, serve until the CLI is gone.
struct Attempt {
    launch: ClaudeLaunch,
    cwd: std::path::PathBuf,
    command_rx: async_channel::Receiver<Command>,
    event_tx: async_channel::Sender<AcpEvent>,
    startup_tx: mpsc::SyncSender<Startup>,
    shared: Arc<Shared>,
    prompt_timeout: Duration,
    permission_timeout: Duration,
    /// Suppress the handshake-failure report so the caller can retry
    /// without the id. Only true on the first attempt of a resuming launch.
    retryable: bool,
}

/// What one attempt owes its caller.
enum AttemptOutcome {
    /// The launcher was told everything it is owed: the session ran, or
    /// its failure was reported.
    Finished,
    /// The handshake failed in a way a stale `--resume` id explains, and
    /// nothing was reported yet: the caller retries once without the id.
    RetryWithoutResume,
}

fn run_attempt(attempt: Attempt) -> AttemptOutcome {
    let retryable = attempt.retryable;
    let Attempt {
        launch,
        cwd,
        command_rx,
        event_tx,
        startup_tx,
        shared,
        prompt_timeout,
        permission_timeout,
        retryable: _,
    } = attempt;

    let line = LaunchLine::chat(launch.resume.as_deref());
    let mut std_command = std::process::Command::new(&launch.program);
    std_command
        .args(&launch.prefix_args)
        .args(&line.args)
        .current_dir(&cwd);
    for (key, value) in &line.env {
        std_command.env(key, value);
    }
    #[cfg(unix)]
    {
        // Make the child a process-group leader so terminating it takes
        // anything it spawned with it, the way the ACP path does.
        use std::os::unix::process::CommandExt as _;
        std_command.process_group(0);
    }
    let mut command = AsyncCommand::from(std_command);
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            let _ = startup_tx.send(Startup::Failed(AcpError::Transport(format!(
                "could not launch `{}`: {error}",
                launch.program.display()
            ))));
            return AttemptOutcome::Finished;
        }
    };
    let stdin = child.stdin.take().expect("stdin was piped");
    let stdout = child.stdout.take().expect("stdout was piped");
    let stderr = child.stderr.take().expect("stderr was piped");
    let child: ChildHandle = Arc::new(Mutex::new(Some(child)));
    if startup_tx
        .send(Startup::Process(Arc::clone(&child)))
        .is_err()
    {
        crate::terminate_and_reap_blocking(&child);
        return AttemptOutcome::Finished;
    }

    let stderr_tail: StderrTail = Arc::new(Mutex::new(std::collections::VecDeque::new()));
    let stderr_drained = Arc::new(AtomicBool::new(false));
    let exit_status: ExitStatusSlot = Arc::new(Mutex::new(None));
    let activity: crate::ActivityClock = Arc::new(Mutex::new(std::time::Instant::now()));
    let drain_tail = Arc::clone(&stderr_tail);
    let drain_done = Arc::clone(&stderr_drained);
    let drain_shared = Arc::clone(&shared);
    let _stderr_drain = std::thread::Builder::new()
        .name("sirio-claude-stderr".into())
        .spawn(move || {
            block_on(drain_stderr(stderr, drain_tail, drain_shared));
            drain_done.store(true, Ordering::Release);
        });

    let outcome = block_on(session(SessionContext {
        stdin,
        stdout,
        command_rx,
        event_tx: event_tx.clone(),
        startup_tx: startup_tx.clone(),
        shared: Arc::clone(&shared),
        child: Arc::clone(&child),
        activity: Arc::clone(&activity),
        prompt_timeout,
        permission_timeout,
    }));

    crate::record_exit_status(&exit_status, crate::terminate_and_reap_blocking(&child));
    match outcome {
        SessionOutcome::CleanShutdown(ack) => {
            let _ = ack.send(());
            AttemptOutcome::Finished
        }
        SessionOutcome::NeverStarted => AttemptOutcome::Finished,
        SessionOutcome::HandshakeFailed(error) => {
            if retryable {
                return AttemptOutcome::RetryWithoutResume;
            }
            let _ = startup_tx.send(Startup::Failed(AcpError::Transport(error)));
            AttemptOutcome::Finished
        }
        SessionOutcome::Died(detail) => {
            // `pending_permissions` lived inside `session` and died with
            // it: the command channel is gone, so any later answer fails,
            // and this report tells the surface to close the cards — the
            // native equivalent of `cancel_permissions`.
            block_on(crate::wait_for_stderr_drain(&stderr_drained));
            let report = crate::stderr_tail_report(&stderr_tail);
            let exit = block_on(crate::child_exit_report(&child, &exit_status));
            let _ = event_tx.send_blocking(AcpEvent::TransportError(format!(
                "the Claude agent stopped{detail}{exit}{report}"
            )));
            AttemptOutcome::Finished
        }
    }
}

/// How the session ended.
enum SessionOutcome {
    /// `Command::Shutdown` was honoured; acknowledge on this channel.
    CleanShutdown(mpsc::SyncSender<()>),
    /// The handshake never completed; the launcher already has the error.
    NeverStarted,
    /// The handshake failed before any session existed, in a way a stale
    /// `--resume` id explains: the CLI exited, or refused `initialize`.
    /// Carries the sentence the launcher reports when there is no id left
    /// to retry without.
    HandshakeFailed(String),
    /// The CLI went away on its own.
    Died(String),
}

struct SessionContext {
    stdin: async_process::ChildStdin,
    stdout: async_process::ChildStdout,
    command_rx: async_channel::Receiver<Command>,
    event_tx: async_channel::Sender<AcpEvent>,
    startup_tx: mpsc::SyncSender<Startup>,
    shared: Arc<Shared>,
    child: ChildHandle,
    activity: crate::ActivityClock,
    prompt_timeout: Duration,
    permission_timeout: Duration,
}

async fn session(context: SessionContext) -> SessionOutcome {
    let SessionContext {
        mut stdin,
        stdout,
        command_rx,
        event_tx,
        startup_tx,
        shared,
        child,
        activity,
        prompt_timeout,
        permission_timeout,
    } = context;
    let mut lines = BufReader::new(stdout).lines();
    let mut fold = Fold::new();
    let request_ids = AtomicU64::new(1);
    let next_request_id = || format!("sirio-{}", request_ids.fetch_add(1, Ordering::Relaxed));
    let mut turn_in_flight = false;
    let mut pending_model: HashMap<String, String> = HashMap::new();
    let mut pending_rewinds: HashMap<String, mpsc::SyncSender<Result<RewindOutcome, String>>> =
        HashMap::new();
    let timeout_reason: Arc<Mutex<Option<AcpError>>> = Arc::new(Mutex::new(None));
    let permission_counter = AtomicU64::new(1);
    let mut pending_permissions: HashMap<u64, PendingPermission> = HashMap::new();
    let mut open_permissions: usize = 0;
    let mut pending_context_usage: Option<PendingContextUsage> = None;

    // The handshake. Its answer is the whole catalogue, so every picker is
    // populated before the user can type into the composer.
    let handshake_id = next_request_id();
    if write_line(&mut stdin, &ControlRequest::initialize(&handshake_id))
        .await
        .is_err()
    {
        return SessionOutcome::HandshakeFailed("could not write the Claude handshake".into());
    }

    let mut catalog = loop {
        let Some(Ok(line)) = lines.next().await else {
            return SessionOutcome::HandshakeFailed(
                "the Claude agent closed its output before answering the handshake".into(),
            );
        };
        crate::touch_activity(&activity);
        let Some(message) = CliMessage::parse(&line) else {
            // Not JSON. A CLI that prints a banner or a warning before its
            // protocol output is not a broken CLI.
            continue;
        };
        if let CliMessage::ControlResponse(value) = &message
            && let Some(envelope) = ControlEnvelope::parse(value)
            && envelope.request_id == handshake_id
        {
            match (envelope.payload, envelope.error) {
                (Some(payload), _) => break Catalog::from_initialize(&payload),
                (None, Some(error)) => {
                    return SessionOutcome::HandshakeFailed(format!(
                        "the Claude agent refused the handshake: {error}"
                    ));
                }
                (None, None) => break Catalog::default(),
            }
        }
        // Anything else before the handshake answer is ordinary traffic.
        for event in fold.apply(message) {
            let _ = event_tx.send(event).await;
        }
    };

    if catalog.is_logged_out() {
        // The same error the ACP path raises, so the surface's existing
        // auth card and its CLI-login guidance apply unchanged (F-CHAT-02).
        let _ = startup_tx.send(Startup::Failed(AcpError::AuthRequired {
            methods: Vec::new(),
        }));
        return SessionOutcome::NeverStarted;
    }

    publish_catalogues(&shared, &catalog);
    // The CLI prints its init line unprompted, possibly before answering
    // the handshake, so the fold may already hold the version. Publish it
    // before `Ready`: unlike the session id — which stays `None` until the
    // first turn opens one — the version is a property of the binary.
    if let Some(version) = fold.claude_version()
        && let Ok(mut held) = shared.claude_version.lock()
    {
        *held = Some(version.to_string());
    }
    let _ = startup_tx.send(Startup::Ready);
    // The effort levels are per-model, so the selector has to be republished
    // whenever the model moves, and the selection carried across the move.
    // This is the same model `publish_catalogues` marks as selected: the CLI
    // lists the session's own model first.
    let mut selected_effort = EFFORT_DEFAULT.to_string();
    let mut selected_model = catalog
        .models()
        .first()
        .map(|model| model.id.clone())
        .unwrap_or_default();
    for event in catalogue_events(&catalog, &selected_model, &mut selected_effort) {
        let _ = event_tx.send(event).await;
    }

    // The session proper: read lines and serve commands until one side
    // stops.
    loop {
        let next_line = lines.next();
        let next_command = command_rx.recv();
        // The idle window, not a cap on the turn. An agentic turn that
        // runs for an hour is normal as long as it keeps reporting; the
        // clock restarts on every line the CLI writes.
        let idle_check = async_io::Timer::after(Duration::from_millis(250));
        futures::pin_mut!(next_line, next_command, idle_check);
        match futures::future::select(futures::future::select(next_line, next_command), idle_check)
            .await
        {
            futures::future::Either::Left((futures::future::Either::Left((line, _)), _)) => {
                match line {
                    Some(Ok(line)) => {
                        crate::touch_activity(&activity);
                        let Some(message) = CliMessage::parse(&line) else {
                            continue;
                        };
                        if let CliMessage::ControlResponse(value) = &message
                            && let Some(envelope) = ControlEnvelope::parse(value)
                        {
                            if let Some(model) = pending_model.remove(&envelope.request_id) {
                                match &envelope.error {
                                    // The wire confirms by echoing; a refusal
                                    // leaves the catalogue where it was rather
                                    // than showing a model the session is not on.
                                    None => {
                                        if let Some(catalogue) =
                                            apply_selected_model(&shared, &model)
                                        {
                                            let _ = event_tx
                                                .send(AcpEvent::ModelCatalog(catalogue))
                                                .await;
                                        }
                                        // A model carries its own effort
                                        // ladder — five levels, three, or
                                        // none at all — so the selector is
                                        // republished for the model the
                                        // session is now on.
                                        selected_model = model;
                                        let _ = event_tx
                                            .send(effort_event(
                                                &catalog,
                                                &selected_model,
                                                &mut selected_effort,
                                            ))
                                            .await;
                                    }
                                    Some(error) => {
                                        let _ = event_tx
                                            .send(AcpEvent::TransportError(format!(
                                                "model selection failed: {error}"
                                            )))
                                            .await;
                                    }
                                }
                            }
                            // The envelope is consumed by the rewind branch
                            // below, so the meter's answer is read first.
                            let report = envelope
                                .payload
                                .as_ref()
                                .and_then(ContextUsageReport::parse);
                            if let Some(pending) = pending_context_usage
                                .take_if(|pending| pending.request_id == envelope.request_id)
                            {
                                resolve_context_usage(&event_tx, pending, report).await;
                            }
                            if let Some(reply) = pending_rewinds.remove(&envelope.request_id) {
                                // A success carries the outcome; a refusal
                                // carries only the CLI's own sentence, which
                                // is what the surface shows.
                                match (envelope.payload, envelope.error) {
                                    (Some(payload), _) => match RewindOutcome::parse(&payload) {
                                        Some(outcome) => {
                                            let _ = reply.send(Ok(outcome));
                                        }
                                        None => {
                                            let _ = reply
                                                .send(Err("the rewind answer could not be read"
                                                    .to_string()));
                                        }
                                    },
                                    (None, Some(error)) => {
                                        let _ = reply.send(Err(error));
                                    }
                                    (None, None) => {
                                        let _ =
                                            reply
                                                .send(Err("the agent refused the rewind request"
                                                    .to_string()));
                                    }
                                }
                            }
                            continue;
                        }
                        if let CliMessage::ControlRequest(value) = &message
                            && let Some(request) = sirio_claude::CanUseTool::parse(value)
                        {
                            let request_id = permission_counter.fetch_add(1, Ordering::Relaxed);
                            // Draw or pause the row this decision belongs to,
                            // so a card never asks about a call nobody sees.
                            let row_events = match request.tool_use_id.as_deref() {
                                Some(tool_use_id) => {
                                    let mut events = fold.start_from_permission(
                                        &request.tool_name,
                                        tool_use_id,
                                        &request.input,
                                    );
                                    events.extend(fold.mark_pending(tool_use_id));
                                    events
                                }
                                None => Vec::new(),
                            };
                            for event in row_events {
                                let _ = event_tx.send(event).await;
                            }
                            let question = permission::is_question(&request)
                                .then(|| crate::parse_permission_question(&request.input))
                                .flatten();
                            let options = if permission::is_question(&request) {
                                permission::question_options(&request)
                            } else {
                                permission::options_for(&request)
                            };
                            let _ = event_tx
                                .send(AcpEvent::PermissionRequest {
                                    request_id,
                                    session_id: shared.session_id().unwrap_or_default(),
                                    title: sirio_claude::tools::describe(
                                        &sirio_claude::message::ToolUse {
                                            id: request.tool_use_id.clone().unwrap_or_default(),
                                            name: request.tool_name.clone(),
                                            input: request.input.clone(),
                                        },
                                    )
                                    .title,
                                    options,
                                    question,
                                })
                                .await;
                            open_permissions += 1;
                            pending_permissions.insert(
                                request_id,
                                PendingPermission {
                                    request,
                                    asked_at: std::time::Instant::now(),
                                },
                            );
                            continue;
                        }
                        let is_result = matches!(message, CliMessage::Result(_));
                        if is_result {
                            turn_in_flight = false;
                        }
                        if let CliMessage::System(system) = &message
                            && let Some(mode) = system.permission_mode.clone()
                        {
                            apply_current_mode(&shared, &mode);
                        }
                        // A plugin or skill loaded mid-session. The list is
                        // republished even when it came back empty: this is
                        // a replacement, and a picker still offering the
                        // handshake's commands would be stale for the rest
                        // of the session.
                        if let CliMessage::System(system) = &message
                            && system.subtype == "commands_changed"
                            && let Some(commands) = &system.commands
                        {
                            catalog.replace_commands(commands);
                            let _ = event_tx
                                .send(AcpEvent::AvailableCommands(command_list(&catalog)))
                                .await;
                        }
                        let mut events = fold.apply(message);
                        shared.push_mcp_warnings(fold.take_mcp_warnings());
                        if let Some(id) = fold.session_id()
                            && let Ok(mut held) = shared.session_id.lock()
                        {
                            *held = Some(id.to_string());
                        }
                        if let Some(version) = fold.claude_version()
                            && let Ok(mut held) = shared.claude_version.lock()
                        {
                            *held = Some(version.to_string());
                        }
                        if is_result {
                            // Two results in a row cannot both be the end of
                            // a turn; if one is still held, end it first
                            // rather than swallow it.
                            if let Some(pending) = pending_context_usage.take() {
                                resolve_context_usage(&event_tx, pending, None).await;
                            }
                            let held = matches!(events.last(), Some(AcpEvent::TurnEnded { .. }))
                                .then(|| events.pop())
                                .flatten();
                            let request_id = next_request_id();
                            pending_context_usage = Some(PendingContextUsage {
                                request_id: request_id.clone(),
                                summary: fold.last_result().cloned().unwrap_or_default(),
                                held,
                                asked_at: std::time::Instant::now(),
                            });
                            if write_line(
                                &mut stdin,
                                &ControlRequest::get_context_usage(&request_id),
                            )
                            .await
                            .is_err()
                            {
                                end_held_turn(&event_tx, &mut pending_context_usage).await;
                                return SessionOutcome::Died(
                                    ": could not write the context usage request".into(),
                                );
                            }
                        }
                        for event in events {
                            let _ = event_tx.send(event).await;
                        }
                    }
                    Some(Err(error)) => {
                        end_held_turn(&event_tx, &mut pending_context_usage).await;
                        return SessionOutcome::Died(format!(": {error}"));
                    }
                    None => {
                        end_held_turn(&event_tx, &mut pending_context_usage).await;
                        return SessionOutcome::Died(String::new());
                    }
                }
            }
            futures::future::Either::Left((futures::future::Either::Right((command, _)), _)) => {
                match command {
                    Ok(Command::Prompt { line, uuid }) => {
                        if let Ok(mut held) = shared.last_user_message_id.lock() {
                            *held = Some(uuid);
                        }
                        // The watchdog measures silence from here. A turn that
                        // reports is a turn that lives, however long it takes.
                        crate::touch_activity(&activity);
                        turn_in_flight = true;
                        if write_line(&mut stdin, &line).await.is_err() {
                            end_held_turn(&event_tx, &mut pending_context_usage).await;
                            return SessionOutcome::Died(": could not write the turn".into());
                        }
                    }
                    Ok(Command::SetModel(model)) => {
                        let id = next_request_id();
                        pending_model.insert(id.clone(), model.clone());
                        if write_line(&mut stdin, &ControlRequest::set_model(&id, &model))
                            .await
                            .is_err()
                        {
                            end_held_turn(&event_tx, &mut pending_context_usage).await;
                            return SessionOutcome::Died(
                                ": could not write the model change".into(),
                            );
                        }
                    }
                    Ok(Command::SetMode(mode)) => {
                        let id = next_request_id();
                        if write_line(&mut stdin, &ControlRequest::set_permission_mode(&id, &mode))
                            .await
                            .is_err()
                        {
                            end_held_turn(&event_tx, &mut pending_context_usage).await;
                            return SessionOutcome::Died(
                                ": could not write the mode change".into(),
                            );
                        }
                    }
                    Ok(Command::SetEffort(level)) => {
                        selected_effort =
                            level.clone().unwrap_or_else(|| EFFORT_DEFAULT.to_string());
                        let id = next_request_id();
                        if write_line(
                            &mut stdin,
                            &ControlRequest::set_effort(&id, level.as_deref()),
                        )
                        .await
                        .is_err()
                        {
                            end_held_turn(&event_tx, &mut pending_context_usage).await;
                            return SessionOutcome::Died(
                                ": could not write the effort change".into(),
                            );
                        }
                    }
                    Ok(Command::Cancel) => {
                        let id = next_request_id();
                        if write_line(&mut stdin, &ControlRequest::interrupt(&id))
                            .await
                            .is_err()
                        {
                            end_held_turn(&event_tx, &mut pending_context_usage).await;
                            return SessionOutcome::Died(": could not write the interrupt".into());
                        }
                    }
                    Ok(Command::RespondPermission { request_id, choice }) => {
                        let Some(pending) = pending_permissions.remove(&request_id) else {
                            continue;
                        };
                        open_permissions = open_permissions.saturating_sub(1);
                        // The card closing is fresh evidence about the agent:
                        // the idle window starts from the answer, not the
                        // question.
                        crate::touch_activity(&activity);
                        let answer = match choice {
                            PermissionChoice::Selected(option_id)
                                if permission::is_question(&pending.request) =>
                            {
                                permission::answer_for_question(&pending.request, &option_id)
                            }
                            PermissionChoice::Selected(option_id) => {
                                permission::answer_for(&pending.request, &option_id)
                            }
                            PermissionChoice::Cancelled => permission::Answer {
                                result: sirio_claude::PermissionResult::deny(permission::REFUSED),
                                then_mode: None,
                            },
                        };
                        let allowed =
                            matches!(answer.result, sirio_claude::PermissionResult::Allow { .. });
                        let line = answer.result.into_response(&pending.request.request_id);
                        if write_line(&mut stdin, &line).await.is_err() {
                            end_held_turn(&event_tx, &mut pending_context_usage).await;
                            return SessionOutcome::Died(": could not answer a permission".into());
                        }
                        if allowed && let Some(tool_use_id) = pending.request.tool_use_id.as_deref()
                        {
                            for event in fold.mark_running(tool_use_id) {
                                let _ = event_tx.send(event).await;
                            }
                        }
                        if let Some(mode) = answer.then_mode {
                            let id = next_request_id();
                            if write_line(
                                &mut stdin,
                                &ControlRequest::set_permission_mode(&id, &mode),
                            )
                            .await
                            .is_err()
                            {
                                end_held_turn(&event_tx, &mut pending_context_usage).await;
                                return SessionOutcome::Died(
                                    ": could not apply the approved mode".into(),
                                );
                            }
                        }
                    }
                    Ok(Command::RewindFiles {
                        user_message_id,
                        dry_run,
                        reply,
                    }) => {
                        let id = next_request_id();
                        pending_rewinds.insert(id.clone(), reply);
                        if write_line(
                            &mut stdin,
                            &ControlRequest::rewind_files(&id, &user_message_id, dry_run),
                        )
                        .await
                        .is_err()
                        {
                            pending_rewinds.remove(&id);
                            end_held_turn(&event_tx, &mut pending_context_usage).await;
                            return SessionOutcome::Died(
                                ": could not write the rewind request".into(),
                            );
                        }
                    }
                    Ok(Command::Shutdown(ack)) => {
                        // Closing stdin is how a stream-json session ends.
                        drop(stdin);
                        end_held_turn(&event_tx, &mut pending_context_usage).await;
                        return SessionOutcome::CleanShutdown(ack);
                    }
                    Err(_) => {
                        end_held_turn(&event_tx, &mut pending_context_usage).await;
                        return SessionOutcome::Died(String::new());
                    }
                }
            }
            // The idle timer: nothing to serve, fall through to the
            // watchdog check below.
            futures::future::Either::Right((_, _)) => {}
        }
        // A card the user never answered is a refusal, not a protocol
        // fault. Answering `internal_error` would hand the agent a failed
        // request it never got wrong.
        let expired: Vec<u64> = pending_permissions
            .iter()
            .filter(|(_, pending)| pending.asked_at.elapsed() >= permission_timeout)
            .map(|(id, _)| *id)
            .collect();
        for request_id in expired {
            let Some(pending) = pending_permissions.remove(&request_id) else {
                continue;
            };
            open_permissions = open_permissions.saturating_sub(1);
            crate::record_timeout(
                &timeout_reason,
                AcpError::Timeout {
                    operation: TimeoutOperation::Permission,
                    duration: permission_timeout,
                },
            );
            let line = sirio_claude::PermissionResult::deny(permission::REFUSED)
                .into_response(&pending.request.request_id);
            if write_line(&mut stdin, &line).await.is_err() {
                end_held_turn(&event_tx, &mut pending_context_usage).await;
                return SessionOutcome::Died(": could not expire a permission".into());
            }
        }
        // The meter has its own grace: the turn it belongs to has already
        // finished, and a CLI that never answers must not hold its end open.
        if let Some(pending) = pending_context_usage
            .take_if(|pending| pending.asked_at.elapsed() >= CONTEXT_USAGE_GRACE)
        {
            resolve_context_usage(&event_tx, pending, None).await;
        }
        if turn_in_flight && open_permissions == 0 && crate::idle_for(&activity) >= prompt_timeout {
            // An unanswered permission card is the client's silence, not
            // the agent's: the agent asked and is doing exactly what it
            // should, which is nothing.
            crate::record_timeout(
                &timeout_reason,
                AcpError::Timeout {
                    operation: TimeoutOperation::Prompt,
                    duration: prompt_timeout,
                },
            );
            let _ = event_tx
                .send(AcpEvent::Timeout {
                    operation: TimeoutOperation::Prompt,
                    duration: prompt_timeout,
                })
                .await;
            crate::terminate_and_reap_blocking(&child);
            end_held_turn(&event_tx, &mut pending_context_usage).await;
            return SessionOutcome::NeverStarted;
        }
    }
}

/// The meter's event for one turn: the agent's own count when it answered,
/// the result's own numbers when it did not, and nothing when neither exists.
/// A meter with no measurement says unknown rather than zero.
fn context_usage_event(
    report: Option<ContextUsageReport>,
    summary: &ResultSummary,
) -> Option<AcpEvent> {
    let (used, size) = match report {
        Some(report) => (report.total_tokens, report.raw_max_tokens),
        None => {
            // What this turn itself saw: the prompt it read (fresh, cached
            // and newly cached alike) plus what it wrote back.
            let size = summary.context_window?;
            let used = summary.input_tokens
                + summary.cached_read_tokens
                + summary.cache_creation_tokens
                + summary.output_tokens;
            (used, size)
        }
    };
    Some(AcpEvent::ContextUsage(ContextUsage {
        used,
        size,
        cost: summary.total_cost_usd.map(|amount| ContextCost {
            amount,
            currency: "USD".into(),
        }),
        input_tokens: Some(summary.input_tokens),
        output_tokens: Some(summary.output_tokens),
        cached_read_tokens: Some(summary.cached_read_tokens),
    }))
}

/// Ends a held turn: the meter first, then the turn's own end. A refusal or
/// an unreadable answer is not an error — the result's numbers stand in.
async fn resolve_context_usage(
    event_tx: &async_channel::Sender<AcpEvent>,
    pending: PendingContextUsage,
    report: Option<ContextUsageReport>,
) {
    if let Some(event) = context_usage_event(report, &pending.summary) {
        let _ = event_tx.send(event).await;
    }
    if let Some(held) = pending.held {
        let _ = event_tx.send(held).await;
    }
}

/// The same, for a held turn the reader will never get an answer for — the
/// CLI went away with the request still in its stdin.
async fn end_held_turn(
    event_tx: &async_channel::Sender<AcpEvent>,
    pending: &mut Option<PendingContextUsage>,
) {
    if let Some(pending) = pending.take() {
        resolve_context_usage(event_tx, pending, None).await;
    }
}

async fn write_line(
    stdin: &mut async_process::ChildStdin,
    value: &serde_json::Value,
) -> std::io::Result<()> {
    stdin.write_all(value.to_string().as_bytes()).await?;
    stdin.write_all(b"\n").await?;
    stdin.flush().await
}

fn publish_catalogues(shared: &Shared, catalog: &Catalog) {
    if let Ok(mut models) = shared.model_catalog.lock() {
        let options: Vec<ModelOption> = catalog
            .models()
            .into_iter()
            .map(|model| ModelOption {
                id: model.id,
                name: model.name,
                description: model.description,
            })
            .collect();
        *models = (!options.is_empty()).then(|| ModelCatalog {
            config_id: "model".into(),
            selected_id: options
                .first()
                .map(|model| model.id.clone())
                .unwrap_or_default(),
            options,
        });
    }
    if let Ok(mut modes) = shared.mode_catalog.lock() {
        let catalog_modes = catalog.modes();
        *modes = Some(ModeCatalog {
            current_id: catalog_modes.current_id,
            options: catalog_modes
                .options
                .into_iter()
                .map(|mode| AgentMode {
                    id: mode.id,
                    name: mode.name,
                    description: Some(mode.description),
                })
                .collect(),
            // Modes are applied with their own control request, not through
            // ACP's config-option mechanism.
            config_option_id: None,
        });
    }
}

fn catalogue_events(
    catalog: &Catalog,
    model_id: &str,
    selected_effort: &mut String,
) -> Vec<AcpEvent> {
    let mut events = Vec::new();
    let commands = command_list(catalog);
    if !commands.is_empty() {
        events.push(AcpEvent::AvailableCommands(commands));
    }
    events.push(effort_event(catalog, model_id, selected_effort));
    events
}

/// The command list as the surface takes it. Two callers: the handshake,
/// and a `commands_changed` line later in the session.
fn command_list(catalog: &Catalog) -> Vec<crate::AvailableCommandInfo> {
    catalog
        .commands()
        .into_iter()
        .map(|command| crate::AvailableCommandInfo {
            name: command.name,
            description: command.description,
            argument_hint: command.argument_hint,
        })
        .collect()
}

/// The effort selector for `model_id`, with `selected` moved to whatever
/// survives: the levels are the model's own, so a level the session was on
/// may simply not exist on the model it just moved to, and a selector
/// showing a level its own list does not carry would be a worse answer than
/// the model's default.
///
/// Empty choices are how a model with no effort at all says so — Haiku sends
/// neither effort field — and the surface reads that as "no selector" rather
/// than as an empty one.
fn effort_event(catalog: &Catalog, model_id: &str, selected: &mut String) -> AcpEvent {
    let effort = catalog.effort_for(model_id);
    let choices: Vec<EffortChoice> = effort
        .map(|effort| {
            effort
                .choices
                .into_iter()
                .map(|choice| EffortChoice {
                    value: choice.value,
                    name: choice.name,
                })
                .collect()
        })
        .unwrap_or_default();
    if !choices.iter().any(|choice| choice.value == *selected) {
        *selected = EFFORT_DEFAULT.to_string();
    }
    AcpEvent::Effort(EffortOption {
        option_id: "effort".into(),
        name: Some("Effort".into()),
        current_value: Some(selected.clone()),
        choices,
    })
}

/// Moves the catalogue's selection, returning the new catalogue to publish.
fn apply_selected_model(shared: &Shared, model_id: &str) -> Option<ModelCatalog> {
    let mut catalog = shared.model_catalog.lock().ok()?;
    let catalog = catalog.as_mut()?;
    catalog.selected_id = model_id.to_string();
    Some(catalog.clone())
}

fn apply_current_mode(shared: &Shared, mode_id: &str) {
    if let Ok(mut catalog) = shared.mode_catalog.lock()
        && let Some(catalog) = catalog.as_mut()
    {
        catalog.current_id = mode_id.to_string();
    }
}

async fn drain_stderr(stderr: async_process::ChildStderr, tail: StderrTail, _shared: Arc<Shared>) {
    let mut reader = BufReader::new(stderr);
    let mut line = String::new();
    loop {
        line.clear();
        match reader.read_line(&mut line).await {
            Ok(0) | Err(_) => break,
            Ok(_) => {
                let trimmed = line.trim_end();
                if trimmed.is_empty() {
                    continue;
                }
                // MCP stderr lines are intentionally not surfaced as warnings
                // on the native path (see `Fold::apply_system`): a broken
                // user server must not banner the chat.
                crate::push_stderr_tail(&tail, trimmed.to_string());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn summary() -> ResultSummary {
        ResultSummary {
            stop_reason: "EndTurn".into(),
            is_error: false,
            error_text: None,
            total_cost_usd: Some(0.5),
            input_tokens: 1_000,
            output_tokens: 200,
            cached_read_tokens: 4_000,
            cache_creation_tokens: 3_000,
            context_window: Some(200_000),
        }
    }

    fn context_usage(event: Option<AcpEvent>) -> ContextUsage {
        let Some(AcpEvent::ContextUsage(usage)) = event else {
            panic!("expected a context usage event, got {event:?}");
        };
        usage
    }

    /// A handshake payload carrying only what the effort selector reads.
    fn handshake(models: serde_json::Value) -> Catalog {
        Catalog::from_initialize(&serde_json::json!({ "models": models }))
    }

    fn selector(event: AcpEvent) -> EffortOption {
        let AcpEvent::Effort(option) = event else {
            panic!("expected an effort event, got {event:?}");
        };
        option
    }

    fn levels(option: &EffortOption) -> Vec<&str> {
        option
            .choices
            .iter()
            .map(|choice| choice.value.as_str())
            .collect()
    }

    #[test]
    fn the_command_list_carries_the_hint_and_drops_the_empty_one() {
        let catalog = Catalog::from_initialize(&serde_json::json!({
            "commands": [
                {"name": "compact", "description": "Compact",
                 "argumentHint": "<optional custom summarization instructions>"},
                {"name": "usage", "description": "Show plan usage", "argumentHint": ""}
            ]
        }));
        let mut selected = EFFORT_DEFAULT.to_string();
        let events = catalogue_events(&catalog, "opus", &mut selected);
        let Some(AcpEvent::AvailableCommands(commands)) = events.into_iter().next() else {
            panic!("the handshake publishes the command list first");
        };
        assert_eq!(
            commands[0].argument_hint.as_deref(),
            Some("<optional custom summarization instructions>")
        );
        assert_eq!(commands[1].argument_hint, None);
    }

    #[test]
    fn the_effort_selector_is_the_selected_models_own_ladder() {
        let catalog = handshake(serde_json::json!([{
            "value": "opus", "displayName": "Opus", "supportsEffort": true,
            "supportedEffortLevels": ["low", "medium", "high", "xhigh", "max"],
        }]));
        let mut selected = EFFORT_DEFAULT.to_string();
        let option = selector(effort_event(&catalog, "opus", &mut selected));
        assert_eq!(
            levels(&option),
            [
                "default",
                "low",
                "medium",
                "high",
                "xhigh",
                "max",
                "ultracode"
            ]
        );
        assert_eq!(option.current_value.as_deref(), Some("default"));
    }

    #[test]
    fn a_model_with_no_effort_publishes_an_empty_selector() {
        // Haiku's real shape: neither effort field. An empty list is how the
        // surface is told there is nothing to pick, as opposed to a picker
        // with nothing in it.
        let catalog = handshake(serde_json::json!([
            {"value": "haiku", "displayName": "Haiku"}
        ]));
        let mut selected = EFFORT_DEFAULT.to_string();
        let option = selector(effort_event(&catalog, "haiku", &mut selected));
        assert!(option.choices.is_empty());
    }

    #[test]
    fn a_level_the_new_model_does_not_offer_gives_way_to_its_default() {
        let catalog = handshake(serde_json::json!([
            {"value": "opus", "displayName": "Opus", "supportsEffort": true,
             "supportedEffortLevels": ["low", "medium", "high", "xhigh", "max"]},
            {"value": "thrifty", "displayName": "Thrifty", "supportsEffort": true,
             "supportedEffortLevels": ["low", "medium", "high"]},
        ]));
        let mut selected = "max".to_string();
        // Staying on a model that offers it keeps the selection.
        let option = selector(effort_event(&catalog, "opus", &mut selected));
        assert_eq!(option.current_value.as_deref(), Some("max"));
        assert_eq!(selected, "max");
        // Moving to one that does not cannot leave the chip reading "Max"
        // for a session that is no longer anywhere near it.
        let option = selector(effort_event(&catalog, "thrifty", &mut selected));
        assert_eq!(option.current_value.as_deref(), Some("default"));
        assert_eq!(selected, "default");
        assert!(!levels(&option).contains(&"ultracode"));
    }

    #[test]
    fn the_agents_own_count_wins_over_the_turns_numbers() {
        let report = ContextUsageReport {
            total_tokens: 48_000,
            raw_max_tokens: 200_000,
            model: Some("claude-fable-5-1".into()),
        };
        let usage = context_usage(context_usage_event(Some(report), &summary()));
        assert_eq!(usage.used, 48_000);
        assert_eq!(usage.size, 200_000);
        assert_eq!(usage.cost.expect("the turn's cost").currency, "USD");
    }

    #[test]
    fn the_fallback_counts_every_token_the_turn_put_in_the_window() {
        // Tokens written to the cache are context too: leaving them out
        // under-reports by exactly the part of the window the prompt cache
        // just took.
        let usage = context_usage(context_usage_event(None, &summary()));
        assert_eq!(usage.used, 1_000 + 4_000 + 3_000 + 200);
        assert_eq!(usage.size, 200_000);
    }

    #[test]
    fn no_answer_and_no_window_is_unknown_rather_than_zero() {
        // A meter that cannot be computed must not render as 0%.
        let mut summary = summary();
        summary.context_window = None;
        assert_eq!(context_usage_event(None, &summary), None);
    }
}
