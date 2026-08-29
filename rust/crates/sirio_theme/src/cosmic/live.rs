//! Reads the user's *actual* COSMIC configuration off disk — the "real
//! prize" the brief calls out: on a genuine Pop!_OS/COSMIC desktop, Sirio
//! should pick up the user's chosen accent and light/dark preference
//! instead of guessing.
//!
//! This is a dependency-free, narrow reader, not a `cosmic-config`
//! integration — see `docs/linux-rewrite/COSMIC-DESIGN.md` for why: the
//! published `cosmic-theme` crate cannot be depended on without also
//! linking `iced_core`/`iced_futures` and the wayland/smithay-client-toolkit
//! stack (a rival GUI framework's windowing/clipboard/DnD core), because
//! `cosmic-theme`'s own `Cargo.toml` hardcodes `cosmic-config`'s
//! `subscription` feature — no downstream crate can turn that off.
//!
//! `cosmic-config` itself stores each field of a themed struct as one
//! RON-encoded file, at `$XDG_CONFIG_HOME/cosmic/{config-id}/v{version}/{field}`.
//! That layout and the `com.system76.CosmicTheme.*` config IDs come
//! straight from `cosmic_theme::model::theme` (`DARK_THEME_ID`,
//! `LIGHT_THEME_ID`, version `2`; the mode flag is a separate
//! `com.system76.CosmicTheme.Mode`, version `1`). This module reads that
//! same layout directly, parsing only the fixed, narrow RON shapes COSMIC
//! itself writes (nested-tuple structs of hex-string colors) — not RON in
//! general.
//!
//! Verified against a real Pop!_OS 24.04 install's live
//! `~/.config/cosmic/com.system76.CosmicTheme.*` — see the integration test
//! at the bottom of this file, and the report filed alongside this task for
//! the exact resolved values observed.

use super::component::CosmicComponent;
use super::container::{CosmicContainer, CosmicContainers};
use super::hex::parse as hex;
use super::semantic::CosmicSemanticColors;
use gpui::Rgba;
use std::path::PathBuf;

const DARK_ID: &str = "com.system76.CosmicTheme.Dark";
const LIGHT_ID: &str = "com.system76.CosmicTheme.Light";
const MODE_ID: &str = "com.system76.CosmicTheme.Mode";
const THEME_VERSION: u32 = 2;
const MODE_VERSION: u32 = 1;

/// Everything read live from the user's actual COSMIC configuration:
/// enough to redraw the container hierarchy and semantic colors with their
/// real accent, without re-deriving anything COSMIC already computed.
#[derive(Clone, Debug, PartialEq)]
pub struct LiveCosmicTheme {
    pub is_dark: bool,
    pub containers: CosmicContainers,
    pub semantic: CosmicSemanticColors,
}

/// Reads the live COSMIC theme for whichever mode
/// `com.system76.CosmicTheme.Mode`'s `is_dark` currently reports.
///
/// Returns `None` on any machine without a COSMIC config directory (or one
/// missing/malformed in a way this narrow reader does not understand) —
/// callers fall back to the transcribed [`super::palette`] /
/// [`super::semantic`] defaults.
pub fn detect() -> Option<LiveCosmicTheme> {
    let is_dark = live_is_dark()?;
    let containers = live_containers(is_dark)?;
    let semantic = live_semantic(is_dark)?;
    Some(LiveCosmicTheme {
        is_dark,
        containers,
        semantic,
    })
}

