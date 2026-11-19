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
    /// The tools the server lists; empty until `mcp_status` has answered.
    pub tools: Vec<McpTool>,
}

/// One tool of a server, with the `_meta.ui` the CLI forwarded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct McpTool {
    pub name: String,
    pub ui: Option<ToolUi>,
}

/// A tool's `_meta.ui`. Absent visibility = both true; an explicit list sets exactly what it
/// names.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolUi {
    pub resource_uri: Option<String>,
    pub for_model: bool,
    pub for_app: bool,
}

impl ToolUi {
    /// Reads `ui.resourceUri`, then the flat `"ui/resourceUri"` only when the nested one is
    /// absent. `None` when the tool carries neither a `ui` object nor the flat key.
    fn from_meta(meta: &Value) -> Option<Self> {
        let ui = meta.get("ui").filter(|ui| ui.is_object());
        let flat = meta.get("ui/resourceUri").and_then(Value::as_str);
        if ui.is_none() && flat.is_none() {
            return None;
        }
        let nested = ui.and_then(|ui| ui.get("resourceUri")).and_then(Value::as_str);
        let (mut for_model, mut for_app) = (true, true);
        if let Some(list) = ui.and_then(|ui| ui.get("visibility")).and_then(Value::as_array) {
            let names = |word: &str| list.iter().any(|name| name.as_str() == Some(word));
            (for_model, for_app) = (names("model"), names("app"));
        }
        Some(Self {
            resource_uri: nested.or(flat).map(str::to_owned),
            for_model,
            for_app,
        })
    }
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
                    tools: parse_tools(entry),
                })
            })
            .collect(),
    )
}

fn parse_tools(entry: &Value) -> Vec<McpTool> {
    let Some(list) = entry.get("tools").and_then(Value::as_array) else {
        return Vec::new();
    };
    list.iter()
        .filter_map(|tool| {
            Some(McpTool {
                name: tool.get("name")?.as_str()?.to_owned(),
                ui: tool.get("_meta").and_then(ToolUi::from_meta),
            })
        })
        .collect()
}

/// The CLI's name rule (claude 2.1.327 `Dn`): `name.replace(/[^a-zA-Z0-9_-]/g, "_")`, with no
/// `u` flag, so it works per UTF-16 code unit: a character outside the BMP is two units and
/// gives two `_`. A name that starts with `claude.ai ` then also gets
/// `.replace(/_+/g, "_").replace(/^_|_$/g, "")`.
fn normalize(name: &str) -> String {
    let mut mapped = String::with_capacity(name.len());
    for c in name.chars() {
        if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
            mapped.push(c);
        } else {
            mapped.extend(std::iter::repeat('_').take(c.len_utf16()));
        }
    }
    if !name.starts_with("claude.ai ") {
        return mapped;
    }
    let mut out = String::with_capacity(mapped.len());
    for c in mapped.chars() {
        if c != '_' || !out.ends_with('_') {
            out.push(c);
        }
    }
    out.trim_matches('_').to_owned()
}

/// The CLI's full name for a tool: `mcp__` + both parts normalized. The tool part goes through
/// the same normalizer as the server part (claude 2.1.327: `wr(e,n)=Pr(e)+Dn(n)`), so a tool
/// called `do.thing` is `mcp__s__do_thing`.
#[must_use]
pub fn claude_tool_name(server: &str, tool: &str) -> String {
    format!("mcp__{}__{}", normalize(server), normalize(tool))
}

/// Whether `mcp_call` maps the pair's full name back to exactly this pair: it splits at the
/// first `__` after `mcp__`.
fn round_trips(server: &str, tool: &str) -> bool {
    let full = claude_tool_name(server, tool);
    full.strip_prefix("mcp__").and_then(|rest| rest.split_once("__"))
        == Some((normalize(server).as_str(), normalize(tool).as_str()))
}

