//! A widget's own tool calls ("asks"), answered through the question dock.
//!
//! A widget may ask the host to run a tool of its own server. The ask becomes a card in the
//! dock that only a click answers: Reject first, Allow second, the arguments shown whole.
//! Every way the card can end answers the widget exactly once: Allow runs the call off the UI
//! thread and answers with its result; Reject, Escape, a dismiss, the five-minute timer, the
//! slot closing and the session ending answer with a denial. A model turn's end touches
//! nothing: the ask is the widget's own.
//!
//! Every answer goes through `answer_widget_ask`, and under test it is also recorded there,
//! because no `McpWidget` can be built without a window system.

#[cfg(test)]
use std::{cell::RefCell, rc::Rc};

use gpui::{App, AppContext as _, Context, Task};
use serde_json::Value;
use sirio_chat::{
    AppCallRefusal, McpOrigin, PERMISSION_TIMEOUT, PermissionDetail, PermissionMcpServer,
    app_call_name, call_tool_result,
};

use super::{
    AnswerOption, Chat, Entry, visible_text::visible_text, widget_slot::SlotPhase,
};
use crate::mcp_widget::AskOutcome;

/// Ids of the cards a widget's ask is shown in start here, far above any id the CLI hands out.
pub(crate) const WIDGET_ASK_BASE: u64 = 1 << 62;

/// The card shows the arguments whole, so it refuses what it could not show.
const MAX_ARGUMENT_CHARS: usize = 4000;
const NO_SESSION: &str = "the session is gone";
const WIDGET_CLOSED: &str = "the widget is closed";
const TOO_LONG: &str = "the arguments are too long to show";
const DESCRIPTION: &str = "A widget asks to run this tool. The agent did not ask.";

/// What `expire_unanswered` does with the cards of widget asks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum WidgetAsks {
    /// A model turn ended: the widget's ask is its own, and stays open.
    Keep,
    /// The session is gone: nothing can run the call.
    Deny,
}

struct OpenAsk {
    request_id: u64,
    tool_use_id: String,
    /// The widget's id for the ask, which its answer carries back.
    ask: u64,
    full_name: String,
    arguments: Value,
    /// Cancelled by dropping it: Allow, Reject and every close do.
    timer: Option<Task<()>>,
    /// Allow was chosen and the call is running: no second choice starts another.
    calling: bool,
}

#[derive(Default)]
pub(super) struct AskState {
    open: Vec<OpenAsk>,
    issued: u64,
    /// Every answer given to a widget, in order: `(tool_use_id, ask, outcome)`.
    #[cfg(test)]
    pub(super) answers: Rc<RefCell<Vec<(String, u64, AskOutcome)>>>,
    /// How many calls Allow started.
    #[cfg(test)]
    pub(super) calls: Rc<RefCell<u32>>,
    /// With no client a test's call finishes at once; this keeps it running instead.
    #[cfg(test)]
    pub(super) hold_calls: bool,
}

fn refusal_reason(refusal: AppCallRefusal) -> &'static str {
    match refusal {
        AppCallRefusal::ServerGone => "the server is gone",
        AppCallRefusal::NotListed => "not a tool of this server",
        AppCallRefusal::NotForApp => "not callable from a widget",
        AppCallRefusal::Ambiguous => "this name is ambiguous",
        AppCallRefusal::NameChanged => "the tool's name would change on the way",
    }
}

fn origin_label(origin: &McpOrigin) -> String {
    match origin {
        McpOrigin::ClaudeAi => "claudeai".to_owned(),
        McpOrigin::Plugin => "plugin".to_owned(),
        McpOrigin::User => "user".to_owned(),
        McpOrigin::Project => "project".to_owned(),
        McpOrigin::Other(word) => visible_text(word, false),
    }
}

