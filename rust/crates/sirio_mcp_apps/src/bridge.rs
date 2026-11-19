//! The JSON-RPC rules between the host and a widget view: what parses, what routes, and the
//! messages the host writes.

use serde_json::{Value, json};

pub const PROTOCOL_VERSION: &str = "2026-01-26";
pub const MAX_MESSAGE_BYTES: usize = 1 << 20;
pub const METHOD_NOT_FOUND: i64 = -32601;
pub const INVALID_PARAMS: i64 = -32602;
pub const DENIED: i64 = -32000;

const MAX_TOOL_NAME_CHARS: usize = 256;
const MAX_HEIGHT: f64 = 100_000.0;

#[derive(Debug, Clone, PartialEq)]
pub enum Inbound {
    Call { id: Value, method: String, params: Value },
    Note { method: String, params: Value },
    Response { id: Value },
    Ignored,
}

/// One message from the view. Anything that is not a well-formed JSON-RPC 2.0 request,
/// notification or response (string or number id) is `Ignored`, and so is a message over
/// `MAX_MESSAGE_BYTES`.
pub fn parse_inbound(text: &str) -> Inbound {
    if text.len() > MAX_MESSAGE_BYTES {
        return Inbound::Ignored;
    }
    let Ok(Value::Object(mut msg)) = serde_json::from_str::<Value>(text) else {
        return Inbound::Ignored;
    };
    if msg.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
        return Inbound::Ignored;
    }
    let id = match msg.remove("id") {
        None => None,
        Some(id) if id.is_string() || id.is_number() => Some(id),
        Some(_) => return Inbound::Ignored,
    };
    let answered = msg.contains_key("result") || msg.contains_key("error");
    match msg.remove("method") {
        Some(Value::String(method)) if !answered => {
            let params = msg.remove("params").unwrap_or(Value::Null);
            match id {
                Some(id) => Inbound::Call { id, method, params },
                None => Inbound::Note { method, params },
            }
        }
        None if answered => id.map_or(Inbound::Ignored, |id| Inbound::Response { id }),
        _ => Inbound::Ignored,
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ToolsCall {
    pub name: String,
    pub arguments: Value,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CallRoute {
    Initialize,
    Ping,
    ToolsCall(ToolsCall),
    DisplayMode,
    Unsupported,
    BadParams,
}

/// The four requests the host serves; every other method is `Unsupported`.
pub fn route_call(method: &str, params: &Value) -> CallRoute {
    match method {
        "ui/initialize" => CallRoute::Initialize,
        "ping" => CallRoute::Ping,
        "ui/request-display-mode" => CallRoute::DisplayMode,
        "tools/call" => tools_call(params),
        _ => CallRoute::Unsupported,
    }
}

fn tools_call(params: &Value) -> CallRoute {
    let name = params.get("name").and_then(Value::as_str).unwrap_or_default();
    if name.is_empty() || name.chars().count() > MAX_TOOL_NAME_CHARS {
        return CallRoute::BadParams;
    }
    let arguments = match params.get("arguments") {
        None | Some(Value::Null) => json!({}),
        Some(a) if a.is_object() => a.clone(),
        Some(_) => return CallRoute::BadParams,
    };
    CallRoute::ToolsCall(ToolsCall { name: name.to_owned(), arguments })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoteRoute {
    Initialized,
    /// The content height in CSS pixels, rounded up.
    SizeChanged(u32),
    Drop,
}

/// The two notifications the host acts on; every other one is dropped.
pub fn route_note(method: &str, params: &Value) -> NoteRoute {
    match method {
        "ui/notifications/initialized" => NoteRoute::Initialized,
        "ui/notifications/size-changed" => match params.get("height").and_then(Value::as_f64) {
            Some(h) if h.is_finite() && h > 0.0 && h <= MAX_HEIGHT => {
                NoteRoute::SizeChanged(h.ceil() as u32)
            }
            _ => NoteRoute::Drop,
        },
        _ => NoteRoute::Drop,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostContext {
    pub dark: bool,
    pub width: u32,
    pub max_height: u32,
    pub host_version: String,
}

/// The `ui/initialize` result: server tools only, shown inline.
pub fn initialize_result(ctx: &HostContext) -> Value {
    json!({
        "protocolVersion": PROTOCOL_VERSION,
        "hostCapabilities": { "serverTools": {} },
        "hostInfo": { "name": "Sirio", "version": ctx.host_version },
        "hostContext": {
            "theme": if ctx.dark { "dark" } else { "light" },
            "displayMode": "inline",
            "availableDisplayModes": ["inline"],
            "containerDimensions": { "width": ctx.width, "maxHeight": ctx.max_height },
            "platform": "desktop",
        },
    })
}

pub fn answer(id: &Value, result: Value) -> String {
    json!({ "jsonrpc": "2.0", "id": id, "result": result }).to_string()
}

pub fn refuse(id: &Value, code: i64, message: &str) -> String {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
        .to_string()
}

pub fn request(id: &Value, method: &str, params: Value) -> String {
    json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }).to_string()
}

pub fn notify(method: &str, params: Value) -> String {
    json!({ "jsonrpc": "2.0", "method": method, "params": params }).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // Failure list for parsing and routing: accepting non-JSON; accepting a message without
    // "jsonrpc":"2.0"; treating a response as a call or losing its id; accepting an id that is
    // an object or array; parsing an oversized message.
    #[test]
    fn parse_inbound_keeps_only_well_formed_messages() {
        assert!(matches!(parse_inbound("not json"), Inbound::Ignored));
        assert!(matches!(parse_inbound("[1]"), Inbound::Ignored));
        assert!(matches!(
            parse_inbound(r#"{"id":1,"method":"ping"}"#),
            Inbound::Ignored
        ));
        assert!(matches!(
            parse_inbound(r#"{"jsonrpc":"1.0","id":1,"method":"ping"}"#),
            Inbound::Ignored
        ));
        match parse_inbound(r#"{"jsonrpc":"2.0","id":"a","method":"ping"}"#) {
            Inbound::Call { id, method, params } => {
                assert_eq!((id, method.as_str(), params), (json!("a"), "ping", Value::Null));
            }
            _ => panic!("not a call"),
        }
        match parse_inbound(r#"{"jsonrpc":"2.0","method":"x","params":{"k":1}}"#) {
            Inbound::Note { method, params } => {
                assert_eq!((method.as_str(), params), ("x", json!({"k": 1})));
            }
            _ => panic!("not a note"),
        }
        for body in [r#""result":{}"#, r#""error":{"code":1,"message":"m"}"#] {
            let text = format!(r#"{{"jsonrpc":"2.0","id":7,{body}}}"#);
            assert!(matches!(parse_inbound(&text), Inbound::Response { id } if id == json!(7)));
        }
        for id in [r#"{"a":1}"#, "[1]", "null", "true"] {
            let call = format!(r#"{{"jsonrpc":"2.0","id":{id},"method":"ping"}}"#);
            let resp = format!(r#"{{"jsonrpc":"2.0","id":{id},"result":{{}}}}"#);
            assert!(matches!(parse_inbound(&call), Inbound::Ignored), "call id {id}");
            assert!(matches!(parse_inbound(&resp), Inbound::Ignored), "response id {id}");
        }
        assert!(matches!(
            parse_inbound(r#"{"jsonrpc":"2.0","id":1}"#),
            Inbound::Ignored
        ));
        let pad = "x".repeat(MAX_MESSAGE_BYTES);
        let big = format!(r#"{{"jsonrpc":"2.0","method":"x","params":{{"p":"{pad}"}}}}"#);
        assert!(matches!(parse_inbound(&big), Inbound::Ignored));
        // What the host sends parses back as what it is.
        assert!(matches!(
            parse_inbound(&request(&json!(3), "ui/resource-teardown", json!({}))),
            Inbound::Call { .. }
        ));
        assert!(matches!(
            parse_inbound(&notify("ui/notifications/tool-result", json!({}))),
            Inbound::Note { .. }
        ));
        assert!(matches!(
            parse_inbound(&answer(&json!(4), json!({}))),
            Inbound::Response { id } if id == json!(4)
        ));
        assert!(matches!(
            parse_inbound(&refuse(&json!(5), DENIED, "no")),
            Inbound::Response { id } if id == json!(5)
        ));
    }

    // Failure list: routing resources/read, ui/open-link, ui/message,
    // ui/update-model-context, ui/download-file, sampling/createMessage or any unknown method
    // anywhere except `Unsupported`.
    #[test]
    fn only_four_methods_are_routed_and_the_rest_unsupported() {
        let p = json!({"name": "t"});
        assert!(matches!(route_call("ui/initialize", &json!({})), CallRoute::Initialize));
        assert!(matches!(route_call("ping", &Value::Null), CallRoute::Ping));
        assert!(matches!(route_call("tools/call", &p), CallRoute::ToolsCall(_)));
        assert!(matches!(
            route_call("ui/request-display-mode", &json!({})),
            CallRoute::DisplayMode
        ));
        for m in [
            "resources/read",
            "resources/list",
            "ui/open-link",
            "ui/message",
            "ui/update-model-context",
            "ui/download-file",
            "sampling/createMessage",
            "tools/list",
            "logging/setLevel",
            "",
            "UI/INITIALIZE",
        ] {
            assert!(matches!(route_call(m, &p), CallRoute::Unsupported), "routed: {m:?}");
        }
        assert!(matches!(
            route_note("ui/notifications/initialized", &Value::Null),
            NoteRoute::Initialized
        ));
        for m in ["ui/notifications/sandbox-proxy-ready", "notifications/cancelled", "ui/message"] {
            assert!(matches!(route_note(m, &p), NoteRoute::Drop), "routed: {m:?}");
        }
    }

    // Failure list: accepting `tools/call` with array arguments, an empty name or a name over
    // 256 chars.
    #[test]
    fn tools_call_params_are_checked() {
        let route = |p: Value| route_call("tools/call", &p);
        match route(json!({"name": "show", "arguments": {"a": 1}})) {
            CallRoute::ToolsCall(c) => {
                assert_eq!((c.name.as_str(), c.arguments), ("show", json!({"a": 1})));
            }
            _ => panic!("not routed"),
        }
        match route(json!({"name": "show"})) {
            CallRoute::ToolsCall(c) => assert_eq!(c.arguments, json!({})),
            _ => panic!("not routed"),
        }
        assert!(matches!(route(json!({"name": "a".repeat(256)})), CallRoute::ToolsCall(_)));
        for bad in [
            json!({"name": "show", "arguments": [1]}),
            json!({"name": "show", "arguments": "x"}),
            json!({"name": ""}),
            json!({"name": "a".repeat(257)}),
            json!({"name": 5}),
            json!({"arguments": {}}),
            json!([]),
            Value::Null,
        ] {
            assert!(matches!(route(bad.clone()), CallRoute::BadParams), "accepted: {bad}");
        }
    }

    // Failure list: accepting a `size-changed` height that is NaN, <= 0 or > 100000.
    #[test]
    fn size_changed_takes_a_sane_height() {
        let size = |p: Value| route_note("ui/notifications/size-changed", &p);
        assert!(matches!(size(json!({"width": 300, "height": 240})), NoteRoute::SizeChanged(240)));
        assert!(matches!(size(json!({"height": 240.2})), NoteRoute::SizeChanged(241)));
        assert!(matches!(size(json!({"height": 100000})), NoteRoute::SizeChanged(100000)));
        for bad in [
            json!({"height": 0}),
            json!({"height": -5}),
            json!({"height": 100001}),
            json!({"height": 1e300}),
            json!({"height": null}),
            json!({"height": "9"}),
            json!({"width": 300}),
            Value::Null,
        ] {
            assert!(matches!(size(bad.clone()), NoteRoute::Drop), "accepted: {bad}");
        }
        // NaN cannot be written in JSON, so a non-finite number never reaches here from text.
        assert!(matches!(
            parse_inbound(r#"{"jsonrpc":"2.0","method":"m","params":{"height":NaN}}"#),
            Inbound::Ignored
        ));
    }

    #[test]
    fn initialize_result_offers_only_server_tools() {
        let ctx =
            HostContext { dark: true, width: 640, max_height: 480, host_version: "9.9.9".into() };
        let r = initialize_result(&ctx);
        assert_eq!(r["protocolVersion"], "2026-01-26");
        let caps = r["hostCapabilities"].as_object().expect("capabilities");
        assert_eq!(caps.keys().collect::<Vec<_>>(), ["serverTools"]);
        assert_eq!(r["hostInfo"], json!({"name": "Sirio", "version": "9.9.9"}));
        let c = &r["hostContext"];
        assert_eq!(c["theme"], "dark");
        assert_eq!(c["displayMode"], "inline");
        assert_eq!(c["availableDisplayModes"], json!(["inline"]));
        assert_eq!(c["platform"], "desktop");
        assert_eq!(c["containerDimensions"], json!({"width": 640, "maxHeight": 480}));
        let light = HostContext { dark: false, ..ctx };
        assert_eq!(initialize_result(&light)["hostContext"]["theme"], "light");
    }
}
