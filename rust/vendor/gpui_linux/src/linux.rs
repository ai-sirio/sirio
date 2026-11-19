mod dispatcher;
mod display_connection;
mod headless;
mod keyboard;
mod platform;
#[cfg(any(feature = "wayland", feature = "x11"))]
mod power;
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
pub(crate) use display_connection::{Backend, DisplayConnection, select_backend};
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

use gpui::WindowingModes;

/// The environment every opener gpui starts (`xdg-open`, the desktop's file and link
/// launchers) runs with, once the app has set it. Unset, they run as upstream starts them.
static CHILD_ENVIRONMENT: std::sync::OnceLock<Vec<(String, String)>> = std::sync::OnceLock::new();

/// Sets the environment gpui's openers run with: each is started with its inherited
/// environment cleared and exactly these pairs given instead (plus `XDG_ACTIVATION_TOKEN`
/// when gpui has one). Set once, before the first link is opened; a later call is ignored.
pub fn set_child_environment(env: Vec<(String, String)>) {
    let _ = CHILD_ENVIRONMENT.set(env);
}

/// Whether [`set_child_environment`] has been called.
pub(crate) fn child_environment_is_set() -> bool {
    CHILD_ENVIRONMENT.get().is_some()
}

/// Gives `command` the environment [`set_child_environment`] set, if any: its inherited
/// environment is replaced by exactly those pairs, plus `XDG_ACTIVATION_TOKEN` if given.
/// Unset, `command` is returned untouched.
pub(crate) fn with_child_environment(
    mut command: std::process::Command,
    activation_token: Option<&str>,
) -> std::process::Command {
    if let Some(env) = CHILD_ENVIRONMENT.get() {
        command
            .env_clear()
            .envs(env.iter().map(|(key, value)| (key, value)));
        if let Some(token) = activation_token {
            command.env("XDG_ACTIVATION_TOKEN", token);
        }
    }
    command
}

/// Returns the default platform implementation for the current OS.
///
/// A windowed platform connects to the display server the process environment names, or starts
/// headless when it names none, and can switch between those modes later. A headless platform
/// stays headless.
pub fn current_platform(headless: bool) -> Rc<dyn gpui::Platform> {
    if headless || std::env::var_os("ZED_HEADLESS").is_some() {
        linux_platform(WindowingModes::HEADLESS)
    } else {
        linux_platform(WindowingModes::all())
    }
}

/// Returns a platform that may switch among `allowed_modes`.
///
/// It starts windowed in the process's own environment, or headless if that names no allowed
/// display server. Set another initial mode with [`gpui::Application::with_windowing`].
pub fn linux_platform(allowed_modes: WindowingModes) -> Rc<dyn gpui::Platform> {
    Rc::new(LinuxPlatform::new(allowed_modes))
}
