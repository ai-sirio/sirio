//! `sirioctl` — control a running Sirio over its unix socket.
//!
//! Ported from `sirioctl/Sirioctl.swift` and `sirioctl/CmuxCommands.swift`
//! with the same subcommand names, options, output columns and error text.
//! Argument parsing is hand-rolled (`--key value`, `--flag`, positional
//! extras) to keep the crate dependency-free; the wire protocol is the
//! parity-critical part.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::exit;
use std::time::Duration;

use sirio_control::protocol::rows;
use sirio_control::{
    ControlRequest, ControlResponse, default_socket_path,
    extract::{session_ref_from_json, session_ref_from_payload_arguments},
    format_version_json, format_version_lines, round_trip,
};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (subcommand_index, leading_socket) = if args.first().map(String::as_str) == Some("--socket")
    {
        if args.len() < 2 {
            eprintln!("sirioctl: --socket requires a path");
            usage();
            exit(2);
        }
        (2, Some(args[1].clone()))
    } else if let Some(socket) = args.first().and_then(|arg| arg.strip_prefix("--socket=")) {
        (1, Some(socket.to_string()))
    } else {
        (0, None)
    };
    let Some(subcommand) = args.get(subcommand_index) else {
        usage();
        exit(2);
    };

    let mut option_args = args[subcommand_index + 1..].to_vec();
    if let Some(socket) = leading_socket {
        option_args.insert(0, format!("--socket={socket}"));
    }
    let parsed = match parse_args(&option_args) {
        Ok(parsed) => parsed,
        Err(message) => {
            eprintln!("sirioctl: {message}");
            exit(2);
        }
    };

    // --socket overrides $SIRIO_SOCKET, which overrides the default path.
    let environment = current_environment();
    let socket = parsed
        .value("socket")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(default_socket_path(&environment)));

    let result = match subcommand.as_str() {
        "ping" => cmd_ping(socket, &parsed),
        "version" => cmd_version(socket, &parsed),
        "quit" => cmd_quit(socket, &parsed),
        "capabilities" => cmd_capabilities(socket, &parsed),
        "identify" => cmd_identify(socket, &parsed, &environment),
        "project" => cmd_project(socket, &parsed),
        "list-workspaces" => cmd_list_workspaces(socket, &parsed),
        "new-workspace" => cmd_new_workspace(socket, &parsed),
        "select-workspace" => cmd_select_workspace(socket, &parsed),
        "current-workspace" => cmd_current_workspace(socket, &parsed),
        "close-workspace" => cmd_close_workspace(socket, &parsed),
        "worktree-set" => cmd_worktree_set(socket, &parsed),
        "restore-session" => cmd_restore_session(socket, &parsed),
        "list-notifications" => cmd_list_notifications(socket, &parsed),
        "clear-notifications" => cmd_clear_notifications(socket, &parsed),
        "notify" => cmd_notify(socket, &parsed, &environment),
        "session-ref" => cmd_session_ref(socket, &parsed, &environment),
        "pane" => cmd_pane(socket, &parsed),
        "tab" => cmd_tab(socket, &parsed),
        "panel" => cmd_panel(socket, &parsed),
        "surface" => cmd_surface(socket, &parsed),
        _ => {
            eprintln!("sirioctl: unknown command '{subcommand}'");
            usage();
            exit(2);
        }
    };

    match result {
        Ok(()) => exit(0),
        Err(message) => {
            eprintln!("sirioctl: {message}");
            exit(1);
        }
    }
}

