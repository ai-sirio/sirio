//! A deliberately local, non-shipping prototype for Settings → Agents.
//!
//! The registry fixture is embedded so this example never reaches the network.
//! Its small model intentionally lives here instead of depending on
//! `sirio_registry`: this is a surface to look at, not a new product seam.

use gpui::{
    App, AppContext, Bounds, Context, CursorStyle, Div, Entity, FocusHandle, FontWeight,
    KeyDownEvent, MouseButton, Render, Rgba, TitlebarOptions, Window, WindowBounds, WindowOptions,
    div, point, prelude::*, px, size,
};
use gpui_platform::application;
use serde_json::Value;
use sirio_agents::AgentAvailability;
use sirio_theme::{Theme, ThemeMode};
use sirio_ui::{
    controls,
    sidebar::icons::{Icon, IconElement, IconSize},
};

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DemoUpdateState {
    UpToDate,
    UpdateAvailableVerified,
    UpdateAvailableUnverifiable,
    InFlight,
    UpdateFailed,
    InstallFailed,
}

#[derive(Clone, Copy, Debug)]
struct DemoInstalledAgent {
    registry_id: &'static str,
    installed_version: Option<&'static str>,
    state: DemoUpdateState,
    reason: Option<&'static str>,
    unverified: bool,
}

const DEMO_PATH_COLUMN_WIDTH: f32 = 220.0;
const DEMO_VERSION_COLUMN_WIDTH: f32 = 150.0;
const DEMO_COLUMN_GAP: f32 = 8.0;

fn demo_fixed_slot(width: f32) -> Div {
    div().w(px(width)).min_w(px(width)).flex_none()
}

fn demo_version_slot() -> Div {
    demo_fixed_slot(DEMO_VERSION_COLUMN_WIDTH).ml(px(DEMO_COLUMN_GAP))
}

const DEMO_INSTALLED_AGENTS: [DemoInstalledAgent; 6] = [
    DemoInstalledAgent {
        registry_id: "amp-acp",
        installed_version: Some("0.9.0"),
        state: DemoUpdateState::UpToDate,
        reason: None,
        unverified: true,
    },
    DemoInstalledAgent {
        registry_id: "kilo",
        installed_version: Some("7.4.0"),
        state: DemoUpdateState::UpdateAvailableVerified,
        reason: None,
        unverified: false,
    },
    DemoInstalledAgent {
        registry_id: "antigravity-acp",
        installed_version: Some("0.9.0"),
        state: DemoUpdateState::UpdateAvailableUnverifiable,
        reason: None,
        unverified: false,
    },
    DemoInstalledAgent {
        registry_id: "goose",
        installed_version: Some("1.46.0"),
        state: DemoUpdateState::InFlight,
        reason: None,
        unverified: false,
    },
    DemoInstalledAgent {
        registry_id: "harn",
        installed_version: Some("0.10.115"),
        state: DemoUpdateState::UpdateFailed,
        reason: Some("The archive download was interrupted."),
        unverified: false,
    },
    DemoInstalledAgent {
        registry_id: "qwen-code",
        installed_version: None,
        state: DemoUpdateState::InstallFailed,
        reason: Some("The package manager could not install v0.22.1."),
        unverified: false,
    },
];

fn demo_version_text(demo: &DemoInstalledAgent, agent: &RegistryAgent) -> Option<String> {
    let installed_version = demo.installed_version?;
    if matches!(
        demo.state,
        DemoUpdateState::UpdateAvailableVerified
            | DemoUpdateState::UpdateAvailableUnverifiable
            | DemoUpdateState::InFlight
            | DemoUpdateState::UpdateFailed
    ) {
        Some(format!("v{installed_version} → v{}", agent.version))
    } else {
        Some(format!("v{installed_version}"))
    }
}

fn demo_action_label(state: DemoUpdateState) -> Option<&'static str> {
    match state {
        DemoUpdateState::UpToDate => Some("Installed"),
        DemoUpdateState::UpdateAvailableVerified => Some("Update"),
        DemoUpdateState::UpdateAvailableUnverifiable | DemoUpdateState::InFlight => None,
        DemoUpdateState::UpdateFailed | DemoUpdateState::InstallFailed => Some("Retry"),
    }
}

