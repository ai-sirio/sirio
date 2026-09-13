//! `sirioctl` — control a running Sirio over its unix socket.
//!
//! Ported from `sirioctl/Sirioctl.swift` and `sirioctl/CmuxCommands.swift`
//! with the same subcommand names, options, output columns and error text.
//! Argument parsing is hand-rolled (`--key value`, `--flag`, positional
//! extras) to keep the crate dependency-free; the wire protocol is the
//! parity-critical part.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::exit;
use std::time::{Duration, Instant};

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
        "browser" => cmd_browser(socket, &parsed, &environment),
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
         \x20 browser open <url> [--id-format uuids|both] [--json]\n\
         \x20 browser navigate <surface> <back|forward|reload>\n\
         \x20 browser get <surface> <url|text|html> [--selector s] [--json]\n\
         \x20 browser screenshot <surface> [--path f]\n\
         \x20 browser snapshot <surface> [--json]\n\
         \x20 browser act <surface> <click|fill|type|press|scroll> [--ref r|--selector s]\n\
         \x20   [--value v] [--key k] [--generation g] [--delta-x n] [--delta-y n] [--snapshot-after]\n\
         \x20 browser wait <surface> (--selector|--text|--url-contains|--load-state|--function) --timeout-ms n\n\
         \x20 browser eval <surface> <script> [--json]\n\
         \x20 browser console <surface> [--since ts]\n\
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

// ---------------------------------------------------------------------------
// browser.* (#458)
// ---------------------------------------------------------------------------
//
// One CLI verb per documented browser-surface verb, over the `browser.*`
// control-socket methods the app already dispatches. The running app answers
// the Rust-era wire: `browser.open` returns a surface id, `browser.get`
// returns status fields, and page scripts run through `browser.eval`. Three
// pieces of the documented V1 contract are wider than that wire, and the CLI
// fills them in itself instead of inventing a second transport:
//
//   * `get text|html` and the content conditions of `wait` run page scripts
//     through `browser.eval`;
//   * `snapshot` assigns `e1..eN` refs page-side and keeps the live elements
//     in `window.__sirioRefs`, so `act --ref` has a stable target and a reload
//     -- which wipes page state -- is exactly what makes a ref `stale_ref`;
//   * `navigate reload` re-navigates to the surface's current URL, because
//     the app's Linux path refuses its own `action` parameter.
//
// Every request also carries the documented `surface`/`workspace` keys, so an
// app that implements the full protocol directly takes precedence over the
// fallbacks wherever it answers.

const BROWSER_POLL: Duration = Duration::from_millis(100);
const BROWSER_TITLE_WAIT: Duration = Duration::from_secs(5);

/// The worktree browser commands target: `$SIRIO_WORKTREE_ID`, or the
/// pre-rebrand name for shells started before the rename.
fn browser_workspace(environment: &BTreeMap<String, String>) -> Option<String> {
    environment
        .get("SIRIO_WORKTREE_ID")
        .or_else(|| environment.get("TILLER_WORKTREE_ID"))
        .filter(|value| !value.is_empty())
        .cloned()
}

/// Params every browser method shares. `surfaceId` rides along with `surface`
/// because the app's other surface methods spell it that way; a dispatch that
/// does not know a key ignores it.
fn browser_params(surface: Option<&str>, workspace: Option<&str>) -> BTreeMap<String, String> {
    let mut params = BTreeMap::new();
    if let Some(surface) = surface {
        params.insert("surface".to_string(), surface.to_string());
        params.insert("surfaceId".to_string(), surface.to_string());
    }
    if let Some(workspace) = workspace {
        params.insert("workspace".to_string(), workspace.to_string());
    }
    params
}

/// Sends one `browser.*` request. A transport failure keeps the canonical
/// "Sirio is not running" exit; an app-level refusal is returned so callers can
/// fall back to the page-script route.
fn browser_call(
    socket: &Path,
    method: &str,
    params: BTreeMap<String, String>,
) -> Result<BTreeMap<String, String>, String> {
    let request = sirio_control::protocol::request::browser(method, params);
    let response = round_trip_or_die(socket.to_path_buf(), &request);
    match (response.ok, response.result) {
        (true, result) => Ok(result.unwrap_or_default()),
        (false, _) => Err(response.error.unwrap_or_else(|| method.to_string())),
    }
}

/// A JS string literal for embedding a caller-supplied value in a page script.
fn browser_js_literal(value: &str) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "\"\"".to_string())
}

