//! A deliberately local, non-shipping prototype for Settings → Agents.
//!
//! The registry fixture is embedded so this example never reaches the network.
//! Its small model intentionally lives here instead of depending on
//! `tiller_registry`: this is a surface to look at, not a new product seam.

use gpui::{
    App, AppContext, Bounds, Context, CursorStyle, Div, Entity, FocusHandle, FontWeight,
    KeyDownEvent, MouseButton, Render, Rgba, TitlebarOptions, Window, WindowBounds, WindowOptions,
    div, point, prelude::*, px, size,
};
use gpui_platform::application;
use serde_json::Value;
use tiller_agents::AgentAvailability;
use tiller_theme::{Theme, ThemeMode};
use tiller_ui::{controls, sidebar::icons::{Icon, IconElement, IconSize}};

const TARGET_PLATFORM: &str = "windows-x86_64";
const INSTALLED_DEMO_ID: &str = "amp-acp";

#[derive(Clone, Debug)]
struct RegistryAgent {
    id: String,
    name: String,
    version: String,
    description: String,
    distribution: Value,
}

/// Decode only the fields this prototype needs. The URL-valued icon is
/// intentionally not rendered: a remote icon would make a network-free
/// prototype pretend it can provide something it cannot.
fn parse_registry(json: &str) -> Vec<RegistryAgent> {
    let document: Value = serde_json::from_str(json).expect("registry sample is valid JSON");
    document
        .get("agents")
        .and_then(Value::as_array)
        .expect("registry sample has an agents array")
        .iter()
        .map(|agent| RegistryAgent {
            id: required_string(agent, "id"),
            name: required_string(agent, "name"),
            version: required_string(agent, "version"),
            description: required_string(agent, "description"),
            distribution: agent
                .get("distribution")
                .cloned()
                .unwrap_or_else(|| Value::Object(Default::default())),
        })
        .collect()
}

fn required_string(agent: &Value, field: &str) -> String {
    agent
        .get(field)
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("registry agent is missing {field}"))
        .to_owned()
}

/// The fixture is resolved for the target platform exactly as the registry
/// resolver's installability rule: a matching binary or any npx distribution
/// wins; uvx-only entries remain unsupported.
fn is_installable(agent: &RegistryAgent) -> bool {
    let has_binary = agent
        .distribution
        .get("binary")
        .and_then(Value::as_object)
        .is_some_and(|artifacts| artifacts.contains_key(TARGET_PLATFORM));
    let has_npx = agent
        .distribution
        .get("npx")
        .is_some_and(Value::is_object);
    has_binary || has_npx
}

fn builtin_state_margin_right(available: bool) -> gpui::Pixels {
    if available { px(0.0) } else { px(8.0) }
}

fn requirements_line(agent: &RegistryAgent) -> &'static str {
    if agent
        .distribution
        .get("npx")
        .is_some_and(Value::is_object)
    {
        return "npm package · runs through npx";
    }

    if let Some(binary) = agent.distribution.get("binary").and_then(Value::as_object) {
        match binary.get(TARGET_PLATFORM).and_then(Value::as_object) {
            Some(artifact) => {
                let archive = artifact.get("archive").and_then(Value::as_str);
                if archive.is_some_and(|archive| {
                    archive.ends_with(".tar.bz2") || archive.ends_with(".tar.xz")
                }) {
                    return "Windows build published in an archive Tiller cannot open";
                }
                return "prebuilt binary · Windows x64";
            }
            None => return "No Windows build — other platforms only",
        }
    }

    if agent
        .distribution
        .get("uvx")
        .is_some_and(Value::is_object)
    {
        return "Python package · needs uvx, which Tiller does not run";
    }

    unreachable!("registry agent has no recognized distribution: {}", agent.id)
}

fn visible_registry_agents<'a>(
    registry: &'a [RegistryAgent],
    query: &str,
) -> Vec<(usize, &'a RegistryAgent)> {
    let query = query.trim().to_lowercase();
    let mut visible = registry
        .iter()
        .enumerate()
        .filter(|(_, agent)| {
            query.is_empty()
                || agent.name.to_lowercase().contains(&query)
                || agent.description.to_lowercase().contains(&query)
        })
        .collect::<Vec<_>>();
    visible.sort_by_cached_key(|(_, agent)| agent.name.to_lowercase());
    visible
}

