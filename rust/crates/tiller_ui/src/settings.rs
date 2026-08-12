//! Full-window settings surface and its small fixture model.

use crate::controls;
use crate::sidebar::icons::{Icon, IconElement};
use gpui::{Context, Entity, FontWeight, Render, Rgba, Window, div, prelude::*, px, text};
use std::rc::Rc;
use tiller_theme::{Theme, ThemeMode};

const CONTENT_WIDTH: f32 = 704.0;
const HEADER_HEIGHT: f32 = 40.0;
const CATEGORY_WIDTH: f32 = 200.0;
const DETAIL_TOP_PADDING: f32 = 15.0;
const DETAIL_BOTTOM_PADDING: f32 = 12.0;
const DETAIL_SECTION_MARGIN: f32 = 20.0;

const SEGMENTED_THEME: &[&str] = &["System", "Light", "Dark"];
const SEGMENTED_FILE_ICONS: &[&str] = &["SF Symbols", "Material"];

fn settings_section(title: &'static str, card: gpui::Div, theme: Theme) -> impl IntoElement {
    div()
        .id(format!("settings-section-{title}"))
        .w_full()
        .mb(px(DETAIL_SECTION_MARGIN))
        .child(
            div()
                .pl(px(10.0))
                .mb(px(9.0))
                .text_size(px(13.0))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(theme.title)
                .child(text!(id = format!("settings-section-title-{title}"), title)),
        )
        .child(card)
}

/// Settings categories in the same order as the Swift surface.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SettingsCategory {
    AiProviders,
    Agents,
    General,
    Permissions,
    Appearance,
}

impl SettingsCategory {
    const ALL: [Self; 5] = [
        Self::AiProviders,
        Self::Agents,
        Self::General,
        Self::Permissions,
        Self::Appearance,
    ];

