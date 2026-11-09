//! The conversion between a number offset, where a course file puts the centre of a control number, and a
//! number placement, the direction and gap the placement module works with (freemapper-print ruling R12,
//! ADR 0038). freemapper-print turns a file's offset into a placement; a `.ppen` writer goes the other way.
//!
//! Units and frames, the same in both functions:
//! - The reference point is the centre of the control circle.
//! - An [`Offset`] is in map mm in the map frame of a fitted event (`FittedEvent`): x east, y south (down on
//!   the paper). A `.ppen` stores `number-location` with y up, so its `dy_mm` changes sign on the way in and
//!   out; the fit (feature `fit`) does that for a whole event, a `.ppen` writer must do it itself.
//! - A [`NumberPlacement`] has its direction in degrees, 0 east and counter-clockwise as seen on paper, and
//!   its gap in mm at the nominal size of the course symbols: the shortest distance from the outer edge of the
//!   circle line to the number box, which is centred on the ray in that direction.
//! - `circle_outer_r` (circle radius plus half the line width) and `box_size` (width, height of the number
//!   box) are in mm at the nominal size too.
//! - `ratio` is the map mm one nominal mm of a course symbol takes, Purple Pen's `CourseObjRatio`
//!   ([`crate::Course::course_obj_ratio`]; 1 when the file does not scale course symbols). A ratio that is
//!   not finite or not above 0 counts as 1.

use crate::model::{NumberPlacement, Offset};
use crate::placement::{Placement, number_box, placement_of};

/// `ratio` if it is finite and above 0, else 1.
fn usable(ratio: f64) -> f64 {
    if ratio > 0.0 && ratio.is_finite() {
        ratio
    } else {
        1.0
    }
}

/// The placement of a number whose box centre a course file puts at `offset` from the circle centre. The
/// offset divided by `ratio` is the box centre at the nominal size; the direction points from the circle
/// centre to it, and the gap is the box's shortest distance to the outer edge of the circle line (negative
/// when the box reaches into the circle). See the module for units and frames.
pub fn placement_from_offset(
    offset: Offset,
    circle_outer_r: f64,
    box_size: (f64, f64),
    ratio: f64,
) -> NumberPlacement {
    let ratio = usable(ratio);
    let centre = (offset.dx_mm / ratio, offset.dy_mm / ratio);
    placement_of((0.0, 0.0), circle_outer_r, centre, box_size).into()
}

/// The offset of the box centre of a number at `placement`, the inverse of [`placement_from_offset`]: the
/// box of [`number_box`] around the circle centre, its centre times `ratio`. The round trip is exact for
/// every box that does not cover the circle centre; a box over the centre comes back in the same direction,
/// moved out until it just touches the centre. See the module for units and frames.
pub fn offset_from_placement(
    placement: NumberPlacement,
    circle_outer_r: f64,
    box_size: (f64, f64),
    ratio: f64,
) -> Offset {
    let ratio = usable(ratio);
    let (x, y) = number_box((0.0, 0.0), circle_outer_r, box_size, placement.into()).centre();
    Offset {
        dx_mm: x * ratio,
        dy_mm: y * ratio,
    }
}

impl From<NumberPlacement> for Placement {
    fn from(p: NumberPlacement) -> Self {
        Placement {
            direction: p.direction,
            gap_mm: p.gap_mm,
        }
    }
}

impl From<Placement> for NumberPlacement {
    fn from(p: Placement) -> Self {
        NumberPlacement {
            direction: p.direction,
            gap_mm: p.gap_mm,
        }
    }
}
