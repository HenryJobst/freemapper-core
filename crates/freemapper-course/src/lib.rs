//! Orienteering courses: the course model, readers for Purple Pen (`.ppen`) and IOF XML 3.0, the fit of a
//! course file to an OOM map, and the placement of control numbers. Terms follow `docs/glossary.md`.
//!
//! The crate uses no other suite crate than `freemapper-omap`; loading the course file a project names stays
//! in the app. Error texts are English; the app translates them.

// Moved here from freemapper-print 918cec2 (`src/course/`, `src/placement/`; freemapper-print ADR 0039,
// core ADR 0013). ADR numbers, rulings and spec sections that the comments cite without naming a project are
// freemapper-print's; "courseplan" means freemapper-courseplan.

pub mod fit;
pub mod iof_xml;
pub mod model;
pub mod number_offset;
pub mod placement;
pub mod ppen;
mod tm;
pub mod xml;

pub use fit::{Fit, FitError, FitNote, SkipReason, fit};
pub use iof_xml::read_iof_xml;
pub use model::{
    Control, ControlKind, Course, CourseEvent, CourseFilter, CourseKind, CourseNote, CourseSource,
    Description, FittedEvent, Leg, NumberPlacement, RawEvent, RawPoint, SourceFormat, Special,
    Stop,
};
pub use number_offset::{offset_from_placement, placement_from_offset};
pub use ppen::read_ppen;

/// Why a course file could not be read.
#[derive(Debug, thiserror::Error)]
pub enum CourseError {
    #[error("invalid course file: {0}")]
    Xml(String),
    #[error("course file too large or too complex: {0}")]
    Limit(String),
    #[error("{0}")]
    Unsupported(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}
