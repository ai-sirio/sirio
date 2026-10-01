//! Regression tests at the host/component boundaries.
use super::tests::{CHAT_FIXTURE, TempDir, pump_chat_until, refresh_frame};
use super::*;
use bezel::ui::widgets::{ButtonStyle, Buttons};
use gpui::{TestAppContext, VisualTestContext, size};

struct ThemeProbe {
    chat: Entity<Chat>,
}
impl Render for ThemeProbe {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .flex()
            .flex_col()
            .child(
                div()
                    .flex()
                    .child(ely_gpui_component::buttons::Button::new(
                        "ely-theme-probe",
                        "Ely",
                    ))
                    .child(bezel::theme::Theme::of(cx).button(
                        "Bezel",
                        ButtonStyle::Prominent,
                        None,
                    )),
            )
            .child(self.chat.clone())
    }
}

#[gpui::test]
async fn ely_theme_change_preserves_chat_state_and_bezel_palette(cx: &mut TestAppContext) {
    let dir = TempDir::new();
    cx.update(Theme::init);
    cx.update(bezel::ui::input::init);
    cx.update(init);
    cx.update(ely_gpui_component::init_chat);
    let (host, cx) = cx.add_window_view(|_, cx| {
        let chat = cx.new(|cx| {
            Chat::from_test_command(
                LaunchSpec::Acp(
                    AgentCommand::new("python3")
                        .arg(CHAT_FIXTURE)
                        .arg("staged")
                        .arg(dir.0.to_str().unwrap()),
                ),
                std::env::temp_dir(),
                cx,
            )
        });
        ThemeProbe { chat }
    });
    let chat = host.read_with(&cx.cx, |v, _| v.chat.clone());
    pump_chat_until(cx, &chat, |v| v.client.is_some());
    chat.update(cx, |v, cx| {
        v.control_send("start", cx);
    });
    pump_chat_until(cx, &chat, |v| v.streaming);
    chat.update(cx, |v, cx| {
        for _ in 0..24 {
            v.push_entry(Entry::Assistant {
                text: "Selezione 🌙 e testo Unicode\n".repeat(4),
                document: parse_chat_markdown(&"Selezione 🌙 e testo Unicode\n".repeat(4)),
            });
        }
        v.control_compose("bozza 🌙 non inviata", cx);
        v.transcript_selection = Some(TranscriptSelection { anchor: 0, head: 5 });
        v.list_state.scroll_to(gpui::ListOffset {
            item_ix: 0,
            offset_in_item: px(0.0),
        });
        cx.notify();
    });
    cx.simulate_resize(size(px(700.0), px(450.0)));
    refresh_frame(cx);
    let (draft_before, selection_before) = chat.read_with(&cx.cx, |v, _| {
        (v.draft_text(), v.selected_transcript_text())
    });
    let focused_handle_before = cx.update(|window, cx| {
        let focus = chat.read(cx).composer_field.read(cx).focus_handle(cx);
        focus.focus(window, cx);
        focus
    });
    cx.update(|_, cx| {
        Theme::set_mode(sirio_theme::ThemeMode::Light, cx);
        Theme::set_interface_font_size(18, cx);
    });
    refresh_frame(cx);
    refresh_frame(cx);
    // Frames with unchanged host tokens must not churn the global theme.
    let updates = Rc::new(Cell::new(0));
    let observed = updates.clone();
    let subscription = cx.update(|_, cx| {
        cx.observe_global::<ely_gpui_component::theme::Theme>(move |_| {
            observed.set(observed.get() + 1)
        })
    });
    refresh_frame(cx);
    refresh_frame(cx);
    assert_eq!(updates.get(), 0);
    drop(subscription);
    assert_eq!(chat.read_with(&cx.cx, |v, _| v.draft_text()), draft_before);
    assert_eq!(
        chat.read_with(&cx.cx, |v, _| v.selected_transcript_text()),
        selection_before
    );
    assert!(cx.update(|window, _| focused_handle_before.is_focused(window)));
    assert!(!chat.read_with(&cx.cx, |v, _| v.list_state.is_following_tail()));
    cx.update(|_, cx| {
        let resolved = *Theme::get(cx);
        let expected = resolved.to_bezel_theme();
        let bezel = bezel::theme::Theme::of(cx);
        assert_eq!(bezel.text, expected.text);
        assert_eq!(bezel.surface, expected.surface);
        assert_eq!(bezel.border, expected.border);
        let ely = cx.global::<ely_gpui_component::theme::Theme>();
        assert_eq!(ely.font_family.as_ref(), resolved.typography.ui_family);
        assert_eq!(ely.colors.bg, gpui::Hsla::from(resolved.colors.surface));
        assert_eq!(
            ely.text_size(ely_gpui_component::theme::TextSize::Base),
            gpui::rems(f32::from(resolved.typography.base_size) / 16.0)
        );
    });
}