fn usage() {
    eprintln!(
        "sirioctl — control a running Sirio.app over its unix socket.\n\
         \n\
         usage: sirioctl [--socket <path>] <command> [options]\n\
         \n\
         commands:\n\
         \x20 ping                              check that Sirio is running\n\
         \x20 version [--json]                  report CLI and running Sirio versions\n\
         \x20 quit                              gracefully quit Sirio\n\
         \x20 capabilities [--json]             list available socket methods\n\
         \x20 project list [--json]              list discovered projects and worktrees\n\
         \x20 project add <path> [--json]        add a project and report the new catalog\n\
         \x20 identify [--json]                 show the current workspace/surface context\n\
         \x20 list-workspaces [--json]          list all worktrees\n\
         \x20 new-workspace --project <p> [--branch <b>]\n\
         \x20 select-workspace --workspace <w>  select a worktree in the sidebar\n\
         \x20 current-workspace [--json]        show the selected worktree\n\
         \x20 close-workspace --workspace <w>   unmount a worktree's terminals\n\
         \x20 worktree-set --worktree <w> [--comment c] [--session s]\n\
         \x20 restore-session                   restore the launch snapshot\n\
         \x20 pane split <right|down>            split the focused application pane\n\
         \x20 pane focus <left|right|up|down>   focus a neighboring application pane\n\
         \x20 pane close                        close the focused application pane\n\
         \x20 tab cycle [forward|backward]      cycle application tabs\n\
         \x20 tab select <index>                select a 1-based application tab\n\
         \x20 surface changes open [--worktree w]\n\
         \x20 surface changes read              read the mounted Changes surface\n\
         \x20 surface changes stage <path> [--worktree w]\n\
         \x20 surface changes unstage <path> [--worktree w]\n\
         \x20 surface changes discard <path> [--worktree w]\n\
         \x20 surface changes stage-all [--worktree w]\n\
         \x20 surface changes discard-all [--worktree w]\n\
         \x20 surface settings open [--section s]\n\
         \x20 surface settings select <section>\n\
         \x20 surface settings read             read the mounted Settings section\n\
         \x20 panel state <id> [--json]         read terminal state and scrollback\n\
         \x20 panel scrollback <id> [--max-bytes n] [--json]\n\
         \x20 panel create [--worktree w] [--cmd c]\n\
         \x20 panel split <dir> --from <id> [--cmd c]\n\
         \x20 panel list [--worktree w]         list control panes\n\
         \x20 panel write <id> --input s [--enter]\n\
         \x20 panel key <id> --key k\n\
         \x20 panel read|wait|focus|close <id>\n\
         \x20 list-notifications [--json]       list delivered notifications\n\
         \x20 clear-notifications               clear delivered notifications\n\
         \x20 notify [--session s] [--status st] [--agent-session r] [--stdin-json] [--title t] [--subtitle s] [--body b] [extra...]\n\
         \x20 session-ref [--session s] --ref r report an agent-native session reference\n\
         \x20   (--session defaults to $SIRIO_PANE_ID; without either, agent-status commands exit 0 silently)\n\
         \n\
         options:\n\
         \x20 --socket <path>   socket path (default: $SIRIO_SOCKET or app support)\n\
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
                "sirioctl: Sirio control socket is disabled or Sirio is not running ({error})"
            );
            exit(1);
        }
    }
}

fn require_ok(socket: PathBuf, request: &ControlRequest) -> ControlResponse {
    let response = round_trip_or_die(socket, request);
    if !response.ok {
        eprintln!(
            "sirioctl: {}",
            response
                .error
                .clone()
                .unwrap_or_else(|| request.method.clone())
        );
        exit(1);
    }
    response
}

/// Prints a rows result: `--json` prints the embedded JSON array verbatim;
/// otherwise one line per row, tab-separated in `columns` order.
fn print_rows(response: &ControlResponse, key: &str, columns: &[&str], as_json: bool) {
    let raw = response
        .result
        .as_ref()
        .and_then(|r| r.get(key))
        .cloned()
        .unwrap_or_else(|| "[]".to_string());
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
    let response = require_ok(socket, &sirio_control::protocol::request::system_ping());
    let _ = response;
    println!("pong");
    Ok(())
}

fn cmd_version(socket: PathBuf, parsed: &ParsedArgs) -> Result<(), String> {
    let app_version = round_trip(
        &socket,
        &sirio_control::protocol::request::system_capabilities(),
        Duration::from_secs(1),
    )
    .ok()
    .and_then(|response| response.result)
    .and_then(|result| result.get("version").cloned());

    if parsed.flag("json") {
        println!(
            "{}",
            format_version_json(sirio_control::VERSION, app_version.as_deref())
        );
    } else {
        for line in format_version_lines(sirio_control::VERSION, app_version.as_deref()) {
            println!("{line}");
        }
    }
    Ok(())
}

fn cmd_quit(socket: PathBuf, _parsed: &ParsedArgs) -> Result<(), String> {
    let _ = require_ok(socket, &sirio_control::protocol::request::system_quit());
    Ok(())
}