/// Page results arrive either as the JS value itself or, on engines that
/// serialize the script result, as a JSON string containing it.
fn decode_js_string(raw: &str) -> String {
    if raw.len() >= 2
        && raw.starts_with('"')
        && raw.ends_with('"')
        && let Ok(decoded) = serde_json::from_str::<String>(raw)
    {
        return decoded;
    }
    raw.to_string()
}

/// Runs a page script through `browser.eval` and returns its result.
fn browser_eval(
    socket: &Path,
    surface: &str,
    workspace: Option<&str>,
    script: &str,
) -> Result<String, String> {
    let mut params = browser_params(Some(surface), workspace);
    params.insert("script".to_string(), script.to_string());
    let result = browser_call(socket, "browser.eval", params)?;
    let raw = result
        .get("value")
        .or_else(|| result.get("result"))
        .cloned()
        .unwrap_or_default();
    Ok(decode_js_string(&raw))
}

/// Assigns `e1..eN` refs to the page's interactive/labelled elements and keeps
/// the live elements in `window.__sirioRefs`. The generation lives in the page
/// too, so it disappears with the refs on navigation -- which is what makes a
/// stale ref detectable without any CLI-side state file.
const BROWSER_SNAPSHOT_SCRIPT: &str = r#"JSON.stringify((() => {
    const generation = String((Number(window.__sirioGeneration) || 0) + 1);
    const refs = {};
    const nodes = [];
    document.querySelectorAll(
        "a,button,input,select,textarea,[role],[aria-label],h1,h2,h3"
    ).forEach((el) => {
        const rect = el.getBoundingClientRect();
        if (rect.width === 0 && rect.height === 0) return;
        const ref = "e" + (nodes.length + 1);
        refs[ref] = el;
        const node = {
            ref: ref,
            role: el.getAttribute("role") || el.tagName.toLowerCase(),
            name: (el.getAttribute("aria-label") || el.innerText || el.value || "")
                .trim()
                .slice(0, 200),
            box: {
                x: Math.round(rect.x),
                y: Math.round(rect.y),
                width: Math.round(rect.width),
                height: Math.round(rect.height),
            },
        };
        if (typeof el.value === "string" && el.value !== "") node.value = el.value;
        nodes.push(node);
    });
    window.__sirioRefs = refs;
    window.__sirioGeneration = generation;
    return { generation: generation, nodes: nodes };
})())"#;

/// The JS action one `browser.act` verb performs on `el`, reusing the same
/// value/key/delta names the socket method documents.
fn browser_act_js(
    verb: &str,
    value: Option<&str>,
    key: Option<&str>,
    delta_x: Option<&str>,
    delta_y: Option<&str>,
) -> Result<String, String> {
    let coordinate = |raw: Option<&str>| -> Result<String, String> {
        match raw {
            Some(raw) => raw
                .parse::<f64>()
                .map(|number| number.to_string())
                .map_err(|_| format!("--delta must be a number (got '{raw}')")),
            None => Ok("0".to_string()),
        }
    };
    match verb {
        "click" => Ok("el.click();".to_string()),
        "fill" | "type" => Ok(format!(
            "el.value = {}; \
             el.dispatchEvent(new Event(\"input\", {{ bubbles: true }})); \
             el.dispatchEvent(new Event(\"change\", {{ bubbles: true }}));",
            browser_js_literal(value.unwrap_or_default())
        )),
        "press" => {
            let key = key.ok_or_else(|| "browser.act press requires --key".to_string())?;
            let key = browser_js_literal(key);
            Ok(format!(
                "el.dispatchEvent(new KeyboardEvent(\"keydown\", {{ key: {key}, bubbles: true }})); \
                 el.dispatchEvent(new KeyboardEvent(\"keyup\", {{ key: {key}, bubbles: true }}));"
            ))
        }
        "scroll" => {
            let (dx, dy) = (coordinate(delta_x)?, coordinate(delta_y)?);
            Ok(format!(
                "el.scrollIntoView({{ block: \"center\" }}); window.scrollBy({dx}, {dy});"
            ))
        }
        other => Err(format!(
            "unknown browser act verb '{other}' (known verbs: click, fill, type, press, scroll)"
        )),
    }
}

