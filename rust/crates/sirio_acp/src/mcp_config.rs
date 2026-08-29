//! Reads a project's `.mcp.json` (Claude Code's own MCP config file format)
//! and converts it into the ACP `session/new` request's `mcpServers` list
//! (F-CHAT-33).
//!
//! **Why this exists at all.** `NewSessionRequest::new(cwd)` — the
//! constructor `AcpClient::launch` actually called — sets `mcp_servers:
//! vec![]` and nothing in this crate ever called `.mcp_servers(...)` to
//! replace it. That is not a rendering gap or a stderr-parsing gap: the ACP
//! spec makes MCP server discovery the **client's** job (`mcp_servers` is a
//! required, non-defaulted field on the wire request — confirmed against
//! `agent-client-protocol-schema`'s `NewSessionRequest`), so an agent that
//! receives `mcpServers: []` has nothing to attempt to connect to and can
//! never emit a connection-failure line on any channel, no matter how a
//! `.mcp.json` in the project is broken. Four live drives against a broken
//! `.mcp.json` produced no banner because of this — not because
//! `looks_like_mcp_warning`'s vocabulary is too narrow. Confirmed against a
//! real ACP agent process driven by hand (via `SIRIO_ACP_PROGRAM`):
//! hand: `session/new` with an explicit `mcpServers: []` (what this crate
//! used to always send) started a session with no MCP-related output at
//! all, while omitting the field outright is rejected by the agent with
//! `"mcpServers": {"_errors": ["Required value is missing"]}` — proving the
//! field is load-bearing on the wire, not an optional nicety.
use agent_client_protocol::schema::v1::{
    EnvVariable, McpServer, McpServerHttp, McpServerSse, McpServerStdio,
};
use std::path::Path;

/// Parses `<cwd>/.mcp.json` into ACP `McpServer` entries. Missing file,
/// unreadable file, or malformed JSON all yield an empty list rather than
/// an error — this is best-effort discovery on the same file Claude Code's
/// own CLI reads, not a contract this crate owns the schema of.
pub fn discover_mcp_servers(cwd: &Path) -> Vec<McpServer> {
    let Ok(text) = std::fs::read_to_string(cwd.join(".mcp.json")) else {
        return Vec::new();
    };
    let Ok(root) = serde_json::from_str::<serde_json::Value>(&text) else {
        return Vec::new();
    };
    let Some(servers) = root.get("mcpServers").and_then(|v| v.as_object()) else {
        return Vec::new();
    };

    servers
        .iter()
        .filter_map(|(name, config)| mcp_server_from_json(name, config))
        .collect()
}

fn mcp_server_from_json(name: &str, config: &serde_json::Value) -> Option<McpServer> {
    let server_type = config.get("type").and_then(|v| v.as_str());
    let url = config.get("url").and_then(|v| v.as_str());

    if server_type == Some("sse")
        && let Some(url) = url
    {
        return Some(McpServer::Sse(McpServerSse::new(name, url)));
    }
    if let Some(url) = url {
        // Covers both an explicit `"type": "http"` and the common case of
        // a remote server with a `url` and no `type` at all.
        return Some(McpServer::Http(McpServerHttp::new(name, url)));
    }

    let command = config.get("command").and_then(|v| v.as_str())?;
    let args: Vec<String> = config
        .get("args")
        .and_then(|v| v.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    let env: Vec<EnvVariable> = config
        .get("env")
        .and_then(|v| v.as_object())
        .map(|map| {
            map.iter()
                .filter_map(|(key, value)| {
                    value
                        .as_str()
                        .map(|value| EnvVariable::new(key.clone(), value))
                })
                .collect()
        })
        .unwrap_or_default();

    let mut stdio = McpServerStdio::new(name, command);
    stdio.args = args;
    stdio.env = env;
    Some(McpServer::Stdio(stdio))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_file_yields_no_servers() {
        let dir = std::env::temp_dir().join("sirio-mcp-config-test-missing");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        assert!(discover_mcp_servers(&dir).is_empty());
    }

    #[test]
    fn malformed_json_yields_no_servers_not_a_panic() {
        let dir = std::env::temp_dir().join("sirio-mcp-config-test-malformed");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(".mcp.json"), "{ not json").unwrap();
        assert!(discover_mcp_servers(&dir).is_empty());
    }

    #[test]
    fn a_stdio_server_pointing_at_a_nonexistent_binary_still_parses() {
        // The exact broken-.mcp.json shape F-CHAT-33's live drives used:
        // valid JSON, a `command` that does not resolve to anything. This
        // must parse into a real McpServer::Stdio entry — whether the
        // *agent* can connect to it is the agent's problem, this function's
        // only job is faithfully carrying the config onto the wire.
        let dir = std::env::temp_dir().join("sirio-mcp-config-test-broken-stdio");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join(".mcp.json"),
            r#"{"mcpServers":{"broken-server":{"command":"/definitely/missing/mcp-nonexistent-binary","args":[]}}}"#,
        )
        .unwrap();
        let servers = discover_mcp_servers(&dir);
        assert_eq!(servers.len(), 1);
        match &servers[0] {
            McpServer::Stdio(stdio) => {
                assert_eq!(stdio.name, "broken-server");
                assert_eq!(
                    stdio.command,
                    Path::new("/definitely/missing/mcp-nonexistent-binary")
                );
            }
            other => panic!("expected Stdio, got {other:?}"),
        }
    }

    #[test]
    fn args_and_env_carry_through() {
        let dir = std::env::temp_dir().join("sirio-mcp-config-test-args-env");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join(".mcp.json"),
            r#"{"mcpServers":{"xcodebuildmcp":{"command":"npx","args":["--yes","xcodebuildmcp@2.6.2","mcp"],"env":{"XCODEBUILDMCP_ENABLED_WORKFLOWS":"simulator,ui-automation"}}}}"#,
        )
        .unwrap();
        let servers = discover_mcp_servers(&dir);
        assert_eq!(servers.len(), 1);
        match &servers[0] {
            McpServer::Stdio(stdio) => {
                assert_eq!(stdio.args, vec!["--yes", "xcodebuildmcp@2.6.2", "mcp"]);
                assert_eq!(stdio.env.len(), 1);
                assert_eq!(stdio.env[0].name, "XCODEBUILDMCP_ENABLED_WORKFLOWS");
                assert_eq!(stdio.env[0].value, "simulator,ui-automation");
            }
            other => panic!("expected Stdio, got {other:?}"),
        }
    }

    #[test]
    fn a_remote_http_server_parses() {
        let dir = std::env::temp_dir().join("sirio-mcp-config-test-http");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join(".mcp.json"),
            r#"{"mcpServers":{"github":{"type":"http","url":"https://api.githubcopilot.com/mcp/"}}}"#,
        )
        .unwrap();
        let servers = discover_mcp_servers(&dir);
        assert_eq!(servers.len(), 1);
        match &servers[0] {
            McpServer::Http(http) => {
                assert_eq!(http.name, "github");
                assert_eq!(http.url, "https://api.githubcopilot.com/mcp/");
            }
            other => panic!("expected Http, got {other:?}"),
        }
    }

    #[test]
    fn an_entry_with_neither_command_nor_url_is_skipped_not_panicked() {
        let dir = std::env::temp_dir().join("sirio-mcp-config-test-empty-entry");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join(".mcp.json"),
            r#"{"mcpServers":{"nothing-useful":{}}}"#,
        )
        .unwrap();
        assert!(discover_mcp_servers(&dir).is_empty());
    }
}
