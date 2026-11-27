// SPDX-License-Identifier: AGPL-3.0-or-later

//! The reserved words of the contract: tag keys and values, part names, the sidecar's file name and keys.
//! freemapper writes them from here and the readers compare against them, so a word exists once.

/// Tag that names where an object comes from. The values [`SOURCE_SHEET`], [`SOURCE_BASE`],
/// [`SOURCE_NORTH_LINES`] and [`SOURCE_PAPER_FRAME`] are reserved; every other value is the identifier of
/// a data source.
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

/// Source of a paper frame: a closed line, the whole sheet of a paper at the map's scale, in the
/// template map [`PAPER_FRAME_FILE`] only, never in the map itself.
pub const SOURCE_PAPER_FRAME: &str = "paper-frame";
/// Tag of a paper frame that names its paper, one of [`PAPERS`].
pub const PAPER_KEY: &str = "freemapper:paper";
/// Tag of a paper frame that names its orientation, one of [`ORIENTATIONS`].
pub const ORIENTATION_KEY: &str = "freemapper:orientation";
/// The papers a paper frame can have, smallest first.
pub const PAPERS: [&str; 5] = ["A6", "A5", "A4", "A3", "A2"];
/// Orientation of a paper frame that is wider than high on the map.
pub const ORIENTATION_LANDSCAPE: &str = "landscape";
/// Orientation of a paper frame that is higher than wide on the map.
pub const ORIENTATION_PORTRAIT: &str = "portrait";
/// The orientations a paper frame can have.
pub const ORIENTATIONS: [&str; 2] = [ORIENTATION_LANDSCAPE, ORIENTATION_PORTRAIT];
/// The template map of the paper frames, relative to the folder of the map. It is there only when the
/// project has frames.
pub const PAPER_FRAME_FILE: &str = "templates/paper-frame.omap";

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
/// Sidecar table of the contours freemapper traced; absent when they have the map kind's interval.
pub const KEY_CONTOURS: &str = "contours";
/// Key in [`KEY_CONTOURS`]: the contour interval in metres (a forest map in flat land: 2.5).
pub const KEY_CONTOUR_INTERVAL: &str = "interval_m";
/// The contract keys of the sidecar. Every other key is freemapper's own record.
pub const CONTRACT_KEYS: [&str; 5] = [KEY_MAP_KIND, KEY_TITLE, KEY_SCALE, KEY_CRS, KEY_CONTOURS];
