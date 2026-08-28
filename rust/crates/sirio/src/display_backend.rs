//! Linux display-backend policy: Sirio asks for X11.
//!
//! # Why
//!
//! The Browser tab's page lives in a real X11 child window reparented into
//! Sirio's own window — that is what `wry`'s `build_as_child` does on Linux.
//! Native Wayland has no way to be that window's parent, and not for want of
//! effort: `wl_surface` is a per-client protocol object with no cross-client
//! reparent, GTK4 removed XEmbed/`GtkSocket`/`GtkPlug` without a replacement,
//! and `xdg_toplevel` has no `set_position`, so a separate top-level cannot be
//! made to track a pane rectangle either. On a Wayland session the browser
//! therefore cannot render at all.
//!
//! The user's decision, 2026-08-19, recorded in
//! `docs/linux-rewrite/DECISION-browser-on-wayland.md`: force X11. On a Wayland
//! session Sirio runs as an XWayland client and the browser works everywhere.
//! The cost was named before the choice and accepted — scaling, per-monitor DPI,
//! input and clipboard become XWayland's rather than the compositor's.
//!
//! # How
//!
//! No GPUI patch, and none should be written. `gpui::guess_compositor()` picks
//! the backend from the **process environment** alone: `WAYLAND_DISPLAY`
//! non-empty wins, else `DISPLAY` non-empty, else headless. So the decision is
//! made by preparing that environment before GPUI reads it.
//!
//! Two variables, and both are required:
//!
//! - clearing `WAYLAND_DISPLAY` moves **GPUI** to X11;
//! - setting `GDK_BACKEND=x11` moves **GDK**, and therefore wry, to X11 —
//!   `wry` builds its child window through `gdk_x11_display_get_xdisplay`, which
//!   needs an X11 `GdkDisplay` to return one.
//!
//! Doing only the first is the failure worth naming: GPUI gives you a real X11
//! window, GDK stays on Wayland, and the webview still cannot attach. The
//! symptom is identical to doing nothing at all.

use std::ffi::OsStr;

/// The environment variable that opts out. On the existing `TILLER_*` precedent
/// (`TILLER_SOCKET_ENABLE`, `TILLER_GIT_TIMEOUT_MS`), and parsed with the same
/// vocabulary `SettingsPolicy::with_environment_override` uses.
const FORCE_FLAG: &str = "TILLER_FORCE_X11";

/// What [`prepare_environment`] should do, decided from the environment alone
/// so the policy can be tested without a display server.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BackendChoice {
    /// Clear `WAYLAND_DISPLAY`, set `GDK_BACKEND=x11`.
    ForceX11,
    /// `TILLER_FORCE_X11` was set to an off value. The user wants a
    /// Wayland-native app and has accepted that the browser will not render.
    OptedOut,
    /// No usable `DISPLAY`. There is no X server and no XWayland to move to, so
    /// forcing here would turn a working Wayland app into a headless one —
    /// `guess_compositor` falls through to "Headless" when neither variable is
    /// set. Leaving the environment alone is the only safe answer.
    NoXServer,
}

/// Reads the policy off two environment values. Pure on purpose.
pub(crate) fn choose(display: Option<&str>, force_flag: Option<&str>) -> BackendChoice {
    if force_flag.is_some_and(|flag| {
        matches!(
            flag.trim().to_ascii_lowercase().as_str(),
            "0" | "false" | "no" | "off"
        )
    }) {
        return BackendChoice::OptedOut;
    }
    // An unset, empty or whitespace-only DISPLAY is not an X server. Note this
    // is checked *after* the opt-out but *regardless of* an explicit opt-in:
    // `TILLER_FORCE_X11=1` cannot conjure a display, and honouring it here
    // would send the app headless.
    match display {
        Some(display) if !display.trim().is_empty() => BackendChoice::ForceX11,
        _ => BackendChoice::NoXServer,
    }
}

