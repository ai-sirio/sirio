//! The ACP agent registry: what agents exist, which are installed, and how
//! to install one.
//!
//! This crate is a leaf by design — see `Cargo.toml`.

mod atomic;
mod client;
mod installer;
mod model;
mod resolve;
mod store;

pub use client::RegistryClient;
pub use installer::{InstallError, Installer, UnpackKind, unpack_kind};
pub use model::{AcpRegistry, BinaryArtifact, Distribution, RegistryAgent};
pub use resolve::{
    BuiltinAcp, InstalledAgent, Integrity, LaunchSource, ResolveInput, UnavailableReason,
    current_platform_key, registry_id, resolve,
};
pub use store::InstallStore;
