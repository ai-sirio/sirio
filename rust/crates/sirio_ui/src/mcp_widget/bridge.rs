//! The host side of a widget's JSON-RPC conversation: pure state, no GPUI, no webview.
//!
//! The view's messages go in through `on_message`; what the host must do about them comes out
//! as effects. The parsing and routing rules live in `sirio_chat::mcp_apps::bridge`.

use std::time::{Duration, Instant};

use serde_json::{Value, json};
use sirio_chat::mcp_apps::bridge::{
    CallRoute, DENIED, HostContext, INVALID_PARAMS, Inbound, METHOD_NOT_FOUND, NoteRoute,
    ToolsCall, answer, initialize_result, notify, parse_inbound, refuse, request, route_call,
    route_note,
};

/// How the chat settled a widget's ask.
#[derive(Debug, Clone, PartialEq)]
pub enum AskOutcome {
    Result(Value),
    Denied,
    /// The ask was never put to the user (too large, session gone, ...); the text is the
    /// JSON-RPC error message the widget gets.
    Refused(String),
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum BridgeEffect {
    /// Deliver this JSON text to the view.
    Send(String),
    Ask { ask: u64, name: String, arguments: Value },
    Resize(u32),
    /// `"<method>:<tool or empty>:<reason>"`.
    Refused(String),
    TornDown,
    Close(String),
}

/// Messages the bridge admits per one-second window.
const RATE_LIMIT: u32 = 50;
/// Consecutive windows over the limit that close the widget.
const RATE_CLOSE_WINDOWS: u32 = 5;
const WINDOW: Duration = Duration::from_secs(1);
const COOLDOWN: Duration = Duration::from_secs(10);
const MAX_DENIALS: u32 = 3;

pub(crate) struct WidgetBridge {
    ctx: HostContext,
    initialized: bool,
    input: Option<Value>,
    input_taken: bool,
    input_sent: bool,
    result_sent: bool,
    result: Option<Value>,
    result_taken: bool,
    cancelled: Option<String>,
    open_ask: Option<(u64, Value)>,
    next_ask: u64,
    denials: u32,
    cooldown_until: Option<Instant>,
    window_start: Option<Instant>,
    window_count: u32,
    window_hit: bool,
    hit_run: u32,
    close_sent: bool,
    next_request: u64,
    teardown_id: Option<Value>,
    /// Set by `teardown`, never cleared: a closed bridge answers nothing.
    closed: bool,
}

impl WidgetBridge {
    pub(crate) fn new(ctx: HostContext) -> Self {
        Self {
            ctx,
            initialized: false,
            input: None,
            input_taken: false,
            input_sent: false,
            result_sent: false,
            result: None,
            result_taken: false,
            cancelled: None,
            open_ask: None,
            next_ask: 1,
            denials: 0,
            cooldown_until: None,
            window_start: None,
            window_count: 0,
            window_hit: false,
            hit_run: 0,
            close_sent: false,
            next_request: 1,
            teardown_id: None,
            closed: false,
        }
    }

    /// True once the view has sent `ui/notifications/initialized`.
    pub(crate) fn ready(&self) -> bool {
        self.initialized
    }

    /// True once `tool-input` went to the view.
    pub(crate) fn input_delivered(&self) -> bool {
        self.input_sent
    }

    /// True once `tool-result` went to the view.
    pub(crate) fn result_delivered(&self) -> bool {
        self.result_sent
    }

    pub(crate) fn on_message(&mut self, text: &str, now: Instant) -> Vec<BridgeEffect> {
        if self.closed {
            return match parse_inbound(text) {
                Inbound::Response { id } if self.teardown_id.as_ref() == Some(&id) => {
                    self.teardown_id = None;
                    vec![BridgeEffect::TornDown]
                }
                _ => Vec::new(),
            };
        }
        match self.admit(now) {
            Admit::Pass => {}
            Admit::Drop => return Vec::new(),
            Admit::Close => return self.close("over the rate limit for 5 s"),
        }
        match parse_inbound(text) {
            Inbound::Call { id, method, params } => self.on_call(id, &method, &params, now),
            Inbound::Note { method, params } => match route_note(&method, &params) {
                NoteRoute::Initialized => self.on_initialized(),
                NoteRoute::SizeChanged(height) => vec![BridgeEffect::Resize(height)],
                NoteRoute::Drop => Vec::new(),
            },
            Inbound::Response { .. } | Inbound::Ignored => Vec::new(),
        }
    }

