//! Identifier types for the entities Sirio manages.
//!
//! Newtypes over `u64` in the same spirit as GPUI's id types: the compiler
//! keeps `ProjectId`, `WorktreeId` and `TabId` from being mixed up at call
//! sites, at zero runtime cost. Ids are minted by [`crate::Workspace`], which
//! owns a monotonic counter, so `load_project`/`add_tab` never collide.

use std::fmt;

/// Defines a newtype id with `Display`, `From<u64>` and the usual value-type
/// derives, following GPUI's `define_id_type!` convention.
macro_rules! define_id_type {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(pub u64);

        impl $name {
            /// Returns the id with the given raw value.
            pub const fn new(value: u64) -> Self {
                Self(value)
            }
        }

        impl From<u64> for $name {
            fn from(value: u64) -> Self {
                Self(value)
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}#{}", stringify!($name), self.0)
            }
        }
    };
}

define_id_type! {
    /// Identifies a [`crate::Project`] within a [`crate::Workspace`].
    ProjectId
}

define_id_type! {
    /// Identifies a [`crate::Worktree`] within a [`crate::Workspace`].
    WorktreeId
}

define_id_type! {
    /// Identifies a [`crate::Tab`] within a [`crate::Workspace`].
    TabId
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_distinct_types_with_value_semantics() {
        let a = ProjectId::new(7);
        let b = ProjectId::new(7);
        let c = ProjectId::new(8);

        assert_eq!(a, b);
        assert_ne!(a, c);
        assert_eq!(a.to_string(), "ProjectId#7");
        assert_eq!(c.0, 8);
    }

    #[test]
    fn ids_support_set_and_map_usage() {
        let mut seen = std::collections::HashSet::new();
        seen.insert(WorktreeId::new(1));
        seen.insert(WorktreeId::new(1));
        seen.insert(WorktreeId::new(2));
        assert_eq!(seen.len(), 2);
        assert!(seen.contains(&WorktreeId::new(2)));
    }
}
