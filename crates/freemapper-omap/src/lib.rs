//! Copied from freemapper 743ce4b (freemapper-print ADR 0014); moved here from freemapper-print
//! 918cec2 (ADR 0012).
//! OpenOrienteering Mapper file format (XML, version 9).

pub mod coord;
pub mod enlarge;
pub mod model;
pub mod read;
pub mod symbol_set;

pub use coord::MapCoord;
pub use model::{
    MapDocument, MapGeoref, MapObject, MapObjectKind, MapPart, PatternOrigin, TemplateKind,
    TemplateRef, closed_ring,
};
pub use symbol_set::{SymbolCode, SymbolSet};

/// Deepest XML element nesting the readers accept. Real OOM files nest about a dozen levels; a
/// deeper file is broken or hostile, and the limit keeps parsing and dropping the element tree
/// from overflowing the stack.
pub const MAX_XML_DEPTH: usize = 256;

/// The error for a file nested deeper than [`MAX_XML_DEPTH`]. Public for other readers of OOM
/// XML, such as the renderer's symbol reader, so all of them report the same text.
pub fn too_deep() -> OmapError {
    OmapError::Xml(format!(
        "nesting too deep: more than {MAX_XML_DEPTH} levels of elements"
    ))
}

#[derive(Debug, thiserror::Error)]
pub enum OmapError {
    #[error("invalid OOM XML: {0}")]
    Xml(String),
    #[error("element <{0}> not found")]
    MissingElement(&'static str),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Unsupported(String),
}