fn unicode_tool() -> Entry {
    Entry::ToolCall {
        id: "reused-tool".into(),
        title: "Read α.rs".into(),
        status: "Completed".into(),
        kind: "Read".into(),
        content: vec![ToolCallContentInfo::Text("risultato 🌙 Unicode".into())],
        locations: vec![],
        raw_input: None,
        raw_output: None,
        expanded: false,
        duration_ms: None,
    }
}

#[gpui::test]
async fn ely_row_identity_changes_only_when_transcript_is_replaced(cx: &mut TestAppContext) {
    let (chat, cx) = super::tests::offline_chat_view(cx);
    let (before, after_chunk, before_replace, after_replace) = chat.update(cx, |v, cx| {
        v.streaming = true;
        v.handle_event(AcpEvent::AgentMessageChunk("ciao 🌙".into()), cx);
        let before = v.row_id(0, cx);
        v.handle_event(AcpEvent::AgentMessageChunk(" ancora".into()), cx);
        let after_chunk = v.row_id(0, cx);
        v.push_entry(unicode_tool());
        let before_replace = v.row_id(1, cx);
        v.new_conversation(cx);
        v.handle_event(AcpEvent::AgentMessageChunk("nuova".into()), cx);
        v.push_entry(unicode_tool());
        (before, after_chunk, before_replace, v.row_id(1, cx))
    });
    assert_eq!(before, after_chunk);
    assert_ne!(before_replace, after_replace);
}

#[gpui::test]
async fn ely_unicode_selection_survives_row_remeasurement(cx: &mut TestAppContext) {
    let (chat, cx) = super::tests::offline_chat_view(cx);
    chat.update(cx, |v, cx| {
        v.push_entry(Entry::User {
            text: "Domanda 🌙 α".into(),
            at: None,
        });
        v.push_entry(Entry::Assistant {
            text: "Risposta **Unicode β**".into(),
            document: parse_chat_markdown("Risposta **Unicode β**"),
        });
        v.push_entry(unicode_tool());
        for _ in 0..18 {
            v.push_entry(Entry::Assistant {
                text: "continuazione\n".repeat(8),
                document: parse_chat_markdown(&"continuazione\n".repeat(8)),
            });
        }
        let end = v.transcript_entry_ranges()[2].end;
        v.transcript_selection = Some(TranscriptSelection {
            anchor: 0,
            head: end,
        });
        v.list_state.scroll_to(gpui::ListOffset {
            item_ix: 0,
            offset_in_item: px(0.0),
        });
        cx.notify();
    });
    refresh_frame(cx);
    let before = chat.read_with(&cx.cx, |v, _| v.selected_transcript_text());
    cx.simulate_resize(size(px(430.0), px(500.0)));
    chat.update(cx, |v, cx| v.toggle_tool_call_expanded(2, cx));
    refresh_frame(cx);
    cx.update(|window, cx| {
        chat.update(cx, |v, cx| {
            v.transcript_focus.focus(window, cx);
            v.copy_transcript(&CopyTranscript, window, cx);
        });
    });
    assert_eq!(
        chat.read_with(&cx.cx, |v, _| v.selected_transcript_text()),
        before
    );
    assert_eq!(
        cx.update(|_, cx| cx.read_from_clipboard().and_then(|c| c.text())),
        before
    );
    assert!(!chat.read_with(&cx.cx, |v, _| v.list_state.is_following_tail()));
}

struct ActivityProbe {
    chat: Entity<Chat>,
}
impl Render for ActivityProbe {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        use ely_gpui_component::{
            agent::ToolCallCard,
            chat::{StepState, ThinkingBlock},
        };
        let (tool_open, thought_open) = self.chat.read_with(cx, |v, _| {
            (
                matches!(
                    v.entries.get(1),
                    Some(Entry::ToolCall { expanded: true, .. })
                ),
                matches!(v.entries.first(), Some(Entry::Thought { open, .. }) if open.get(false)),
            )
        });
        let tool_owner = self.chat.clone();
        let thought_owner = self.chat.clone();
        div()
            .size_full()
            .flex()
            .flex_col()
            .child(
                ToolCallCard::new("host-tool", "Read", StepState::Done)
                    .header_selector("host-tool-toggle")
                    .body(
                        div()
                            .debug_selector(|| "host-tool-body".into())
                            .child("host body"),
                    )
                    .expanded(tool_open, move |desired, _, cx| {
                        tool_owner.update(cx, |v, cx| {
                            if let Some(Entry::ToolCall { expanded, .. }) = v.entries.get_mut(1)
                                && *expanded != desired
                            {
                                v.toggle_tool_call_expanded(1, cx);
                            }
                        })
                    }),
            )
            .child(
                ThinkingBlock::new("host-thought", "", false, std::time::Duration::ZERO)
                    .header_selector("host-thought-toggle")
                    .body(
                        div()
                            .debug_selector(|| "host-thought-body".into())
                            .child("host reasoning"),
                    )
                    .expanded(thought_open, move |desired, _, cx| {
                        thought_owner.update(cx, |v, cx| {
                            if let Some(Entry::Thought { open, .. }) = v.entries.first()
                                && open.get(false) != desired
                            {
                                v.toggle_thought(0, cx);
                            }
                        })
                    }),
            )
            .child(div().w_full().flex_1().min_h_0().child(self.chat.clone()))
    }
}

