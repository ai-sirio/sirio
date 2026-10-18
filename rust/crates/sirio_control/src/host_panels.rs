//! `panel.*` answered by the host's terminal sessions (spec SP2 §5).
//!
//! One translation, used twice: by `sirioctl` when no app is listening, and
//! by the app, which forwards every `panel.*` here and then adds the layout
//! fields it alone knows. The wire names are the control socket's, unchanged;
//! only the answering side moved.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;

use sirio_host_client::TermClient;
use sirio_host_client::connection::CallError;
use sirio_host_protocol::messages::ErrorCode;
use sirio_host_protocol::term::{
    CreateParams, ExitStatus, Origin, ReadFormat, SessionSummary, TermCommand, TermSize, b64_encode,
};

use crate::panel::{ControlSession, PaneError, PaneInfo, terminal_key_bytes};
use crate::protocol::{ControlRequest, ControlResponse};

/// The size a control-owned session starts with; nothing draws it.
const CONTROL_SIZE: TermSize = TermSize { cols: 120, rows: 40 };

/// How long one `term.wait` call may block when a caller waits without a
/// timeout. The loop repeats until the session exits, so the bound only
/// keeps each call inside the client's deadline.
const UNBOUNDED_WAIT_SLICE: Duration = Duration::from_secs(30);

/// Why a call did not get an answer from the host. `Unreached` is a lost
/// connection: the request's fate is unknown and the caller may reconnect.
#[derive(Debug, PartialEq, Eq)]
pub enum Unreached {
    No(String),
}

/// A failed dispatch: a pane error the caller reports, or no answer at all.
#[derive(Debug)]
pub enum Failure {
    Pane(String),
    Unreached(String),
}

impl From<String> for Failure {
    fn from(message: String) -> Self {
        Self::Pane(message)
    }
}

pub struct HostPanels {
    client: TermClient,
    /// The variables a pane gets over the user environment, from the directory
    /// it starts in (`SIRIO_WORKTREE_ID`, from the app's worktree that holds
    /// it). `None`, as sirioctl's fallback has it, gives every pane none.
    worktree_environment: Option<
        std::sync::Arc<dyn Fn(&std::path::Path) -> BTreeMap<String, String> + Send + Sync>,
    >,
}

impl HostPanels {
    pub fn new(client: TermClient) -> Self {
        Self {
            client,
            worktree_environment: None,
        }
    }

    /// The panes this translation creates get the variables `resolve` names
    /// for the directory each one starts in, whichever side answered. `resolve`
    /// runs once per pane, at the moment it is created.
    pub fn with_worktree_environment(
        mut self,
        resolve: std::sync::Arc<dyn Fn(&std::path::Path) -> BTreeMap<String, String> + Send + Sync>,
    ) -> Self {
        self.worktree_environment = Some(resolve);
        self
    }

    /// The `term.create` of a pane, in the user's environment. `env_exact` is
    /// what the host can honour: `TermClient::create` clears it for a host
    /// without the capability, and the blanks then stand in for it. A split
    /// passes its parent's `cwd`, so a split resolves from the same directory.
    fn create_params(
        &self,
        cwd: String,
        command: Option<&String>,
        parent: Option<String>,
    ) -> CreateParams {
        let exact = self.client.env_exact();
        let extra = self
            .worktree_environment
            .as_ref()
            .map_or_else(BTreeMap::new, |resolve| resolve(std::path::Path::new(&cwd)));
        CreateParams {
            session_id: None,
            cwd,
            command: pane_command(command.map(String::as_str)),
            env: sirio_env::host_create_env(
                exact,
                &sirio_env::for_child(),
                &extra,
                &sirio_env::absent_from_user(),
            ),
            size: CONTROL_SIZE,
            origin: Origin::Control,
            parent,
            prelude: None,
            env_exact: exact,
        }
    }

    /// Answers one `panel.*` request. Pane errors keep the app's wording, so
    /// a script sees the same text whichever side answered; a lost connection
    /// is `Err(Unreached)`, never a pane error.
    pub fn handle(&self, request: &ControlRequest) -> Result<ControlResponse, Unreached> {
        match self.answer(request) {
            Ok(response) => Ok(response),
            Err(Failure::Unreached(message)) => Err(Unreached::No(message)),
            Err(Failure::Pane(message)) => Ok(ControlResponse::failure(&request.id, message)),
        }
    }