/// Resolves a snapshot ref page-side, refusing it when the page has moved on
/// (a reload leaves neither `window.__sirioRefs` nor the generation in place).
fn browser_act_ref_script(
    verb: &str,
    reference: &str,
    generation: &str,
    value: Option<&str>,
    key: Option<&str>,
    delta_x: Option<&str>,
    delta_y: Option<&str>,
) -> Result<String, String> {
    let action = browser_act_js(verb, value, key, delta_x, delta_y)?;
    Ok(format!(
        "JSON.stringify((() => {{ \
         const generation = {generation}; \
         if (String(window.__sirioGeneration || \"\") !== generation) \
             return {{ error: \"stale_ref\" }}; \
         const el = (window.__sirioRefs || {{}})[{reference}]; \
         if (!el || !el.isConnected) return {{ error: \"stale_ref\" }}; \
         {action} \
         return {{ ok: true, generation: generation }}; \
         }})())",
        generation = browser_js_literal(generation),
        reference = browser_js_literal(reference),
    ))
}

/// The `surface:N` short handle the skill documents. Derived from the surface
/// id so repeated calls agree; the CLI never maps it back, every request
/// forwards the handle verbatim.
fn short_surface_ref(surface: &str) -> String {
    let hash = surface.bytes().fold(0u32, |acc, byte| {
        acc.wrapping_mul(31).wrapping_add(byte as u32)
    });
    format!("surface:{}", hash % 1000)
}

fn print_browser_row(row: &BTreeMap<String, String>, columns: &[&str], as_json: bool) {
    if as_json {
        println!("{}", rows::encode(std::slice::from_ref(row)));
        return;
    }
    let line: Vec<String> = columns
        .iter()
        .map(|column| row.get(*column).cloned().unwrap_or_default())
        .collect();
    println!("{}", line.join("\t"));
}

fn browser_surface(parsed: &ParsedArgs) -> Result<String, String> {
    parsed
        .positional
        .get(1)
        .filter(|surface| !surface.is_empty())
        .cloned()
        .ok_or_else(|| "Missing browser surface".to_string())
}

fn cmd_browser(
    socket: PathBuf,
    parsed: &ParsedArgs,
    environment: &BTreeMap<String, String>,
) -> Result<(), String> {
    let action = parsed
        .positional
        .first()
        .map(String::as_str)
        .ok_or_else(|| "Missing browser action".to_string())?;
    let workspace = browser_workspace(environment);
    match action {
        "open" => browser_open(&socket, parsed, workspace.as_deref()),
        "navigate" => browser_navigate(&socket, parsed, workspace.as_deref()),
        "get" => browser_get(&socket, parsed, workspace.as_deref()),
        "screenshot" => browser_screenshot(&socket, parsed, workspace.as_deref()),
        "snapshot" => browser_snapshot_command(&socket, parsed, workspace.as_deref()),
        "act" => browser_act(&socket, parsed, workspace.as_deref()),
        "wait" => browser_wait(&socket, parsed, workspace.as_deref()),
        "eval" => browser_eval_command(&socket, parsed, workspace.as_deref()),
        "console" => browser_console(&socket, parsed, workspace.as_deref()),
        "errors" => Err("not_supported: browser.errors is not available in Sirio V1".to_string()),
        other => Err(format!("unknown browser action '{other}'")),
    }
}

/// Polls the page title after `browser.open`, because the app's open answer is
/// produced before the page has finished loading on this engine.
fn browser_wait_for_title(
    socket: &Path,
    surface: &str,
    workspace: Option<&str>,
    deadline: Instant,
) -> String {
    loop {
        if let Ok(title) = browser_eval(socket, surface, workspace, "document.title")
            && !title.is_empty()
        {
            return title;
        }
        if Instant::now() >= deadline {
            return String::new();
        }
        std::thread::sleep(BROWSER_POLL);
    }
}

