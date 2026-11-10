//! The reserved words of the contract and the class of an object.

/// Tag that names where an object comes from. Three values are reserved; every other value is the
/// identifier of a data source.
pub(crate) const SOURCE_KEY: &str = "freemapper:source";
/// Tag that names the role of a sheet furniture object, e.g. `scale-bar`.
pub(crate) const ROLE_KEY: &str = "freemapper:sheet";
/// Every tag key freemapper writes starts with this.
pub(crate) const TAG_PREFIX: &str = "freemapper:";

pub(crate) const SOURCE_SHEET: &str = "sheet";
pub(crate) const SOURCE_BASE: &str = "base";
pub(crate) const SOURCE_NORTH_LINES: &str = "north-lines";
/// What freemapper v0.9.0 wrote on the scale bar instead of `sheet`.
pub(crate) const LEGACY_SOURCE_SCALE_BAR: &str = "scale-bar";

/// The part with the sheet furniture.
pub(crate) const SHEET_PART: &str = "peripheral information";
/// The part with the base area.
pub(crate) const BASE_PART: &str = "base";

/// What an object is to a reader of the map.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObjectClass {
    /// Sheet furniture (title block, scale bar, credit line): not map content, not part of the map extent.
    Furniture,
    /// The base area: printed, but no feature.
    Base,
    /// A magnetic north line: map content.
    NorthLines,
    /// Everything else, with or without tags.
    Content,
}

/// The value of the tag `key`, if the object carries it.
pub(crate) fn tag<'a>(tags: &'a [(String, String)], key: &str) -> Option<&'a str> {
    tags.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
}

/// The class of an object, from its source tag alone; the part it lies in plays no role.
pub(crate) fn classify(tags: &[(String, String)]) -> ObjectClass {
    match tag(tags, SOURCE_KEY) {
        Some(SOURCE_SHEET | LEGACY_SOURCE_SCALE_BAR) => ObjectClass::Furniture,
        Some(SOURCE_BASE) => ObjectClass::Base,
        Some(SOURCE_NORTH_LINES) => ObjectClass::NorthLines,
        _ => ObjectClass::Content,
    }
}
