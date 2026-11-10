//! Coordinates and coordinate reference systems of the freemapper suite: geographic and projected points,
//! bounding boxes, the 1 km and 2 km tile grids, ETRS89 / UTM zones 32N and 33N ([`Crs`]) and the
//! Transverse Mercator projection they share with UTM zones read from a map's CRS text ([`tm`]).
//!
//! The crate needs no dependencies. Declination from the World Magnetic Model stays in freemapper, which
//! owns the date handling and the OOM coordinate types it needs.

pub mod crs;
pub mod tiles;
pub mod tm;

pub use crs::{
    BoundingBox, Crs, EPSG_UTM33, LatLon, ProjectedPoint, grid_convergence_deg, point_scale_factor,
    utm33_forward, utm33_inverse,
};
pub use tiles::{TileId, tiles_covering, tiles_covering_grid};