fn browser_open(socket: &Path, parsed: &ParsedArgs, workspace: Option<&str>) -> Result<(), String> {
    let url = parsed
        .positional
        .get(1)
        .filter(|url| !url.is_empty())
        .cloned()
        .ok_or_else(|| "Missing open URL".to_string())?;
    let id_format = match parsed.value("id-format").unwrap_or("uuids") {
        "" | "uuids" => "uuids",
        "both" => "both",
        other => return Err(format!("--id-format must be uuids or both (got '{other}')")),
    };
    let mut params = browser_params(None, workspace);
    params.insert("url".to_string(), url.clone());
    params.insert("id-format".to_string(), id_format.to_string());
    if let Some(window) = parsed.value("window") {
        params.insert("window".to_string(), window.to_string());
    }
    let result = browser_call(socket, "browser.open", params)?;

    let surface = result.get("surface").cloned().unwrap_or_default();
    if surface.is_empty() {
        return Err("browser.open returned no surface".to_string());
    }
    let mut title = result.get("title").cloned().unwrap_or_default();
    if title.is_empty() {
        title = browser_wait_for_title(
            socket,
            &surface,
            workspace,
            Instant::now() + BROWSER_TITLE_WAIT,
        );
    }
    let mut row = BTreeMap::new();
    row.insert("surface".to_string(), surface.clone());
    row.insert(
        "surfaceRef".to_string(),
        result.get("surfaceRef").cloned().unwrap_or_else(|| {
            if id_format == "both" {
                short_surface_ref(&surface)
            } else {
                String::new()
            }
        }),
    );
    row.insert("url".to_string(), result.get("url").cloned().unwrap_or(url));
    row.insert("title".to_string(), title);
    print_browser_row(
        &row,
        &["surface", "surfaceRef", "url", "title"],
        parsed.flag("json"),
    );
    Ok(())
}

fn print_browser_navigation(result: &BTreeMap<String, String>) {
    let mut row = BTreeMap::new();
    row.insert(
        "url".to_string(),
        result
            .get("url")
            .or_else(|| result.get("value"))
            .cloned()
            .unwrap_or_default(),
    );
    row.insert(
        "title".to_string(),
        result.get("title").cloned().unwrap_or_default(),
    );
    print_browser_row(&row, &["url", "title"], false);
}

fn browser_navigate(
    socket: &Path,
    parsed: &ParsedArgs,
    workspace: Option<&str>,
) -> Result<(), String> {
    let surface = browser_surface(parsed)?;
    let action = parsed
        .positional
        .get(2)
        .map(String::as_str)
        .ok_or_else(|| "Missing navigation action (back, forward or reload)".to_string())?;
    if !matches!(action, "back" | "forward" | "reload") {
        return Err(format!(
            "unknown navigation action '{action}' (use back, forward or reload)"
        ));
    }

    // The documented wire first: an app that answers it with a URL owns the
    // navigation.
    let mut params = browser_params(Some(&surface), workspace);
    params.insert("action".to_string(), action.to_string());
    if let Ok(result) = browser_call(socket, "browser.navigate", params)
        && (result.contains_key("url") || result.contains_key("value"))
    {
        print_browser_navigation(&result);
        return Ok(());
    }

    match action {
        "reload" => {
            // The app's Linux path refuses `action`; reload is a navigation to
            // the URL the surface already shows.
            let state = browser_call(
                socket,
                "browser.get",
                browser_params(Some(&surface), workspace),
            )?;
            let url = state
                .get("url")
                .or_else(|| state.get("value"))
                .filter(|url| !url.is_empty())
                .cloned()
                .ok_or_else(|| "browser.navigate reload: no current url".to_string())?;
            let mut params = browser_params(Some(&surface), workspace);
            params.insert("url".to_string(), url);
            let result = browser_call(socket, "browser.navigate", params)?;
            print_browser_navigation(&result);
        }
        direction => {
            let state = browser_call(
                socket,
                "browser.get",
                browser_params(Some(&surface), workspace),
            )?;
            let flag = if direction == "back" {
                "canGoBack"
            } else {
                "canGoForward"
            };
            if state.get(flag).map(String::as_str) == Some("false") {
                return Err(format!(
                    "navigation_unavailable: no history to go {direction}"
                ));
            }
            browser_eval(
                socket,
                &surface,
                workspace,
                &format!("history.{direction}()"),
            )?;
            std::thread::sleep(Duration::from_millis(250));
            let result = browser_call(
                socket,
                "browser.get",
                browser_params(Some(&surface), workspace),
            )?;
            print_browser_navigation(&result);
        }
    }
    Ok(())
}

/// `get text|html` as a page script, for engines that only answer the `url`
/// field.
fn browser_content_script(what: &str, selector: &str) -> String {
    let target = if selector.is_empty() {
        "document.body".to_string()
    } else {
        format!("document.querySelector({})", browser_js_literal(selector))
    };
    let property = if what == "text" {
        "innerText"
    } else {
        "outerHTML"
    };
    format!(
        "JSON.stringify((() => {{ const el = {target}; return el ? el.{property} : \"\"; }})())"
    )
}

