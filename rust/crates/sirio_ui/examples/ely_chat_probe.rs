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

/// Three chat sessions for the History popover: one with a title far longer
/// than the menu, so a narrow pane shows how a row cuts it.
fn seed_history(path: &std::path::Path) {
    use sirio_persistence::{
        AppDatabase, ChatEntry, ChatTranscript, ChatTurn, ProjectRecord, TabRecord, WorktreeRecord,
    };
    let db = AppDatabase::open(path).expect("probe history database");
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
    let titles = [
        ("current-chat", "Current chat"),
        (
            "long-chat",
            "Refactor the whole authentication layer so that sessions survive a restart and tokens rotate",
        ),
        ("short-chat", "Fix the flaky test"),
        ("empty-chat", ""),
    ];
    db.save_tabs("worktree", &titles.map(|(id, title)| tab(id, title)))
        .unwrap();
    for (id, _) in titles.iter().skip(1) {
        db.save_chat_transcript(&ChatTranscript {
            tab_id: (*id).into(),
            turns: vec![ChatTurn {
                entries: vec![ChatEntry::UserMessage {
                    text: format!("said in {id}"),
                    at: None,
                }],
            }],
        })
        .unwrap();
        std::thread::sleep(std::time::Duration::from_millis(3));
    }
}

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
        .with_assets(sirio_ui::ely::AppAssets)
        .run(|cx: &mut App| {
            sirio_theme::register_ui_fonts(cx).expect("Sirio fonts");
            sirio_theme::Theme::init(cx);
            sirio_ui::chat::init(cx);
            if std::env::var("ELY_PROBE_APPEARANCE").as_deref() == Ok("light") {
                sirio_theme::Theme::set_mode(sirio_theme::ThemeMode::Light, cx);
            } else {
                sirio_theme::Theme::set_mode(sirio_theme::ThemeMode::Dark, cx);
            }
            if let Some(size) = std::env::var("ELY_PROBE_UI_SIZE")
                .ok()
                .and_then(|size| size.parse::<i32>().ok())
            {
                sirio_theme::Theme::set_interface_font_size(size, cx);
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
            if std::env::var_os("ELY_PROBE_CHAT").is_some() {
                cx.open_window(
                    WindowOptions {
                        window_bounds: Some(WindowBounds::Windowed(bounds)),
                        ..Default::default()
                    },
                    |window, cx| {
                        bezel::ui::input::init(cx);
                        sirio_ui::chat::init(cx);
                        window.set_window_title("Sirio Ely Chat Probe");
                        let chat = cx.new(|cx| {
                            let fixture = concat!(
                                env!("CARGO_MANIFEST_DIR"),
                                "/tests/fixtures/chat_fixture.py"
                            );
                            let mode = std::env::var("ELY_PROBE_FIXTURE_MODE")
                                .unwrap_or_else(|_| "plain".into());
                            let mut command = sirio_acp::AgentCommand::new("python3")
                                .arg(fixture)
                                .arg(mode);
                            if let Ok(extra) = std::env::var("ELY_PROBE_FIXTURE_DIR") {
                                command = command.arg(extra);
                            }
                            let launch = sirio_acp::LaunchSpec::Acp(command);
                            let mut chat = match std::env::var_os("ELY_PROBE_HISTORY_DB") {
                                Some(path) => {
                                    let path = std::path::PathBuf::from(path);
                                    seed_history(&path);
                                    sirio_ui::chat::Chat::launch_with_command_and_persistence(
                                        launch,
                                        std::env::temp_dir(),
                                        path,
                                        "current-chat".into(),
                                        "worktree".into(),
                                        cx,
                                    )
                                }
                                None => sirio_ui::chat::Chat::launch_with_command(
                                    launch,
                                    std::env::temp_dir(),
                                    cx,
                                ),
                            };
                            let agent = std::env::var("ELY_PROBE_AGENT")
                                .unwrap_or_else(|_| "claude".into());
                            let name = sirio_agents::ALL
                                .iter()
                                .find(|adapter| adapter.id() == agent)
                                .map(|adapter| adapter.display_name())
                                .unwrap_or("Claude Code");
                            chat.set_agent_identity(Some(agent), Some(name.into()));
                            if let Ok(history) = std::env::var("ELY_PROBE_HISTORY") {
                                chat.restore_transcript(&history, cx);
                            }
                            chat
                        });
                        window.focus(&chat.focus_handle(cx), cx);
                        chat
                    },
                )
                .unwrap();
            } else {
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
            }
            // Native X11 has no portal file dialog and no clipboard tool here: the
            // probe puts a picture on the clipboard itself, so Ctrl+V attaches it.
            if let Ok(path) = std::env::var("ELY_PROBE_CLIPBOARD_IMAGE") {
                let bytes = std::fs::read(path).expect("ELY_PROBE_CLIPBOARD_IMAGE is readable");
                cx.write_to_clipboard(gpui::ClipboardItem::new_image(&gpui::Image {
                    format: gpui::ImageFormat::Png,
                    bytes,
                    id: 1,
                }));
            }
            cx.activate(true);
        });
}