fn browse_action_label(agent: &RegistryAgent) -> Option<&'static str> {
    if agent.id == INSTALLED_DEMO_ID {
        Some("Installed")
    } else if is_installable(agent) {
        Some("Install")
    } else {
        None
    }
}

fn builtin_registry_id(adapter_id: &str) -> Option<&'static str> {
    match adapter_id {
        "claude" => Some("claude-acp"),
        "codex" => Some("codex-acp"),
        "opencode" => Some("opencode"),
        "pi" => Some("pi-acp"),
        _ => None,
    }
}

fn builtin_version<'a>(adapter_id: &str, registry: &'a [RegistryAgent]) -> Option<&'a str> {
    let registry_id = builtin_registry_id(adapter_id)?;
    registry
        .iter()
        .find(|agent| agent.id == registry_id)
        .map(|agent| agent.version.as_str())
}

fn agent_icon(id: &str) -> Icon {
    Icon::for_agent_id(id).unwrap_or(Icon::Sparkles)
}

fn agent_icon_color(theme: Theme, id: &str) -> Rgba {
    agent_icon(id)
        .agent_mark_color(theme.title)
        .unwrap_or(theme.meta)
}

fn pill(id: String, label: String, theme: Theme, danger: bool) -> impl IntoElement {
    let selector = id.clone();
    div()
        .id(id)
        .debug_selector(move || selector.clone())
        .max_w(px(220.0))
        .px(px(8.0))
        .py(px(3.0))
        .rounded(theme.radii.row_card)
        .overflow_hidden()
        .text_ellipsis()
        .text_size(theme.typography.caption2)
        .font_weight(if danger {
            FontWeight::SEMIBOLD
        } else {
            FontWeight::NORMAL
        })
        .text_color(if danger { theme.background } else { theme.subtitle })
        .bg(if danger {
            theme.tab_error
        } else {
            theme.primary_pill_bg
        })
        .child(label)
}

fn version_text(version: Option<&str>) -> Option<String> {
    version.map(|version| format!("v{version}"))
}

fn version_label(id: String, version: Option<&str>, theme: Theme) -> impl IntoElement {
    let selector = id.clone();
    div()
        .id(id)
        .debug_selector(move || selector.clone())
        .flex_none()
        .text_size(theme.typography.caption2)
        .text_color(theme.meta)
        .when_some(version_text(version), |this, version| this.child(version))
}

fn agent_name_line(
    name: String,
    version: Option<String>,
    color: Rgba,
    theme: Theme,
) -> Div {
    let line_height = theme.typography.ui_line_height;
    let mut line = div()
        .w_full()
        .h(line_height)
        .line_height(line_height)
        .overflow_hidden()
        .text_ellipsis()
        .flex()
        .items_center()
        .gap(px(6.0))
        .text_size(theme.typography.headline)
        .text_color(color)
        .child(name);
    if let Some(version) = version {
        line = line.child(
            div()
                .flex_none()
                .text_size(theme.typography.caption2)
                .text_color(theme.meta)
                .child(version),
        );
    }
    line
}

fn agent_description_line(description: String, color: Rgba, theme: Theme) -> Div {
    let line_height = theme.typography.ui_line_height;
    div()
        .w_full()
        .h(line_height)
        .line_height(line_height)
        .overflow_hidden()
        .text_ellipsis()
        .text_size(theme.typography.footnote)
        .text_color(color)
        .child(description)
}

fn agent_label(
    icon: Icon,
    icon_color: Rgba,
    name: String,
    version: Option<String>,
    description: String,
    requirements: Option<String>,
    enabled: bool,
    theme: Theme,
) -> Div {
    let title_color = if enabled { theme.title } else { theme.meta };
    let description_color = if enabled { theme.subtitle } else { theme.text_ghost };
    let mut text = div()
        .flex_1()
        .min_w_0()
        .flex()
        .flex_col()
        .gap(px(2.0))
        .child(agent_name_line(name, version, title_color, theme))
        .child(agent_description_line(description, description_color, theme));
    if let Some(requirements) = requirements {
        text = text.child(agent_description_line(requirements, description_color, theme));
    }

    div()
        .flex()
        .items_center()
        .gap(px(10.0))
        .child(
            div()
                .w(px(18.0))
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
                .child(IconElement::new(icon, IconSize::Small).text_color(icon_color)),
        )
        .child(text)
}

