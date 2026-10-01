//! Native compatibility probe; uses the same GPUI packages as Sirio.
use ely_gpui_component::{
    agent::ToolCallCard,
    chat::{ChatContainer, MessageBubble, PromptInput, Role, StepState},
    forms::TextInput,
    theme::{ActiveTheme, TextSize},
};
use gpui::{
    App, Bounds, Context, Entity, Focusable, Window, WindowBounds, WindowOptions, div, prelude::*,
    px, size,
};

#[path = "../src/chat/identity.rs"]
mod identity;

struct Probe {
    input: Entity<TextInput>,
    messages: Vec<String>,
}

impl Probe {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input = cx.new(|cx| {
            TextInput::new(window, cx)
                .multi_line(3, 8)
                .placeholder("Write a message")
        });
        window.focus(&input.focus_handle(cx), cx);
        cx.observe(&input, |_, _, cx| cx.notify()).detach();
        Self {
            input,
            messages: vec!["Ely components on Sirio’s GPUI".into()],
        }
    }
}

impl Render for Probe {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let owner = cx.entity();
        let messages = div()
            .id("probe-messages")
            .size_full()
            .overflow_y_scroll()
            .p_4()
            .flex()
            .flex_col()
            .gap_4()
            .text_size(cx.theme().text_size(TextSize::Base))
            .font_family(cx.theme().font_family.clone())
            .text_color(cx.theme().colors.fg)
            .child(
                div().flex().gap_3().children(
                    [
                        ("claude", "Claude Code"),
                        ("codex", "Codex"),
                        ("opencode", "OpenCode"),
                        ("pi", "Pi"),
                        ("omp", "Oh My Pi"),
                        ("unrecognized", "Unknown agent"),
                    ]
                    .into_iter()
                    .map(|(id, name)| {
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                ely_gpui_component::chat::MessageAvatar::new(
                                    gpui::ElementId::Name(format!("mark-{id}").into()),
                                    Role::Assistant,
                                    name,
                                )
                                .content(identity::render_mark(
                                    gpui::ElementId::Name(format!("agent-{id}").into()),
                                    Some(id),
                                    name,
                                    sirio_theme::Theme::get(cx),
                                )),
                            )
                            .child(name)
                    }),
                ),
            )
            .children(self.messages.iter().enumerate().map(|(index, text)| {
                MessageBubble::new(if index == 0 {
                    Role::Assistant
                } else {
                    Role::User
                })
                .child(text.clone())
            }))
            .child(
                ToolCallCard::new("probe-tool", "Read", StepState::Done)
                    .summary("Native compatibility")
                    .arguments("{\"path\":\"src/chat.rs\"}")
                    .result("Component and asset loading verified"),
            );
        let composer = PromptInput::new("probe-composer", &self.input, move |_, cx| {
            owner.update(cx, |view, cx| {
                let text = view.input.read(cx).text().to_owned();
                if !text.trim().is_empty() {
                    println!("probe sent: {text}");
                    view.messages.push(text);
                    view.input.update(cx, |input, cx| input.set_text("", cx));
                    cx.notify();
                }
            });
        });
        ChatContainer::new(messages).composer(composer)
    }
}

fn main() {
    gpui_platform::application()
        .with_assets(sirio_ui::chat::ChatAssets)
        .run(|cx: &mut App| {
            bezel::ui::register_fonts(cx).expect("Sirio fonts");
            sirio_theme::Theme::init(cx);
            sirio_ui::chat::init(cx);
            if std::env::var("ELY_PROBE_APPEARANCE").as_deref() == Ok("light") {
                sirio_theme::Theme::set_mode(sirio_theme::ThemeMode::Light, cx);
            } else {
                sirio_theme::Theme::set_mode(sirio_theme::ThemeMode::Dark, cx);
            }
            for (path, _) in identity::ASSETS {
                assert!(identity::asset(path).is_some());
                assert!(
                    cx.asset_source().load(path).unwrap().is_some(),
                    "missing {path}"
                );
            }
            assert!(
                cx.asset_source()
                    .load(ely_gpui_component::primitives::IconName::Check.path())
                    .unwrap()
                    .is_some()
            );
            let bounds = Bounds::centered(None, size(px(820.0), px(700.0)), cx);
            cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    ..Default::default()
                },
                |window, cx| {
                    sirio_ui::chat::init(cx);
                    window.set_window_title("Ely Chat Probe");
                    cx.new(|cx| Probe::new(window, cx))
                },
            )
            .unwrap();
            cx.activate(true);
        });
}