impl Chat {
    /// A widget asks to call `name` with `arguments`: a refusal it can be told at once, or a
    /// card in the dock.
    pub(super) fn widget_asked(
        &mut self,
        tool_use_id: &str,
        ask: u64,
        name: String,
        arguments: Value,
        cx: &mut Context<Self>,
    ) {
        let Some(slot) = self.widgets.slot(tool_use_id).filter(|s| s.phase != SlotPhase::Closed)
        else {
            self.answer_widget_ask(tool_use_id, ask, AskOutcome::Refused(WIDGET_CLOSED.into()), cx);
            return;
        };
        let (server, raw_server, origin) =
            (slot.server.clone(), slot.raw_server.clone(), slot.origin.clone());
        let servers = self
            .widgets
            .known_servers()
            .or_else(|| self.client.as_ref().and_then(|client| client.mcp_servers()));
        let full_name = match servers {
            Some(list) => app_call_name(&list, &raw_server, &origin, &name),
            None => Err(AppCallRefusal::ServerGone),
        };
        let full_name = match full_name {
            Ok(full_name) => full_name,
            Err(refusal) => {
                self.refuse_widget_ask(tool_use_id, ask, &name, refusal_reason(refusal), cx);
                return;
            }
        };
        let pretty = serde_json::to_string_pretty(&arguments).unwrap_or_default();
        let body = visible_text(&pretty, true);
        if body.chars().count() > MAX_ARGUMENT_CHARS {
            self.refuse_widget_ask(tool_use_id, ask, &name, TOO_LONG, cx);
            return;
        }
        let request_id = WIDGET_ASK_BASE + self.widgets.asks.issued;
        self.widgets.asks.issued += 1;
        self.push_entry(Entry::Permission {
            request_id,
            title: format!("Widget · {server} · {}", visible_text(&name, false)),
            prompt: body,
            options: vec![
                AnswerOption {
                    id: "reject_once".into(),
                    label: "Reject".into(),
                    description: None,
                    is_rejection: true,
                },
                AnswerOption {
                    id: "allow_once".into(),
                    label: "Allow".into(),
                    description: None,
                    is_rejection: false,
                },
            ],
            text_input: None,
            is_question: false,
            resolved: None,
            expired: false,
            dismissed: false,
            withdrawn: false,
            detail: Some(PermissionDetail {
                description: Some(DESCRIPTION.into()),
                mcp_server: Some(PermissionMcpServer {
                    name: server,
                    source: origin_label(&origin),
                }),
                default_to_no: true,
                ..PermissionDetail::default()
            }),
        });
        let timer = cx.spawn(async move |this, cx| {
            cx.background_executor().timer(PERMISSION_TIMEOUT).await;
            let _ = this.update(cx, |chat, cx| chat.deny_widget_ask(request_id, cx));
        });
        self.widgets.asks.open.push(OpenAsk {
            request_id,
            tool_use_id: tool_use_id.to_owned(),
            ask,
            full_name,
            arguments,
            timer: Some(timer),
            calling: false,
        });
        cx.notify();
    }

    /// The widget is told no, and keeps a note of why.
    fn refuse_widget_ask(
        &mut self,
        tool_use_id: &str,
        ask: u64,
        name: &str,
        reason: &str,
        cx: &mut Context<Self>,
    ) {
        let entry = format!("tools/call:{}:{reason}", visible_text(name, false));
        if let Some(widget) = self.widgets.slot(tool_use_id).and_then(|slot| slot.widget.clone()) {
            widget.update(cx, |widget, _| widget.record_refusal(entry));
        }
        self.answer_widget_ask(tool_use_id, ask, AskOutcome::Refused(reason.to_owned()), cx);
    }

    /// The one place a widget is answered. A widget closed since answers nothing, so every
    /// close path denies and expires its card here first.
    fn answer_widget_ask(
        &mut self,
        tool_use_id: &str,
        ask: u64,
        outcome: AskOutcome,
        cx: &mut App,
    ) {
        #[cfg(test)]
        self.widgets.asks.answers.borrow_mut().push((tool_use_id.to_owned(), ask, outcome.clone()));
        let Some(widget) = self.widgets.slot(tool_use_id).and_then(|slot| slot.widget.clone())
        else {
            return;
        };
        widget.update(cx, |widget, cx| widget.answer_ask(ask, outcome, cx));
        self.refresh_snapshot(tool_use_id, cx);
    }

