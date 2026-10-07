//! GitHub pull requests and GitLab merge requests — "change requests" —
//! read over one GraphQL layer and two transports: the forge's own CLI
//! (`gh`, `glab`) or HTTP with a personal token.
//!
//! A leaf crate: no GPUI and no Sirio crate but `sirio_perf`. Every call
//! blocks; the app runs them on GPUI's background executor. A token, when
//! there is one, is handed in by the caller and never stored or logged here.

mod action;
mod client;
mod error;
mod github;
mod gitlab;
mod graphql;
mod log;
mod mapping;
mod model;
mod resolve;
mod scopes;
mod target;
mod transport;

#[doc(hidden)]
pub use action::{LiveProbe, live_probes};
pub use action::{Action, ActionOutcome, RerunTarget, ReviewTarget, ReviewVerdict};
pub use client::ForgeClient;
pub use error::ForgeError;

pub use model::{
    AnchorLine, Capabilities, ChangeHeader, ChangePage, ChangeRef, ChangeState, ChangeSummary, Check, CheckJob,
    CheckStatus, CiState, CommentKind, CommentRef, CommitSummary, EventKind, FileChange,
    FileChangeKind, Filter, Forge, LineAnchor, LineComment, LineKind, ListQuery, Listing, PageCursor, Progress,
    ReviewOutcome, ReviewState, ReviewThread, Reviewer, Revisions, Side, ThreadComment, TimelineItem, BlockReason, Candidate, Label,
    MergeCapability, MergeMethod, MergeMethods, MergeVerdict, Draft, HeadRepository,
};
pub use scopes::TokenScopes;
pub use resolve::{HostSetting, Means, Probes, Resolution, SystemProbes, known_forge, resolve};
pub use target::{ForgeTarget, parse_remote_url};
pub use transport::{
    ApiResponse, CliProgram, CliTransport, RestMethod, RestRequest, TokenTransport, Transport,
};

pub use model::Log;

/// How much of a log's end Sirio keeps (spec §15.2).
pub const LOG_TAIL_BYTES: usize = 4 * 1024 * 1024;
/// The most a log download may be; past it the log is refused with a pointer
/// to the browser rather than held whole in memory.
pub const LOG_DOWNLOAD_LIMIT: u64 = 64 * 1024 * 1024;
