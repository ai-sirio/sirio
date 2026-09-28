//! Reading and asking macOS. Everything here can block — a notification
//! read answers through a completion handler, and a prompt waits for the
//! user — so callers run it on a background thread, never the UI's.
//!
//! Off macOS nothing is read: every status is "check manually" and every
//! request is a no-op, so the host's wiring compiles and runs unchanged on
//! the platforms that never show the page.

use crate::{PermissionKind, PermissionStatus};

/// What macOS currently says about `kind`. Never shows a prompt.
pub fn status(kind: PermissionKind) -> PermissionStatus {
    #[cfg(target_os = "macos")]
    let status = macos::status(kind);
    #[cfg(not(target_os = "macos"))]
    let status = {
        let _ = kind;
        PermissionStatus::CheckManually
    };
    if kind == PermissionKind::Notifications {
        last_known::store(status);
    }
    status
}

/// Makes macOS ask for `kind`: its own prompt where there is an API for it,
/// a first use where there is not. Returns once the prompt is answered where
/// macOS says when that is (Notifications, Automation), at once otherwise.
/// Full Disk Access has neither — its row only ever opens System Settings.
pub fn request(kind: PermissionKind) {
    #[cfg(target_os = "macos")]
    macos::request(kind);
    #[cfg(not(target_os = "macos"))]
    let _ = kind;
}

/// Whether this process runs from an app bundle. Outside one,
/// `UNUserNotificationCenter` aborts the process on first touch.
pub fn is_bundled() -> bool {
    #[cfg(target_os = "macos")]
    return macos::is_bundled();
    #[cfg(not(target_os = "macos"))]
    false
}

/// The last Notifications status [`status`] read, `None` before the first.
/// The notification path reads this rather than asking, because it runs
/// on threads that must not wait for a completion handler.
pub fn last_known_notifications() -> Option<PermissionStatus> {
    last_known::load()
}

mod last_known {
    use std::sync::atomic::{AtomicU8, Ordering};

    use crate::PermissionStatus;

    /// 0 is "never read"; the rest are the statuses in declaration order.
    static NOTIFICATIONS: AtomicU8 = AtomicU8::new(0);

    pub(super) fn store(status: PermissionStatus) {
        let encoded = match status {
            PermissionStatus::Granted => 1,
            PermissionStatus::Denied => 2,
            PermissionStatus::NotRequested => 3,
            PermissionStatus::CheckManually => 4,
        };
        NOTIFICATIONS.store(encoded, Ordering::Relaxed);
    }