    /// The card shows no answer any more: expired, unless the user already chose.
    fn expire_widget_card(&mut self, request_id: u64) {
        let found = self.entries.iter_mut().enumerate().rev().find_map(|(index, entry)| {
            match entry {
                Entry::Permission { request_id: id, resolved: None, expired, .. }
                    if *id == request_id =>
                {
                    *expired = true;
                    Some(index)
                }
                _ => None,
            }
        });
        if let Some(index) = found {
            self.remeasure_entry(index);
        }
    }

    /// A click on Reject or Allow, the only answers the card takes.
    pub(super) fn widget_card_chosen(
        &mut self,
        request_id: u64,
        option: &AnswerOption,
        cx: &mut Context<Self>,
    ) {
        let Some(position) =
            self.widgets.asks.open.iter().position(|open| open.request_id == request_id)
        else {
            return;
        };
        if self.widgets.asks.open[position].calling {
            return;
        }
        let found = self.entries.iter_mut().enumerate().rev().find_map(|(index, entry)| match entry
        {
            Entry::Permission { request_id: id, resolved, expired: false, .. }
                if *id == request_id && resolved.is_none() =>
            {
                *resolved = Some(option.label.clone());
                Some(index)
            }
            _ => None,
        });
        if let Some(index) = found {
            self.remeasure_entry(index);
        }
        self.clear_question_answer_focus();
        if option.is_rejection {
            let open = self.widgets.asks.open.remove(position);
            self.answer_widget_ask(&open.tool_use_id, open.ask, AskOutcome::Denied, cx);
            cx.notify();
            return;
        }
        // Allow: the timer's job is done, and the call runs off the UI thread.
        let open = &mut self.widgets.asks.open[position];
        open.timer = None;
        open.calling = true;
        #[cfg(test)]
        {
            *self.widgets.asks.calls.borrow_mut() += 1;
        }
        let open = &self.widgets.asks.open[position];
        let (full_name, arguments) = (open.full_name.clone(), open.arguments.clone());
        match self.client.as_ref().and_then(|client| client.mcp_control()) {
            #[cfg(test)]
            None if self.widgets.asks.hold_calls => {}
            None => self.widget_call_returned(request_id, Err(NO_SESSION.to_owned()), cx),
            Some(control) => {
                cx.spawn(async move |this, cx| {
                    let outcome = cx
                        .background_spawn(async move {
                            control.call_tool_after_user_allowed(&full_name, arguments)
                        })
                        .await
                        .map_err(|error| format!("{error:#}"));
                    let _ = this.update(cx, |chat, cx| {
                        chat.widget_call_returned(request_id, outcome, cx)
                    });
                })
                .detach();
            }
        }
        cx.notify();
    }

    /// The call an Allow started has finished. The widget gets the answer only while the ask
    /// is still open: a close in the meantime answered it with a denial, and a closed widget
    /// would ignore this anyway.
    pub(super) fn widget_call_returned(
        &mut self,
        request_id: u64,
        result: Result<Value, String>,
        cx: &mut Context<Self>,
    ) {
        let Some(position) =
            self.widgets.asks.open.iter().position(|open| open.request_id == request_id)
        else {
            return;
        };
        let open = self.widgets.asks.open.remove(position);
        let live = self
            .widgets
            .slot(&open.tool_use_id)
            .is_some_and(|slot| slot.phase != SlotPhase::Closed);
        if !live {
            return;
        }
        let result = match result {
            Ok(value) => call_tool_result(&value, false),
            Err(text) => call_tool_result(&Value::String(text), true),
        };
        self.answer_widget_ask(&open.tool_use_id, open.ask, AskOutcome::Result(result), cx);
        cx.notify();
    }

    /// Escape, a dismiss, or the timer: the ask is denied and the card expires.
    pub(super) fn deny_widget_ask(&mut self, request_id: u64, cx: &mut Context<Self>) {
        self.deny_widget_asks_where(|open| open.request_id == request_id, cx);
    }

