//! The course model, independent of the input format. Readers return a [`RawEvent`] (points in file
//! coordinates); fitting to the map (feature `fit`) turns it into a `FittedEvent` with
//! [`CourseEvent::map_points`].

#[cfg(feature = "fit")]
use freemapper_omap::coord::MapCoord;

/// Largest course file a reader accepts (ADR 0019).
pub const MAX_COURSE_FILE_BYTES: usize = 50 * 1024 * 1024;
/// Most controls in one file.
pub const MAX_CONTROLS: usize = 10_000;
/// Most courses in one file.
pub const MAX_COURSES: usize = 1_000;
/// Most stops in one course.
pub const MAX_STOPS_PER_COURSE: usize = 500;
/// Most points in one path or ring of a special object or leg.
pub const MAX_POINTS_PER_PATH: usize = 10_000;
/// Most points one read returns in total: leg bends, leg and circle gap ranges, and special object points,
/// counted over all courses (a leg or control shared by many courses counts once per course).
pub const MAX_TOTAL_POINTS: usize = 2_000_000;
/// Longest control code, in characters.
pub const MAX_CODE_LEN: usize = 64;
/// Longest course or course family name, in characters.
pub const MAX_NAME_LEN: usize = 200;
/// Longest description standard, in characters (real values are "2004", "2018", "2024").
pub const MAX_STANDARD_LEN: usize = 16;
/// Most bytes of codes, names and description standards one read returns in total, counted per copy (a code
/// shared by many stops counts once per stop).
pub const MAX_TOTAL_TEXT_BYTES: usize = 16 * 1024 * 1024;
/// Most special objects in one file.
pub const MAX_SPECIALS: usize = 10_000;
/// Most notes kept; further ones are dropped and one `NotesTruncated` is added.
pub const MAX_NOTES: usize = 1_000;
/// Deepest XML nesting a course reader accepts.
pub const MAX_XML_DEPTH: usize = 64;

/// A point in millimetres in file coordinates, before fitting to the map. The axis direction is whatever
/// the writer used.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct RawPoint {
    pub x_mm: f64,
    pub y_mm: f64,
}

/// A text position relative to the centre of its control circle, in print-independent file mm: x to the
/// right, y in the file's axis direction (the fit flips it like y of a point, and does not translate it).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Offset {
    pub dx_mm: f64,
    pub dy_mm: f64,
}

/// An event as a reader returns it.
pub type RawEvent = CourseEvent<RawPoint>;
/// An event with its points in map coordinates.
#[cfg(feature = "fit")]
pub type FittedEvent = CourseEvent<MapCoord>;

/// Everything read from a course file, generic over the point type `P`.
#[derive(Debug, Clone, PartialEq)]
pub struct CourseEvent<P> {
    pub controls: Vec<Control<P>>,
    pub courses: Vec<Course<P>>,
    pub specials: Vec<Special<P>>,
    pub source: CourseSource,
    pub notes: Vec<CourseNote>,
}

impl<P> CourseEvent<P> {
    /// Converts every point with `f` and every number offset with `fo`, keeping the rest unchanged.
    pub fn map_points<Q>(
        self,
        f: impl Fn(P) -> Q,
        fo: impl Fn(Offset) -> Offset,
    ) -> CourseEvent<Q> {
        CourseEvent {
            controls: self
                .controls
                .into_iter()
                .map(|c| Control {
                    id: c.id,
                    code: c.code,
                    kind: c.kind,
                    position: c.position.map(&f),
                    geo: c.geo,
                    description: c.description,
                })
                .collect(),
            courses: self
                .courses
                .into_iter()
                .map(|c| Course {
                    id: c.id,
                    name: c.name,
                    family: c.family,
                    kind: c.kind,
                    legs: c
                        .legs
                        .into_iter()
                        .map(|l| Leg {
                            bends: l.bends.into_iter().map(&f).collect(),
                            gaps: l.gaps,
                            marked_route: l.marked_route,
                        })
                        .collect(),
                    stops: c
                        .stops
                        .into_iter()
                        .map(|s| Stop {
                            control: s.control,
                            kind: s.kind,
                            number: s.number,
                            number_text_at: s.number_text_at.map(&fo),
                            circle_gaps: s.circle_gaps,
                            map_flip: s.map_flip,
                        })
                        .collect(),
                    climb_m: c.climb_m,
                    length_m: c.length_m,
                    print_scale_hint: c.print_scale_hint,
                    course_obj_ratio: c.course_obj_ratio,
                    secondary_title: c.secondary_title,
                })
                .collect(),
            specials: self
                .specials
                .into_iter()
                .map(|s| s.map_points(&f))
                .collect(),
            source: self.source,
            notes: self.notes,
        }
    }
}

