//! Reads every permission the Permissions page shows and prints what macOS
//! said, one line each — the page's model with no window around it.
//!
//! ```text
//! cargo run -p sirio_privacy --example permissions
//! ```
//!
//! It never asks: a read shows no prompt. Run from a terminal it reports the
//! terminal's own TCC envelope, not Sirio's, and — outside an app bundle —
//! Notifications as "check manually", because the notification center aborts
//! an unbundled process on first touch and the probe never touches it there.
//! That last line is what `.github/workflows/macos-check.yml` runs it for:
//! every framework symbol the probe names resolves at link time, and every
//! read returns rather than aborting.

use sirio_privacy::{PermissionKind, action_for, probe};

fn main() {
    println!("bundled: {}", probe::is_bundled());
    for kind in PermissionKind::ALL {
        let status = probe::status(kind);
        println!(
            "{:<18} {:<16} -> {}",
            kind.title(),
            status.label(),
            action_for(kind, status).label()
        );
    }
}