#[gpui::test]
async fn ely_tool_and_thought_expansion_survives_virtualization(cx: &mut TestAppContext) {
    cx.update(Theme::init);
    cx.update(bezel::ui::input::init);
    cx.update(init);
    let (host, cx) = cx.add_window_view(|_, cx| {
        let chat = cx.new(|cx| {
            let mut v = Chat::new(None, std::env::temp_dir(), cx);
            v.push_entry(Entry::Thought {
                text: "Analisi 🌙 α".into(),
                open: Default::default(),
                started: None,
                duration_ms: None,
            });
            v.push_entry(unicode_tool());
            for _ in 0..40 {
                v.push_entry(Entry::Assistant {
                    text: "long row\n".repeat(6),
                    document: parse_chat_markdown(&"long row\n".repeat(6)),
                });
            }
            v.list_state.scroll_to(gpui::ListOffset {
                item_ix: 0,
                offset_in_item: px(0.0),
            });
            v.transcript_selection = Some(TranscriptSelection {
                anchor: 0,
                head: "Analisi 🌙 α".len(),
            });
            v
        });
        cx.observe(&chat, |_, _, cx| cx.notify()).detach();
        ActivityProbe { chat }
    });
    let chat = host.read_with(&cx.cx, |v, _| v.chat.clone());
    cx.simulate_resize(size(px(700.0), px(650.0)));
    refresh_frame(cx);
    let selection_before = chat.read_with(&cx.cx, |v, _| v.selected_transcript_text());
    for selector in ["host-tool-toggle", "host-thought-toggle"] {
        let bounds = cx
            .debug_bounds(selector)
            .expect("controlled header rendered");
        cx.simulate_click(bounds.center(), gpui::Modifiers::none());
        cx.run_until_parked();
        refresh_frame(cx);
    }
    assert!(cx.debug_bounds("host-tool-body").is_some());
    assert!(cx.debug_bounds("host-thought-body").is_some());
    chat.update(cx, |v, cx| {
        v.list_state.scroll_to(gpui::ListOffset {
            item_ix: 35,
            offset_in_item: px(0.0),
        });
        cx.notify();
    });
    refresh_frame(cx);
    assert!(cx.debug_bounds("thought-body-0").is_none());
    chat.update(cx, |v, cx| {
        v.handle_event(AcpEvent::AgentMessageChunk("final chunk".into()), cx);
        v.handle_event(
            AcpEvent::TurnEnded {
                stop_reason: "EndTurn".into(),
            },
            cx,
        );
        v.list_state.scroll_to(gpui::ListOffset {
            item_ix: 0,
            offset_in_item: px(0.0),
        });
        cx.notify();
    });
    refresh_frame(cx);
    assert!(cx.debug_bounds("thought-body-0").is_some());
    assert!(cx.debug_bounds("tool-output-1-0").is_some());
    chat.read_with(&cx.cx, |v, _| {
        assert!(matches!(
            &v.entries[1],
            Entry::ToolCall { expanded: true, .. }
        ));
        assert!(matches!(&v.entries[0], Entry::Thought { open, .. } if open.get(false)));
        assert_eq!(v.selected_transcript_text(), selection_before);
    });
    chat.update(cx, |v, cx| {
        v.new_conversation(cx);
        v.push_entry(Entry::Thought {
            text: "New thought".into(),
            open: Default::default(),
            started: None,
            duration_ms: None,
        });
        v.push_entry(unicode_tool());
        cx.notify();
    });
    refresh_frame(cx);
    assert!(cx.debug_bounds("host-tool-body").is_none());
    assert!(cx.debug_bounds("host-thought-body").is_none());
    assert!(chat.read_with(&cx.cx, |v, _| matches!(
        &v.entries[1],
        Entry::ToolCall {
            expanded: false,
            ..
        }
    )));
}

#[gpui::test]
async fn ely_raw_arguments_share_the_rich_tool_selection_projection(cx: &mut TestAppContext) {
    let (chat, cx) = super::tests::offline_chat_view(cx);
    chat.update(cx, |v, cx| {
        let mut tool = unicode_tool();
        if let Entry::ToolCall {
            raw_input,
            raw_output,
            content,
            expanded,
            ..
        } = &mut tool
        {
            *raw_input = Some(
                serde_json::to_string_pretty(&serde_json::json!({"path":"src/α.rs"})).unwrap(),
            );
            *raw_output = Some("{\"exitCode\":0}".into());
            *expanded = true;
            content.push(ToolCallContentInfo::Diff(ToolCallDiff {
                path: "src/α.rs".into(),
                old_text: Some("old 🌙\n".into()),
                new_text: "new β\n".into(),
            }));
        }
        v.push_entry(tool);
        cx.notify();
    });
    refresh_frame(cx);
    let projection = chat.read_with(&cx.cx, |v, _| v.entries[0].plain_text());
    assert!(
        projection.contains("\"path\": \"src/α.rs\""),
        "reported arguments must be selectable alongside the rich diff"
    );
    assert!(
        projection.contains("exitCode"),
        "reported raw result remains available alongside rich output"
    );
    assert!(projection.contains("old 🌙") && projection.contains("new β"));
    assert!(cx.debug_bounds("tool-output-0-0").is_some());
}

