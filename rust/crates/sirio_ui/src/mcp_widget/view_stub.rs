//! Off Linux there is no widget webview: `widget_webview_available()` is false, so the chat
//! never builds one. This stub keeps the chat's code free of `cfg`.

use gpui::{Bounds, Context, EventEmitter, Pixels, Window};
use serde_json::Value;
use sirio_chat::mcp_apps::{bridge::HostContext, document::WidgetDocument};

use super::{AskOutcome, WidgetEvent, WidgetPhase, WidgetSnapshot};

pub struct McpWidget;

impl EventEmitter<WidgetEvent> for McpWidget {}

impl McpWidget {
    pub fn new(
        _doc: WidgetDocument,
        _ctx: HostContext,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Self {
        Self
    }

    pub fn tool_input(&mut self, _arguments: Value, _cx: &mut Context<Self>) {}

    pub fn tool_result(&mut self, _result: Value, _cx: &mut Context<Self>) {}

    pub fn tool_cancelled(&mut self, _cx: &mut Context<Self>) {}

    pub fn answer_ask(&mut self, _ask: u64, _outcome: AskOutcome, _cx: &mut Context<Self>) {}

    pub fn place(&mut self, _at: Option<Bounds<Pixels>>, _cx: &mut Context<Self>) {}

    pub fn close(&mut self, _cx: &mut Context<Self>) {}

    pub fn record_refusal(&mut self, _entry: String) {}

    pub fn snapshot(&self) -> WidgetSnapshot {
        WidgetSnapshot {
            phase: WidgetPhase::Closed,
            height: 0,
            mapped: false,
            asks: 0,
            answered: 0,
            refused: Vec::new(),
            xid: None,
            input_delivered: false,
            result_delivered: false,
        }
    }
}