fn demo_status_lines(demo: &DemoInstalledAgent, agent: &RegistryAgent) -> Vec<String> {
    let latest = format!("v{}", agent.version);
    match demo.state {
        DemoUpdateState::UpdateAvailableVerified | DemoUpdateState::UpdateAvailableUnverifiable => {
            vec![format!(
                "v{} stays in use until you relaunch · restored if the update fails",
                demo.installed_version
                    .expect("update demo has an old version")
            )]
        }
        DemoUpdateState::InFlight => vec![
            format!("Updating to {latest}…"),
            format!(
                "v{} stays in use until you relaunch",
                demo.installed_version
                    .expect("in-flight demo has an old version")
            ),
        ],
        DemoUpdateState::UpdateFailed => vec![
            format!(
                "Update to {latest} failed — v{} is still installed and working.",
                demo.installed_version
                    .expect("failed update demo has an old version")
            ),
            demo.reason
                .expect("failed update demo has a reason")
                .to_owned(),
        ],
        DemoUpdateState::InstallFailed => vec![
            "Install failed.".to_owned(),
            demo.reason
                .expect("failed install demo has a reason")
                .to_owned(),
        ],
        _ => Vec::new(),
    }
}

fn demo_path_text(demo: &DemoInstalledAgent) -> Option<String> {
    demo.installed_version?;
    Some(if cfg!(windows) {
        format!(r"%LOCALAPPDATA%\Sirio\agents\{}\bin", demo.registry_id)
    } else {
        format!("~/.local/share/sirio/agents/{}/bin", demo.registry_id)
    })
}

fn demo_version_marker(demo: &DemoInstalledAgent) -> Option<&'static str> {
    if demo.unverified {
        Some("unverified")
    } else if demo.state == DemoUpdateState::UpdateFailed {
        Some("failed")
    } else {
        None
    }
}

fn unverifiable_confirmation_heading(agent_name: &str, latest_version: &str) -> String {
    format!("No checksum for {agent_name} v{latest_version}")
}

fn unverifiable_confirmation_body() -> &'static str {
    "Its publisher does not publish checksums and Sirio found none from any other source, so it cannot tell whether the file it downloaded from releases.antigravity.dev is the one they built. If it was tampered with, Sirio will run it with your permissions."
}

fn unverifiable_confirmation_survival(installed_version: &str) -> String {
    format!("v{installed_version} is restored if the update fails — not after it succeeds.")
}

fn confirmation_primary_label() -> &'static str {
    "Install unverified v1.0.0"
}

fn confirmation_secondary_label() -> &'static str {
    "Keep v0.9.0"
}

fn confirmation_default_focus() -> &'static str {
    confirmation_secondary_label()
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
    let has_npx = agent.distribution.get("npx").is_some_and(Value::is_object);
    has_binary || has_npx
}

fn builtin_state_margin_right(available: bool) -> gpui::Pixels {
    if available { px(0.0) } else { px(8.0) }
}

