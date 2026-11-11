//! Provenance: the render code of freemapper-print, which copied it from freemapper 743ce4b; moved here
//! unchanged (ADR 0019).
//!
//! The training map: OOM symbols drawn the way OOM draws them, written as PDF or SVG.
//!
//! The public part is [`build`] (the page builder: from a map to a page, ADR 0023), [`page`] (what
//! every backend draws, with the page mapping), [`pdf`] (the PDF backend), [`svg`] (the SVG backend
//! for the browser editors), [`hit`] (which map objects lie under a point of the page) and [`fonts`]
//! (text metrics for layouts). [`symbols`], [`draw`], [`bounds`] and [`geometry`] are public because
//! freemapper-print's sheet and overprint code still draws with them; dashes, line and pattern
//! drawing stay inside the crate.

/// The program and where to find it, as named in the PDF metadata of [`pdf::PdfMeta::new`].
pub const CREDIT: &str = "freemapper-print – https://github.com/HenryJobst/freemapper-print";

pub mod bounds;
pub mod build;
mod dash;
pub mod draw;
pub mod fonts;
pub mod geometry;
pub mod hit;
pub mod image;
mod line;
pub mod overprint;
pub mod page;
mod pattern;
pub mod pdf;
pub mod svg;
pub mod symbols;