fn cmd_capabilities(socket: PathBuf, parsed: &ParsedArgs) -> Result<(), String> {
    let response = require_ok(
        socket,
        &sirio_control::protocol::request::system_capabilities(),
    );
    print_rows(&response, "methods", &["method"], parsed.flag("json"));
    Ok(())
}

fn cmd_identify(
    socket: PathBuf,
    parsed: &ParsedArgs,
    environment: &BTreeMap<String, String>,
) -> Result<(), String> {
    // A shell opened before the rebrand still carries the old names.
    let worktree = environment
        .get("SIRIO_WORKTREE_ID")
        .or_else(|| environment.get("TILLER_WORKTREE_ID"))
        .map(String::as_str);
    let pane = environment
        .get("SIRIO_PANE_ID")
        .or_else(|| environment.get("TILLER_PANE_ID"))
        .map(String::as_str);
    let response = require_ok(
        socket,
        &sirio_control::protocol::request::system_identify(worktree, pane),
    );
    print_result(
        &response,
        &["project", "branch", "path", "workspaceId", "surfaceId"],
        parsed.flag("json"),
    );
    Ok(())
}

fn cmd_project(socket: PathBuf, parsed: &ParsedArgs) -> Result<(), String> {
    let action = parsed
        .positional
        .first()
        .ok_or_else(|| "Missing project action".to_string())?;
    match action.as_str() {
        "list" => {
            let response = require_ok(socket, &sirio_control::protocol::request::project_list());
            print_rows(
                &response,
                "projects",
                &["id", "name", "path", "isGit", "worktreeCount", "empty"],
                parsed.flag("json"),
            );
            Ok(())
        }
        "add" => {
            let path = parsed
                .value("path")
                .or_else(|| parsed.positional.get(1).map(String::as_str))
                .filter(|path| !path.is_empty())
                .ok_or_else(|| "Missing project path".to_string())?;
            let response = require_ok(socket, &sirio_control::protocol::request::project_add(path));
            print_result(
                &response,
                &["added", "projectId", "worktreeCount"],
                parsed.flag("json"),
            );
            Ok(())
        }
        other => Err(format!("unknown project action '{other}'")),
    }
}

fn cmd_list_workspaces(socket: PathBuf, parsed: &ParsedArgs) -> Result<(), String> {
    let response = require_ok(socket, &sirio_control::protocol::request::workspace_list());
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
        &sirio_control::protocol::request::workspace_create(&project, branch.as_deref()),
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
        &sirio_control::protocol::request::workspace_select(&workspace),
    );
    Ok(())
}

fn cmd_current_workspace(socket: PathBuf, parsed: &ParsedArgs) -> Result<(), String> {
    let response = require_ok(
        socket,
        &sirio_control::protocol::request::workspace_current(),
    );
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
        &sirio_control::protocol::request::workspace_close(&workspace),
    );
    Ok(())
}

fn cmd_worktree_set(socket: PathBuf, parsed: &ParsedArgs) -> Result<(), String> {
    let worktree = parsed.require("worktree")?;
    let response = require_ok(
        socket,
        &sirio_control::protocol::request::worktree_set(
            &worktree,
            parsed.value("comment"),
            parsed.value("session"),
        ),
    );
    print_result(
        &response,
        &["id", "path", "comment", "session"],
        parsed.flag("json"),
    );
    Ok(())
}

fn cmd_restore_session(socket: PathBuf, _parsed: &ParsedArgs) -> Result<(), String> {
    let _ = require_ok(socket, &sirio_control::protocol::request::session_restore());
    Ok(())
}

fn cmd_pane(socket: PathBuf, parsed: &ParsedArgs) -> Result<(), String> {
    let action = parsed
        .positional
        .first()
        .ok_or_else(|| "Missing pane action".to_string())?;
    match action.as_str() {
        "split" => {
            let direction = parsed
                .value("direction")
                .or_else(|| parsed.positional.get(1).map(String::as_str))
                .ok_or_else(|| "Missing split direction".to_string())?;
            let _ = require_ok(
                socket,
                &sirio_control::protocol::request::pane_split(direction),
            );
            Ok(())
        }
        "focus" => {
            let direction = parsed
                .value("direction")
                .or_else(|| parsed.positional.get(1).map(String::as_str))
                .ok_or_else(|| "Missing focus direction".to_string())?;
            let _ = require_ok(
                socket,
                &sirio_control::protocol::request::pane_focus(direction),
            );
            Ok(())
        }
        "close" => {
            let _ = require_ok(socket, &sirio_control::protocol::request::pane_close());
            Ok(())
        }
        other => Err(format!("unknown pane action '{other}'")),
    }
}

