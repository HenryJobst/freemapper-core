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

mod census;
mod check;
mod class;
mod expected;
#[cfg(feature = "fixtures")]
mod fixtures;
mod sidecar;
#[cfg(test)]
mod testing;
mod vocabulary;

pub use census::{Bbox, Census, ClassBoxes, ClassCounts, census};
pub use check::{Rule, Violation, check};
pub use class::ObjectClass;
pub use expected::Expected;
#[cfg(feature = "fixtures")]
pub use fixtures::{Fixture, fixture, fixtures};
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
pub(crate) fn read_map(omap_xml: &str) -> Result<freemapper_omap::MapDocument, ContractError> {
    freemapper_omap::read::read_map(omap_xml).map_err(|e| ContractError::Map(e.to_string()))
}