#[gpui::test]
async fn ely_rich_diff_drag_and_copy_includes_raw_projection_offsets(cx: &mut TestAppContext) {
    let (chat, cx) = super::tests::offline_chat_view(cx);
    chat.update(cx, |v, cx| {
        let mut tool = unicode_tool();
        if let Entry::ToolCall {
            raw_input,
            raw_output,
            expanded,
            content,
            ..
        } = &mut tool
        {
            *raw_input = Some("{\"path\":\"src/α.rs\"}".into());
            *raw_output = Some("{\"exitCode\":0}".into());
            *expanded = true;
            content.push(ToolCallContentInfo::Diff(ToolCallDiff {
                path: "src/α.rs".into(),
                old_text: Some("old 🌙\n".into()),
                new_text: "new β\n".into(),
            }));
        }
        v.push_entry(tool);
        cx.notify();
    });
    cx.simulate_resize(size(px(700.0), px(700.0)));
    refresh_frame(cx);
    let from = cx.debug_bounds("tool-diff-0-0-text-0").unwrap();
    let to = cx.debug_bounds("tool-diff-0-0-text-1").unwrap();
    let start = point(from.left() + px(1.0), from.center().y);
    let end = point(to.right() - px(1.0), to.center().y);
    cx.simulate_event(MouseDownEvent {
        position: start,
        button: MouseButton::Left,
        modifiers: gpui::Modifiers::none(),
        click_count: 1,
        first_mouse: false,
    });
    cx.simulate_event(MouseMoveEvent {
        position: end,
        pressed_button: Some(MouseButton::Left),
        modifiers: gpui::Modifiers::none(),
    });
    cx.simulate_event(MouseUpEvent {
        position: end,
        button: MouseButton::Left,
        modifiers: gpui::Modifiers::none(),
        click_count: 1,
    });
    cx.run_until_parked();
    let selected = chat
        .read_with(&cx.cx, |v, _| v.selected_transcript_text())
        .unwrap();
    assert!(
        selected.starts_with("old 🌙\nnew"),
        "diff drag must address the rich row after raw argument bytes: {selected:?}"
    );
    cx.update(|window, cx| chat.update(cx, |v, cx| v.copy_transcript(&CopyTranscript, window, cx)));
    assert_eq!(cx.read_from_clipboard().unwrap().text().unwrap(), selected);
}

