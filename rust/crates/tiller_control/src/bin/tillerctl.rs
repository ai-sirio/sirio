//! `tillerctl` — control a running Tiller over its unix socket.
//!
//! Ported from `tillerctl/Tillerctl.swift` and `tillerctl/CmuxCommands.swift`
//! with the same subcommand names, options, output columns and error text.
//! Argument parsing is hand-rolled (`--key value`, `--flag`, positional
//! extras) to keep the crate dependency-free; the wire protocol is the
//! parity-critical part.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::exit;
use std::time::Duration;

use tiller_control::protocol::rows;
use tiller_control::{
    ControlRequest, ControlResponse, default_socket_path,
    extract::{session_ref_from_json, session_ref_from_payload_arguments},
    round_trip,
};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(subcommand) = args.first() else {
        usage();
        exit(2);
    };

    let parsed = match parse_args(&args[1..]) {
        Ok(parsed) => parsed,
        Err(message) => {
            eprintln!("tillerctl: {message}");
            exit(2);
        }
    };

    // --socket overrides $TILLER_SOCKET, which overrides the default path.
    let environment = current_environment();
    let socket = parsed
        .value("socket")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(default_socket_path(&environment)));

    let result = match subcommand.as_str() {
        "ping" => cmd_ping(socket, &parsed),
        "capabilities" => cmd_capabilities(socket, &parsed),
        "identify" => cmd_identify(socket, &parsed, &environment),
        "list-workspaces" => cmd_list_workspaces(socket, &parsed),
        "new-workspace" => cmd_new_workspace(socket, &parsed),
        "select-workspace" => cmd_select_workspace(socket, &parsed),
        "current-workspace" => cmd_current_workspace(socket, &parsed),
        "close-workspace" => cmd_close_workspace(socket, &parsed),
        "list-notifications" => cmd_list_notifications(socket, &parsed),
        "clear-notifications" => cmd_clear_notifications(socket, &parsed),
        "notify" => cmd_notify(socket, &parsed),
        "session-ref" => cmd_session_ref(socket, &parsed),
        _ => {
            eprintln!("tillerctl: unknown command '{subcommand}'");
            usage();
            exit(2);
        }
    };

    match result {
        Ok(()) => exit(0),
        Err(message) => {
            eprintln!("tillerctl: {message}");
            exit(1);
        }
    }
}

fn usage() {
    eprintln!(
        "tillerctl — control a running Tiller.app over its unix socket.\n\
         \n\
         usage: tillerctl [--socket <path>] <command> [options]\n\
         \n\
         commands:\n\
         \x20 ping                              check that Tiller is running\n\
         \x20 capabilities [--json]             list available socket methods\n\
         \x20 identify [--json]                 show the current workspace/surface context\n\
         \x20 list-workspaces [--json]          list all worktrees\n\
         \x20 new-workspace --project <p> [--branch <b>]\n\
         \x20 select-workspace --workspace <w>  select a worktree in the sidebar\n\
         \x20 current-workspace [--json]        show the selected worktree\n\
         \x20 close-workspace --workspace <w>   unmount a worktree's terminals\n\
         \x20 list-notifications [--json]       list delivered notifications\n\
         \x20 clear-notifications               clear delivered notifications\n\
         \x20 notify [--session s] [--status st] [--agent-session r] [--stdin-json] [--title t] [--subtitle s] [--body b] [extra...]\n\
         \x20 session-ref --session s --ref r   report an agent-native session reference\n\
         \n\
         options:\n\
         \x20 --socket <path>   socket path (default: $TILLER_SOCKET or app support)\n\
         \x20 --json            output raw JSON\n\
         \x20 --stdin-json      (notify) read a hook JSON payload from stdin and extract the agent session id"
    );
}

/// Parsed command-line arguments: `--key value` pairs, `--flag` booleans,
/// and positional extras, in order.
struct ParsedArgs {
    values: BTreeMap<String, String>,
    flags: std::collections::BTreeSet<String>,
    positional: Vec<String>,
}