    /// The slot closes: its open ask is denied.
    pub(super) fn deny_widget_asks_of(&mut self, tool_use_id: &str, cx: &mut Context<Self>) {
        self.deny_widget_asks_where(|open| open.tool_use_id == tool_use_id, cx);
    }

    /// The session ends, a transcript is replaced: every open ask is denied.
    pub(super) fn deny_all_widget_asks(&mut self, cx: &mut Context<Self>) {
        self.deny_widget_asks_where(|_| true, cx);
    }

    fn deny_widget_asks_where(&mut self, which: impl Fn(&OpenAsk) -> bool, cx: &mut Context<Self>) {
        let (denied, kept): (Vec<_>, Vec<_>) =
            std::mem::take(&mut self.widgets.asks.open).into_iter().partition(which);
        self.widgets.asks.open = kept;
        if denied.is_empty() {
            return;
        }
        for open in denied {
            self.expire_widget_card(open.request_id);
            self.answer_widget_ask(&open.tool_use_id, open.ask, AskOutcome::Denied, cx);
        }
        self.clear_question_answer_focus();
        cx.notify();
    }

    /// The chat entity is going away: every ask is denied and every widget closed, while the
    /// widget entities are still reachable.
    pub(super) fn release_widgets(&mut self, cx: &mut App) {
        for open in std::mem::take(&mut self.widgets.asks.open) {
            self.answer_widget_ask(&open.tool_use_id, open.ask, AskOutcome::Denied, cx);
        }
        for widget in self.widgets.entities() {
            widget.update(cx, |widget, cx| widget.close(cx));
        }
    }
}

#[cfg(test)]
mod tests {
    use gpui::{TestAppContext, VisualTestContext};
    use serde_json::json;
    use sirio_chat::{
        McpOrigin, McpServer, McpState, McpTool, PERMISSION_TIMEOUT, ToolUi, UiToolRef,
    };

    use super::*;
    use crate::chat::tests::refresh_frame;
    use crate::chat::testing::{offline_chat, prepare};
    use crate::chat::widget_lookup::Held;
    use crate::chat::widget_slot::{Slot, SlotPhase};
    use crate::chat::Entry;
    use crate::mcp_widget::WidgetEvent;

    fn server() -> McpServer {
        McpServer {
            name: "s".into(),
            status: McpState::Connected,
            origin: McpOrigin::User,
            tools: vec![
                McpTool { name: "do".into(), ui: None },
                McpTool {
                    name: "hidden".into(),
                    ui: Some(ToolUi { resource_uri: None, for_model: true, for_app: false }),
                },
            ],
        }
    }

    fn hit() -> UiToolRef {
        UiToolRef {
            server: "s".into(),
            origin: McpOrigin::User,
            tool: "show".into(),
            resource_uri: "ui://s/show".into(),
        }
    }

    type Answers = Rc<RefCell<Vec<(String, u64, AskOutcome)>>>;

    /// A chat with the server list seeded and the shared record of every answer.
    fn ready(chat: &mut Chat) -> Answers {
        chat.widgets.seed_test_servers(vec![server()]);
        chat.widgets.asks.answers.clone()
    }

    fn slot(chat: &mut Chat, id: &str, phase: SlotPhase) {
        let mut slot = Slot::resolving(id.into(), 1, &hit(), Held::new(json!({})));
        slot.phase = phase;
        chat.widgets.insert_test_slot(slot);
    }

    /// Opens an ask of the slot `id` and returns the id of its card.
    fn open(chat: &mut Chat, id: &str, cx: &mut Context<Chat>) -> u64 {
        slot(chat, id, SlotPhase::Resolving);
        ask_on(chat, id, cx)
    }

    fn ask_on(chat: &mut Chat, id: &str, cx: &mut Context<Chat>) -> u64 {
        chat.widget_asked(id, 7, "do".into(), json!({"key": "value"}), cx);
        card(chat).unwrap_or_else(|| panic!("no card for {id}"))
    }

