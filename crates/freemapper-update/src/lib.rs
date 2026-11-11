//! The pure parts of a self-update for the freemapper suite's apps: versions, platform names, the signed
//! update manifest, the host rule, the check record and the binary swap.
//!
//! The crate has no network code (core ADR 0007). Apps bring the network through the `UpdateSource` trait.

pub mod error;
pub mod manifest;
pub mod platform;
pub mod signature;
pub mod source;
pub mod version;

pub use error::UpdateError;
pub use manifest::{Asset, Manifest, parse_manifest};
pub use platform::current_platform;
pub use signature::verify;
pub use source::{ALLOWED_HOSTS, check_url};
pub use version::{ParseVersionError, Version};
