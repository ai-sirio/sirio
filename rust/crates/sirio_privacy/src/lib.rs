//! The macOS privacy (TCC) permissions a pane's tools depend on.
//!
//! A CLI or agent running in a Sirio pane inherits Sirio's TCC envelope: the
//! child processes are attributed to the app that launched them, so a screen
//! capture or an Apple Event from `claude` in a pane is granted or refused on
//! Sirio's behalf. Settings → Permissions is where that envelope is read and
//! changed; this crate is everything that page knows, without any UI.
//!
//! Ported in intent from the Swift app's `TillerCore/Permissions.swift` and
//! `App/Permissions/SystemPermissionProbe.swift` (read them with `git show
//! 5430d7bfdb4a295be8ce072526ae5108259b80f8:<path>`). The Rust port had kept
//! that page's look and none of its logic — every status was a literal and
//! every button a no-op — which is why it is a crate of its own now: the part
//! that decides is plain data and testable on any platform, and the part that
//! asks macOS is the only part that is not.

pub mod notifications;
pub mod probe;

/// One TCC permission the Permissions page shows, in display order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PermissionKind {
    Notifications,
    ScreenRecording,
    Accessibility,
    FullDiskAccess,
    Automation,
    LocalNetwork,
}

impl PermissionKind {
    /// Every permission, in the order the page lists them.
    pub const ALL: [Self; 6] = [
        Self::Notifications,
        Self::ScreenRecording,
        Self::Accessibility,
        Self::FullDiskAccess,
        Self::Automation,
        Self::LocalNetwork,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Self::Notifications => "Notifications",
            Self::ScreenRecording => "Screen Recording",
            Self::Accessibility => "Accessibility",
            Self::FullDiskAccess => "Full Disk Access",
            Self::Automation => "Automation",
            Self::LocalNetwork => "Local Network",
        }
    }

    pub fn detail(self) -> &'static str {
        match self {
            Self::Notifications => "Alerts when agents finish or need input.",
            Self::ScreenRecording => "Screenshot, visual automation, and UI inspection tools.",
            Self::Accessibility => "Keystroke injection, window control, and UI automation tools.",
            Self::FullDiskAccess => {
                "Recommended when projects or worktrees touch macOS-protected folders."
            }
            Self::Automation => "Apple Events for scripts that control other local apps.",
            Self::LocalNetwork => "Discovery and access for development servers on your network.",
        }
    }

    /// A stable, content-free slug for element ids and selectors.
    pub fn slug(self) -> &'static str {
        match self {
            Self::Notifications => "notifications",
            Self::ScreenRecording => "screen-recording",
            Self::Accessibility => "accessibility",
            Self::FullDiskAccess => "full-disk-access",
            Self::Automation => "automation",
            Self::LocalNetwork => "local-network",
        }
    }

    /// The System Settings pane that holds this permission's switch. Opened
    /// with the platform's URL handler (`NSWorkspace openURL:`), which is
    /// what resolves the `x-apple.systempreferences` scheme.
    pub fn settings_url(self) -> &'static str {
        match self {
            Self::Notifications => {
                "x-apple.systempreferences:com.apple.Notifications-Settings.extension"
            }
            Self::ScreenRecording => {
                "x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture"
            }
            Self::Accessibility => {
                "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility"
            }
            Self::FullDiskAccess => {
                "x-apple.systempreferences:com.apple.preference.security?Privacy_AllFiles"
            }
            Self::Automation => {
                "x-apple.systempreferences:com.apple.preference.security?Privacy_Automation"
            }
            Self::LocalNetwork => {
                "x-apple.systempreferences:com.apple.preference.security?Privacy_LocalNetwork"
            }
        }
    }
}

/// What macOS says about one permission.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PermissionStatus {
    Granted,
    Denied,
    /// The prompt has never been shown. macOS shows each one once; after an
    /// answer, only System Settings can change it.
    NotRequested,
    /// No reliable way to read it: Local Network has no check API at all,
    /// Full Disk Access only a canary, and Automation needs its target
    /// running before it can answer.
    CheckManually,
}

impl PermissionStatus {
    /// The row's state, in the lower-case words the other Settings rows use
    /// for theirs ("on PATH", "absent").
    pub fn label(self) -> &'static str {
        match self {
            Self::Granted => "granted",
            Self::Denied => "denied",
            Self::NotRequested => "not requested",
            Self::CheckManually => "check manually",
        }
    }
}