    fn card(chat: &Chat) -> Option<u64> {
        chat.entries.iter().rev().find_map(|entry| match entry {
            Entry::Permission { request_id, resolved: None, expired: false, .. }
                if *request_id >= WIDGET_ASK_BASE =>
            {
                Some(*request_id)
            }
            _ => None,
        })
    }

    fn option(chat: &Chat, request_id: u64, rejection: bool) -> AnswerOption {
        chat.entries
            .iter()
            .find_map(|entry| match entry {
                Entry::Permission { request_id: id, options, .. } if *id == request_id => {
                    options.iter().find(|option| option.is_rejection == rejection).cloned()
                }
                _ => None,
            })
            .expect("the card offers the option")
    }

    fn expired(chat: &Chat, request_id: u64) -> bool {
        chat.entries.iter().any(|entry| {
            matches!(entry, Entry::Permission { request_id: id, expired: true, .. }
                if *id == request_id)
        })
    }

    fn phase(chat: &Chat, id: &str) -> SlotPhase {
        chat.widgets.slot(id).expect("the slot exists").phase
    }

    fn outcomes(answers: &Answers) -> Vec<(String, AskOutcome)> {
        answers.borrow().iter().map(|(id, _, outcome)| (id.clone(), outcome.clone())).collect()
    }

    fn denied(answers: &Answers, id: &str) -> usize {
        outcomes(answers)
            .iter()
            .filter(|(slot, outcome)| slot == id && *outcome == AskOutcome::Denied)
            .count()
    }

    fn window(cx: &mut TestAppContext) -> (gpui::Entity<Chat>, &mut VisualTestContext) {
        prepare(cx);
        cx.add_window_view(|_, cx| offline_chat(cx))
    }

