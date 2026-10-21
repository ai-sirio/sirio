//! MCP servers as Claude Code reports them: the `mcp_status` answer, and the
//! sign-in an `mcp_authenticate` request starts.
//!
//! Like everything in this crate it degrades rather than rejects: an entry
//! without a name is dropped, an unknown status or source is kept verbatim,
//! and only an answer with no `mcpServers` array gives `None`.

use serde_json::Value;

use crate::message::McpServerStatus;

/// One MCP server and where it stands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct McpServer {
    pub name: String,
    pub status: McpState,
    pub origin: McpOrigin,
}

/// The CLI's word for a server's state; one it does not know is kept as sent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum McpState {
    Connected,
    Pending,
    NeedsAuth,
    Failed,
    Disabled,
    Other(String),
}

impl McpState {
    fn from_wire(word: &str) -> Self {
        match word {
            "connected" => Self::Connected,
            "pending" => Self::Pending,
            "needs-auth" => Self::NeedsAuth,
            "failed" => Self::Failed,
            "disabled" => Self::Disabled,
            other => Self::Other(other.to_owned()),
        }
    }
}

/// Where a server was configured, from the entry's `source`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum McpOrigin {
    ClaudeAi,
    Plugin,
    User,
    Project,
    Other(String),
}

impl McpOrigin {
    fn from_wire(word: &str) -> Self {
        match word {
            "claudeai" => Self::ClaudeAi,
            "plugin" => Self::Plugin,
            "user" => Self::User,
            "project" => Self::Project,
            other => Self::Other(other.to_owned()),
        }
    }
}

/// What `mcp_authenticate` answered: the page to open, when there is one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct McpAuthStart {
    pub auth_url: Option<String>,
    pub requires_user_action: bool,
}

/// The servers of an `mcp_status` answer. `None` only when `mcpServers` is
/// not an array; an entry without a name is skipped.
#[must_use]
pub fn parse_mcp_status(payload: &Value) -> Option<Vec<McpServer>> {
    let list = payload.get("mcpServers")?.as_array()?;
    Some(
        list.iter()
            .filter_map(|entry| {
                let name = entry.get("name")?.as_str()?.to_owned();
                let word = |key: &str| entry.get(key).and_then(Value::as_str).unwrap_or("");
                Some(McpServer {
                    name,
                    status: McpState::from_wire(word("status")),
                    origin: McpOrigin::from_wire(word("source")),
                })
            })
            .collect(),
    )
}

impl McpAuthStart {
    /// `None` only when the payload is not an object.
    #[must_use]
    pub fn parse(payload: &Value) -> Option<Self> {
        let object = payload.as_object()?;
        Some(Self {
            auth_url: object
                .get("authUrl")
                .and_then(Value::as_str)
                .map(str::to_owned),
            requires_user_action: object
                .get("requiresUserAction")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        })
    }
}

/// `system/init`'s list carries no `source`, so the origin is unknown.
impl From<&McpServerStatus> for McpServer {
    fn from(status: &McpServerStatus) -> Self {
        Self {
            name: status.name.clone(),
            status: McpState::from_wire(&status.status),
            origin: McpOrigin::Other(String::new()),
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn the_probed_status_list_parses() {
        let payload = json!({"mcpServers": [
            {"name": "claude.ai Notion", "status": "needs-auth", "scope": "claudeai", "source": "claudeai"},
            {"name": "codegraph", "status": "connected", "scope": "user", "source": "user"},
        ]});
        assert_eq!(
            parse_mcp_status(&payload),
            Some(vec![
                McpServer {
                    name: "claude.ai Notion".into(),
                    status: McpState::NeedsAuth,
                    origin: McpOrigin::ClaudeAi
                },
                McpServer {
                    name: "codegraph".into(),
                    status: McpState::Connected,
                    origin: McpOrigin::User
                },
            ])
        );
    }

    #[test]
    fn an_unknown_status_is_kept_verbatim() {
        let payload =
            json!({"mcpServers": [{"name": "x", "status": "warming", "source": "plugin"}]});
        let servers = parse_mcp_status(&payload).expect("list");
        assert_eq!(servers[0].status, McpState::Other("warming".into()));
        assert_eq!(servers[0].origin, McpOrigin::Plugin);
    }

    #[test]
    fn an_entry_without_a_name_is_skipped() {
        let payload = json!({"mcpServers": [
            {"status": "connected"},
            {"name": "kept", "status": "pending", "source": "project"},
        ]});
        let servers = parse_mcp_status(&payload).expect("list");
        assert_eq!(servers.len(), 1);
        assert_eq!(servers[0].name, "kept");
        assert_eq!(servers[0].status, McpState::Pending);
        assert_eq!(servers[0].origin, McpOrigin::Project);
    }

    #[test]
    fn a_status_answer_without_a_list_is_none() {
        assert_eq!(parse_mcp_status(&json!({})), None);
    }

    #[test]
    fn an_auth_answer_carries_the_url_to_open() {
        let payload = json!({
            "authUrl": "https://example.com/authorize?x=1",
            "requiresUserAction": true,
            "callbackExpected": true,
            "redirectScheme": "localhost",
            "state": "abc",
            "callbackPort": 61628
        });
        let start = McpAuthStart::parse(&payload).expect("answer");
        assert_eq!(
            start.auth_url.as_deref(),
            Some("https://example.com/authorize?x=1")
        );
        assert!(start.requires_user_action);
    }

    #[test]
    fn an_auth_answer_without_url_is_still_read() {
        let start = McpAuthStart::parse(&json!({"requiresUserAction": false})).expect("answer");
        assert_eq!(start.auth_url, None);
        assert!(!start.requires_user_action);
    }

    #[test]
    fn an_init_entry_converts_with_an_empty_origin() {
        let init = McpServerStatus {
            name: "linear".into(),
            status: "failed".into(),
        };
        let server = McpServer::from(&init);
        assert_eq!(server.status, McpState::Failed);
        assert_eq!(server.origin, McpOrigin::Other(String::new()));
    }
}
