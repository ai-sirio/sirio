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
use sirio_claude::{Catalog, CliMessage, ControlEnvelope, ControlRequest, LaunchLine};

use super::ClaudeLaunch;
use super::events::Fold;
use super::permission;
use crate::{
    AcpError, AcpEvent, AgentMode, ChildHandle, EffortChoice, EffortOption, ExitStatusSlot,
    ModeCatalog, ModelCatalog, ModelOption, StderrTail, TimeoutOperation,
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
}

/// A card the surface is showing, waiting on a human.
struct PendingPermission {
    request: sirio_claude::CanUseTool,
    /// When it was asked, for the expiry sweep. A card is not a turn: it
    /// has its own deadline, and missing it is a refusal rather than a
    /// fault.
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
            return;
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
        return;
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
        startup_tx,
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
        }
        SessionOutcome::NeverStarted => {}
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
        }
    }
}

/// How the session ended.
enum SessionOutcome {
    /// `Command::Shutdown` was honoured; acknowledge on this channel.
    CleanShutdown(mpsc::SyncSender<()>),
    /// The handshake never completed; the launcher already has the error.
    NeverStarted,
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
    let timeout_reason: Arc<Mutex<Option<AcpError>>> = Arc::new(Mutex::new(None));
    let permission_counter = AtomicU64::new(1);
    let mut pending_permissions: HashMap<u64, PendingPermission> = HashMap::new();
    let mut open_permissions: usize = 0;

    // The handshake. Its answer is the whole catalogue, so every picker is
    // populated before the user can type into the composer.
    let handshake_id = next_request_id();
    if write_line(&mut stdin, &ControlRequest::initialize(&handshake_id))
        .await
        .is_err()
    {
        let _ = startup_tx.send(Startup::Failed(AcpError::Transport(
            "could not write the Claude handshake".into(),
        )));
        return SessionOutcome::NeverStarted;
    }

    let catalog = loop {
        let Some(Ok(line)) = lines.next().await else {
            let _ = startup_tx.send(Startup::Failed(AcpError::Transport(
                "the Claude agent closed its output before answering the handshake".into(),
            )));
            return SessionOutcome::NeverStarted;
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
                    let _ = startup_tx.send(Startup::Failed(AcpError::Transport(format!(
                        "the Claude agent refused the handshake: {error}"
                    ))));
                    return SessionOutcome::NeverStarted;
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
    let _ = startup_tx.send(Startup::Ready);
    for event in catalogue_events(&catalog) {
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
                                match envelope.error {
                                    // The wire confirms by echoing; a refusal
                                    // leaves the catalogue where it was rather
                                    // than showing a model the session is not on.
                                    None => {
                                        if let Some(catalog) = apply_selected_model(&shared, &model)
                                        {
                                            let _ = event_tx
                                                .send(AcpEvent::ModelCatalog(catalog))
                                                .await;
                                        }
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
                        if matches!(message, CliMessage::Result(_)) {
                            turn_in_flight = false;
                        }
                        if let CliMessage::System(system) = &message
                            && let Some(mode) = system.permission_mode.clone()
                        {
                            apply_current_mode(&shared, &mode);
                        }
                        let events = fold.apply(message);
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
                        for event in events {
                            let _ = event_tx.send(event).await;
                        }
                    }
                    Some(Err(error)) => {
                        return SessionOutcome::Died(format!(": {error}"));
                    }
                    None => return SessionOutcome::Died(String::new()),
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
                            return SessionOutcome::Died(
                                ": could not write the mode change".into(),
                            );
                        }
                    }
                    Ok(Command::SetEffort(level)) => {
                        let id = next_request_id();
                        if write_line(
                            &mut stdin,
                            &ControlRequest::set_effort(&id, level.as_deref()),
                        )
                        .await
                        .is_err()
                        {
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
                                return SessionOutcome::Died(
                                    ": could not apply the approved mode".into(),
                                );
                            }
                        }
                    }
                    Ok(Command::Shutdown(ack)) => {
                        // Closing stdin is how a stream-json session ends.
                        drop(stdin);
                        return SessionOutcome::CleanShutdown(ack);
                    }
                    Err(_) => return SessionOutcome::Died(String::new()),
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
                return SessionOutcome::Died(": could not expire a permission".into());
            }
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
            return SessionOutcome::NeverStarted;
        }
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

fn catalogue_events(catalog: &Catalog) -> Vec<AcpEvent> {
    let mut events = Vec::new();
    let commands: Vec<crate::AvailableCommandInfo> = catalog
        .commands()
        .into_iter()
        .map(|command| crate::AvailableCommandInfo {
            name: command.name,
            description: command.description,
        })
        .collect();
    if !commands.is_empty() {
        events.push(AcpEvent::AvailableCommands(commands));
    }
    let effort = catalog.effort();
    events.push(AcpEvent::Effort(EffortOption {
        option_id: effort.option_id,
        name: Some("Effort".into()),
        current_value: Some("default".into()),
        choices: effort
            .choices
            .into_iter()
            .map(|choice| EffortChoice {
                value: choice.value,
                name: choice.name,
            })
            .collect(),
    }));
    events
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

async fn drain_stderr(stderr: async_process::ChildStderr, tail: StderrTail, shared: Arc<Shared>) {
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
                if crate::looks_like_mcp_warning(trimmed) {
                    shared.push_mcp_warnings(vec![trimmed.to_string()]);
                }
                crate::push_stderr_tail(&tail, trimmed.to_string());
            }
        }
    }
}