/// Where the data came from.
#[derive(Debug, Clone, PartialEq)]
pub struct CourseSource {
    pub format: SourceFormat,
    pub event_name: Option<String>,
    /// The map scale denominator the file states, if any.
    pub map_scale: Option<u32>,
    /// How the file scales course symbols (Purple Pen `course-appearance/@scale-sizes`). `None` for IOF XML,
    /// which has no such setting.
    pub item_scaling: Option<ItemScaling>,
}

/// How Purple Pen scales the size of course symbols (`course-appearance/@scale-sizes`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ItemScaling {
    /// Symbols keep their nominal size on the printed page.
    #[default]
    None,
    /// Symbols scale with the map: their size relative to the map stays the same at every print scale.
    RelativeToMap,
    /// Symbols are sized for a print scale of 1:15000.
    RelativeTo15000,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceFormat {
    IofXml,
    PurplePen,
}

impl SourceFormat {
    /// The y axis the format always writes: up for Purple Pen; `None` for IOF XML, whose writers differ
    /// (ADR 0032).
    #[cfg(feature = "fit")]
    pub fn y_axis(self) -> Option<super::fit::YAxis> {
        match self {
            SourceFormat::PurplePen => Some(super::fit::YAxis::Up),
            SourceFormat::IofXml => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlKind {
    Start,
    Control,
    Finish,
    CrossingPoint,
    /// End of a marked route (IOF XML `EndOfMarkedRoute`).
    EndOfMarkedRoute,
    /// A map exchange; no IOF XML value produces it, the Purple Pen reader does.
    MapExchange,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Control<P> {
    /// Purple Pen's id of the control (`control/@id` in the `.ppen`), so that a writer can refer to the same
    /// control again. `None` for IOF XML, which has no such id, and for an id outside `u32`.
    pub id: Option<u32>,
    pub code: String,
    pub kind: ControlKind,
    /// `None` for a control the file places only by `geo`; the fit computes it.
    pub position: Option<P>,
    /// Latitude and longitude in degrees, if the file gives them (IOF XML `Position`).
    pub geo: Option<(f64, f64)>,
    pub description: Option<Description>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CourseKind {
    Normal,
    Score,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Course<P> {
    /// Purple Pen's id of the course (`course/@id` in the `.ppen`). `None` for IOF XML and for an id outside
    /// `u32`.
    pub id: Option<u32>,
    pub name: String,
    /// The course family: courses with the same family are the variants of one fork.
    pub family: Option<String>,
    pub kind: CourseKind,
    /// One leg between each two consecutive stops.
    pub legs: Vec<Leg<P>>,
    pub stops: Vec<Stop>,
    pub climb_m: Option<f64>,
    pub length_m: Option<f64>,
    pub print_scale_hint: Option<u32>,
    /// Purple Pen's `CourseView.CourseObjRatio` for this course: how many map mm one mm of a course symbol's
    /// nominal size takes in the file, by `course-appearance/@scale-sizes` (none: print scale over map scale;
    /// relative to the map: 1; relative to 1:15000: 15000 over map scale). A number offset divided by it is in
    /// mm at the nominal size. `None` when the file does not scale course symbols (IOF XML): 1.
    pub course_obj_ratio: Option<f64>,
    /// A second title line for the description box (Purple Pen `secondary-title`); `|` separates lines.
    pub secondary_title: Option<String>,
}

/// One visit of a control in a course.
#[derive(Debug, Clone, PartialEq)]
pub struct Stop {
    /// The control code.
    pub control: String,
    /// The kind of the control in this course, if the course overrides the control's own kind (IOF XML
    /// `CourseControl/@type`, a Purple Pen map exchange). A control can be the finish of one course and a
    /// plain control of another.
    pub kind: Option<ControlKind>,
    pub number: Option<NumberPlacement>,
    /// Where the file puts the number text, relative to the circle centre (IOF XML `MapTextPosition`).
    /// Converted to `number` once circle and box sizes are known.
    pub number_text_at: Option<Offset>,
    /// Angle ranges `(start, stop)` in degrees where the circle is not drawn, measured from +x towards +y
    /// of the frame the event is in (Purple Pen file: y up, so counter-clockwise). Like Purple Pen, `stop`
    /// lies in 0..360 and `start` below it, so a range across +x starts negative. `map_points` does not
    /// touch them; `fit` turns `(a, b)` into `(-b, -a)` when it flips y, so a fitted event is entirely in
    /// the map frame.
    pub circle_gaps: Vec<(f64, f64)>,
    /// The map exchange at this stop is a map flip (Purple Pen `map-flip`): the description shows 15.6.
    pub map_flip: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Leg<P> {
    pub bends: Vec<P>,
    /// Distance ranges `(from, to)` in file mm where the line is not drawn, measured along the leg (through
    /// its bends) from the centre of the leg's first control.
    pub gaps: Vec<(f64, f64)>,
    pub marked_route: bool,
}

/// Where a control number sits relative to its circle. Agreed with freemapper-courseplan.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NumberPlacement {
    /// Degrees; 0 is east, counter-clockwise.
    pub direction: f64,
    /// Print mm: the shortest distance between the outer edge of the circle line and the number box, with
    /// the box centred on the direction ray.
    pub gap_mm: f64,
}

/// Which courses a special object belongs to.
#[derive(Debug, Clone, PartialEq)]
pub enum CourseFilter {
    All,
    Only(Vec<String>),
}

/// A special object of the course layout. `id` is Purple Pen's id, see [`Special::id`].
#[derive(Debug, Clone, PartialEq)]
pub enum Special<P> {
    OutOfBounds {
        id: Option<u32>,
        rings: Vec<Vec<P>>,
        courses: CourseFilter,
    },
    Dangerous {
        id: Option<u32>,
        rings: Vec<Vec<P>>,
        courses: CourseFilter,
    },
    /// A forbidden route. Purple Pen marks one with a single cross, which is a path of one point.
    ForbiddenRoute {
        id: Option<u32>,
        path: Vec<P>,
        courses: CourseFilter,
    },
    Boundary {
        id: Option<u32>,
        path: Vec<P>,
        courses: CourseFilter,
    },
    MarkedRoute {
        id: Option<u32>,
        path: Vec<P>,
        courses: CourseFilter,
    },
    FirstAid {
        id: Option<u32>,
        pt: P,
        courses: CourseFilter,
    },
    Water {
        id: Option<u32>,
        pt: P,
        courses: CourseFilter,
    },
    /// Text in a box. `pt` is the box's top-left corner as the file gives it (Purple Pen: the corner with
    /// the smallest x and the largest y, y up); `size_mm` is the font height, or the box height when the file
    /// leaves the font height to the box (an approximation: Purple Pen fits auto-sized text, possibly several
    /// lines, into the box).
    Text {
        id: Option<u32>,
        pt: P,
        text: String,
        size_mm: f64,
        courses: CourseFilter,
    },
    Line {
        id: Option<u32>,
        path: Vec<P>,
        courses: CourseFilter,
    },
    WhiteOut {
        id: Option<u32>,
        rings: Vec<Vec<P>>,
        courses: CourseFilter,
    },
}

impl<P> Special<P> {
    /// Purple Pen's id of the special object (`special-object/@id` in the `.ppen`), so that a writer can
    /// refer to the same object again. `None` for an id outside `u32`.
    pub fn id(&self) -> Option<u32> {
        match self {
            Special::OutOfBounds { id, .. }
            | Special::Dangerous { id, .. }
            | Special::ForbiddenRoute { id, .. }
            | Special::Boundary { id, .. }
            | Special::MarkedRoute { id, .. }
            | Special::FirstAid { id, .. }
            | Special::Water { id, .. }
            | Special::Text { id, .. }
            | Special::Line { id, .. }
            | Special::WhiteOut { id, .. } => *id,
        }
    }

    /// The courses the object belongs to.
    pub fn courses(&self) -> &CourseFilter {
        match self {
            Special::OutOfBounds { courses, .. }
            | Special::Dangerous { courses, .. }
            | Special::ForbiddenRoute { courses, .. }
            | Special::Boundary { courses, .. }
            | Special::MarkedRoute { courses, .. }
            | Special::FirstAid { courses, .. }
            | Special::Water { courses, .. }
            | Special::Text { courses, .. }
            | Special::Line { courses, .. }
            | Special::WhiteOut { courses, .. } => courses,
        }
    }

    pub fn map_points<Q>(self, f: impl Fn(P) -> Q) -> Special<Q> {
        let rings = |r: Vec<Vec<P>>| -> Vec<Vec<Q>> {
            r.into_iter()
                .map(|v| v.into_iter().map(&f).collect())
                .collect()
        };
        let path = |v: Vec<P>| -> Vec<Q> { v.into_iter().map(&f).collect() };
        match self {
            Special::OutOfBounds {
                id,
                rings: r,
                courses,
            } => Special::OutOfBounds {
                id,
                rings: rings(r),
                courses,
            },
            Special::Dangerous {
                id,
                rings: r,
                courses,
            } => Special::Dangerous {
                id,
                rings: rings(r),
                courses,
            },
            Special::ForbiddenRoute {
                id,
                path: p,
                courses,
            } => Special::ForbiddenRoute {
                id,
                path: path(p),
                courses,
            },
            Special::Boundary {
                id,
                path: p,
                courses,
            } => Special::Boundary {
                id,
                path: path(p),
                courses,
            },
            Special::MarkedRoute {
                id,
                path: p,
                courses,
            } => Special::MarkedRoute {
                id,
                path: path(p),
                courses,
            },
            Special::FirstAid { id, pt, courses } => Special::FirstAid {
                id,
                pt: f(pt),
                courses,
            },
            Special::Water { id, pt, courses } => Special::Water {
                id,
                pt: f(pt),
                courses,
            },
            Special::Text {
                id,
                pt,
                text,
                size_mm,
                courses,
            } => Special::Text {
                id,
                pt: f(pt),
                text,
                size_mm,
                courses,
            },
            Special::Line {
                id,
                path: p,
                courses,
            } => Special::Line {
                id,
                path: path(p),
                courses,
            },
            Special::WhiteOut {
                id,
                rings: r,
                courses,
            } => Special::WhiteOut {
                id,
                rings: rings(r),
                courses,
            },
        }
    }
}

/// Control descriptions of one control.
#[derive(Debug, Clone, PartialEq)]
pub struct Description {
    /// Index 0 is column C and index 5 is column H. Symbol ids as the file stores them.
    pub columns: [Option<String>; 6],
    /// The description standard, e.g. "2018".
    pub standard: String,
    /// Text in column F, such as the dimensions `5x5|3`, when the file gives text instead of or besides a
    /// symbol.
    pub column_f_text: Option<String>,
}

/// Things read but not supported.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum CourseNote {
    UnknownElement(String),
    /// Courses of one family are the variants of a fork; each prints as its own sheet.
    Forked {
        family: String,
        variants: Vec<String>,
    },
    /// The named course has a `CourseControl` with alternative controls and is left out.
    AlternativeControls(String),
    /// More notes than `MAX_NOTES`; the rest were dropped.
    NotesTruncated,
    WhiteOutLaterInM4,
    NoDescriptions,
    /// The map issue point of this control code is used as the start.
    MapIssueAsStart(String),
    /// The named course has forks or loops (Purple Pen variations) and is left out. Purple Pen's IOF XML
    /// export writes each variation as its own course.
    Variations(String),
}