    // How it can fail, written first. An ask that no path answers leaves the widget waiting
    // for ever and its denial count short; an ask answered twice (a click, then a close)
    // gives the widget two answers to one id.
    //
    // 1. Reject, Escape or a dismiss answers a widget ask through the CLI's permission
    //    channel, or answers nothing, or answers with a result.
    // 2. The five-minute timer never fires, fires for an ask already answered, or answers
    //    with anything but a denial.
    // 3. A model turn ending closes the widget or expires its card.
    // 4. The CLI going away (a transport error) leaves the ask open, or leaves the widget
    //    live with nothing behind it.
    // 5. Opening a history session or a new conversation drops the slots and leaves their
    //    asks unanswered; the cap closes the oldest widget and leaves its ask unanswered,
    //    with a card an Allow can still run.
    // 6. A widget that closes itself leaves its card answerable.
    // 7. A restart leaves the old widgets live in the new session, or builds a page read for
    //    the old one.
    // 8. The chat entity being released leaves its asks unanswered.
    // 9. Any path above answers with a result instead of a denial.
    #[gpui::test]
    async fn a_widget_ask_is_denied_on_every_close_but_allow(cx: &mut TestAppContext) {
        let (chat, cx) = window(cx);
        let answers = chat.update(cx, |chat, _| ready(chat));

        // Reject.
        let reject = chat.update(cx, |chat, cx| {
            let request = open(chat, "reject", cx);
            let option = option(chat, request, true);
            chat.respond_permission(request, &option, cx);
            request
        });
        assert_eq!(denied(&answers, "reject"), 1);
        chat.read_with(cx, |chat, _| assert!(card(chat).is_none() && !expired(chat, reject)));

        // Escape.
        chat.update(cx, |chat, cx| {
            let request = open(chat, "escape", cx);
            chat.cancel_question(request, cx);
            assert!(expired(chat, request));
        });
        assert_eq!(denied(&answers, "escape"), 1);

        // Dismiss.
        chat.update(cx, |chat, cx| {
            let request = open(chat, "dismiss", cx);
            chat.dismiss_permission(request, cx);
            assert!(expired(chat, request));
        });
        assert_eq!(denied(&answers, "dismiss"), 1);

        // The timer.
        let timed = chat.update(cx, |chat, cx| open(chat, "timer", cx));
        cx.executor().advance_clock(PERMISSION_TIMEOUT / 2);
        cx.run_until_parked();
        assert_eq!(denied(&answers, "timer"), 0, "the timer has not run out");
        cx.executor().advance_clock(PERMISSION_TIMEOUT);
        cx.run_until_parked();
        assert_eq!(denied(&answers, "timer"), 1);
        chat.read_with(cx, |chat, _| assert!(expired(chat, timed)));

        // A turn's end keeps the ask, the card and the widget.
        let kept = chat.update(cx, |chat, cx| {
            let request = open(chat, "turn", cx);
            chat.streaming = true;
            chat.handle_event(
                sirio_chat::ChatEvent::TurnEnded { stop_reason: "EndTurn".into() },
                cx,
            );
            request
        });
        chat.read_with(cx, |chat, _| {
            assert_eq!(card(chat), Some(kept), "the card is still open");
            assert_eq!(phase(chat, "turn"), SlotPhase::Resolving);
        });
        assert_eq!(denied(&answers, "turn"), 0);
        chat.update(cx, |chat, cx| chat.cancel_question(kept, cx));
        assert_eq!(denied(&answers, "turn"), 1);

        // The cap closes the oldest widget of seven.
        let capped = chat.update(cx, |chat, cx| {
            for index in 0..7 {
                slot(chat, &format!("cap{index}"), SlotPhase::Loading);
            }
            let request = ask_on(chat, "cap0", cx);
            chat.cap_live_widgets(cx);
            request
        });
        assert_eq!(denied(&answers, "cap0"), 1);
        chat.read_with(cx, |chat, _| {
            assert!(expired(chat, capped));
            assert_eq!(phase(chat, "cap0"), SlotPhase::Closed);
            assert_eq!(phase(chat, "cap1"), SlotPhase::Loading);
        });

        // The widget closes itself.
        chat.update(cx, |chat, cx| {
            let request = open(chat, "self", cx);
            chat.widget_event("self", &WidgetEvent::Closed("three denials".into()), cx);
            assert!(expired(chat, request));
            assert_eq!(phase(chat, "self"), SlotPhase::Closed);
        });
        assert_eq!(denied(&answers, "self"), 1);

        // The CLI goes away mid-turn.
        chat.update(cx, |chat, cx| {
            let request = open(chat, "transport", cx);
            chat.streaming = true;
            chat.handle_event(sirio_chat::ChatEvent::TransportError("gone".into()), cx);
            assert!(expired(chat, request));
            assert_eq!(phase(chat, "transport"), SlotPhase::Closed);
        });
        assert_eq!(denied(&answers, "transport"), 1);

        // A restart: the ask is denied, the slot closed, and a page read for the old session
        // is not held for the new one.
        chat.update(cx, |chat, cx| {
            let request = open(chat, "restart", cx);
            slot(chat, "waiting", SlotPhase::Resolving);
            chat.widgets.hold_test_read("waiting");
            chat.connecting = false;
            chat.start_connection(cx);
            assert!(expired(chat, request));
            assert_eq!(phase(chat, "restart"), SlotPhase::Closed);
            assert_eq!(phase(chat, "waiting"), SlotPhase::Closed);
            assert!(!chat.widgets.has_waiting_read());
        });
        assert_eq!(denied(&answers, "restart"), 1);

        // Opening a history session or starting a new conversation replaces the transcript.
        chat.update(cx, |chat, cx| {
            // The restart dropped the server list with the old session.
            ready(chat);
            let request = open(chat, "history", cx);
            chat.close_widgets(cx);
            assert!(expired(chat, request));
            assert!(chat.widgets.slot("history").is_none());
        });
        assert_eq!(denied(&answers, "history"), 1);

        // No answer was given twice.
        let all = outcomes(&answers);
        assert!(all.iter().all(|(_, outcome)| *outcome == AskOutcome::Denied));
        assert_eq!(all.len(), 10);

        // The chat is released with an ask open.
        let shared = chat.update(cx, |chat, _| chat.widgets.asks.answers.clone());
        let doomed = cx.update(|_, app| app.new(|cx| offline_chat(cx)));
        doomed.update(cx, |chat, cx| {
            chat.widgets.asks.answers = shared.clone();
            ready(chat);
            open(chat, "release", cx);
        });
        assert_eq!(denied(&shared, "release"), 0);
        drop(doomed);
        cx.run_until_parked();
        assert_eq!(denied(&shared, "release"), 1);
    }