fn row_shell(index: usize, row: Div) -> impl IntoElement {
    row.id(("registry-proto-row", index))
        .debug_selector(move || format!("registry-proto-row-{index}"))
}

fn action_button_shell(label: &'static str, theme: Theme) -> Div {
    div()
        .flex_none()
        .px(px(8.0))
        .py(px(3.0))
        .rounded(theme.radii.row_card)
        .text_size(theme.typography.caption2)
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(theme.title)
        .bg(theme.primary_pill_bg)
        .child(label)
}

fn action_affordance(
    id: String,
    label: &'static str,
    theme: Theme,
    entity: Entity<RegistryBrowseProto>,
    message: String,
) -> impl IntoElement {
    let selector = id.clone();
    action_button_shell(label, theme)
        .id(id)
        .debug_selector(move || selector.clone())
        .hover(|style| style.bg(theme.row_hover))
        .cursor(CursorStyle::PointingHand)
        .on_click(move |_, _, cx| {
            let message = message.clone();
            entity.update(cx, |prototype, cx| {
                prototype.notice = Some(message);
                cx.notify();
            });
        })
}

fn trailing_column(control: impl IntoElement) -> Div {
    div()
        .flex_1()
        .min_w_0()
        .flex()
        .items_center()
        .justify_end()
        .child(control)
}

struct RegistryBrowseProto {
    registry: Vec<RegistryAgent>,
    builtins: Vec<AgentAvailability>,
    search: String,
    search_focus: FocusHandle,
    notice: Option<String>,
}

impl RegistryBrowseProto {
    fn new(cx: &mut Context<Self>) -> Self {
        Self {
            registry: parse_registry(include_str!("registry-sample.json")),
            builtins: tiller_agents::ALL
                .iter()
                .map(|adapter| adapter.availability())
                .collect(),
            search: String::new(),
            search_focus: cx.focus_handle(),
            notice: None,
        }
    }

    fn on_search_key(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        let key = event.keystroke.key.as_str();
        if key == "backspace" || key == "delete" {
            self.search.pop();
        } else if let Some(character) = event.keystroke.key_char.as_deref()
            && !event.keystroke.modifiers.platform
            && !event.keystroke.modifiers.control
        {
            self.search.push_str(character);
        }
        cx.notify();
    }

    fn render_builtin_row(
        &self,
        index: usize,
        availability: &AgentAvailability,
        theme: Theme,
    ) -> impl IntoElement {
        let version = builtin_version(availability.id, &self.registry);
        let description = format!(
            "Built-in: uses the {} binary on your PATH.",
            availability.id
        );
        let path = availability
            .executable
            .as_deref()
            .map(tiller_project::display_path)
            .unwrap_or_else(|| "—".to_owned());
        let available = availability.is_available();
        let mut trailing = div()
            .flex_none()
            .flex()
            .items_center()
            .gap(px(6.0))
            .child(pill(
                format!("your-agent-path-{index}"),
                path,
                theme,
                false,
            ));
        trailing = trailing
            .child(version_label(
                format!("your-agent-version-{index}"),
                version,
                theme,
            ))
            .child(
                div()
                    .mr(builtin_state_margin_right(available))
                    .child(pill(
                        format!("your-agent-state-{index}"),
                        if available {
                            "Available".to_owned()
                        } else {
                            "Not found on PATH".to_owned()
                        },
                        theme,
                        !available,
                    )),
            );

        let row = controls::row_view(
            agent_label(
                agent_icon(availability.id),
                agent_icon_color(theme, availability.id),
                availability.display_name.to_owned(),
                None,
                description,
                None,
                true,
                theme,
            ),
            trailing_column(trailing),
            theme,
        );
        row_shell(index, row)
    }