fn browser_get(socket: &Path, parsed: &ParsedArgs, workspace: Option<&str>) -> Result<(), String> {
    let surface = browser_surface(parsed)?;
    let what = parsed
        .positional
        .get(2)
        .map(String::as_str)
        .ok_or_else(|| "Missing browser get value (url, text or html)".to_string())?;
    if !matches!(what, "url" | "text" | "html") {
        return Err(format!(
            "unknown browser get value '{what}' (use url, text or html)"
        ));
    }
    let selector = parsed.value("selector");
    let mut params = browser_params(Some(&surface), workspace);
    params.insert("what".to_string(), what.to_string());
    if let Some(selector) = selector {
        params.insert("selector".to_string(), selector.to_string());
    }
    let result = browser_call(socket, "browser.get", params)?;
    let value = match what {
        "url" => result
            .get("value")
            .or_else(|| result.get("url"))
            .cloned()
            .unwrap_or_else(|| {
                browser_eval(socket, &surface, workspace, "location.href").unwrap_or_default()
            }),
        _ => match result.get("value") {
            Some(value) => value.clone(),
            None => browser_eval(
                socket,
                &surface,
                workspace,
                &browser_content_script(what, selector.unwrap_or_default()),
            )?,
        },
    };
    let mut row = BTreeMap::new();
    row.insert("value".to_string(), value);
    print_browser_row(&row, &["value"], parsed.flag("json"));
    Ok(())
}

fn browser_screenshot(
    socket: &Path,
    parsed: &ParsedArgs,
    workspace: Option<&str>,
) -> Result<(), String> {
    let surface = browser_surface(parsed)?;
    let mut params = browser_params(Some(&surface), workspace);
    if let Some(path) = parsed.value("path") {
        params.insert("path".to_string(), path.to_string());
    }
    let result = browser_call(socket, "browser.screenshot", params)?;
    let mut row = BTreeMap::new();
    row.insert(
        "path".to_string(),
        result.get("path").cloned().unwrap_or_default(),
    );
    print_browser_row(&row, &["path"], parsed.flag("json"));
    Ok(())
}

/// The `generation` + `nodes` pair a snapshot answers with. An app that
/// implements it directly wins; otherwise the refs are assigned page-side.
fn browser_snapshot(
    socket: &Path,
    surface: &str,
    workspace: Option<&str>,
) -> Result<(String, String), String> {
    if let Ok(result) = browser_call(
        socket,
        "browser.snapshot",
        browser_params(Some(surface), workspace),
    ) && let (Some(generation), Some(nodes)) = (result.get("generation"), result.get("nodes"))
    {
        return Ok((generation.clone(), nodes.clone()));
    }
    let raw = browser_eval(socket, surface, workspace, BROWSER_SNAPSHOT_SCRIPT)?;
    let value: serde_json::Value =
        serde_json::from_str(&raw).map_err(|error| format!("browser.snapshot failed: {error}"))?;
    let generation = value
        .get("generation")
        .and_then(|value| value.as_str())
        .unwrap_or("1")
        .to_string();
    let nodes = serde_json::to_string(
        value
            .get("nodes")
            .unwrap_or(&serde_json::Value::Array(Vec::new())),
    )
    .map_err(|error| error.to_string())?;
    Ok((generation, nodes))
}

fn print_browser_snapshot(generation: &str, nodes: &str, as_json: bool) {
    let mut row = BTreeMap::new();
    row.insert("generation".to_string(), generation.to_string());
    row.insert("nodes".to_string(), nodes.to_string());
    if as_json {
        println!("{}", rows::encode(&[row]));
    } else {
        println!("{generation}");
        println!("{nodes}");
    }
}

fn browser_snapshot_command(
    socket: &Path,
    parsed: &ParsedArgs,
    workspace: Option<&str>,
) -> Result<(), String> {
    let surface = browser_surface(parsed)?;
    let (generation, nodes) = browser_snapshot(socket, &surface, workspace)?;
    print_browser_snapshot(&generation, &nodes, parsed.flag("json"));
    Ok(())
}