    // How it can fail, written first. Allow answers Denied (or nothing); it answers twice; a
    // late result of the call reaches a widget that was closed or already answered.
    //
    // This chat has no client, so the call cannot leave the thread: Allow then answers at
    // once with the call's failure as a result. What the test shows is that Allow answers
    // with a result, once, and never with a denial; a result that arrives after any answer
    // or close is dropped; and a close after Allow does not answer again. Whether the call
    // really runs off the thread is for the E2E.
    #[gpui::test]
    async fn an_allowed_widget_ask_is_answered_once_with_a_result(cx: &mut TestAppContext) {
        let (chat, cx) = window(cx);
        let answers = chat.update(cx, |chat, _| ready(chat));
        chat.update(cx, |chat, cx| {
            let request = open(chat, "allow", cx);
            let option = option(chat, request, false);
            chat.respond_permission(request, &option, cx);
            chat.widget_call_returned(request, Ok(json!({"late": true})), cx);
            chat.close_widgets(cx);
        });
        let all = outcomes(&answers);
        assert_eq!(all.len(), 1);
        assert!(matches!(all[0].1, AskOutcome::Result(_)));

        // A call that finishes after its widget was closed is dropped.
        chat.update(cx, |chat, cx| {
            let request = open(chat, "closed", cx);
            chat.cancel_question(request, cx);
            chat.widget_call_returned(request, Ok(json!({"late": true})), cx);
        });
        assert_eq!(outcomes(&answers).len(), 2);
        assert_eq!(denied(&answers, "closed"), 1);
    }

    // How it can fail, written first: a tool the server does not list, one the widget may not
    // call, or arguments too long to show get a card anyway, or no answer; a refusal leaves
    // a card behind.
    #[gpui::test]
    async fn a_widget_ask_that_cannot_be_shown_is_refused_with_no_card(cx: &mut TestAppContext) {
        let (chat, cx) = window(cx);
        let answers = chat.update(cx, |chat, _| ready(chat));
        chat.update(cx, |chat, cx| {
            slot(chat, "a", SlotPhase::Resolving);
            chat.widget_asked("a", 1, "missing".into(), json!({}), cx);
            chat.widget_asked("a", 2, "hidden".into(), json!({}), cx);
            chat.widget_asked("a", 3, "do".into(), json!({"text": "x".repeat(4000)}), cx);
            assert!(card(chat).is_none());
        });
        let reasons: Vec<_> = outcomes(&answers)
            .into_iter()
            .map(|(_, outcome)| match outcome {
                AskOutcome::Refused(reason) => reason,
                other => format!("{other:?}"),
            })
            .collect();
        assert_eq!(
            reasons,
            [
                "not a tool of this server",
                "not callable from a widget",
                "the arguments are too long to show",
            ]
        );
        refresh_frame(cx);
    }

    // How it can fail, written first: the CLI dies between turns, which is when a widget's ask
    // lives, and `event_source_closed` denies only inside `if self.streaming`; or it denies
    // there and leaves the widget live with nothing behind it.
    async fn event_source_closes(cx: &mut TestAppContext, id: &str, streaming: bool) {
        let (chat, cx) = crate::chat::tests::silent_state_chat(cx);
        let answers = chat.update(cx, |chat, _| ready(chat));
        chat.update(cx, |chat, cx| {
            let request = open(chat, id, cx);
            chat.streaming = streaming;
            chat.event_source_closed(cx);
            assert!(expired(chat, request));
            assert_eq!(phase(chat, id), SlotPhase::Closed);
        });
        assert_eq!(denied(&answers, id), 1);
        assert_eq!(outcomes(&answers).len(), 1);
    }

