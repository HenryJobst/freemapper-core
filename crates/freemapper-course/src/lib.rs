//! Orienteering courses: the course model, readers for Purple Pen (`.ppen`) and IOF XML 3.0, a `.ppen` writer
//! that also patches an existing file, the fit of a course file to an OOM map, and the placement of control
//! numbers. Terms follow `docs/glossary.md`.
//!
//! Two modules serve the apps beyond reading and writing:
//!
//! - [`placement`] holds the number search of freemapper-print. `placement::place_course_numbers` places all
//!   numbers of one course in running order, with kept numbers fixed and each placed number an obstacle for
//!   the next (ADR 0016).
//! - [`description_symbols`] holds the IOF control description symbols (pictograms and German and English
//!   texts) from Purple Pen's `symbols.xml`, and the mapping between Purple Pen ids and IOF 2024 numbers in
//!   both directions (ADR 0017).
//!
//! The crate uses no other suite crate than `freemapper-omap`, and that only for the fit: the cargo feature
//! `fit` (on by default) brings in the fit, `FittedEvent` and `SourceFormat::y_axis` with the omap crate.
//! Without it, the model, the readers and the number placement build alone. Loading the course file a project
//! names stays in the app. Error texts are English; the app translates them.

// Moved here from freemapper-print 918cec2 (`src/course/`, `src/placement/`; freemapper-print ADR 0039,
// core ADR 0013). ADR numbers, rulings and spec sections that the comments cite without naming a project are
// freemapper-print's; "courseplan" means freemapper-courseplan.

pub mod description_symbols;
#[cfg(feature = "fit")]
pub mod fit;
pub mod iof_xml;
pub mod model;
pub mod number_offset;
pub mod placement;
pub mod ppen;
pub mod xml;

#[cfg(feature = "fit")]
pub use fit::{Fit, FitError, FitNote, SkipReason, fit};
pub use iof_xml::read_iof_xml;
#[cfg(feature = "fit")]
pub use model::FittedEvent;
pub use model::{
    Control, ControlKind, Course, CourseEvent, CourseFilter, CourseKind, CourseNote, CourseSource,
    Description, ItemScaling, Leg, NumberPlacement, RawEvent, RawPoint, SourceFormat, Special,
    Stop,
};
pub use number_offset::{offset_from_placement, placement_from_offset};
pub use ppen::write::{
    AssignedIds, Deletions, DescriptionStandard, KeptObject, LineEnding, MapStandard, PatchOptions,
    PpenMap, PpenWriteError, ScratchOptions, WriteNote, Written, write_ppen, write_ppen_patch,
};
pub use ppen::{course_obj_ratio, read_ppen};

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