    /// `handle` with the failures still typed: a pane error is an `Ok`
    /// failure response, a lost connection is `Err(Failure::Unreached)`.
    pub fn answer(&self, request: &ControlRequest) -> Result<ControlResponse, Failure> {
        match self.dispatch(request) {
            Ok(result) => Ok(ControlResponse::success(&request.id, result)),
            Err(failure @ Failure::Unreached(_)) => Err(failure),
            Err(Failure::Pane(message)) => Ok(ControlResponse::failure(&request.id, message)),
        }
    }

    /// Whether a request may be sent again after the connection was rebuilt:
    /// reads and waits always; a write, a create or a close only where the
    /// caller knows it was not sent (the app never retries those).
    pub fn is_read_only(method: &str) -> bool {
        matches!(
            method,
            "panel.list" | "panel.read" | "panel.scrollback" | "panel.state" | "panel.wait"
        )
    }

    fn dispatch(&self, request: &ControlRequest) -> Result<BTreeMap<String, String>, Failure> {
        let params = &request.params;
        match request.method.as_str() {
            "panel.create" => {
                let cwd = match params.get("cwd") {
                    Some(cwd) => cwd.clone(),
                    None => std::env::current_dir()
                        .map_err(|error| format!("cannot spawn pane: {error}"))?
                        .to_string_lossy()
                        .into_owned(),
                };
                let created = self
                    .client
                    .create(&self.create_params(cwd, params.get("cmd"), None))
                    .map_err(|error| host_error("", error))?;
                Ok(one("id", created.session_id))
            }
            "panel.split" => {
                let source = required(params, "from", "panel.split requires from")?;
                let direction = required(params, "direction", "panel.split requires direction")?;
                if !matches!(direction.as_str(), "left" | "right" | "up" | "down") {
                    return Err(Failure::Pane(PaneError::InvalidDirection(direction).to_string()));
                }
                let parent = self.session(&source)?;
                let created = self
                    .client
                    .create(&self.create_params(
                        parent.cwd,
                        params.get("cmd"),
                        Some(source.clone()),
                    ))
                    .map_err(|error| host_error(&source, error))?;
                Ok(one("id", created.session_id))
            }
            "panel.list" => {
                let panes = self.control_panes(None)?;
                let rows: Vec<BTreeMap<String, String>> = panes
                    .into_iter()
                    .map(|pane| {
                        BTreeMap::from([
                            ("id".to_string(), pane.id),
                            ("tab".to_string(), pane.tab),
                            ("title".to_string(), pane.title),
                            ("agent".to_string(), pane.agent),
                            ("active".to_string(), pane.active.to_string()),
                        ])
                    })
                    .collect();
                Ok(one("panels", crate::protocol::rows::encode(&rows)))
            }
            "panel.write" => {
                let id = required(params, "id", "panel.write requires id")?;
                let input = required(params, "input", "panel.write requires input")?;
                self.client
                    .write(&id, input.as_bytes())
                    .map_err(|error| host_error(&id, error))?;
                Ok(BTreeMap::new())
            }
            "panel.key" => {
                let id = required(params, "id", "panel.key requires id")?;
                let key = required(params, "key", "panel.key requires key")?;
                // The control vocabulary is validated here and sent as the
                // bytes it names, so the host's own key names never leak in.
                let bytes = terminal_key_bytes(&key).map_err(|error| error.to_string())?;
                self.client
                    .write(&id, &bytes)
                    .map_err(|error| host_error(&id, error))?;
                Ok(BTreeMap::new())
            }
            "panel.read" => {
                let id = required(params, "id", "panel.read requires id")?;
                let format = read_format(params.get("format").map(String::as_str))?;
                let read = self
                    .client
                    .read(&id, format, None)
                    .map_err(|error| host_error(&id, error))?;
                Ok(one("data", base64_of(read.format, read.data)))
            }
            "panel.scrollback" => {
                let id = required(params, "id", "panel.scrollback requires id")?;
                let max_bytes = match params.get("maxBytes") {
                    Some(value) => Some(
                        value
                            .parse::<u64>()
                            .map_err(|_| "maxBytes must be a nonnegative integer".to_string())?,
                    ),
                    None => None,
                };
                let read = self
                    .client
                    .read(&id, Some(ReadFormat::Text), max_bytes)
                    .map_err(|error| host_error(&id, error))?;
                let bytes = read.data.len();
                Ok(BTreeMap::from([
                    ("data".to_string(), b64_encode(read.data.as_bytes())),
                    ("bytes".to_string(), bytes.to_string()),
                ]))
            }
            "panel.state" => {
                let id = required(params, "id", "panel.state requires id")?;
                let session = self.session(&id)?;
                let mut result = BTreeMap::from([
                    ("workingDirectory".to_string(), session.cwd),
                    ("survives".to_string(), "true".to_string()),
                ]);
                match session.exit {
                    None => {
                        result.insert("exitStatus".to_string(), "running".to_string());
                    }
                    Some(status) => {
                        let (label, code) = exit_label(&status);
                        result.insert("exitStatus".to_string(), label);
                        if let Some(code) = code {
                            result.insert("exitCode".to_string(), code.to_string());
                        }
                    }
                }
                Ok(result)
            }
            "panel.wait" => {
                let id = required(params, "id", "panel.wait requires id")?;
                let timeout = match params.get("timeoutMs") {
                    Some(value) => Some(Duration::from_millis(
                        value
                            .parse::<u64>()
                            .map_err(|_| "timeoutMs must be a nonnegative integer".to_string())?,
                    )),
                    None => None,
                };
                let exit_code = self.wait(&id, timeout)?;
                Ok(one("exitCode", exit_code.to_string()))
            }
            "panel.close" => {
                let id = required(params, "id", "panel.close requires id")?;
                self.client
                    .close(&id)
                    .map_err(|error| host_error(&id, error))?;
                Ok(BTreeMap::new())
            }
            "panel.show" => Err(Failure::Pane(
                "Show session needs Sirio running".to_string(),
            )),
            "panel.focus" => Err(Failure::Pane(
                "panel.focus needs Sirio running".to_string(),
            )),
            other => Err(Failure::Pane(format!("unknown method: {other}"))),
        }
    }