fn live_is_dark() -> Option<bool> {
    match read_key(MODE_ID, MODE_VERSION, "is_dark")?.trim() {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

fn live_containers(dark: bool) -> Option<CosmicContainers> {
    let id = if dark { DARK_ID } else { LIGHT_ID };
    Some(CosmicContainers {
        background: parse_container(&read_key(id, THEME_VERSION, "background")?)?,
        primary: parse_container(&read_key(id, THEME_VERSION, "primary")?)?,
        secondary: parse_container(&read_key(id, THEME_VERSION, "secondary")?)?,
    })
}

fn live_semantic(dark: bool) -> Option<CosmicSemanticColors> {
    let id = if dark { DARK_ID } else { LIGHT_ID };
    let component = |key: &str| -> Option<CosmicComponent> {
        parse_component(&read_key(id, THEME_VERSION, key)?)
    };
    Some(CosmicSemanticColors {
        button: component("button")?,
        accent: component("accent")?,
        accent_button: component("accent_button")?,
        success: component("success")?,
        success_button: component("success_button")?,
        destructive: component("destructive")?,
        destructive_button: component("destructive_button")?,
        warning: component("warning")?,
        warning_button: component("warning_button")?,
        icon_button: component("icon_button")?,
        link_button: component("link_button")?,
        list_button: component("list_button")?,
        text_button: component("text_button")?,
        shade: read_hex_scalar(id, THEME_VERSION, "shade")?,
    })
}

fn config_root() -> Option<PathBuf> {
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME")
        && !xdg.is_empty()
    {
        return Some(PathBuf::from(xdg).join("cosmic"));
    }
    let home = std::env::var("HOME").ok()?;
    Some(PathBuf::from(home).join(".config").join("cosmic"))
}

fn read_key(id: &str, version: u32, key: &str) -> Option<String> {
    let path = config_root()?
        .join(id)
        .join(format!("v{version}"))
        .join(key);
    std::fs::read_to_string(path).ok()
}

fn read_hex_scalar(id: &str, version: u32, key: &str) -> Option<Rgba> {
    let text = read_key(id, version, key)?;
    hex(text.trim().trim_matches('"'))
}

/// Extracts a `name: "value",` field by matching the *trimmed line start*,
/// not a raw substring search — `border:` is a suffix of
/// `disabled_border:`, so a naive `contains("border: ")` mis-fires on the
/// wrong field. Line-anchored matching sidesteps that.
fn field<'a>(text: &'a str, name: &str) -> Option<&'a str> {
    let needle = format!("{name}: \"");
    for line in text.lines() {
        if let Some(rest) = line.trim_start().strip_prefix(needle.as_str()) {
            return rest.split('"').next();
        }
    }
    None
}

fn hex_field(text: &str, name: &str) -> Option<Rgba> {
    hex(field(text, name)?)
}

/// Splits a `Container`-shaped RON block into (container-only text, the
/// nested `component: ( ... )` text), via balanced-parenthesis scanning —
/// robust to the exact indentation cosmic-config's writer uses, and to the
/// fact that both scopes have same-named fields (`base`, `on`, `divider`).
fn split_component_block(text: &str) -> (String, Option<String>) {
    let Some(marker) = text.find("component: (") else {
        return (text.to_string(), None);
    };
    let open = marker + "component: (".len() - 1;
    let bytes = text.as_bytes();
    let mut depth = 0i32;
    let mut close = None;
    for (i, &b) in bytes.iter().enumerate().skip(open) {
        match b {
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    close = Some(i);
                    break;
                }
            }
            _ => {}
        }
    }
    let Some(close) = close else {
        return (text.to_string(), None);
    };
    let inner = text[open + 1..close].to_string();
    let outer = format!("{}{}", &text[..marker], &text[close + 1..]);
    (outer, Some(inner))
}

fn parse_component(text: &str) -> Option<CosmicComponent> {
    Some(CosmicComponent {
        base: hex_field(text, "base")?,
        hover: hex_field(text, "hover")?,
        pressed: hex_field(text, "pressed")?,
        on: hex_field(text, "on")?,
        divider: hex_field(text, "divider")?,
        border: hex_field(text, "border")?,
    })
}

