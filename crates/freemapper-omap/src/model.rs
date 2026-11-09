//! In-memory model of an OOM map document.

// Copied from freemapper 743ce4b (freemapper-print ADR 0014); moved here from freemapper-print 918cec2
// (ADR 0012).

use super::coord::{FLAG_CLOSE, FLAG_HOLE, MapCoord};
use super::symbol_set::SymbolSet;

#[derive(Debug, Clone, PartialEq)]
pub enum MapObjectKind {
    Point {
        rotation_rad: f64,
    },
    Path,
    /// Text at a single anchor or in a box; alignments as in OOM (`h_align` 0 left, 1 centre,
    /// 2 right; `v_align` 0 baseline, 1 top, 2 centre, 3 bottom).
    Text {
        text: String,
        h_align: u8,
        v_align: u8,
        /// Rotation in radians as stored by OOM (counter-clockwise on the map).
        rotation_rad: f64,
        /// Width and height (0.001 mm) of a box text: the anchor is the middle of the box, the
        /// lines wrap at its width and align to its edges. `None` for a single-anchor text.
        box_size: Option<(f64, f64)>,
    },
}

/// Origin and rotation of an object's area pattern (`<pattern>` in OOM).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PatternOrigin {
    pub rotation_rad: f64,
    pub origin: MapCoord,
}

/// A single map object with symbol, geometry and tags.
#[derive(Debug, Clone, PartialEq)]
pub struct MapObject {
    pub symbol_id: u32,
    pub kind: MapObjectKind,
    pub coords: Vec<MapCoord>,
    pub tags: Vec<(String, String)>,
    /// Pattern rotation and origin of area objects, when the file stores them.
    pub pattern: Option<PatternOrigin>,
}

/// A group of map objects (OOM part).
#[derive(Debug, Clone, PartialEq)]
pub struct MapPart {
    pub name: String,
    pub objects: Vec<MapObject>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TemplateKind {
    /// Raster image located by a world file (`TemplateImage`).
    Image,
    Gdal,
    Ogr,
    Map,
}

/// Reference to a template file, relative to the map file.
#[derive(Debug, Clone, PartialEq)]
pub struct TemplateRef {
    pub kind: TemplateKind,
    pub relpath: String,
    pub georeferenced: bool,
    pub crs_spec: Option<String>,
    pub visible: bool,
    pub opacity: f32,
    /// Drawn below the map objects (rasters) rather than above them.
    pub behind_map: bool,
}

/// Georeferencing of an OOM map as read from the file. freemapper-print's own type: it is only
/// read, never written back as in freemapper, and printing uses the scale of it.
#[derive(Debug, Clone, PartialEq)]
pub struct MapGeoref {
    pub scale_denominator: u32,
    pub declination_deg: f64,
    pub grivation_deg: f64,
    pub ref_projected: Option<(f64, f64)>,
    pub crs_spec: Option<String>,
    /// Grid distance over ground distance (`grid_scale_factor` in the file, OOM's combined scale
    /// factor); 1 when the file has none.
    pub combined_scale_factor: f64,
    /// The map point (mm, y down) that `ref_projected` belongs to: `<ref_point>` directly under
    /// `<georeferencing>`, (0, 0) when the file has none.
    pub map_ref_mm: (f64, f64),
}

impl Default for MapGeoref {
    fn default() -> Self {
        Self {
            scale_denominator: 0,
            declination_deg: 0.0,
            grivation_deg: 0.0,
            ref_projected: None,
            crs_spec: None,
            combined_scale_factor: 1.0,
            map_ref_mm: (0.0, 0.0),
        }
    }
}

/// A complete OOM map document.
#[derive(Debug, Clone, PartialEq)]
pub struct MapDocument {
    pub notes: String,
    pub georef: MapGeoref,
    pub symbol_set: SymbolSet,
    pub parts: Vec<MapPart>,
    pub templates: Vec<TemplateRef>,
    /// Objects of a type the reader does not know (not point, path or text), left out.
    pub unknown_objects: usize,
}

/// Builds one closed ring: drops a duplicate closing input point, then repeats the first point
/// with the hole and close flags (18), as OOM expects for every ring of an area.
pub fn closed_ring(points: &[(i32, i32)]) -> Vec<MapCoord> {
    let mut ring: Vec<MapCoord> = points.iter().map(|&(x, y)| MapCoord::new(x, y)).collect();
    if ring.len() > 1 && ring.first() == ring.last() {
        ring.pop();
    }
    if let Some(&first) = ring.first() {
        ring.push(MapCoord {
            flags: FLAG_HOLE | FLAG_CLOSE,
            ..first
        });
    }
    ring
}
