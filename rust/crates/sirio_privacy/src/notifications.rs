//! Which road a desktop notification takes on macOS.
//!
//! Until this existed Sirio posted every notification through `osascript -e
//! 'display notification …'`. macOS attributes those to Script Editor, not to
//! Sirio: they carry Script Editor's icon, obey Script Editor's switch in
//! System Settings → Notifications, and Sirio's own Notifications permission
//! governed nothing it ever showed. The native road is gpui's
//! `show_system_notification` (`UNUserNotificationCenter`), which is Sirio's
//! own and is what the Permissions row reads and requests.

use crate::PermissionStatus;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NotificationRoute {
    /// `UNUserNotificationCenter`, through gpui: Sirio's icon, Sirio's switch,
    /// a banner even while Sirio is frontmost. gpui asks for authorization
    /// the first time it posts, so this road can show the system prompt.
    Native,
    /// `osascript`: attributed to Script Editor, governed by its switch —
    /// the only road there was before, and the only one a process outside
    /// an app bundle has.
    AppleScript,
    /// Not shown at all.
    Suppressed,
}

/// The road for the next notification.
///
/// `bundled` is whether this process runs from an app bundle
/// ([`crate::probe::is_bundled`]); `status` is the last Notifications status
/// macOS reported ([`crate::probe::last_known_notifications`]), `None` until
/// the first read has come back.
pub fn route(bundled: bool, status: Option<PermissionStatus>) -> NotificationRoute {
    if !bundled {
        return NotificationRoute::AppleScript;
    }
    match status {
        Some(PermissionStatus::Granted) => NotificationRoute::Native,
        // The user said no to Sirio's own notifications; going round that
        // through Script Editor would overrule them.
        Some(PermissionStatus::Denied) => NotificationRoute::Suppressed,
        // Never asked, not read yet, or unreadable: the road that worked
        // before. The native road would make gpui ask on the first agent
        // notification — a prompt nobody requested, which also swallows the
        // notification that raised it. The Permissions page's Request is
        // where Sirio asks.
        Some(PermissionStatus::NotRequested | PermissionStatus::CheckManually) | None => {
            NotificationRoute::AppleScript
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `UNUserNotificationCenter` raises `NSInternalInconsistencyException`
    /// in a process with no bundle identifier — a `cargo run` build. gpui
    /// guards that by dropping the notification, so a native route here
    /// loses it without a trace.
    #[test]
    fn an_unbundled_process_never_takes_the_native_road() {
        for status in [
            None,
            Some(PermissionStatus::Granted),
            Some(PermissionStatus::Denied),
            Some(PermissionStatus::NotRequested),
            Some(PermissionStatus::CheckManually),
        ] {
            assert_ne!(
                route(false, status),
                NotificationRoute::Native,
                "{status:?}"
            );
        }
    }
}
