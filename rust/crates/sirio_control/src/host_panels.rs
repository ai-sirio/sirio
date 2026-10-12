//! `panel.*` answered by the host's terminal sessions (spec SP2 §5).
//!
//! One translation, used twice: by `sirioctl` when no app is listening, and
//! by the app, which forwards every `panel.*` here and then adds the layout
//! fields it alone knows. The wire names are the control socket's, unchanged;
//! only the answering side moved.

use std::collections::BTreeMap;
use std::time::Duration;

use sirio_host_client::TermClient;
use sirio_host_client::connection::CallError;
use sirio_host_protocol::messages::ErrorCode;
use sirio_host_protocol::term::{
    CreateParams, ExitStatus, Origin, ReadFormat, SessionSummary, TermCommand, TermSize, b64_encode,
};

use crate::panel::{PaneError, PaneInfo, terminal_key_bytes};
use crate::protocol::{ControlRequest, ControlResponse};

/// The size a control-owned session starts with; nothing draws it.
const CONTROL_SIZE: TermSize = TermSize { cols: 120, rows: 40 };

/// How long one `term.wait` call may block when a caller waits without a
/// timeout. The loop repeats until the session exits, so the bound only
/// keeps each call inside the client's deadline.
const UNBOUNDED_WAIT_SLICE: Duration = Duration::from_secs(30);

pub struct HostPanels {
    client: TermClient,
}

impl HostPanels {
    pub fn new(client: TermClient) -> Self {
        Self { client }
    }

    /// Answers one `panel.*` request. Errors keep the app's wording, so a
    /// script sees the same text whichever side answered.
    pub fn handle(&self, request: &ControlRequest) -> ControlResponse {
        match self.dispatch(request) {
            Ok(result) => ControlResponse::success(&request.id, result),
            Err(message) => ControlResponse::failure(&request.id, message),
        }
    }

    fn dispatch(&self, request: &ControlRequest) -> Result<BTreeMap<String, String>, String> {
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
                    .create(&create_params(cwd, params.get("cmd"), None))
                    .map_err(|error| host_error("", error))?;
                Ok(one("id", created.session_id))
            }
            "panel.split" => {
                let source = required(params, "from", "panel.split requires from")?;
                let direction = required(params, "direction", "panel.split requires direction")?;
                if !matches!(direction.as_str(), "left" | "right" | "up" | "down") {
                    return Err(PaneError::InvalidDirection(direction).to_string());
                }
                let parent = self.session(&source)?;
                let created = self
                    .client
                    .create(&create_params(
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
            "panel.focus" | "panel.show" => Err(format!(
                "{} needs Sirio running",
                request.method
            )),
            other => Err(format!("unknown method: {other}")),
        }
    }

    /// The control-owned sessions, in the shape `panel.list` reports. With
    /// `cwd`, only the sessions whose working directory is exactly that path.
    pub fn control_panes(&self, cwd: Option<&str>) -> Result<Vec<PaneInfo>, String> {
        let listed = self.client.list().map_err(|error| host_error("", error))?;
        Ok(listed
            .sessions
            .into_iter()
            .filter(|session| session.origin == Origin::Control)
            .filter(|session| cwd.is_none_or(|cwd| session.cwd == cwd))
            .map(|session| PaneInfo {
                id: session.session_id,
                tab: String::new(),
                title: session.title,
                agent: String::new(),
                active: false,
            })
            .collect())
    }

    fn session(&self, id: &str) -> Result<SessionSummary, String> {
        let listed = self
            .client
            .list()
            .map_err(|error| host_error(id, error))?;
        listed
            .sessions
            .into_iter()
            .find(|session| session.session_id == id)
            .ok_or_else(|| PaneError::UnknownPane(id.to_string()).to_string())
    }

    /// Waits for the exit code. A timed-out wait is `pane timed out`; with no
    /// timeout the wait repeats in slices until the session exits.
    fn wait(&self, id: &str, timeout: Option<Duration>) -> Result<i32, String> {
        loop {
            let slice = timeout.unwrap_or(UNBOUNDED_WAIT_SLICE);
            let waited = self
                .client
                .wait(id, slice)
                .map_err(|error| host_error(id, error))?;
            match waited.exit {
                Some(status) => return Ok(exit_code_of(&status)),
                None if timeout.is_some() => {
                    return Err(PaneError::TimedOut(id.to_string()).to_string());
                }
                None => {}
            }
        }
    }
}

fn create_params(cwd: String, command: Option<&String>, parent: Option<String>) -> CreateParams {
    CreateParams {
        session_id: None,
        cwd,
        command: pane_command(command.map(String::as_str)),
        env: std::env::vars().collect(),
        size: CONTROL_SIZE,
        origin: Origin::Control,
        parent,
        prelude: None,
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
fn host_error(id: &str, error: CallError) -> String {
    match error {
        CallError::Remote(body) => match body.code {
            ErrorCode::UnknownSession => PaneError::UnknownPane(id.to_string()).to_string(),
            ErrorCode::SessionExited => PaneError::ClosedPane(id.to_string()).to_string(),
            ErrorCode::InvalidCwd => PaneError::InvalidWorkingDirectory(body.message).to_string(),
            ErrorCode::SpawnFailed => PaneError::Spawn(body.message).to_string(),
            _ => PaneError::Io(body.message).to_string(),
        },
        other => PaneError::Io(other.to_string()).to_string(),
    }
}
