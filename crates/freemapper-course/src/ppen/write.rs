//! Writer for Purple Pen's native event files (`.ppen`), the inverse of [`read_ppen`](super::read_ppen).
//!
//! The output follows Purple Pen's own writer (`EventDB.Save` and the `WriteAttributesAndContent` methods in
//! `src/PurplePenCore/EventDB.cs` at the commit the reader names): .NET's `XmlTextWriter` with
//! `Formatting.Indented`. That means one element per line, two spaces of indent per level, attributes as
//! `name="value"` separated by one space, an empty element as `<x a="1" />`, an element with only text on one
//! line, and no XML declaration. Purple Pen 3.x on Windows writes a UTF-8 byte order mark and CRLF line ends.
//! Every number is written as an `f32` in its shortest round-trip form without an exponent, since Purple Pen
//! stores `float`.
//!
//! A patch of an existing file keeps every byte the writer does not change. The base parse (`base.rs`) splits
//! the file into its top-level objects and the bytes around them, takes the output style from the file, and
//! refuses a file from a Purple Pen version the writer does not know. Purple Pen writes no version number, so
//! the gate checks the shape of the file: root `course-scribe-event` without attributes, at most one
//! `event`, top-level children only `event`, `control`, `course`, `course-control`, `leg` and
//! `special-object`, UTF-8. Unknown content below the top level passes, because Purple Pen skips it on load
//! and the patch carries it.

// `write_ppen` and the patch (the next steps of the writer) are the callers of both modules.
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "used by the writer still to come")
)]
mod base;
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "used by the writer still to come")
)]
mod out;

use crate::CourseError;

/// Why an event could not be written as a `.ppen`.
#[derive(Debug, thiserror::Error)]
pub enum PpenWriteError {
    /// The base file of a patch is unreadable or over a reader limit.
    #[error(transparent)]
    Base(#[from] CourseError),
    #[error("the .ppen comes from a Purple Pen version this writer does not know: {0}")]
    UnknownVersion(String),
    #[error("cannot be written to a .ppen: {0}")]
    Unsupported(String),
    #[error("invalid event: {0}")]
    Invalid(String),
    #[error(
        "courses {first} and {second} draw the leg {from} to {to} differently; Purple Pen keeps one leg per pair of controls"
    )]
    SharedLeg {
        from: String,
        to: String,
        first: String,
        second: String,
    },
    #[error(
        "courses {first} and {second} give control {control} different circle gaps at 1:{scale}"
    )]
    SharedCircleGaps {
        control: String,
        scale: i64,
        first: String,
        second: String,
    },
    #[error("too large for a .ppen: {0}")]
    Limit(String),
}

/// The line end of a written file. Purple Pen on Windows writes CRLF.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LineEnding {
    #[default]
    Crlf,
    Lf,
}

impl LineEnding {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            LineEnding::Crlf => "\r\n",
            LineEnding::Lf => "\n",
        }
    }
}
