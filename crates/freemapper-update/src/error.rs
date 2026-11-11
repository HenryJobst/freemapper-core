//! The errors of an update.

use std::path::PathBuf;

use crate::version::Version;

/// Everything that can go wrong while checking for, downloading or installing an update. More cases
/// may follow, so a match needs a wildcard arm.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum UpdateError {
    /// The manifest is malformed, too big, of another app or of an unknown format.
    #[error("update manifest: {0}")]
    Manifest(String),
    /// The signature does not verify. Never treated as "no update".
    #[error("the update signature is not valid")]
    Signature,
    /// The signature names a key the program does not accept.
    #[error("the update is signed with an unknown key")]
    UnknownKey,
    /// A URL outside the host rule.
    #[error("refused URL: {0}")]
    Url(String),
    /// The transfer failed.
    #[error("download failed: {0}")]
    Download(String),
    /// A size or hash differs from the manifest.
    #[error("{what} does not match the manifest")]
    Hash { what: &'static str },
    /// The program folder cannot be written.
    #[error("cannot write to {}", .0.display())]
    NotWritable(PathBuf),
    /// The installed executable reports another version than the manifest.
    #[error("the new executable reports {got:?}, expected {expected}")]
    VersionMismatch { expected: Version, got: String },
    #[error(transparent)]
    Io(#[from] std::io::Error),
    /// The release has no asset for this platform.
    #[error("no release asset for platform {0}")]
    NoAssetForPlatform(String),
    /// Another install or rollback holds the lock file in the program folder.
    #[error("another update is running (lock file {})", .0.display())]
    Busy(PathBuf),
    /// `install` was handed `Outcome::Current`.
    #[error("there is no update to install")]
    NothingToInstall,
    /// Release `version` cannot be installed by this one's self-update: the running version is older
    /// than `needed` (the manifest's `min_self_update_from`). `notes` links the release page that
    /// describes the manual step (spec 3.5 step 2).
    #[error(
        "{version} cannot be installed from this version; install {needed} or newer by hand, see {notes}"
    )]
    TooOldToSelfUpdate {
        needed: Version,
        version: Version,
        notes: String,
    },
}
