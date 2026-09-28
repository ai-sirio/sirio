//! Desktop notifications through gpui's `show_system_notification` — on
//! macOS, `UNUserNotificationCenter`, so a notification is Sirio's own
//! rather than Script Editor's (see `sirio_privacy::notifications`).
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

static SENDER: OnceLock<UnboundedSender<NotificationPayload>> = OnceLock::new();

/// Starts the task that posts what [`post`] hands it. Once per process;
/// a second call is a no-op.
pub(crate) fn install(cx: &mut App) {
    let (sender, mut receiver) = mpsc::unbounded::<NotificationPayload>();
    if SENDER.set(sender).is_err() {
        return;
    }
    cx.spawn(async move |cx| {
        while let Some(payload) = receiver.next().await {
            let notification = SystemNotification {
                tag: tag_for(&payload).into(),
                title: payload.title.into(),
                body: payload.body.into(),
                actions: Vec::new(),
            };
            cx.update(|cx| cx.show_system_notification(notification));
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
        .is_some_and(|sender| sender.unbounded_send(payload.clone()).is_ok())
}

/// A pane's newer notification replaces its older one; one that belongs to
/// no pane (`sirioctl notify`) stands on its own.
fn tag_for(payload: &NotificationPayload) -> String {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    if payload.pane_id.is_empty() {
        format!("sirio-{}", NEXT.fetch_add(1, Ordering::Relaxed))
    } else {
        format!("sirio-pane-{}", payload.pane_id)
    }
}