    #[gpui::test]
    async fn a_widget_ask_is_denied_when_the_cli_goes_away_between_turns(
        cx: &mut TestAppContext,
    ) {
        event_source_closes(cx, "idle", false).await;
    }

    #[gpui::test]
    async fn a_widget_ask_is_denied_when_the_cli_goes_away_mid_turn(cx: &mut TestAppContext) {
        event_source_closes(cx, "busy", true).await;
    }

    // How it can fail, written first. A send that fails drops the client and nothing else:
    // the cards stay answerable and the widget mapped, and the event source closing after it
    // returns early (no client) before it denies anything.
    //
    // 1. The client is dropped (as a failed send does) and the ask stays open.
    // 2. `event_source_closed` with no client leaves a later ask open.
    // 3. Either answers twice, or answers with a result.
    //
    // The send itself cannot fail here (no client), so the test drops the client through
    // `lose_client`, the helper the send-failure branch calls.
    #[gpui::test]
    async fn a_lost_client_denies_widget_asks_once_even_after_the_stream_closes(
        cx: &mut TestAppContext,
    ) {
        let (chat, cx) = window(cx);
        let answers = chat.update(cx, |chat, _| ready(chat));
        chat.update(cx, |chat, cx| {
            let request = open(chat, "lost", cx);
            chat.lose_client(cx);
            assert!(expired(chat, request));
            assert_eq!(phase(chat, "lost"), SlotPhase::Closed);
            chat.event_source_closed(cx);
        });
        assert_eq!(outcomes(&answers).len(), 1);
        assert_eq!(denied(&answers, "lost"), 1);
        // An ask opened after the client is gone is denied when the stream closes.
        chat.update(cx, |chat, cx| {
            let request = open(chat, "late", cx);
            chat.event_source_closed(cx);
            assert!(expired(chat, request));
        });
        assert_eq!(denied(&answers, "late"), 1);
        assert_eq!(outcomes(&answers).len(), 2);
    }

    // How it can fail, written first. The control surface (served in release builds) approves
    // a widget's card with no click; a second Allow starts a second call.
    //
    // 1. `control_permission` with `allow_once` answers, or errors after answering.
    // 2. `control_permission` with `reject_once` does not answer Denied, or answers twice.
    // 3. A second Allow while the call runs starts another call.
    // 4. A close while the call runs does not deny it, or the late result is delivered.
    //
    // The guard is shown by counting the calls Allow starts; with no client the test keeps the
    // call running (`hold_calls`) so the ask stays in flight. It proves the chat starts one
    // call per ask; it does not prove what a real `mcp_call` does.
    #[gpui::test]
    async fn a_widget_card_is_approved_only_by_a_click_and_runs_one_call(
        cx: &mut TestAppContext,
    ) {
        let (chat, cx) = window(cx);
        let answers = chat.update(cx, |chat, _| ready(chat));
        chat.update(cx, |chat, cx| {
            let request = open(chat, "ctl", cx);
            assert!(chat.control_permission(request, "allow_once", cx).is_err());
            assert_eq!(*chat.widgets.asks.calls.borrow(), 0);
            assert!(card(chat).is_some(), "the card is still open");
            assert!(chat.control_permission(request, "reject_once", cx).is_ok());
        });
        assert_eq!(outcomes(&answers).len(), 1);
        assert_eq!(denied(&answers, "ctl"), 1);

        chat.update(cx, |chat, cx| {
            chat.widgets.asks.hold_calls = true;
            let request = open(chat, "once", cx);
            let allow = option(chat, request, false);
            chat.respond_permission(request, &allow, cx);
            chat.respond_permission(request, &allow, cx);
            assert_eq!(*chat.widgets.asks.calls.borrow(), 1);
            // The widget closes while the call runs; the late result is dropped.
            chat.cancel_question(request, cx);
            chat.close_widgets(cx);
            chat.widget_call_returned(request, Ok(json!({"late": true})), cx);
        });
        assert_eq!(outcomes(&answers).len(), 2);
        assert_eq!(denied(&answers, "once"), 1);
    }
}