fn cmd_tab(socket: PathBuf, parsed: &ParsedArgs) -> Result<(), String> {
    let action = parsed
        .positional
        .first()
        .ok_or_else(|| "Missing tab action".to_string())?;
    match action.as_str() {
        "cycle" => {
            let direction = parsed
                .value("direction")
                .or_else(|| parsed.positional.get(1).map(String::as_str))
                .unwrap_or("forward");
            let forward = match direction {
                "forward" | "next" => true,
                "backward" | "previous" => false,
                other => return Err(format!("unknown tab cycle direction '{other}'")),
            };
            let _ = require_ok(
                socket,
                &sirio_control::protocol::request::tab_cycle(forward),
            );
            Ok(())
        }
        "select" => {
            let position = parsed
                .value("index")
                .or_else(|| parsed.positional.get(1).map(String::as_str))
                .ok_or_else(|| "Missing tab index".to_string())?
                .parse::<usize>()
                .map_err(|_| "Tab index must be a positive integer".to_string())?;
            if position == 0 {
                return Err("Tab index must be a positive integer".to_string());
            }
            let _ = require_ok(
                socket,
                &sirio_control::protocol::request::tab_select(position),
            );
            Ok(())
        }
        other => Err(format!("unknown tab action '{other}'")),
    }
}

fn cmd_panel(socket: PathBuf, parsed: &ParsedArgs) -> Result<(), String> {
    let action = parsed
        .positional
        .first()
        .ok_or_else(|| "Missing panel action".to_string())?;
    match action.as_str() {
        "create" => {
            let response = require_ok(
                socket,
                &sirio_control::protocol::request::panel_create(
                    parsed.value("worktree"),
                    parsed.value("cmd"),
                ),
            );
            print_result(&response, &["id"], parsed.flag("json"));
            Ok(())
        }
        "split" => {
            let direction = parsed
                .value("direction")
                .or_else(|| parsed.positional.get(1).map(String::as_str))
                .ok_or_else(|| "Missing split direction".to_string())?;
            let from = parsed.require("from")?;
            let response = require_ok(
                socket,
                &sirio_control::protocol::request::panel_split(
                    &from,
                    direction,
                    parsed.value("cmd"),
                ),
            );
            print_result(&response, &["id"], parsed.flag("json"));
            Ok(())
        }
        "list" => {
            let response = require_ok(
                socket,
                &sirio_control::protocol::request::panel_list(parsed.value("worktree")),
            );
            print_rows(
                &response,
                "panels",
                &["id", "tab", "title", "agent", "active"],
                parsed.flag("json"),
            );
            Ok(())
        }
        "write" => {
            let id = panel_id(parsed)?;
            let mut input = parsed.require("input")?;
            if parsed.flag("enter") {
                input.push('\r');
            }
            let _ = require_ok(
                socket,
                &sirio_control::protocol::request::panel_write(&id, &input),
            );
            Ok(())
        }
        "key" => {
            let id = panel_id(parsed)?;
            let key = parsed.require("key")?;
            let _ = require_ok(
                socket,
                &sirio_control::protocol::request::panel_key(&id, &key),
            );
            Ok(())
        }
        "read" => {
            let id = panel_id(parsed)?;
            let response = require_ok(socket, &sirio_control::protocol::request::panel_read(&id));
            print_result(&response, &["data"], parsed.flag("json"));
            Ok(())
        }
        "state" => {
            let id = panel_id(parsed)?;
            let response = require_ok(socket, &sirio_control::protocol::request::panel_state(&id));
            print_result(
                &response,
                &[
                    "workingDirectory",
                    "exitStatus",
                    "exitCode",
                    "scrollbackBytes",
                ],
                parsed.flag("json"),
            );
            Ok(())
        }
        "scrollback" => {
            let id = panel_id(parsed)?;
            let max_bytes = parsed
                .value("max-bytes")
                .map(|value| {
                    value
                        .parse::<usize>()
                        .map_err(|_| "--max-bytes must be a nonnegative integer".to_string())
                })
                .transpose()?;
            let response = require_ok(
                socket,
                &sirio_control::protocol::request::panel_scrollback(&id, max_bytes),
            );
            print_result(&response, &["data", "bytes"], parsed.flag("json"));
            Ok(())
        }
        "wait" => {
            let id = panel_id(parsed)?;
            let timeout_ms = parsed
                .value("timeout-ms")
                .map(|value| {
                    value
                        .parse::<u64>()
                        .map_err(|_| "--timeout-ms must be a nonnegative integer".to_string())
                })
                .transpose()?;
            let response = require_ok(
                socket,
                &sirio_control::protocol::request::panel_wait(&id, timeout_ms),
            );
            print_result(&response, &["exitCode"], parsed.flag("json"));
            Ok(())
        }
        "focus" => {
            let id = panel_id(parsed)?;
            let _ = require_ok(socket, &sirio_control::protocol::request::panel_focus(&id));
            Ok(())
        }
        "close" => {
            let id = panel_id(parsed)?;
            let _ = require_ok(socket, &sirio_control::protocol::request::panel_close(&id));
            Ok(())
        }
        other => Err(format!("unknown panel action '{other}'")),
    }
}

