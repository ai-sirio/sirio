//! Claude Code's IDE protocol, served by Sirio: the endpoint (token, bind) and the lockfile
//! Claude discovers it by. A leaf: no GPUI, and it never touches user-global Claude config.

pub mod endpoint;
pub mod lockfile;

pub use endpoint::{PORT_RANGE, bind, new_token};
pub use lockfile::{LockBody, Lockfile};
