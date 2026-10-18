//! The shell pieces the code editor uses, from upstream `src/shell/`.
//! Only `StatusBarItem` is vendored; see LOCAL-CHANGES.md.
mod bars;

pub use bars::StatusBarItem;
