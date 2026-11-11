//! The contract between what freemapper writes (a map and its `freemapper.toml`) and the programs that
//! read it, freemapper-print and freemapper-courseplan (ADR 0018).
//!
//! The crate sorts every object of a map into an [`ObjectClass`] and derives three things from that: a
//! [`Census`] (how many objects of each class, and where), a [`Vocabulary`] (the words of the file, without
//! its content) and the [`Violation`]s of the rules freemapper's output must keep ([`check`]).
//!
//! With the feature `fixtures` the crate also carries maps and sidecars exactly as freemapper wrote them
//! (`fixtures()`), each with the values a reader must derive from it ([`Expected`]).
//!
//! Maps and sidecars go in as text, plain data comes out. No type of `freemapper-omap` is part of the API,
//! so each app reads the same text with the reader it ships.
//!
//! The shared types are always there and need no map reader: [`MapKind`] with the scales freemapper draws
//! it at, [`Sidecar`] (the contract keys of `freemapper.toml`), [`ObjectClass::of`] and the reserved words
//! in [`tags`]. Census, vocabulary and rules sit behind the feature `check`, which is on by default; an
//! app that only needs the types turns the default features off.

#[cfg(feature = "check")]
mod census;
#[cfg(feature = "check")]
mod check;
mod class;
#[cfg(feature = "check")]
mod expected;
#[cfg(feature = "fixtures")]
mod fixtures;
mod kind;
mod sidecar;
pub mod tags;
#[cfg(all(test, feature = "check"))]
mod testing;
#[cfg(feature = "check")]
mod vocabulary;

#[cfg(feature = "check")]
pub use census::{Bbox, Census, ClassBoxes, ClassCounts, census};
#[cfg(feature = "check")]
pub use check::{Rule, Violation, check};
pub use class::{ObjectClass, tag};
#[cfg(feature = "check")]
pub use expected::Expected;
#[cfg(feature = "fixtures")]
pub use fixtures::{Fixture, fixture, fixtures};
pub use kind::{MapKind, UnknownKind};
pub use sidecar::{Sidecar, SidecarError, SidecarKind, sidecar_path};
#[cfg(feature = "check")]
pub use vocabulary::Vocabulary;

/// Version of the contract the rules and fixtures of this crate describe.
pub const CONTRACT_VERSION: u32 = 1;

#[derive(Debug, thiserror::Error)]
pub enum ContractError {
    #[error("the map cannot be read: {0}")]
    Map(String),
    #[error("freemapper.toml cannot be read: {0}")]
    Sidecar(String),
    #[error("expected.toml cannot be read: {0}")]
    Expected(String),
    #[error("vocabulary.toml cannot be read: {0}")]
    Vocabulary(String),
}

/// Reads a map with the crate's own reader.
#[cfg(feature = "check")]
pub(crate) fn read_map(omap_xml: &str) -> Result<freemapper_omap::MapDocument, ContractError> {
    freemapper_omap::read::read_map(omap_xml).map_err(|e| ContractError::Map(e.to_string()))
}
