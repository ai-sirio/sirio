//! Desktop notifications through gpui's `show_system_notification` — on
//! macOS, `UNUserNotificationCenter`, so a notification is Sirio's own
//! rather than Script Editor's (see `sirio_privacy::notifications`); on Linux,
//! the XDG notifications D-Bus interface.
//!
//! gpui's notification center lives on the main thread, and notifications
//! are posted from two others as well: an activity transition posts from the
//! UI thread, `sirioctl notify` from the control socket's. Both hand the
//! payload to this channel, and one foreground task posts it.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};

use futures::StreamExt as _;
use futures::channel::mpsc::{self, UnboundedSender};
use gpui::{App, SystemNotification};
use sirio_activity::NotificationPayload;

/// What the posting task is asked to do.
enum Post {
    Show(NotificationPayload),
    /// Withdraw the notification of the session with this persistence id.
    Dismiss(String),
}

static SENDER: OnceLock<UnboundedSender<Post>> = OnceLock::new();

/// A session's notification tag is this prefix and its persistence id, which
/// is how a click on it names the session to open.
const TAB_TAG_PREFIX: &str = "sirio-tab-";

/// Starts the task that posts what [`post`] hands it. Once per process;
/// a second call is a no-op.
pub(crate) fn install(cx: &mut App) {
    let (sender, mut receiver) = mpsc::unbounded::<Post>();
    if SENDER.set(sender).is_err() {
        return;
    }
    cx.spawn(async move |cx| {
        while let Some(post) = receiver.next().await {
            match post {
                Post::Show(payload) => {
                    let notification = SystemNotification {
                        tag: tag_for(&payload).into(),
                        title: payload.title.into(),
                        body: payload.body.into(),
                        actions: Vec::new(),
                    };
                    cx.update(|cx| cx.show_system_notification(notification));
                }
                Post::Dismiss(session_id) => {
                    cx.update(|cx| cx.dismiss_system_notification(&tab_tag(&session_id)));
                }
            }
        }
    })
    .detach();
}

/// Hands `payload` to the posting task. `false` when none was installed —
/// a test harness, or a call before startup reached [`install`] — so the
/// caller can take another road instead of losing it.
pub(crate) fn post(payload: &NotificationPayload) -> bool {
    SENDER
        .get()
        .is_some_and(|sender| sender.unbounded_send(Post::Show(payload.clone())).is_ok())
}

/// Withdraws the notification of a session that has been seen, through the
/// same channel as [`post`]. Does nothing when no posting task was installed.
// Only macOS can withdraw a notification: the XDG interface closes one only
// through a live handle, which gpui's Linux route does not keep.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) fn dismiss(session_id: &str) {
    if let Some(sender) = SENDER.get() {
        let _ = sender.unbounded_send(Post::Dismiss(session_id.to_string()));
    }
}

/// The tag of the notification about the session `session_id`.
pub(crate) fn tab_tag(session_id: &str) -> String {
    format!("{TAB_TAG_PREFIX}{session_id}")
}

/// The persistence id a notification tag names, or `None` for a tag that is
/// not a session's (a `sirioctl notify` one, another app's).
pub(crate) fn session_id_of_tag(tag: &str) -> Option<&str> {
    tag.strip_prefix(TAB_TAG_PREFIX).filter(|id| !id.is_empty())
}

/// A session's newer notification replaces its older one; one that belongs to
/// no session (`sirioctl notify`) stands on its own.
fn tag_for(payload: &NotificationPayload) -> String {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    if payload.session_id.is_empty() {
        format!("sirio-{}", NEXT.fetch_add(1, Ordering::Relaxed))
    } else {
        tab_tag(&payload.session_id)
    }
}