    fn render_installed_demo_row(
        &self,
        index: usize,
        agent: &RegistryAgent,
        theme: Theme,
        entity: Entity<Self>,
    ) -> impl IntoElement {
        let path = if cfg!(windows) {
            r"%LOCALAPPDATA%\Tiller\agents\amp-acp\0.9.0\amp-acp.exe".to_owned()
        } else {
            "~/.local/share/tiller/agents/amp-acp/0.9.0/amp-acp".to_owned()
        };
        let uninstall = action_affordance(
            "your-agent-uninstall-amp".to_owned(),
            "Uninstall",
            theme,
            entity,
            "Uninstall is not wired in this prototype.".to_owned(),
        );
        let trailing = div()
            .flex_none()
            .flex()
            .items_center()
            .gap(px(8.0))
            .child(pill(
                "your-agent-installed-path".to_owned(),
                path,
                theme,
                false,
            ))
            .child(version_label(
                "your-agent-installed-version".to_owned(),
                Some(agent.version.as_str()),
                theme,
            ))
            .child(pill(
                "your-agent-installed-state".to_owned(),
                "Installed".to_owned(),
                theme,
                false,
            ))
            .child(uninstall);
        let row = controls::row_view(
            agent_label(
                Icon::Sparkles,
                theme.meta,
                agent.name.clone(),
                None,
                "Installed by Tiller from the ACP registry.".to_owned(),
                None,
                true,
                theme,
            ),
            trailing_column(trailing),
            theme,
        );
        row_shell(index, row)
    }

    fn render_your_agents(&self, theme: Theme, entity: Entity<Self>) -> Div {
        let mut card = controls::card(theme);
        for (index, availability) in self.builtins.iter().enumerate() {
            if index > 0 {
                card = card.child(controls::separator(theme));
            }
            card = card.child(self.render_builtin_row(index, availability, theme));
        }
        if let Some(agent) = self.registry.iter().find(|agent| agent.id == INSTALLED_DEMO_ID) {
            card = card.child(controls::separator(theme));
            card = card.child(self.render_installed_demo_row(
                self.builtins.len(),
                agent,
                theme,
                entity,
            ));
        }
        card
    }

    fn render_browse_row(
        &self,
        index: usize,
        agent: &RegistryAgent,
        theme: Theme,
        entity: Entity<Self>,
    ) -> impl IntoElement {
        let action_label = browse_action_label(agent);
        let enabled = action_label.is_some();
        let action = match action_label {
            Some("Install") => action_affordance(
                format!("registry-install-{index}"),
                "Install",
                theme,
                entity,
                format!("Install is not wired in this prototype ({})", agent.name),
            )
            .into_any_element(),
            Some("Installed") => pill(
                format!("registry-installed-{index}"),
                "Installed".to_owned(),
                theme,
                false,
            )
            .into_any_element(),
            None => div()
                .flex_none()
                .text_size(theme.typography.callout)
                .text_color(theme.meta)
                .child("Unavailable")
                .into_any_element(),
            _ => unreachable!("unknown browse action label"),
        };
        let row = controls::row_view(
            agent_label(
                Icon::Sparkles,
                if enabled {
                    theme.meta
                } else {
                    theme.text_ghost
                },
                agent.name.clone(),
                Some(format!("v{}", agent.version)),
                agent.description.clone(),
                Some(requirements_line(agent).to_owned()),
                enabled,
                theme,
            ),
            trailing_column(action),
            theme,
        );
        row_shell(index, row)
    }

