// SPDX-License-Identifier: AGPL-3.0-or-later

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
    /// The event date as the file writes it (`YYYY-MM-DD`), if any. IOF XML `Event/StartTime/Date`.
    pub event_date: Option<String>,
    /// The organiser, if the file names one. IOF XML `Event/Organiser/Name`.
    pub organiser: Option<String>,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn p(x: f64, y: f64) -> RawPoint {
        RawPoint { x_mm: x, y_mm: y }
    }

    /// One of each special object, with ids 1 to 10 in this order, on the course `A`.
    fn every_special() -> Vec<Special<RawPoint>> {
        let only_a = || CourseFilter::Only(vec!["A".to_string()]);
        let ring = || {
            vec![
                vec![p(0.0, 0.0), p(1.0, 0.0), p(1.0, 1.0)],
                vec![p(5.0, 5.0)],
            ]
        };
        let path = || vec![p(2.0, 3.0), p(4.0, 5.0)];
        vec![
            Special::OutOfBounds {
                id: Some(1),
                rings: ring(),
                courses: only_a(),
            },
            Special::Dangerous {
                id: Some(2),
                rings: ring(),
                courses: only_a(),
            },
            Special::ForbiddenRoute {
                id: Some(3),
                path: path(),
                courses: only_a(),
            },
            Special::Boundary {
                id: Some(4),
                path: path(),
                courses: only_a(),
            },
            Special::MarkedRoute {
                id: Some(5),
                path: path(),
                courses: only_a(),
            },
            Special::FirstAid {
                id: Some(6),
                pt: p(7.0, 8.0),
                courses: only_a(),
            },
            Special::Water {
                id: Some(7),
                pt: p(7.0, 8.0),
                courses: only_a(),
            },
            Special::Text {
                id: Some(8),
                pt: p(7.0, 8.0),
                text: "Water".into(),
                size_mm: 3.0,
                courses: only_a(),
            },
            Special::Line {
                id: Some(9),
                path: path(),
                courses: only_a(),
            },
            Special::WhiteOut {
                id: Some(10),
                rings: ring(),
                courses: only_a(),
            },
        ]
    }

    #[test]
    fn every_special_names_its_id_and_its_courses() {
        for (i, s) in every_special().iter().enumerate() {
            assert_eq!(s.id(), Some(i as u32 + 1), "{s:?}");
            assert_eq!(
                s.courses(),
                &CourseFilter::Only(vec!["A".to_string()]),
                "{s:?}"
            );
        }
        let none = Special::FirstAid {
            id: None,
            pt: p(0.0, 0.0),
            courses: CourseFilter::All,
        };
        assert_eq!(none.id(), None);
        assert_eq!(none.courses(), &CourseFilter::All);
    }

    #[test]
    fn mapping_the_points_of_a_special_keeps_everything_else() {
        let shift = |q: RawPoint| (q.x_mm + 100.0, -q.y_mm);
        for s in every_special() {
            let (id, courses) = (s.id(), s.courses().clone());
            let mapped = s.clone().map_points(shift);
            assert_eq!(mapped.id(), id);
            assert_eq!(mapped.courses(), &courses);
            // Same kind of special object, with every point moved and none lost.
            match (&s, &mapped) {
                (
                    Special::OutOfBounds { rings: a, .. }
                    | Special::Dangerous { rings: a, .. }
                    | Special::WhiteOut { rings: a, .. },
                    Special::OutOfBounds { rings: b, .. }
                    | Special::Dangerous { rings: b, .. }
                    | Special::WhiteOut { rings: b, .. },
                ) => {
                    assert_eq!(a.len(), b.len());
                    for (ra, rb) in a.iter().zip(b) {
                        let want: Vec<_> = ra.iter().map(|q| shift(*q)).collect();
                        assert_eq!(&want, rb);
                    }
                }
                (
                    Special::ForbiddenRoute { path: a, .. }
                    | Special::Boundary { path: a, .. }
                    | Special::MarkedRoute { path: a, .. }
                    | Special::Line { path: a, .. },
                    Special::ForbiddenRoute { path: b, .. }
                    | Special::Boundary { path: b, .. }
                    | Special::MarkedRoute { path: b, .. }
                    | Special::Line { path: b, .. },
                ) => {
                    let want: Vec<_> = a.iter().map(|q| shift(*q)).collect();
                    assert_eq!(&want, b);
                }
                (
                    Special::FirstAid { pt: a, .. } | Special::Water { pt: a, .. },
                    Special::FirstAid { pt: b, .. } | Special::Water { pt: b, .. },
                ) => assert_eq!(shift(*a), *b),
                (
                    Special::Text {
                        pt: a,
                        text: ta,
                        size_mm: sa,
                        ..
                    },
                    Special::Text {
                        pt: b,
                        text: tb,
                        size_mm: sb,
                        ..
                    },
                ) => {
                    assert_eq!(shift(*a), *b);
                    assert_eq!((ta, sa), (tb, sb));
                }
                (a, b) => panic!("the kind changed: {a:?} became {b:?}"),
            }
        }
    }

    fn event() -> RawEvent {
        CourseEvent {
            controls: vec![
                Control {
                    id: Some(4),
                    code: "31".into(),
                    kind: ControlKind::Control,
                    position: Some(p(10.0, 20.0)),
                    geo: Some((48.0, 11.0)),
                    description: None,
                },
                Control {
                    id: None,
                    code: "32".into(),
                    kind: ControlKind::Control,
                    position: None,
                    geo: None,
                    description: None,
                },
            ],
            courses: vec![Course {
                id: Some(1),
                name: "A".into(),
                family: Some("F".into()),
                kind: CourseKind::Normal,
                legs: vec![Leg {
                    bends: vec![p(1.0, 2.0), p(3.0, 4.0)],
                    gaps: vec![(0.5, 1.5)],
                    marked_route: true,
                }],
                stops: vec![
                    Stop {
                        control: "31".into(),
                        kind: None,
                        number: None,
                        number_text_at: Some(Offset {
                            dx_mm: 1.0,
                            dy_mm: 2.0,
                        }),
                        circle_gaps: vec![(10.0, 20.0)],
                        map_flip: true,
                    },
                    Stop {
                        control: "32".into(),
                        kind: Some(ControlKind::Finish),
                        number: None,
                        number_text_at: None,
                        circle_gaps: Vec::new(),
                        map_flip: false,
                    },
                ],
                climb_m: Some(50.0),
                length_m: Some(2500.0),
                print_scale_hint: Some(10_000),
                course_obj_ratio: Some(1.5),
                secondary_title: Some("Second".into()),
            }],
            specials: every_special(),
            source: CourseSource {
                format: SourceFormat::PurplePen,
                event_name: Some("Event".into()),
                event_date: None,
                organiser: None,
                map_scale: Some(10_000),
                item_scaling: Some(ItemScaling::RelativeToMap),
            },
            notes: vec![CourseNote::NoDescriptions],
        }
    }

    #[test]
    fn mapping_an_event_converts_points_and_offsets_and_keeps_the_rest() {
        let ev = event();
        let mapped = ev.clone().map_points(
            |q| (q.x_mm * 2.0, q.y_mm * 2.0),
            |o| Offset {
                dx_mm: o.dx_mm + 10.0,
                dy_mm: -o.dy_mm,
            },
        );
        // Controls: the position, and nothing else, goes through the point function.
        assert_eq!(mapped.controls[0].position, Some((20.0, 40.0)));
        assert_eq!(mapped.controls[1].position, None);
        assert_eq!(mapped.controls[0].geo, Some((48.0, 11.0)));
        assert_eq!(mapped.controls[0].id, Some(4));
        assert_eq!(mapped.controls[0].code, "31");
        // Legs: the bends move, gaps and marking stay.
        let leg = &mapped.courses[0].legs[0];
        assert_eq!(leg.bends, [(2.0, 4.0), (6.0, 8.0)]);
        assert_eq!(leg.gaps, [(0.5, 1.5)]);
        assert!(leg.marked_route);
        // Stops: the number offset goes through the offset function, circle gaps and the flip stay.
        let stop = &mapped.courses[0].stops[0];
        assert_eq!(
            stop.number_text_at,
            Some(Offset {
                dx_mm: 11.0,
                dy_mm: -2.0
            })
        );
        assert_eq!(stop.circle_gaps, [(10.0, 20.0)]);
        assert!(stop.map_flip);
        assert_eq!(mapped.courses[0].stops[1].number_text_at, None);
        assert_eq!(mapped.courses[0].stops[1].kind, Some(ControlKind::Finish));
        // Course data.
        let c = &mapped.courses[0];
        let o = &ev.courses[0];
        assert_eq!(
            (&c.id, &c.name, &c.family, &c.kind, &c.climb_m, &c.length_m),
            (&o.id, &o.name, &o.family, &o.kind, &o.climb_m, &o.length_m)
        );
        assert_eq!(
            (&c.print_scale_hint, &c.course_obj_ratio, &c.secondary_title),
            (&o.print_scale_hint, &o.course_obj_ratio, &o.secondary_title)
        );
        // Specials, source and notes.
        assert_eq!(mapped.specials.len(), 10);
        assert!(matches!(
            &mapped.specials[5],
            Special::FirstAid { pt, .. } if *pt == (14.0, 16.0)
        ));
        assert_eq!(mapped.source, ev.source);
        assert_eq!(mapped.notes, ev.notes);
    }
}