    pub(super) fn load() -> Option<PermissionStatus> {
        match NOTIFICATIONS.load(Ordering::Relaxed) {
            1 => Some(PermissionStatus::Granted),
            2 => Some(PermissionStatus::Denied),
            3 => Some(PermissionStatus::NotRequested),
            4 => Some(PermissionStatus::CheckManually),
            _ => None,
        }
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use std::ffi::c_void;
    use std::net::{Ipv4Addr, UdpSocket};
    use std::path::Path;
    use std::process::Command;
    use std::ptr::NonNull;
    use std::sync::mpsc;
    use std::time::Duration;

    use block2::RcBlock;
    use objc2::rc::Retained;
    use objc2::runtime::Bool;
    use objc2_foundation::{NSBundle, NSDictionary, NSError, NSNumber, NSString, NSUserDefaults};
    use objc2_user_notifications::{
        UNAuthorizationOptions, UNAuthorizationStatus, UNNotificationSettings,
        UNUserNotificationCenter,
    };

    use crate::{PermissionKind, PermissionStatus};

    /// How long a read that answers through a completion handler may take.
    const READ_TIMEOUT: Duration = Duration::from_secs(5);
    /// How long a prompt may wait for the user before the row is re-read
    /// anyway. The prompt itself stays up; only the wait ends.
    const PROMPT_TIMEOUT: Duration = Duration::from_secs(120);

    #[link(name = "CoreGraphics", kind = "framework")]
    unsafe extern "C" {
        fn CGPreflightScreenCaptureAccess() -> bool;
        fn CGRequestScreenCaptureAccess() -> bool;
    }

    // `Boolean` in HIServices is an `unsigned char`, not a C `bool`.
    #[link(name = "ApplicationServices", kind = "framework")]
    unsafe extern "C" {
        fn AXIsProcessTrusted() -> u8;
        fn AXIsProcessTrustedWithOptions(options: *const c_void) -> u8;
    }

    #[link(name = "CoreServices", kind = "framework")]
    unsafe extern "C" {
        fn AECreateDesc(
            type_code: u32,
            data: *const c_void,
            size: isize,
            result: *mut AEDesc,
        ) -> i16;
        fn AEDisposeDesc(desc: *mut AEDesc) -> i16;
        fn AEDeterminePermissionToAutomateTarget(
            target: *const AEDesc,
            event_class: u32,
            event_id: u32,
            ask_user_if_needed: u8,
        ) -> i32;
    }

    /// Carbon declares `AEDesc` under `#pragma pack(2)`: a four-byte type
    /// code then a pointer, twelve bytes. This is sixteen, so it holds that
    /// layout or the naturally aligned one, and Rust never reads a field —
    /// only AppleEvents does, through the pointer it wrote through.
    #[repr(C, align(8))]
    struct AEDesc([u8; 16]);

    const fn four_char_code(code: &[u8; 4]) -> u32 {
        u32::from_be_bytes(*code)
    }

    /// `typeApplicationBundleID`.
    const TYPE_APPLICATION_BUNDLE_ID: u32 = four_char_code(b"bund");
    /// `typeWildCard`: any event class, any event id.
    const TYPE_WILD_CARD: u32 = four_char_code(b"****");
    /// `errAEEventNotPermitted`.
    const EVENT_NOT_PERMITTED: i32 = -1743;
    /// `errAEEventWouldRequireUserConsent`.
    const WOULD_REQUIRE_CONSENT: i32 = -1744;

    /// Automation consent is per target app. System Events is the one agent
    /// scripts reach for most, so it stands in for the rest.
    const SYSTEM_EVENTS: &str = "com.apple.systemevents";

    pub(super) fn is_bundled() -> bool {
        NSBundle::mainBundle().bundleIdentifier().is_some()
    }

    pub(super) fn status(kind: PermissionKind) -> PermissionStatus {
        match kind {
            PermissionKind::Notifications => notifications_status(),
            PermissionKind::ScreenRecording => {
                // SAFETY: takes no arguments and only reads TCC state.
                if unsafe { CGPreflightScreenCaptureAccess() } {
                    PermissionStatus::Granted
                } else {
                    answered_or_not(kind)
                }
            }
            PermissionKind::Accessibility => {
                // SAFETY: takes no arguments and only reads TCC state.
                if unsafe { AXIsProcessTrusted() } != 0 {
                    PermissionStatus::Granted
                } else {
                    answered_or_not(kind)
                }
            }
            PermissionKind::FullDiskAccess => full_disk_access_status(),
            PermissionKind::Automation => automation_status(),
            // No public API reads it.
            PermissionKind::LocalNetwork => PermissionStatus::CheckManually,
        }
    }

    pub(super) fn request(kind: PermissionKind) {
        match kind {
            PermissionKind::Notifications => request_notifications(),
            PermissionKind::ScreenRecording => {
                mark_requested(kind);
                // SAFETY: takes no arguments; shows the system alert once in
                // the app's lifetime and returns without waiting for it.
                unsafe { CGRequestScreenCaptureAccess() };
            }
            PermissionKind::Accessibility => {
                mark_requested(kind);
                request_accessibility();
            }
            PermissionKind::Automation => trigger_automation(),
            PermissionKind::LocalNetwork => trigger_local_network(),
            PermissionKind::FullDiskAccess => {}
        }
    }

    /// Screen Recording and Accessibility read the same "not granted"
    /// whether the prompt was refused or never shown, and a Request button
    /// for a refused one does nothing. Whether Sirio already asked tells the
    /// two apart — the Swift app kept the same flag, under the same keys.
    fn answered_or_not(kind: PermissionKind) -> PermissionStatus {
        if NSUserDefaults::standardUserDefaults().boolForKey(&requested_key(kind)) {
            PermissionStatus::Denied
        } else {
            PermissionStatus::NotRequested
        }
    }

    fn mark_requested(kind: PermissionKind) {
        NSUserDefaults::standardUserDefaults().setBool_forKey(true, &requested_key(kind));
    }

    fn requested_key(kind: PermissionKind) -> Retained<NSString> {
        let name = match kind {
            PermissionKind::ScreenRecording => "screenRecording",
            PermissionKind::Accessibility => "accessibility",
            _ => kind.slug(),
        };
        NSString::from_str(&format!("permissions.requested.{name}"))
    }

    fn notifications_status() -> PermissionStatus {
        if !is_bundled() {
            return PermissionStatus::CheckManually;
        }
        let center = UNUserNotificationCenter::currentNotificationCenter();
        let (sender, receiver) = mpsc::sync_channel(1);
        let handler = RcBlock::new(move |settings: NonNull<UNNotificationSettings>| {
            // SAFETY: the center passes a valid settings object that lives
            // for the duration of the call.
            let status = unsafe { settings.as_ref() }.authorizationStatus();
            let _ = sender.try_send(status);
        });
        center.getNotificationSettingsWithCompletionHandler(&handler);
        let Ok(status) = receiver.recv_timeout(READ_TIMEOUT) else {
            return PermissionStatus::CheckManually;
        };
        if status == UNAuthorizationStatus::NotDetermined {
            PermissionStatus::NotRequested
        } else if status == UNAuthorizationStatus::Denied {
            PermissionStatus::Denied
        } else if status == UNAuthorizationStatus::Authorized
            || status == UNAuthorizationStatus::Provisional
            || status == UNAuthorizationStatus::Ephemeral
        {
            PermissionStatus::Granted
        } else {
            PermissionStatus::CheckManually
        }
    }

    fn request_notifications() {
        if !is_bundled() {
            return;
        }
        let center = UNUserNotificationCenter::currentNotificationCenter();
        let (sender, receiver) = mpsc::sync_channel(1);
        let handler = RcBlock::new(move |_granted: Bool, _error: *mut NSError| {
            let _ = sender.try_send(());
        });
        center.requestAuthorizationWithOptions_completionHandler(
            UNAuthorizationOptions::Alert | UNAuthorizationOptions::Sound,
            &handler,
        );
        let _ = receiver.recv_timeout(PROMPT_TIMEOUT);
    }

    fn request_accessibility() {
        // `kAXTrustedCheckOptionPrompt`'s value, spelled out: the constant
        // is a CFString global, and the literal is what it holds.
        let key = NSString::from_str("AXTrustedCheckOptionPrompt");
        let prompt = NSNumber::numberWithBool(true);
        let options = NSDictionary::from_slices(&[&*key], &[&*prompt]);
        // SAFETY: NSDictionary is toll-free bridged with CFDictionary, and
        // `options` outlives the call.
        unsafe { AXIsProcessTrustedWithOptions(Retained::as_ptr(&options).cast()) };
    }

    /// Nothing reads this but a file only Full Disk Access can open: the TCC
    /// database itself. Unreadable proves nothing — it may simply not be
    /// there — so it is "check manually", never "denied".
    fn full_disk_access_status() -> PermissionStatus {
        let Some(home) = std::env::var_os("HOME") else {
            return PermissionStatus::CheckManually;
        };
        let database = Path::new(&home).join("Library/Application Support/com.apple.TCC/TCC.db");
        if std::fs::File::open(database).is_ok() {
            PermissionStatus::Granted
        } else {
            PermissionStatus::CheckManually
        }
    }

    fn automation_status() -> PermissionStatus {
        let mut target = AEDesc([0; 16]);
        // SAFETY: the bundle id is a live byte slice of the length passed,
        // and `target` is writable storage large enough for an AEDesc.
        let created = unsafe {
            AECreateDesc(
                TYPE_APPLICATION_BUNDLE_ID,
                SYSTEM_EVENTS.as_ptr().cast(),
                SYSTEM_EVENTS.len() as isize,
                &mut target,
            )
        };
        if created != 0 {
            return PermissionStatus::CheckManually;
        }
        // SAFETY: `target` was initialised by AECreateDesc above; asking
        // with `ask_user_if_needed = 0` never shows a prompt.
        let answer = unsafe {
            AEDeterminePermissionToAutomateTarget(&target, TYPE_WILD_CARD, TYPE_WILD_CARD, 0)
        };
        // SAFETY: disposes the descriptor AECreateDesc created, once.
        unsafe { AEDisposeDesc(&mut target) };
        match answer {
            0 => PermissionStatus::Granted,
            EVENT_NOT_PERMITTED => PermissionStatus::Denied,
            WOULD_REQUIRE_CONSENT => PermissionStatus::NotRequested,
            // procNotFound: System Events is not running, so it cannot say.
            _ => PermissionStatus::CheckManually,
        }
    }

    /// A harmless Apple Event to System Events, sent the way an agent in a
    /// pane sends one: from `osascript`, a child process TCC attributes to
    /// Sirio. It launches System Events if needed and waits on the consent
    /// prompt, so this returns once the user has answered.
    fn trigger_automation() {
        let _ = Command::new("/usr/bin/osascript")
            .args([
                "-e",
                "tell application \"System Events\" to count processes",
            ])
            .output();
    }

    /// Local Network has no request API; its prompt appears on first use. One
    /// mDNS query for the service list is the smallest real use there is.
    fn trigger_local_network() {
        let Ok(socket) = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)) else {
            return;
        };
        let _ = socket.send_to(
            &mdns_services_query(),
            (Ipv4Addr::new(224, 0, 0, 251), 5353),
        );
    }

    /// A DNS query, id 0, for the PTR records of
    /// `_services._dns-sd._udp.local` — the DNS-SD service enumeration name.
    fn mdns_services_query() -> Vec<u8> {
        // Header: id, flags, one question, no answer/authority/additional.
        let mut packet = vec![0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0];
        for label in ["_services", "_dns-sd", "_udp", "local"] {
            packet.push(label.len() as u8);
            packet.extend_from_slice(label.as_bytes());
        }
        packet.push(0);
        // QTYPE PTR (12), QCLASS IN (1).
        packet.extend_from_slice(&[0, 12, 0, 1]);
        packet
    }
}