fn parse_args(args: &[String]) -> Result<ParsedArgs, String> {
    let mut parsed = ParsedArgs {
        values: BTreeMap::new(),
        flags: std::collections::BTreeSet::new(),
        positional: Vec::new(),
    };
    let mut index = 0;
    while index < args.len() {
        let arg = &args[index];
        if let Some(key) = arg.strip_prefix("--") {
            if key.is_empty() {
                return Err(format!("unexpected argument '{arg}'"));
            }
            if let Some(rest) = key.split_once('=') {
                parsed.values.insert(rest.0.to_string(), rest.1.to_string());
            } else if index + 1 < args.len() && !args[index + 1].starts_with("--") {
                // Value-taking option.
                index += 1;
                parsed.values.insert(key.to_string(), args[index].clone());
            } else {
                // Boolean flag.
                parsed.flags.insert(key.to_string());
            }
        } else {
            parsed.positional.push(arg.clone());
        }
        index += 1;
    }
    Ok(parsed)
}

impl ParsedArgs {
    fn value(&self, key: &str) -> Option<&str> {
        self.values.get(key).map(String::as_str)
    }

    fn flag(&self, key: &str) -> bool {
        self.flags.contains(key)
    }

    fn require(&self, key: &str) -> Result<String, String> {
        self.value(key)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .ok_or_else(|| format!("Missing required option '--{key}'"))
    }
}

fn current_environment() -> BTreeMap<String, String> {
    std::env::vars().collect()
}

// ---------------------------------------------------------------------------
// Round trip plumbing
// ---------------------------------------------------------------------------

/// Sends the request and returns the response, or exits with the canonical
/// connection-failure message on a transport error.
fn round_trip_or_die(socket: PathBuf, request: &ControlRequest) -> ControlResponse {
    match round_trip(&socket, request, Duration::from_secs(3600)) {
        Ok(response) => response,
        Err(error) => {
            eprintln!(
                "tillerctl: Tiller control socket is disabled or Tiller is not running ({error})"
            );
            exit(1);
        }
    }
}

fn require_ok(socket: PathBuf, request: &ControlRequest) -> ControlResponse {
    let response = round_trip_or_die(socket, request);
    if !response.ok {
        eprintln!(
            "tillerctl: {}",
            response.error.clone().unwrap_or_else(|| request.method.clone())
        );
        exit(1);
    }
    response
}

/// Prints a rows result: `--json` prints the embedded JSON array verbatim;
/// otherwise one line per row, tab-separated in `columns` order.
fn print_rows(response: &ControlResponse, key: &str, columns: &[&str], as_json: bool) {
    let raw = response.result.as_ref().and_then(|r| r.get(key)).cloned().unwrap_or_else(|| "[]".to_string());
    if as_json {
        println!("{raw}");
        return;
    }
    if let Some(rows) = rows::decode(&raw) {
        for row in rows {
            let line: Vec<&str> = columns
                .iter()
                .map(|column| row.get(*column).map(String::as_str).unwrap_or(""))
                .collect();
            println!("{}", line.join("\t"));
        }
    }
}