fn request_wire(dir: &std::path::Path) -> Vec<serde_json::Value> {
    std::fs::read_to_string(dir.join("wire.jsonl"))
        .unwrap_or_default()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

#[gpui::test]
async fn ely_request_sends_only_the_advertised_option_id(cx: &mut TestAppContext) {
    let dir = TempDir::new();
    let (chat, cx) = super::tests::chat_view(cx, &["ely-permission", dir.0.to_str().unwrap()]);
    pump_chat_until(cx, &chat, |v| v.client.is_some());
    chat.update(cx, |v, cx| {
        v.control_send("inspect", cx);
    });
    pump_chat_until(cx, &chat, |v| v.pending_question().is_some());
    refresh_frame(cx);
    let request_id = chat.read_with(&cx.cx, |v, _| {
        question_dock::question_view(&v.entries).unwrap().request_id
    });
    let request_selector: &'static str = format!("ely-request-{request_id}").leak();
    assert!(
        cx.debug_bounds(request_selector).is_some(),
        "wire-driven Ely shell must render the live request"
    );
    assert!(cx.debug_bounds("ely-request-always").is_none());
    cx.executor()
        .advance_clock(question_dock::DOCK_ARMING_DELAY);
    cx.run_until_parked();
    let allow = cx
        .debug_bounds("permission-option-allow:this-call")
        .unwrap();
    cx.simulate_click(allow.center(), gpui::Modifiers::none());
    cx.run_until_parked();
    pump_chat_until(cx, &chat, |v| {
        v.entries.iter().any(|e| matches!(e, Entry::Assistant { text, .. } if text.contains("Echoed option: allow:this-call")))
    });
    let wire = request_wire(&dir.0);
    assert_eq!(wire.len(), 1);
    assert_eq!(wire[0]["result"]["outcome"]["optionId"], "allow:this-call");
}

#[gpui::test]
async fn ely_expired_request_cannot_answer_the_next_request(cx: &mut TestAppContext) {
    let dir = TempDir::new();
    let (chat, cx) = super::tests::chat_view(cx, &["ely-expiry", dir.0.to_str().unwrap()]);
    pump_chat_until(cx, &chat, |v| v.client.is_some());
    cx.update(|window, cx| {
        let focus = chat.read(cx).composer_field.read(cx).focus_handle(cx);
        focus.focus(window, cx);
    });
    chat.update(cx, |v, cx| {
        v.control_send("first", cx);
    });
    pump_chat_until(cx, &chat, |v| v.pending_question().is_some());
    refresh_frame(cx);
    refresh_frame(cx);
    let request_id = chat.read_with(&cx.cx, |v, _| {
        question_dock::question_view(&v.entries).unwrap().request_id
    });
    let request_selector: &'static str = format!("ely-request-{request_id}").leak();
    assert!(cx.debug_bounds(request_selector).is_some());
    chat.update(cx, |v, cx| {
        v.question_answer.for_request = Some(request_id);
        v.question_answer.draft = "stale draft".into();
        v.question_dock.selected = 1;
        cx.notify();
    });
    std::fs::write(dir.0.join("expire"), "go").unwrap();
    pump_chat_until(cx, &chat, |v| {
        !v.streaming && v.pending_question().is_none()
    });
    refresh_frame(cx);
    refresh_frame(cx);
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert!(
        request_wire(&dir.0).is_empty(),
        "expired request must not send any answer"
    );
    assert!(cx.debug_bounds(request_selector).is_none());
    chat.update(cx, |v, cx| {
        v.control_send("second", cx);
    });
    pump_chat_until(cx, &chat, |v| {
        v.entries.iter().any(|e| matches!(e, Entry::Permission { request_id: id, expired: false, .. } if *id != request_id))
    });
    refresh_frame(cx);
    refresh_frame(cx);
    chat.read_with(&cx.cx, |v, _| {
        assert!(v.question_answer.draft.is_empty());
        assert_eq!(v.question_dock.selected, 0);
    });
    cx.executor()
        .advance_clock(question_dock::DOCK_ARMING_DELAY);
    cx.run_until_parked();
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    pump_chat_until(cx, &chat, |v| !v.streaming);
    let wire = request_wire(&dir.0);
    assert_eq!(wire.len(), 1);
    assert_eq!(wire[0]["id"], 9002);
    assert_eq!(wire[0]["result"]["outcome"]["optionId"], "allow:this-call");
}

#[gpui::test]
async fn ely_composer_hint_names_enter_only_while_enter_acts(cx: &mut TestAppContext) {
    let dir = TempDir::new();
    let (chat, cx) = super::tests::chat_view(cx, &["ely-permission", dir.0.to_str().unwrap()]);
    pump_chat_until(cx, &chat, |v| v.client.is_some());
    refresh_frame(cx);
    assert!(
        cx.debug_bounds("composer-input-hint").is_some(),
        "an idle composer says what Enter does"
    );
    chat.update(cx, |v, cx| {
        v.control_send("inspect", cx);
    });
    pump_chat_until(cx, &chat, |v| v.pending_question().is_some());
    refresh_frame(cx);
    assert!(
        cx.debug_bounds("composer-input-hint").is_none(),
        "while a request waits Enter neither sends nor queues, so no hint may promise it"
    );
}

#[gpui::test]
async fn ely_header_names_the_connection_and_activity_state(cx: &mut TestAppContext) {
    let dir = TempDir::new();
    let (chat, cx) = super::tests::chat_view(cx, &["staged", dir.0.to_str().unwrap()]);
    pump_chat_until(cx, &chat, |v| v.client.is_some());
    refresh_frame(cx);
    let header = cx.debug_bounds("chat-header").unwrap();
    let idle = cx
        .debug_bounds("chat-header-state-idle")
        .expect("a connected, idle chat says so in its header");
    assert!(inside(header, idle));
    chat.update(cx, |v, cx| {
        v.control_send("hello", cx);
    });
    pump_chat_until(cx, &chat, |v| v.streaming);
    refresh_frame(cx);
    assert!(cx.debug_bounds("chat-header-state-working").is_some());
    assert!(cx.debug_bounds("chat-header-state-idle").is_none());
    std::fs::write(dir.0.join("go"), "go").unwrap();
    pump_chat_until(cx, &chat, |v| !v.streaming);
    refresh_frame(cx);
    assert!(cx.debug_bounds("chat-header-state-idle").is_some());
}

#[gpui::test]
async fn ely_generating_indicator_sits_on_the_transcript_text_column(cx: &mut TestAppContext) {
    let dir = TempDir::new();
    let (chat, cx) = super::tests::chat_view(cx, &["staged", dir.0.to_str().unwrap()]);
    pump_chat_until(cx, &chat, |v| v.client.is_some());
    chat.update(cx, |v, cx| {
        v.control_send("hello", cx);
    });
    pump_chat_until(cx, &chat, |v| v.streaming);
    refresh_frame(cx);
    let transcript = cx.debug_bounds("chat-transcript").unwrap();
    let indicator = cx.debug_bounds("chat-generating-indicator").unwrap();
    let column = transcript.left() + px(24.0);
    assert!(
        (indicator.left() - column).abs() < px(0.5),
        "the Thinking row starts where transcript text starts: {:?} vs {:?}",
        indicator.left(),
        column
    );
    std::fs::write(dir.0.join("go"), "go").unwrap();
    pump_chat_until(cx, &chat, |v| !v.streaming);
}

fn user_turns(chat: &Entity<Chat>, cx: &VisualTestContext) -> usize {
    chat.read_with(&cx.cx, |chat, _| {
        chat.entries
            .iter()
            .filter(|entry| matches!(entry, Entry::User { .. }))
            .count()
    })
}

#[gpui::test]
async fn ely_composer_enter_respects_completion_and_ime(cx: &mut TestAppContext) {
    use gpui::EntityInputHandler as _;
    let (chat, cx) = super::tests::chat_view(cx, &["composer"]);
    pump_chat_until(cx, &chat, |v| v.client.is_some());
    refresh_frame(cx);
    assert!(
        cx.debug_bounds("ely-prompt-input").is_some(),
        "the composer is drawn by Ely's prompt input"
    );
    let composer = cx.debug_bounds("composer").unwrap();
    cx.simulate_click(composer.center(), gpui::Modifiers::none());
    cx.run_until_parked();

    // A completion row is open: Enter accepts it and sends nothing.
    cx.simulate_keystrokes("/");
    cx.run_until_parked();
    assert!(cx.debug_bounds("slash-popup").is_some());
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert_eq!(
        user_turns(&chat, cx),
        0,
        "accepting a popup row sends no turn"
    );
    assert!(
        chat.read_with(&cx.cx, |v, _| v.draft_text())
            .starts_with("/cr")
    );

    // A marked composition is in flight: Enter belongs to the IME.
    let field = chat.read_with(&cx.cx, |v, _| v.composer_field.clone());
    cx.update(|window, cx| {
        field.update(cx, |field, cx| {
            field.replace_and_mark_text_in_range(None, "か", None, window, cx)
        })
    });
    cx.run_until_parked();
    assert!(
        cx.update(|window, cx| field.update(cx, |f, cx| f.marked_text_range(window, cx)))
            .is_some(),
        "the field is composing"
    );
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert_eq!(
        user_turns(&chat, cx),
        0,
        "Enter during composition sends nothing"
    );

    // The composition commits; the next Enter is an ordinary send.
    cx.update(|window, cx| {
        field.update(cx, |field, cx| {
            field.replace_text_in_range(None, "か", window, cx)
        })
    });
    cx.run_until_parked();
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    pump_chat_until(cx, &chat, |v| v.has_completed_turn);
    assert_eq!(
        user_turns(&chat, cx),
        1,
        "one Enter after the commit sends once"
    );
    assert!(chat.read_with(&cx.cx, |v, _| {
        v.entries
            .iter()
            .any(|e| matches!(e, Entry::User { text, .. } if text.contains("か")))
    }));
}

#[gpui::test]
async fn ely_composer_sends_an_attachment_without_text(cx: &mut TestAppContext) {
    let (chat, cx) = super::tests::chat_view(cx, &["echo-blocks"]);
    pump_chat_until(cx, &chat, |v| v.client.is_some());
    refresh_frame(cx);
    assert!(
        cx.debug_bounds("ely-prompt-input").is_some(),
        "the composer is drawn by Ely's prompt input"
    );
    cx.cx
        .write_to_clipboard(gpui::ClipboardItem::new_image(&gpui::Image {
            format: gpui::ImageFormat::Png,
            bytes: vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a],
            id: 11,
        }));
    let composer = cx.debug_bounds("composer").unwrap();
    cx.simulate_click(composer.center(), gpui::Modifiers::none());
    cx.run_until_parked();
    #[cfg(target_os = "macos")]
    cx.simulate_keystrokes("cmd-v");
    #[cfg(not(target_os = "macos"))]
    cx.simulate_keystrokes("ctrl-v");
    cx.run_until_parked();
    refresh_frame(cx);
    assert_eq!(chat.read_with(&cx.cx, |v, _| v.attachments.len()), 1);
    assert_eq!(chat.read_with(&cx.cx, |v, _| v.draft_text()), "");

    let send = cx
        .debug_bounds("send-ready")
        .expect("an attachment alone makes the send action available");
    cx.simulate_click(send.center(), gpui::Modifiers::none());
    cx.run_until_parked();
    pump_chat_until(cx, &chat, |v| {
        v.entries
            .iter()
            .any(|e| matches!(e, Entry::Assistant { text, .. } if text.starts_with("blocks:")))
    });
    let reply = chat.read_with(&cx.cx, |v, _| {
        v.entries
            .iter()
            .find_map(|e| match e {
                Entry::Assistant { text, .. } if text.starts_with("blocks:") => Some(text.clone()),
                _ => None,
            })
            .unwrap()
    });
    assert_eq!(
        reply, "blocks: image(image/png)",
        "the agent received the picture and no text block"
    );
    assert!(chat.read_with(&cx.cx, |v, _| v.attachments.is_empty()));
}

