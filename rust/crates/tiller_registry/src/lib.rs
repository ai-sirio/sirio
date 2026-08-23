//! The ACP agent registry: what agents exist, which are installed, and how
//! to install one.
//!
//! This crate is a leaf by design — see `Cargo.toml`.

mod model;

pub use model::{AcpRegistry, BinaryArtifact, Distribution, RegistryAgent};