    /// The control-owned sessions the host lists, with their cwd.
    /// Runs off the GPUI thread: the app calls it from the control socket.
    pub fn control_sessions(&self) -> Result<Vec<ControlSession>, Failure> {
        let listed = self.client.list().map_err(|error| host_error("", error))?;
        Ok(control_sessions_from(&listed.sessions))
    }

    /// The control-owned sessions in the shape `panel.list` reports. With
    /// `cwd`, only the sessions whose working directory is exactly that path.
    pub fn control_panes(&self, cwd: Option<&str>) -> Result<Vec<PaneInfo>, Failure> {
        Ok(self
            .control_sessions()?
            .into_iter()
            .filter(|session| {
                cwd.is_none_or(|cwd| session.working_directory == std::path::Path::new(cwd))
            })
            .map(|session| session.info)
            .collect())
    }

    fn session(&self, id: &str) -> Result<SessionSummary, Failure> {
        let listed = self
            .client
            .list()
            .map_err(|error| host_error(id, error))?;
        listed
            .sessions
            .into_iter()
            .find(|session| session.session_id == id)
            .ok_or_else(|| Failure::Pane(PaneError::UnknownPane(id.to_string()).to_string()))
    }

    /// Waits for the exit code. A timed-out wait is `pane timed out`; with no
    /// timeout the wait repeats in slices until the session exits.
    fn wait(&self, id: &str, timeout: Option<Duration>) -> Result<i32, Failure> {
        loop {
            let slice = timeout.unwrap_or(UNBOUNDED_WAIT_SLICE);
            let waited = self
                .client
                .wait(id, slice)
                .map_err(|error| host_error(id, error))?;
            match waited.exit {
                Some(status) => return Ok(exit_code_of(&status)),
                None if timeout.is_some() => {
                    return Err(Failure::Pane(PaneError::TimedOut(id.to_string()).to_string()));
                }
                None => {}
            }
        }
    }
}

