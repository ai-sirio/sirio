//! GitHub pull requests and GitLab merge requests — "change requests" —
//! read over one GraphQL layer and two transports: the forge's own CLI
//! (`gh`, `glab`) or HTTP with a personal token.
//!
//! A leaf crate: no GPUI and no Sirio crate but `sirio_perf`. Every call
//! blocks; the app runs them on GPUI's background executor. A token, when
//! there is one, is handed in by the caller and never stored or logged here.

mod target;

pub use target::{ForgeTarget, parse_remote_url};
