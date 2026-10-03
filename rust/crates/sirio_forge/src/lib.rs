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
mod mapping;
mod model;
mod resolve;
mod scopes;
mod target;
mod transport;

#[doc(hidden)]
pub use action::{LiveProbe, live_probes};
pub use action::{Action, ActionOutcome, ReviewVerdict};
pub use client::ForgeClient;
pub use error::ForgeError;

pub use model::{
    Capabilities, ChangeHeader, ChangePage, ChangeRef, ChangeState, ChangeSummary, Check,
    CheckStatus, CiState, CommentKind, CommentRef, CommitSummary, EventKind, FileChange,
    FileChangeKind, Filter, Forge, LineComment, ListQuery, Listing, PageCursor, Progress,
    ReviewOutcome, ReviewState, Reviewer, Revisions, TimelineItem, BlockReason, Candidate, Label,
    MergeCapability, MergeMethod, MergeMethods, MergeVerdict,
};
pub use scopes::TokenScopes;
pub use resolve::{HostSetting, Means, Probes, Resolution, SystemProbes, known_forge, resolve};
pub use target::{ForgeTarget, parse_remote_url};
pub use transport::{
    ApiResponse, CliProgram, CliTransport, RestMethod, RestRequest, TokenTransport, Transport,
};