/// Applies the policy to this process's own environment.
///
/// Must be called as the first thing `main` does. That is not style: in edition
/// 2024 `set_var`/`remove_var` are `unsafe` because another thread reading the
/// environment concurrently is a data race, and "no other thread exists yet" is
/// what makes the call sound.
pub(crate) fn prepare_environment() {
    let display = std::env::var("DISPLAY").ok();
    let force_flag = std::env::var(FORCE_FLAG).ok();
    let on_wayland = std::env::var_os("WAYLAND_DISPLAY")
        .as_deref()
        .is_some_and(|value| !value.is_empty());

    match choose(display.as_deref(), force_flag.as_deref()) {
        BackendChoice::ForceX11 => {
            // SAFETY: called as the first statement of `main`, before any
            // thread in this process exists, so nothing can be reading the
            // environment concurrently.
            unsafe {
                std::env::remove_var("WAYLAND_DISPLAY");
                // Deliberately overwrites an explicit GDK_BACKEND. wry cannot
                // build its child window against a Wayland GdkDisplay, so
                // honouring GDK_BACKEND=wayland here would leave the app in the
                // exact broken state this whole module exists to avoid. The
                // way to keep Wayland is TILLER_FORCE_X11=0, which is checked
                // above and takes precedence over this.
                std::env::set_var("GDK_BACKEND", "x11");
            }
            if on_wayland {
                eprintln!(
                    "[display] Wayland session detected; running on X11 (XWayland) so the \
                     embedded browser can attach. Set {FORCE_FLAG}=0 to stay Wayland-native \
                     without it."
                );
            }
        }
        BackendChoice::OptedOut => {
            if on_wayland {
                eprintln!(
                    "[display] {FORCE_FLAG} is off: staying Wayland-native. The Browser tab \
                     cannot render a page on Wayland."
                );
            }
        }
        BackendChoice::NoXServer => {
            if on_wayland {
                eprintln!(
                    "[display] no usable DISPLAY, so X11 cannot be forced; staying on Wayland. \
                     The Browser tab cannot render a page here."
                );
            }
        }
    }
}

/// Kept so a caller can name the variable without repeating the literal.
#[allow(dead_code)]
pub(crate) fn force_flag_name() -> &'static OsStr {
    OsStr::new(FORCE_FLAG)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_usable_display_is_forced_to_x11() {
        assert_eq!(choose(Some(":0"), None), BackendChoice::ForceX11);
        assert_eq!(choose(Some(":1"), None), BackendChoice::ForceX11);
        // What a Wayland session with XWayland actually looks like: both are
        // set, and this is the case the whole decision exists for.
        assert_eq!(choose(Some(":1"), Some("1")), BackendChoice::ForceX11);
    }

    #[test]
    fn the_off_switch_uses_the_same_vocabulary_as_the_socket_flag() {
        for value in ["0", "false", "no", "off", "OFF", " off "] {
            assert_eq!(
                choose(Some(":1"), Some(value)),
                BackendChoice::OptedOut,
                "{value:?} should opt out"
            );
        }
        // Anything unrecognised is not an opt-out. Silently treating a typo as
        // "off" would turn a misspelling into a browser that never renders,
        // with nothing to point at.
        for value in ["1", "true", "yes", "on", "", "maybe"] {
            assert_eq!(
                choose(Some(":1"), Some(value)),
                BackendChoice::ForceX11,
                "{value:?} should not opt out"
            );
        }
    }

    /// The one that stops this from being a footgun. With no DISPLAY there is
    /// no X server; clearing `WAYLAND_DISPLAY` would leave `guess_compositor`
    /// with neither variable set, and it answers "Headless" — a windowless app.
    /// So an explicit opt-in must not be honoured either.
    #[test]
    fn without_a_display_nothing_is_forced_even_when_asked() {
        assert_eq!(choose(None, None), BackendChoice::NoXServer);
        assert_eq!(choose(Some(""), None), BackendChoice::NoXServer);
        assert_eq!(choose(Some("   "), None), BackendChoice::NoXServer);
        assert_eq!(choose(None, Some("1")), BackendChoice::NoXServer);
        assert_eq!(choose(Some(""), Some("true")), BackendChoice::NoXServer);
    }

    /// Opting out wins over everything, including a perfectly usable display —
    /// it is the user's escape hatch and must not be conditional.
    #[test]
    fn opting_out_wins_over_a_usable_display() {
        assert_eq!(choose(Some(":1"), Some("off")), BackendChoice::OptedOut);
        assert_eq!(choose(None, Some("off")), BackendChoice::OptedOut);
    }
}
