//! The errors of an update.

use std::path::PathBuf;

use crate::version::Version;

/// Everything that can go wrong while checking for, downloading or installing an update.
#[derive(Debug, thiserror::Error)]
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
    /// The installed version predates the self-update path.
    #[error("this version cannot update itself; install {needed} or newer by hand")]
    TooOldToSelfUpdate { needed: Version },
}
