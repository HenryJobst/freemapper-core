//! Provenance: the render code of freemapper-print, which copied it from freemapper 743ce4b; moved here
//! unchanged (ADR 0019).
//!
//! The training map: OOM symbols drawn the way OOM draws them, written as PDF.
//!
//! The public part is [`page`] (what every backend draws), [`pdf`] (the PDF backend) and
//! [`fonts`] (text metrics for layouts). [`symbols`], [`draw`], [`bounds`] and [`geometry`] are public
//! because freemapper-print's page builder still sits outside this crate; dashes, line and pattern
//! drawing stay inside it.

/// The program and where to find it, as named in the PDF metadata of [`pdf::PdfMeta::new`].
pub const CREDIT: &str = "freemapper-print – https://github.com/HenryJobst/freemapper-print";

pub mod bounds;
mod dash;
pub mod draw;
pub mod fonts;
pub mod geometry;
pub mod image;
mod line;
pub mod page;
mod pattern;
pub mod pdf;
pub mod symbols;