    /// Counts one inbound message in its one-second window.
    fn admit(&mut self, now: Instant) -> Admit {
        let elapsed = self.window_start.map(|start| now.saturating_duration_since(start));
        if elapsed.is_none_or(|elapsed| elapsed >= WINDOW) {
            // A window that ended without a hit, or a gap of a whole window, breaks the run.
            if !self.window_hit || elapsed.is_some_and(|elapsed| elapsed >= WINDOW * 2) {
                self.hit_run = 0;
            }
            self.window_start = Some(now);
            self.window_count = 0;
            self.window_hit = false;
        }
        self.window_count = self.window_count.saturating_add(1);
        if self.window_count <= RATE_LIMIT {
            return Admit::Pass;
        }
        if !self.window_hit {
            self.window_hit = true;
            self.hit_run += 1;
            if self.hit_run >= RATE_CLOSE_WINDOWS && !self.close_sent {
                return Admit::Close;
            }
        }
        Admit::Drop
    }

    fn close(&mut self, why: &str) -> Vec<BridgeEffect> {
        self.close_sent = true;
        vec![BridgeEffect::Close(why.to_owned())]
    }

    fn on_call(
        &mut self,
        id: Value,
        method: &str,
        params: &Value,
        now: Instant,
    ) -> Vec<BridgeEffect> {
        match route_call(method, params) {
            CallRoute::Initialize => {
                vec![BridgeEffect::Send(answer(&id, initialize_result(&self.ctx)))]
            }
            CallRoute::Ping => vec![BridgeEffect::Send(answer(&id, json!({})))],
            CallRoute::DisplayMode => {
                vec![BridgeEffect::Send(answer(&id, json!({ "mode": "inline" })))]
            }
            CallRoute::Unsupported => vec![
                BridgeEffect::Send(refuse(&id, METHOD_NOT_FOUND, "method not found")),
                BridgeEffect::Refused(format!("{method}::unsupported")),
            ],
            CallRoute::BadParams => vec![
                BridgeEffect::Send(refuse(&id, INVALID_PARAMS, "invalid params")),
                BridgeEffect::Refused(format!("{method}::bad params")),
            ],
            CallRoute::ToolsCall(call) => self.on_tools_call(id, call, now),
        }
    }

    fn on_tools_call(&mut self, id: Value, call: ToolsCall, now: Instant) -> Vec<BridgeEffect> {
        let ToolsCall { name, arguments } = call;
        let reason = if !self.initialized {
            Some("the view is not ready")
        } else if self.close_sent {
            Some("the widget is closing")
        } else if self.cooldown_until.is_some_and(|until| now < until) {
            Some("cooling down after a denial")
        } else if self.open_ask.is_some() {
            Some("one ask at a time")
        } else {
            None
        };
        if let Some(reason) = reason {
            return vec![
                BridgeEffect::Send(refuse(&id, DENIED, reason)),
                BridgeEffect::Refused(format!("tools/call:{name}:{reason}")),
            ];
        }
        let ask = self.next_ask;
        self.next_ask += 1;
        self.open_ask = Some((ask, id));
        vec![BridgeEffect::Ask { ask, name, arguments }]
    }

    fn on_initialized(&mut self) -> Vec<BridgeEffect> {
        if self.initialized {
            return Vec::new();
        }
        self.initialized = true;
        self.flush_pending()
    }