fn browser_act(socket: &Path, parsed: &ParsedArgs, workspace: Option<&str>) -> Result<(), String> {
    let surface = browser_surface(parsed)?;
    let verb = parsed
        .positional
        .get(2)
        .map(String::as_str)
        .ok_or_else(|| {
            "Missing browser act verb (click, fill, type, press or scroll)".to_string()
        })?;
    if !matches!(verb, "click" | "fill" | "type" | "press" | "scroll") {
        return Err(format!(
            "unknown browser act verb '{verb}' (known verbs: click, fill, type, press, scroll)"
        ));
    }
    let reference = parsed.value("ref");
    let selector = parsed.value("selector");
    let generation = parsed.value("generation");
    let value = parsed.value("value");
    let key = parsed.value("key");
    let snapshot_after = parsed.flag("snapshot-after");
    let mut row = BTreeMap::new();

    if let Some(reference) = reference {
        if selector.is_some() {
            return Err("browser.act takes either --ref or --selector, not both".to_string());
        }
        let generation =
            generation.ok_or_else(|| "browser.act --ref requires --generation".to_string())?;

        // An app that answers `browser.snapshot` with generation+nodes owns
        // those refs, so it gets the action first. An app whose wire has no
        // refs (the Rust-era one) refuses a ref act without a selector, and
        // the CLI then resolves the ref against the snapshot it took itself.
        let mut params = browser_params(Some(&surface), workspace);
        params.insert("verb".to_string(), verb.to_string());
        params.insert("ref".to_string(), reference.to_string());
        params.insert("generation".to_string(), generation.to_string());
        if let Some(value) = value {
            params.insert("value".to_string(), value.to_string());
        }
        if let Some(key) = key {
            params.insert("key".to_string(), key.to_string());
        }
        if let Some(delta_x) = parsed.value("delta-x") {
            params.insert("deltaX".to_string(), delta_x.to_string());
        }
        if let Some(delta_y) = parsed.value("delta-y") {
            params.insert("deltaY".to_string(), delta_y.to_string());
        }
        let mut app_acted = false;
        match browser_call(socket, "browser.act", params) {
            Ok(result) => {
                row.insert("ok".to_string(), "true".to_string());
                row.insert("generation".to_string(), generation.to_string());
                if let Some(nodes) = result.get("nodes") {
                    row.insert("nodes".to_string(), nodes.clone());
                }
                app_acted = true;
            }
            Err(error) if error.contains("stale_ref") => return Err(error),
            Err(_) => {}
        }
        if !app_acted {
            let script = browser_act_ref_script(
                verb,
                reference,
                generation,
                value,
                key,
                parsed.value("delta-x"),
                parsed.value("delta-y"),
            )?;
            let raw = browser_eval(socket, &surface, workspace, &script)?;
            let result: serde_json::Value = serde_json::from_str(&raw)
                .map_err(|error| format!("browser.act failed: {error}"))?;
            if let Some(error) = result.get("error").and_then(|value| value.as_str()) {
                return Err(format!("{error}: browser.act {verb} on ref {reference}"));
            }
            row.insert("ok".to_string(), "true".to_string());
            row.insert("generation".to_string(), generation.to_string());
        }
    } else {
        let selector =
            selector.ok_or_else(|| format!("browser.act {verb} requires --ref or --selector"))?;
        let mut params = browser_params(Some(&surface), workspace);
        params.insert("verb".to_string(), verb.to_string());
        params.insert("selector".to_string(), selector.to_string());
        if let Some(value) = value {
            params.insert("value".to_string(), value.to_string());
        }
        if let Some(key) = key {
            params.insert("key".to_string(), key.to_string());
        }
        if let Some(generation) = generation {
            params.insert("generation".to_string(), generation.to_string());
        }
        if let Some(delta_x) = parsed.value("delta-x") {
            params.insert("deltaX".to_string(), delta_x.to_string());
        }
        if let Some(delta_y) = parsed.value("delta-y") {
            params.insert("deltaY".to_string(), delta_y.to_string());
        }
        if snapshot_after {
            params.insert("snapshotAfter".to_string(), "true".to_string());
        }
        let result = browser_call(socket, "browser.act", params)?;
        row.insert("ok".to_string(), "true".to_string());
        if let Some(generation) = result.get("generation") {
            row.insert("generation".to_string(), generation.clone());
        }
        if let Some(nodes) = result.get("nodes") {
            row.insert("nodes".to_string(), nodes.clone());
        }
    }

    if snapshot_after {
        let (generation, nodes) = browser_snapshot(socket, &surface, workspace)?;
        row.insert("generation".to_string(), generation);
        row.insert("nodes".to_string(), nodes);
    }
    println!("{}", rows::encode(&[row]));
    Ok(())
}