/// How many (server, tool) pairs across `servers` build `full`.
fn pairs_building(servers: &[McpServer], full: &str) -> usize {
    servers
        .iter()
        .flat_map(|server| server.tools.iter().map(move |tool| (server, tool)))
        .filter(|(server, tool)| claude_tool_name(&server.name, &tool.name) == full)
        .count()
}

/// Servers other than `servers[index]`, in any state, with the same normalized name.
fn has_twin(servers: &[McpServer], index: usize) -> bool {
    let norm = normalize(&servers[index].name);
    servers
        .iter()
        .enumerate()
        .any(|(other, server)| other != index && normalize(&server.name) == norm)
}

/// The tool a `tool_use` named, when it is a tool with a widget and the CLI's name maps back to
/// it alone.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UiToolRef {
    pub server: String,
    pub origin: McpOrigin,
    pub tool: String,
    pub resource_uri: String,
}

/// The unique (server, tool) pair whose full name is `tool_use_name`, when it round-trips,
/// no other pair builds the name, no other server in any state shares the server's normalized
/// name, and the tool names a `resourceUri`.
#[must_use]
pub fn ui_tool_for(servers: &[McpServer], tool_use_name: &str) -> Option<UiToolRef> {
    if pairs_building(servers, tool_use_name) != 1 {
        return None;
    }
    let (index, server) = servers.iter().enumerate().find(|(_, server)| {
        server
            .tools
            .iter()
            .any(|tool| claude_tool_name(&server.name, &tool.name) == tool_use_name)
    })?;
    let tool = server
        .tools
        .iter()
        .find(|tool| claude_tool_name(&server.name, &tool.name) == tool_use_name)?;
    if has_twin(servers, index) || !round_trips(&server.name, &tool.name) {
        return None;
    }
    let resource_uri = tool.ui.as_ref()?.resource_uri.clone()?;
    Some(UiToolRef {
        server: server.name.clone(),
        origin: server.origin.clone(),
        tool: tool.name.clone(),
        resource_uri,
    })
}

/// Why a widget's call is refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AppCallRefusal {
    ServerGone,
    NotListed,
    NotForApp,
    Ambiguous,
    NameChanged,
}

/// The full `mcp__...` name a widget of `server`/`origin` may call `tool` by. The tool must be
/// listed and visible to the app, its name must equal its own normalized form (the CLI may
/// forward the normalized name to the server for a tool hidden from the model), and the name
/// must map back to this pair alone.
///
/// A tool with no `_meta.ui` has the default visibility, model and app.
pub fn app_call_name(
    servers: &[McpServer],
    server: &str,
    origin: &McpOrigin,
    tool: &str,
) -> Result<String, AppCallRefusal> {
    let (index, found) = servers
        .iter()
        .enumerate()
        .find(|(_, candidate)| candidate.name == server && candidate.origin == *origin)
        .ok_or(AppCallRefusal::ServerGone)?;
    let listed = found
        .tools
        .iter()
        .find(|candidate| candidate.name == tool)
        .ok_or(AppCallRefusal::NotListed)?;
    if listed.ui.as_ref().is_some_and(|ui| !ui.for_app) {
        return Err(AppCallRefusal::NotForApp);
    }
    let full = claude_tool_name(server, tool);
    if has_twin(servers, index) || pairs_building(servers, &full) != 1 {
        return Err(AppCallRefusal::Ambiguous);
    }
    if normalize(tool) != tool || !round_trips(server, tool) {
        return Err(AppCallRefusal::NameChanged);
    }
    Ok(full)
}