fn cmd_surface(socket: PathBuf, parsed: &ParsedArgs) -> Result<(), String> {
    let surface = parsed
        .positional
        .first()
        .ok_or_else(|| "Missing surface name".to_string())?;
    let action = parsed
        .positional
        .get(1)
        .ok_or_else(|| "Missing surface action".to_string())?;
    let response = match (surface.as_str(), action.as_str()) {
        ("changes", "open") => require_ok(
            socket,
            &sirio_control::protocol::request::changes_open(parsed.value("worktree")),
        ),
        ("changes", "read") => {
            require_ok(socket, &sirio_control::protocol::request::changes_read())
        }
        ("changes", "stage") => {
            let path = parsed
                .positional
                .get(2)
                .filter(|path| !path.is_empty())
                .ok_or_else(|| "Missing changes path".to_string())?;
            require_ok(
                socket,
                &sirio_control::protocol::request::changes_stage(path, parsed.value("worktree")),
            )
        }
        ("changes", "unstage") => {
            let path = parsed
                .positional
                .get(2)
                .filter(|path| !path.is_empty())
                .ok_or_else(|| "Missing changes path".to_string())?;
            require_ok(
                socket,
                &sirio_control::protocol::request::changes_unstage(path, parsed.value("worktree")),
            )
        }
        ("changes", "discard") => {
            let path = parsed
                .positional
                .get(2)
                .filter(|path| !path.is_empty())
                .ok_or_else(|| "Missing changes path".to_string())?;
            require_ok(
                socket,
                &sirio_control::protocol::request::changes_discard(path, parsed.value("worktree")),
            )
        }
        ("changes", "stage-all") => require_ok(
            socket,
            &sirio_control::protocol::request::changes_stage_all(parsed.value("worktree")),
        ),
        ("changes", "discard-all") => require_ok(
            socket,
            &sirio_control::protocol::request::changes_discard_all(parsed.value("worktree")),
        ),
        ("settings", "open") => require_ok(
            socket,
            &sirio_control::protocol::request::settings_open(parsed.value("section")),
        ),
        ("settings", "select") => {
            let section = parsed
                .value("section")
                .or_else(|| parsed.positional.get(2).map(String::as_str))
                .ok_or_else(|| "Missing settings section".to_string())?;
            require_ok(
                socket,
                &sirio_control::protocol::request::settings_select(section),
            )
        }
        ("settings", "read") => {
            require_ok(socket, &sirio_control::protocol::request::settings_read())
        }
        _ => return Err(format!("unknown surface action '{surface} {action}'")),
    };
    print_result(
        &response,
        &[
            "surfaceId",
            "section",
            "worktree",
            "stagedCount",
            "changedCount",
            "untrackedCount",
            "ready",
        ],
        parsed.flag("json"),
    );
    Ok(())
}