fn requirements_line(agent: &RegistryAgent) -> &'static str {
    if agent.distribution.get("npx").is_some_and(Value::is_object) {
        return "npm package · runs through npx";
    }

    if let Some(binary) = agent.distribution.get("binary").and_then(Value::as_object) {
        match binary.get(TARGET_PLATFORM).and_then(Value::as_object) {
            Some(artifact) => {
                let archive = artifact.get("archive").and_then(Value::as_str);
                if archive.is_some_and(|archive| {
                    archive.ends_with(".tar.bz2") || archive.ends_with(".tar.xz")
                }) {
                    return "Windows build published in an archive Sirio cannot open";
                }
                return "prebuilt binary · Windows x64";
            }
            None => return "No Windows build — other platforms only",
        }
    }

    if agent.distribution.get("uvx").is_some_and(Value::is_object) {
        return "Python package · needs uvx, which Sirio does not run";
    }

    unreachable!(
        "registry agent has no recognized distribution: {}",
        agent.id
    )
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
        .agent_mark_color(theme.text)
        .unwrap_or(theme.text_faint)
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
        .text_ellipsis_start()
        .text_size(theme.typography.caption2)
        .font_weight(if danger {
            FontWeight::SEMIBOLD
        } else {
            FontWeight::NORMAL
        })
        .text_color(if danger {
            theme.surface
        } else {
            theme.text_muted
        })
        .bg(if danger {
            theme.tab_error
        } else {
            theme.surface_raised
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
        .text_color(theme.text_faint)
        .when_some(version_text(version), |this, version| this.child(version))
}

fn agent_name_line(name: String, version: Option<String>, color: Rgba, theme: Theme) -> Div {
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
                .text_color(theme.text_faint)
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
    let title_color = if enabled { theme.text } else { theme.text_faint };
    let description_color = if enabled {
        theme.text_muted
    } else {
        theme.text_dim
    };
    let mut text = div()
        .flex_1()
        .min_w_0()
        .flex()
        .flex_col()
        .gap(px(2.0))
        .child(agent_name_line(name, version, title_color, theme))
        .child(agent_description_line(
            description,
            description_color,
            theme,
        ));
    if let Some(requirements) = requirements {
        text = text.child(agent_description_line(
            requirements,
            description_color,
            theme,
        ));
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
        .text_color(theme.text)
        .bg(theme.element_active)
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
    confirmation_focus: FocusHandle,
    confirmation_focus_requested: bool,
    notice: Option<String>,
}

impl RegistryBrowseProto {
    fn new(cx: &mut Context<Self>) -> Self {
        Self {
            registry: parse_registry(include_str!("registry-sample.json")),
            builtins: sirio_agents::ALL
                .iter()
                .map(|adapter| adapter.availability())
                .collect(),
            search: String::new(),
            search_focus: cx.focus_handle(),
            confirmation_focus: cx.focus_handle().tab_stop(true),
            confirmation_focus_requested: false,
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
            .map(sirio_project::display_path)
            .unwrap_or_else(|| "—".to_owned());
        let available = availability.is_available();
        let mut trailing = div()
            .flex_none()
            .flex()
            .items_center()
            .gap(px(6.0))
            .child(pill(format!("your-agent-path-{index}"), path, theme, false));
        trailing = trailing
            .child(version_label(
                format!("your-agent-version-{index}"),
                version,
                theme,
            ))
            .child(div().mr(builtin_state_margin_right(available)).child(pill(
                format!("your-agent-state-{index}"),
                if available {
                    "Available".to_owned()
                } else {
                    "Not found on PATH".to_owned()
                },
                theme,
                !available,
            )));

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

    fn render_unverifiable_confirmation(
        &self,
        demo: &DemoInstalledAgent,
        agent: &RegistryAgent,
        theme: Theme,
        entity: Entity<Self>,
        window: &Window,
    ) -> impl IntoElement {
        let install_entity = entity.clone();
        let keep_entity = entity;
        let keep_focused = self.confirmation_focus.is_focused(window);
        let install = div()
            .id("registry-confirm-unverified")
            .debug_selector(|| "registry-confirm-unverified".to_owned())
            .px(px(10.0))
            .py(px(3.0))
            .rounded(theme.radii.control)
            .border_1()
            .border_color(theme.hairline)
            .text_size(theme.typography.caption2)
            .text_color(theme.text)
            .hover(|style| style.bg(theme.row_hover))
            .cursor(CursorStyle::PointingHand)
            .on_click(move |_, _, cx| {
                install_entity.update(cx, |prototype, cx| {
                    prototype.notice =
                        Some("Install unverified is not wired in this prototype.".to_owned());
                    cx.notify();
                });
            })
            .child(confirmation_primary_label());
        let keep = div()
            .id("registry-confirm-keep")
            .debug_selector(|| "registry-confirm-keep".to_owned())
            .track_focus(&self.confirmation_focus)
            .focusable()
            .tab_stop(true)
            .px(px(10.0))
            .py(px(3.0))
            .rounded(theme.radii.control)
            .text_size(theme.typography.caption2)
            .text_color(theme.text)
            .bg(theme.element_active)
            .border_1()
            .hover(|style| style.bg(theme.row_hover))
            .cursor(CursorStyle::PointingHand)
            .on_click(move |_, _, cx| {
                keep_entity.update(cx, |prototype, cx| {
                    prototype.notice = Some("Keeping the installed version.".to_owned());
                    cx.notify();
                });
            })
            .border_color(if keep_focused {
                theme.text
            } else {
                theme.element_active
            })
            .child(confirmation_default_focus());
        div()
            .id("registry-unverifiable-confirmation")
            .debug_selector(|| "registry-unverifiable-confirmation".to_owned())
            .mx(px(36.0))
            .mb(px(8.0))
            .p(px(12.0))
            .rounded(theme.radii.control)
            .bg(theme.surface)
            .border_1()
            .border_color(theme.hairline)
            .flex()
            .flex_col()
            .gap(px(8.0))
            .text_size(theme.typography.footnote)
            .text_color(theme.text_muted)
            .child(
                div()
                    .text_size(theme.typography.headline)
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme.text)
                    .child(unverifiable_confirmation_heading(
                        &agent.name,
                        &agent.version,
                    )),
            )
            .child(unverifiable_confirmation_body())
            .child(unverifiable_confirmation_survival(
                demo.installed_version
                    .expect("unverifiable update has an old version"),
            ))
            .child(
                div()
                    .flex()
                    .justify_end()
                    .items_center()
                    .gap(px(8.0))
                    .child(install)
                    .child(keep),
            )
    }

    fn render_demo_registry_row(
        &self,
        index: usize,
        demo: &DemoInstalledAgent,
        agent: &RegistryAgent,
        theme: Theme,
        entity: Entity<Self>,
        window: &Window,
    ) -> impl IntoElement {
        let mut path_slot = demo_fixed_slot(DEMO_PATH_COLUMN_WIDTH).flex().justify_end();
        if let Some(path) = demo_path_text(demo) {
            path_slot =
                path_slot.child(pill(format!("your-agent-path-{index}"), path, theme, false));
        }

        let mut version_slot = demo_version_slot()
            .flex()
            .items_center()
            .justify_end()
            .gap(px(4.0));
        if let Some(version) = demo_version_text(demo, agent) {
            version_slot = version_slot.child(
                div()
                    .text_size(theme.typography.caption2)
                    .text_color(theme.text_faint)
                    .child(version),
            );
        }
        if let Some(marker) = demo_version_marker(demo) {
            version_slot = version_slot.child(
                div()
                    .text_size(theme.typography.caption2)
                    .text_color(if marker == "failed" {
                        theme.tab_error
                    } else {
                        theme.tab_needs_input
                    })
                    .child(marker),
            );
        }

        let status_lines = demo_status_lines(demo, agent);
        let action = match demo_action_label(demo.state) {
            Some("Installed") => pill(
                format!("your-agent-state-{index}"),
                "Installed".to_owned(),
                theme,
                false,
            )
            .into_any_element(),
            Some("Update") => action_affordance(
                format!("your-agent-update-{index}"),
                "Update",
                theme,
                entity.clone(),
                format!("Update is not wired in this prototype ({})", agent.name),
            )
            .into_any_element(),
            Some("Retry") => action_affordance(
                format!("your-agent-retry-{index}"),
                "Retry",
                theme,
                entity.clone(),
                format!("Retry is not wired in this prototype ({})", agent.name),
            )
            .into_any_element(),
            None if demo.state == DemoUpdateState::UpdateAvailableUnverifiable => {
                div().flex_none().into_any_element()
            }
            None => {
                let mut status = div()
                    .flex_none()
                    .max_w(px(300.0))
                    .flex()
                    .flex_col()
                    .items_end()
                    .gap(px(2.0));
                for (line_index, line) in status_lines.iter().enumerate() {
                    status = status.child(
                        div()
                            .text_size(theme.typography.footnote)
                            .text_color(if line_index == 0 {
                                theme.text
                            } else {
                                theme.text_muted
                            })
                            .child(line.clone()),
                    );
                }
                status.into_any_element()
            }
            _ => unreachable!("unknown demo action label"),
        };
        let trailing = div()
            .flex_none()
            .flex()
            .items_center()
            .gap(px(DEMO_COLUMN_GAP))
            .child(path_slot)
            .child(version_slot)
            .child(action);
        let row = controls::row_view(
            agent_label(
                Icon::Sparkles,
                theme.text_faint,
                agent.name.clone(),
                None,
                if demo.state == DemoUpdateState::InstallFailed {
                    "Install requested from the ACP registry.".to_owned()
                } else {
                    "Installed by Sirio from the ACP registry.".to_owned()
                },
                None,
                true,
                theme,
            ),
            trailing_column(trailing),
            theme,
        );
        let mut row_container = div().w_full().flex().flex_col().child(row);
        if demo.state != DemoUpdateState::InFlight {
            for line in status_lines {
                row_container = row_container.child(
                    div()
                        .pl(px(42.0))
                        .pr(px(12.0))
                        .text_size(theme.typography.footnote)
                        .font_weight(FontWeight::NORMAL)
                        .text_color(theme.text_muted)
                        .child(line),
                );
            }
        }
        if demo.state == DemoUpdateState::UpdateAvailableUnverifiable {
            row_container = row_container
                .child(self.render_unverifiable_confirmation(demo, agent, theme, entity, window));
        }
        row_shell(index, row_container)
    }

    fn render_your_agents(&self, theme: Theme, entity: Entity<Self>, window: &Window) -> Div {
        let mut card = controls::card(theme);
        for (index, availability) in self.builtins.iter().enumerate() {
            if index > 0 {
                card = card.child(controls::separator(theme));
            }
            card = card.child(self.render_builtin_row(index, availability, theme));
        }
        for (demo_index, demo) in DEMO_INSTALLED_AGENTS.iter().enumerate() {
            if let Some(agent) = self
                .registry
                .iter()
                .find(|agent| agent.id == demo.registry_id)
            {
                card = card.child(controls::separator(theme));
                card = card.child(self.render_demo_registry_row(
                    self.builtins.len() + demo_index,
                    demo,
                    agent,
                    theme,
                    entity.clone(),
                    window,
                ));
            }
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
                .text_color(theme.text_faint)
                .child("Unavailable")
                .into_any_element(),
            _ => unreachable!("unknown browse action label"),
        };
        let row = controls::row_view(
            agent_label(
                Icon::Sparkles,
                if enabled {
                    theme.text_faint
                } else {
                    theme.text_dim
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
            .bg(theme.input_bg)
            .border_1()
            .border_color(if focused {
                theme.text
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
                theme.text_faint
            } else {
                theme.text
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
                    .text_color(theme.text_muted)
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
        if !self.confirmation_focus_requested {
            self.confirmation_focus_requested = true;
            let focus = self.confirmation_focus.clone();
            window.on_next_frame(move |window, cx| window.focus(&focus, cx));
        }
        div()
            .id("registry-browse-proto")
            .size_full()
            .bg(theme.surface)
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
                            .text_color(theme.text)
                            .child("Settings · Agents"),
                    )
                    .when_some(self.notice.clone(), |this, notice| {
                        this.child(
                            div()
                                .mb(px(12.0))
                                .text_size(theme.typography.footnote)
                                .text_color(theme.text_muted)
                                .child(notice),
                        )
                    })
                    .child(controls::section(
                        "Your agents",
                        self.render_your_agents(theme, entity.clone(), window),
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
        assert_eq!(
            registry
                .iter()
                .filter(|agent| is_installable(agent))
                .count(),
            37
        );
        assert_eq!(
            registry
                .iter()
                .filter(|agent| !is_installable(agent))
                .count(),
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
            agent_name_line("Agoragentic".to_owned(), None, theme.text, theme),
            agent_description_line(
                "Agent marketplace with capabilities".to_owned(),
                theme.text_muted,
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
                "Windows build published in an archive Sirio cannot open",
            ),
            (
                json!({"uvx": {"package": "agent"}}),
                "Python package · needs uvx, which Sirio does not run",
            ),
        ];
        for (distribution, expected) in cases {
            assert_eq!(
                requirements_line(&test_agent("test", "Test", distribution)),
                expected
            );
        }

        let registry = parse_registry(include_str!("registry-sample.json"));
        assert!(
            registry
                .iter()
                .all(|agent| !requirements_line(agent).is_empty())
        );
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

        let amp = registry
            .iter()
            .find(|agent| agent.id == INSTALLED_DEMO_ID)
            .unwrap();
        let other = registry.iter().find(|agent| agent.id == "auggie").unwrap();
        let unsupported = registry
            .iter()
            .find(|agent| agent.id == "fast-agent")
            .unwrap();
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
    fn update_demo_rows_cover_every_requested_state() {
        assert_eq!(DEMO_INSTALLED_AGENTS.len(), 6);
        assert_eq!(
            DEMO_INSTALLED_AGENTS
                .iter()
                .map(|demo| demo.state)
                .collect::<Vec<_>>(),
            vec![
                DemoUpdateState::UpToDate,
                DemoUpdateState::UpdateAvailableVerified,
                DemoUpdateState::UpdateAvailableUnverifiable,
                DemoUpdateState::InFlight,
                DemoUpdateState::UpdateFailed,
                DemoUpdateState::InstallFailed,
            ]
        );
        assert!(DEMO_INSTALLED_AGENTS[0].unverified);
        assert_eq!(
            demo_action_label(DemoUpdateState::UpdateAvailableUnverifiable),
            None
        );
    }

    #[test]
    fn update_rows_show_old_and_new_versions_and_the_right_gesture() {
        let registry = parse_registry(include_str!("registry-sample.json"));
        let demo = DEMO_INSTALLED_AGENTS
            .iter()
            .find(|demo| demo.registry_id == "kilo")
            .unwrap();
        let agent = registry
            .iter()
            .find(|agent| agent.id == demo.registry_id)
            .unwrap();
        assert_eq!(
            demo_version_text(demo, agent),
            Some("v7.4.0 → v7.4.23".to_owned())
        );
        assert_eq!(demo_action_label(demo.state), Some("Update"));
        assert_eq!(
            demo_status_lines(demo, agent),
            vec![
                "v7.4.0 stays in use until you relaunch · restored if the update fails".to_owned()
            ]
        );
    }

    #[test]
    fn unverifiable_update_names_the_agent_version_and_unchecked_executable_risk() {
        assert_eq!(
            unverifiable_confirmation_heading("Google Antigravity", "1.0.0"),
            "No checksum for Google Antigravity v1.0.0"
        );
        assert_eq!(
            unverifiable_confirmation_body(),
            "Its publisher does not publish checksums and Sirio found none from any other source, so it cannot tell whether the file it downloaded from releases.antigravity.dev is the one they built. If it was tampered with, Sirio will run it with your permissions."
        );
        assert_eq!(
            unverifiable_confirmation_survival("0.9.0"),
            "v0.9.0 is restored if the update fails — not after it succeeds."
        );
        assert_eq!(confirmation_primary_label(), "Install unverified v1.0.0");
        assert_eq!(confirmation_secondary_label(), "Keep v0.9.0");
        assert_eq!(confirmation_default_focus(), "Keep v0.9.0");
    }

    #[test]
    fn update_in_flight_and_failure_copy_preserve_the_old_agent_state() {
        let registry = parse_registry(include_str!("registry-sample.json"));
        let updating = DEMO_INSTALLED_AGENTS
            .iter()
            .find(|demo| demo.state == DemoUpdateState::InFlight)
            .unwrap();
        let updating_agent = registry
            .iter()
            .find(|agent| agent.id == updating.registry_id)
            .unwrap();
        assert_eq!(
            demo_status_lines(updating, updating_agent),
            vec![
                "Updating to v1.47.0…".to_owned(),
                "v1.46.0 stays in use until you relaunch".to_owned(),
            ]
        );

        let failed = DEMO_INSTALLED_AGENTS
            .iter()
            .find(|demo| demo.state == DemoUpdateState::UpdateFailed)
            .unwrap();
        let failed_agent = registry
            .iter()
            .find(|agent| agent.id == failed.registry_id)
            .unwrap();
        assert_eq!(
            demo_status_lines(failed, failed_agent),
            vec![
                "Update to v0.10.116 failed — v0.10.115 is still installed and working.".to_owned(),
                "The archive download was interrupted.".to_owned(),
            ]
        );
        assert_eq!(demo_action_label(failed.state), Some("Retry"));
    }

    #[test]
    fn failed_install_has_no_false_reassurance_and_can_retry() {
        let registry = parse_registry(include_str!("registry-sample.json"));
        let demo = DEMO_INSTALLED_AGENTS
            .iter()
            .find(|demo| demo.state == DemoUpdateState::InstallFailed)
            .unwrap();
        let agent = registry
            .iter()
            .find(|agent| agent.id == demo.registry_id)
            .unwrap();
        assert_eq!(demo_version_text(demo, agent), None);
        assert_eq!(
            demo_status_lines(demo, agent),
            vec![
                "Install failed.".to_owned(),
                "The package manager could not install v0.22.1.".to_owned(),
            ]
        );
        assert_eq!(demo_action_label(demo.state), Some("Retry"));
    }

    #[test]
    fn paths_keep_the_distinguishing_suffix_and_columns_stay_fixed() {
        let kilo = DEMO_INSTALLED_AGENTS
            .iter()
            .find(|demo| demo.registry_id == "kilo")
            .unwrap();
        assert_eq!(
            demo_path_text(kilo),
            Some(r"%LOCALAPPDATA%\Sirio\agents\kilo\bin".to_owned())
        );
        let qwen = DEMO_INSTALLED_AGENTS
            .iter()
            .find(|demo| demo.registry_id == "qwen-code")
            .unwrap();
        assert_eq!(demo_path_text(qwen), None);
        assert_eq!(DEMO_PATH_COLUMN_WIDTH, 220.0);
        assert_eq!(DEMO_VERSION_COLUMN_WIDTH, 150.0);
        assert_eq!(
            demo_version_marker(&DEMO_INSTALLED_AGENTS[0]),
            Some("unverified")
        );
        let failed = DEMO_INSTALLED_AGENTS
            .iter()
            .find(|demo| demo.state == DemoUpdateState::UpdateFailed)
            .unwrap();
        assert_eq!(demo_version_marker(failed), Some("failed"));
    }

    #[test]
    fn demo_columns_have_non_collapsing_slots_and_a_real_gap() {
        use gpui::Styled;

        let mut path_slot = demo_fixed_slot(DEMO_PATH_COLUMN_WIDTH);
        let mut version_slot = demo_version_slot();
        assert!(Styled::style(&mut path_slot).min_size.width.is_some());
        assert!(Styled::style(&mut version_slot).min_size.width.is_some());
        assert!(Styled::style(&mut version_slot).margin.left.is_some());
        assert_eq!(Styled::style(&mut path_slot).flex_shrink, Some(0.0));
        assert_eq!(Styled::style(&mut version_slot).flex_shrink, Some(0.0));
        assert!(DEMO_COLUMN_GAP > 0.0);
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
        assert_eq!(style.background, Some(theme.element_active.into()));
        assert_eq!(style.text.font_size, Some(theme.typography.caption2.into()));
        assert_eq!(style.text.font_weight, Some(FontWeight::SEMIBOLD));
        assert_eq!(builtin_state_margin_right(false), px(8.0));
        assert_eq!(builtin_state_margin_right(true), px(0.0));
    }
}
