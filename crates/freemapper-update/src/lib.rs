//! The pure parts of a self-update for the freemapper suite's apps: versions, platform names, the signed
//! update manifest, the host rule, the check record and the binary swap.
//!
//! The crate has no network code (core ADR 0007). Apps bring the network through the `UpdateSource` trait.

mod atomic;
pub mod check;
pub mod error;
pub mod manifest;
pub mod platform;
pub mod record;
pub mod settings;
pub mod signature;
pub mod source;
pub mod swap;
pub mod version;

pub use check::{Outcome, check_for_update};
pub use error::UpdateError;
pub use manifest::{Asset, Manifest, parse_manifest};
pub use platform::current_platform;
pub use record::{CheckRecord, RecordResult, due, load_record, save_record};
pub use settings::{Mode, load_mode, save_mode};
pub use signature::verify;
pub use source::{ALLOWED_HOSTS, App, UpdateSource, check_url, manifest_url};
pub use swap::{
    ProcessRunner, VersionRunner, exe_path, previous_path, remove_stale, rollback, swap_in,
};
pub use version::{ParseVersionError, Version};