    /// Sends what is held, in protocol order: input, then result, then a cancellation.
    fn flush_pending(&mut self) -> Vec<BridgeEffect> {
        let mut effects = Vec::new();
        if !self.initialized || self.closed {
            return effects;
        }
        if let Some(arguments) = self.input.take() {
            let params = json!({ "arguments": arguments });
            effects.push(BridgeEffect::Send(notify("ui/notifications/tool-input", params)));
            self.input_sent = true;
        }
        if self.input_sent {
            if let Some(result) = self.result.take() {
                effects.push(BridgeEffect::Send(notify("ui/notifications/tool-result", result)));
                self.result_sent = true;
            }
        }
        if let Some(reason) = self.cancelled.take() {
            let params = json!({ "reason": reason });
            effects.push(BridgeEffect::Send(notify("ui/notifications/tool-cancelled", params)));
        }
        effects
    }

    pub(crate) fn tool_input(&mut self, arguments: Value) -> Vec<BridgeEffect> {
        if self.closed || std::mem::replace(&mut self.input_taken, true) {
            return Vec::new();
        }
        self.input = Some(arguments);
        self.flush_pending()
    }

    pub(crate) fn tool_result(&mut self, result: Value) -> Vec<BridgeEffect> {
        if self.closed || std::mem::replace(&mut self.result_taken, true) {
            return Vec::new();
        }
        self.result = Some(result);
        self.flush_pending()
    }

    pub(crate) fn tool_cancelled(&mut self, reason: &str) -> Vec<BridgeEffect> {
        if self.closed || self.cancelled.is_some() {
            return Vec::new();
        }
        self.cancelled = Some(reason.to_owned());
        self.flush_pending()
    }

    pub(crate) fn answer_ask(
        &mut self,
        ask: u64,
        outcome: AskOutcome,
        now: Instant,
    ) -> Vec<BridgeEffect> {
        if self.closed {
            return Vec::new();
        }
        let id = match self.open_ask.take() {
            Some((open, id)) if open == ask => id,
            other => {
                self.open_ask = other;
                return Vec::new();
            }
        };
        match outcome {
            AskOutcome::Result(value) => {
                self.denials = 0;
                vec![BridgeEffect::Send(answer(&id, value))]
            }
            AskOutcome::Refused(text) => vec![BridgeEffect::Send(refuse(&id, DENIED, &text))],
            AskOutcome::Denied => {
                self.denials += 1;
                self.cooldown_until = Some(now + COOLDOWN);
                let mut effects = vec![BridgeEffect::Send(refuse(&id, DENIED, "denied"))];
                if self.denials >= MAX_DENIALS && !self.close_sent {
                    effects.extend(self.close("three denials"));
                }
                effects
            }
        }
    }

    pub(crate) fn teardown(&mut self) -> Vec<BridgeEffect> {
        if self.closed {
            return Vec::new();
        }
        self.closed = true;
        self.open_ask = None;
        if !self.initialized {
            // The view never finished its handshake, so it has nothing to save.
            return vec![BridgeEffect::TornDown];
        }
        let id = json!(self.next_request);
        self.next_request += 1;
        let text = request(&id, "ui/resource-teardown", json!({}));
        self.teardown_id = Some(id);
        vec![BridgeEffect::Send(text)]
    }
}

enum Admit {
    Pass,
    Drop,
    Close,
}

#[cfg(test)]
mod tests {
    use super::*;

    // Failure list: the bridge may
    // - send anything host-initiated before `initialized` (answers to the view's own requests
    //   are allowed);
    // - send tool-input twice;
    // - send tool-result before tool-input;
    // - answer `ui/initialize` with capabilities beyond `serverTools`;
    // - let `tools/call` through before `initialized`;
    // - let a second call through while one is asked;
    // - fail to give every unsupported request a -32601;
    // - act on `notifications/message`, or answer a malformed request;
    // - answer an unknown ask id;
    // - turn a deny into a result;
    // - fail to cut a flood, or to close a widget that floods for 5 s, or close one that
    //   floods less;
    // - skip the cooldown after a denial, keep it too long, or count a refusal as a denial;
    // - close after fewer or more than three denials in a row;
    // - miss the teardown answer, or take another response (or a request) for it;
    // - after teardown: raise an Ask or answer a call that arrives in the same batch as the
    //   teardown answer, answer an ask, deliver input or result, or emit a second Close;
    // - send a teardown request before `initialized`, or answer a second teardown.