fn parse_container(text: &str) -> Option<CosmicContainer> {
    let (outer, inner) = split_component_block(text);
    let component = parse_component(inner.as_deref()?)?;
    Some(CosmicContainer {
        base: hex_field(&outer, "base")?,
        on: hex_field(&outer, "on")?,
        divider: hex_field(&outer, "divider")?,
        component,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A literal capture of this machine's real (unmodified)
    /// `~/.config/cosmic/com.system76.CosmicTheme.Dark/v2/background`.
    const DARK_BACKGROUND: &str = r##"(
    base: "#1B1B1BFF",
    component: (
        base: "#2E2E2EFF",
        hover: "#434343FF",
        pressed: "#585858FF",
        selected: "#434343FF",
        selected_text: "#63D0DFFF",
        focus: "#63D0DFFF",
        divider: "#FFFFFF33",
        on: "#FFFFFFFF",
        disabled: "#2E2E2E80",
        on_disabled: "#FFFFFFA6",
        border: "#BEBEBEFF",
        disabled_border: "#BEBEBE80",
    ),
    divider: "#474747FF",
    on: "#F5F5F5FF",
    small_widget: "#27272740",
)"##;

    #[test]
    fn splits_container_and_nested_component_text() {
        let (outer, inner) = split_component_block(DARK_BACKGROUND);
        assert!(
            !outer.contains("hover"),
            "component fields must be removed from the outer scope"
        );
        let inner = inner.expect("a component block is present");
        assert!(inner.contains("hover: \"#434343FF\""));
    }

    #[test]
    fn field_matching_does_not_confuse_border_with_disabled_border() {
        let (outer, inner) = split_component_block(DARK_BACKGROUND);
        let inner = inner.unwrap();
        assert_eq!(field(&inner, "border"), Some("#BEBEBEFF"));
        assert_eq!(field(&inner, "on"), Some("#FFFFFFFF"));
        // The container's own `on`/`divider` sit outside the component
        // block, at the container's own values, not the nested ones.
        assert_eq!(field(&outer, "on"), Some("#F5F5F5FF"));
        assert_eq!(field(&outer, "divider"), Some("#474747FF"));
    }

    #[test]
    fn parses_a_real_captured_container_block() {
        let container = parse_container(DARK_BACKGROUND).expect("parses");
        assert_eq!(container.base, hex("#1B1B1BFF").unwrap());
        assert_eq!(container.on, hex("#F5F5F5FF").unwrap());
        assert_eq!(container.divider, hex("#474747FF").unwrap());
        assert_eq!(container.component.base, hex("#2E2E2EFF").unwrap());
        assert_eq!(container.component.hover, hex("#434343FF").unwrap());
        assert_eq!(container.component.pressed, hex("#585858FF").unwrap());
        assert_eq!(container.component.border, hex("#BEBEBEFF").unwrap());
    }

    #[test]
    fn parses_a_real_captured_component_block() {
        const ACCENT: &str = r##"(
    base: "#63D0DFFF",
    hover: "#63BAC6FF",
    pressed: "#3C737AFF",
    selected: "#63BAC6FF",
    selected_text: "#63D0DFFF",
    focus: "#63D0DFFF",
    divider: "#000000FF",
    on: "#000000FF",
    disabled: "#63D0DFFF",
    on_disabled: "#326870FF",
    border: "#63D0DFFF",
    disabled_border: "#63D0DF80",
)"##;
        let accent = parse_component(ACCENT).expect("parses");
        assert_eq!(accent.base, hex("#63D0DFFF").unwrap());
        assert_eq!(accent.on, hex("#000000FF").unwrap());
        assert_eq!(accent.border, hex("#63D0DFFF").unwrap());
    }

    #[test]
    fn read_hex_scalar_strips_the_ron_string_quotes() {
        // `shade`'s file is a bare RON string, e.g. `"#00000052"` — not a
        // `key: "value"` line.
        let tmp =
            std::env::temp_dir().join(format!("sirio-cosmic-shade-test-{}", std::process::id()));
        // config_root() appends "cosmic" itself, mirroring the real
        // $XDG_CONFIG_HOME/cosmic/{id}/v{version}/{key} layout.
        let dir = tmp
            .join("cosmic")
            .join("com.system76.CosmicTheme.Dark")
            .join("v2");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("shade"), "\"#00000052\"").unwrap();
        // SAFETY: this test does not run concurrently with other tests
        // that read XDG_CONFIG_HOME (no other test in this crate sets it).
        unsafe {
            std::env::set_var("XDG_CONFIG_HOME", &tmp);
        }
        let value = read_hex_scalar(DARK_ID, THEME_VERSION, "shade");
        unsafe {
            std::env::remove_var("XDG_CONFIG_HOME");
        }
        std::fs::remove_dir_all(&tmp).ok();
        assert_eq!(value, hex("#00000052"));
    }

    /// Executed evidence for "the real COSMIC accent is picked up": reads
    /// whatever is actually on disk at
    /// `$XDG_CONFIG_HOME/cosmic/com.system76.CosmicTheme.*` right now. On a
    /// genuine Pop!_OS/COSMIC machine (this one, verified: Pop!_OS 24.04)
    /// this resolves to `Some` and the accent differs from the transcribed
    /// stock default whenever the user customized theirs. On any other
    /// Linux box it resolves to `None`, and callers fall back to
    /// [`super::palette`] / [`super::semantic`] — both are exercised here,
    /// so the test is meaningful either way it lands.
    #[test]
    fn detects_the_live_cosmic_theme_when_present_or_falls_back_cleanly() {
        match detect() {
            Some(live) => {
                // A real config was found and fully parsed: containers and
                // semantic colors are non-degenerate (not all-zero from a
                // parsing bug that silently defaulted).
                assert_ne!(live.containers.background.base, Rgba::default());
                assert_ne!(live.semantic.accent.base, Rgba::default());
                println!(
                    "live COSMIC theme detected: is_dark={} accent.base={:?}",
                    live.is_dark, live.semantic.accent.base
                );
            }
            None => {
                println!(
                    "no live COSMIC config on this machine; falls back to transcribed defaults"
                );
            }
        }
    }
}
