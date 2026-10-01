//! Regression tests at the host/component boundaries.
use super::tests::{CHAT_FIXTURE, TempDir, pump_chat_until, refresh_frame};
use super::*;
use bezel::ui::widgets::{ButtonStyle, Buttons};
use gpui::{TestAppContext, size};

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
                            if let Some(Entry::ToolCall { expanded, .. }) = v.entries.get_mut(1) {
                                if *expanded != desired {
                                    v.toggle_tool_call_expanded(1, cx);
                                }
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
                            if let Some(Entry::Thought { open, .. }) = v.entries.first() {
                                if open.get(false) != desired {
                                    v.toggle_thought(0, cx);
                                }
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
