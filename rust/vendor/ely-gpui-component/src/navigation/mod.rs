//! The navigation components Sirio uses, from upstream `src/navigation/`.
//! Only `breadcrumb` and `tabs` are vendored; see LOCAL-CHANGES.md.
mod breadcrumb;
mod tabs;

pub use breadcrumb::{Breadcrumb, Crumb};
pub use tabs::{TabPlacement, Tabs};