fn panel_id(parsed: &ParsedArgs) -> Result<String, String> {
    parsed
        .value("id")
        .or_else(|| parsed.positional.get(1).map(String::as_str))
        .filter(|id| !id.is_empty())
        .map(str::to_string)
        .ok_or_else(|| "Missing required panel id (use '<id>' or '--id <id>)".to_string())
}

fn cmd_list_notifications(socket: PathBuf, parsed: &ParsedArgs) -> Result<(), String> {
    let response = require_ok(
        socket,
        &sirio_control::protocol::request::notification_list(),
    );
    print_rows(
        &response,
        "notifications",
        &["date", "title", "subtitle", "body"],
        parsed.flag("json"),
    );
    Ok(())
}

fn cmd_clear_notifications(socket: PathBuf, _parsed: &ParsedArgs) -> Result<(), String> {
    let _ = require_ok(
        socket,
        &sirio_control::protocol::request::notification_clear(),
    );
    Ok(())
}

/// Which pane an agent-status command (`notify`, `session-ref`) targets,
/// and how the pane was named.
#[derive(Debug)]
struct AgentSessionTarget {
    session: String,
    /// True when the pane came from the environment rather than
    /// `--session`. A hook Sirio writes into a worktree names its pane
    /// outright; a hook installed user-globally (Settings → Install Hooks)
    /// cannot, so it relies on the `SIRIO_PANE_ID` every Sirio pane exports
    /// — and such a hook also fires in agents Sirio never launched, where
    /// there is nothing to report to and nothing worth saying about it.
    implicit: bool,
}

/// Resolves the pane for an agent-status command: `--session` when given
/// (empty is still the caller's mistake), else the pane environment under
/// its current or pre-rebrand name, else `None` — outside a Sirio pane.
fn agent_session_target(
    parsed: &ParsedArgs,
    environment: &BTreeMap<String, String>,
) -> Result<Option<AgentSessionTarget>, String> {
    if parsed.value("session").is_some() {
        return Ok(Some(AgentSessionTarget {
            session: parsed.require("session")?,
            implicit: false,
        }));
    }
    let session = environment
        .get("SIRIO_PANE_ID")
        .or_else(|| environment.get("TILLER_PANE_ID"))
        .filter(|value| !value.is_empty())
        .cloned();
    Ok(session.map(|session| AgentSessionTarget {
        session,
        implicit: true,
    }))
}

/// Sends an agent-status request for `target`. An explicit pane keeps the
/// strict contract (a refused or unreachable socket is an error the hook
/// author sees); an implicit one is best effort — Sirio may have quit while
/// the agent kept running, and a user-global hook must never turn that into
/// noise inside the agent.
fn send_agent_status(socket: PathBuf, target: &AgentSessionTarget, request: &ControlRequest) {
    if target.implicit {
        let _ = round_trip(&socket, request, Duration::from_secs(5));
    } else {
        let _ = require_ok(socket, request);
    }
}

fn cmd_session_ref(
    socket: PathBuf,
    parsed: &ParsedArgs,
    environment: &BTreeMap<String, String>,
) -> Result<(), String> {
    let Some(target) = agent_session_target(parsed, environment)? else {
        return Ok(());
    };
    let ref_value = parsed.require("ref")?;
    send_agent_status(
        socket,
        &target,
        &sirio_control::protocol::request::session_ref(&target.session, &ref_value),
    );
    Ok(())
}