    fn render_search(
        &self,
        theme: Theme,
        entity: Entity<Self>,
        window: &Window,
    ) -> impl IntoElement {
        let focus = self.search_focus.clone();
        let focus_for_click = focus.clone();
        let click_entity = entity.clone();
        let key_entity = entity;
        let search = self.search.clone();
        let focused = focus.is_focused(window);
        div()
            .id("registry-search")
            .debug_selector(|| "registry-search".to_owned())
            .track_focus(&focus)
            .focusable()
            .w(px(340.0))
            .h(px(30.0))
            .px(px(10.0))
            .flex()
            .items_center()
            .rounded(theme.radii.control)
            .bg(theme.filter_field_bg)
            .border_1()
            .border_color(if focused {
                theme.selection_ring
            } else {
                theme.hairline
            })
            .cursor(CursorStyle::IBeam)
            .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                click_entity.update(cx, |_, cx| focus_for_click.focus(window, cx));
            })
            .on_key_down(move |event, _, cx| {
                key_entity.update(cx, |prototype, cx| prototype.on_search_key(event, cx));
            })
            .text_size(theme.typography.callout)
            .text_color(if search.is_empty() {
                theme.meta
            } else {
                theme.title
            })
            .child(if search.is_empty() {
                "Search agents by name or description".to_owned()
            } else {
                search
            })
    }

    fn render_browse(&self, theme: Theme, entity: Entity<Self>, window: &Window) -> Div {
        let mut card = controls::card(theme).child(
            div()
                .px(px(theme.cosmic.spacing.xs as f32))
                .py(px(theme.cosmic.spacing.xs as f32))
                .child(self.render_search(theme, entity.clone(), window)),
        );
        let visible = visible_registry_agents(&self.registry, &self.search);
        if visible.is_empty() {
            card = card.child(controls::separator(theme)).child(
                div()
                    .px(px(theme.cosmic.spacing.xs as f32))
                    .py(px(theme.cosmic.spacing.xs as f32))
                    .text_size(theme.typography.footnote)
                    .text_color(theme.subtitle)
                    .child("No agents match this search."),
            );
            return card;
        }
        card = card.child(controls::separator(theme));
        for (visible_index, (index, agent)) in visible.into_iter().enumerate() {
            if visible_index > 0 {
                card = card.child(controls::separator(theme));
            }
            card = card.child(self.render_browse_row(index, agent, theme, entity.clone()));
        }
        card
    }
}

impl Render for RegistryBrowseProto {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *Theme::get(cx);
        let entity = cx.entity();
        div()
            .id("registry-browse-proto")
            .size_full()
            .bg(theme.background)
            .overflow_y_scroll()
            .child(
                div()
                    .w(px(960.0))
                    .mx_auto()
                    .px(px(28.0))
                    .py(px(28.0))
                    .child(
                        div()
                            .mb(px(18.0))
                            .text_size(theme.typography.title)
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme.title)
                            .child("Settings · Agents"),
                    )
                    .when_some(self.notice.clone(), |this, notice| {
                        this.child(
                            div()
                                .mb(px(12.0))
                                .text_size(theme.typography.footnote)
                                .text_color(theme.subtitle)
                                .child(notice),
                        )
                    })
                    .child(controls::section(
                        "Your agents",
                        self.render_your_agents(theme, entity.clone()),
                        theme,
                    ))
                    .child(controls::section(
                        "Browse the registry",
                        self.render_browse(theme, entity, window),
                        theme,
                    )),
            )
    }
}

