//! The git components Sirio uses, from upstream `src/git/`. Only the badges
//! are vendored, with `GitStatus` brought out of upstream's `src/lists/`;
//! see LOCAL-CHANGES.md.
mod badges;
mod status;

pub use badges::{DiffStat, GitStatusBadge};
pub use status::GitStatus;