/// Whether `inner` lies within `outer`, to the half pixel layout rounds to.
fn inside(outer: gpui::Bounds<gpui::Pixels>, inner: gpui::Bounds<gpui::Pixels>) -> bool {
    let slack = px(0.5);
    inner.left() >= outer.left() - slack
        && inner.top() >= outer.top() - slack
        && inner.right() <= outer.right() + slack
        && inner.bottom() <= outer.bottom() + slack
}

fn history_database(path: &std::path::Path, long_title: &str) {
    use sirio_persistence::{ProjectRecord, TabRecord, WorktreeRecord};
    let db = AppDatabase::open(path).unwrap();
    db.save_project(&ProjectRecord {
        id: "project".into(),
        name: "Project".into(),
        root_path: "/tmp/project".into(),
        order_idx: 0,
        color_hex: None,
        display_name: None,
        icon_kind: "icon".into(),
        icon_value: None,
        avatar_image: None,
        default_worktree_base: None,
        worktree_location_override: None,
    })
    .unwrap();
    db.save_worktree(&WorktreeRecord {
        id: "worktree".into(),
        project_id: "project".into(),
        branch: "main".into(),
        path: "/tmp/project".into(),
        order_idx: 0,
        is_primary: true,
        comment: None,
        created_at: None,
        updated_at: None,
        secondary_pane_hidden: false,
    })
    .unwrap();
    let tab = |id: &str, title: &str| TabRecord {
        id: id.into(),
        worktree_id: "worktree".into(),
        title: title.into(),
        kind: "chat".into(),
        agent_id: None,
        agent_session_id: None,
        order_idx: 0,
        is_active: false,
        last_event_at: None,
        closed_at: None,
    };
    db.save_tabs(
        "worktree",
        &[
            tab("current-chat", "Current chat"),
            tab("long-title-chat", long_title),
            tab("short-chat", "Short chat"),
        ],
    )
    .unwrap();
    for (id, text) in [
        ("long-title-chat", "said in the long titled chat"),
        ("short-chat", "said in the short chat"),
    ] {
        db.save_chat_transcript(&ChatTranscript {
            tab_id: id.into(),
            turns: vec![ChatTurn {
                entries: vec![ChatEntry::UserMessage {
                    text: text.into(),
                    at: None,
                }],
            }],
        })
        .unwrap();
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
}

#[gpui::test]
async fn ely_history_opens_the_chosen_session_and_confirms_deletion(cx: &mut TestAppContext) {
    let dir = TempDir::new();
    let database_path = dir.0.join("history.db");
    let long_title = "LongSessionTitle".repeat(12);
    history_database(&database_path, &long_title);
    cx.update(Theme::init);
    cx.update(bezel::ui::input::init);
    cx.update(init);
    let path = database_path.clone();
    let (chat, cx) = cx.add_window_view(move |_, cx| {
        let mut chat = Chat::new(
            Some(LaunchSpec::Acp(AgentCommand::new(
                "/definitely/missing/sirio-acp-agent",
            ))),
            std::env::temp_dir(),
            cx,
        );
        chat.persistence = Some(ChatPersistence {
            database_path: path,
            tab_id: "current-chat".into(),
            worktree_id: "worktree".into(),
        });
        chat
    });
    refresh_frame(cx);

    let overflow = cx.debug_bounds("composer-overflow").unwrap();
    cx.simulate_click(overflow.center(), gpui::Modifiers::none());
    cx.run_until_parked();
    let history = cx.debug_bounds("overflow-chat-history").unwrap();
    cx.simulate_click(history.center(), gpui::Modifiers::none());
    cx.run_until_parked();
    refresh_frame(cx);

    // The long title is cut inside the row; its controls stay in the row.
    let row = cx.debug_bounds("chat-history-row-long-title-chat").unwrap();
    let open = cx
        .debug_bounds("chat-history-open-long-title-chat")
        .unwrap();
    let delete = cx
        .debug_bounds("chat-history-delete-long-title-chat")
        .unwrap();
    assert!(inside(row, open) && inside(row, delete));
    assert!(row.size.width <= px(280.0), "{row:?}");

    // Deleting asks first; Cancel leaves the session where it was.
    cx.simulate_click(delete.center(), gpui::Modifiers::none());
    cx.run_until_parked();
    refresh_frame(cx);
    let cancel = cx
        .debug_bounds("chat-history-cancel-delete-long-title-chat")
        .expect("the row asks for confirmation");
    assert!(inside(
        row,
        cx.debug_bounds("chat-history-confirm-delete-long-title-chat")
            .unwrap()
    ));
    cx.simulate_click(cancel.center(), gpui::Modifiers::none());
    cx.run_until_parked();
    refresh_frame(cx);
    assert!(
        cx.debug_bounds("chat-history-delete-long-title-chat")
            .is_some()
    );
    let stored = |path: &std::path::Path| {
        AppDatabase::open(path)
            .unwrap()
            .chat_sessions("worktree")
            .unwrap()
            .into_iter()
            .map(|session| session.tab_id)
            .collect::<Vec<_>>()
    };
    assert!(stored(&database_path).contains(&"long-title-chat".to_string()));

    // Confirming removes the stored transcript and the row.
    let delete = cx
        .debug_bounds("chat-history-delete-long-title-chat")
        .unwrap();
    cx.simulate_click(delete.center(), gpui::Modifiers::none());
    cx.run_until_parked();
    refresh_frame(cx);
    let confirm = cx
        .debug_bounds("chat-history-confirm-delete-long-title-chat")
        .unwrap();
    cx.simulate_click(confirm.center(), gpui::Modifiers::none());
    cx.run_until_parked();
    refresh_frame(cx);
    assert!(
        cx.debug_bounds("chat-history-row-long-title-chat")
            .is_none()
    );
    assert!(!stored(&database_path).contains(&"long-title-chat".to_string()));

    // Opening another session rebinds this tab's persistence to it.
    let open = cx.debug_bounds("chat-history-open-short-chat").unwrap();
    cx.simulate_click(open.center(), gpui::Modifiers::none());
    cx.run_until_parked();
    chat.read_with(&cx.cx, |chat, _| {
        assert_eq!(
            chat.persistence.as_ref().map(|p| p.tab_id.as_str()),
            Some("short-chat")
        );
        assert!(!chat.history_open);
        assert!(chat.entries.iter().any(
            |entry| matches!(entry, Entry::User { text, .. } if text == "said in the short chat")
        ));
    });
}

#[gpui::test]
async fn ely_narrow_catalogue_popup_keeps_send_and_focus_reachable(cx: &mut TestAppContext) {
    let (chat, cx) = super::tests::chat_view(cx, &["plain"]);
    pump_chat_until(cx, &chat, |v| v.client.is_some());
    cx.simulate_resize(size(px(430.0), px(720.0)));
    chat.update(cx, |chat, cx| {
        chat.has_completed_turn = true;
        chat.available_models = vec![
            ModelOption {
                id: "advertised-long".into(),
                name: "An agent reported model whose label is far too long for a 430 pixel pane"
                    .into(),
                description: None,
            },
            ModelOption {
                id: "advertised-short".into(),
                name: "Short".into(),
                description: None,
            },
        ];
        chat.model_config_id = Some("model".into());
        chat.selected_model = Some("advertised-short".into());
        cx.notify();
    });
    refresh_frame(cx);
    let pane = cx.debug_bounds("chat-root").unwrap();

    let chip = cx.debug_bounds("model-chip").unwrap();
    cx.simulate_click(chip.center(), gpui::Modifiers::none());
    cx.run_until_parked();
    refresh_frame(cx);
    refresh_frame(cx);
    let send_bounds = cx.debug_bounds("send").expect("send stays drawn");
    assert!(inside(pane, send_bounds), "{send_bounds:?} in {pane:?}");

    // Up from the current model reaches the long one; Enter chooses it.
    cx.simulate_keystrokes("up");
    cx.run_until_parked();
    refresh_frame(cx);
    let selected_row_bounds = cx
        .debug_bounds("model-option-advertised-long")
        .expect("the long label's row is drawn");
    assert!(
        inside(pane, selected_row_bounds),
        "{selected_row_bounds:?} in {pane:?}"
    );
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    let selected_model_id = chat.read_with(&cx.cx, |v, _| v.selected_model.clone());
    assert_eq!(selected_model_id.as_deref(), Some("advertised-long"));
    assert!(
        cx.debug_bounds("model-picker").is_none(),
        "choosing closes it"
    );

    // Escape closes a reopened picker and the composer has the focus again.
    refresh_frame(cx);
    let chip = cx.debug_bounds("model-chip").unwrap();
    cx.simulate_click(chip.center(), gpui::Modifiers::none());
    cx.run_until_parked();
    refresh_frame(cx);
    assert!(cx.debug_bounds("model-picker").is_some());
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    refresh_frame(cx);
    assert!(cx.debug_bounds("model-picker").is_none());
    let composer = chat.read_with(&cx.cx, |v, cx| v.composer_field.read(cx).focus_handle(cx));
    let composer_focus_after_escape =
        cx.update(|window, app| window.focused(app).is_some_and(|f| f == composer));
    assert!(composer_focus_after_escape, "Escape hands the focus back");
    let send_bounds = cx.debug_bounds("send").unwrap();
    assert!(inside(pane, send_bounds));
}