fn main() {
    application().run(|cx: &mut App| {
        Theme::install(ThemeMode::Dark, cx);
        let bounds = Bounds::centered(None, size(px(1100.0), px(900.0)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(TitlebarOptions {
                    appears_transparent: true,
                    traffic_light_position: Some(point(px(12.0), px(12.0))),
                    ..Default::default()
                }),
                ..Default::default()
            },
            |_, cx| cx.new(RegistryBrowseProto::new),
        )
        .expect("open registry browse prototype window");
        cx.activate(true);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_published_registry_sample() {
        let registry = parse_registry(include_str!("registry-sample.json"));
        assert_eq!(registry.len(), 39);
        assert!(registry.iter().any(|agent| agent.name == "Amp"));
        assert!(registry.iter().all(|agent| !agent.description.is_empty()));
    }

    #[test]
    fn resolves_the_sample_for_windows_like_the_registry() {
        let registry = parse_registry(include_str!("registry-sample.json"));
        assert_eq!(registry.iter().filter(|agent| is_installable(agent)).count(), 37);
        assert_eq!(
            registry.iter().filter(|agent| !is_installable(agent)).count(),
            2
        );
        assert!(!is_installable(
            registry
                .iter()
                .find(|agent| agent.id == "fast-agent")
                .expect("fast-agent fixture")
        ));
        assert!(!is_installable(
            registry
                .iter()
                .find(|agent| agent.id == "minion-code")
                .expect("minion-code fixture")
        ));
    }

    #[test]
    fn search_matches_name_or_description_without_case() {
        let registry = parse_registry(include_str!("registry-sample.json"));
        let query = "FRONTIER CODING AGENT".to_lowercase();
        assert!(registry.iter().any(|agent| {
            agent.name.to_lowercase().contains(&query)
                || agent.description.to_lowercase().contains(&query)
        }));
    }

    #[test]
    fn agent_text_lines_use_a_full_theme_line_box() {
        use gpui::{Length, Styled};

        let theme = Theme::dark();
        let expected_height = Length::Definite(theme.typography.ui_line_height.into());
        for mut line in [
            agent_name_line("Agoragentic".to_owned(), None, theme.title, theme),
            agent_description_line(
                "Agent marketplace with capabilities".to_owned(),
                theme.subtitle,
                theme,
            ),
        ] {
            let style = Styled::style(&mut line);
            assert_eq!(
                style.text.line_height,
                Some(theme.typography.ui_line_height.into())
            );
            assert_eq!(style.size.height, Some(expected_height));
        }
    }

    fn test_agent(id: &str, name: &str, distribution: serde_json::Value) -> RegistryAgent {
        RegistryAgent {
            id: id.to_owned(),
            name: name.to_owned(),
            version: "1.0.0".to_owned(),
            description: "test agent".to_owned(),
            distribution,
        }
    }

    #[test]
    fn registry_requirements_use_only_distribution_and_platform_facts() {
        use serde_json::json;

        let cases = [
            (
                json!({"npx": {"package": "agent@1.0.0"}}),
                "npm package · runs through npx",
            ),
            (
                json!({"binary": {"windows-x86_64": {"archive": "agent.zip"}}}),
                "prebuilt binary · Windows x64",
            ),
            (
                json!({"binary": {"linux-x86_64": {"archive": "agent.tar.gz"}}}),
                "No Windows build — other platforms only",
            ),
            (
                json!({"binary": {"windows-x86_64": {"archive": "agent.tar.xz"}}}),
                "Windows build published in an archive Tiller cannot open",
            ),
            (
                json!({"uvx": {"package": "agent"}}),
                "Python package · needs uvx, which Tiller does not run",
            ),
        ];
        for (distribution, expected) in cases {
            assert_eq!(requirements_line(&test_agent("test", "Test", distribution)), expected);
        }

        let registry = parse_registry(include_str!("registry-sample.json"));
        assert!(registry.iter().all(|agent| !requirements_line(agent).is_empty()));
    }

    #[test]
    fn registry_rows_sort_by_display_name_and_installed_rows_have_no_install_action() {
        let registry = parse_registry(include_str!("registry-sample.json"));
        let names = visible_registry_agents(&registry, "")
            .into_iter()
            .take(4)
            .map(|(_, agent)| agent.name.as_str())
            .collect::<Vec<_>>();
        assert_eq!(names, ["Agoragentic", "Amp", "Auggie CLI", "Autohand Code"]);

        let amp = registry.iter().find(|agent| agent.id == INSTALLED_DEMO_ID).unwrap();
        let other = registry.iter().find(|agent| agent.id == "auggie").unwrap();
        let unsupported = registry.iter().find(|agent| agent.id == "fast-agent").unwrap();
        assert_eq!(browse_action_label(amp), Some("Installed"));
        assert_eq!(browse_action_label(other), Some("Install"));
        assert_eq!(browse_action_label(unsupported), None);
    }

    #[test]
    fn missing_versions_render_no_placeholder_text() {
        assert_eq!(version_text(None), None);
        assert_eq!(version_text(Some("1.2.3")), Some("v1.2.3".to_owned()));
    }

    #[test]
    fn browse_controls_are_right_aligned_and_use_install_button_chrome() {
        use gpui::{FontWeight, JustifyContent, Styled};

        let theme = Theme::dark();
        let mut trailing = trailing_column(div().child("Install"));
        assert_eq!(
            Styled::style(&mut trailing).justify_content,
            Some(JustifyContent::End)
        );

        let mut button = action_button_shell("Install", theme);
        let style = Styled::style(&mut button);
        assert_eq!(style.background, Some(theme.primary_pill_bg.into()));
        assert_eq!(style.text.font_size, Some(theme.typography.caption2.into()));
        assert_eq!(style.text.font_weight, Some(FontWeight::SEMIBOLD));
        assert_eq!(builtin_state_margin_right(false), px(8.0));
        assert_eq!(builtin_state_margin_right(true), px(0.0));
    }
}
