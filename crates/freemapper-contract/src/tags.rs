//! The reserved words of the contract: tag keys and values, part names, the sidecar's file name and keys.
//! freemapper writes them from here and the readers compare against them, so a word exists once.

/// Tag that names where an object comes from. The values [`SOURCE_SHEET`], [`SOURCE_BASE`] and
/// [`SOURCE_NORTH_LINES`] are reserved; every other value is the identifier of a data source.
pub const SOURCE_KEY: &str = "freemapper:source";
/// Tag that names the role of a sheet object, e.g. [`ROLE_SCALE_BAR`]. A reader meets roles it does not
/// know and treats their objects as sheet objects all the same.
pub const ROLE_KEY: &str = "freemapper:sheet";
/// Every tag key freemapper writes starts with this.
pub const TAG_PREFIX: &str = "freemapper:";

/// Source of a sheet object (title block, scale bar, credit line).
pub const SOURCE_SHEET: &str = "sheet";
/// Source of the base area.
pub const SOURCE_BASE: &str = "base";
/// Source of a magnetic north line.
pub const SOURCE_NORTH_LINES: &str = "north-lines";
/// What freemapper v0.9.0 wrote on the scale bar instead of [`SOURCE_SHEET`]. Readers accept it; freemapper
/// no longer writes it.
pub const LEGACY_SOURCE_SCALE_BAR: &str = "scale-bar";

/// Role of the objects that make up the scale bar.
pub const ROLE_SCALE_BAR: &str = "scale-bar";

/// The part with the sheet objects.
pub const SHEET_PART: &str = "peripheral information";
/// The part with the base area.
pub const BASE_PART: &str = "base";

/// Name of the sidecar file, in the folder of the map.
pub const SIDECAR_FILE: &str = "freemapper.toml";
/// Sidecar key of the map kind: the name of a [`crate::MapKind`].
pub const KEY_MAP_KIND: &str = "map_kind";
/// Sidecar key of the map title.
pub const KEY_TITLE: &str = "title";
/// Sidecar key of the map scale (denominator).
pub const KEY_SCALE: &str = "scale_denominator";
/// Sidecar key of the map's coordinate reference system, an EPSG code.
pub const KEY_CRS: &str = "crs";
/// The contract keys of the sidecar. Every other key is freemapper's own record.
pub const CONTRACT_KEYS: [&str; 4] = [KEY_MAP_KIND, KEY_TITLE, KEY_SCALE, KEY_CRS];
