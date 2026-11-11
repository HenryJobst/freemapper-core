//! Print rules as data: the map kinds, the symbol size table by print scale, the north-line spacing and the
//! course symbol sizes, as data profiles with their sources (ADR 0025).
//!
//! The base profile `iof-print` is embedded. [`rules::base`] resolves it; [`scale`] holds the map kinds and
//! the factors a print scale gives (symbol factor, geometry factor). The format and the checks of profiles
//! live in `freemapper-rules`; this crate holds what is specific to printing orienteering maps.

pub mod rules;
pub mod scale;