/// A result as an MCP CallToolResult. `value` is a `tool_use_result` or an `mcp_call` answer
/// (object or string); `is_error` comes from the `tool_result` block or the error envelope.
/// Drops `com.anthropic/*` meta keys; turns string content into one text block. `isError: true`
/// appears when `is_error` is true or the object's own `isError` is `true`; `isError: false` is
/// never emitted, and a non-boolean own `isError` is removed.
#[must_use]
pub fn call_tool_result(value: &Value, is_error: bool) -> Value {
    let text_block = |text: &str| serde_json::json!([{"type": "text", "text": text}]);
    let mut result = match value {
        Value::Object(fields) => fields.clone(),
        Value::String(text) => {
            let mut fields = serde_json::Map::new();
            fields.insert("content".to_owned(), text_block(text));
            fields
        }
        other => {
            let mut fields = serde_json::Map::new();
            fields.insert("content".to_owned(), text_block(&other.to_string()));
            fields
        }
    };
    if let Some(Value::String(text)) = result.get("content") {
        let blocks = text_block(text);
        result.insert("content".to_owned(), blocks);
    }
    if let Some(Value::Object(meta)) = result.get_mut("_meta") {
        meta.retain(|key, _| !key.starts_with("com.anthropic/"));
        if meta.is_empty() {
            result.remove("_meta");
        }
    }
    if is_error || result.get("isError") == Some(&Value::Bool(true)) {
        result.insert("isError".to_owned(), Value::Bool(true));
    } else {
        result.remove("isError");
    }
    Value::Object(result)
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
            tools: Vec::new(),
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
                    origin: McpOrigin::ClaudeAi,
                    tools: Vec::new()
                },
                McpServer {
                    name: "codegraph".into(),
                    status: McpState::Connected,
                    origin: McpOrigin::User,
                    tools: Vec::new()
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

    // Failure list for `ui_tool_for`: matching two servers whose names normalize alike (even when
    // one is pending, failed or disabled, or lists no tools); matching `files.` + `t`, whose full
    // name splits as another pair; matching tools `a.b` and `a_b` of one server; missing
    // `my.server`, `claude.ai Linear` or a tool whose own name has a `.`; returning a tool with no
    // `resourceUri`; preferring the flat `ui/resourceUri` over `ui.resourceUri`.
    fn tool(name: &str, uri: Option<&str>) -> McpTool {
        McpTool {
            name: name.into(),
            ui: uri.map(|uri| ToolUi {
                resource_uri: Some(uri.into()),
                for_model: true,
                for_app: true,
            }),
        }
    }

    fn server(name: &str, status: McpState, tools: Vec<McpTool>) -> McpServer {
        McpServer { name: name.into(), status, origin: McpOrigin::User, tools }
    }

    fn connected(name: &str, tools: Vec<McpTool>) -> McpServer {
        server(name, McpState::Connected, tools)
    }

    #[test]
    fn ui_tool_for_needs_one_unambiguous_match() {
        let ui = Some("ui://x");
        let found = |servers: &[McpServer], name: &str| {
            ui_tool_for(servers, name).map(|found| (found.server, found.tool))
        };
        let dotted = connected("my.server", vec![tool("show", ui)]);
        assert_eq!(
            found(&[dotted.clone()], "mcp__my_server__show"),
            Some(("my.server".into(), "show".into()))
        );
        for state in [McpState::Pending, McpState::Failed, McpState::Disabled] {
            let twin = server("my_server", state, Vec::new());
            assert_eq!(found(&[dotted.clone(), twin], "mcp__my_server__show"), None);
        }
        let linear = connected("claude.ai Linear", vec![tool("go", ui)]);
        assert_eq!(
            found(&[linear], "mcp__claude_ai_Linear__go"),
            Some(("claude.ai Linear".into(), "go".into()))
        );
        let files = connected("files.", vec![tool("t", ui)]);
        assert_eq!(found(&[files], "mcp__files___t"), None);
        // The CLI's regex has no `u` flag: a character outside the BMP is two units, two `_`.
        assert_eq!(claude_tool_name("a😀b", "show"), "mcp__a__b__show");
        let clash = connected("s", vec![tool("a.b", ui), tool("a_b", ui)]);
        assert_eq!(found(&[clash], "mcp__s__a_b"), None);
        let dotted_tool = connected("s", vec![tool("do.thing", ui)]);
        assert_eq!(
            found(&[dotted_tool], "mcp__s__do_thing"),
            Some(("s".into(), "do.thing".into()))
        );
        let plain = connected("s", vec![tool("plain", None)]);
        assert_eq!(found(&[plain], "mcp__s__plain"), None);
        assert_eq!(found(&[connected("s", vec![tool("t", ui)])], "mcp__s__other"), None);
    }

    // Failure list for `app_call_name`: allowing a tool not listed; allowing visibility
    // `["model"]`; refusing an absent visibility; allowing a server whose `source` differs;
    // allowing a name that does not split back into its own pair, or that another pair also
    // builds; allowing a server when another server, in any state, has the same normalized
    // name; allowing a tool whose name is not its own normalized form.
    fn app_tool(name: &str, for_model: bool, for_app: bool) -> McpTool {
        McpTool {
            name: name.into(),
            ui: Some(ToolUi { resource_uri: None, for_model, for_app }),
        }
    }

    #[test]
    fn app_call_name_allows_listed_app_tools() {
        let servers = [connected(
            "my.server",
            vec![
                app_tool("both", true, true),
                app_tool("app", false, true),
                app_tool("model", true, false),
                tool("plain", None),
                tool("novis", Some("ui://x")),
            ],
        )];
        let call = |name: &str, origin: &McpOrigin| {
            app_call_name(&servers, "my.server", origin, name)
        };
        let user = McpOrigin::User;
        assert_eq!(call("both", &user), Ok("mcp__my_server__both".into()));
        assert_eq!(call("app", &user), Ok("mcp__my_server__app".into()));
        assert_eq!(call("plain", &user), Ok("mcp__my_server__plain".into()));
        assert_eq!(call("novis", &user), Ok("mcp__my_server__novis".into()));
        assert_eq!(call("model", &user), Err(AppCallRefusal::NotForApp));
        assert_eq!(call("missing", &user), Err(AppCallRefusal::NotListed));
        assert_eq!(call("both", &McpOrigin::Plugin), Err(AppCallRefusal::ServerGone));
        assert_eq!(
            app_call_name(&servers, "gone", &user, "both"),
            Err(AppCallRefusal::ServerGone)
        );
    }

    #[test]
    fn app_call_name_needs_a_name_that_round_trips() {
        let user = McpOrigin::User;
        let ui = Some("ui://x");
        let one = |servers: &[McpServer], name: &str, tool: &str| {
            app_call_name(servers, name, &user, tool)
        };
        let dotted_tool = [connected("s", vec![tool("do.thing", ui)])];
        assert_eq!(one(&dotted_tool, "s", "do.thing"), Err(AppCallRefusal::NameChanged));
        let files = [connected("files.", vec![tool("t", ui)])];
        assert_eq!(one(&files, "files.", "t"), Err(AppCallRefusal::NameChanged));
        let clash = [connected("s", vec![tool("a.b", ui), tool("a_b", ui)])];
        assert_eq!(one(&clash, "s", "a_b"), Err(AppCallRefusal::Ambiguous));
        for state in [McpState::Pending, McpState::Failed, McpState::Disabled] {
            let twins = [
                connected("my.server", vec![tool("t", ui)]),
                server("my_server", state, Vec::new()),
            ];
            assert_eq!(one(&twins, "my.server", "t"), Err(AppCallRefusal::Ambiguous));
        }
        let other = [connected("my.server", vec![tool("t", ui)]), connected("other", Vec::new())];
        assert_eq!(one(&other, "my.server", "t"), Ok("mcp__my_server__t".into()));
    }

    #[test]
    fn mcp_status_keeps_each_tools_ui() {
        let payload = json!({"mcpServers": [
            {"name": "probe", "status": "connected", "source": "dynamic", "tools": [
                {"name": "show", "annotations": {}, "_meta": {"ui": {
                    "resourceUri": "ui://probe/show.html", "visibility": ["model", "app"]}}},
                {"name": "app_only", "_meta": {"ui": {
                    "resourceUri": "ui://probe/show.html", "visibility": ["app"]}}},
                {"name": "flat", "_meta": {"ui/resourceUri": "ui://probe/flat.html"}},
                {"name": "bad", "_meta": {"ui": {"visibility": ["model", "app"]}}},
                {"name": "plain", "annotations": {}},
                {"name": "notobj", "_meta": {"ui": {"visibility": []}}},
                {"name": "mixed", "_meta": {"ui": {"resourceUri": "ui://probe/show.html"},
                    "ui/resourceUri": "ui://probe/flat.html"}},
                {"name": "odd", "_meta": {"ui": {"visibility": ["app", "other"]}}},
            ]},
            {"name": "other", "status": "connected", "source": "dynamic", "tools": [
                {"name": "ping", "annotations": {}}]},
        ]});
        let servers = parse_mcp_status(&payload).expect("list");
        let ui = |name: &str| {
            servers[0].tools.iter().find(|tool| tool.name == name).expect("tool").ui.clone()
        };
        let want = |uri: Option<&str>, for_model, for_app| {
            Some(ToolUi { resource_uri: uri.map(str::to_owned), for_model, for_app })
        };
        assert_eq!(ui("show"), want(Some("ui://probe/show.html"), true, true));
        assert_eq!(ui("app_only"), want(Some("ui://probe/show.html"), false, true));
        assert_eq!(ui("flat"), want(Some("ui://probe/flat.html"), true, true));
        assert_eq!(ui("bad"), want(None, true, true));
        assert_eq!(ui("plain"), None);
        assert_eq!(ui("notobj"), want(None, false, false));
        assert_eq!(ui("mixed"), want(Some("ui://probe/show.html"), true, true));
        assert_eq!(ui("odd"), want(None, false, true));
        assert_eq!(servers[1].tools[0].ui, None);
    }

    // Failure list for `call_tool_result`: leaving a `com.anthropic/` meta key; leaving an empty
    // `_meta`; emitting `isError: false`; dropping `structuredContent`; leaving string content
    // unwrapped; losing an array `content`; a string answer without a text block.
    #[test]
    fn results_become_call_tool_results() {
        let object = json!({
            "content": "shown",
            "structuredContent": {"n": 1},
            "_meta": {"other/key": 1, "com.anthropic/x": 2}
        });
        assert_eq!(
            call_tool_result(&object, false),
            json!({
                "content": [{"type": "text", "text": "shown"}],
                "structuredContent": {"n": 1},
                "_meta": {"other/key": 1}
            })
        );
        let only_ours = json!({"content": "a", "_meta": {"com.anthropic/x": 2}});
        assert_eq!(
            call_tool_result(&only_ours, false),
            json!({"content": [{"type": "text", "text": "a"}]})
        );
        let blocks = json!({"content": [{"type": "text", "text": "kept"}]});
        assert_eq!(call_tool_result(&blocks, false), blocks);
        assert_eq!(
            call_tool_result(&json!("boom"), true),
            json!({"content": [{"type": "text", "text": "boom"}], "isError": true})
        );
        assert_eq!(
            call_tool_result(&json!({"content": "x", "isError": true}), false)["isError"],
            json!(true)
        );
        assert_eq!(
            call_tool_result(&json!({"content": "x", "isError": "yes"}), false),
            json!({"content": [{"type": "text", "text": "x"}]})
        );
        assert_eq!(
            call_tool_result(&json!({"content": "x", "isError": false}), false),
            json!({"content": [{"type": "text", "text": "x"}]})
        );
        assert_eq!(
            call_tool_result(&json!("fine"), false),
            json!({"content": [{"type": "text", "text": "fine"}]})
        );
    }
}