/// `cmd` runs through the platform shell, as the control-owned PTY always
/// did; no `cmd` opens the login shell.
fn pane_command(command: Option<&str>) -> TermCommand {
    match command {
        None => TermCommand::Shell,
        Some(command) => {
            let (program, flag) = command_shell();
            TermCommand::Program {
                program,
                args: vec![flag.to_string(), command.to_string()],
            }
        }
    }
}

#[cfg(unix)]
fn command_shell() -> (String, &'static str) {
    let program = std::env::var("SHELL")
        .ok()
        .filter(|shell| !shell.is_empty())
        .unwrap_or_else(|| "/bin/sh".to_string());
    (program, "-c")
}

#[cfg(windows)]
fn command_shell() -> (String, &'static str) {
    let program = std::env::var("COMSPEC").unwrap_or_else(|_| "cmd.exe".to_string());
    (program, "/c")
}

fn read_format(value: Option<&str>) -> Result<Option<ReadFormat>, String> {
    match value {
        None => Ok(None),
        Some("raw") => Ok(Some(ReadFormat::Raw)),
        Some("text") => Ok(Some(ReadFormat::Text)),
        Some(other) => Err(format!("unknown read format: {other}")),
    }
}

/// `panel.read` answers base64 of the bytes whichever format the host used.
fn base64_of(format: ReadFormat, data: String) -> String {
    match format {
        ReadFormat::Raw => data,
        ReadFormat::Text | ReadFormat::Other => b64_encode(data.as_bytes()),
    }
}

fn exit_label(status: &ExitStatus) -> (String, Option<i32>) {
    match status {
        ExitStatus::Success => ("success".to_string(), Some(0)),
        ExitStatus::Code { code } => (format!("code:{code}"), Some(*code)),
        ExitStatus::Unknown | ExitStatus::Other => ("unknown".to_string(), None),
    }
}

/// The same convention `PaneRegistry` used: a signal or an unknown end is 1.
fn exit_code_of(status: &ExitStatus) -> i32 {
    match status {
        ExitStatus::Success => 0,
        ExitStatus::Code { code } => *code,
        ExitStatus::Unknown | ExitStatus::Other => 1,
    }
}

fn required(
    params: &BTreeMap<String, String>,
    name: &str,
    message: &str,
) -> Result<String, String> {
    params
        .get(name)
        .cloned()
        .ok_or_else(|| message.to_string())
}

fn one(key: &str, value: String) -> BTreeMap<String, String> {
    BTreeMap::from([(key.to_string(), value)])
}

/// Maps a host failure onto the app's pane errors. `id` is the pane the call
/// was about, empty when none was named.
fn host_error(id: &str, error: CallError) -> Failure {
    match error {
        CallError::Remote(body) => Failure::Pane(match body.code {
            ErrorCode::UnknownSession => PaneError::UnknownPane(id.to_string()).to_string(),
            ErrorCode::SessionExited => PaneError::ClosedPane(id.to_string()).to_string(),
            ErrorCode::InvalidCwd => PaneError::InvalidWorkingDirectory(body.message).to_string(),
            ErrorCode::SpawnFailed => PaneError::Spawn(body.message).to_string(),
            _ => PaneError::Io(body.message).to_string(),
        }),
        CallError::Io(_) | CallError::Frame(_) => Failure::Unreached(error.to_string()),
        other => Failure::Pane(PaneError::Io(other.to_string()).to_string()),
    }
}

/// The control-owned sessions among a host listing, as the registry holds them.
pub fn control_sessions_from(sessions: &[SessionSummary]) -> Vec<ControlSession> {
    sessions
        .iter()
        .filter(|session| session.origin == Origin::Control)
        .map(|session| ControlSession {
            info: PaneInfo {
                id: session.session_id.clone(),
                tab: String::new(),
                title: session.title.clone(),
                agent: String::new(),
                active: false,
            },
            working_directory: PathBuf::from(&session.cwd),
        })
        .collect()
}
