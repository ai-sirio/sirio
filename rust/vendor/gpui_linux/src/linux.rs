mod dispatcher;
mod headless;
mod keyboard;
mod platform;
mod system_notifications;
#[cfg(any(feature = "wayland", feature = "x11"))]
mod text_system;
#[cfg(feature = "wayland")]
mod wayland;
#[cfg(feature = "x11")]
mod x11;

#[cfg(any(feature = "wayland", feature = "x11"))]
mod xdg_desktop_portal;

pub use dispatcher::*;
pub(crate) use headless::*;
pub(crate) use keyboard::*;
pub(crate) use platform::*;
#[cfg(any(feature = "wayland", feature = "x11"))]
pub(crate) use text_system::*;
#[cfg(feature = "wayland")]
pub(crate) use wayland::*;
#[cfg(feature = "x11")]
pub(crate) use x11::*;

use std::rc::Rc;

/// The environment every child process gpui starts (`xdg-open`, the URL
/// opener) runs with, once the app has set it; unset, they inherit the
/// process's own, as they always did.
static CHILD_ENVIRONMENT: std::sync::OnceLock<Vec<(String, String)>> = std::sync::OnceLock::new();

/// Sets the environment gpui's child processes run with: each is started with
/// its inherited environment cleared and these pairs given instead. Set once,
/// before the first link is opened; a later call is ignored.
pub fn set_child_environment(env: Vec<(String, String)>) {
    let _ = CHILD_ENVIRONMENT.set(env);
}

/// Gives `command` the environment `set_child_environment` set, if any.
pub(crate) fn with_child_environment(
    mut command: std::process::Command,
) -> std::process::Command {
    if let Some(env) = CHILD_ENVIRONMENT.get() {
        command.env_clear().envs(env.iter().map(|(key, value)| (key, value)));
    }
    command
}

/// Returns the default platform implementation for the current OS.
pub fn current_platform(headless: bool) -> Rc<dyn gpui::Platform> {
    #[cfg(feature = "x11")]
    use anyhow::Context as _;

    if headless {
        return Rc::new(LinuxPlatform {
            inner: HeadlessClient::new(),
        });
    }

    match gpui::guess_compositor() {
        #[cfg(feature = "wayland")]
        "Wayland" => Rc::new(LinuxPlatform {
            inner: WaylandClient::new(),
        }),

        #[cfg(feature = "x11")]
        "X11" => Rc::new(LinuxPlatform {
            inner: X11Client::new()
                .context("Failed to initialize X11 client.")
                .unwrap(),
        }),

        "Headless" => Rc::new(LinuxPlatform {
            inner: HeadlessClient::new(),
        }),
        _ => unreachable!(
            r#"At least one of the "wayland" or "x11" features must be enabled on gpui_linux or gpui_platform."#
        ),
    }
}