    fn ctx() -> HostContext {
        HostContext { dark: true, width: 640, max_height: 800, host_version: "1.2.3".to_owned() }
    }

    fn ready_bridge(t0: Instant) -> WidgetBridge {
        let mut bridge = WidgetBridge::new(ctx());
        bridge.on_message(&line(1, "ui/initialize", json!({})), t0);
        bridge.on_message(&note("ui/notifications/initialized"), t0);
        bridge
    }

    fn line(id: u64, method: &str, params: Value) -> String {
        request(&json!(id), method, params)
    }

    fn note(method: &str) -> String {
        notify(method, json!({}))
    }

    fn call(id: u64, name: &str) -> String {
        line(id, "tools/call", json!({"name": name, "arguments": {"k": 1}}))
    }

    fn sent(effects: &[BridgeEffect]) -> Vec<Value> {
        effects
            .iter()
            .filter_map(|e| match e {
                BridgeEffect::Send(text) => Some(serde_json::from_str(text).unwrap()),
                _ => None,
            })
            .collect()
    }

    fn error_code(message: &Value) -> Option<i64> {
        message["error"]["code"].as_i64()
    }

    fn ask_id(effects: &[BridgeEffect]) -> Option<u64> {
        effects.iter().find_map(|e| match e {
            BridgeEffect::Ask { ask, .. } => Some(*ask),
            _ => None,
        })
    }

    fn secs(base: Instant, s: f64) -> Instant {
        base + Duration::from_secs_f64(s)
    }

    #[test]
    fn the_handshake_comes_first_and_input_precedes_result() {
        let t0 = Instant::now();
        let mut bridge = WidgetBridge::new(ctx());
        assert!(bridge.tool_input(json!({"a": 1})).is_empty());
        assert!(bridge.tool_result(json!({"content": []})).is_empty());
        let answered = bridge.on_message(&line(1, "ui/initialize", json!({})), t0);
        assert_eq!(answered.len(), 1);
        let messages = sent(&answered);
        assert_eq!(messages[0]["id"], json!(1));
        let capabilities = messages[0]["result"]["hostCapabilities"].as_object().unwrap();
        assert_eq!(capabilities.keys().collect::<Vec<_>>(), ["serverTools"]);
        let delivered = bridge.on_message(&note("ui/notifications/initialized"), t0);
        let messages = sent(&delivered);
        assert_eq!(delivered.len(), 2);
        assert_eq!(messages[0]["method"], "ui/notifications/tool-input");
        assert_eq!(messages[0]["params"], json!({"arguments": {"a": 1}}));
        assert_eq!(messages[1]["method"], "ui/notifications/tool-result");
        assert_eq!(messages[1]["params"], json!({"content": []}));
        assert!(bridge.tool_input(json!({"a": 2})).is_empty());
        assert!(bridge.on_message(&note("ui/notifications/initialized"), t0).is_empty());
    }