/// The notify command: user notification (--title) or agent-status update
/// (--session/--status), mirroring the Swift CLI's two modes.
fn cmd_notify(
    socket: PathBuf,
    parsed: &ParsedArgs,
    environment: &BTreeMap<String, String>,
) -> Result<(), String> {
    let has_agent_mode_options = parsed.value("session").is_some()
        || parsed.value("status").is_some()
        || parsed.value("agent-session").is_some()
        || parsed.flag("stdin-json");
    if parsed.value("title").is_some() && has_agent_mode_options {
        return Err(
            "ambiguous notify modes: choose either agent status (--session/--status) or user notification (--title/--body)"
                .to_string(),
        );
    }
    if let Some(title) = parsed.value("title") {
        let subtitle = parsed.value("subtitle").map(str::to_string);
        let body = parsed.value("body").unwrap_or("").to_string();
        let _ = require_ok(
            socket,
            &sirio_control::protocol::request::notification_create(
                title,
                subtitle.as_deref(),
                &body,
            ),
        );
        return Ok(());
    }

    let Some(target) = agent_session_target(parsed, environment)? else {
        return Ok(());
    };
    let status = parsed.require("status")?;

    let mut ref_value = parsed.value("agent-session").map(str::to_string);
    if ref_value.is_none() && parsed.flag("stdin-json") {
        let data = read_bounded_stdin();
        ref_value = session_ref_from_json(&data);
    }
    if ref_value.is_none() {
        ref_value = session_ref_from_payload_arguments(&parsed.positional);
    }

    send_agent_status(
        socket,
        &target,
        &sirio_control::protocol::request::notify(&target.session, &status, ref_value.as_deref()),
    );
    Ok(())
}

/// Reads stdin until EOF or 5 seconds, whichever comes first — the Swift CLI
/// bounds this read because a hung hook would otherwise wedge sirioctl
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_command_succeeds_without_a_running_app() {
        let parsed = ParsedArgs {
            values: BTreeMap::new(),
            flags: std::collections::BTreeSet::new(),
            positional: Vec::new(),
        };
        let socket =
            std::env::temp_dir().join(format!("sirioctl-version-no-socket-{}", std::process::id()));

        assert!(cmd_version(socket, &parsed).is_ok());
    }

    fn parsed(values: &[(&str, &str)]) -> ParsedArgs {
        ParsedArgs {
            values: values
                .iter()
                .map(|(key, value)| (key.to_string(), value.to_string()))
                .collect(),
            flags: std::collections::BTreeSet::new(),
            positional: Vec::new(),
        }
    }

    fn environment(values: &[(&str, &str)]) -> BTreeMap<String, String> {
        values
            .iter()
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect()
    }

    /// A worktree-local hook names its pane outright; `--session` must keep
    /// winning over whatever pane the environment happens to carry.
    #[test]
    fn agent_session_prefers_the_explicit_option() {
        let target = agent_session_target(
            &parsed(&[("session", "pane-9")]),
            &environment(&[("SIRIO_PANE_ID", "pane-1")]),
        )
        .expect("explicit session parses")
        .expect("explicit session is a target");
        assert_eq!(target.session, "pane-9");
        assert!(!target.implicit);
    }

    /// A user-global hook (Settings → Install Hooks) cannot embed a pane id:
    /// it relies on the `SIRIO_PANE_ID` every Sirio pane exports, under its
    /// pre-rebrand name too.
    #[test]
    fn agent_session_falls_back_to_the_pane_environment() {
        let target = agent_session_target(
            &parsed(&[("status", "running")]),
            &environment(&[("SIRIO_PANE_ID", "pane-1")]),
        )
        .expect("environment session parses")
        .expect("environment session is a target");
        assert_eq!(target.session, "pane-1");
        assert!(target.implicit);

        let legacy = agent_session_target(
            &parsed(&[("status", "running")]),
            &environment(&[("TILLER_PANE_ID", "pane-2")]),
        )
        .expect("legacy environment session parses")
        .expect("legacy environment session is a target");
        assert_eq!(legacy.session, "pane-2");
        assert!(legacy.implicit);
    }

    /// Outside a Sirio pane a user-global hook has nothing to report to —
    /// that is `None`, not an error, so the hook stays silent instead of
    /// surfacing "Missing required option" in an agent that Sirio never
    /// launched.
    #[test]
    fn agent_session_is_absent_outside_a_sirio_pane() {
        let target = agent_session_target(&parsed(&[("status", "running")]), &environment(&[]))
            .expect("an absent session is not an error");
        assert!(target.is_none());
    }

    /// An explicit but empty `--session` is still the caller's mistake.
    #[test]
    fn agent_session_rejects_an_empty_explicit_option() {
        let error = agent_session_target(
            &parsed(&[("session", "")]),
            &environment(&[("SIRIO_PANE_ID", "pane-1")]),
        )
        .expect_err("an empty explicit session is rejected");
        assert!(error.contains("--session"), "got: {error}");
    }
}