    fn title(self) -> &'static str {
        match self {
            Self::AiProviders => "AI Providers",
            Self::Agents => "Agents",
            Self::General => "General",
            Self::Permissions => "Permissions",
            Self::Appearance => "Appearance",
        }
    }

    fn glyph(self) -> Icon {
        match self {
            Self::AiProviders => Icon::Sparkles,
            Self::Agents => Icon::SquareTerminal,
            Self::General => Icon::Settings,
            Self::Permissions => Icon::Shield,
            Self::Appearance => Icon::SunMoon,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FileIconChoice {
    SfSymbols,
    Material,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ProviderKind {
    Claude,
    Codex,
    OpenCodeGo,
}

/// Small settings view model. The real application can replace these values
/// with its persistence layer without changing the reusable settings UI.
pub struct Settings {
    category: SettingsCategory,
    on_back: Option<Rc<dyn Fn()>>,
    translucency: bool,
    interface_font_size: i32,
    terminal_font_size: i32,
    file_icons: FileIconChoice,
    claude_show_in_bar: bool,
    codex_show_in_bar: bool,
    opencode_show_in_bar: bool,
    refresh_interval: i32,
    resume_agent_sessions: bool,
    auto_naming: bool,
    limit_chat_history: bool,
    chat_retention: i32,
    limit_mounted_worktrees: bool,
    mounted_worktrees: i32,
    control_socket_enabled: bool,
}

impl Settings {
    /// Creates the default settings model used by the demo and shell wiring.
    pub fn new(_cx: &mut Context<Self>) -> Self {
        Self {
            category: SettingsCategory::Appearance,
            on_back: None,
            translucency: false,
            interface_font_size: 13,
            terminal_font_size: 14,
            file_icons: FileIconChoice::Material,
            claude_show_in_bar: true,
            codex_show_in_bar: true,
            opencode_show_in_bar: false,
            refresh_interval: 5,
            resume_agent_sessions: true,
            auto_naming: false,
            limit_chat_history: true,
            chat_retention: 100,
            limit_mounted_worktrees: false,
            mounted_worktrees: 6,
            control_socket_enabled: true,
        }
    }

    /// Installs the host callback used to return from the full-window
    /// settings surface to the host's main surface.
    pub fn on_back(mut self, callback: impl Fn() + 'static) -> Self {
        self.on_back = Some(Rc::new(callback));
        self
    }

    fn select_category(&mut self, category: SettingsCategory, cx: &mut Context<Self>) {
        self.category = category;
        cx.notify();
    }

    fn set_theme_mode(&mut self, mode: ThemeMode, cx: &mut Context<Self>) {
        Theme::set_mode(mode, cx);
        // Repaint every mounted surface, including the shell behind this
        // full-window view. Theme is a GPUI global, so existing entities read
        // the new palette on their next render without owning a stale copy.
        cx.refresh_windows();
        cx.notify();
    }

    fn set_translucency(&mut self, enabled: bool, cx: &mut Context<Self>) {
        self.translucency = enabled;
        cx.notify();
    }

    fn set_interface_font_size(&mut self, value: i32, cx: &mut Context<Self>) {
        self.interface_font_size = value.clamp(10, 20);
        cx.notify();
    }

    fn set_terminal_font_size(&mut self, value: i32, cx: &mut Context<Self>) {
        self.terminal_font_size = value.clamp(9, 24);
        cx.notify();
    }

    fn set_file_icons(&mut self, index: usize, cx: &mut Context<Self>) {
        self.file_icons = if index == 0 {
            FileIconChoice::SfSymbols
        } else {
            FileIconChoice::Material
        };
        cx.notify();
    }

    fn theme_segment(mode: ThemeMode) -> usize {
        match mode {
            ThemeMode::System => 0,
            ThemeMode::Light => 1,
            ThemeMode::Dark => 2,
        }
    }

    fn set_provider_visibility(
        &mut self,
        provider: ProviderKind,
        enabled: bool,
        cx: &mut Context<Self>,
    ) {
        match provider {
            ProviderKind::Claude => self.claude_show_in_bar = enabled,
            ProviderKind::Codex => self.codex_show_in_bar = enabled,
            ProviderKind::OpenCodeGo => self.opencode_show_in_bar = enabled,
        }
        cx.notify();
    }

    fn provider_visibility(&self, provider: ProviderKind) -> bool {
        match provider {
            ProviderKind::Claude => self.claude_show_in_bar,
            ProviderKind::Codex => self.codex_show_in_bar,
            ProviderKind::OpenCodeGo => self.opencode_show_in_bar,
        }
    }

    fn set_refresh_interval(&mut self, value: i32, cx: &mut Context<Self>) {
        self.refresh_interval = value.clamp(1, 60);
        cx.notify();
    }

    fn render_header(&self, theme: Theme) -> impl IntoElement {
        let back = self.on_back.clone();
        div()
            .h(px(HEADER_HEIGHT))
            .w_full()
            .px(px(12.0))
            .flex()
            .items_center()
            .gap(px(10.0))
            .bg(theme.background)
            .child(
                div()
                    .id("settings-back")
                    .h(px(24.0))
                    .flex()
                    .items_center()
                    .gap(px(4.0))
                    .rounded(px(5.0))
                    .text_size(px(13.0))
                    .text_color(theme.title)
                    .hover(|style| style.bg(theme.row_hover))
                    .on_click(move |_, _, _| {
                        if let Some(callback) = &back {
                            callback();
                        }
                    })
                    .child(IconElement::new(Icon::ChevronLeft, px(14.0)).text_color(theme.meta))
                    .child(text!("Back")),
            )
            .child(
                div()
                    .text_size(px(14.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme.title)
                    .child(text!("Settings")),
            )
    }

    fn render_categories(&self, theme: Theme, entity: Entity<Self>) -> impl IntoElement {
        let mut list = div()
            .w(px(CATEGORY_WIDTH))
            .h_full()
            .p(px(8.0))
            .flex()
            .flex_col()
            .gap(px(2.0))
            .bg(theme.background);

        for (category_index, category) in SettingsCategory::ALL.into_iter().enumerate() {
            let selected = self.category == category;
            let entity = entity.clone();
            list = list.child(
                div()
                    .id(format!("settings-category-{:?}", category))
                    .w_full()
                    .h(px(30.0))
                    .px(px(10.0))
                    .flex()
                    .items_center()
                    .gap(px(10.0))
                    .rounded(px(6.0))
                    .text_size(px(13.0))
                    .font_weight(if selected {
                        FontWeight::SEMIBOLD
                    } else {
                        FontWeight::NORMAL
                    })
                    .text_color(if selected {
                        theme.title
                    } else {
                        theme.subtitle
                    })
                    .when(selected, |this| this.bg(theme.selection_fill))
                    .hover(|style| style.bg(theme.row_hover))
                    .on_click(move |_, _, cx| {
                        entity.update(cx, |this, cx| this.select_category(category, cx));
                    })
                    .child(
                        div()
                            .w(px(16.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(
                                IconElement::new(category.glyph(), px(14.0))
                                    .text_color(theme.meta),
                            ),
                    )
                    .child(text!(id = ("settings-category-title", category_index), category.title())),
            );
        }

        list.child(div().flex_1())
    }

    fn render_appearance(&self, theme: Theme, mode: ThemeMode, entity: Entity<Self>) -> gpui::Div {
        let theme_entity = entity.clone();
        let theme_control = controls::segmented(
            "appearance-theme",
            SEGMENTED_THEME,
            Self::theme_segment(mode),
            theme,
            move |index, cx| {
                let mode = match index {
                    1 => ThemeMode::Light,
                    2 => ThemeMode::Dark,
                    _ => ThemeMode::System,
                };
                theme_entity.update(cx, |this, cx| this.set_theme_mode(mode, cx));
            },
        );
        let translucency_entity = entity.clone();
        let translucency = controls::toggle(
            "appearance-translucency",
            self.translucency,
            theme,
            move |_, _, cx| {
                translucency_entity
                    .update(cx, |this, cx| this.set_translucency(!this.translucency, cx));
            },
        );

        let mut theme_card = controls::card(theme)
            .child(
                div()
                    .id("settings-appearance-theme-row")
                    .child(controls::row("Appearance", None, theme_control, theme)),
            )
            .child(controls::separator(theme));
        theme_card = theme_card.child(
            div()
                .id("settings-appearance-translucency-row")
                .child(controls::row("Translucency", None, translucency, theme)),
        );

        let interface_entity = entity.clone();
        let interface_stepper = controls::stepper(
            "interface-font-size",
            self.interface_font_size,
            theme,
            move |value, cx| {
                interface_entity.update(cx, |this, cx| this.set_interface_font_size(value, cx));
            },
        );
        let terminal_entity = entity.clone();
        let terminal_stepper = controls::stepper(
            "terminal-font-size",
            self.terminal_font_size,
            theme,
            move |value, cx| {
                terminal_entity.update(cx, |this, cx| this.set_terminal_font_size(value, cx));
            },
        );

        let interface_card = controls::card(theme).child(
            div()
                .id("settings-interface-font-size-row")
                .child(controls::row(
                    "Font size",
                    Some(format!("{} pt", self.interface_font_size)),
                    interface_stepper,
                    theme,
                )),
        );
        let terminal_card = controls::card(theme).child(
            div()
                .id("settings-terminal-font-size-row")
                .child(controls::row(
                    "Font size",
                    Some(format!("{} pt", self.terminal_font_size)),
                    terminal_stepper,
                    theme,
                )),
        );

        let file_entity = entity.clone();
        let file_icons = controls::segmented(
            "file-icons",
            SEGMENTED_FILE_ICONS,
            if self.file_icons == FileIconChoice::SfSymbols {
                0
            } else {
                1
            },
            theme,
            move |index, cx| file_entity.update(cx, |this, cx| this.set_file_icons(index, cx)),
        );
        let files_card =
            controls::card(theme).child(controls::row("File icons", None, file_icons, theme));

        let agent_card = self.render_agent_colors(theme);

        div()
            .w(px(CONTENT_WIDTH))
            .pt(px(DETAIL_TOP_PADDING))
            .pb(px(DETAIL_BOTTOM_PADDING))
            .child(settings_section("Theme", theme_card, theme))
            .child(settings_section("Interface", interface_card, theme))
            .child(settings_section("Terminal", terminal_card, theme))
            .child(settings_section("Files", files_card, theme))
            .child(settings_section("Agent Colors", agent_card, theme))
    }

    fn render_agent_colors(&self, theme: Theme) -> gpui::Div {
        let agents: [(&str, &str, Rgba); 5] = [
            ("✳", "Claude Code", theme.rail_question),
            ("◉", "Codex", theme.tab_focus_accent),
            ("▣", "OpenCode", theme.tab_needs_input),
            ("π", "Pi", theme.tab_done),
            ("π", "Oh-My-Pi", theme.rail_task),
        ];
        let mut card = controls::card(theme);
        for (index, (glyph, name, color)) in agents.into_iter().enumerate() {
            if index > 0 {
                card = card.child(controls::separator(theme));
            }
            let label = div()
                .flex()
                .items_center()
                .gap(px(9.0))
                .text_size(px(13.0))
                .text_color(theme.title)
                .child(div().w(px(14.0)).text_color(color).child(text!(id = ("settings-agent-color-glyph", index), glyph)))
                .child(text!(id = ("settings-agent-color-name", index), name));
            card = card.child(
                div()
                    .id(("settings-agent-color-row", index))
                    .min_h(px(44.0))
                    .w_full()
                    .px(px(10.0))
                    .py(px(7.0))
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(label)
                    .child(controls::color_swatch(
                        match index {
                            0 => "agent-color-claude",
                            1 => "agent-color-codex",
                            2 => "agent-color-opencode",
                            3 => "agent-color-pi",
                            _ => "agent-color-omp",
                        },
                        color,
                        theme,
                    )),
            );
        }
        card
    }

    fn render_provider_card(
        &self,
        provider: ProviderKind,
        title: &'static str,
        glyph: &'static str,
        glyph_color: Rgba,
        entity: Entity<Self>,
        theme: Theme,
    ) -> gpui::Div {
        let status_label = div()
            .flex()
            .items_center()
            .gap(px(8.0))
            .text_size(px(13.0))
            .text_color(theme.title)
            .child(
                div()
                    .w(px(18.0))
                    .text_size(px(17.0))
                    .text_color(glyph_color)
                    .child(text!(id = format!("settings-provider-status-glyph-{title}"), glyph)),
            )
            .child(text!(id = format!("settings-provider-status-label-{title}"), "Status"));
        let status = div()
            .flex()
            .items_center()
            .gap(px(6.0))
            .text_size(px(12.0))
            .text_color(theme.title)
            .child(
                div()
                    .w(px(8.0))
                    .h(px(8.0))
                    .rounded(px(4.0))
                    .bg(theme.tab_done),
            )
            .child(text!(id = format!("settings-provider-active-{title}"), "Active"));

        let visibility_entity = entity.clone();
        let visibility = controls::toggle(
            match provider {
                ProviderKind::Claude => "provider-claude-visibility",
                ProviderKind::Codex => "provider-codex-visibility",
                ProviderKind::OpenCodeGo => "provider-opencode-visibility",
            },
            self.provider_visibility(provider),
            theme,
            move |_, _, cx| {
                visibility_entity.update(cx, |this, cx| {
                    let enabled = !this.provider_visibility(provider);
                    this.set_provider_visibility(provider, enabled, cx);
                });
            },
        );

        let refresh_entity = entity.clone();
        let refresh_stepper = controls::stepper_with_unit(
            match provider {
                ProviderKind::Claude => "provider-claude-refresh",
                ProviderKind::Codex => "provider-codex-refresh",
                ProviderKind::OpenCodeGo => "provider-opencode-refresh",
            },
            self.refresh_interval,
            "min",
            theme,
            move |value, cx| {
                refresh_entity.update(cx, |this, cx| this.set_refresh_interval(value, cx));
            },
        );

        let mut card = controls::card(theme)
            .child(controls::row_view(status_label, status, theme))
            .child(controls::separator(theme))
            .child(controls::row(
                "Last read",
                None,
                div()
                    .text_size(px(12.0))
                    .text_color(theme.subtitle)
                    .child(text!(id = format!("settings-provider-last-read-{title}"), "21:09")),
                theme,
            ))
            .child(controls::separator(theme))
            .child(controls::row("Show in usage bar", None, visibility, theme))
            .child(controls::separator(theme))
            .child(controls::row(
                "Refresh interval",
                None,
                refresh_stepper,
                theme,
            ))
            .child(controls::separator(theme))
            .child(controls::action_row(
                controls::button(
                    match provider {
                        ProviderKind::Claude => "refresh-claude-now",
                        ProviderKind::Codex => "refresh-codex-now",
                        ProviderKind::OpenCodeGo => "refresh-opencode-now",
                    },
                    "Refresh now",
                    theme,
                    |_, _, _| {},
                ),
                theme,
            ))
            .child(controls::separator(theme));

        card = card
            .child(controls::subsection_header(
                "Accounts",
                "Showing accounts for this device. New accounts are added there.",
                controls::button(
                    match provider {
                        ProviderKind::Claude => "add-claude-account",
                        ProviderKind::Codex => "add-codex-account",
                        ProviderKind::OpenCodeGo => "add-opencode-account",
                    },
                    "Add Account",
                    theme,
                    |_, _, _| {},
                ),
                theme,
            ))
            .child(controls::account_row(
                "System default",
                "Use your current CLI login on this device.",
                true,
                theme,
            ));
        let _ = title;
        card
    }

    fn render_ai_providers(&self, theme: Theme, entity: Entity<Self>) -> gpui::Div {
        div()
            .w(px(CONTENT_WIDTH))
            .pt(px(DETAIL_TOP_PADDING))
            .pb(px(DETAIL_BOTTOM_PADDING))
            .child(settings_section(
                "Claude Code",
                self.render_provider_card(
                    ProviderKind::Claude,
                    "Claude Code",
                    "✳",
                    theme.rail_question,
                    entity.clone(),
                    theme,
                ),
                theme,
            ))
            .child(settings_section(
                "Codex",
                self.render_provider_card(
                    ProviderKind::Codex,
                    "Codex",
                    "◉",
                    theme.subtitle,
                    entity.clone(),
                    theme,
                ),
                theme,
            ))
            .child(settings_section(
                "OpenCode Go",
                self.render_provider_card(
                    ProviderKind::OpenCodeGo,
                    "OpenCode Go",
                    "▣",
                    theme.tab_needs_input,
                    entity,
                    theme,
                ),
                theme,
            ))
    }

    fn render_agents(&self, theme: Theme) -> gpui::Div {
        let agents: [(&str, &str, &str, &str); 5] = [
            (
                "omp",
                "omp (Oh My Pi)",
                "Built-in: uses the omp binary on your PATH.",
                "Available",
            ),
            (
                "claude",
                "Claude Code",
                "Built-in: uses the claude binary on your PATH.",
                "Available",
            ),
            (
                "codex",
                "Codex",
                "Built-in: uses the codex binary on your PATH.",
                "Available",
            ),
            (
                "opencode",
                "OpenCode",
                "Built-in: uses the opencode binary on your PATH.",
                "Available",
            ),
            (
                "pi",
                "Pi",
                "Built-in: uses the pi binary on your PATH.",
                "Available",
            ),
        ];
        let mut agent_rows = controls::card(theme);
        for (index, (id, name, description, status)) in agents.into_iter().enumerate() {
            if index > 0 {
                agent_rows = agent_rows.child(controls::separator(theme));
            }
            let glyph = match id {
                "claude" => "✳",
                "codex" => "◉",
                "opencode" => "▣",
                _ => "π",
            };
            let color = match id {
                "claude" => theme.rail_question,
                "codex" => theme.tab_focus_accent,
                "opencode" => theme.tab_needs_input,
                _ => theme.rail_task,
            };
            let label = div()
                .flex()
                .items_center()
                .gap(px(10.0))
                .child(div().w(px(18.0)).text_color(color).child(text!(id = ("settings-agent-glyph", index), glyph)))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(2.0))
                        .child(
                            div()
                                .text_size(px(13.0))
                                .text_color(theme.title)
                                .child(text!(id = ("settings-agent-name", index), name)),
                        )
                        .child(
                            div()
                                .text_size(px(11.0))
                                .text_color(theme.subtitle)
                                .child(text!(id = ("settings-agent-description", index), description)),
                        ),
                );
            agent_rows = agent_rows.child(
                div()
                    .id(("settings-agent-row", index))
                    .child(controls::row_view(
                        label,
                        controls::button(
                            match id {
                                "omp" => "agent-omp-status",
                                "claude" => "agent-claude-status",
                                "codex" => "agent-codex-status",
                                "opencode" => "agent-opencode-status",
                                _ => "agent-pi-status",
                            },
                            status,
                            theme,
                            |_, _, _| {},
                        ),
                        theme,
                    )),
            );
        }

        div()
            .w(px(CONTENT_WIDTH))
            .pt(px(DETAIL_TOP_PADDING))
            .pb(px(DETAIL_BOTTOM_PADDING))
            .child(
                div()
                    .w_full()
                    .mb(px(14.0))
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .w(px(260.0))
                            .h(px(28.0))
                            .px(px(10.0))
                            .flex()
                            .items_center()
                            .rounded(px(6.0))
                            .bg(theme.primary_pill_bg)
                            .text_size(px(12.0))
                            .text_color(theme.subtitle)
                            .child(text!("Search agents")),
                    )
                    .child(controls::button(
                        "refresh-agents",
                        "↻ Refresh",
                        theme,
                        |_, _, _| {},
                    )),
            )
            .child(agent_rows)
    }

    fn render_general(&self, theme: Theme, entity: Entity<Self>) -> gpui::Div {
        let resume_entity = entity.clone();
        let auto_entity = entity.clone();
        let history_entity = entity.clone();
        let mounted_entity = entity.clone();
        let socket_entity = entity.clone();
        let resume = controls::toggle(
            "general-resume-sessions",
            self.resume_agent_sessions,
            theme,
            move |_, _, cx| {
                resume_entity.update(cx, |this, cx| {
                    this.resume_agent_sessions = !this.resume_agent_sessions;
                    cx.notify();
                });
            },
        );
        let auto = controls::toggle(
            "general-auto-naming",
            self.auto_naming,
            theme,
            move |_, _, cx| {
                auto_entity.update(cx, |this, cx| {
                    this.auto_naming = !this.auto_naming;
                    cx.notify();
                });
            },
        );
        let history = controls::toggle(
            "general-chat-history",
            self.limit_chat_history,
            theme,
            move |_, _, cx| {
                history_entity.update(cx, |this, cx| {
                    this.limit_chat_history = !this.limit_chat_history;
                    cx.notify();
                });
            },
        );
        let mounted = controls::toggle(
            "general-mounted-worktrees",
            self.limit_mounted_worktrees,
            theme,
            move |_, _, cx| {
                mounted_entity.update(cx, |this, cx| {
                    this.limit_mounted_worktrees = !this.limit_mounted_worktrees;
                    cx.notify();
                });
            },
        );
        let socket = controls::toggle(
            "general-control-socket",
            self.control_socket_enabled,
            theme,
            move |_, _, cx| {
                socket_entity.update(cx, |this, cx| {
                    this.control_socket_enabled = !this.control_socket_enabled;
                    cx.notify();
                });
            },
        );
        let mut about = controls::card(theme).child(controls::row(
            "Version",
            None,
            div()
                .text_size(px(12.0))
                .text_color(theme.subtitle)
                .child(text!("0.1.0")),
            theme,
        ));
        about = about
            .child(controls::separator(theme))
            .child(controls::action_row(
                controls::button(
                    "general-check-updates",
                    "Check for Updates",
                    theme,
                    |_, _, _| {},
                ),
                theme,
            ));

        let agents = controls::card(theme).child(controls::row(
            "Resume agent sessions on launch",
            Some(
                "Relaunch supported agents with their previous conversation after Tiller restarts."
                    .into(),
            ),
            resume,
            theme,
        ));
        let automation = controls::card(theme)
            .child(controls::row(
                "Auto-rename tabs and agents",
                Some(
                    "Summarizes each session into a short tab title using the selected agent."
                        .into(),
                ),
                auto,
                theme,
            ))
            .child(controls::separator(theme))
            .child(controls::row(
                "Summarizer agent",
                Some("Falls back to the session's own agent when it fails.".into()),
                controls::button("general-summarizer", "Claude Code", theme, |_, _, _| {}),
                theme,
            ));
        let history_card = controls::card(theme)
            .child(controls::row(
                "Limit stored chats",
                Some("Keeps only the most recent conversations per worktree.".into()),
                history,
                theme,
            ))
            .child(controls::separator(theme));
        let history_stepper_entity = entity.clone();
        let history_stepper = controls::stepper(
            "general-chat-retention",
            self.chat_retention,
            theme,
            move |value, cx| {
                history_stepper_entity.update(cx, |this, cx| {
                    this.chat_retention = value.clamp(5, 500);
                    cx.notify();
                });
            },
        );
        let history_card = history_card.child(controls::row(
            "Keep chats per worktree",
            None,
            history_stepper,
            theme,
        ));
        let mounted_stepper_entity = entity.clone();
        let mounted_stepper = controls::stepper(
            "general-mounted-count",
            self.mounted_worktrees,
            theme,
            move |value, cx| {
                mounted_stepper_entity.update(cx, |this, cx| {
                    this.mounted_worktrees = value.clamp(2, 50);
                    cx.notify();
                });
            },
        );
        let performance = controls::card(theme)
            .child(controls::row(
                "Limit mounted worktrees",
                Some("Frees terminal RAM by unmounting idle worktrees beyond this count.".into()),
                mounted,
                theme,
            ))
            .child(controls::separator(theme))
            .child(controls::row("Keep mounted", None, mounted_stepper, theme));
        let control = controls::card(theme)
            .child(controls::row(
                "Enable control socket",
                Some("Required by tillerctl and agent lifecycle hooks.".into()),
                socket,
                theme,
            ))
            .child(controls::separator(theme))
            .child(controls::row(
                "Bundled binary",
                None,
                div()
                    .text_size(px(11.0))
                    .text_color(theme.subtitle)
                    .child(text!("tillerctl")),
                theme,
            ))
            .child(controls::separator(theme))
            .child(controls::action_row(
                controls::button(
                    "general-install-path",
                    "Copy install command",
                    theme,
                    |_, _, _| {},
                ),
                theme,
            ));
        let skill = controls::card(theme).child(controls::action_row(
            controls::button(
                "general-install-skill",
                "Install Skill",
                theme,
                |_, _, _| {},
            ),
            theme,
        ));

        div()
            .w(px(CONTENT_WIDTH))
            .pt(px(DETAIL_TOP_PADDING))
            .pb(px(DETAIL_BOTTOM_PADDING))
            .child(settings_section("About", about, theme))
            .child(settings_section("Agents", agents, theme))
            .child(settings_section("Automation", automation, theme))
            .child(settings_section("Chat history", history_card, theme))
            .child(settings_section("Performance", performance, theme))
            .child(settings_section("tillerctl", control, theme))
            .child(settings_section("Agent Skill", skill, theme))
    }

    fn render_permission_row(
        &self,
        glyph: &'static str,
        title: &'static str,
        description: &'static str,
        badge_label: &'static str,
        badge_background: Rgba,
        badge_foreground: Rgba,
        action: &'static str,
        theme: Theme,
    ) -> impl IntoElement {
        let label = div()
            .flex()
            .items_center()
            .gap(px(8.0))
            .child(div().w(px(20.0)).text_color(theme.meta).child(text!(id = format!("settings-permission-glyph-{title}"), glyph)))
            .child(
                div()
                    .text_size(px(13.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme.title)
                    .child(text!(id = format!("settings-permission-title-{title}"), title)),
            )
            .child(controls::badge(
                badge_label,
                badge_background,
                badge_foreground,
            ));
        let description = div()
            .text_size(px(11.0))
            .text_color(theme.subtitle)
            .child(text!(id = format!("settings-permission-description-{title}"), description));
        let label = div()
            .flex()
            .flex_col()
            .gap(px(2.0))
            .child(label)
            .child(description);
        controls::row_view(
            label,
            controls::button(
                match title {
                    "Notifications" => "permission-notifications",
                    "Screen Recording" => "permission-screen-recording",
                    "Accessibility" => "permission-accessibility",
                    "Full Disk Access" => "permission-full-disk",
                    "Automation" => "permission-automation",
                    _ => "permission-local-network",
                },
                action,
                theme,
                |_, _, _| {},
            ),
            theme,
        )
        .id(format!("settings-permission-row-{title}"))
    }

    fn render_permissions(&self, theme: Theme) -> gpui::Div {
        let granted = theme.tab_done;
        let denied = theme.tab_error;
        let neutral = theme.primary_pill_bg;
        let rows = controls::card(theme)
            .child(self.render_permission_row(
                "♧",
                "Notifications",
                "Alerts when agents finish or need input.",
                "GRANTED",
                granted,
                theme.background,
                "Open Settings",
                theme,
            ))
            .child(controls::separator(theme))
            .child(self.render_permission_row(
                "◉",
                "Screen Recording",
                "Screenshot, visual automation, and UI inspection tools.",
                "GRANTED",
                granted,
                theme.background,
                "Open Settings",
                theme,
            ))
            .child(controls::separator(theme))
            .child(self.render_permission_row(
                "♙",
                "Accessibility",
                "Keystroke injection, window control, and UI automation tools.",
                "DENIED",
                denied,
                theme.title_selected,
                "Open Settings",
                theme,
            ))
            .child(controls::separator(theme))
            .child(self.render_permission_row(
                "▱",
                "Full Disk Access",
                "Recommended when projects or worktrees touch macOS-protected folders.",
                "CHECK MANUALLY",
                neutral,
                theme.subtitle,
                "Open Settings",
                theme,
            ))
            .child(controls::separator(theme))
            .child(self.render_permission_row(
                "⚙",
                "Automation",
                "Apple Events for scripts that control other local apps.",
                "GRANTED",
                granted,
                theme.background,
                "Open Settings",
                theme,
            ))
            .child(controls::separator(theme))
            .child(self.render_permission_row(
                "◎",
                "Local Network",
                "Discovery and access for development servers on your network.",
                "CHECK MANUALLY",
                neutral,
                theme.subtitle,
                "Trigger Prompt",
                theme,
            ));

        let grants = [
            "1533B573-3BEF-438C-B3AD-2586AA2546C4",
            "C7CE2EAA-35C1-4DDB-90B4-56CF61CC4528",
            "73C23955-2FAF-420D-8239-AC4FB67C1130",
            "4E0DA223-128C-43B3-8EDD-47991D3AB022",
            "B5B8813-0871-423E-8913-28423E580186",
        ];
        let mut grants_card = controls::card(theme);
        for (index, grant) in grants.into_iter().enumerate() {
            if index > 0 {
                grants_card = grants_card.child(controls::separator(theme));
            }
            grants_card = grants_card.child(
                div()
                    .id(("settings-grant-row", index))
                    .min_h(px(44.0))
                    .w_full()
                    .px(px(10.0))
                    .py(px(7.0))
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(2.0))
                            .child(
                                div()
                                    .text_size(px(13.0))
                                    .text_color(theme.title)
                                    .child(text!(id = ("settings-grant-prefix", index), "file:")),
                            )
                            .child(
                                div()
                                    .text_size(px(11.0))
                                    .text_color(theme.subtitle)
                                    .child(text!(id = ("settings-grant", index), grant)),
                            ),
                    )
                    .child(controls::button(
                        match index {
                            0 => "revoke-grant-0",
                            1 => "revoke-grant-1",
                            2 => "revoke-grant-2",
                            3 => "revoke-grant-3",
                            _ => "revoke-grant-4",
                        },
                        "Revoke",
                        theme,
                        |_, _, _| {},
                    )),
            );
        }

        div()
            .w(px(CONTENT_WIDTH))
            .pt(px(DETAIL_TOP_PADDING))
            .pb(px(DETAIL_BOTTOM_PADDING))
            .child(
                controls::card(theme)
                    .child(controls::row(
                        "Terminal tools inherit Tiller's macOS privacy envelope.",
                        Some("Use these controls when a CLI or agent in a pane needs macOS privacy access. Tiller does not ask at startup.".into()),
                        controls::button("refresh-permissions", "Refresh", theme, |_, _, _| {}),
                        theme,
                    )),
            )
            .child(
                div()
                    .mt(px(8.0))
                    .child(settings_section("macOS Permissions", rows, theme)),
            )
            .child(settings_section("Browser origin grants", grants_card, theme))
    }
}

impl Render for Settings {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *Theme::get(cx);
        let mode = Theme::get(cx).mode;
        let entity = cx.entity();
        let category_sidebar = self.render_categories(theme, entity.clone());
        let detail = match self.category {
            SettingsCategory::AiProviders => self.render_ai_providers(theme, entity),
            SettingsCategory::Agents => self.render_agents(theme),
            SettingsCategory::General => self.render_general(theme, entity),
            SettingsCategory::Permissions => self.render_permissions(theme),
            SettingsCategory::Appearance => self.render_appearance(theme, mode, entity),
        };

        div()
            .id("settings-surface")
            .size_full()
            .flex()
            .flex_col()
            .bg(theme.background)
            .child(self.render_header(theme))
            .child(div().h(px(1.0)).w_full().bg(theme.hairline))
            .child(
                div()
                    .flex_1()
                    .w_full()
                    .flex()
                    .child(category_sidebar)
                    .child(div().w(px(1.0)).h_full().bg(theme.hairline))
                    .child(
                        div()
                            .id("settings-detail-scroll")
                            .flex_1()
                            .h_full()
                            .flex()
                            .justify_center()
                            .overflow_y_scroll()
                            .child(detail),
                    ),
            )
    }
}