/// Whether the page currently satisfies one wait condition.
fn browser_wait_met(
    socket: &Path,
    surface: &str,
    workspace: Option<&str>,
    condition: &str,
    expected: &str,
) -> Result<bool, String> {
    match condition {
        "selector" => Ok(browser_eval(
            socket,
            surface,
            workspace,
            &format!("!!document.querySelector({})", browser_js_literal(expected)),
        )? == "true"),
        "text" => Ok(browser_eval(
            socket,
            surface,
            workspace,
            &format!(
                "(document.body ? document.body.innerText : \"\").includes({})",
                browser_js_literal(expected)
            ),
        )? == "true"),
        "loadState" => Ok(browser_eval(
            socket,
            surface,
            workspace,
            &format!("document.readyState === {}", browser_js_literal(expected)),
        )? == "true"),
        "function" => {
            Ok(browser_eval(socket, surface, workspace, &format!("!!({expected})"))? == "true")
        }
        "urlContains" => {
            let result = browser_call(
                socket,
                "browser.get",
                browser_params(Some(surface), workspace),
            )?;
            Ok(result
                .get("url")
                .or_else(|| result.get("value"))
                .map(|url| url.contains(expected))
                .unwrap_or(false))
        }
        other => Err(format!("unknown wait condition '{other}'")),
    }
}

fn browser_wait(socket: &Path, parsed: &ParsedArgs, workspace: Option<&str>) -> Result<(), String> {
    let surface = browser_surface(parsed)?;
    let timeout_ms = parsed
        .require("timeout-ms")?
        .parse::<u64>()
        .map_err(|_| "--timeout-ms must be a nonnegative integer".to_string())?;
    let conditions: Vec<(&str, &str)> = [
        ("selector", parsed.value("selector")),
        ("text", parsed.value("text")),
        ("urlContains", parsed.value("url-contains")),
        ("loadState", parsed.value("load-state")),
        ("function", parsed.value("function")),
    ]
    .into_iter()
    .filter_map(|(key, value)| value.map(|value| (key, value)))
    .collect();
    if conditions.len() != 1 {
        return Err(
            "pass exactly one wait condition (--selector, --text, --url-contains, --load-state or --function)"
                .to_string(),
        );
    }
    let (condition, expected) = conditions[0];
    let started = Instant::now();

    // The documented wire first; an app that understands the condition answers
    // `ok`. The Rust-era app only waits for load state, so anything else falls
    // through to polling the page.
    let mut params = browser_params(Some(&surface), workspace);
    params.insert(condition.to_string(), expected.to_string());
    params.insert("timeoutMs".to_string(), timeout_ms.to_string());
    if let Ok(result) = browser_call(socket, "browser.wait", params)
        && result.contains_key("ok")
    {
        let mut row = BTreeMap::new();
        row.insert(
            "ok".to_string(),
            result
                .get("ok")
                .cloned()
                .unwrap_or_else(|| "true".to_string()),
        );
        row.insert(
            "elapsedMs".to_string(),
            result
                .get("elapsedMs")
                .cloned()
                .unwrap_or_else(|| started.elapsed().as_millis().to_string()),
        );
        println!("{}", rows::encode(&[row]));
        return Ok(());
    }

    let deadline = started + Duration::from_millis(timeout_ms);
    loop {
        if browser_wait_met(socket, &surface, workspace, condition, expected)? {
            let mut row = BTreeMap::new();
            row.insert("ok".to_string(), "true".to_string());
            row.insert(
                "elapsedMs".to_string(),
                started.elapsed().as_millis().to_string(),
            );
            println!("{}", rows::encode(&[row]));
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "timeout: browser.wait {condition} did not match within {timeout_ms}ms"
            ));
        }
        std::thread::sleep(BROWSER_POLL);
    }
}

fn browser_eval_command(
    socket: &Path,
    parsed: &ParsedArgs,
    workspace: Option<&str>,
) -> Result<(), String> {
    let surface = browser_surface(parsed)?;
    let script = parsed
        .positional
        .get(2)
        .filter(|script| !script.is_empty())
        .cloned()
        .ok_or_else(|| "Missing browser eval script".to_string())?;
    let mut params = browser_params(Some(&surface), workspace);
    params.insert("script".to_string(), script);
    let result = browser_call(socket, "browser.eval", params)?;
    let mut row = BTreeMap::new();
    row.insert(
        "value".to_string(),
        result
            .get("value")
            .or_else(|| result.get("result"))
            .cloned()
            .unwrap_or_default(),
    );
    print_browser_row(&row, &["value"], parsed.flag("json"));
    Ok(())
}