    #[test]
    fn a_result_waits_for_its_input_after_the_handshake() {
        let t0 = Instant::now();
        let mut bridge = ready_bridge(t0);
        assert!(bridge.tool_result(json!({"r": 1})).is_empty());
        let delivered = bridge.tool_input(json!({"a": 1}));
        let messages = sent(&delivered);
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0]["method"], "ui/notifications/tool-input");
        assert_eq!(messages[1]["method"], "ui/notifications/tool-result");
    }

    #[test]
    fn a_call_before_initialized_is_refused() {
        let t0 = Instant::now();
        let mut bridge = WidgetBridge::new(ctx());
        let effects = bridge.on_message(&call(3, "go"), t0);
        assert_eq!(error_code(&sent(&effects)[0]), Some(-32000));
        assert!(ask_id(&effects).is_none());
        assert!(effects.iter().any(
            |e| matches!(e, BridgeEffect::Refused(r) if r.starts_with("tools/call:go:"))
        ));
    }

    #[test]
    fn a_call_becomes_one_ask() {
        let t0 = Instant::now();
        let mut bridge = ready_bridge(t0);
        let effects = bridge.on_message(&call(5, "go"), t0);
        assert_eq!(
            effects,
            vec![BridgeEffect::Ask { ask: 1, name: "go".to_owned(), arguments: json!({"k": 1}) }]
        );
    }

    #[test]
    fn a_call_with_bad_params_is_refused() {
        let t0 = Instant::now();
        let mut bridge = ready_bridge(t0);
        let effects = bridge.on_message(&line(5, "tools/call", json!({"arguments": {}})), t0);
        assert_eq!(error_code(&sent(&effects)[0]), Some(-32602));
        assert!(effects.contains(&BridgeEffect::Refused("tools/call::bad params".to_owned())));
        assert!(ask_id(&effects).is_none());
    }

    #[test]
    fn a_second_call_while_one_is_asked_is_refused() {
        let t0 = Instant::now();
        let mut bridge = ready_bridge(t0);
        let first = bridge.on_message(&call(5, "go"), t0);
        let ask = ask_id(&first).unwrap();
        let second = bridge.on_message(&call(6, "go"), t0);
        let messages = sent(&second);
        assert_eq!((messages[0]["id"].clone(), error_code(&messages[0])), (json!(6), Some(-32000)));
        assert!(second.contains(&BridgeEffect::Refused(
            "tools/call:go:one ask at a time".to_owned()
        )));
        assert!(ask_id(&second).is_none());
        // The first ask is still the open one.
        let settled = bridge.answer_ask(ask, AskOutcome::Result(json!({"ok": 1})), t0);
        assert_eq!(sent(&settled)[0]["id"], json!(5));
    }

    #[test]
    fn unsupported_requests_get_method_not_found() {
        let t0 = Instant::now();
        let mut bridge = ready_bridge(t0);
        for method in [
            "resources/read",
            "ui/open-link",
            "ui/message",
            "ui/update-model-context",
            "ui/download-file",
            "sampling/createMessage",
        ] {
            let effects = bridge.on_message(&line(9, method, json!({})), t0);
            let messages = sent(&effects);
            assert_eq!(messages.len(), 1, "{method}");
            assert_eq!(error_code(&messages[0]), Some(-32601), "{method}");
            assert_eq!(messages[0]["id"], json!(9), "{method}");
            assert!(effects.contains(&BridgeEffect::Refused(format!("{method}::unsupported"))));
            assert!(ask_id(&effects).is_none(), "{method}");
        }
    }

    #[test]
    fn logs_and_malformed_requests_get_nothing() {
        let t0 = Instant::now();
        let mut bridge = ready_bridge(t0);
        let log = notify("notifications/message", json!({"level": "info", "data": "x"}));
        assert!(bridge.on_message(&log, t0).is_empty());
        assert!(bridge.on_message(r#"{"id":4,"method":"ping"}"#, t0).is_empty());
        assert!(bridge.on_message("not json", t0).is_empty());
    }

    #[test]
    fn display_mode_answers_inline() {
        let t0 = Instant::now();
        let mut bridge = ready_bridge(t0);
        let effects = bridge
            .on_message(&line(2, "ui/request-display-mode", json!({"mode": "fullscreen"})), t0);
        assert_eq!(sent(&effects)[0]["result"], json!({"mode": "inline"}));
    }

    #[test]
    fn size_changes_become_a_resize() {
        let t0 = Instant::now();
        let mut bridge = ready_bridge(t0);
        let size = notify("ui/notifications/size-changed", json!({"height": 123.2}));
        assert_eq!(bridge.on_message(&size, t0), vec![BridgeEffect::Resize(124)]);
    }

    #[test]
    fn an_ask_resolves_to_result_or_denial() {
        let t0 = Instant::now();
        let asked = |bridge: &mut WidgetBridge| {
            ask_id(&bridge.on_message(&call(5, "go"), t0)).unwrap()
        };
        let mut bridge = ready_bridge(t0);
        let ask = asked(&mut bridge);
        let denied = sent(&bridge.answer_ask(ask, AskOutcome::Denied, t0));
        assert_eq!((error_code(&denied[0]), denied[0].get("result")), (Some(-32000), None));

        let mut bridge = ready_bridge(t0);
        let ask = asked(&mut bridge);
        let refused = bridge.answer_ask(ask, AskOutcome::Refused("too big".to_owned()), t0);
        assert_eq!(sent(&refused)[0]["error"]["message"], "too big");
        assert_eq!(error_code(&sent(&refused)[0]), Some(-32000));

        let mut bridge = ready_bridge(t0);
        let ask = asked(&mut bridge);
        let result = sent(&bridge.answer_ask(ask, AskOutcome::Result(json!({"v": 7})), t0));
        assert_eq!(result[0]["id"], json!(5));
        assert_eq!(result[0]["result"], json!({"v": 7}));

        let mut bridge = ready_bridge(t0);
        let ask = asked(&mut bridge);
        assert!(bridge.answer_ask(ask + 1, AskOutcome::Denied, t0).is_empty());
        // The unknown answer changed nothing: the real ask is still the open one.
        assert!(!bridge.answer_ask(ask, AskOutcome::Denied, t0).is_empty());
    }

    #[test]
    fn a_flood_is_cut_at_fifty_a_second() {
        let t0 = Instant::now();
        let mut bridge = ready_bridge(t0);
        let t1 = secs(t0, 3.0);
        let answered: usize = (0..60)
            .map(|i| sent(&bridge.on_message(&line(100 + i, "ping", json!({})), t1)).len())
            .sum();
        assert_eq!(answered, 50);
    }

    #[test]
    fn a_widget_over_the_rate_for_five_seconds_is_closed() {
        let t0 = Instant::now();
        let flood = |bridge: &mut WidgetBridge, at: Instant| {
            (0..60)
                .flat_map(|i| bridge.on_message(&line(i, "ping", json!({})), at))
                .filter(|e| matches!(e, BridgeEffect::Close(_)))
                .collect::<Vec<_>>()
        };
        let mut bridge = WidgetBridge::new(ctx());
        for second in 0..4 {
            assert!(flood(&mut bridge, secs(t0, f64::from(second))).is_empty());
        }
        assert_eq!(
            flood(&mut bridge, secs(t0, 4.0)),
            vec![BridgeEffect::Close("over the rate limit for 5 s".to_owned())]
        );

        let mut bridge = WidgetBridge::new(ctx());
        for second in 0..4 {
            assert!(flood(&mut bridge, secs(t0, f64::from(second))).is_empty());
        }
        let quiet = bridge.on_message(&line(1, "ping", json!({})), secs(t0, 4.0));
        assert!(!quiet.iter().any(|e| matches!(e, BridgeEffect::Close(_))));
        assert!(flood(&mut bridge, secs(t0, 5.0)).is_empty());
    }

    #[test]
    fn a_denial_cools_down_and_three_close_the_widget() {
        let t0 = Instant::now();
        let mut bridge = ready_bridge(t0);
        let mut id = 10;
        let mut at = t0;
        // Asks, then settles it at `at`; returns the settle's effects.
        let mut settle = |bridge: &mut WidgetBridge, at: Instant, outcome: AskOutcome| {
            id += 1;
            let ask = ask_id(&bridge.on_message(&call(id, "go"), at)).expect("an ask");
            bridge.answer_ask(ask, outcome, at)
        };
        let closes = |effects: &[BridgeEffect]| {
            effects.contains(&BridgeEffect::Close("three denials".to_owned()))
        };

        // Denied at t, then t+9.9 s refused with no ask, t+10 s asks.
        assert!(!closes(&settle(&mut bridge, at, AskOutcome::Denied)));
        let cooling = bridge.on_message(&call(900, "go"), secs(at, 9.9));
        assert!(ask_id(&cooling).is_none());
        assert_eq!(error_code(&sent(&cooling)[0]), Some(-32000));
        assert!(cooling.contains(&BridgeEffect::Refused(
            "tools/call:go:cooling down after a denial".to_owned()
        )));
        at = secs(at, 10.0);
        // Second denial, then a Result resets the count.
        assert!(!closes(&settle(&mut bridge, at, AskOutcome::Denied)));
        at = secs(at, 10.0);
        assert!(!closes(&settle(&mut bridge, at, AskOutcome::Result(json!({})))));
        // Two more denials in a row do not close: the reset happened.
        for _ in 0..2 {
            at = secs(at, 10.0);
            assert!(!closes(&settle(&mut bridge, at, AskOutcome::Denied)));
        }
        // A refusal neither cools down nor counts: the next call asks at once.
        at = secs(at, 10.0);
        let refused = settle(&mut bridge, at, AskOutcome::Refused("no".to_owned()));
        assert!(!closes(&refused));
        let ask = ask_id(&bridge.on_message(&call(901, "go"), at)).expect("an ask at once");
        // That makes the third denial in a row, which closes.
        assert!(closes(&bridge.answer_ask(ask, AskOutcome::Denied, at)));
    }

    #[test]
    fn teardown_waits_for_its_own_answer() {
        let t0 = Instant::now();
        let mut bridge = ready_bridge(t0);
        let effects = bridge.teardown();
        let messages = sent(&effects);
        assert_eq!(effects.len(), 1);
        assert_eq!(messages[0]["method"], "ui/resource-teardown");
        let id = messages[0]["id"].clone();
        assert!(!id.is_null());
        let other = json!({"jsonrpc": "2.0", "id": "other", "result": {}}).to_string();
        assert!(bridge.on_message(&other, t0).is_empty());
        // A request carrying the id is not an answer.
        let imposter = line(id.as_u64().unwrap_or(0), "ping", json!({}));
        assert!(!bridge.on_message(&imposter, t0).contains(&BridgeEffect::TornDown));
        let own = json!({"jsonrpc": "2.0", "id": id, "result": {}}).to_string();
        assert_eq!(bridge.on_message(&own, t0), vec![BridgeEffect::TornDown]);
    }

    #[test]
    fn a_torn_down_widget_answers_nothing() {
        let t0 = Instant::now();
        let mut bridge = ready_bridge(t0);
        let open = ask_id(&bridge.on_message(&call(5, "go"), t0)).unwrap();
        let sent_teardown = bridge.teardown();
        let id = sent(&sent_teardown)[0]["id"].clone();
        assert!(bridge.teardown().is_empty());
        // The call arrives before the teardown answer, in the same batch.
        assert!(bridge.on_message(&call(6, "go"), t0).is_empty());
        assert!(bridge.on_message(&line(7, "ping", json!({})), t0).is_empty());
        let own = json!({"jsonrpc": "2.0", "id": id, "result": {}}).to_string();
        assert_eq!(bridge.on_message(&own, t0), vec![BridgeEffect::TornDown]);
        // And after the answer, still nothing.
        assert!(bridge.on_message(&call(8, "go"), t0).is_empty());
        assert!(bridge.answer_ask(open, AskOutcome::Result(json!({})), t0).is_empty());
        assert!(bridge.tool_input(json!({"a": 1})).is_empty());
        assert!(bridge.tool_result(json!({})).is_empty());
        assert!(bridge.tool_cancelled("x").is_empty());
    }

    #[test]
    fn a_denial_after_teardown_cannot_close_again() {
        let t0 = Instant::now();
        let mut bridge = ready_bridge(t0);
        for i in 0..2_u32 {
            let at = secs(t0, 10.0 * f64::from(i));
            let ask = ask_id(&bridge.on_message(&call(10 + u64::from(i), "go"), at)).unwrap();
            bridge.answer_ask(ask, AskOutcome::Denied, at);
        }
        let at = secs(t0, 20.0);
        let ask = ask_id(&bridge.on_message(&call(30, "go"), at)).unwrap();
        bridge.teardown();
        assert!(bridge.answer_ask(ask, AskOutcome::Denied, at).is_empty());
    }

    #[test]
    fn teardown_before_initialized_sends_nothing_and_is_done() {
        let t0 = Instant::now();
        let mut bridge = WidgetBridge::new(ctx());
        assert_eq!(bridge.teardown(), vec![BridgeEffect::TornDown]);
        assert!(bridge.teardown().is_empty());
        assert!(bridge.on_message(&line(1, "ui/initialize", json!({})), t0).is_empty());
    }
}
