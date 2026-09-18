//! The thread that owns the `claude` process.
//!
//! One thread, three jobs: write command lines to its stdin, fold its
//! stdout into events, and make sure that when it dies the user is told
//! why. The stderr drain, the exit probe and the tail report are the ACP
//! client's own helpers, shared rather than re-derived — a death report
//! that names one cause here and another there would be worse than none.

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
use crate::{
    AcpError, AcpEvent, AgentMode, ChildHandle, EffortChoice, EffortOption, ExitStatusSlot,
    ModeCatalog, ModelCatalog, ModelOption, StderrTail,
};

/// What the client asks the worker to do.
#[derive(Debug)]
pub(super) enum Command {
    /// A plain-text turn. Task 8 replaces this arm with the block-building
    /// one; the variant stays.
    Prompt(String),
    /// Stop: close the CLI's stdin, wait for it, acknowledge.
    Shutdown(mpsc::SyncSender<()>),
    // Tasks 8 and 9 add SetModel, SetMode, SetEffort, Cancel,
    // RespondPermission and RewindFiles here.
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
        child: _child,
        activity,
        prompt_timeout: _prompt_timeout,
        permission_timeout: _permission_timeout,
    } = context;
    let mut lines = BufReader::new(stdout).lines();
    let mut fold = Fold::new();
    let request_ids = AtomicU64::new(1);
    let next_request_id = || format!("sirio-{}", request_ids.fetch_add(1, Ordering::Relaxed));
    let prompt_ids = AtomicU64::new(1);

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
    // stops. Tasks 8 and 9 extend the command arm.
    loop {
        let next_line = lines.next();
        let next_command = command_rx.recv();
        futures::pin_mut!(next_line, next_command);
        match futures::future::select(next_line, next_command).await {
            futures::future::Either::Left((line, _)) => match line {
                Some(Ok(line)) => {
                    crate::touch_activity(&activity);
                    let Some(message) = CliMessage::parse(&line) else {
                        continue;
                    };
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
            },
            futures::future::Either::Right((command, _)) => match command {
                Ok(Command::Prompt(text)) => {
                    // The watchdog measures silence from here. A turn that
                    // reports is a turn that lives, however long it takes.
                    crate::touch_activity(&activity);
                    let uuid = format!(
                        "sirio-prompt-{}",
                        prompt_ids.fetch_add(1, Ordering::Relaxed)
                    );
                    let line = serde_json::json!({
                        "type": "user",
                        "uuid": uuid,
                        "message": {
                            "role": "user",
                            "content": [{"type": "text", "text": text}],
                        },
                    });
                    if write_line(&mut stdin, &line).await.is_err() {
                        return SessionOutcome::Died(": could not write the turn".into());
                    }
                }
                Ok(Command::Shutdown(ack)) => {
                    // Closing stdin is how a stream-json session ends.
                    drop(stdin);
                    return SessionOutcome::CleanShutdown(ack);
                }
                Err(_) => return SessionOutcome::Died(String::new()),
            },
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