/// The one action a row offers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PermissionAction {
    /// Show macOS's own prompt for a permission that has an API to ask.
    Request,
    /// Do the thing that makes macOS ask — for permissions with no request
    /// API, only a first use.
    TriggerPrompt,
    /// Open the System Settings pane that holds the switch.
    OpenSettings,
}

impl PermissionAction {
    pub fn label(self) -> &'static str {
        match self {
            Self::Request => "Request",
            Self::TriggerPrompt => "Trigger Prompt",
            Self::OpenSettings => "Open Settings",
        }
    }
}

/// The action a row offers for `kind` in `status`.
pub fn action_for(kind: PermissionKind, status: PermissionStatus) -> PermissionAction {
    use PermissionAction::*;
    use PermissionKind::*;
    use PermissionStatus::*;
    match (kind, status) {
        // No request API exists for it.
        (FullDiskAccess, _) => OpenSettings,
        // Answered: macOS will not show the prompt again.
        (_, Granted | Denied) => OpenSettings,
        // No request API either, but a first use makes macOS ask — and for
        // Automation, launches the System Events it needs to answer at all.
        (LocalNetwork | Automation, _) => TriggerPrompt,
        (_, NotRequested) => Request,
        // Asked before, or unreadable: the switch is the only sure road.
        (_, CheckManually) => OpenSettings,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const STATUSES: [PermissionStatus; 4] = [
        PermissionStatus::Granted,
        PermissionStatus::Denied,
        PermissionStatus::NotRequested,
        PermissionStatus::CheckManually,
    ];

    /// macOS shows a TCC prompt once. After an answer, a Request button
    /// calls an API that returns without showing anything — the "Trigger
    /// Prompt does nothing" report, arrived at by a different road.
    #[test]
    fn an_answered_permission_only_ever_opens_settings() {
        for kind in PermissionKind::ALL {
            for status in [PermissionStatus::Granted, PermissionStatus::Denied] {
                assert_eq!(
                    action_for(kind, status),
                    PermissionAction::OpenSettings,
                    "{kind:?} {status:?} offers a prompt macOS will not show again"
                );
            }
        }
    }

    /// Full Disk Access has no request API at all: the only road to it is
    /// the user adding the app in System Settings.
    #[test]
    fn full_disk_access_only_ever_opens_settings() {
        for status in STATUSES {
            assert_eq!(
                action_for(PermissionKind::FullDiskAccess, status),
                PermissionAction::OpenSettings,
                "Full Disk Access {status:?}"
            );
        }
    }

    /// An app appears in a privacy pane's list only after it has asked.
    /// Sending the user to System Settings for a permission never requested
    /// sends them to a list Sirio is not in.
    #[test]
    fn a_never_asked_permission_offers_the_prompt_not_settings() {
        for kind in [
            PermissionKind::Notifications,
            PermissionKind::ScreenRecording,
            PermissionKind::Accessibility,
            PermissionKind::Automation,
        ] {
            assert_ne!(
                action_for(kind, PermissionStatus::NotRequested),
                PermissionAction::OpenSettings,
                "{kind:?} was never asked for, so System Settings does not list Sirio"
            );
        }
    }

    /// Local Network has no status API, so the probe can only ever say
    /// "check manually" — and a first use is the one lever that exists.
    /// Automation says the same when System Events is not running, and the
    /// trigger is what launches it.
    #[test]
    fn a_permission_that_cannot_be_read_offers_the_trigger() {
        for kind in [PermissionKind::LocalNetwork, PermissionKind::Automation] {
            assert_eq!(
                action_for(kind, PermissionStatus::CheckManually),
                PermissionAction::TriggerPrompt,
                "{kind:?}"
            );
        }
    }

    /// Six rows, six panes: a copy-paste slip here opens the wrong switch
    /// with nothing on screen to say so.
    #[test]
    fn each_row_opens_its_own_settings_pane() {
        let urls: std::collections::HashSet<_> = PermissionKind::ALL
            .iter()
            .map(|kind| kind.settings_url())
            .collect();
        assert_eq!(urls.len(), PermissionKind::ALL.len());
        for url in urls {
            assert!(url.starts_with("x-apple.systempreferences:"), "{url}");
        }
    }
}