/// Prints a single-object result: `--json` re-encodes the result as one row;
/// otherwise the columns tab-separated.
fn print_result(response: &ControlResponse, columns: &[&str], as_json: bool) {
    let result = response.result.clone().unwrap_or_default();
    if as_json {
        println!("{}", rows::encode(&[result]));
        return;
    }
    let line: Vec<&str> = columns
        .iter()
        .filter_map(|column| result.get(*column).map(String::as_str))
        .collect();
    println!("{}", line.join("\t"));
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

fn cmd_ping(socket: PathBuf, _parsed: &ParsedArgs) -> Result<(), String> {
    let response = require_ok(socket, &tiller_control::protocol::request::system_ping());
    let _ = response;
    println!("pong");
    Ok(())
}

fn cmd_capabilities(socket: PathBuf, parsed: &ParsedArgs) -> Result<(), String> {
    let response = require_ok(socket, &tiller_control::protocol::request::system_capabilities());
    print_rows(&response, "methods", &["method"], parsed.flag("json"));
    Ok(())
}

fn cmd_identify(
    socket: PathBuf,
    parsed: &ParsedArgs,
    environment: &BTreeMap<String, String>,
) -> Result<(), String> {
    let worktree = environment.get("TILLER_WORKTREE_ID").map(String::as_str);
    let pane = environment.get("TILLER_PANE_ID").map(String::as_str);
    let response = require_ok(
        socket,
        &tiller_control::protocol::request::system_identify(worktree, pane),
    );
    print_result(
        &response,
        &["project", "branch", "path", "workspaceId", "surfaceId"],
        parsed.flag("json"),
    );
    Ok(())
}

fn cmd_list_workspaces(socket: PathBuf, parsed: &ParsedArgs) -> Result<(), String> {
    let response = require_ok(socket, &tiller_control::protocol::request::workspace_list());
    print_rows(
        &response,
        "workspaces",
        &["id", "project", "branch", "path", "selected"],
        parsed.flag("json"),
    );
    Ok(())
}

fn cmd_new_workspace(socket: PathBuf, parsed: &ParsedArgs) -> Result<(), String> {
    let project = parsed.require("project")?;
    let branch = parsed.value("branch").map(str::to_string);
    let response = require_ok(
        socket,
        &tiller_control::protocol::request::workspace_create(&project, branch.as_deref()),
    );
    if let Some(id) = response.result.as_ref().and_then(|r| r.get("id")) {
        println!("{id}");
    }
    Ok(())
}

fn cmd_select_workspace(socket: PathBuf, parsed: &ParsedArgs) -> Result<(), String> {
    let workspace = parsed.require("workspace")?;
    let _ = require_ok(
        socket,
        &tiller_control::protocol::request::workspace_select(&workspace),
    );
    Ok(())
}

fn cmd_current_workspace(socket: PathBuf, parsed: &ParsedArgs) -> Result<(), String> {
    let response = require_ok(socket, &tiller_control::protocol::request::workspace_current());
    print_result(
        &response,
        &["project", "branch", "path", "id"],
        parsed.flag("json"),
    );
    Ok(())
}

fn cmd_close_workspace(socket: PathBuf, parsed: &ParsedArgs) -> Result<(), String> {
    let workspace = parsed.require("workspace")?;
    let _ = require_ok(
        socket,
        &tiller_control::protocol::request::workspace_close(&workspace),
    );
    Ok(())
}

fn cmd_list_notifications(socket: PathBuf, parsed: &ParsedArgs) -> Result<(), String> {
    let response = require_ok(socket, &tiller_control::protocol::request::notification_list());
    print_rows(
        &response,
        "notifications",
        &["date", "title", "subtitle", "body"],
        parsed.flag("json"),
    );
    Ok(())
}

fn cmd_clear_notifications(socket: PathBuf, _parsed: &ParsedArgs) -> Result<(), String> {
    let _ = require_ok(socket, &tiller_control::protocol::request::notification_clear());
    Ok(())
}

fn cmd_session_ref(socket: PathBuf, parsed: &ParsedArgs) -> Result<(), String> {
    let session = parsed.require("session")?;
    let ref_value = parsed.require("ref")?;
    let _ = require_ok(
        socket,
        &tiller_control::protocol::request::session_ref(&session, &ref_value),
    );
    Ok(())
}

/// The notify command: user notification (--title) or agent-status update
/// (--session/--status), mirroring the Swift CLI's two modes.
fn cmd_notify(socket: PathBuf, parsed: &ParsedArgs) -> Result<(), String> {
    if let Some(title) = parsed.value("title") {
        let subtitle = parsed.value("subtitle").map(str::to_string);
        let body = parsed.value("body").unwrap_or("").to_string();
        let _ = require_ok(
            socket,
            &tiller_control::protocol::request::notification_create(
                title,
                subtitle.as_deref(),
                &body,
            ),
        );
        return Ok(());
    }

    let session = parsed.require("session")?;
    let status = parsed.require("status")?;

    let mut ref_value = parsed.value("agent-session").map(str::to_string);
    if ref_value.is_none() && parsed.flag("stdin-json") {
        let data = read_bounded_stdin();
        ref_value = session_ref_from_json(&data);
    }
    if ref_value.is_none() {
        ref_value = session_ref_from_payload_arguments(&parsed.positional);
    }

    let _ = require_ok(
        socket,
        &tiller_control::protocol::request::notify(&session, &status, ref_value.as_deref()),
    );
    Ok(())
}

/// Reads stdin until EOF or 5 seconds, whichever comes first — the Swift CLI
/// bounds this read because a hung hook would otherwise wedge tillerctl
/// forever. Returns whatever was read (partial data included).
fn read_bounded_stdin() -> Vec<u8> {
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut data = Vec::new();
        let mut chunk = [0u8; 4096];
        loop {
            match std::io::Read::read(&mut std::io::stdin(), &mut chunk) {
                Ok(0) => break,
                Ok(n) => {
                    data.extend_from_slice(&chunk[..n]);
                    if data.len() > 1 << 20 {
                        break; // bounded payload
                    }
                }
                Err(_) => break,
            }
        }
        let _ = sender.send(data);
    });
    receiver
        .recv_timeout(Duration::from_secs(5))
        .unwrap_or_default()
}