fn browser_console(
    socket: &Path,
    parsed: &ParsedArgs,
    workspace: Option<&str>,
) -> Result<(), String> {
    let surface = browser_surface(parsed)?;
    let since = parsed
        .value("since")
        .map(|value| {
            value
                .parse::<f64>()
                .map_err(|_| "--since must be a number".to_string())
        })
        .transpose()?;
    let mut params = browser_params(Some(&surface), workspace);
    if let Some(since) = since {
        params.insert("since".to_string(), since.to_string());
    }
    let result = browser_call(socket, "browser.console", params)?;
    let entries = result
        .get("entries")
        .or_else(|| result.get("messages"))
        .cloned()
        .unwrap_or_else(|| "[]".to_string());
    println!("{}", filter_console_entries(&entries, since));
    Ok(())
}

/// `--since` filtering for an app that returns the whole buffer.
fn filter_console_entries(entries: &str, since: Option<f64>) -> String {
    let Some(since) = since else {
        return entries.to_string();
    };
    let Ok(mut value) = serde_json::from_str::<serde_json::Value>(entries) else {
        return entries.to_string();
    };
    if let Some(items) = value.as_array_mut() {
        items.retain(|item| {
            item.get("timestamp")
                .or_else(|| item.get("time"))
                .and_then(|value| value.as_f64())
                .map(|timestamp| timestamp >= since)
                .unwrap_or(true)
        });
    }
    serde_json::to_string(&value).unwrap_or_else(|_| entries.to_string())
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

    /// The short ref the skill documents, stable for the same surface and
    /// always `surface:N` -- the e2e test takes it as an opaque handle.
    #[test]
    fn short_surface_ref_is_stable_and_prefixed() {
        let first = short_surface_ref("9f1b2c3d-4e5f-6071-8293-a4b5c6d7e8f9");
        let second = short_surface_ref("9f1b2c3d-4e5f-6071-8293-a4b5c6d7e8f9");
        assert_eq!(first, second);
        assert!(first.starts_with("surface:"), "got: {first}");
        assert_ne!(first, short_surface_ref("another-surface"));
    }

    /// Engines that serialize the script result wrap it in a JSON string; the
    /// plain form must survive unchanged.
    #[test]
    fn js_results_unwrap_a_json_string() {
        assert_eq!(
            decode_js_string("\"Known browser e2e body text\""),
            "Known browser e2e body text"
        );
        assert_eq!(
            decode_js_string("Known browser e2e body text"),
            "Known browser e2e body text"
        );
        assert_eq!(decode_js_string("a \" quoted"), "a \" quoted");
    }

    /// A ref act must refuse a page that moved on, and the refusal is the
    /// documented `stale_ref` the e2e greps for.
    #[test]
    fn ref_acts_carry_the_stale_ref_guard() {
        let script = browser_act_ref_script("click", "e2", "7", None, None, None, None)
            .expect("click needs no extra argument");
        assert!(script.contains("stale_ref"), "got: {script}");
        assert!(script.contains("\"e2\""), "got: {script}");
        assert!(script.contains("\"7\""), "got: {script}");

        let error = browser_act_ref_script("press", "e1", "1", None, None, None, None)
            .expect_err("press without --key is rejected");
        assert!(error.contains("--key"), "got: {error}");
    }

    /// Caller values are embedded as JSON string literals, never interpolated
    /// raw into the page script.
    #[test]
    fn content_scripts_escape_their_selector() {
        let script = browser_content_script("text", "a\"] , body");
        assert!(
            script.contains("document.querySelector(\"a\\\"] , body\")"),
            "got: {script}"
        );

        let act = browser_act_ref_script(
            "fill",
            "e1",
            "1",
            Some("O'Neil \"quoted\""),
            None,
            None,
            None,
        )
        .expect("fill accepts any value");
        assert!(
            act.contains("el.value = \"O'Neil \\\"quoted\\\"\""),
            "got: {act}"
        );
    }

    /// The running-pane name wins over the pre-rebrand one.
    #[test]
    fn browser_workspace_prefers_the_current_name() {
        assert_eq!(
            browser_workspace(&environment(&[
                ("SIRIO_WORKTREE_ID", "current"),
                ("TILLER_WORKTREE_ID", "legacy"),
            ])),
            Some("current".to_string())
        );
        assert_eq!(
            browser_workspace(&environment(&[("TILLER_WORKTREE_ID", "legacy")])),
            Some("legacy".to_string())
        );
        assert_eq!(
            browser_workspace(&environment(&[("SIRIO_WORKTREE_ID", "")])),
            None
        );
    }
}
