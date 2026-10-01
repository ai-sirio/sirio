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
