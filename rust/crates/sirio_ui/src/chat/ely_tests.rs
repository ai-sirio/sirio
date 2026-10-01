//! Regression tests at the host/component boundaries.
use super::tests::{CHAT_FIXTURE, TempDir, pump_chat_until, refresh_frame};
use super::*;
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
